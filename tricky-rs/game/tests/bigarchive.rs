//! The rest of the BIG archive code in `tricky_game::asyncbig` (board row E2g) against the original in the
//! function runner (TRICKY_ELF; skipped without it): archives open or not with names of any length, the memory
//! flags word and the request table filled at random, every call out a stand-in on both sides (the opens and mounts
//! log the flags word they see, the copy copies, the rest answer by their arguments), and the calls, the answer
//! and the memory compared. `strcpy` and `BigArchive_Reset` run as they are.

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
const FLAGS: (u32, u32) = (0x3bb970, 0x10);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x1000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const ARCHIVE: u32 = ARENA + 0x100;
const NAMES: u32 = ARENA + 0x400;
const DST: u32 = ARENA + 0x800;
const BUFS: u32 = ARENA + 0xa00;

const FUNCS: [u32; 8] = [0x23bb98, 0x23bc28, 0x23bcc0, 0x23be28, 0x23be98, 0x23bea0, 0x23c7c8, 0x23c818];
const STUBS: [u32; 11] = [
    ab::MEM_DELETE,
    ab::EAMEM_SET,
    ab::FS_OPEN_SYNC,
    ab::FS_MOUNT_BIG,
    ab::FS_UNMOUNT_BIG,
    ab::MEMSET,
    ab::COMPRESS_UNPACKED_SIZE,
    ab::COMPRESS_DECODE,
    ab::EAMEM_GET_SIZE,
    ab::EAMEM_COPY,
    ab::EAMEM_FREE,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

thread_local! {
    static SALT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn arity(f: u32) -> usize {
    match f {
        ab::FS_MOUNT_BIG => 4,
        ab::EAMEM_SET | ab::MEMSET | ab::EAMEM_COPY => 3,
        ab::FS_OPEN_SYNC | ab::FS_UNMOUNT_BIG | ab::COMPRESS_DECODE => 2,
        _ => 1,
    }
}
fn answer(f: u32, w: &[u32]) -> u32 {
    let h = w.iter().fold(f ^ SALT.get(), |h, x| (h.rotate_left(5) ^ x).wrapping_mul(0x9e37_79b1));
    let h = h ^ (h >> 16);
    match f {
        ab::FS_OPEN_SYNC | ab::FS_MOUNT_BIG | ab::FS_UNMOUNT_BIG | ab::COMPRESS_UNPACKED_SIZE if h & 3 == 0 => 0,
        ab::EAMEM_GET_SIZE => h % 0x40,
        _ => h,
    }
}

trait W {
    fn r8(&mut self, a: u32) -> u8;
    fn w8(&mut self, a: u32, v: u8);
    fn r32(&mut self, a: u32) -> u32 {
        (0..4).map(|k| (self.r8(a + k) as u32) << (8 * k)).sum()
    }
}
impl W for Mem {
    fn r8(&mut self, a: u32) -> u8 {
        Mem::r8(self, a)
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
    fn w8(&mut self, a: u32, v: u8) {
        self.write_u8(a, v)
    }
}
/// What a stand-in sees (the opens and mounts: the flags word) and writes (the copy copies)
fn work(m: &mut dyn W, f: u32, w: &mut Vec<u32>) {
    if f == ab::FS_OPEN_SYNC || f == ab::FS_MOUNT_BIG {
        let v = m.r32(ab::MEM_FLAGS);
        w.push(v);
    }
    if f == ab::EAMEM_COPY {
        for k in 0..w[2] {
            let b = m.r8(w[1] + k);
            m.w8(w[0] + k, b);
        }
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
        work(m, func, &mut w);
        self.log.push(Call(func, w));
        v
    }
}

#[test]
fn big_archive_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    // `BigArchive_Reset` and `strcpy` run as they are
    r.allow(0x23bbe8);
    r.allow(0x2fd14c);
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for func in STUBS {
        let l = log.clone();
        r.stub(func, move |c, m| {
            let mut w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            let v = answer(func, &w);
            work(m, func, &mut w);
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut g = Rng(0xe2a0_0000_0000_0001);
    let mut calls = 0;
    let mut answers = [0u32; 2];
    for case in 0..FUNCS.len() as u32 * 500 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (GLOBALS.0, vec![0; GLOBALS.1 as usize]),
                (FLAGS.0, vec![0; FLAGS.1 as usize]),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        for k in 0..GLOBALS.1 / 4 {
            m.w32(GLOBALS.0 + 4 * k, g.next());
        }
        for k in 0..FLAGS.1 / 4 {
            m.w32(FLAGS.0 + 4 * k, g.next());
        }
        // names of 0 to 0x7f characters, each ended
        for k in 0..4u32 {
            let any = g.below(0x80);
            let n = g.pick(&[0, 1, 5, 20, any]);
            for j in 0..n {
                m.w8(NAMES + 0x80 * k + j, b'a' + (g.below(26) as u8));
            }
            m.w8(NAMES + 0x80 * k + n.min(0x7f), 0);
        }
        let any = g.next();
        m.w32(ARCHIVE, g.pick(&[0, 0, 1, any]));
        let i = g.below(ab::COUNT);
        let buf = BUFS + 4 * g.below(0x40);
        m.w32(ab::req(i) + 0x1c, buf);
        let name = NAMES + 0x80 * g.below(4);
        let x = g.next();
        let flags = g.pick(&[0, 1, 2, 3, 0x10, x]);
        SALT.set(g.next());
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mm = &mut m;
        let (args, ours): (Vec<u32>, Option<u32>) = match func {
            0x23bb98 => (vec![ARCHIVE, flags], { ab::archive_delete(mm, e, ARCHIVE, flags); None }),
            0x23bc28 => (vec![ARCHIVE, name, flags], Some(ab::archive_open(mm, e, ARCHIVE, name, flags))),
            0x23bcc0 => (vec![ARCHIVE], Some(ab::archive_close(mm, e, ARCHIVE))),
            0x23be28 => (vec![], { ab::clear_table(mm, e); None }),
            0x23be98 => (vec![ARCHIVE], { ab::vf3(); None }),
            0x23bea0 => (vec![ARCHIVE], { ab::vf2(); None }),
            0x23c7c8 => (vec![ARCHIVE, i], Some(ab::unpacked_size(mm, e, i))),
            0x23c818 => (vec![ARCHIVE, i, DST], { ab::decompress_buffer(mm, e, i, DST); None }),
            _ => unreachable!(),
        };
        if let Some(v) = ours {
            answers[(v != 0) as usize] += 1;
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
    println!("{calls} calls, answers 0 / not: {answers:?}");
    assert!(calls > 4000);
    assert!(answers.iter().all(|&a| a > 200));
}
