//! The Trick Tutorial's pad maps (board row C11c2) against the original in the function runner (TRICKY_ELF; skipped
//! without it): `LessonMan_Init` 0x14aaa0 itself runs, with its own pad code real (`Pad_GetConfig` 0x17a290,
//! `Pad_SetConfig` 0x17a2f8, `EAMem_Copy`, the `PadCfg_*` getters, `Input_BindButton` 0x179818, `Input_BindAxis`
//! 0x179838, the trick table 0x333098) and everything else stood in for (the node and trick-info constructors, the
//! reset, the allocations, the trick pictures, the archive, `Pad_LoadButtonMap` 0x179a88 handing it a random full map
//! or failing). Then `input::LessonPads::new` on the same player's map, map and rider must give the same three
//! tables kept for the player (+0xdc..), for the lesson (+0x2a4..) and left in the player's `cInput`; and the
//! preview's swap (`Pad_SetConfig` from `cLessonPreviewHandler::Enter` / `Exit`) runs the real `Pad_SetConfig` on
//! both.

use r5900::Runner;
use tricky_game::controls::ButtonMap;
use tricky_game::input::{LessonPads, PadConfig, PadTables};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

const GAPP: u32 = 0x337c58;
const LESSON_INIT: u32 = 0x14aaa0;
const SET_CONFIG: u32 = 0x17a2f8;
const REAL: [u32; 12] =
    [0x17a290, SET_CONFIG, 0x2ce114, 0x17a018, 0x17a028, 0x17a108, 0x17a118, 0x17a128, 0x17a138, 0x179818, 0x179838, LESSON_INIT];
/// stood in for, answering 0x40 (an address nobody reads)
const STUBS: [u32; 12] =
    [0x17cf50, 0x154d20, 0x14b5a8, 0x2cc808, 0x2fca28, 0x2c8018, 0x2c266c, 0x2ccfc0, 0x23bb70, 0x23bc28, 0x14f550, SHAPE];
const ALLOC: u32 = 0x2ccf70;
const LOAD_MAP: u32 = 0x179a88;
/// the renderer's +0x154 (a picture added)
const SHAPE: u32 = 0x0182_0154;

fn tables_at(m: &mut r5900::Mem, p: u32, l: u32, a: u32) -> PadTables {
    PadTables {
        pressed: std::array::from_fn(|k| m.read_u32(p + 4 * k as u32)),
        looked_at: std::array::from_fn(|k| m.read_u32(l + 4 * k as u32)),
        axes: std::array::from_fn(|k| (m.read_u32(a + 8 * k as u32), m.read_u32(a + 8 * k as u32 + 4) != 0)),
    }
}

fn input_tables(m: &mut r5900::Mem, i: u32) -> PadTables {
    tables_at(m, i + 4, i + 0xa8, i + 0x14c)
}

#[test]
fn lesson_pad_maps_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in REAL {
        r.allow(f);
    }
    for f in STUBS {
        r.stub(f, |c, _| c.set_gpr(2, 0x40));
    }
    let buf = r.mem.alloc(0x10);
    r.stub(ALLOC, move |c, _| c.set_gpr(2, buf as u64));
    let map: std::rc::Rc<std::cell::RefCell<Option<PadConfig>>> = Default::default();
    let mp = map.clone();
    r.stub(LOAD_MAP, move |c, m| {
        let ctx = c.gpr(4) as u32;
        match &*mp.borrow() {
            Some(cfg) => {
                for (base, pairs) in [(0x14u32, &cfg.bools), (0x84, &cfg.grabs), (0xfc, &cfg.analog)] {
                    for (k, (a, b)) in pairs.iter().enumerate() {
                        m.write_u32(ctx + base + 8 * k as u32, *a);
                        m.write_u32(ctx + base + 4 + 8 * k as u32, *b);
                    }
                }
                for (o, n) in [(8u32, cfg.bools.len()), (0xc, cfg.grabs.len()), (0x10, cfg.analog.len())] {
                    m.write_u32(ctx + o, n as u32);
                }
                c.set_gpr(2, 1);
            }
            None => c.set_gpr(2, 0),
        }
    });
    let lesson = r.mem.alloc(0x700);
    let app = r.mem.alloc(0x800);
    let rend = r.mem.alloc(0x40);
    let vt = r.mem.alloc(0x200);
    let game = r.mem.alloc(0x40);
    let race = r.mem.alloc(0x100);
    let ctrl = r.mem.alloc(0x900);
    let boarder = r.mem.alloc(0x500);
    let stats = r.mem.alloc(0x80);
    let input = r.mem.alloc(0x200);
    r.mem.write_u32(GAPP, app);
    r.mem.write_u32(app + 0x724, rend);
    r.mem.write_u32(app + 0x730, game);
    r.mem.write_u32(rend + 0x10, vt);
    r.mem.write_u16(vt + 0x150, 0);
    r.mem.write_u32(vt + 0x154, SHAPE);
    r.mem.write_u32(game + 0x1c, race);
    r.mem.write_u32(race + 0x8c, ctrl);
    r.mem.write_u32(ctrl, boarder);
    r.mem.write_u32(ctrl + 0x870, input);
    r.mem.write_u32(boarder + 0x464, stats);
    let mut g = Rng(0xc11c_2000_0000_0001);
    let mut loaded = 0;
    for case in 0..600 {
        let (ch, kind) = (g.below(12), g.below(3));
        r.mem.write_u32(stats + 0x38, ch);
        r.mem.write_u32(stats + 0x5c, ch);
        r.mem.write_u32(stats + 0x44, kind);
        // (the player's map in the input: random words)
        let mut ours = ButtonMap { pressed: [0; 0x29], looked_at: [0; 0x29], axes: [(0, false); 16], shown: [0; 0x29] };
        for k in 0..0x29 {
            ours.pressed[k] = g.next();
            ours.looked_at[k] = g.next();
            r.mem.write_u32(input + 4 + 4 * k as u32, ours.pressed[k]);
            r.mem.write_u32(input + 0xa8 + 4 * k as u32, ours.looked_at[k]);
        }
        for k in 0..16 {
            ours.axes[k] = (g.next(), g.below(2) == 1);
            r.mem.write_u32(input + 0x14c + 8 * k as u32, ours.axes[k].0);
            r.mem.write_u32(input + 0x150 + 8 * k as u32, ours.axes[k].1 as u32);
        }
        let cfg = (g.below(5) != 0).then(|| PadConfig {
            bools: (0..14).map(|_| (g.next(), g.next())).collect(),
            grabs: (0..15).map(|_| (g.next(), g.next())).collect(),
            analog: (0..16).map(|_| (g.next(), g.below(2))).collect(),
        });
        loaded += cfg.is_some() as u32;
        *map.borrow_mut() = cfg.clone();
        r.cpu.set_gpr(4, lesson as u64);
        r.cpu.set_gpr(5, 0);
        r.cpu.set_gpr(6, 0);
        r.call(LESSON_INIT).unwrap_or_else(|e| panic!("case {case}: {e}"));
        // (the rider's slots as the game reads them, and our table of them)
        let slots: [u16; 15] = std::array::from_fn(|k| {
            let at = 0x333098 + 0x10 * k as u32 + 0x2d0 * ch + 0xf0 * kind;
            r.mem.read_u32(at) as u16
        });
        let table = &tricky_data::trick_slots::TRICK_SLOTS[ch as usize][kind as usize];
        assert_eq!(slots, std::array::from_fn(|k| table[k][0]), "case {case}: the trick table");
        let pads = LessonPads::new(&mut ours, cfg.as_ref(), slots);
        let player = tables_at(&mut r.mem, lesson + 0xdc, lesson + 0x180, lesson + 0x224);
        let lessons = tables_at(&mut r.mem, lesson + 0x2a4, lesson + 0x348, lesson + 0x3ec);
        assert_eq!(player, pads.player, "case {case}: the player's tables kept");
        assert_eq!(lessons, pads.lesson, "case {case}: the lesson's tables");
        assert_eq!(input_tables(&mut r.mem, input), tricky_game::input::get_config(&ours), "case {case}: the input after");
        // the preview's swap in and out, with the game's own Pad_SetConfig
        for (to_lesson, at) in [(true, 0x2a4u32), (false, 0xdc)] {
            r.cpu.set_gpr(4, input as u64);
            for (k, o) in [0u32, 0xa4, 0x148].into_iter().enumerate() {
                r.cpu.set_gpr(5 + k, (lesson + at + o) as u64);
            }
            r.call(SET_CONFIG).unwrap();
            pads.swap(&mut ours, to_lesson);
            assert_eq!(input_tables(&mut r.mem, input), tricky_game::input::get_config(&ours), "case {case}: swap {to_lesson}");
        }
    }
    assert!(loaded > 300 && loaded < 600, "{loaded}");
}
