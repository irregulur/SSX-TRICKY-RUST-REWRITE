//! Runner checks for board row E12j (the `save` functions left unchecked): the memory card overlay's constructor,
//! destructor and small calls, `SaveOverlay_Init`, `MemCard_Create`, the card op's create / destroy / start / more
//! calls, the slot cursor virtuals, the tags, `MCSlot_GetLabel` and `MemCardUI_SetMessage`, each against the
//! original in the function runner (TRICKY_ELF; skipped without it). Every call out (the card system's and the
//! overlay's virtuals, the strings, `EAMem_*`) is a stub on both sides that logs its arguments and answers from the
//! same hash of the case's seed, the callee and the call's place in the log; the calls, the answers kept and the
//! memory are compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::fe3dstate::FeEnv;
use tricky_game::memcard::*;
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

/// The answer of a call out: a third of them 0 (the branches on "none"), the rest a hash.
fn answer(seed: u64, func: u32, n: usize) -> u32 {
    let mut h = seed ^ (func as u64) << 32 ^ n as u64;
    h = (h ^ h >> 30).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ h >> 27).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    if h % 3 == 0 {
        0
    } else {
        (h >> 8) as u32
    }
}

type Log = Rc<RefCell<(u64, Vec<(u32, Vec<u32>)>)>>;

struct RecFe(Log);
impl FeEnv for RecFe {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut l = self.0.borrow_mut();
        let v = answer(l.0, func, l.1.len());
        l.1.push((func, args.to_vec()));
        v
    }
}

/// The vtable stubs: 0x40 entries each, at these addresses (+ the entry's offset).
const CARD_FN: u32 = 0x7e2000;
const OWNER_FN: u32 = 0x7e3000;
const OUTS: [u32; 11] = [
    EAMEM_SET,
    EAMEM_ALLOC,
    MEMCARD_CTOR,
    OBJECT_DELETE,
    MEM_DELETE,
    BACKDROP_OVERLAY_INIT,
    LOC_STRING,
    WSTR_COPY,
    WSTR_FORMAT,
    WSTR_FORMAT_ASCII,
    WSTR_COUNT,
];

const SIZE: u32 = 0x2000;

#[test]
fn save_ops_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    // run natively: the mode setter (in the constructor), the constructor (in `SaveOverlay_Init`), the state's draw
    // (in `MCOverlay_Draw`) and the slot count (in vf50 / vf51); each is also checked on its own below
    for a in [0x1cee28u32, 0x1ce490, 0x1cf6e8, 0x1d1eb8] {
        r.allow(a);
    }
    let log: Log = Rc::new(RefCell::new((0, vec![])));
    let stubs: Vec<u32> = OUTS.iter().copied().chain((0..0x40).flat_map(|k| [CARD_FN + k * 8, OWNER_FN + k * 8])).collect();
    for f in stubs {
        let l = log.clone();
        r.stub(f, move |c, _| {
            let mut l = l.borrow_mut();
            let v = answer(l.0, f, l.1.len());
            l.1.push((f, [4, 5, 6, 7, 8].iter().map(|&k| c.gpr(k) as u32).collect()));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let base = r.mem.alloc(SIZE);
    let (o, card, vt_card, vt_owner, app, out) = (base, base + 0x800, base + 0xb00, base + 0xd00, base + 0xf00, base + 0x1000);
    // sp on entry (`Runner::call` starts every call on the same stack)
    r.cpu.set_gpr(4, o as u64);
    r.call(0x1d21c0).unwrap();
    let sp = r.cpu.gpr(29) as u32;
    let frame = sp - 0xd0;
    let mut statics = vec![];
    for (a, n) in [(SLOT_LABEL_FMT, 0x40u32), (SAVE_OVERLAY_BASE_VT, 0x98)] {
        let mut b = vec![0u8; n as usize];
        r.mem.read(a, &mut b).unwrap();
        statics.push((a, b));
    }
    let app_word = r.mem.read_u32(APP);

    let mut g = Rng(0x0e12_6a11_1ce4_9001);
    let mut seen = [0u32; 27];
    for case in 0..40_000u32 {
        let which = if case < 27 * 50 { case % 27 } else { g.below(27) };
        seen[which as usize] += 1;
        let mut img = vec![0u8; SIZE as usize];
        for b in img.iter_mut() {
            *b = g.next() as u8;
        }
        r.mem.write(base, &img).unwrap();
        let w = |r: &mut Runner, a: u32, v: u32| r.mem.write_u32(a, v);
        w(&mut r, o + 0x224, card);
        w(&mut r, o + 0x428, vt_owner);
        w(&mut r, card, vt_card);
        for k in 0..0x40u32 {
            r.mem.write_u16(vt_card + k * 8, (g.below(11) as u16).wrapping_sub(5));
            w(&mut r, vt_card + k * 8 + 4, CARD_FN + k * 8);
            r.mem.write_u16(vt_owner + k * 8, (g.below(11) as u16).wrapping_sub(5));
            w(&mut r, vt_owner + k * 8 + 4, OWNER_FN + k * 8);
        }
        w(&mut r, app + 0x50, g.next());
        let small = |g: &mut Rng| match g.below(4) {
            0 => 0,
            1 => g.below(6),
            2 => (g.below(6) as i32 - 3) as u32,
            _ => g.next(),
        };
        w(&mut r, o + 4, [0, 1, 2, g.next()][g.below(4) as usize]);
        w(&mut r, o + 0x54, g.below(40));
        w(&mut r, o + 0x3c, g.below(2));
        for off in [0x2c, 0x38, 0x218, 0x21c] {
            let v = small(&mut g);
            w(&mut r, o + off, v);
        }
        // the save overlay's virtual base: its own (+0x438) or somewhere in the block
        let vbase = if g.below(2) == 0 { o + 0x438 } else { base + 0x500 + 8 * g.below(0x40) };
        w(&mut r, o + 0x42c, vbase);
        // the card slots' records (`MCSlot_GetLabel`): each flag 0 half the time
        let slot = g.below(5);
        for k in 0..5u32 {
            for off in [0x168, 0x16c, 0x170, 0x174] {
                let v = if g.below(2) == 0 { 0 } else { g.next() };
                w(&mut r, card + k * 0x58 + off, v);
            }
        }
        r.mem.write_u32(MC_SAVE_FLAG, g.next());
        r.mem.write_u32(APP, app);
        let (a1, a2, a3) = (g.next(), g.next(), g.next());
        let a1 = match which {
            1 | 24 => [0, 1, 2, a1][g.below(4) as usize],
            2 => [0, a1][g.below(2) as usize],
            _ => a1,
        };
        let seed = g.next() as u64 | (g.next() as u64) << 32;

        let mut m = Mem::default();
        let mut snap = vec![0u8; SIZE as usize];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        m.regions.push((MC_SAVE_FLAG, r.mem.read_u32(MC_SAVE_FLAG).to_le_bytes().to_vec()));
        m.regions.push((APP, app.to_le_bytes().to_vec()));
        let mut fr = vec![0u8; 0x98];
        r.mem.read(frame, &mut fr).unwrap();
        m.regions.push((frame, fr));
        m.regions.extend(statics.iter().cloned());

        *log.borrow_mut() = (seed, vec![]);
        let (addr, args): (u32, Vec<u32>) = match which {
            0 => (0x1ce490, vec![o]),
            1 => (0x1ce580, vec![o, a1]),
            2 => (0x1b8568, vec![o, a1]),
            3 => (0x23d6e0, vec![]),
            4 => (0x1cee68, vec![o]),
            5 => (0x1ceeb0, vec![o]),
            6 => (0x1ce730, vec![o, a1]),
            7 => (0x1ce8b0, vec![o, a1]),
            8 => (0x1ce6b8, vec![o, a1, a2, a3]),
            9 => (0x1d1d80, vec![o, a1, a2, a3]),
            10 => (0x1d1f18, vec![o, a1, a2]),
            11 => (0x1d1fd0, vec![o, a1, a2]),
            12 => (0x1cf628, vec![o]),
            13 => (0x1d1eb8, vec![o]),
            14 => (0x1d21a0, vec![o]),
            15 => (0x1d21b0, vec![o]),
            16 => (0x1d21c0, vec![o]),
            17 => (0x1d21c8, vec![o]),
            18 => (0x1d2200, vec![o]),
            19 => (0x1d2278, vec![o]),
            20 => (0x1d2230, vec![o]),
            21 => (0x1d2f60, vec![out + g.below(8)]),
            22 => (0x1d2fa0, vec![out + g.below(8)]),
            23 => (0x1d2fc8, vec![out + g.below(8)]),
            24 => (0x1cee28, vec![o, a1]),
            25 => (0x1d2d50, vec![card, slot, out]),
            _ => (0x1d2fe8, vec![o, out, a2 & 0xff]),
        };
        for k in 4..8 {
            r.cpu.set_gpr(k, args.get(k - 4).copied().unwrap_or(0xbad0_0000 + k as u32) as i32 as i64 as u64);
        }
        r.cpu.set_gpr(2, 0xdead_beef);
        r.call(addr).unwrap_or_else(|e| panic!("case {case} {addr:#x}: {e}"));
        let theirs_ret = r.cpu.gpr(2) as u32;
        let theirs_log = std::mem::take(&mut log.borrow_mut().1);
        let mut theirs = vec![0u8; SIZE as usize];
        r.mem.read(base, &mut theirs).unwrap();
        let theirs_flag = r.mem.read_u32(MC_SAVE_FLAG);
        let mut theirs_frame = vec![0u8; 0x98];
        r.mem.read(frame, &mut theirs_frame).unwrap();

        let e = &mut RecFe(log.clone());
        let a = &args;
        let mine_ret: Option<u32> = match which {
            0 => Some(mc_overlay_manager_new(&mut m, e, o)),
            1 => {
                mc_overlay_manager_drop(&mut m, e, o, a[1]);
                None
            }
            2 => Some(save_overlay_init(&mut m, e, o, a[1], sp)),
            3 => Some(memcard_create(&mut m, e)),
            4 => {
                memcard_op_create(&mut m, e, o);
                None
            }
            5 => {
                memcard_op_destroy(&mut m, e, o);
                None
            }
            6 | 7 => {
                mc_overlay_set_save_bytes(&mut m, e, o, a[1]);
                None
            }
            8 => {
                mc_overlay_begin_load(&mut m, e, o, a[1], a[2], a[3]);
                None
            }
            9 => {
                memcard_op_start_write(&mut m, e, o, a[1], a[2], a[3]);
                None
            }
            10 => {
                mc_overlay_save_more(&mut m, e, o, a[1], a[2]);
                None
            }
            11 => {
                mc_overlay_load_more(&mut m, e, o, a[1], a[2]);
                None
            }
            12 => {
                mc_overlay_draw(&mut m, e, o);
                None
            }
            13 => Some(memcard_ui_get_slot_count(&mut m, e, o)),
            14 => Some(memcard_op_is_done(&m, o)),
            15 => Some(memcard_op_is_failed(&m, o)),
            16 => Some(memcard_op_get_result(&m, o)),
            17 => {
                mc_overlay_vf47(&mut m, o);
                None
            }
            18 => {
                mc_overlay_vf48(&mut m, o);
                None
            }
            19 => {
                mc_overlay_vf50(&mut m, e, o);
                None
            }
            20 => {
                mc_overlay_vf51(&mut m, e, o);
                None
            }
            21 => {
                save_get_game_code(&mut m, a[0]);
                None
            }
            22 => {
                memcard_write_tag_rep(&mut m, a[0]);
                None
            }
            23 => {
                memcard_write_tag_set(&mut m, a[0]);
                None
            }
            24 => {
                mc_overlay_set_mode(&mut m, o, a[1]);
                None
            }
            25 => {
                mc_slot_get_label(&mut m, e, card, slot, out);
                None
            }
            _ => {
                memcard_ui_set_message(&mut m, e, o, out, a[2]);
                None
            }
        };
        let mine_log = std::mem::take(&mut log.borrow_mut().1);
        let ctx = format!("case {case} which {which} ({addr:#x})");
        assert_eq!(theirs_log.len(), mine_log.len(), "{ctx}: calls out {theirs_log:x?} vs {mine_log:x?}");
        for (i, (t, p)) in theirs_log.iter().zip(&mine_log).enumerate() {
            assert_eq!(t.0, p.0, "{ctx}: call {i}");
            // the card's virtuals that take arguments: `this` and how many after it
            let need = match t.0.wrapping_sub(CARD_FN) {
                0x80 => 2,
                0xb8 | 0xf0 => 4,
                0xc0 | 0x110 => 3,
                _ => 1,
            };
            assert!(p.1.len() >= need, "{ctx}: call {i} ({:#x}) with {} arguments", t.0, p.1.len());
            assert_eq!(&t.1[..p.1.len()], &p.1[..], "{ctx}: arguments of call {i} ({:#x})", t.0);
        }
        if let Some(v) = mine_ret {
            assert_eq!(theirs_ret, v, "{ctx}: result");
        }
        if theirs != m.regions[0].1 {
            let at = theirs.iter().zip(&m.regions[0].1).position(|(x, y)| x != y).unwrap();
            panic!("{ctx}: memory differs at +{at:#x}: game {:#x} port {:#x}", theirs[at], m.regions[0].1[at]);
        }
        assert_eq!(theirs_flag, m.r32(MC_SAVE_FLAG), "{ctx}: save flag");
        if which == 2 {
            // the copied construction vtable; other functions' own frames overlap it with their saved registers
            assert_eq!(theirs_frame, m.regions[3].1, "{ctx}: frame");
        }
        r.mem.write_u32(APP, app_word);
    }
    eprintln!("cases per function: {seen:?}");
}
