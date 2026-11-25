//! `tricky_game::memorybig` (board row E2e) against the original in the function runner (TRICKY_ELF; skipped
//! without it): the in-memory BIG file and the BIG wrappers on a random request table, `BIG_Lookup` stood in on both
//! sides (it answers a name or 0 and fills the offset and size words it is given), every call out logged and the
//! answers and memory compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::asyncbig::{self as ab, BigEnv};
use tricky_game::memorybig as mb;
use tricky_game::pf::Mem;

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

const GLOBALS: (u32, u32) = (0x343a50, 0x2400);
const SINK: (u32, u32) = (0x3bb960, 0x10);
const BIG: u32 = ARENA + 0x1000;
const LIST: u32 = ARENA + 0x700;
const WORDS: u32 = ARENA + 0x780;
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x2000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const F: [u32; 2] = [ARENA + 0x100, ARENA + 0x140];
const NAMES: u32 = ARENA + 0x400;
const OUT: u32 = ARENA + 0x600;
const HANDLES: [u32; 3] = [0x11, 0x22, 0x33];
const CB: u32 = 0x01f1_0000;
fn vf(off: u32) -> u32 {
    0x01f0_0000 + off
}
const NAME_TEXTS: [&[u8]; 6] = [b"|a\0", b"|b\0", b"a\0", b"b\0", b"|c\0", b"\0"];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32) -> usize {
    match f {
        ab::MUTEX_CREATE => 0,
        ab::MEMPOOL_ALLOC => 6,
        mb::BIG_LOOKUP => 5,
        mb::FILE_LOAD_ALLOC => 2,
        ab::ASYNC_FINISH | ab::EAMEM_SET | ab::EAMEM_ALLOC | ab::EAMEM_COPY => 3,
        ab::COMPRESS_DECODE | ab::ASYNC_LOAD | ab::ASYNC_SET_CALLBACK | CB => 2,
        f if f == vf(0x40) || f == vf(0x48) => 3,
        f if f == vf(0x50) => 2,
        _ => 1,
    }
}
fn answer(f: u32, w: &[u32]) -> u32 {
    let h = w.iter().fold(f, |h, x| (h.rotate_left(5) ^ x).wrapping_mul(0x9e37_79b1));
    let h = h ^ (h >> 16);
    match f {
        ab::COMPRESS_UNPACKED_SIZE if h & 3 == 0 => 0,
        mb::BIG_LOOKUP if h & 3 == 0 => 0,
        mb::BIG_LOOKUP => NAMES + 0x10 * ((h >> 8) % NAME_TEXTS.len() as u32),
        ab::ASYNC_LOAD => HANDLES[(h >> 7) as usize % 3],
        f if f == vf(0x48) => h >> 3,
        ab::EAMEM_ALLOC | ab::MEMPOOL_ALLOC if h & 7 == 0 => 0,
        _ => h & 0xff,
    }
}

/// The memory both kinds of stand-in write through
trait W {
    fn r8(&mut self, a: u32) -> u8;
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
/// What a stand-in writes
fn effect(m: &mut dyn W, f: u32, w: &[u32], v: u32) {
    if f == vf(0x40) {
        let mut k = 0;
        loop {
            let b = m.r8(w[2] + k);
            m.w8(w[1] + k, b);
            if b == 0 {
                break;
            }
            k += 1;
        }
    } else if f == vf(0x48) && v & 1 != 0 {
        m.w32(w[1] - 0x24 + 0xc, 3);
    } else if f == mb::BIG_LOOKUP && v != 0 {
        // found: the offset and size from all the arguments
        let h = w.iter().fold(0x5bd1_e995u32, |h, x| (h ^ x).wrapping_mul(0x9e37_79b1).rotate_left(13));
        if w[3] != 0 {
            m.w32(w[3], (h >> 4) & 0xffc);
        }
        if w[4] != 0 {
            m.w32(w[4], h >> 20);
        }
    } else if f == ab::ASYNC_FINISH && w[1] != 0 {
        m.w32(w[1], v.wrapping_mul(0x1001));
        m.w32(w[2], v ^ 0x55);
    }
}

struct Env {
    log: Vec<Call>,
}
impl BigEnv for Env {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func), 0);
        let v = answer(func, &w);
        effect(m, func, &w, v);
        self.log.push(Call(func, w));
        v
    }
}

const FUNCS: [u32; 16] = [
    0x2cc010, 0x2cc030, 0x2cc060, 0x2cc0a8, 0x2cc120, 0x2cc268, 0x23c9d0, 0x23ca08, 0x23ca70, 0x23caa0, 0x23cb70,
    0x23ca50, 0x23cad8, 0x23cb78, 0x23cb80, 0x23cdb8,
];
const LISTS: [&[u8]; 4] = [b"a,b,|c\0", b"a\0", b",b,\0", b"\0"];

#[test]
fn memory_big_file_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    assert_eq!(r.gp().wrapping_sub(0x7f90), mb::SIZE_SINK);
    for f in FUNCS {
        r.allow(f);
    }
    // cAsyncBigFile's constructor and destructor (row E2d), its empty vf2, the C library's string calls
    for f in [0x23bd10, 0x23bd90, 0x23bea0, 0x2fd14c, 0x2fce78] {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let stubs = [
        ab::MEM_DELETE,
        ab::MEM_FREE,
        ab::MEMPOOL_ALLOC,
        ab::COMPRESS_DECODE,
        ab::COMPRESS_UNPACKED_SIZE,
        ab::EAMEM_SET,
        ab::EAMEM_ALLOC,
        ab::EAMEM_FREE,
        ab::EAMEM_COPY,
        ab::MUTEX_CREATE,
        ab::MUTEX_DESTROY,
        mb::BIG_LOOKUP,
        mb::FILE_LOAD_ALLOC,
    ];
    for func in stubs {
        let l = log.clone();
        r.stub(func, move |c, m| {
            let w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            let v = answer(func, &w);
            effect(m, func, &w, v);
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut g = Rng(0xe2e0_0000_0000_0001);
    let mut calls = 0;
    for case in 0..FUNCS.len() as u32 * 300 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (GLOBALS.0, vec![0; GLOBALS.1 as usize]),
                (SINK.0, vec![0; SINK.1 as usize]),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        for (k, t) in NAME_TEXTS.iter().enumerate() {
            for (j, b) in t.iter().enumerate() {
                m.w8(NAMES + 0x10 * k as u32 + j as u32, *b);
            }
        }
        for k in 0..STACK.1 / 4 {
            m.w32(STACK.0 + 4 * k, g.next());
        }
        for (j, b) in LISTS[g.below(4) as usize].iter().enumerate() {
            m.w8(LIST + j as u32, *b);
        }
        let f = g.pick(&F);
        m.w32(f, g.pick(&[0, 1]));
        m.w32(f + 0x14, g.pick(&[0, BIG, BIG]));
        m.w32(ab::LOCK, g.next());
        m.w32(ab::FILES, g.pick(&[1, 1, 2, 5]));
        let i = g.below(ab::COUNT);
        let q = ab::req(i);
        let any = g.next();
        m.w32(q + 0x20, g.pick(&[0, 0, any]));
        for (j, b) in NAME_TEXTS[g.below(5) as usize].iter().enumerate() {
            m.w8(q + 0x24 + j as u32, *b);
        }
        m.w32(q + 0x58, g.pick(&[0, 1, 1, 2, u32::MAX]));
        let how = g.below(4);
        let (x, y) = (g.next(), g.below(8));
        let k = g.below(NAME_TEXTS.len() as u32);
        let name = g.pick(&[0, NAMES + 0x10 * k]);
        let flags = g.pick(&[0, 0x4000_0000, 0x8000_0000, 0xc000_0000, x]);
        let (o1, o2) = (g.pick(&[0, WORDS]), g.pick(&[0, WORDS + 4]));
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mm = &mut m;
        let (args, ours): (Vec<u32>, Option<u32>) = match func {
            0x2cc010 => (vec![BIG, name, y, o1, o2], Some(mb::find(mm, e, BIG, name, y, o1, o2))),
            0x2cc030 => (vec![BIG, name], Some(mb::file_offset(mm, e, BIG, name, TOP))),
            0x2cc060 => (vec![BIG, y], Some(mb::file_data_by_index(mm, e, BIG, y, TOP))),
            0x2cc0a8 => (vec![BIG, name], Some(mb::file_data(mm, e, BIG, name, TOP))),
            0x2cc120 => (vec![BIG, LIST, WORDS], { mb::file_data_list(mm, e, BIG, LIST, WORDS, TOP); None }),
            0x2cc268 => (vec![BIG, y, OUT], Some(mb::file_name(mm, e, BIG, y, OUT))),
            0x23c9d0 => (vec![f], Some(mb::construct(mm, e, f))),
            0x23ca08 => (vec![f, how], { mb::destruct(mm, e, f, how); None }),
            0x23ca70 => (vec![f, name, x], { mb::vf1(mm, e, f, name, x); None }),
            0x23caa0 => (vec![f], { mb::vf2(mm, e, f); None }),
            0x23cb70 => (vec![f], { mb::vf6(); None }),
            0x23ca50 => (vec![f, OUT, NAMES + 0x10 * (y % 6)], { mb::vf7(mm, OUT, NAMES + 0x10 * (y % 6)); None }),
            0x23cad8 => (vec![f, i], { mb::vf10(mm, e, i); None }),
            0x23cb78 => (vec![f], Some(mb::vf11())),
            0x23cb80 => (vec![f, i, flags], Some(mb::vf12(mm, e, f, i, flags, TOP))),
            0x23cdb8 => (vec![f, i, flags, x], Some(mb::vf13(mm, e, f, i, flags, TOP))),
            _ => unreachable!(),
        };
        calls += env.log.len();
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        for (k, v) in args.iter().enumerate() {
            r.cpu.set_gpr(if k < 4 { 4 + k } else { 8 + k - 4 }, *v as i32 as i64 as u64);
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
    println!("{calls} calls");
    assert!(calls > 3000);
}
