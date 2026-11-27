//! The file system's devices in `tricky_game::filesys` (board row E2w) against the original in the function runner
//! (TRICKY_ELF; skipped without it): random calls of the device functions on random paths, roots, devices, disc
//! tables, handles and host read requests. The kernel's file calls, the disc's find and read, the poll list, the
//! lock and the running operation's callback are answered the same on both sides and logged. Memory (all but the
//! stack, which is filled the same on both sides), answers and the calls out are compared after every call.

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
const S_REGION: (u32, u32) = (0x34eec0, 0x140);
const DISC_REGION: (u32, u32) = (0x349d40, 0x60);
const GLOBALS: (u32, u32) = (0x3bb900, 0x200);
const REQUEST: (u32, u32) = (0x4039e0, 0x40);
const POOL: u32 = 0x0100_0000;
const PATHS: u32 = 0x0104_0000;
const DST: u32 = 0x0105_0000;
const TABLE: u32 = 0x0106_0000;
const CB_A: u32 = 0x0170_0200;

const PATH_LIST: &[&[u8]] = &[
    b"cd:\\data\\x.big",
    b"cd:a",
    b"host0:dir\\f",
    b"host0:",
    b"/abs/f",
    b"\\abs\\f",
    b"~x",
    b"rel\\a\\b",
    b"x:y",
    b"ab:c",
    b"a:b",
    b"",
    b"c",
    b"hos",
];
const ROOTS: &[&[u8]] = &[b"data/", b"", b"\\root\\", b"r"];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

#[derive(Clone)]
struct World {
    seed: u32,
    n: u32,
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32]) -> (u32, Vec<(u32, u32)>) {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        let mut w = vec![];
        let v = match f {
            fs::MUTEX_LOCK | fs::MUTEX_UNLOCK => 0,
            fs::CDFS_FIND_FILE => {
                if h % 3 == 0 {
                    0
                } else {
                    w.push((a[2], 1 + (h >> 4) % 8));
                    1
                }
            }
            fs::ADD_POLL_TASK => {
                if h % 3 == 0 {
                    w.push((fs::DEVICES, h >> 8));
                }
                0
            }
            fs::SCE_OPEN => {
                if h % 4 == 0 {
                    -(1 + (h >> 4) as i32 % 20) as u32
                } else {
                    (h >> 4) % 0x40
                }
            }
            fs::SCE_READ => {
                if h % 3 == 0 {
                    h >> 4
                } else {
                    a[2]
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
        let (v, writes) = self.w.answer(f, args);
        for (a, x) in writes {
            m.w32(a, x);
        }
        self.log.push(Call(f, args.to_vec()));
        v
    }
}

#[test]
fn the_devices_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let funcs = [0x2ca0b0, 0x2ca120, 0x2cc2c0, 0x2cc368, 0x2cc4b8, 0x2cc5a8, 0x2cc600, 0x2cc680, 0x2cc730, 0x2cc7a0];
    // the queue (`FileSys_CompleteCurrent`, `RunQueue`), the C library
    for f in funcs.into_iter().chain([0x2cb780, 0x2cb328, 0x2fd630, 0x2fd478, 0x2fce78, 0x2fd260, 0x2fcd4c, 0x2fd14c]) {
        r.allow(f);
    }
    let world = Rc::new(RefCell::new(World { seed: 0, n: 0 }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let outs = [
        (fs::MUTEX_LOCK, 1),
        (fs::MUTEX_UNLOCK, 1),
        (fs::CDFS_FIND_FILE, 3),
        (fs::CDFS_START_READ, 4),
        (fs::ADD_POLL_TASK, 3),
        (fs::SCE_OPEN, 2),
        (fs::SCE_CLOSE, 1),
        (fs::SCE_LSEEK, 3),
        (fs::SCE_READ, 3),
        (fs::SCE_WRITE, 3),
        (CB_A, 3),
    ];
    for (f, n) in outs {
        let (world, log) = (world.clone(), log.clone());
        r.stub(f, move |c, m| {
            let a: Vec<u32> = (4..4 + n).map(|k| c.gpr(k) as u32).collect();
            let (v, writes) = world.borrow_mut().answer(f, &a);
            for (at, x) in writes {
                m.write(at, &x.to_le_bytes()).unwrap();
            }
            log.borrow_mut().push(Call(f, a));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut globals = vec![0u8; GLOBALS.1 as usize];
    r.mem.read(GLOBALS.0, &mut globals).unwrap();

    let mut g = Rng(0xde71_0000_0000_0001);
    let mut hits = std::collections::BTreeMap::<&str, u32>::new();
    for seq in 0..400 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let mut paths = vec![0u8; 0x100 * PATH_LIST.len()];
        for (k, p) in PATH_LIST.iter().enumerate() {
            paths[0x100 * k..0x100 * k + p.len()].copy_from_slice(p);
        }
        let mut s = vec![0u8; S_REGION.1 as usize];
        let root = ROOTS[g.below(ROOTS.len() as u32) as usize];
        s[0x38..0x38 + root.len()].copy_from_slice(root);
        let mut m = Mem {
            regions: vec![
                (S_REGION.0, s),
                (DISC_REGION.0, rnd(&mut g, DISC_REGION.1)),
                (GLOBALS.0, globals.clone()),
                (REQUEST.0, rnd(&mut g, REQUEST.1)),
                (POOL, rnd(&mut g, 0x100)),
                (PATHS, paths),
                (DST, rnd(&mut g, 0x400)),
                (TABLE, rnd(&mut g, 0x400)),
                (STACK, vec![0; 0x1000]),
            ],
        };
        m.w32(fs::S + 8, 0xff);
        m.w32(fs::S + 0x24, POOL);
        m.w32(fs::DISC_ENTRY, 0x10 + g.below(0x20));
        m.w32(fs::DISC_TABLE, TABLE);
        m.w32(fs::ROOT_DEVICE, [1, 2, 0][g.below(3) as usize]);
        m.w32(fs::DEVICES, g.below(4));
        m.w32(fs::HOST_LAST_FD, if g.below(2) == 0 { -1i32 as u32 } else { g.below(4) });
        m.w32(fs::HOST_LAST_POS, g.below(4) * 0x100);
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let w0 = World { seed: g.next(), n: 0 };
        *world.borrow_mut() = w0.clone();
        log.borrow_mut().clear();
        let mut env = Env { w: w0, log: vec![] };
        for step in 0..40 {
            let st = rnd(&mut g, 0x1000);
            r.mem.write(STACK, &st).unwrap();
            m.regions[8].1 = st;
            // the operation running when one completes (its callback logged), or none
            let cur = if g.below(3) == 0 { 0 } else { POOL };
            for (a, v) in [(fs::S + 0x20, cur), (fs::S + 0x10, 0), (fs::S + 0x2c, 0), (POOL + 4, 0), (POOL + 0x28, [0, CB_A][g.below(2) as usize])] {
                m.w32(a, v);
                r.mem.write_u32(a, v);
            }
            if g.below(3) == 0 {
                // a host read waiting
                let req = [1, g.below(4), DST, g.below(4) * 0x100, g.below(0x200)];
                for (k, v) in req.iter().enumerate() {
                    m.w32(fs::HOST_REQUEST + 4 * k as u32, *v);
                    r.mem.write_u32(fs::HOST_REQUEST + 4 * k as u32, *v);
                }
            }
            let path = PATHS + 0x100 * g.below(PATH_LIST.len() as u32);
            // (a disc handle's number names an entry of the table: 1 to 8)
            let dev = if g.below(8) == 0 { [0x11, 0x12, 0xff, 0x81][g.below(4) as usize] } else { g.below(4) };
            let handle = (dev << 24) | (1 + g.below(8)) | if dev != 1 && g.below(8) == 0 { 0x80_0000 } else { 0 };
            let (x, y) = (g.next(), g.below(0x1000));
            let (f, args, ours, name): (u32, Vec<u32>, u32, &str) = match g.below(10) {
                0 => {
                    let (nm, sz) = (if g.below(2) == 0 { 0 } else { DST }, if g.below(2) == 0 { 0 } else { DST + 0x300 });
                    let i = 1 + g.below(8);
                    (0x2ca0b0, vec![i, nm, sz], fs::cdfs_get_file_info(&mut m, i, nm, sz), "file info")
                }
                1 => (0x2ca120, vec![path], fs::cdfs_find_entry(&mut m, &mut env, STACK_TOP, path), "find entry"),
                2 => {
                    let disc = g.below(2);
                    (0x2cc2c0, vec![x, disc], fs::hostfs_init(&mut m, &mut env, disc), "host init")
                }
                3 => {
                    let dev = if g.below(4) == 0 { 0 } else { DST + 0x200 };
                    (0x2cc368, vec![path, DST, dev], fs::make_device_path(&mut m, path, DST, dev), "device path")
                }
                4 => {
                    let mode = g.below(8);
                    let out = DST + 0x380;
                    (0x2cc4b8, vec![path, mode, out], fs::open_on_disc(&mut m, &mut env, STACK_TOP, path, mode, out), "open")
                }
                5 => (0x2cc5a8, vec![handle], fs::hostfs_close(&mut m, &mut env, handle), "close"),
                6 => (0x2cc600, vec![handle, DST, y, x % 0x200], fs::dev_read(&mut m, &mut env, handle, DST, y, x % 0x200), "read"),
                7 => (0x2cc680, vec![], fs::hostfs_service_read(&mut m, &mut env), "service read"),
                8 => (0x2cc730, vec![handle, DST, y, x % 0x200], fs::hostfs_write(&mut m, &mut env, handle, DST, y, x % 0x200), "write"),
                _ => (0x2cc7a0, vec![handle], fs::dev_get_size(&mut m, &mut env, handle), "size"),
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
            assert_eq!(r.cpu.gpr(2) as u32, ours, "seq {seq} step {step}: {name} {f:#x} {args:x?}: the answer");
            *hits.entry(name).or_default() += (ours != 0) as u32;
        }
    }
    println!("non-zero answers: {hits:?}");
}
