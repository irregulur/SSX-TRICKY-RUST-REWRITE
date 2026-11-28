//! `tricky_game::strm` (board row E2z) against the original in the function runner (TRICKY_ELF; skipped without
//! it): every small call of the stream library on random handles (none, a header without the magic, good ones
//! with random fields). `FileSys_SetPriority` is answered by a stub and logged. Memory, answers and calls compared.

use std::cell::RefCell;
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

const HANDLES: u32 = 0x0104_0000;
const HEADERS: u32 = 0x0105_0000;
const OUT: u32 = 0x0106_0000;

struct Env {
    log: Vec<(u32, Vec<u32>)>,
}

impl StrmEnv for Env {
    fn call(&mut self, _m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        self.log.push((f, args.to_vec()));
        0
    }
}

#[test]
fn the_stream_calls_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let funcs = [0x2c6358, 0x2c6390, 0x2c6e48, 0x2c7220, 0x2c7270, 0x2c72e0, 0x2c7340, 0x2c73a0, 0x2c7c10, 0x2c7c40, 0x2c7c70, 0x2c7cb8, 0x2c7cf0];
    for f in funcs {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<(u32, Vec<u32>)>>> = Rc::default();
    {
        let log = log.clone();
        r.stub(strm::FS_SET_PRIORITY, move |c, _| {
            log.borrow_mut().push((strm::FS_SET_PRIORITY, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, 0);
        });
    }
    let mut g = Rng(0x5747_0000_0000_0001);
    let mut nonzero = 0;
    for run in 0..300 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let mut m = Mem { regions: vec![(HANDLES, rnd(&mut g, 0x100)), (HEADERS, rnd(&mut g, 0x800)), (OUT, rnd(&mut g, 0x10))] };
        // 8 handles, each pointing at a header; most with the magic
        for k in 0..8 {
            let head = HEADERS + 0x200 * (k % 4);
            m.w32(HANDLES + 0x10 * k, head);
            if g.below(4) != 0 {
                m.w32(head, strm::MAGIC);
            }
            for (o, v) in [(0x24, g.below(6)), (0x34, g.below(3)), (0x40, g.below(0x100)), (0x48, g.below(0x100)), (0x8, if g.below(2) == 0 { 0 } else { g.next() })] {
                if o == 8 {
                    m.w32(HANDLES + 0x10 * k + 8, v);
                } else {
                    m.w32(head + o, v);
                }
            }
        }
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let mut env = Env { log: vec![] };
        log.borrow_mut().clear();
        for step in 0..40 {
            let h = match g.below(8) {
                0 => 0,
                _ => HANDLES + 0x10 * g.below(8),
            };
            let (x, y, z) = (g.next(), g.below(0x120), g.below(8));
            let (f, args, ours): (u32, Vec<u32>, Option<u32>) = match g.below(13) {
                0 => (0x2c6358, vec![h, OUT, OUT + 8], Some(strm::get_header(&mut m, h, OUT, OUT + 8))),
                1 => {
                    let (a, b, c) = (g.below(4) * 0x10, g.below(4) * 0x10, g.below(4) * 0x10);
                    (0x2c6390, vec![a, b, c], Some(strm::ring_in_range(a, b, c)))
                }
                2 => (0x2c6e48, vec![x, y, z], Some(strm::calc_mem_size(x, y, z))),
                3 => {
                    strm::set_priorities(&mut m, h, x, y);
                    (0x2c7220, vec![h, x, y], None)
                }
                4 => {
                    strm::set_low_water(&mut m, &mut env, h, y);
                    (0x2c7270, vec![h, y], None)
                }
                5 => {
                    let u = g.below(3);
                    strm::set_urgent(&mut m, &mut env, h, u);
                    (0x2c72e0, vec![h, u], None)
                }
                6 => {
                    let n = if g.below(6) == 0 { x } else { z };
                    (0x2c7340, vec![h, n], Some(strm::get_chunk(&m, h, n)))
                }
                7 => (0x2c73a0, vec![h], Some(strm::get_word4(&m, h))),
                8 => (0x2c7c10, vec![h], Some(strm::get_buffered_bytes(&m, h))),
                9 => (0x2c7c40, vec![h], Some(strm::get_state(&m, h))),
                10 => (0x2c7c70, vec![h], Some(strm::is_finished(&m, h))),
                11 => (0x2c7cb8, vec![h], Some(strm::get_buffer_size(&m, h))),
                _ => (0x2c7cf0, vec![h], Some(strm::get_fill_bytes(&m, h))),
            };
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
            }
            r.call(f).unwrap_or_else(|e| panic!("run {run} step {step}: {f:#x} {args:x?}: {e}"));
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                assert_eq!(&t, v, "run {run} step {step}: {f:#x} {args:x?}: memory at {a:#x}");
            }
            assert_eq!(*log.borrow(), env.log, "run {run} step {step}: {f:#x} {args:x?}: the calls out");
            if let Some(v) = ours {
                assert_eq!(r.cpu.gpr(2) as u32, v, "run {run} step {step}: {f:#x} {args:x?}: the answer");
                nonzero += (v != 0) as u32;
            }
        }
    }
    assert!(nonzero > 1000);
}

const ST: u32 = 0x0107_0000;
const REQS: u32 = 0x0108_0000;
const TYPES: u32 = 0x0109_0000;
const GLOBALS: (u32, u32) = (0x3bbb00, 0x40);

/// The request list (row E2aa): random allocs, appends, unlinks (of queued requests), lookups and chunk matches
#[test]
fn the_request_list_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x2c6460, 0x2c64e0, 0x2c6558, 0x2c65a0, 0x2c6610] {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<(u32, Vec<u32>)>>> = Rc::default();
    for f in [strm::MUTEX_LOCK, strm::MUTEX_UNLOCK] {
        let log = log.clone();
        r.stub(f, move |c, _| {
            log.borrow_mut().push((f, vec![c.gpr(4) as u32]));
            c.set_gpr(2, 0);
        });
    }
    let mut g = Rng(0x5747_aa00_0000_0001);
    let mut found = 0;
    for run in 0..300 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let n = 1 + g.below(8);
        let mut m = Mem {
            regions: vec![(ST, rnd(&mut g, 0x80)), (REQS, rnd(&mut g, 8 * 0x124)), (TYPES, rnd(&mut g, 0x60)), (GLOBALS.0, rnd(&mut g, GLOBALS.1)), (OUT, rnd(&mut g, 0x10))],
        };
        m.w32(ST + 0x10, REQS);
        m.w32(ST + 0x14, n);
        m.w32(ST + 0x18, TYPES);
        m.w32(ST + 0x1c, if g.below(6) == 0 { -1i32 as u32 } else { g.below(8) });
        for k in 0..8 {
            // masks with few bits, so some match
            m.w32(TYPES + 0xc * k, 1 << g.below(4) | 1 << g.below(4));
            m.w32(TYPES + 0xc * k + 4, g.below(4));
        }
        m.w32(strm::REQUEST_NUMBER, if g.below(4) == 0 { 0xffff_fe00 } else { g.next() & !0xff });
        for (o, v) in [(0x58, 0), (0x5c, 0), (0x60, 0), (0x64, REQS)] {
            m.w32(ST + o, v);
        }
        for k in 0..n {
            let q = REQS + 0x124 * k;
            m.w32(q, k);
            m.w32(q + 4, 0);
            m.w32(q + 0xc, if k + 1 < n { q + 0x124 } else { 0 });
        }
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let mut env = Env { log: vec![] };
        log.borrow_mut().clear();
        let mut free: Vec<u32> = vec![];
        for step in 0..60 {
            let queued: Vec<u32> = {
                let mut v = vec![];
                let mut q = m.r32(ST + 0x58);
                while q != 0 && v.len() < 16 {
                    v.push(q);
                    q = m.r32(q + 0xc);
                }
                v
            };
            let (f, args, ours): (u32, Vec<u32>, Option<u32>) = match g.below(6) {
                0 => {
                    let a = strm::alloc_request(&mut m, &mut env, ST);
                    if a != 0 {
                        free.push(a);
                    }
                    (0x2c6460, vec![ST], Some(a))
                }
                1 if !free.is_empty() => {
                    let q = free.remove(g.below(free.len() as u32) as usize);
                    strm::append_request(&mut m, &mut env, ST, q);
                    (0x2c64e0, vec![ST, q], None)
                }
                2 if !queued.is_empty() => {
                    let q = queued[g.below(queued.len() as u32) as usize];
                    // (the one being read anywhere in the queue)
                    let cur = queued[g.below(queued.len() as u32) as usize];
                    m.w32(ST + 0x5c, cur);
                    r.mem.write_u32(ST + 0x5c, cur);
                    strm::unlink_request(&mut m, ST, q);
                    (0x2c65a0, vec![ST, q], None)
                }
                3 | 4 => {
                    let id = match g.below(3) {
                        0 if !queued.is_empty() => m.r32(queued[g.below(queued.len() as u32) as usize]),
                        1 => m.r32(REQS + 0x124 * g.below(n)),
                        _ => g.below(10) | (g.next() & 0x300),
                    };
                    let v = strm::lookup_request(&m, ST, id);
                    found += (v != 0) as u32;
                    (0x2c6558, vec![ST, id], Some(v))
                }
                _ => {
                    m.w32(OUT, g.below(16));
                    r.mem.write_u32(OUT, m.r32(OUT));
                    (0x2c6610, vec![ST, OUT], Some(strm::match_chunk_type(&m, ST, OUT)))
                }
            };
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
            }
            r.call(f).unwrap_or_else(|e| panic!("run {run} step {step}: {f:#x} {args:x?}: {e}"));
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                assert_eq!(&t, v, "run {run} step {step}: {f:#x} {args:x?}: memory at {a:#x}");
            }
            assert_eq!(*log.borrow(), env.log, "run {run} step {step}: {f:#x} {args:x?}: the calls out");
            if let Some(v) = ours {
                assert_eq!(r.cpu.gpr(2) as u32, v, "run {run} step {step}: {f:#x} {args:x?}: the answer");
            }
        }
    }
    assert!(found > 100, "{found}");
}
