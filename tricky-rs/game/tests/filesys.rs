//! `tricky_game::filesys` (board row E2v) against the original in the function runner (TRICKY_ELF; skipped
//! without it): each run makes the file system (`FileSys_Init` / `InitWithBuffer` on a small pool) and then makes
//! random calls of every function: opens, reads, writes, closes, sizes, mounts and unmounts of archives (their
//! directories made-up `C0FB` / `BIGF` ones, some longer than the first read, some not archives), callbacks,
//! priorities, cancels, waits, results, the root path and the path rules. The lock, the devices, the heap, the
//! poll and the callbacks not the file system's own are answered the same on both sides and logged. Memory (all
//! but the stack), answers and the calls out are compared after every call.
//!
//! Calls the game would hang or crash on are not made: an operation or a handle asked for with none free (the
//! game spins), a result taken of an operation still queued (its slot used again while queued loops the queue),
//! an unmount of an archive not mounted (the game reads near 0), a read with no handle. (A result may be taken
//! of an operation still in the queue only if it has not ended: one the poll ends while it runs.)

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::filesys::{self as fs, FsEnv};
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

const STACK_TOP: u32 = 0x01ff_0000;
const STACK: u32 = STACK_TOP - 0x1000;
const S_REGION: (u32, u32) = (0x34eec0, 0x340);
const DISC_REGION: (u32, u32) = (0x349d40, 0x10);
const GLOBALS: (u32, u32) = (0x3bb900, 0x200);
const POOL: u32 = 0x0100_0000;
const POOL_SIZE: u32 = 0x18000;
const PATHS: u32 = 0x0104_0000;
const DST: u32 = 0x0105_0000;
const BUFS: u32 = 0x0110_0000;
const SLOT: u32 = 0x1000;
const SLOTS: u32 = 48;
const ALLOC_FN: u32 = 0x0170_0000;
const FREE_FN: u32 = 0x0170_0100;
const CB_A: u32 = 0x0170_0200;
const CB_B: u32 = 0x0170_0300;
const RUN_FN: u32 = 0x0170_0400;

const PATH_LIST: &[&[u8]] = &[
    b"a",
    b"b.dat",
    b"arch1",
    b"arch2",
    b"arch3",
    b"arch1|a",
    b"arch2|b.dat",
    b"arch3|zz",
    b"|b.dat",
    b"|zz",
    b"|A",
    b"nope|a",
    b"zz",
    b"",
    b"cd:\\data\\",
    b"cd:x/",
    b"host0:",
    b"host0:dir",
    b"cd:",
    b"hos",
];
const NAMES: &[&[u8]] = &[b"a", b"b.dat", b"zz", b"A", b"c/d.ssh", b"x"];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

#[derive(Clone)]
struct World {
    seed: u32,
    n: u32,
    slot: u32,
    /// the operation `FileSys_WaitOp` waits on
    wait: u32,
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32], rd: &mut dyn FnMut(u32) -> u32) -> (u32, Vec<(u32, u32)>) {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        let mut w = vec![];
        let v = match f {
            fs::MUTEX_INIT | fs::MUTEX_LOCK | fs::MUTEX_UNLOCK | FREE_FN => 0,
            fs::POLL => {
                let t = self.wait;
                if t != 0 {
                    if rd(fs::S + 0x20) == t {
                        if h % 3 == 0 {
                            w.push((t + 8, [1, -2i32 as u32, 7, -5i32 as u32, -1i32 as u32][(h >> 4) as usize % 5]));
                        }
                    } else if h % 5 == 0 {
                        w.push((t, rd(t) ^ 1));
                    }
                }
                0
            }
            fs::OPEN_ON_DISC => {
                if h % 3 == 0 {
                    0
                } else {
                    w.push((a[2], 0x100 + (h >> 8) % 0x100));
                    1
                }
            }
            fs::DEV_GET_SIZE => (h >> 4) % 0x3000,
            fs::HOSTFS_CLOSE => [0, 0, 1, -1i32 as u32][(h % 4) as usize],
            ALLOC_FN => {
                if a[0] == fs::NAME_FILE_SYS {
                    POOL
                } else {
                    self.slot += 1;
                    BUFS + SLOT * ((self.slot - 1) % SLOTS)
                }
            }
            _ => h,
        };
        (v, w)
    }
}

struct Env {
    w: World,
    log: Vec<Call>,
}

impl FsEnv for Env {
    fn call(&mut self, m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        let (v, writes) = self.w.answer(f, args, &mut |a| m.r32(a));
        for (a, x) in writes {
            m.w32(a, x);
        }
        self.log.push(Call(f, args.to_vec()));
        v
    }
}

/// A made-up archive directory (or not one), in a slot
fn slot_content(g: &mut Rng) -> Vec<u8> {
    let mut v: Vec<u8> = (0..SLOT).map(|_| g.next() as u8).collect();
    let kind = g.below(4);
    if kind == 0 {
        v[0] = 0;
        return v;
    }
    let bigf = g.below(2) == 0;
    let mut d = vec![];
    for _ in 0..g.below(6) {
        let (off, size) = (g.next() % 0x100000, g.next() % 0x10000);
        let w = if bigf { 4 } else { 3 };
        d.extend(&off.to_be_bytes()[4 - w..]);
        d.extend(&size.to_be_bytes()[4 - w..]);
        d.extend(NAMES[g.below(NAMES.len() as u32) as usize]);
        d.push(0);
    }
    let start = if bigf { 0x10 } else { 6 };
    let mut size = start + d.len() as u32;
    if kind == 3 {
        // longer than the first read, or as long
        size = if g.below(4) == 0 { 0x800 } else { 0x800 + g.below(0x600) };
    } else if kind == 2 && !d.is_empty() {
        // the directory ends inside its last name
        size -= 1 + g.below(2);
    }
    let mut head = vec![];
    if bigf {
        head.extend(b"BIGF");
        head.extend(&g.next().to_le_bytes());
        head.extend(&(d.len() as u32).to_be_bytes());
        head.extend(&size.to_be_bytes());
    } else {
        head.extend([0xc0, 0xfb]);
        head.extend(&((size - 4) as u16).to_be_bytes());
        head.extend(&(g.below(6) as u16).to_be_bytes());
    }
    head.extend(&d);
    v[..head.len()].copy_from_slice(&head);
    // zeros from the directory's end, as padding
    for b in &mut v[head.len()..(size as usize + 0x10).max(head.len())] {
        *b = 0;
    }
    v
}

fn ops(m: &Mem) -> Vec<u32> {
    let n = m.r32(fs::S).min(0x40);
    (0..n).map(|i| m.r32(fs::S + 0x24) + 0x30 * i).collect()
}

fn handles(m: &Mem) -> Vec<u32> {
    let n = m.r32(fs::S + 4).min(0x40);
    (0..n).map(|i| m.r32(fs::S + 0x28) + 0x10c * i).collect()
}

fn queued(m: &Mem, op: u32) -> bool {
    let mut q = m.r32(fs::S + 0x2c);
    let mut k = 0;
    while q != 0 && k < 0x100 {
        if q == op {
            return true;
        }
        q = m.r32(q + 0x2c);
        k += 1;
    }
    false
}

fn bigs(m: &Mem) -> Vec<u32> {
    let mut v = vec![];
    let mut b = m.r32(fs::S + 0x30);
    while b != 0 && v.len() < 0x40 {
        v.push(m.r32(b + 8));
        b = m.r32(b + 0x10);
    }
    v
}

#[test]
fn the_file_system_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let funcs = [
        0x2ca170, 0x2ca208, 0x2ca320, 0x2ca368, 0x2ca3b0, 0x2ca3e0, 0x2ca410, 0x2ca460, 0x2ca5c0, 0x2ca628, 0x2ca6f8, 0x2ca7f8, 0x2ca8a8, 0x2ca960,
        0x2caa08, 0x2caae0, 0x2caba0, 0x2cac38, 0x2caca8, 0x2cadc0, 0x2cae70, 0x2cafa8, 0x2cafd8, 0x2cb190, 0x2cb1f0, 0x2cb218, 0x2cb300, 0x2cb328,
        0x2cb780, 0x2cb830, 0x2cb9c8, 0x2cb9d0, 0x2cbb18, 0x2cbb68, 0x2cbc28, 0x2cc328, 0x2cc348,
    ];
    // the BIG library, the C library, the fill
    for f in funcs.into_iter().chain([0x2cbc88, 0x2cbd08, 0x2cbdc0, 0x2fcc98, 0x2fd260, 0x2fd630, 0x2fd478, 0x2fce78, 0x2fd14c, 0x2fd008, 0x2fcd4c, 0x2cc808]) {
        r.allow(f);
    }
    let world = Rc::new(RefCell::new(World { seed: 0, n: 0, slot: 0, wait: 0 }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let outs = [
        (fs::MUTEX_INIT, 1),
        (fs::MUTEX_LOCK, 1),
        (fs::MUTEX_UNLOCK, 1),
        (fs::POLL, 1),
        (fs::CDFS_MOUNT, 3),
        (fs::HOSTFS_INIT, 2),
        (fs::OPEN_ON_DISC, 3),
        (fs::HOSTFS_CLOSE, 1),
        (fs::DEV_READ, 4),
        (fs::HOSTFS_WRITE, 4),
        (fs::DEV_GET_SIZE, 1),
        (ALLOC_FN, 3),
        (FREE_FN, 1),
        (CB_A, 3),
        (CB_B, 3),
        (RUN_FN, 2),
    ];
    for (f, n) in outs {
        let (world, log) = (world.clone(), log.clone());
        r.stub(f, move |c, m| {
            let a: Vec<u32> = (4..4 + n).map(|k| c.gpr(k) as u32).collect();
            let (v, writes) = world.borrow_mut().answer(f, &a, &mut |at| {
                let mut b = [0u8; 4];
                m.read(at, &mut b).unwrap();
                u32::from_le_bytes(b)
            });
            for (at, x) in writes {
                m.write(at, &x.to_le_bytes()).unwrap();
            }
            log.borrow_mut().push(Call(f, a));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut globals = vec![0u8; GLOBALS.1 as usize];
    r.mem.read(GLOBALS.0, &mut globals).unwrap();

    let mut g = Rng(0xf11e_5000_0000_0001);
    let mut hits = std::collections::BTreeMap::<&str, u32>::new();
    for seq in 0..1500 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let mut bufs = vec![];
        for _ in 0..SLOTS {
            bufs.extend(slot_content(&mut g));
        }
        let mut paths = vec![0u8; 0x200 * (PATH_LIST.len() + 1)];
        for (k, p) in PATH_LIST.iter().enumerate() {
            paths[0x200 * k..0x200 * k + p.len()].copy_from_slice(p);
        }
        // a root longer than its 0x100 bytes
        let long = PATH_LIST.len() * 0x200;
        for b in &mut paths[long..long + 0x108] {
            *b = b'r';
        }
        let mut s = vec![0u8; S_REGION.1 as usize];
        if g.below(2) == 0 {
            s[0x38..0x138].copy_from_slice(&rnd(&mut g, 0x100));
        }
        let mut m = Mem {
            regions: vec![
                (S_REGION.0, s),
                (DISC_REGION.0, vec![0; DISC_REGION.1 as usize]),
                (GLOBALS.0, globals.clone()),
                (POOL, rnd(&mut g, POOL_SIZE)),
                (PATHS, paths),
                (DST, rnd(&mut g, 0x400)),
                (BUFS, bufs),
                (STACK, vec![0; 0x1000]),
                // a failed mount gives back handle 0: the game zeroes 0x10c bytes at 0
                (0, rnd(&mut g, 0x200)),
            ],
        };
        m.w32(fs::ALLOC_PTR, ALLOC_FN);
        m.w32(fs::FREE_PTR, FREE_FN);
        m.w32(fs::OP_NUMBER, if g.below(3) == 0 { 0xffff0 + g.below(0x10) } else { g.below(0x100) });
        m.w32(fs::BIG_FLAGS, g.next());
        m.w32(fs::DEVICES, g.below(4));
        m.w32(fs::HAS_DISC, g.below(2));
        m.w32(fs::DISC_ENTRY, g.below(0x40));
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let w0 = World { seed: g.next(), n: 0, slot: 0, wait: 0 };
        *world.borrow_mut() = w0.clone();
        log.borrow_mut().clear();
        let mut env = Env { w: w0, log: vec![] };
        let mut ids: Vec<u32> = vec![];
        for step in 0..100 {
            let st = rnd(&mut g, 0x1000);
            r.mem.write(STACK, &st).unwrap();
            m.regions[7].1 = st;
            let path = |g: &mut Rng| PATHS + 0x200 * g.below(PATH_LIST.len() as u32);
            let prio = if g.below(4) == 0 { g.next() } else { g.below(8) };
            let ctx = g.next();
            let (x, y) = (g.next(), g.next());
            let free_ops = ops(&m).iter().filter(|&&o| m.r32(o) >> 20 & 0xf == 0).count();
            let free_handles = handles(&m).iter().filter(|&&h| m.r32(h) == 0).count();
            let room = free_ops >= 5 && free_handles >= 2 && env.w.slot + 4 < SLOTS;
            let hs = handles(&m);
            // a handle: any, one of a file in an archive, or an archive's
            let inside: Vec<u32> = hs.iter().copied().filter(|&h| m.r32(h + 8) != 0).collect();
            let archives = bigs(&m);
            let handle = |g: &mut Rng| match g.below(4) {
                0 if !inside.is_empty() => inside[g.below(inside.len() as u32) as usize],
                1 if !archives.is_empty() => archives[g.below(archives.len() as u32) as usize],
                _ => hs[g.below(hs.len() as u32) as usize],
            };
            let id = |g: &mut Rng| {
                if !ids.is_empty() && g.below(6) != 0 {
                    ids[g.below(ids.len() as u32) as usize]
                } else {
                    let o = ops(&m);
                    let op = o[g.below(o.len() as u32) as usize];
                    m.r32(op) ^ if g.below(4) == 0 { 1 } else { 0 }
                }
            };
            let op_of = |m: &Mem, id: u32| m.r32(fs::S + 0x24) + (id >> 24) * 0x30;
            let pick = if step == 0 || g.below(50) == 0 { 100 } else { g.below(43) };
            let (f, args, ours, name): (u32, Vec<u32>, u32, &str) = match pick {
                100 => {
                    let h = if g.below(5) == 0 { 0 } else { 1 + g.below(6) };
                    let n = if g.below(3) == 0 { 0 } else { 1 + g.below(0x20) };
                    let o = if g.below(5) == 0 { 0 } else { 5 + g.below(8) };
                    if g.below(2) == 0 {
                        (0x2ca170, vec![h, n, o], fs::init(&mut m, &mut env, h, n, o), "init")
                    } else {
                        (0x2ca208, vec![h, n, o, POOL], fs::init_with_buffer(&mut m, &mut env, h, n, o, POOL), "init with buffer")
                    }
                }
                0 | 1 | 2 if room => {
                    let (p, mode) = (path(&mut g), g.below(4));
                    let ours = fs::queue_open(&mut m, &mut env, p, mode, prio, ctx);
                    ids.push(ours);
                    (0x2ca8a8, vec![p, mode, prio, ctx], ours, "queue open")
                }
                3 if room => {
                    let p = path(&mut g);
                    let ours = fs::open(&mut m, &mut env, p, prio, ctx);
                    ids.push(ours);
                    (0x2ca7f8, vec![p, prio, ctx], ours, "open")
                }
                4 | 5 if room => {
                    // (an archive mounted from inside another: its close clears the error its queueing set)
                    let nested: Vec<u32> = bigs(&m).into_iter().filter(|&h| m.r32(h + 8) != 0).collect();
                    let h = if !nested.is_empty() && g.below(2) == 0 { nested[0] } else { handle(&mut g) };
                    let ours = fs::queue_close(&mut m, &mut env, h, prio, ctx);
                    ids.push(ours);
                    (0x2ca960, vec![h, prio, ctx], ours, "queue close")
                }
                6 | 7 if room => {
                    let h = handle(&mut g);
                    let (pos, n) = (g.below(0x2000), if g.below(4) == 0 { g.next() } else { g.below(0x2000) });
                    let ours = fs::queue_read(&mut m, &mut env, h, pos, DST, n, prio, ctx);
                    ids.push(ours);
                    (0x2caa08, vec![h, pos, DST, n, prio, ctx], ours, "queue read")
                }
                8 if room => {
                    let h = handle(&mut g);
                    let ours = fs::queue_write(&mut m, &mut env, h, x, DST, y, prio, ctx);
                    ids.push(ours);
                    (0x2caae0, vec![h, x, DST, y, prio, ctx], ours, "queue write")
                }
                9 if room => {
                    let h = if g.below(6) == 0 { 0 } else { handle(&mut g) };
                    let ours = fs::get_file_size(&mut m, &mut env, h, prio, ctx);
                    ids.push(ours);
                    (0x2caba0, vec![h, prio, ctx], ours, "get file size")
                }
                10 if room => {
                    let ours = fs::queue_op7(&mut m, &mut env, prio, ctx);
                    ids.push(ours);
                    (0x2cac38, vec![x, prio, ctx], ours, "queue op 7")
                }
                11..=13 | 40..=42 if room => {
                    let p = PATHS + 0x200 * (2 + g.below(4));
                    let ours = fs::queue_mount_big(&mut m, &mut env, p, x, prio, ctx);
                    if ours >> 24 != 1 {
                        ids.push(ours);
                    }
                    (0x2cae70, vec![p, x, prio, ctx], ours, "mount")
                }
                14 | 34 | 35 if room && !bigs(&m).is_empty() => {
                    let b = bigs(&m);
                    let h = b[g.below(b.len() as u32) as usize];
                    let ours = fs::queue_unmount_big(&mut m, &mut env, h, prio, ctx);
                    ids.push(ours);
                    (0x2cafd8, vec![h, prio, ctx], ours, "unmount")
                }
                15 => {
                    let (i, cb) = (id(&mut g), if g.below(2) == 0 { CB_A } else { CB_B });
                    fs::set_callback(&mut m, &mut env, i, cb);
                    (0x2ca410, vec![i, cb], 0, "set callback")
                }
                16 => {
                    // often an operation in the queue, at its own priority or another's there
                    let mut q = vec![];
                    let mut at = m.r32(fs::S + 0x2c);
                    while at != 0 && q.len() < 0x40 {
                        q.push(at);
                        at = m.r32(at + 0x2c);
                    }
                    let (i, prio) = if !q.is_empty() && g.below(2) == 0 {
                        let op = q[g.below(q.len() as u32) as usize];
                        let other = q[g.below(q.len() as u32) as usize];
                        (m.r32(op), m.r32(if g.below(2) == 0 { op } else { other } + 0x10))
                    } else {
                        (id(&mut g), prio)
                    };
                    let op = op_of(&m, i);
                    if i & 0xfffff == m.r32(op) & 0xfffff && !queued(&m, op) && m.r32(op + 8) == 0 && op != m.r32(fs::S + 0x20) {
                        continue;
                    }
                    fs::set_priority(&mut m, &mut env, i, prio);
                    (0x2ca460, vec![i, prio], 0, "set priority")
                }
                17 => {
                    let i = id(&mut g);
                    fs::cancel_op(&mut m, i);
                    (0x2ca5c0, vec![i], 0, "cancel")
                }
                18 | 19 => {
                    let i = id(&mut g);
                    let op = op_of(&m, i);
                    env.w.wait = op;
                    world.borrow_mut().wait = op;
                    (0x2ca628, vec![i], fs::wait_op(&mut m, &mut env, i), "wait")
                }
                20..=22 => {
                    let i = id(&mut g);
                    let op = op_of(&m, i);
                    if m.r32(op + 8) == 0 || queued(&m, op) {
                        continue;
                    }
                    ids.retain(|&k| k >> 24 != i >> 24);
                    if g.below(4) == 0 {
                        // an end a device could give
                        let v = [-3i32, -5, 2][g.below(3) as usize] as u32;
                        m.w32(op + 8, v);
                        r.mem.write_u32(op + 8, v);
                    }
                    (0x2ca6f8, vec![i], fs::get_result(&mut m, &mut env, i), "get result")
                }
                23 => {
                    let i = id(&mut g);
                    match g.below(3) {
                        0 => (0x2ca368, vec![i], fs::get_status(&m, i), "status"),
                        1 => (0x2ca3b0, vec![i], fs::get_op_state(&m, i), "op state"),
                        _ => (0x2ca3e0, vec![i], fs::get_op_context(&m, i), "op context"),
                    }
                }
                24..=26 | 36..=39 => {
                    if m.r32(fs::S + 0x20) == 0 && g.below(8) != 0 {
                        continue;
                    }
                    let ok = g.below(3).min(1) * if g.below(4) == 0 { x } else { 1 };
                    fs::complete_current(&mut m, &mut env, ok);
                    (0x2cb780, vec![ok], 0, "complete")
                }
                27 => {
                    let level = if g.below(2) == 0 { 0xff } else { g.below(8) };
                    (0x2cb190, vec![RUN_FN, x, level, y], fs::run_with_heap(&mut m, &mut env, RUN_FN, level, y), "run with heap")
                }
                28 => {
                    let level = if g.below(2) == 0 { 0xff } else { g.below(8) };
                    fs::set_heap(&mut m, &mut env, level);
                    (0x2cb1f0, vec![level], 0, "set heap")
                }
                29 => {
                    let p = if g.below(5) == 0 { PATHS + long as u32 } else { path(&mut g) };
                    let f = if g.below(2) == 0 { 0x2cb218 } else { 0x2cc328 };
                    (f, vec![p], fs::set_root_path(&mut m, p), "set root")
                }
                30 => {
                    let f = if g.below(2) == 0 { 0x2cb300 } else { 0x2cc348 };
                    (f, vec![DST], fs::get_root_path(&mut m, DST), "get root")
                }
                31 | 32 => {
                    let (p, mode) = (path(&mut g), g.below(4));
                    let out = DST + 0x200;
                    (0x2cb830, vec![p, mode, out], fs::parse_path(&mut m, &mut env, STACK_TOP, p, mode, out), "parse path")
                }
                33 => match g.below(5) {
                    0 if free_ops > 0 => (0x2cb9d0, vec![], fs::alloc_op(&mut m, &mut env), "alloc op"),
                    1 if free_handles > 0 => (0x2cbb68, vec![], fs::alloc_handle(&mut m, &mut env), "alloc handle"),
                    2 => {
                        let h = handle(&mut g);
                        fs::free_handle(&mut m, &mut env, h);
                        (0x2cbc28, vec![h], 0, "free handle")
                    }
                    3 => (0x2ca320, vec![x % 8, y % 0x40, ctx % 0x40], fs::calc_mem_size(&m, x % 8, y % 0x40, ctx % 0x40), "calc size"),
                    _ => {
                        let o = ops(&m);
                        let op = o[g.below(o.len() as u32) as usize];
                        if (m.r32(op + 8) == 0 && m.r32(op) >> 20 & 0xf != 1) || queued(&m, op) {
                            continue;
                        }
                        fs::free_op(&mut m, &mut env, op);
                        (0x2cbb18, vec![op], 0, "free op")
                    }
                },
                _ => continue,
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
                    panic!("seq {seq} step {step}: {name} {f:#x} {args:x?}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
                }
            }
            assert_eq!(*log.borrow(), env.log, "seq {seq} step {step}: {name} {f:#x} {args:x?}: the calls out");
            // (the functions with no answer leave whatever their last call did)
            if ![0x2ca410, 0x2ca460, 0x2ca5c0, 0x2cb780, 0x2cb1f0, 0x2cbc28, 0x2cbb18].contains(&f) {
                assert_eq!(r.cpu.gpr(2) as u32, ours, "seq {seq} step {step}: {name} {f:#x} {args:x?}: the answer");
            }
            *hits.entry(name).or_default() += 1;
        }
        *hits.entry("archives mounted").or_default() += bigs(&m).len() as u32;
        *hits.entry("archives in archives").or_default() += bigs(&m).iter().filter(|&&h| m.r32(h + 8) != 0).count() as u32;
    }
    println!("calls: {hits:?}");
}
