//! `tricky_game::fe_manager::fe_init` (board row G9b2d2b) against the original in the function runner (TRICKY_ELF;
//! skipped without it): the front end manager's vf1 0x263b30, the 19 objects' set-up. Every case builds a random
//! manager, app, renderer, career and front-end globals in the arena, with the game's own object tables (0x3ab900..,
//! 0x3b5800..); the function then runs on both sides. The game's own functions it calls that are already checked
//! run for real on both sides (the player count 0x263670, the order 0x265788 and a character's place 0x2658e8, a
//! player's reset 0x265bb8, the effects loader 0x17b8a8); every other callee is a stand-in answering from the case's
//! seed: the constructors answer their object (some of them 0 at times) and give it tables of stand-ins at +0, +0xc
//! and +0x10, the platform's position is a random vector, the career's profiles a random count of random slots. The
//! calls are compared with their arguments (the stack's addresses as one token), the answers and the memory after
//! (the heap's objects included; the stack's scratch not).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use r5900::Runner;
use tricky_game::fe3dstate::{FeEnv, ALLOC, FILL, GAPP};
use tricky_game::fe_manager::init as it;
use tricky_game::pf::Mem;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        // splitmix64: successive answers unrelated
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
}

const DATA: (u32, u32) = (0x337c00, 0x400);
const CAREER: (u32, u32) = (0x32b000, 0x4000);
const SYS: (u32, u32) = (0x347000, 0x300);
const HEAPWORD: (u32, u32) = (0x3bb900, 0x100);
/// the game's object tables (`cFE3DAnimRotate`, `cFE3DMovieGeometry`, `cFE3DGeometry`; `cFE3DGroup`, `cFE3DThing`)
const TABLES: [(u32, u32); 2] = [(0x3ab900, 0x300), (0x3b5800, 0x300)];
/// their virtuals the set-up calls: the group's +0x20 and the geometry's +0x28
const GAME_SLOTS: [u32; 2] = [0x2accf8, 0x2accf0];
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x4000;
const APPOBJ: u32 = ARENA + 0x100;
const FOG: u32 = ARENA + 0x600;
/// three career slots: player 0, player 1, none (-1)
const SLOTS_AT: u32 = ARENA + 0x700;
const R: u32 = ARENA + 0x900;
const FE: u32 = ARENA + 0x1000;
const VT: u32 = ARENA + 0x2000;
/// the stand-in tables: one for each place an object keeps one (+0, +4, +8, +0xc, +0x10), each with its own adjust
fn vt2(k: u32) -> u32 {
    ARENA + 0x3800 + 0x180 * k
}
fn obj(k: u32) -> u32 {
    ARENA + 0x2800 + 0x80 * k
}
const HEAP: (u32, u32) = (0x0110_0000, 0x8000);
/// the stack's scratch on our side (in the stack's range, so its addresses log as the token)
const BUF: (u32, u32) = (0x01fe_0000, 0x100);
const STACK_TOKEN: u32 = 0x5c5c_5c5c;
fn slot(off: u32) -> u32 {
    0x0181_0000 + off
}
fn slot2(off: u32) -> u32 {
    0x0182_0000 + off
}
/// the objects' virtuals (the archives' +0x10, the title's +0x60, the scene's +0x118); the renderer's is slot(0x138)
const SLOTS2: [u32; 3] = [0x10, 0x60, 0x118];
/// the game's own functions here, run for real on both sides
const REAL: [u32; 5] = [0x263670, 0x265788, 0x2658e8, 0x265bb8, 0x17b8a8];
const FUNC: u32 = 0x263b30;

/// the constructors: answering their object (those in `MAYBE_NONE` 0 at times), giving it the stand-in tables
const CTORS: [u32; 11] = [
    it::MEMBIG_CTOR, it::ASYNCBIG_CTOR, it::CHAR_INIT, it::SOUND_CTOR, it::TITLE_CTOR,
    tricky_game::app::EFFECTS_CTOR, it::VIDEO_CTOR, it::BIG_INIT, it::BDR_INIT, it::CAMERA_CTOR, it::FOG_CTOR,
];
const MAYBE_NONE: [u32; 5] = [it::VIDEO_CTOR, it::BIG_INIT, it::BDR_INIT, it::CAMERA_CTOR, it::FOG_CTOR];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn token(a: u32) -> u32 {
    if (0x01f0_0000..0x01ff_0000).contains(&a) { STACK_TOKEN } else { a }
}

fn arity(f: u32) -> (usize, usize) {
    use it::*;
    match f {
        ANIM_OPEN => (5, 0),
        BDR_MANAGERS | tricky_game::fe_charselect::SEEN => (4, 0),
        ALLOC | FILL | BLOCKS_INIT | VIDEO_ADD | FONT_PARSE | BIG_OPEN | ICON_PACK => (3, 0),
        REFRESH => (2, 1),
        TRANSITION => (3, 3),
        f if f == slot2(0x10) => (3, 0),
        LISTS_INIT | PAD_WORD | LENS_FX | VIDEO_CTOR | COURSE_LOAD | BIG_DELETE | FOG_CTOR | BDR_MODE | PLAYER_SLOT
        | PLAYER_PROFILE | CHAR_INIT | CHAR_RESTORE | PUSH_BACK | ADD_WIDGET | BIND | GET_POS | GEOM_DTOR
        | SOUND_CTOR | tricky_game::fe_trickbook::CLEAR_SLOT => (2, 0),
        f if f == slot(0x138) || GAME_SLOTS.contains(&f) => (2, 0),
        _ => (1, 0),
    }
}

struct Answers {
    seed: Cell<u64>,
    n: Cell<u64>,
    heap: Cell<u32>,
}
impl Answers {
    fn next(&self) -> u32 {
        self.n.set(self.n.get() + 1);
        Rng(self.seed.get() ^ self.n.get().wrapping_mul(0x1234_5678_9abc_def1)).next()
    }
}

/// A stand-in's answer, its logged call and the words it writes
fn standin(f: u32, w: &[u32], fl: &[u32], ans: &Answers) -> (u32, Call, Vec<(u32, u32)>) {
    let (ni, nf) = arity(f);
    let mut log: Vec<u32> = w[..ni].iter().map(|&a| token(a)).collect();
    log.extend(&fl[..nf]);
    let r = ans.next();
    let mut g = Rng(r as u64 | 1);
    let mut writes = Vec::new();
    let answer = match f {
        ALLOC => {
            let at = ans.heap.get();
            ans.heap.set(at + ((w[1] + 0xf) & !0xf));
            assert!(ans.heap.get() <= HEAP.0 + HEAP.1, "the heap ran out");
            at
        }
        f if CTORS.contains(&f) => {
            let o = if MAYBE_NONE.contains(&f) && g.below(4) == 0 { 0 } else { w[0] };
            if o != 0 {
                writes.extend([(o, vt2(0)), (o + 0xc, vt2(3)), (o + 0x10, vt2(4))]);
            }
            o
        }
        it::COUNT_PROFILES => g.pick(&[0u32, 0, 1, 2, 3]),
        it::PLAYER_SLOT => g.pick(&[0u32, SLOTS_AT, SLOTS_AT + 1, SLOTS_AT + 2]),
        it::GET_POS => {
            for k in 0..3 {
                let v = (g.below(40001) as i32 - 20000) as f32 * 0.0371;
                writes.push((w[1] + 4 * k, v.to_bits()));
            }
            r
        }
        _ => r & !0x0100_0000,
    };
    (answer, Call(f, log), writes)
}

struct Env<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl FeEnv for Env<'_> {
    fn call(&mut self, m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        let (ni, nf) = arity(f);
        let mut w: Vec<u32> = args.iter().take(ni).copied().collect();
        w.resize(8, 0);
        let mut fl: Vec<u32> = args.iter().skip(ni).take(nf).copied().collect();
        fl.resize(8, 0);
        let (a, c, writes) = standin(f, &w, &fl, self.ans);
        for (at, v) in writes {
            m.w32(at, v);
        }
        self.log.push(c);
        assert!(self.log.len() < 100_000, "the port calls on and on");
        a
    }
}

#[test]
fn front_end_init_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=c").unwrap();
    for f in std::iter::once(&FUNC).chain(REAL.iter()) {
        r.allow(*f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0), heap: Cell::new(HEAP.0) });
    use it::*;
    let mut stubs = vec![
        BLOCKS_INIT, LISTS_INIT, PAD_WORD, ANIM_OPEN, LENS_FX, VIDEO_CTOR, VIDEO_ADD, FONT_PARSE, BIG_INIT, BIG_OPEN,
        BIG_CLOSE, BIG_DELETE, COURSE_LOAD, RUN_SCRIPT, FOG_CTOR, SPRITE_RECTS, ICON_PACK, DUMMY_TEXTURES,
        MEMBIG_CTOR, ASYNCBIG_CTOR, BDR_INIT, BDR_MODE, BDR_MANAGERS, COUNT_PROFILES, PLAYER_SLOT, PLAYER_PROFILE,
        CHAR_INIT, CHAR_RESTORE, CAMERA_CTOR, PUSH_BACK, LIST_INIT, ADD_WIDGET, NAME_HASH, BIND, GET_POS, GEOM_DTOR,
        REFRESH, SOUND_CTOR, TRANSITION, TITLE_CTOR, ALLOC, FILL, tricky_game::app::EFFECTS_CTOR,
        tricky_game::fe_charselect::SEEN, tricky_game::fe_trickbook::CLEAR_SLOT, slot(0x138),
    ];
    stubs.extend(GAME_SLOTS);
    stubs.extend(SLOTS2.map(slot2));
    for f in stubs {
        let (l, a) = (log.clone(), ans.clone());
        r.stub(f, move |c, mem| {
            let w: Vec<u32> = (4..12).map(|k| c.gpr(k) as u32).collect();
            let fl: Vec<u32> = (12..20).map(|k| c.f(k).to_bits()).collect();
            let (v, call, writes) = standin(f, &w, &fl, &a);
            for (at, x) in writes {
                mem.write_u32(at, x);
            }
            l.borrow_mut().push(call);
            assert!(l.borrow().len() < 100_000, "the original calls on and on");
            c.set_gpr(2, v as i32 as i64 as u64);
            c.set_f(0, f32::from_bits(v));
        });
    }
    let game = |r: &mut Runner, (at, n): (u32, u32)| {
        let mut b = vec![0u8; n as usize];
        r.mem.read(at, &mut b).unwrap();
        (at, b)
    };
    let data = game(&mut r, DATA);
    let tables: Vec<(u32, Vec<u8>)> = TABLES.iter().map(|t| game(&mut r, *t)).collect();
    let mut g = Rng(0x9b2d_2b00_0000_0001);
    let mut seen = [0u32; 4];
    for case in 0..3000u32 {
        let mut regions = vec![
            data.clone(),
            (CAREER.0, vec![0; CAREER.1 as usize]),
            (SYS.0, vec![0; SYS.1 as usize]),
            (HEAPWORD.0, vec![0; HEAPWORD.1 as usize]),
            (ARENA, vec![0; SIZE as usize]),
            (HEAP.0, vec![0xa5; HEAP.1 as usize]),
            (BUF.0, vec![0; BUF.1 as usize]),
        ];
        regions.extend(tables.iter().cloned());
        let mut m = Mem { regions };
        // (random words, none in the stack's range)
        for (at, n) in [(ARENA, SIZE), (CAREER.0, CAREER.1), (SYS.0, SYS.1), (HEAPWORD.0, HEAPWORD.1)] {
            for k in 0..n / 4 {
                m.w32(at + 4 * k, g.next() & !0x0100_0000);
            }
        }
        let mode = g.pick(&[0u32, 1, 2, 3, 4, 5, 8, 16, 16, 17]);
        m.w32(tricky_game::fe_characcept::CAREER + 0x3260, mode);
        m.w32(GAPP, APPOBJ);
        m.w32(APPOBJ + 0x724, R);
        m.w32(APPOBJ + 0x5c, obj(28));
        m.w32(APPOBJ + 0x60, obj(29));
        m.w32(R + 0x10, VT);
        m.w32(R + 0x47c, FOG);
        m.w16(VT + 0x138, g.pick(&[0u16, 4]));
        m.w32(VT + 0x13c, slot(0x138));
        for k in 0..5 {
            for off in SLOTS2 {
                m.w16(vt2(k) + off, 4 * k as u16);
                m.w32(vt2(k) + off + 4, slot2(off));
            }
        }
        for k in 0..32 {
            for (j, at) in [0u32, 4, 8, 0xc, 0x10].into_iter().enumerate() {
                m.w32(obj(k) + at, vt2(j as u32));
            }
        }
        m.w8(SLOTS_AT, 0);
        m.w8(SLOTS_AT + 1, 1);
        m.w8(SLOTS_AT + 2, 0xff);
        m.w32(FE + 0xa4, if g.below(2) == 0 { 0 } else { obj(12) });
        m.w32(tricky_game::fe_manager::setup::FIRST, obj(20));
        m.w8(FE + 4, g.below(3) as u8);
        seen[(mode == 16) as usize * 2 + (m.r32(FE + 0xa4) == 0) as usize] += 1;
        for (at, b) in &m.regions {
            r.mem.write(*at, b).unwrap();
        }
        r.mem.write(0x01ff_0000 - 0x4000, &[0u8; 0x4000]).unwrap();
        ans.seed.set(((g.next() as u64) << 32) | g.next() as u64);
        ans.n.set(0);
        ans.heap.set(HEAP.0);
        let mut env = Env { log: Vec::new(), ans: &ans };
        tricky_game::fe_manager::fe_init(&mut m, &mut env, FE, BUF.0);
        ans.n.set(0);
        ans.heap.set(HEAP.0);
        log.borrow_mut().clear();
        r.cpu.set_gpr(4, FE as i32 as i64 as u64);
        r.call(FUNC).unwrap_or_else(|e| panic!("case {case}: {e}"));
        assert_eq!(*log.borrow(), env.log, "case {case}: the calls");
        for (at, b) in &m.regions {
            if *at == BUF.0 {
                continue;
            }
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    assert!(seen.iter().all(|&n| n > 100), "every way through: {seen:?}");
}
