//! Runner checks for board row E12i (last 21 `not started` `save` functions):
//! `Profile_NumberCharacterRecords` 0x166c70, `MCOverlay_EndLoad` 0x1ce700, `MCOverlay_EndSave` 0x1ce880,
//! `MCOverlay_DispatchByCount` 0x1cf648, `MCOverlay_DispatchByMode` 0x1cf698, `MCOverlay_DrawState` 0x1cf6e8,
//! `MCOverlay_OpenSave` 0x1d1dd8, `MCOverlay_BeginSave` 0x1d1e30, `MCOverlay_ResetCard` 0x1d1e88,
//! `cMCOverlayManager::vf17_1d22c8` 0x1d22c8, `vf53/52/49/26/23/16/15/13/2` 0x1d3a48..0x1d3b00,
//! `cSaveOverlay::vf3_1d3af8` 0x1d3af8, `StaticInit_MemCard` 0x23d5f8.
//! (TRICKY_ELF; skipped without it).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::copies::RAND_SEED;
use tricky_game::fe3dstate::FeEnv;
use tricky_game::memcard::{
    mc_overlay_begin_save, mc_overlay_dispatch_by_count, mc_overlay_dispatch_by_mode, mc_overlay_draw_state,
    mc_overlay_end_load, mc_overlay_end_save, mc_overlay_noop, mc_overlay_open_save, mc_overlay_reset_card,
    mc_overlay_vf17, mc_overlay_zero, profile_number_character_records, static_init_memcard, MC_SAVE_FLAG,
    MEMCARD_RAND_A, MEMCARD_RAND_B, MEMCARD_UI_SET_MESSAGE,
};
use tricky_game::pf::Mem;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

type Log = Rc<RefCell<Vec<(u32, Vec<u32>)>>>;

struct RecFe(Log);
impl FeEnv for RecFe {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.0.borrow_mut().push((func, args.to_vec()));
        0
    }
}

#[test]
fn profile_and_static_init_memcard_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.allow(0x2401a0); // Rand_Init

    let buf = r.mem.alloc(0x1a00);
    let mut g = Rng(0x166c_70aa_23d5_0001);

    // Profile_NumberCharacterRecords 0x166c70
    for case in 0..200u32 {
        let mut init = vec![0u8; 0x1a00];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(buf, &init).unwrap();
        let mut m = Mem::default();
        m.regions.push((buf, init));

        r.cpu.set_gpr(4, buf as u64);
        r.call(0x166c70).unwrap_or_else(|e| panic!("Profile_NumberCharacterRecords {case}: {e}"));
        let mut theirs = vec![0u8; 0x1a00];
        r.mem.read(buf, &mut theirs).unwrap();

        profile_number_character_records(&mut m, buf);
        assert_eq!(&theirs, &m.regions[0].1, "case {case}: profile memory mismatch");
    }

    // StaticInit_MemCard 0x23d5f8
    let mut seed = vec![0u8; 0x18];
    r.mem.read(RAND_SEED, &mut seed).unwrap();
    for case in 0..50u32 {
        let mut init = vec![0u8; 0x30];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(MEMCARD_RAND_A, &init).unwrap();
        let mut m = Mem::default();
        m.regions.push((MEMCARD_RAND_A, init));
        m.regions.push((RAND_SEED, seed.clone()));

        r.call(0x23d5f8).unwrap_or_else(|e| panic!("StaticInit_MemCard {case}: {e}"));
        let mut theirs = vec![0u8; 0x30];
        r.mem.read(MEMCARD_RAND_A, &mut theirs).unwrap();

        static_init_memcard(&mut m);
        assert_eq!(MEMCARD_RAND_B, MEMCARD_RAND_A + 0x18);
        assert_eq!(&theirs, &m.regions[0].1, "case {case}: StaticInit_MemCard mismatch");
    }
}

#[test]
fn mc_overlay_dispatch_and_stubs_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let base = r.mem.alloc(0x1000);
    let (ovl, card, vt_card, vt_owner) = (base, base + 0x500, base + 0x600, base + 0x800);
    let log: Log = Rc::new(RefCell::new(vec![]));

    // Stub MemCardUI_SetMessage 0x1d2fe8
    {
        let l2 = log.clone();
        r.stub(MEMCARD_UI_SET_MESSAGE, move |c, _| {
            l2.borrow_mut().push((
                MEMCARD_UI_SET_MESSAGE,
                vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32],
            ));
        });
    }

    // Card vtable slots: 0x60, 0xd0, 0xf8 (3 args), 0x100 (3 args), 0x120
    const CARD_BASE: u32 = 0x7e2000;
    for (slot, arity) in [(0x60u32, 1usize), (0xd0, 1), (0xf8, 4), (0x100, 4), (0x120, 1)] {
        let f = CARD_BASE + slot;
        let l2 = log.clone();
        r.stub(f, move |c, _| {
            let v: Vec<u32> = (0..arity).map(|i| c.gpr(4 + i) as u32).collect();
            l2.borrow_mut().push((f, v));
        });
    }

    // Owner vtable slots used across DispatchByCount, DispatchByMode, DrawState, vf17
    const OWNER_BASE: u32 = 0x7e3000;
    let owner_slots: [(u32, usize); 28] = [
        (0x78, 1),
        (0x80, 2),
        (0x88, 1),
        (0x90, 1),
        (0x98, 1),
        (0xa0, 1),
        (0xa8, 1),
        (0xb0, 1),
        (0xb8, 1),
        (0xc0, 1),
        (0xc8, 1),
        (0xd0, 1),
        (0xd8, 1),
        (0xe0, 1),
        (0xe8, 1),
        (0xf0, 1),
        (0xf8, 1),
        (0x100, 1),
        (0x108, 1),
        (0x110, 1),
        (0x118, 1),
        (0x120, 1),
        (0x128, 1),
        (0x130, 1),
        (0x138, 1),
        (0x140, 1),
        (0x148, 1),
        (0x150, 1),
    ];
    for &(slot, arity) in &owner_slots {
        let f = OWNER_BASE + slot;
        let l2 = log.clone();
        r.stub(f, move |c, _| {
            let v: Vec<u32> = (0..arity).map(|i| c.gpr(4 + i) as u32).collect();
            l2.borrow_mut().push((f, v));
        });
    }

    let mut g = Rng(0x1cf6_e8bb_d00d_0002);

    // Test every DrawState value -2..=40 exhaustively + random cases across all other functions
    for case in 0..1_200u32 {
        let mut init = vec![0u8; 0x1000];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(base, &init).unwrap();
        r.mem.write_u32(MC_SAVE_FLAG, g.next());
        r.mem.write_u32(ovl + 0x224, card);
        r.mem.write_u32(ovl + 0x428, vt_owner);
        r.mem.write_u32(card, vt_card);

        for &(slot, _) in &[(0x60u32, 1usize), (0xd0, 1), (0xf8, 4), (0x100, 4), (0x120, 1)] {
            r.mem.write_u16(vt_card + slot, ((g.below(11) as i16) - 5) as u16);
            r.mem.write_u32(vt_card + slot + 4, CARD_BASE + slot);
        }
        for &(slot, _) in &owner_slots {
            r.mem.write_u16(vt_owner + slot, ((g.below(11) as i16) - 5) as u16);
            r.mem.write_u32(vt_owner + slot + 4, OWNER_BASE + slot);
        }

        let state = if case < 45 {
            (case as i32 - 2) as u32
        } else {
            g.below(40)
        };
        let count_val = match g.below(3) {
            0 => 0u32,
            1 => -(g.below(10) as i32 + 1) as u32,
            _ => g.below(10) + 1,
        };
        let mode_val = if g.below(2) == 0 { 0 } else { g.next() | 1 };
        r.mem.write_u32(ovl + 0x54, state);
        r.mem.write_u32(ovl + 0x44, count_val);
        r.mem.write_u32(ovl + 8, mode_val);

        let a1 = g.next();
        let a2 = g.next();
        let a3 = g.next();

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x1000];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut fb = vec![0u8; 4];
        r.mem.read(MC_SAVE_FLAG, &mut fb).unwrap();
        m.regions.push((MC_SAVE_FLAG, fb));

        let which = if case < 45 { 5 } else { case % 9 };
        log.borrow_mut().clear();
        match which {
            0 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.call(0x1ce700).unwrap_or_else(|e| panic!("EndLoad {case}: {e}"));
            }
            1 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.call(0x1ce880).unwrap_or_else(|e| panic!("EndSave {case}: {e}"));
            }
            2 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.call(0x1d1e88).unwrap_or_else(|e| panic!("ResetCard {case}: {e}"));
            }
            3 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.cpu.set_gpr(5, a1 as u64);
                r.cpu.set_gpr(6, a2 as u64);
                r.cpu.set_gpr(7, a3 as u64);
                r.call(0x1d1dd8).unwrap_or_else(|e| panic!("OpenSave {case}: {e}"));
            }
            4 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.cpu.set_gpr(5, a1 as u64);
                r.cpu.set_gpr(6, a2 as u64);
                r.cpu.set_gpr(7, a3 as u64);
                r.call(0x1d1e30).unwrap_or_else(|e| panic!("BeginSave {case}: {e}"));
            }
            5 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.call(0x1cf6e8).unwrap_or_else(|e| panic!("DrawState {case} state {state}: {e}"));
            }
            6 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.call(0x1cf648).unwrap_or_else(|e| panic!("DispatchByCount {case}: {e}"));
            }
            7 => {
                r.cpu.set_gpr(4, ovl as u64);
                r.call(0x1cf698).unwrap_or_else(|e| panic!("DispatchByMode {case}: {e}"));
            }
            _ => {
                r.cpu.set_gpr(4, ovl as u64);
                r.call(0x1d22c8).unwrap_or_else(|e| panic!("vf17_1d22c8 {case}: {e}"));
            }
        }
        let sp = r.cpu.gpr(29) as u32;
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x1000];
        r.mem.read(base, &mut theirs_mem).unwrap();
        let theirs_flag = r.mem.read_u32(MC_SAVE_FLAG);

        let env = &mut RecFe(log.clone());
        match which {
            0 => mc_overlay_end_load(&mut m, env, ovl),
            1 => mc_overlay_end_save(&mut m, env, ovl),
            2 => mc_overlay_reset_card(&mut m, env, ovl),
            3 => mc_overlay_open_save(&mut m, env, ovl, a1, a2, a3),
            4 => mc_overlay_begin_save(&mut m, env, ovl, a1, a2, a3),
            5 => mc_overlay_draw_state(&mut m, env, ovl),
            6 => mc_overlay_dispatch_by_count(&mut m, env, ovl),
            7 => mc_overlay_dispatch_by_mode(&mut m, env, ovl),
            _ => mc_overlay_vf17(&mut m, env, ovl, sp),
        }
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
        assert_eq!(theirs_flag, m.r32(MC_SAVE_FLAG), "case {case} which {which}: save flag mismatch");
    }

    // Check all 10 small virtual stubs (9 no-ops + vf2 returning 0)
    let noops = [
        0x1d3a48u32,
        0x1d3a50,
        0x1d3a58,
        0x1d3a88,
        0x1d3a90,
        0x1d3aa0,
        0x1d3aa8,
        0x1d3ab0,
        0x1d3af8,
    ];
    for &addr in &noops {
        let mut before = vec![0u8; 0x1000];
        r.mem.read(base, &mut before).unwrap();
        r.cpu.set_gpr(4, ovl as u64);
        r.call(addr).unwrap_or_else(|e| panic!("noop {addr:#x}: {e}"));
        mc_overlay_noop();
        let mut after = vec![0u8; 0x1000];
        r.mem.read(base, &mut after).unwrap();
        assert_eq!(before, after, "noop {addr:#x} mutated memory");
    }
    r.cpu.set_gpr(2, 0xdead_beef);
    r.cpu.set_gpr(4, ovl as u64);
    r.call(0x1d3b00).unwrap();
    assert_eq!(r.cpu.gpr(2) as u32, mc_overlay_zero());
}
