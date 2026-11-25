//! The async rider loader in `tricky_game::bdrload` (board row E2f) against the original in the function runner
//! (TRICKY_ELF; skipped without it). Every case is one of the row's functions on fresh random memory: a loader
//! whose twelve slots are free, loading or loaded at random, ids below 0 or not, one, two or three managers (the
//! board's sometimes none) with a table of stand-in virtuals, a rider whose choices pick names from the exe's own
//! tables (copied in), request words of any bits. The managers' virtuals, `File_Load`, the frees, the copy, the
//! skin build and `Mem_Delete` are stand-ins on both sides: the copy copies, the skin build sometimes clears a
//! file the loader frees after, the rest answer by their arguments. The calls, the answer and the memory (not the
//! stack's) are compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::bdrload as bl;
use tricky_game::pf::Mem;
use tricky_game::ps2f::F;
use tricky_game::textbox::TextBoxEnv;

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
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
}

/// The exe's name tables (0x3419c8..0x341f98), copied in
const TABLES: (u32, u32) = (0x341900, 0x700);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x2000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const LOAD: u32 = ARENA + 0x100;
const RIDER: u32 = ARENA + 0x600;
const CHOICES: u32 = ARENA + 0xb00;
const MGR: [u32; 3] = [ARENA + 0xc00, ARENA + 0xc40, ARENA + 0xc80];
const VTAB: u32 = ARENA + 0xd00;
/// A request word outside the loader
const WORD: u32 = ARENA + 0xe00;
fn mv(off: u32) -> u32 {
    0x01f0_0000 + off
}
const VOFFS: [u32; 3] = [0x58, 0x60, 0x80];
const BUILD_SKIN: u32 = bl::BUILD_SKIN;

const FUNCS: [u32; 12] = [
    0x201788, 0x201790, 0x201a38, 0x201a98, 0x201ad8, 0x201b50, 0x201b90, 0x201c10, 0x201c20, 0x201d30, 0x201e68,
    0x202478,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32) -> usize {
    match f {
        bl::MEM_COPY => 3,
        bl::FILE_LOAD | BUILD_SKIN => 2,
        f if f == mv(0x80) => 5,
        f if f == mv(0x58) || f == mv(0x60) => 2,
        _ => 1,
    }
}
thread_local! {
    /// Each case's salt for the stand-ins' answers
    static SALT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}
fn answer(f: u32, w: &[u32]) -> u32 {
    let h = w.iter().fold(f ^ SALT.get(), |h, x| (h.rotate_left(5) ^ x).wrapping_mul(0x9e37_79b1));
    let h = h ^ (h >> 16);
    match f {
        bl::FILE_LOAD if h & 3 == 0 => 0,
        bl::FILE_LOAD => 0x0300_0000 | (h & 0xfff0),
        f if f == mv(0x60) => (h & 7 != 0) as u32 * (h >> 8),
        f if f == mv(0x80) => h,
        _ => h & 0xff,
    }
}

trait W {
    fn r8(&mut self, a: u32) -> u8;
    fn r32(&mut self, a: u32) -> u32 {
        (0..4).map(|k| (self.r8(a + k) as u32) << (8 * k)).sum()
    }
    fn w32(&mut self, a: u32, v: u32);
    fn w8(&mut self, a: u32, v: u8);
}
impl W for Mem {
    fn r8(&mut self, a: u32) -> u8 {
        Mem::r8(self, a)
    }
    fn w32(&mut self, a: u32, v: u32) {
        Mem::w32(self, a, v)
    }
    fn w8(&mut self, a: u32, v: u8) {
        Mem::w8(self, a, v)
    }
}
impl W for r5900::Mem {
    fn r8(&mut self, a: u32) -> u8 {
        let mut b = [0u8];
        self.read(a, &mut b).unwrap();
        b[0]
    }
    fn w32(&mut self, a: u32, v: u32) {
        self.write_u32(a, v)
    }
    fn w8(&mut self, a: u32, v: u8) {
        self.write_u8(a, v)
    }
}
/// What a stand-in sees: the skin build the slot it is handed (its first four words and the eight files)
fn seen(m: &mut dyn W, f: u32, w: &mut Vec<u32>) {
    if f == BUILD_SKIN {
        for at in [0, 4, 8, 0xc, 0x1c, 0x20, 0x24, 0x3c, 0x40, 0x44, 0x48, 0x4c] {
            let v = m.r32(w[1] + at);
            w.push(v);
        }
    }
}
/// What a stand-in writes: the copy copies (not always), the skin build clears the head .mpf or the helm .ssh it
/// was handed
fn effect(m: &mut dyn W, f: u32, w: &[u32], v: u32) {
    if f == bl::MEM_COPY && v & 1 == 0 {
        for k in 0..w[2] {
            let b = m.r8(w[1] + k);
            m.w8(w[0] + k, b);
        }
    } else if f == BUILD_SKIN && v & 3 != 0 {
        m.w32(w[1] + if v & 1 != 0 { 0x20 } else { 0x40 }, 0);
    }
}

struct Env {
    log: Vec<Call>,
}
impl TextBoxEnv for Env {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32], _f: &[F]) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func), 0);
        let v = answer(func, &w);
        seen(m, func, &mut w);
        effect(m, func, &w, v);
        self.log.push(Call(func, w));
        v
    }
}

#[test]
fn async_rider_loader_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    let mut tables = vec![0u8; TABLES.1 as usize];
    r.mem.read(TABLES.0, &mut tables).unwrap();
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let mut stubs = vec![bl::MEM_COPY, bl::FILE_LOAD, bl::EAMEM_FREE, bl::DELETE, BUILD_SKIN];
    stubs.extend(VOFFS.map(mv));
    for func in stubs {
        let l = log.clone();
        r.stub(func, move |c, m| {
            let mut w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            let v = answer(func, &w);
            seen(m, func, &mut w);
            effect(m, func, &w, v);
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut g = Rng(0xe2f0_0000_0000_0001);
    let mut calls = 0;
    let mut seen = [0u32; 3];
    for case in 0..FUNCS.len() as u32 * 400 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (TABLES.0, tables.clone()),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        for off in VOFFS {
            m.w16(VTAB + off, g.pick(&[0u16, 0, 0x10, 0xfff8]));
            m.w32(VTAB + off + 4, mv(off));
        }
        for mgr in MGR {
            m.w32(mgr + 0x10, VTAB);
        }
        m.w32(LOAD + 8, MGR[0]);
        m.w32(LOAD + 0xc, g.pick(&[0, MGR[1]]));
        m.w32(LOAD + 0x10, g.pick(&[MGR[2], MGR[0]]));
        let free = g.pick(&[0u32, 1, 3, 11, 12, 12]);
        for n in 0..bl::SLOTS {
            let s = LOAD + 0x14 + bl::SLOT * n;
            let state = if n < free && g.below(5) != 0 { g.pick(&[1, 1, 2, 3]) } else { g.pick(&[0, 0, 0, 1, 2]) };
            m.w32(s, state);
            for k in 0..8 {
                let at = if k < 3 { 0x10 + 4 * k } else { 0x28 + 4 * (k - 3) };
                let v = g.next();
                m.w32(s + at, g.pick(&[v & 0xff, v | 0x8000_0000, v & 0xfff, u32::MAX]));
            }
        }
        m.w32(RIDER + 0x464, CHOICES);
        for at in [0x48, 0x4c, 0x50, 0x54, 0x58] {
            m.w32(CHOICES + at, g.below(6));
        }
        let (w0, w1) = (g.next(), g.next());
        let word = g.pick(&[w0, w1 & 0x1f00, 0x1f01, 0x2601, w1 | 0x1f00]);
        let n0 = g.below(bl::SLOTS);
        let n = g.pick(&[n0, n0, n0, 0]);
        let (x, y) = (g.next(), g.next());
        let flags = g.pick(&[0, 1, 2, 3, x]);
        SALT.set(g.next());
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mm = &mut m;
        let (args, ours): (Vec<u32>, Option<u32>) = match func {
            0x201788 => (vec![LOAD, x], { bl::set_mode(mm, LOAD, x); None }),
            0x201790 => (vec![LOAD, RIDER, y, word], { bl::request_rider(mm, e, LOAD, RIDER, y, word, TOP - 0xf0); None }),
            0x201a38 => (vec![WORD], { bl::fe_request(mm, WORD); None }),
            0x201a98 => (vec![WORD], { bl::default_request(mm, WORD); None }),
            0x201ad8 => (vec![WORD], { bl::fe_board_request(mm, WORD); None }),
            0x201b50 => (vec![LOAD], Some(bl::init(mm, LOAD))),
            0x201b90 => (vec![LOAD, flags], { bl::destruct(mm, e, LOAD, flags); None }),
            0x201c10 => (vec![LOAD, x, y, flags], { bl::set_managers(mm, LOAD, x, y, flags); None }),
            0x201c20 => (vec![LOAD, n], { bl::free_slot(mm, e, LOAD, n); None }),
            0x201d30 => (vec![LOAD, n], Some(bl::is_slot_ready(mm, e, LOAD, n))),
            0x201e68 => (vec![LOAD, RIDER, y, word], Some(bl::request(mm, e, LOAD, RIDER, y, word, TOP - 0xb0))),
            0x202478 => (vec![LOAD, flags], { bl::base_destruct(mm, e, LOAD, flags); None }),
            _ => unreachable!(),
        };
        if func == 0x201d30 || func == 0x201e68 {
            let v = ours.unwrap();
            seen[if v == 0 { 0 } else if v == 1 { 1 } else { 2 }] += 1;
        }
        calls += env.log.len();
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        for (k, v) in args.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *v as i32 as i64 as u64);
        }
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if let Some(v) = ours {
            assert_eq!(r.cpu.gpr(2) as u32, v, "case {case} {func:#x}: the answer");
        }
        assert_eq!(*log.borrow(), env.log, "case {case} {func:#x}: the calls");
        for (at, b) in &m.regions {
            if *at == STACK.0 {
                continue;
            }
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    println!("{calls} calls, answers 0 / 1 / other: {seen:?}");
    assert!(calls > 5000);
    assert!(seen.iter().all(|&s| s > 50));
}
