//! The stream library's streaming in `tricky_game::strm` (board row E2ab) against the original in the function
//! runner (TRICKY_ELF; skipped without it): a stream made (`Strm_Create`, some sizes refused), its chunk types set,
//! then random requests from memory and from files (made-up packet streams, some packets of no chunk, each ending
//! in its last packet), packets taken and given back, purges, stops, the file system's answers (its callbacks
//! called when the test lets an operation end), lookups and a close. The file system writes the file's bytes where
//! a read asks and answers how many; it, the locks and the threads are answered the same on both sides and logged.
//! Memory, answers and the calls out are compared after every call.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::pf::Mem;
use tricky_game::strm::{self, StrmEnv};

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

const ST: u32 = 0x0110_0000;
const ST_SIZE: u32 = 0x40000;
const SRC: u32 = 0x0120_0000;
const NAMES: u32 = 0x0130_0000;
const GLOBALS: (u32, u32) = (0x3bbb00, 0x40);
const END: u32 = 0x7e45_4e44;

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

/// A packet stream: (bytes, where each packet starts); ids by top byte 1..5 (5: of no chunk type), the last END
fn packets(g: &mut Rng, n: usize) -> (Vec<u8>, Vec<u32>) {
    let mut v = vec![];
    let mut at = vec![];
    for k in 0..n {
        at.push(v.len() as u32);
        // (some last packets inside, so a request can end before the data does)
        let id = if k + 1 == n || g.below(40) == 0 { END } else { (1 + g.below(5)) << 24 | g.below(0x100) };
        let big = g.below(8) == 0;
        let size = 8 + 4 * g.below(if big { 0x800 } else { 0x80 });
        v.extend(id.to_le_bytes());
        v.extend(size.to_le_bytes());
        v.extend((8..size).map(|_| g.next() as u8));
    }
    (v, at)
}

#[derive(Clone)]
struct World {
    seed: u32,
    n: u32,
    file: Rc<Vec<u8>>,
    ops: HashMap<u32, u32>,
    next_op: u32,
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32]) -> (u32, Vec<(u32, Vec<u8>)>) {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        let mut w = vec![];
        let op = |res: u32, me: &mut World| {
            if h % 9 == 0 {
                return 0;
            }
            me.next_op += 1;
            me.ops.insert(me.next_op, res);
            me.next_op
        };
        let v = match f {
            strm::FS_QUEUE_OPEN => op(if h % 7 == 0 { 0 } else { 0x0200_0000 | (h >> 8) % 0x100 }, self),
            strm::FS_QUEUE_CLOSE => op(1, self),
            strm::FS_QUEUE_READ => {
                let (at, to, n) = (a[1] as usize, a[2], a[3] as usize);
                let end = self.file.len().min(at.saturating_add(n));
                let got = end.saturating_sub(at);
                if got > 0 {
                    w.push((to, self.file[at..end].to_vec()));
                }
                let got = if h % 11 == 0 { got.saturating_sub(4) } else { got };
                op(got as u32, self)
            }
            strm::FS_GET_RESULT => self.ops.remove(&a[0]).unwrap_or(0),
            strm::THREAD_IS_CURRENT => h % 2,
            strm::POLL | strm::THREAD_SLEEP_MS => {
                if h % 3 == 0 {
                    w.push((ST + 0x34, vec![0; 4]));
                }
                0
            }
            _ => 0,
        };
        (v, w)
    }
}

struct Env {
    w: World,
    log: Vec<Call>,
}

impl StrmEnv for Env {
    fn call(&mut self, m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        let (v, writes) = self.w.answer(f, args);
        for (a, b) in writes {
            for (k, x) in b.iter().enumerate() {
                m.w8(a + k as u32, *x);
            }
        }
        self.log.push(Call(f, args.to_vec()));
        v
    }
}

#[test]
fn the_streams_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let funcs = [
        0x2c63d0, 0x2c6668, 0x2c6890, 0x2c68d0, 0x2c6928, 0x2c6a28, 0x2c6b88, 0x2c6e70, 0x2c7098, 0x2c7178, 0x2c73d0, 0x2c74c8, 0x2c75d8, 0x2c7858,
        0x2c7a28, 0x2c7b30, 0x2c7d20, 0x2c7de8,
    ];
    // the stream library's other calls, the C library, the copy
    let others = [
        0x2c6358, 0x2c6390, 0x2c6e48, 0x2c72e0, 0x2c7270, 0x2c6460, 0x2c64e0, 0x2c6558, 0x2c65a0, 0x2c6610, 0x2fd008, 0x2fd14c, 0x2fd630, 0x2fb468, 0x2ce114,
    ];
    for f in funcs.into_iter().chain(others) {
        r.allow(f);
    }
    let world = Rc::new(RefCell::new(World { seed: 0, n: 0, file: Rc::default(), ops: HashMap::new(), next_op: 0 }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let outs = [
        (strm::MUTEX_INIT, 1),
        (strm::MUTEX_DESTROY, 1),
        (strm::MUTEX_LOCK, 1),
        (strm::MUTEX_UNLOCK, 1),
        (strm::FS_SET_PRIORITY, 2),
        (strm::FS_GET_RESULT, 1),
        (strm::FS_QUEUE_OPEN, 4),
        (strm::FS_QUEUE_CLOSE, 3),
        (strm::FS_QUEUE_READ, 6),
        (strm::FS_SET_CALLBACK, 2),
        (strm::FILE_CLOSE_SYNC, 2),
        (strm::THREAD_IS_CURRENT, 1),
        (strm::THREAD_SLEEP_MS, 1),
        (strm::POLL, 1),
    ];
    for (f, n) in outs {
        let (world, log) = (world.clone(), log.clone());
        r.stub(f, move |c, m| {
            let a: Vec<u32> = (4..4 + n).map(|k| c.gpr(k) as u32).collect();
            let (v, writes) = world.borrow_mut().answer(f, &a);
            for (at, b) in writes {
                m.write(at, &b).unwrap();
            }
            log.borrow_mut().push(Call(f, a));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }

    let mut g = Rng(0x5747_ab00_0000_0001);
    let mut hits = std::collections::BTreeMap::<&str, u32>::new();
    for seq in 0..600 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let (ns, nf) = (40 + g.below(80) as usize, 200 + g.below(800) as usize);
        let (src, src_at) = packets(&mut g, ns);
        let (file, file_at) = packets(&mut g, nf);
        let mut names = vec![0u8; 0x200];
        names[..5].copy_from_slice(b"a.vid");
        names[0x100..0x105].copy_from_slice(b"b.vid");
        let mut srcv = src.clone();
        srcv.resize(0x40000, 0);
        let mut m = Mem { regions: vec![(ST, rnd(&mut g, ST_SIZE)), (SRC, srcv), (NAMES, names), (GLOBALS.0, rnd(&mut g, GLOBALS.1))] };
        // (the request numbers go up in 0x100s: the low byte is the request's place)
        m.w32(strm::REQUEST_NUMBER, g.next() & !0xff);
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let w0 = World { seed: g.next(), n: 0, file: Rc::new(file), ops: HashMap::new(), next_op: 0 };
        *world.borrow_mut() = w0.clone();
        log.borrow_mut().clear();
        let mut env = Env { w: w0, log: vec![] };
        // the file system operation whose callback is set and not yet called
        let mut pending: Option<(u32, u32)> = None;
        let mut ids: Vec<u32> = vec![];
        // packets taken (the game's callers give back only those)
        let mut taken: Vec<(u32, u32)> = vec![];
        let mut handle = 0;
        let mut closed = false;
        for step in 0..80 {
            let st = ST;
            let chunks = if handle != 0 { m.r32(st + 0x24).min(16) } else { 0 };
            let chunk = |g: &mut Rng| if chunks == 0 { handle } else { m.r32(st + 0x20) + 16 * g.below(chunks) };
            let pick = if step == 0 { 100 } else if step == 1 { 101 } else { g.below(24) };
            if std::env::var("SDBG").is_ok() { eprintln!("seq {seq} step {step} pick {pick}"); }
            let (f, args, ours): (u32, Vec<u32>, Option<u32>) = match pick {
                100 => {
                    let nreq = [2, 3, 5, 8, 1, 0x101][g.below(6).min(4) as usize + if g.below(10) == 0 { 1 } else { 0 }];
                    let ntypes = 1 + g.below(6);
                    let nchunks = 1 + g.below(ntypes);
                    let need = strm::calc_mem_size(nreq, ntypes, nchunks);
                    let size = if g.below(10) == 0 { need + 0x17000 } else { need + 0x18000 + g.below(0x10000) };
                    let h = strm::create(&mut m, &mut env, nreq, ntypes, nchunks, st, size.min(ST_SIZE));
                    handle = h;
                    (0x2c6e70, vec![nreq, ntypes, nchunks, st, size.min(ST_SIZE)], Some(h))
                }
                101 => {
                    // the types: 1..: top byte k to chunk k (or none), the last catch-all
                    let types = if handle != 0 { m.r32(st + 0x1c) } else { 0 };
                    let mut args = vec![];
                    for k in 1..types {
                        let c = if g.below(5) == 0 { [-1i32 as u32, -2i32 as u32][g.below(2) as usize] } else { 1 + g.below(chunks) };
                        let a = vec![handle, k, 0xff00_0000, k << 24, c];
                        strm::set_playlist_entry(&mut m, handle, k, 0xff00_0000, k << 24, c);
                        for (j, x) in a.iter().enumerate() {
                            r.cpu.set_gpr(4 + j, *x as i32 as i64 as u64);
                        }
                        r.call(0x2c7098).unwrap();
                        args = a;
                    }
                    (0x2c7098, if args.is_empty() { vec![handle, 0, 0, 0, 1] } else { args }, None)
                }
                _ if handle == 0 || closed => continue,
                0..=3 => {
                    let k = g.below(src_at.len() as u32 / 2) as usize;
                    let at = SRC + src_at[k];
                    let size = if g.below(3) == 0 {
                        0
                    } else {
                        let j = k + 1 + g.below((src_at.len() - k - 1) as u32) as usize;
                        src_at[j] - src_at[k]
                    };
                    let id = strm::queue(&mut m, &mut env, handle, at, size, END);
                    if id != 0 {
                        ids.push(id);
                    }
                    (0x2c74c8, vec![handle, at, size, END], Some(id))
                }
                4 | 5 => {
                    let name = NAMES + 0x100 * g.below(2);
                    let start = file_at[g.below(file_at.len() as u32 / 2) as usize];
                    let id = strm::queue_file(&mut m, &mut env, handle, name, start, END);
                    if id != 0 {
                        ids.push(id);
                    }
                    (0x2c73d0, vec![handle, name, start, END], Some(id))
                }
                6..=9 => {
                    // the file system's operation ends
                    let Some((_, cb)) = pending.take() else { continue };
                    match cb {
                        strm::ON_READ_DONE => strm::on_read_done(&mut m, &mut env, st),
                        strm::OPEN_AFTER_CLOSE => strm::open_after_close(&mut m, &mut env, st),
                        _ => strm::on_data_read(&mut m, &mut env, st),
                    }
                    (cb, vec![0, 1, st], None)
                }
                10..=13 => {
                    let c = chunk(&mut g);
                    let p = strm::get_packet(&mut m, &mut env, c);
                    *hits.entry("packets").or_default() += (p != 0) as u32;
                    if p != 0 {
                        taken.push((c, p));
                    }
                    (0x2c7a28, vec![c], Some(p))
                }
                14 | 15 => {
                    // a packet given back: one taken, or one outside the buffer
                    let (c, p) = if !taken.is_empty() && g.below(6) != 0 {
                        let k = g.below(taken.len() as u32) as usize;
                        // (once only: once the reader is past it, its bytes are someone else's)
                        taken.remove(k)
                    } else {
                        (chunk(&mut g), [st, st + 0x28 - 4, m.r32(st + 0x30) - 4][g.below(3) as usize])
                    };
                    strm::release_packet(&mut m, &mut env, c, p);
                    (0x2c7b30, vec![c, p], None)
                }
                16 if !ids.is_empty() => {
                    let id = ids[g.below(ids.len() as u32) as usize];
                    strm::purge(&mut m, &mut env, handle, id);
                    (0x2c75d8, vec![handle, id], None)
                }
                17 if g.below(3) == 0 => {
                    // (with every request from the last back queued, the game reads near 0)
                    let mut ok = m.r32(st + 0x60) == 0;
                    let mut q = m.r32(st + 0x60);
                    while q != 0 && !ok {
                        ok = m.r32(q + 4) != 1;
                        q = m.r32(q + 8);
                    }
                    if !ok {
                        continue;
                    }
                    strm::stop(&mut m, &mut env, handle);
                    // (a stop uses up the packets taken too: their bytes are free again)
                    taken.clear();
                    (0x2c7858, vec![handle], None)
                }
                18 => {
                    let c = chunk(&mut g);
                    (0x2c7d20, vec![c], Some(strm::find_buffered_request(&mut m, &mut env, c)))
                }
                19 => {
                    let id = if !ids.is_empty() && g.below(4) != 0 { ids[g.below(ids.len() as u32) as usize] } else { g.below(8) };
                    (0x2c7de8, vec![handle, id], Some(strm::get_request_state(&m, handle, id)))
                }
                22 => {
                    // a low-water mark (the stream turns urgent below it)
                    let mark = g.below(0x20000);
                    strm::set_low_water(&mut m, &mut env, handle, mark);
                    (0x2c7270, vec![handle, mark], None)
                }
                21 => {
                    // a type set later (refused while reading; the last type only catch-all)
                    let types = m.r32(st + 0x1c);
                    let k = 1 + g.below(types.max(1));
                    let (mask, value) = match g.below(3) {
                        0 => (0, 0),
                        1 => (0, k << 24),
                        _ => (0xff00_0000, k << 24),
                    };
                    let c = 1 + g.below(chunks.max(1));
                    strm::set_playlist_entry(&mut m, handle, k, mask, value, c);
                    (0x2c7098, vec![handle, k, mask, value, c], None)
                }
                20 if step > 70 && g.below(3) == 0 => {
                    closed = true;
                    strm::close(&mut m, &mut env, handle);
                    (0x2c7178, vec![handle], None)
                }
                _ => continue,
            };
            if pick != 101 {
                for (k, a) in args.iter().enumerate() {
                    r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
                }
                r.call(f).unwrap_or_else(|e| panic!("seq {seq} step {step}: {f:#x} {args:x?}: {e}"));
            }
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                if &t != v {
                    let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
                    panic!("seq {seq} step {step}: {f:#x} {args:x?}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
                }
            }
            assert_eq!(*log.borrow(), env.log, "seq {seq} step {step}: {f:#x} {args:x?}: the calls out");
            if let Some(v) = ours {
                assert_eq!(r.cpu.gpr(2) as u32, v, "seq {seq} step {step}: {f:#x} {args:x?}: the answer");
            }
            // the last callback set by this call (an operation that ends later)
            let calls = env.log.len();
            for Call(fun, a) in env.log[..calls].iter().rev() {
                if *fun == strm::FS_SET_CALLBACK {
                    if pending.map_or(true, |(op, _)| op != a[0]) {
                        pending = Some((a[0], a[1]));
                    }
                    break;
                }
            }
            env.log.clear();
            log.borrow_mut().clear();
            *hits.entry("calls").or_default() += 1;
        }
        *hits.entry("streams").or_default() += (handle != 0) as u32;
    }
    println!("{hits:?}");
}
