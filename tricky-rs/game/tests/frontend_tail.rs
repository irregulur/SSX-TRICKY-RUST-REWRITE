//! Function-runner check for row G9v (the last 42 `not started` `frontend` functions):
//! - `overlaybase`: `cHUDOverlay::~cHUDOverlay` 0x1a6b20, `RaceMenuOverlay_UpdateSave` 0x1ef750,
//!   `RaceMenuOverlay_Draw` 0x1efe38, `RaceMenuOverlay_UpdateConfirm` 0x1f00c8
//! - `nextevent`: 22 `cSelectNextEventOverlay` virtuals / `Update` (`0x1d2320..0x1d2cf0`) and 14 stubs
//!   (`0x1d3a60..0x1d3af0`)
//! - `overlays`: `ScreenAnimTrack_Delete` 0x1f0e60
//! - `fe_wlist`: `ListFE3DOffsetOp_Apply6` 0x2b6978

use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::fe3dstate::FeEnv;
use tricky_game::overlays::OverlayManEnv;
use tricky_game::pf::Mem;
use tricky_game::ps2f::F;
use tricky_game::textbox::TextBoxEnv;
use tricky_game::{fe_wlist, nextevent, overlaybase, overlays};

const BASE: u32 = 0x0140_0000;
const LOC_GET_STRING: u32 = 0x2c5160;
const APP: u32 = 0x337c58;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
}

fn runner() -> Option<Runner> {
    let p = std::env::var("TRICKY_ELF").ok()?;
    let bytes = std::fs::read(&p).expect("read ELF");
    Some(Runner::from_elf(&bytes).expect("load ELF"))
}

fn call_fn(r: &mut Runner, addr: u32, args: &[u32]) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn fresh_mem() -> Mem {
    Mem {
        regions: vec![
            (0x0032_0000, vec![0u8; 0x000a_0000]),
            (BASE, vec![0u8; 0x20000]),
            (0x01fe_0000, vec![0u8; 0x10000]),
        ],
    }
}

#[derive(Default)]
struct TbLog {
    calls: Vec<(u32, Vec<u32>, Vec<u32>)>,
    mc_done: u32,
    depth_ret: u32,
    dev_match_idx: i32,
    poll_ret: u32,
    pad_state_val: u32,
}
impl TextBoxEnv for TbLog {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32], floats: &[F]) -> u32 {
        self.calls.push((func, args.to_vec(), floats.iter().map(|x| x.0.to_bits()).collect()));
        match func {
            LOC_GET_STRING => 0x0190_0000 | (args[1] << 4),
            overlaybase::MEMCARD_OP_IS_DONE => self.mc_done,
            overlaybase::INPUT_GET_DEVICE => {
                if args[1] as i32 == self.dev_match_idx {
                    self.pad_state_val
                } else {
                    self.pad_state_val.wrapping_add(0x100 + args[1])
                }
            }
            overlaybase::DIALOG_POLL_BUTTONS => self.poll_ret,
            0x001a_0010 => self.depth_ret, // renderer +0x1dc stub target
            _ => 0,
        }
    }
}

#[test]
fn select_next_event_virtuals_and_stubs() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x1d23_2001);
    let app = BASE;
    let this = BASE + 0x1000;
    let vt = BASE + 0x2000;

    // Distinct valid code-range stub targets for the 3 virtual slots (+0x74, +0x84, +0xdc)
    const VF_DRAW_TEXT: u32 = 0x001b_9530; // cSelectNextEventOverlay::DrawText
    const VF_RESET: u32 = 0x001b_9968;     // cSelectNextEventOverlay::Reset
    const VF_POST: u32 = 0x001b_8bb8;      // cSelectNextEventOverlay::vf26

    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>, Vec<u32>)>::new()));
    for &(addr, nargs) in &[
        (nextevent::MEMCARD_SET_MESSAGE, 3usize),
        (VF_DRAW_TEXT, 5),
        (VF_RESET, 2),
        (VF_POST, 1),
    ] {
        let log = log.clone();
        r.stub(addr, move |c, _| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            log.borrow_mut().push((addr, args, vec![]));
            c.set_gpr(2, 0);
        });
    }
    {
        let log = log.clone();
        r.stub(LOC_GET_STRING, move |c, _| {
            let a0 = c.gpr(4) as u32;
            let a1 = c.gpr(5) as u32;
            log.borrow_mut().push((LOC_GET_STRING, vec![a0, a1], vec![]));
            c.set_gpr(2, (0x0190_0000 | (a1 << 4)) as u64);
        });
    }

    let setup = |r: &mut Runner, rng: &mut Rng| -> Mem {
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        w(&mut m, r, APP, app);
        w(&mut m, r, app + 0x50, 0x0150_0000 | (rng.next() & 0xff0));
        w(&mut m, r, this + 0x428, vt);
        for &(off, target) in &[(0x70u32, VF_DRAW_TEXT), (0x80, VF_RESET), (0xd8, VF_POST)] {
            let adj = ((rng.next() % 0x80) as i32 - 0x40) as i16 as u16;
            m.w16(vt + off, adj);
            r.mem.write(vt + off, &adj.to_le_bytes()).unwrap();
            w(&mut m, r, vt + off + 4, target);
        }
        m
    };

    // Pattern A: 3 functions
    for &(addr, str_id) in &[(0x1d2320u32, 0x1bfu32), (0x1d24f8, 0x173), (0x1d2be8, 0x158)] {
        for case in 0..40 {
            log.borrow_mut().clear();
            let mut m = setup(&mut r, &mut rng);
            call_fn(&mut r, addr, &[this]);
            let mut env = TbLog::default();
            nextevent::sne_prompt_loc(&mut m, &mut env, this, str_id);
            assert_eq!(env.calls, *log.borrow(), "pattern A {addr:#x} case {case}");
        }
    }

    // Pattern B: 5 functions
    for &addr in &[0x1d23b0u32, 0x1d2428, 0x1d2588, 0x1d2a28, 0x1d2c78] {
        for case in 0..40 {
            log.borrow_mut().clear();
            let mut m = setup(&mut r, &mut rng);
            call_fn(&mut r, addr, &[this]);
            let sp = r.cpu.gpr(29) as u32;
            let mut env = TbLog::default();
            nextevent::sne_prompt_msg_ok(&mut m, &mut env, this, sp);
            assert_eq!(env.calls, *log.borrow(), "pattern B {addr:#x} case {case}");
        }
    }

    // Pattern C & D: 4 + 3 = 7 functions
    for &(addr, with_post) in &[
        (0x1d24a0u32, true),
        (0x1d29d0, true),
        (0x1d2b38, true),
        (0x1d2b90, true),
        (0x1d2600, false),
        (0x1d2648, false),
        (0x1d2690, false),
    ] {
        for case in 0..40 {
            log.borrow_mut().clear();
            let mut m = setup(&mut r, &mut rng);
            call_fn(&mut r, addr, &[this]);
            let sp = r.cpu.gpr(29) as u32;
            let mut env = TbLog::default();
            nextevent::sne_notice_msg(&mut m, &mut env, this, sp, with_post);
            assert_eq!(env.calls, *log.borrow(), "pattern CD {addr:#x} case {case}");
        }
    }

    // Pattern E: 6 functions
    for &(addr, s1, s2, sw) in &[
        (0x1d26d8u32, 0x1d6u32, 0x188u32, 1u32),
        (0x1d2770, 0x1ba, 0x154, 1),
        (0x1d2808, 0x1ba, 0x154, 1),
        (0x1d28a0, 0x1ba, 0x154, 1),
        (0x1d2938, 0x162, 0x154, 0),
        (0x1d2aa0, 0x1d6, 0x188, 1),
    ] {
        for case in 0..40 {
            log.borrow_mut().clear();
            let mut m = setup(&mut r, &mut rng);
            call_fn(&mut r, addr, &[this]);
            let sp = r.cpu.gpr(29) as u32;
            let mut env = TbLog::default();
            nextevent::sne_prompt_msg_two(&mut m, &mut env, this, sp, s1, s2, sw);
            assert_eq!(env.calls, *log.borrow(), "pattern E {addr:#x} case {case}");
        }
    }

    // cSelectNextEventOverlay::Update 0x1d2cf0
    for case in 0..40 {
        log.borrow_mut().clear();
        let mut m = setup(&mut r, &mut rng);
        call_fn(&mut r, 0x1d2cf0, &[this]);
        let mut env = TbLog::default();
        nextevent::sne_update(&mut m, &mut env, this);
        assert_eq!(env.calls, *log.borrow(), "sne update case {case}");
    }

    // 11 no-op stubs + 3 return-0 stubs
    for &addr in &[
        0x1d3a60u32, 0x1d3a68, 0x1d3a70, 0x1d3a78, 0x1d3a80, 0x1d3a98, 0x1d3ab8, 0x1d3ad0, 0x1d3ad8, 0x1d3ae0,
        0x1d3ae8,
    ] {
        call_fn(&mut r, addr, &[this]);
        nextevent::sne_noop();
    }
    for &addr in &[0x1d3ac0u32, 0x1d3ac8, 0x1d3af0] {
        r.cpu.set_gpr(2, 0xdead_beef);
        let rv_o = call_fn(&mut r, addr, &[this]);
        assert_eq!(nextevent::sne_zero(), rv_o, "zero stub {addr:#x}");
    }
}

#[test]
fn hud_overlay_and_race_menu_overlay() {
    let Some(mut r) = runner() else { return };
    r.allow(0x1f00c8); // RaceMenuOverlay_UpdateConfirm (tail-called from 0x1ef750 case 4)
    let mut rng = Rng(0x1ef7_5002);
    let app = BASE;
    let rend = BASE + 0x1000;
    let rvt = BASE + 0x1400;
    let inp_mgr = BASE + 0x1800;
    let this = BASE + 0x2000;
    const REND_GET_DEPTH: u32 = 0x001a_0010;

    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>, Vec<u32>)>::new()));
    let mc_done = Rc::new(RefCell::new(0u32));
    let depth_ret = Rc::new(RefCell::new(0u32));
    let dev_match_idx = Rc::new(RefCell::new(-1i32));
    let poll_ret = Rc::new(RefCell::new(0u32));
    let pad_state_val = Rc::new(RefCell::new(0u32));

    for &(addr, nargs) in &[
        (overlaybase::SCREEN_OVERLAY_DTOR, 2usize),
        (overlaybase::MEMCARD_OP_UPDATE, 1),
        (overlaybase::RACE_MENU_SET_STATE, 2),
        (overlaybase::RACE_MENU_UPDATE_REPLAY, 1),
        (overlaybase::RACE_MENU_DRAW_REPLAY, 1),
        (overlaybase::MCOVERLAY_DRAW, 1),
        (overlaybase::DIALOG_DRAW, 1),
    ] {
        let log = log.clone();
        r.stub(addr, move |c, _| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            log.borrow_mut().push((addr, args, vec![]));
            c.set_gpr(2, 0);
        });
    }
    {
        let log = log.clone();
        r.stub(overlaybase::BACKDROP_DRAW_FRAME, move |c, _| {
            let args = vec![c.gpr(4) as u32, c.gpr(5) as u32];
            let f12 = c.fpr[12];
            log.borrow_mut().push((overlaybase::BACKDROP_DRAW_FRAME, args, vec![f12]));
            c.set_gpr(2, 0);
        });
    }
    {
        let log = log.clone();
        let d = mc_done.clone();
        r.stub(overlaybase::MEMCARD_OP_IS_DONE, move |c, _| {
            log.borrow_mut().push((overlaybase::MEMCARD_OP_IS_DONE, vec![c.gpr(4) as u32], vec![]));
            c.set_gpr(2, *d.borrow() as u64);
        });
    }
    {
        let log = log.clone();
        let dr = depth_ret.clone();
        r.stub(REND_GET_DEPTH, move |c, _| {
            log.borrow_mut().push((REND_GET_DEPTH, vec![c.gpr(4) as u32], vec![]));
            c.set_gpr(2, *dr.borrow() as u64);
        });
    }
    {
        let log = log.clone();
        let dm = dev_match_idx.clone();
        let ps = pad_state_val.clone();
        r.stub(overlaybase::INPUT_GET_DEVICE, move |c, _| {
            let a0 = c.gpr(4) as u32;
            let a1 = c.gpr(5) as u32;
            log.borrow_mut().push((overlaybase::INPUT_GET_DEVICE, vec![a0, a1], vec![]));
            let v = if a1 as i32 == *dm.borrow() {
                *ps.borrow()
            } else {
                ps.borrow().wrapping_add(0x100 + a1)
            };
            c.set_gpr(2, v as u64);
        });
    }
    {
        let log = log.clone();
        let pr = poll_ret.clone();
        r.stub(overlaybase::DIALOG_POLL_BUTTONS, move |c, _| {
            log.borrow_mut().push((overlaybase::DIALOG_POLL_BUTTONS, vec![c.gpr(4) as u32, c.gpr(5) as u32], vec![]));
            c.set_gpr(2, *pr.borrow() as u64);
        });
    }

    // 1. cHUDOverlay::~cHUDOverlay 0x1a6b20
    for case in 0..100 {
        log.borrow_mut().clear();
        let mut m = fresh_mem();
        let init_vt = rng.next();
        m.w32(this + 0x28, init_vt);
        r.mem.write_u32(this + 0x28, init_vt);
        let how = (case % 4) as u32;
        call_fn(&mut r, 0x1a6b20, &[this, how]);
        let mut env = TbLog::default();
        overlaybase::hud_overlay_destruct(&mut m, &mut env, this, how);
        assert_eq!(env.calls, *log.borrow(), "hud dtor calls case {case}");
        assert_eq!(m.r32(this + 0x28), r.mem.read_u32(this + 0x28), "hud dtor vt case {case}");
    }

    // 2. RaceMenuOverlay_UpdateSave 0x1ef750, RaceMenuOverlay_Draw 0x1efe38, RaceMenuOverlay_UpdateConfirm 0x1f00c8
    for case in 0..600 {
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        w(&mut m, &mut r, APP, app);
        w(&mut m, &mut r, app + 0x724, rend);
        w(&mut m, &mut r, rend + 0x10, rvt);
        let adj = ((rng.next() % 0x80) as i32 - 0x40) as i16 as u16;
        m.w16(rvt + 0x1d8, adj);
        r.mem.write(rvt + 0x1d8, &adj.to_le_bytes()).unwrap();
        w(&mut m, &mut r, rvt + 0x1dc, REND_GET_DEPTH);

        w(&mut m, &mut r, app + 0x24, inp_mgr);
        let n_devs = ((case % 6) as i32 - 1) as u32; // -1..=4
        w(&mut m, &mut r, inp_mgr + 0x14, n_devs);
        let ps_val = 0x0188_0000 | (rng.next() & 0xff0);
        w(&mut m, &mut r, overlaybase::PAD_STATE, ps_val);
        w(&mut m, &mut r, this + 0x42c, 0x0177_0000 | (rng.next() & 0xff0));
        let state = (case % 8) as u32; // 0..=7 (covers all 6 JT entries + out-of-range)
        w(&mut m, &mut r, this + 0x438, state);

        let done_v = (rng.next() & 1) as u32;
        let depth_v = rng.next();
        let match_i = (rng.next() % 5) as i32 - 1; // -1..=3
        let poll_v = rng.next() % 4;
        *mc_done.borrow_mut() = done_v;
        *depth_ret.borrow_mut() = depth_v;
        *dev_match_idx.borrow_mut() = match_i;
        *poll_ret.borrow_mut() = poll_v;
        *pad_state_val.borrow_mut() = ps_val;

        let mk_env = || TbLog {
            calls: vec![],
            mc_done: done_v,
            depth_ret: depth_v,
            dev_match_idx: match_i,
            poll_ret: poll_v,
            pad_state_val: ps_val,
        };

        log.borrow_mut().clear();
        call_fn(&mut r, 0x1ef750, &[this]);
        let mut env = mk_env();
        overlaybase::race_menu_update_save(&mut m, &mut env, this);
        assert_eq!(env.calls, *log.borrow(), "update_save case {case} state {state}");

        log.borrow_mut().clear();
        call_fn(&mut r, 0x1efe38, &[this]);
        let mut env = mk_env();
        overlaybase::race_menu_draw(&mut m, &mut env, this);
        assert_eq!(env.calls, *log.borrow(), "draw case {case} state {state}");

        log.borrow_mut().clear();
        call_fn(&mut r, 0x1f00c8, &[this]);
        let mut env = mk_env();
        overlaybase::race_menu_update_confirm(&mut m, &mut env, this);
        assert_eq!(env.calls, *log.borrow(), "update_confirm case {case}");
    }
}

#[test]
fn screen_anim_track_delete_and_list_offset_apply6() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x2b69_7803);
    let op = BASE;
    let item = BASE + 0x100;
    let vt = BASE + 0x200;
    const ITEM_VF70: u32 = 0x002b_6720; // cFE3DWList::vf13

    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>)>::new()));
    for &(addr, nargs) in &[(0x114f80u32, 1usize), (ITEM_VF70, 2)] {
        let log = log.clone();
        r.stub(addr, move |c, _| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            log.borrow_mut().push((addr, args));
            c.set_gpr(2, 0);
        });
    }

    #[derive(Default)]
    struct SimpleLog(Vec<(u32, Vec<u32>)>);
    impl OverlayManEnv for SimpleLog {
        fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
            self.0.push((func, args.to_vec()));
            0
        }
        fn virtual_call(&mut self, _: &mut Mem, _: u32, _: u32, _: &[u32], _: &[f32]) -> u32 { 0 }
        fn alloc(&mut self, _: &mut Mem, _: u32, _: u32) -> u32 { 0 }
        fn device(&mut self, _: u32) -> u32 { 0 }
        fn is_player_pad(&mut self, _: u32) -> bool { false }
        fn device_pressed(&mut self, _: u32) -> bool { false }
    }
    impl FeEnv for SimpleLog {
        fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
            self.0.push((func, args.to_vec()));
            0
        }
    }

    // 1. ScreenAnimTrack_Delete 0x1f0e60
    for case in 0..100 {
        log.borrow_mut().clear();
        let mut m = fresh_mem();
        let trk = BASE + (rng.next() & 0xff0);
        let how = (case % 4) as u32;
        call_fn(&mut r, 0x1f0e60, &[trk, how]);
        let mut env = SimpleLog::default();
        overlays::screen_anim_track_delete(&mut m, &mut env, trk, how);
        assert_eq!(env.0, *log.borrow(), "track delete case {case}");
    }

    // 2. ListFE3DOffsetOp_Apply6 0x2b6978
    for case in 0..200 {
        log.borrow_mut().clear();
        let mut m = fresh_mem();
        m.w32(item + 0xc, vt);
        r.mem.write_u32(item + 0xc, vt);
        let adj = ((rng.next() % 0x80) as i32 - 0x40) as i16 as u16;
        m.w16(vt + 0x70, adj);
        r.mem.write(vt + 0x70, &adj.to_le_bytes()).unwrap();
        m.w32(vt + 0x74, ITEM_VF70);
        r.mem.write_u32(vt + 0x74, ITEM_VF70);
        r.cpu.set_gpr(2, 0xdead_beef);
        let rv_o = call_fn(&mut r, 0x2b6978, &[op, item]);
        let mut env = SimpleLog::default();
        let rv_p = fe_wlist::list_offset_op_apply6(&mut m, &mut env, op, item);
        assert_eq!(rv_p, rv_o, "apply6 rv case {case}");
        assert_eq!(env.0, *log.borrow(), "apply6 calls case {case}");
    }
}
