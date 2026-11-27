//! The disc's reads in `tricky_game::filesys` (board row E2x) against the original in the function runner
//! (TRICKY_ELF; skipped without it): random reads started on a made-up disc table (aligned and not, whole sectors
//! and parts, the first sector already in the buffer or not), the read thread woken on random reads in flight (its
//! loop ended at its next wait), the drive's callback, the recovery and the one-sector read. The drive (`sceCd*`,
//! which also writes a word of each sector it reads), the semaphores, the cache flush and the done callback
//! (`FileSys_CompleteCurrent` or a stub) are answered the same on both sides and logged. Memory, answers and the
//! calls out are compared after every call.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::{Runner, RETURN_SENTINEL};
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

const S_REGION: (u32, u32) = (0x34eec0, 0x140);
const CD_REGION: (u32, u32) = (0x349d40, 0x80);
const SECTOR: (u32, u32) = (0x349dc0, 0x800);
const DST: u32 = 0x0105_0000;
const DST_SIZE: u32 = 0x12000;
const BOUNCE: u32 = 0x0106_8000;
const TABLE: u32 = 0x0107_0000;
const CB_A: u32 = 0x0170_0200;
/// an operation running (its callback 0) for `FileSys_CompleteCurrent`
const OP: u32 = 0x0108_0000;

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

#[derive(Clone)]
struct World {
    seed: u32,
    n: u32,
}

fn mapped(a: u32) -> bool {
    (DST..DST + DST_SIZE - 4).contains(&a) || (BOUNCE..BOUNCE + 0x7fc).contains(&a) || (SECTOR.0..SECTOR.0 + 0x7fc).contains(&a)
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32]) -> (u32, Vec<(u32, u32)>) {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        let mut w = vec![];
        let v = match f {
            fs::SCE_CD_READ => {
                if h % 4 == 0 {
                    0
                } else {
                    if mapped(a[2]) {
                        w.push((a[2] & !3, h));
                    }
                    1
                }
            }
            fs::SCE_CD_SYNC => (a[0] == 1 && h % 3 == 0) as u32,
            fs::SCE_CD_GET_ERROR => (h % 4 == 0) as u32 * (1 + (h >> 4) % 0x30),
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
fn the_disc_reads_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.step_limit = 2_000_000_000;
    let funcs = [0x2c90c8, 0x2c9498, 0x2c94c0, 0x2c9580, 0x2c9710, 0x2c9930, 0x2c9e20];
    // `FileSys_CompleteCurrent` (with nothing running), `EAMem_Copy`
    for f in funcs.into_iter().chain([0x2cb780, 0x2cb328, 0x2ce114]) {
        r.allow(f);
    }
    let world = Rc::new(RefCell::new(World { seed: 0, n: 0 }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let waits = Rc::new(RefCell::new(0));
    let outs = [
        (fs::SCE_CD_INIT, 1),
        (fs::SCE_CD_MMODE, 1),
        (fs::SCE_CD_DISK_READY, 1),
        (fs::SCE_CD_READ, 4),
        (fs::SCE_CD_SYNC, 1),
        (fs::SCE_CD_GET_ERROR, 0),
        (fs::SCE_CD_CALLBACK, 1),
        (fs::SCE_CD_INIT_EE_CB, 3),
        (fs::FLUSH_CACHE, 1),
        (fs::SIGNAL_SEMA, 1),
        (fs::I_SIGNAL_SEMA, 1),
        (fs::WAIT_SEMA, 1),
        (CB_A, 1),
        (fs::MUTEX_LOCK, 1),
        (fs::MUTEX_UNLOCK, 1),
    ];
    for (f, n) in outs {
        let (world, log, waits) = (world.clone(), log.clone(), waits.clone());
        r.stub(f, move |c, m| {
            let a: Vec<u32> = (4..4 + n).map(|k| c.gpr(k) as u32).collect();
            let (v, writes) = world.borrow_mut().answer(f, &a);
            for (at, x) in writes {
                m.write(at, &x.to_le_bytes()).unwrap();
            }
            log.borrow_mut().push(Call(f, a));
            c.set_gpr(2, v as i32 as i64 as u64);
            if f == fs::WAIT_SEMA {
                // the thread's next wait ends the run
                *waits.borrow_mut() += 1;
                if *waits.borrow() == 2 {
                    c.set_gpr(31, RETURN_SENTINEL as i32 as i64 as u64);
                }
            }
        });
    }

    let mut g = Rng(0xcd00_0000_0000_0001);
    let mut hits = std::collections::BTreeMap::<&str, u32>::new();
    let mut recovers = 0;
    for seq in 0..400 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        // the disc's table: 8 entries of 0x10 + 0xc bytes (sector, size)
        let mut table = rnd(&mut g, 0x100);
        for k in 0..8 {
            let at = k * 0x1c;
            table[at + 4..at + 8].copy_from_slice(&(0x100 + g.below(0x1000)).to_le_bytes());
            let size = if g.below(4) == 0 { g.below(0x800) } else { g.below(0x10000) };
            table[at + 8..at + 0xc].copy_from_slice(&size.to_le_bytes());
        }
        let mut m = Mem {
            regions: vec![
                (S_REGION.0, vec![0; S_REGION.1 as usize]),
                (CD_REGION.0, rnd(&mut g, CD_REGION.1)),
                (SECTOR.0, rnd(&mut g, SECTOR.1)),
                (DST, rnd(&mut g, DST_SIZE)),
                (BOUNCE, rnd(&mut g, 0x800)),
                (TABLE, table),
                (OP, vec![0; 0x30]),
            ],
        };
        m.w32(fs::DISC_ENTRY, 0x10);
        m.w32(fs::DISC_TABLE, TABLE);
        m.w32(fs::CD + 0x40, BOUNCE);
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let w0 = World { seed: g.next(), n: 0 };
        *world.borrow_mut() = w0.clone();
        log.borrow_mut().clear();
        let mut env = Env { w: w0, log: vec![] };
        for step in 0..30 {
            // the state: flags, tries, the callback, and (for the thread and the recovery) a read in flight
            let set = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
                m.w32(a, v);
                r.mem.write_u32(a, v);
            };
            set(&mut m, &mut r, fs::S + 0x10, 0);
            set(&mut m, &mut r, fs::S + 0x20, if g.below(2) == 0 { 0 } else { OP });
            set(&mut m, &mut r, OP + 4, 0);
            set(&mut m, &mut r, fs::CD, g.below(8));
            set(&mut m, &mut r, fs::CD + 0x10, g.below(4));
            set(&mut m, &mut r, fs::CD + 0x18, [0, CB_A, fs::COMPLETE_CURRENT][g.below(3) as usize]);
            let off = if g.below(2) == 0 { 0 } else { g.below(0x800) };
            let chunk = g.below(0x801 - off);
            let left = match g.below(5) {
                0 => 0,
                1 => g.next() | 0x8000_0000,
                2 => g.below(0x800),
                3 => 0x800,
                _ => g.below(0x8000),
            };
            let dest = DST + (g.below(0x8000) & if g.below(2) == 0 { !0x3f } else { !0 });
            let read = [chunk, left, off, 0x100 + g.below(0x1000), g.below(5), dest];
            for (k, v) in read.iter().enumerate() {
                set(&mut m, &mut r, fs::CD + 0x24 + 4 * k as u32, *v);
            }
            let (f, args, ours, name): (u32, Vec<u32>, u32, &str) = match g.below(12) {
                0..=4 => {
                    let index = 1 + g.below(8);
                    let entry = TABLE + (index - 1) * 0x1c;
                    let size = m.r32(entry + 8);
                    let mut far = false;
                    let pos = match g.below(7) {
                        0 => 0,
                        // before the start or past the end (the read then never takes the buffered sector)
                        5 => {
                            far = true;
                            0u32.wrapping_sub(1 + g.below(0x1000))
                        }
                        6 => {
                            far = true;
                            size + 1 + g.below(0x1000)
                        }
                        1 => (g.below(0x10) * 0x800).min(size),
                        2 => size.saturating_sub(g.below(0x900)),
                        _ => g.below(size.max(1)),
                    };
                    let n = match g.below(5) {
                        0 => g.below(0x900),
                        // up to the end of the first sector, or one sector
                        1 => 0x800 - (pos & 0x7ff),
                        2 => 0x800,
                        _ => g.below(0x10000),
                    };
                    let to = DST + (g.below(0x400) & if g.below(2) == 0 { !0x3f } else { !0 });
                    // often the first sector in the buffer already
                    if far {
                        set(&mut m, &mut r, fs::CD + 0x14, u32::MAX);
                    } else if g.below(2) == 0 {
                        let sector = m.r32(entry + 4).wrapping_add(((pos as i32).max(0) >> 11) as u32);
                        set(&mut m, &mut r, fs::CD + 0x14, sector);
                    }
                    (0x2c9e20, vec![index, to, pos, n], fs::cdfs_start_read(&mut m, &mut env, index, to, pos, n), "start read")
                }
                5 | 6 => {
                    fs::cdstream_thread_wake(&mut m, &mut env);
                    let sema = m.r32(fs::CD + 0xc);
                    env.call(&mut m, fs::WAIT_SEMA, &[sema]);
                    (0x2c9580, vec![], 0, "thread")
                }
                7 => {
                    fs::cdstream_issue_read(&mut m, &mut env);
                    (0x2c94c0, vec![], 0, "issue read")
                }
                8 => {
                    let reason = g.below(3);
                    fs::cdfs_cd_callback(&mut m, &mut env, reason);
                    (0x2c9498, vec![reason], 0, "cd callback")
                }
                9 | 10 if recovers < 80 => {
                    recovers += 1;
                    set(&mut m, &mut r, fs::CD + 0x34, g.below(4));
                    if g.below(2) == 0 {
                        (0x2c9930, vec![], fs::cdfs_poll_recover(&mut m, &mut env), "poll recover")
                    } else {
                        (0x2c9710, vec![], fs::cdfs_recover_read(&mut m, &mut env), "recover")
                    }
                }
                _ => (0x2c90c8, vec![], fs::cdfs_read_sector(&mut m, &mut env), "read sector"),
            };
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
            }
            *waits.borrow_mut() = 0;
            r.call(f).unwrap_or_else(|e| panic!("seq {seq} step {step}: {name} {f:#x} {args:x?}: {e}"));
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                if &t != v {
                    let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
                    panic!("seq {seq} step {step}: {name} {f:#x} {args:x?}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
                }
            }
            assert_eq!(*log.borrow(), env.log, "seq {seq} step {step}: {name} {f:#x} {args:x?}: the calls out");
            if ![0x2c9580, 0x2c94c0, 0x2c9498].contains(&f) {
                assert_eq!(r.cpu.gpr(2) as u32, ours, "seq {seq} step {step}: {name} {f:#x} {args:x?}: the answer");
            }
            *hits.entry(name).or_default() += 1;
        }
    }
    println!("calls: {hits:?}");
}
