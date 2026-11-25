//! `tricky_game::asyncbig` (board row E2d) against the original in the function runner (TRICKY_ELF; skipped without
//! it): two files with a table of stand-in virtuals, the shared request table filled at random (states, files,
//! handles, names, holders), every call out logged on both sides and the answers and memory compared. The stand-ins
//! that write do the same on both sides: the name maker copies the name, the read starter marks some requests read
//! at once, `AsyncFile_Finish` fills its two words.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::asyncbig::{self as ab, BigEnv};
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
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x2000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const F: [u32; 2] = [ARENA + 0x100, ARENA + 0x140];
/// A third file only the request table names
const F3: u32 = ARENA + 0x180;
const VTAB: u32 = ARENA + 0x200;
const NAMES: u32 = ARENA + 0x400;
const OUT: u32 = ARENA + 0x600;
const ARCHIVE: u32 = ARENA + 0x800;
const HANDLES: [u32; 3] = [0x11, 0x22, 0x33];
const CB: u32 = 0x01f1_0000;
fn vf(off: u32) -> u32 {
    0x01f0_0000 + off
}
const VOFFS: [u32; 4] = [0x38, 0x40, 0x48, 0x50];
const NAME_TEXTS: [&[u8]; 6] = [b"|a\0", b"|b\0", b"a\0", b"b\0", b"|c\0", b"\0"];

const FUNCS: [u32; 21] = [
    0x23bb70, 0x23bbe8, 0x23bd10, 0x23bd90, 0x23be90, 0x23bea8, 0x23d080, 0x23beb0, 0x23bf18, 0x23c010, 0x23c050,
    0x23c088, 0x23c190, 0x23c1d0, 0x23c210, 0x23c370, 0x23c398, 0x23c5c0, 0x23c8d0, 0x23c088, 0x23c210,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32) -> usize {
    match f {
        ab::MUTEX_CREATE => 0,
        ab::MEMPOOL_ALLOC => 6,
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

#[test]
fn async_big_file_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    // vf2 (empty, from the destructor) and the C library's string calls run as they are
    for f in [0x23bea0, 0x2fd008, 0x2fd14c, 0x2fcd4c] {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let mut stubs = vec![
        ab::MEM_DELETE,
        ab::MEM_FREE,
        ab::MEMPOOL_ALLOC,
        ab::COMPRESS_DECODE,
        ab::COMPRESS_UNPACKED_SIZE,
        ab::ASYNC_LOAD,
        ab::ASYNC_SET_CALLBACK,
        ab::ASYNC_FINISH,
        ab::ASYNC_CANCEL,
        ab::EAMEM_SET,
        ab::EAMEM_ALLOC,
        ab::EAMEM_FREE,
        ab::EAMEM_GET_SIZE,
        ab::EAMEM_COPY,
        ab::MUTEX_CREATE,
        ab::MUTEX_DESTROY,
        ab::MUTEX_LOCK,
        ab::MUTEX_UNLOCK,
        CB,
    ];
    stubs.extend(VOFFS.map(vf));
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
    let mut g = Rng(0xe2d0_0000_0000_0001);
    let mut calls = 0;
    for case in 0..FUNCS.len() as u32 * 300 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (GLOBALS.0, vec![0; GLOBALS.1 as usize]),
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
        for off in VOFFS {
            m.w16(VTAB + off, g.pick(&[0u16, 0, 0x10, 0xfff8]));
            m.w32(VTAB + off + 4, vf(off));
        }
        for f in [F[0], F[1], F3] {
            for at in [0, 8, 0xc] {
                m.w32(f + at, g.pick(&[0, 1, 1]));
            }
            m.w32(f + 0x10, VTAB);
        }
        m.w32(ab::LOCK, g.next());
        m.w32(ab::FILES, g.pick(&[0, 1, 1, 2, 5]));
        let busy = g.pick(&[0u32, 3, 10, 100]);
        for i in 0..ab::COUNT {
            let q = ab::REQUESTS + ab::REQUEST * i;
            if i >= busy && g.below(4) != 0 {
                continue;
            }
            let any = g.next();
            m.w32(q, g.pick(&[F[0], F[0], F[1], F3]));
            m.w32(q + 4, g.pick(&[0, CB]));
            for at in [8, 0x10, 0x14] {
                let v = g.next();
                m.w32(q + at, v);
            }
            m.w32(q + 0xc, g.pick(&[0, 1, 1, 2, 2, 3, 3, 7]));
            let h = g.pick(&HANDLES);
            m.w32(q + 0x18, g.pick(&[h, h, any]));
            for at in [0x1c, 0x20] {
                m.w32(q + at, g.pick(&[0, any]));
            }
            let t = NAME_TEXTS[g.below(5) as usize];
            for (j, b) in t.iter().enumerate() {
                m.w8(q + 0x24 + j as u32, *b);
            }
            m.w32(q + 0x58, g.pick(&[0, 1, 1, 2, u32::MAX]));
        }
        let f = g.pick(&F);
        let i = g.below(ab::COUNT);
        let how = g.below(4);
        let h = g.pick(&[HANDLES[0], HANDLES[1], HANDLES[2], 0x44]);
        let (x, y) = (g.next(), g.next());
        let name = NAMES + 0x10 * g.below(NAME_TEXTS.len() as u32);
        let flags = g.pick(&[0, 0x4000_0000, 0x8000_0000, 0xc000_0000, x]);
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mm = &mut m;
        let q = ab::REQUESTS + ab::REQUEST * i;
        let (args, ours): (Vec<u32>, Option<u32>) = match func {
            0x23bb70 => (vec![ARCHIVE], Some(ab::archive_init(mm, e, ARCHIVE))),
            0x23bbe8 => (vec![ARCHIVE], { ab::archive_reset(mm, e, ARCHIVE); None }),
            0x23bd10 => (vec![f], Some(ab::construct(mm, e, f))),
            0x23bd90 => (vec![f, how], { ab::destruct(mm, e, f, how); None }),
            0x23be90 => (vec![f], { ab::vf1(); None }),
            0x23bea8 => (vec![f], { ab::vf4(); None }),
            0x23d080 => (vec![f], Some(ab::vf5(mm, f))),
            0x23beb0 => (vec![h], { ab::finish_read(mm, e, h, TOP); None }),
            0x23bf18 => (vec![h, x, y], { ab::complete_request(mm, e, h, x, y); None }),
            0x23c010 => (vec![f, name, x], Some(ab::vf8(mm, e, name, x))),
            0x23c050 => (vec![f, q], { ab::vf9(mm, e, q); None }),
            0x23c088 => (vec![f], { ab::vf6(mm, e, f); None }),
            0x23c190 => (vec![f, OUT, name], { ab::vf7(mm, OUT, name); None }),
            0x23c1d0 => (vec![f], Some(ab::vf14(mm))),
            0x23c210 => (vec![f, name, x, CB, y], Some(ab::vf15(mm, e, f, name, x, CB, y, TOP))),
            0x23c370 => (vec![f, i], Some(ab::vf11(mm, i))),
            0x23c398 => (vec![f, i, flags], Some(ab::vf12(mm, e, i, flags))),
            0x23c5c0 => (vec![f, i, flags, name], Some(ab::vf13(mm, e, i, flags, name))),
            0x23c8d0 => (vec![f, i], { ab::vf10(mm, e, f, i); None }),
            _ => unreachable!(),
        };
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
    println!("{calls} calls");
    assert!(calls > 5000);
}
