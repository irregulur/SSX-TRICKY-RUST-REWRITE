//! `tricky_game::patch_cache` (board row E3h) against the original in the function runner (TRICKY_ELF; skipped
//! without it): a cache with random counts, keys and sizes, the allocator, `Mem_Free`, `Mem_Delete` and the VU0
//! calls stood in, every call out logged on both sides and the answers and memory compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::patch_cache::{self as pc, CacheEnv};
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

const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x1_0000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const C: u32 = ARENA + 0x100;
const ENTRIES: u32 = ARENA + 0x200;
const RECORDS: u32 = ARENA + 0x400;
const OUT: u32 = ARENA + 0x300;

const FUNCS: [u32; 13] = [
    0x251ba0, 0x251bc8, 0x251c20, 0x251d18, 0x251d70, 0x251db8, 0x251e60, 0x251ea8, 0x251ee0, 0x251ef8, 0x251f50,
    0x252028, 0x2520a0,
];
const STUBS: [u32; 7] = [
    pc::MEM_DELETE,
    pc::MEM_FREE,
    pc::EAMEM_ALLOC,
    pc::EAMEM_SET,
    pc::VU0_RUN_PATCH_PROGRAM,
    pc::VU0_GET_VI1,
    pc::VU0_COPY_OUT_DATA,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32) -> usize {
    match f {
        pc::EAMEM_ALLOC | pc::EAMEM_SET => 3,
        pc::VU0_RUN_PATCH_PROGRAM | pc::VU0_COPY_OUT_DATA => 2,
        _ => 1,
    }
}
/// What a stand-in answers: the allocator a block in the arena by its name (none for 0 bytes), the rest a mix
fn answer(f: u32, w: &[u32]) -> u32 {
    if f == pc::EAMEM_ALLOC {
        if w[1] == 0 { 0 } else { ARENA + 0x8000 + (w[0] & 0x10) * 0x100 }
    } else {
        (f ^ w[0]).wrapping_mul(0x9e37)
    }
}

struct Env {
    log: Vec<Call>,
}
impl CacheEnv for Env {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func), 0);
        let v = answer(func, &w);
        self.log.push(Call(func, w));
        v
    }
}

#[test]
fn patch_cache_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for func in STUBS {
        let l = log.clone();
        r.stub(func, move |c, _| {
            let w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            let v = answer(func, &w);
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut g = Rng(0xcac4_e000_0000_0001);
    let mut calls = 0;
    for case in 0..FUNCS.len() as u32 * 600 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem { regions: vec![(ARENA, vec![0; SIZE as usize]), (STACK.0, vec![0; STACK.1 as usize])] };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        // the cache: up to four entries and 0x1cd0-byte records
        let n = g.pick(&[0, 1, 2, 3, 4, 4, u32::MAX, 0x8000_0000]);
        let live = if (n as i32) > 0 { n.min(4) } else { 0 };
        m.w32(C, n);
        let size = if func == 0x251f50 { pc::COLL_RECORD } else { {
            let big = g.next();
            g.pick(&[pc::COLL_RECORD, 0x40, big])
        } };
        m.w32(C + 4, size);
        m.w32(C + 8, g.pick(&[ENTRIES, ENTRIES, 0]));
        m.w32(C + 0xc, g.pick(&[RECORDS, RECORDS, 0]));
        if func == 0x251f50 || func == 0x251d70 || func == 0x251db8 || func == 0x2520a0 || func == 0x251e60 {
            m.w32(C + 8, ENTRIES);
            m.w32(C + 0xc, RECORDS);
        }
        for k in 0..live {
            for at in [0, 4] {
                let big = g.next();
                let v = g.pick(&[0, 1, 2, 3, u32::MAX, -7i32 as u32, 1_999_999_999, 2_000_000_000, 0x8000_0000, big]);
                m.w32(ENTRIES + 8 * k + at, v);
            }
        }
        let keys = [0x0123_4560, 0x0123_4570, 0x0123_4580, 0, 0xdead_c0ed];
        for k in 0..live {
            let key = g.pick(&keys);
            m.w32(RECORDS + pc::COLL_RECORD * k, key);
        }
        let key = g.pick(&keys);
        let idx = g.pick(&[0, 1, 2, 3, 4, 5, u32::MAX, 0x8000_0000]);
        let init_n = g.pick(&[0, 1, 2, 3, u32::MAX]);
        let init_size = g.pick(&[0, 0x40, 0x1cd0]);
        let how = g.below(4);
        let rec = RECORDS + pc::COLL_RECORD * g.below(4);
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mm = &mut m;
        let (args, ours): ([u32; 3], Option<u32>) = match func {
            0x251ba0 => ([C, 0, 0], Some(pc::construct(mm, C))),
            0x251bc8 => ([C, how, 0], { pc::destruct(mm, e, C, how); None }),
            0x251c20 => ([C, init_n, init_size], { pc::init(mm, e, C, init_n, init_size); None }),
            0x251d18 => ([C, 0, 0], { pc::free(mm, e, C); None }),
            0x251d70 => ([C, 0, 0], { pc::age(mm, C); None }),
            0x251db8 => ([C, 0, 0], Some(pc::evict(mm, C))),
            0x251e60 => ([C, idx, 0], { pc::touch(mm, C, idx); None }),
            0x251ea8 => ([C, 0, 0], Some(pc::construct_fwd_diff(mm, C))),
            0x251ee0 => ([C, how, 0], { pc::destruct_fwd_diff(mm, e, C, how); None }),
            0x251ef8 => ([C, init_n, 0], { pc::init_coll(mm, e, C, init_n); None }),
            0x251f50 => ([C, key, OUT], { pc::lookup(mm, e, C, key, OUT); None }),
            0x252028 => ([C, rec, 0], { pc::clear_entry(mm, rec); None }),
            0x2520a0 => ([C, 0, 0], Some(pc::evict_thunk(mm, C))),
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
            let mut got = vec![0u8; b.len()];
            r.mem.read(*at, &mut got).unwrap();
            if let Some(k) = (0..b.len()).find(|&k| got[k] != b[k]) {
                panic!("case {case} {func:#x}: memory differs at {:#x}", *at + k as u32);
            }
        }
    }
    assert!(calls > 1000, "only {calls} calls out");
}
