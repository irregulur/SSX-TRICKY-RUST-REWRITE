//! `tricky_game::fileload` (board row E2u) against the original in the function runner (TRICKY_ELF; skipped
//! without it): random calls of every load, sync operation and callback, the callbacks also run on their own with
//! made-up argument blocks. The file system, the heap and the thread runner are answered the same on both sides
//! (their waits end reads, the runner fills in sizes) and logged; the heap hands out buffers holding random bytes,
//! packed files (some the codec refuses) or archive directories (shorter and longer than the first read). Memory
//! (all but the stack, which is filled the same on both sides before each call), answers and the calls out are
//! compared after every call.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::fileload::{self as fl, LoadEnv};
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
}

const GLOBALS: u32 = 0x3bb900;
const STACK_TOP: u32 = 0x01ff_0000;
const STACK: u32 = STACK_TOP - 0x1000;
const BUFS: u32 = 0x0100_0000;
const SLOT: u32 = 0x6000;
const SLOTS: u32 = 8;
const BLOCK: u32 = 0x0110_0000;
const DST: u32 = 0x0111_0000;
const ALLOC_FN: u32 = 0x0170_0000;
const FREE_FN: u32 = 0x0170_0100;

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

#[derive(Clone)]
struct World {
    seed: u32,
    n: u32,
    /// read and write operations and their blocks
    reads: Vec<(u32, u32)>,
}

fn is_queue(f: u32) -> bool {
    [fl::FS_QUEUE_OPEN, fl::FS_QUEUE_CLOSE, fl::FS_GET_FILE_SIZE, fl::FS_OPEN, fl::FS_QUEUE_MOUNT_BIG, fl::FS_QUEUE_UNMOUNT_BIG, fl::FS_QUEUE_READ, fl::FS_QUEUE_WRITE]
        .contains(&f)
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32]) -> (u32, Vec<(u32, u32)>) {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        let mut w = vec![];
        let v = match f {
            _ if is_queue(f) => {
                if h % 8 == 0 {
                    0
                } else {
                    let op = (h >> 4) | 0x10;
                    if f == fl::FS_QUEUE_READ || f == fl::FS_QUEUE_WRITE {
                        self.reads.push((op, a[5]));
                    }
                    op
                }
            }
            fl::FS_WAIT_OP => {
                let b = self.reads.iter().rev().find(|(o, _)| *o == a[0] || a[0] == 0).map(|x| x.1);
                if let Some(b) = b {
                    if h % 4 == 0 {
                        // the read's callback counted bytes
                        w.push((b + 0x10, h >> 18));
                    }
                    if h % 3 != 0 {
                        w.push((b + 0x20, 0));
                        if h % 3 == 1 {
                            w.push((b + 0xc, 0));
                        }
                    }
                }
                0
            }
            fl::FS_GET_STATUS => [1, 1, 1, 2, 0][(h % 5) as usize],
            fl::FS_GET_RESULT => match h % 6 {
                0 => 0,
                1 | 2 => 0x8000,
                3 => h & 0xfff,
                4 => 0x8000 + (h >> 20),
                _ => h,
            },
            fl::RUN_WITH_HEAP => {
                let b = a[3];
                let v = if h % 5 == 0 {
                    0
                } else if a[0] == fl::LOAD_INTO_CB {
                    h % 0x100
                } else {
                    BUFS + SLOT * ((h >> 3) % SLOTS)
                };
                // a load into a buffer on the stack (`File_PeekUnpackedSize`'s) read a packed file's header
                if a[0] == fl::LOAD_INTO_CB && b == STACK_TOP - 0xc0 && h % 2 == 0 {
                    let size = h >> 12;
                    w.push((STACK_TOP - 0x90, 0xfb10 | (size >> 16 & 0xff) << 16 | (size >> 8 & 0xff) << 24));
                    w.push((STACK_TOP - 0x8c, size & 0xff));
                }
                // a load that gave a buffer gave its size
                if h % 3 != 0 || (v != 0 && a[0] == fl::LOAD_ALLOC_SIZE_CB) {
                    // (never shorter than a packed file it gave: the game's sizes are the files')
                    w.push((b + 8, if h % 7 == 0 { 0 } else { 0x400 + (h >> 8) % 0xc00 }));
                }
                v
            }
            ALLOC_FN => {
                if h % 6 == 0 {
                    0
                } else {
                    BUFS + SLOT * ((h >> 3) % SLOTS)
                }
            }
            _ => h,
        };
        (v, w)
    }
}

/// The words of a load's block (the stack past it holds the caller's saved registers)
fn block_words(cb: u32) -> u32 {
    if cb == fl::LOAD_MANY_CB {
        5
    } else {
        4
    }
}

struct Env {
    w: World,
    log: Vec<Call>,
}

impl LoadEnv for Env {
    fn call(&mut self, m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        let mut logged = args.to_vec();
        if f == fl::RUN_WITH_HEAP {
            logged.extend((0..block_words(args[0])).map(|k| m.r32(args[3] + 4 * k)));
        }
        let (v, writes) = self.w.answer(f, args);
        for (a, x) in writes {
            m.w32(a, x);
        }
        self.log.push(Call(f, logged));
        v
    }
}

/// A RefPack stream of `data` (literals only); type 0x11 with a second size `b` the codec answers
fn packed(data: &[u8], second: Option<u32>) -> Vec<u8> {
    let n = data.len() as u32;
    let mut s = vec![if second.is_some() { 0x11 } else { 0x10 }, 0xfb, (n >> 16) as u8, (n >> 8) as u8, n as u8];
    if let Some(b) = second {
        s.extend([(b >> 16) as u8, (b >> 8) as u8, b as u8]);
    }
    let mut p = 0;
    while data.len() - p >= 4 {
        let k = ((data.len() - p) / 4).min(28);
        s.push(0xe0 + (k - 1) as u8);
        s.extend(&data[p..p + 4 * k]);
        p += 4 * k;
    }
    s.push(0xfc + (data.len() - p) as u8);
    s.extend(&data[p..]);
    s
}

fn slot_content(g: &mut Rng) -> Vec<u8> {
    let mut v: Vec<u8> = (0..SLOT).map(|_| g.next() as u8).collect();
    let head: Vec<u8> = match g.below(6) {
        0 | 1 => {
            let data: Vec<u8> = (0..g.below(0x300)).map(|_| g.next() as u8).collect();
            let second = match g.below(3) {
                0 => Some(0),
                1 => Some(data.len() as u32),
                _ => None,
            };
            packed(&data, second)
        }
        2 => {
            let size = if g.below(2) == 0 { g.below(0xa90) } else { 0xa80 + g.below(0x2000) };
            [0xc0, 0xfb, (size >> 8) as u8, size as u8].to_vec()
        }
        3 => {
            let size = match g.below(4) {
                0 => g.below(0xa91),
                1 => 0xa90 + g.below(2),
                _ => 0xa91 + g.below(0x4000),
            };
            [&b"BIGF"[..], &[0; 8], &size.to_be_bytes()].concat()
        }
        _ => vec![],
    };
    v[..head.len()].copy_from_slice(&head);
    v
}

#[test]
fn the_file_loads_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let funcs = [
        0x2c7e38, 0x2c7e58, 0x2c7ec8, 0x2c7f18, 0x2c7f60, 0x2c8018, 0x2c8068, 0x2c80b8, 0x2c8178, 0x2c81e8, 0x2c8258, 0x2c82f0, 0x2c8348, 0x2c8398,
        0x2c8430, 0x2c84a0, 0x2c8508, 0x2c8668, 0x2c86b8, 0x2c88d8, 0x2c8910, 0x2c8938, 0x2c8a70, 0x2c8a90, 0x2c8bc8, 0x2c8be8, 0x2c8cf0, 0x2c8d98,
        0x2c8e08, 0x2c8e28, 0x2c8e48, 0x2c8ea0, 0x2c8ef8, 0x2c8f50, 0x2c8fc0, 0x2c9018, 0x2c9070, 0x2cebb0,
    ];
    // the BIG library, the codecs, the copy
    for f in funcs.into_iter().chain([0x2cbc88, 0x2cbd08, 0x2c2fb0, 0x2c2de0, 0x2ce114, 0x2c31b0, 0x2c3730, 0x2c3540, 0x2c34d8, 0x2c2d18, 0x2c2bb8, 0x2c2bf8, 0x2c27c8]) {
        r.allow(f);
    }
    let world = Rc::new(RefCell::new(World { seed: 0, n: 0, reads: vec![] }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let outs = [
        (fl::FS_QUEUE_OPEN, 4),
        (fl::FS_QUEUE_CLOSE, 3),
        (fl::FS_GET_FILE_SIZE, 3),
        (fl::FS_OPEN, 3),
        (fl::FS_QUEUE_MOUNT_BIG, 4),
        (fl::FS_QUEUE_UNMOUNT_BIG, 3),
        (fl::FS_QUEUE_READ, 6),
        (fl::FS_QUEUE_WRITE, 6),
        (fl::FS_WAIT_OP, 1),
        (fl::FS_GET_STATUS, 1),
        (fl::FS_GET_RESULT, 1),
        (fl::FS_SET_CALLBACK, 2),
        (fl::RUN_WITH_HEAP, 4),
        (ALLOC_FN, 3),
        (FREE_FN, 1),
    ];
    for (f, n) in outs {
        let (world, log) = (world.clone(), log.clone());
        r.stub(f, move |c, m| {
            let a: Vec<u32> = (4..4 + n).map(|k| c.gpr(k) as u32).collect();
            let mut logged = a.clone();
            if f == fl::RUN_WITH_HEAP {
                for k in 0..block_words(a[0]) {
                    let mut b = [0u8; 4];
                    m.read(a[3] + 4 * k, &mut b).unwrap();
                    logged.push(u32::from_le_bytes(b));
                }
            }
            let (v, writes) = world.borrow_mut().answer(f, &a);
            for (at, x) in writes {
                m.write(at, &x.to_le_bytes()).unwrap();
            }
            log.borrow_mut().push(Call(f, logged));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }

    let mut g = Rng(0xf11e_0000_0000_0001);
    let mut hits = std::collections::BTreeMap::<&str, u32>::new();
    for seq in 0..400 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let mut bufs = vec![];
        for _ in 0..SLOTS {
            bufs.extend(slot_content(&mut g));
        }
        let mut m = Mem {
            regions: vec![(GLOBALS, rnd(&mut g, 0x100)), (BUFS, bufs), (BLOCK, rnd(&mut g, 0x200)), (DST, rnd(&mut g, 0x1000)), (STACK, vec![0; 0x1000])],
        };
        m.w32(fl::ALLOC_PTR, ALLOC_FN);
        m.w32(fl::FREE_PTR, FREE_FN);
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let w0 = World { seed: g.next(), n: 0, reads: vec![] };
        *world.borrow_mut() = w0.clone();
        log.borrow_mut().clear();
        let mut env = Env { w: w0, log: vec![] };
        for step in 0..40 {
            // the stack the same on both sides
            let st = rnd(&mut g, 0x1000);
            r.mem.write(STACK, &st).unwrap();
            m.regions[4].1 = st;
            let sp = STACK_TOP;
            let (p, q, x) = (g.next(), g.next(), g.next());
            let n = g.below(8);
            let one = g.below(2);
            let small = |g: &mut Rng| match g.below(5) {
                0 => 0,
                1 => g.next() | 0x8000_0000,
                2 => 0x8000 * (1 + g.below(3)),
                _ => g.below(0x20000),
            };
            // a made-up block for the callbacks
            let blk: Vec<u32> = (0..10).map(|_| g.next()).collect();
            let (f, args, ours, name): (u32, Vec<u32>, u32, &str) = match g.below(30) {
                0 => {
                    let out = BLOCK + 0x100;
                    (0x2c8d98, vec![p, q, n, out], fl::open_sync(&mut m, &mut env, p, q, n, out), "open sync")
                }
                1 => {
                    let out = BLOCK + 0x100;
                    (0x2c8f50, vec![p, q, x, out], fl::mount_big(&mut m, &mut env, p, q, x, out), "mount big")
                }
                2 => (0x2c8e48, vec![p, n], fl::close_sync(&mut m, &mut env, p, n), "close sync"),
                3 => (0x2c8ea0, vec![p, n], fl::size_sync(&mut m, &mut env, p, n), "size sync"),
                4 => (0x2c9018, vec![p, n], fl::fs_open_sync(&mut m, &mut env, p, n), "fs open sync"),
                5 => (0x2c8fc0, vec![p, n], fl::unmount_big(&mut m, &mut env, p, n), "unmount big"),
                6 => (if one == 1 { 0x2c8ef8 } else { 0x2c9070 }, vec![p, q, x], fl::sync_unsupported(), "unsupported"),
                7 => (0x2c7e38, vec![p], fl::exists(&mut m, &mut env, p), "exists"),
                8 | 9 => {
                    let size = small(&mut g);
                    let (f, ours) = if one == 1 {
                        (0x2c8e08, fl::read_sync(&mut m, &mut env, sp, p, q, x, size, n))
                    } else {
                        (0x2c8e28, fl::write_sync(&mut m, &mut env, sp, p, q, x, size, n))
                    };
                    (f, vec![p, q, x, size, n], ours, "read / write sync")
                }
                10 | 11 => {
                    for (k, v) in blk.iter().enumerate() {
                        m.w32(BLOCK + 4 * k as u32, *v);
                        r.mem.write_u32(BLOCK + 4 * k as u32, *v);
                    }
                    let left = small(&mut g);
                    let chunk = if g.below(2) == 0 { 0x8000 } else { g.below(0x9000) };
                    let queue = if one == 1 { fl::FS_QUEUE_READ } else { fl::FS_QUEUE_WRITE };
                    for (o, v) in [(0xc, left), (0x14, chunk), (0x1c, queue)] {
                        m.w32(BLOCK + o, v);
                        r.mem.write_u32(BLOCK + o, v);
                    }
                    let status = [1, 1, 1, 0, 2][g.below(5) as usize];
                    fl::read_chunk_cb(&mut m, &mut env, p, status, BLOCK);
                    (fl::READ_CHUNK_CB, vec![p, status, BLOCK], 0, "read chunk cb")
                }
                12..=18 => {
                    // a callback on a made-up block
                    for (k, v) in blk.iter().enumerate() {
                        m.w32(BLOCK + 4 * k as u32, *v);
                        r.mem.write_u32(BLOCK + 4 * k as u32, *v);
                    }
                    let k = g.below(7);
                    if k == 4 {
                        // the paths and the list the buffers go into
                        let count = g.below(6);
                        for (o, v) in [(0, count), (4, BLOCK + 0x40), (8, BLOCK + 0x80)] {
                            m.w32(BLOCK + o, v);
                            r.mem.write_u32(BLOCK + o, v);
                        }
                    }
                    if k == 3 && g.below(2) == 0 {
                        m.w32(BLOCK + 8, small(&mut g));
                        r.mem.write_u32(BLOCK + 8, m.r32(BLOCK + 8));
                    }
                    let (f, ours, name) = match k {
                        0 => (fl::GET_SIZE_CB, fl::get_size_cb(&mut m, &mut env, sp, n, BLOCK), "get size cb"),
                        1 => (fl::LOAD_ALLOC_CB, fl::load_alloc_cb(&mut m, &mut env, sp, n, BLOCK), "load alloc cb"),
                        2 => (fl::LOAD_ALLOC_SIZE_CB, fl::load_alloc_size_cb(&mut m, &mut env, sp, n, BLOCK), "load alloc size cb"),
                        3 => (fl::LOAD_INTO_CB, fl::load_into_cb(&mut m, &mut env, sp, n, BLOCK), "load into cb"),
                        4 => (fl::LOAD_MANY_CB, fl::load_many_cb(&mut m, &mut env, sp, BLOCK), "load many cb"),
                        _ => (fl::OPEN_BIG_CB, fl::open_big_cb(&mut m, &mut env, sp, n, BLOCK), "open big cb"),
                    };
                    (f, vec![n, BLOCK], ours, name)
                }
                19 => (if one == 1 { 0x2c7ec8 } else { 0x2c7f18 }, vec![p], fl::get_size(&mut m, &mut env, sp, p, one), "get size"),
                20 => (if one == 1 { 0x2c8018 } else { 0x2c8068 }, vec![p, q], fl::load_alloc(&mut m, &mut env, sp, p, q, one), "load alloc"),
                21 => {
                    let out = if g.below(4) == 0 { 0 } else { BLOCK + 0x100 };
                    let f = if one == 1 { 0x2c8178 } else { 0x2c81e8 };
                    (f, vec![p, out, q], fl::load_alloc_size(&mut m, &mut env, sp, p, out, q, one), "load alloc size")
                }
                22 => {
                    let size = small(&mut g);
                    (if one == 1 { 0x2c82f0 } else { 0x2c8348 }, vec![p, q, size], fl::load_into(&mut m, &mut env, sp, p, q, size, one), "load into")
                }
                23 => {
                    let count = if g.below(4) == 0 { g.next() } else { g.below(6) };
                    let f = if one == 1 { 0x2c8430 } else { 0x2c84a0 };
                    // the list of paths is read (its first word, in a delay slot) even for no files
                    let p = BLOCK + 0x40;
                    (f, vec![count, p, q, x], fl::load_many(&mut m, &mut env, sp, count, p, q, x, one), "load many")
                }
                24 => (if one == 1 { 0x2c8668 } else { 0x2c86b8 }, vec![p, q], fl::open_big(&mut m, &mut env, sp, p, q, one), "open big"),
                25 => (if one == 1 { 0x2c88d8 } else { 0x2c8910 }, vec![p], fl::peek_unpacked_size(&mut m, &mut env, sp, p, one == 1), "peek"),
                26 | 27 => {
                    let ours = if one == 1 { fl::load_and_unpack(&mut m, &mut env, sp, p, q) } else { fl::load_and_unpack(&mut m, &mut env, sp - 0x10, p, q) };
                    (if one == 1 { 0x2c8938 } else { 0x2c8a70 }, vec![p, q], ours, "load and unpack")
                }
                _ => {
                    let max = if g.below(3) == 0 { g.next() } else { g.below(0x1000) };
                    let ours = if one == 1 {
                        fl::load_unpack_into(&mut m, &mut env, sp, p, DST, max)
                    } else {
                        fl::load_unpack_into(&mut m, &mut env, sp - 0x10, p, DST, max)
                    };
                    (if one == 1 { 0x2c8a90 } else { 0x2c8bc8 }, vec![p, DST, max], ours, "load unpack into")
                }
            };
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
            }
            r.call(f).unwrap_or_else(|e| panic!("seq {seq} step {step}: {name} {f:#x} {args:x?}: {e}"));
            for (a, v) in m.regions.iter().filter(|(a, _)| *a != STACK) {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                if &t != v {
                    let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
                    panic!("seq {seq} step {step}: {name} {f:#x}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
                }
            }
            assert_eq!(*log.borrow(), env.log, "seq {seq} step {step}: {name} {f:#x} {args:x?}: the calls out");
            if f != fl::READ_CHUNK_CB {
                assert_eq!(r.cpu.gpr(2) as u32, ours, "seq {seq} step {step}: {name} {f:#x} {args:x?}: the answer");
            }
            *hits.entry(name).or_default() += (ours != 0) as u32;
        }
    }
    println!("non-zero answers: {hits:?}");
    assert!(hits.iter().all(|(k, v)| *v > 100 || ["read chunk cb", "unsupported"].contains(k)));
}
