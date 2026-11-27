//! `tricky_game::asyncfile` (board row E2t) against the original in the function runner (TRICKY_ELF; skipped
//! without it): random tables (1 to 0x100 slots, and refused sizes), then random loads, loads into a buffer, reads
//! on an open file, callbacks set, polls, finishes, cancels, the file system's callbacks fired on pending slots
//! (handles of 0, short and whole chunks, cancels in between) and the whole table cancelled and waited on. The file
//! system, the heap and the threads are answered the same on both sides (their waits and sleeps end operations)
//! and logged; memory, answers and the calls out compared after every call.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::asyncfile::{self as af, FileEnv};
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
const SLOTS: u32 = 0x0100_0040;
const SCRATCH: u32 = 0x0110_0000;
const ALLOC_FN: u32 = 0x0170_0000;
const FREE_FN: u32 = 0x0170_0100;
const USER_FN: u32 = 0x0170_0200;
const MUTEX: [u32; 4] = [0x2ceb00, 0x2ceb60, 0x2cea98, 0x2ceae0];
const CALLBACKS: [u32; 5] = [af::ON_CLOSED, af::ON_CHUNK_THEN_CLOSE, af::ON_SIZED, af::ON_OPENED, af::ON_CHUNK];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

/// The file system, heap and threads: answers from a hash of the call, and the operations it has queued
#[derive(Clone)]
struct World {
    seed: u32,
    n: u32,
    ops: Vec<(u32, u32)>,
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32]) -> (u32, Vec<(u32, u32)>) {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        let mut writes = vec![];
        let v = match f {
            af::FS_QUEUE_OPEN | af::FS_QUEUE_READ | af::FS_QUEUE_CLOSE | af::FS_GET_FILE_SIZE => {
                if h % 9 == 0 {
                    0
                } else {
                    let op = (h >> 4) | 0x10;
                    self.ops.push((op, *a.last().unwrap()));
                    op
                }
            }
            af::FS_GET_RESULT => match h % 7 {
                0 => 0,
                1 | 2 => 0x8000,
                3 => h & 0x7fff,
                4 => 0x8000 | (h >> 20),
                5 => 0x7fff,
                _ => h,
            },
            af::FS_WAIT_OP => {
                if let Some(&(_, slot)) = self.ops.iter().rev().find(|(o, _)| *o == a[0]) {
                    let next = if h % 5 == 0 {
                        let op = (h >> 4) | 0x10;
                        self.ops.push((op, slot));
                        op
                    } else {
                        0
                    };
                    writes.push((slot + 0x1c, next));
                    if h % 4 == 0 {
                        writes.push((slot + 0x10, 1));
                    }
                }
                0
            }
            af::THREAD_SLEEP_MS => {
                let k = (self.ops.len() + 1) / 2;
                for (_, slot) in self.ops.drain(..k) {
                    writes.push((slot + 0x1c, 0));
                }
                0
            }
            af::THREAD_IS_CURRENT => h & 1,
            ALLOC_FN => {
                if a[0] == af::TABLE_NAME {
                    SLOTS
                } else if h % 5 == 0 {
                    0
                } else {
                    h | 2
                }
            }
            _ => h,
        };
        (v, writes)
    }
}

struct Env {
    w: World,
    log: Vec<Call>,
}

impl FileEnv for Env {
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
fn the_async_files_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [
        0x2c5660, 0x2c56c8, 0x2c5730, 0x2c5760, 0x2c57a8, 0x2c5808, 0x2c5898, 0x2c58d0, 0x2c59c0, 0x2c5a98, 0x2c5b98, 0x2c5c60, 0x2c5d30, 0x2c5e20,
        0x2c5ed0, 0x2c5f98, 0x2c6070, 0x2c6108, 0x2c6160, 0x2c6278,
    ] {
        r.allow(f);
    }
    for f in MUTEX {
        r.stub(f, |_, _| {});
    }
    let world = Rc::new(RefCell::new(World { seed: 0, n: 0, ops: vec![] }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let outs = [
        (af::FS_QUEUE_OPEN, 4),
        (af::FS_QUEUE_READ, 6),
        (af::FS_QUEUE_CLOSE, 3),
        (af::FS_GET_FILE_SIZE, 3),
        (af::FS_SET_CALLBACK, 2),
        (af::FS_GET_RESULT, 1),
        (af::FS_WAIT_OP, 1),
        (af::FS_CANCEL_OP, 1),
        (af::THREAD_IS_CURRENT, 1),
        (af::RUN_POLL_TASKS, 1),
        (af::THREAD_SLEEP_MS, 1),
        (ALLOC_FN, 3),
        (FREE_FN, 1),
        (USER_FN, 1),
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

    let mut g = Rng(0xa5f1_0000_0000_0001);
    let (mut fired, mut finished, mut waits, mut cancels_all) = (0u32, 0u32, 0u32, 0u32);
    for seq in 0..300 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let mut m = Mem { regions: vec![(GLOBALS, rnd(&mut g, 0x300)), (SLOTS - 0x40, rnd(&mut g, 0x3080)), (SCRATCH, rnd(&mut g, 0x10))] };
        m.w32(af::TABLE, 0);
        m.w32(af::ALLOC_PTR, ALLOC_FN);
        m.w32(af::FREE_PTR, FREE_FN);
        // the game starts at 0; whole generations only (the index is or-ed in below them)
        m.w32(af::GENERATION, if g.below(4) == 0 { 0xffff_fc00 + (g.below(4) << 8) } else { g.next() & !0xff });
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let w0 = World { seed: g.next(), n: 0, ops: vec![] };
        *world.borrow_mut() = w0.clone();
        log.borrow_mut().clear();
        let mut env = Env { w: w0, log: vec![] };
        let run = |r: &mut Runner, m: &Mem, env: &Env, what: &str, f: u32, args: &[u32], ours: Option<u32>| {
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
            }
            r.call(f).unwrap_or_else(|e| panic!("seq {seq}: {what}: {e}"));
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                if &t != v {
                    let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
                    panic!("seq {seq}: {what}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
                }
            }
            assert_eq!(*log.borrow(), env.log, "seq {seq}: {what}: the calls out");
            if let Some(o) = ours {
                assert_eq!(r.cpu.gpr(2) as u32, o, "seq {seq}: {what}: the answer");
            }
        };
        let mut ids: Vec<u32> = vec![];
        for step in 0..150 {
            let what = format!("step {step}");
            if m.r32(af::TABLE) == 0 || g.below(60) == 0 {
                let count = match g.below(8) {
                    0 => 0x101 + g.below(0x100),
                    1 => 0x100,
                    2 => 1 + g.below(3),
                    _ => 1 + g.below(64),
                };
                let flags = g.next();
                af::init(&mut m, &mut env, count, flags);
                run(&mut r, &m, &env, &format!("{what}: init {count:#x}"), 0x2c5c60, &[count, flags], None);
                continue;
            }
            let id = if !ids.is_empty() && g.below(6) != 0 { ids[g.below(ids.len() as u32) as usize] } else { g.next() & 0xffff };
            let late = ids.len().saturating_sub(4);
            let fresh = if ids.len() > late { ids[late + g.below((ids.len() - late) as u32) as usize] } else { id };
            match g.below(22) {
                0..=2 => {
                    let (p, fl) = (g.next(), g.next());
                    let ours = af::load(&mut m, &mut env, p, fl);
                    if ours != 0 {
                        ids.push(ours);
                    }
                    run(&mut r, &m, &env, &format!("{what}: load"), 0x2c5e20, &[p, fl], Some(ours));
                }
                3 | 4 => {
                    let (p, b) = (g.next(), g.next());
                    let size = match g.below(4) {
                        0 => 0,
                        1 => g.next() | 0x8000_0000,
                        _ => g.below(0x30000),
                    };
                    let ours = af::load_into(&mut m, &mut env, p, b, size);
                    if ours != 0 {
                        ids.push(ours);
                    }
                    run(&mut r, &m, &env, &format!("{what}: load into {size:#x}"), 0x2c5ed0, &[p, b, size], Some(ours));
                }
                5 => {
                    let (h, o, b) = (g.next(), g.next(), g.next());
                    let size = if g.below(4) == 0 { g.next() } else { g.below(0x30000) };
                    let ours = af::start_read(&mut m, &mut env, h, o, b, size);
                    if ours != 0 {
                        ids.push(ours);
                    }
                    run(&mut r, &m, &env, &format!("{what}: start read {size:#x}"), 0x2c5f98, &[h, o, b, size], Some(ours));
                }
                6 => {
                    af::set_callback(&mut m, &mut env, id, USER_FN);
                    run(&mut r, &m, &env, &format!("{what}: set callback {id:#x}"), 0x2c6070, &[id, USER_FN], None);
                }
                7 => {
                    let ours = af::is_done(&m, id);
                    run(&mut r, &m, &env, &format!("{what}: is done {id:#x}"), 0x2c6108, &[id], Some(ours));
                }
                8 | 9 | 20 | 21 => {
                    let id = fresh;
                    let b = if g.below(4) == 0 { 0 } else { SCRATCH };
                    let s = if g.below(4) == 0 { 0 } else { SCRATCH + 8 };
                    let n = env.log.iter().filter(|c| c.0 == af::FS_WAIT_OP).count();
                    let ours = af::finish(&mut m, &mut env, id, b, s);
                    if ours == 1 || g.below(2) == 0 {
                        ids.retain(|x| *x != id);
                    }
                    waits += (env.log.iter().filter(|c| c.0 == af::FS_WAIT_OP).count() - n) as u32;
                    finished += (ours == 1) as u32;
                    run(&mut r, &m, &env, &format!("{what}: finish {id:#x}"), 0x2c6160, &[id, b, s], Some(ours));
                }
                10 => {
                    let id = fresh;
                    let ours = af::cancel(&mut m, &mut env, id);
                    if g.below(2) == 0 {
                        ids.retain(|x| *x != id);
                    }
                    run(&mut r, &m, &env, &format!("{what}: cancel {id:#x}"), 0x2c6278, &[id], Some(ours));
                }
                12 => {
                    let (t, count) = (m.r32(af::TABLE), m.r32(af::COUNT));
                    let id = if g.below(3) == 0 && count <= 0x100 {
                        // the word just past the table holding an id of the slot that would be there
                        let id = (g.next() | 0x100) & !0xff | count;
                        m.w32(t + 0x30 * count, id);
                        r.mem.write_u32(t + 0x30 * count, id);
                        id
                    } else {
                        id
                    };
                    let ours = af::find_slot(&m, id);
                    run(&mut r, &m, &env, &format!("{what}: find {id:#x}"), 0x2c5760, &[id], Some(ours));
                }
                14 | 15 if g.below(3) == 0 => {
                    // another thread is finishing the load (and may have cancelled it): what the game's other
                    // threads would leave for the callbacks and cancels to find
                    let s = af::find_slot(&m, fresh);
                    if s != 0 {
                        m.w32(s + 0xc, 1);
                        r.mem.write_u32(s + 0xc, 1);
                        if g.below(2) == 0 {
                            m.w32(s + 0x10, 1);
                            r.mem.write_u32(s + 0x10, 1);
                        }
                    }
                }
                13 if g.below(8) == 0 => {
                    cancels_all += 1;
                    af::cancel_all(&mut m, &mut env);
                    run(&mut r, &m, &env, &format!("{what}: cancel all"), 0x2c5d30, &[], None);
                    ids.clear();
                }
                _ => {
                    // a pending slot's operation ends: the callback set for it (any, if none was)
                    let (t, count) = (m.r32(af::TABLE), m.r32(af::COUNT));
                    let busy: Vec<u32> = (0..count).map(|i| t + 0x30 * i).filter(|s| m.r32(s + 0x1c) != 0).collect();
                    if busy.is_empty() {
                        continue;
                    }
                    let slot = busy[g.below(busy.len() as u32) as usize];
                    let op = m.r32(slot + 0x1c);
                    let cb = env
                        .log
                        .iter()
                        .rev()
                        .find(|c| c.0 == af::FS_SET_CALLBACK && c.1[0] == op)
                        .map(|c| c.1[1])
                        .unwrap_or_else(|| CALLBACKS[g.below(5) as usize]);
                    match cb {
                        af::ON_CLOSED => af::on_closed(&mut m, &mut env, slot),
                        af::ON_CHUNK_THEN_CLOSE => af::on_chunk_then_close(&mut m, &mut env, slot),
                        af::ON_SIZED => af::on_sized(&mut m, &mut env, slot),
                        af::ON_OPENED => af::on_opened(&mut m, &mut env, slot),
                        _ => af::on_chunk(&mut m, &mut env, slot),
                    }
                    fired += 1;
                    let (a0, a1) = (g.next(), g.next());
                    run(&mut r, &m, &env, &format!("{what}: callback {cb:#x} on {slot:#x}"), cb, &[a0, a1, slot], None);
                }
            }
        }
    }
    println!("{fired} file system callbacks, {finished} loads finished, {waits} waits, {cancels_all} tables cancelled");
    assert!(fired > 8000 && finished > 1500 && waits > 1500 && cancels_all > 100);
}
