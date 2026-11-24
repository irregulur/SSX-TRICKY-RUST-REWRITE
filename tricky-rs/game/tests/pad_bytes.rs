//! `tricky_game::input::bytes` (board row E10d) against the original in the function runner (TRICKY_ELF; skipped
//! without it): the pair parsers, the config's getters and the pad devices' constructor, empty virtuals and
//! destructors on random bytes. `Pad_ParseButtonNames`, `Mem_Delete` and `scePadPortClose` are stand-ins on both
//! sides that log their arguments and answer from the case's seed (the names parser sometimes bumping the three
//! counts, to show they are read again). The calls, the answers and the memory are compared.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use r5900::Runner;
use tricky_game::fe3dstate::FeEnv;
use tricky_game::input::bytes as pb;
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
const SIZE: u32 = 0x400;
const CFG: u32 = ARENA;

const FUNCS: [u32; 20] = [
    0x179fb0, 0x17a038, 0x17a0a0, 0x17a018, 0x17a028, 0x17a108, 0x17a118, 0x17a128, 0x17a138, 0x17aa08, 0x17aa58,
    0x17aa60, 0x17aa68, 0x17aa70, 0x17aa80, 0x17aa88, 0x17aa90, 0x17ab58, 0x17ad10, 0x17ad60,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

struct Answers {
    seed: Cell<u64>,
    n: Cell<u64>,
}
impl Answers {
    fn next(&self) -> u32 {
        self.n.set(self.n.get() + 1);
        Rng(self.seed.get() ^ self.n.get().wrapping_mul(0x1234_5678_9abc_def1)).next()
    }
}

fn arity(func: u32) -> usize {
    match func {
        pb::MEM_DELETE => 1,
        _ => 2,
    }
}

/// The answer, and the counts to bump (the names parser, one time in four)
fn answer(func: u32, r: u32) -> (u32, bool) {
    let v = if func == pb::PARSE_NAMES { [0, 1, 0x40, r][(r % 4) as usize] } else { r };
    (v, func == pb::PARSE_NAMES && r % 16 < 4)
}

struct Env<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl FeEnv for Env<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.log.push(Call(func, args[..arity(func)].to_vec()));
        let (v, bump) = answer(func, self.ans.next());
        if bump {
            for at in [8, 0xc, 0x10] {
                let n = m.r32(CFG + at);
                m.w32(CFG + at, n + 1);
            }
        }
        v
    }
}

#[test]
fn pad_config_bytes_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    for func in [pb::PARSE_NAMES, pb::MEM_DELETE, pb::PAD_PORT_CLOSE] {
        let (l, a) = (log.clone(), ans.clone());
        r.stub(func, move |c, mem| {
            let w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            l.borrow_mut().push(Call(func, w));
            let (v, bump) = answer(func, a.next());
            if bump {
                for at in [8, 0xc, 0x10] {
                    let n = mem.read_u32(CFG + at);
                    mem.write(CFG + at, &(n + 1).to_le_bytes()).unwrap();
                }
            }
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut g = Rng(0x9ad_0000_0000_0001);
    let mut parsed = 0usize;
    for case in 0..10_000u32 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem { regions: vec![(ARENA, vec![0; SIZE as usize])] };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        for at in [8u32, 0xc, 0x10] {
            m.w32(CFG + at, g.below(14));
        }
        let (a0, a1, a2) = match func {
            0x17aa08 | 0x17ab58 | 0x17ad10 => (ARENA + 4 * g.below(0xc0), g.pick(&[0, 1, 2, 3]), 0),
            0x17a018 | 0x17a028 | 0x17a108 | 0x17a118 | 0x17a128 | 0x17a138 => (CFG, g.below(14), 0),
            _ => (CFG, g.next(), g.next()),
        };
        let seed = g.next() as u64 | (g.next() as u64) << 32;
        ans.seed.set(seed);
        ans.n.set(0);
        let before = m.regions.clone();
        let mut env = Env { log: vec![], ans: &ans };
        let none = 0x5eed_5eed;
        let (mm, e) = (&mut m, &mut env);
        let ours = match func {
            0x179fb0 => { pb::parse_button_pair(mm, e, a0, a1, a2); none }
            0x17a038 => { pb::parse_grab_pair(mm, e, a0, a1, a2); none }
            0x17a0a0 => { pb::parse_axis_pair(mm, e, a0, a1, a2); none }
            0x17a018 => pb::get(mm, a0, 0x14, a1),
            0x17a028 => pb::get(mm, a0, 0x18, a1),
            0x17a108 => pb::get(mm, a0, 0xfc, a1),
            0x17a118 => pb::get(mm, a0, 0x100, a1),
            0x17a128 => pb::get(mm, a0, 0x84, a1),
            0x17a138 => pb::get(mm, a0, 0x88, a1),
            0x17aa08 => pb::pad_device_construct(mm, a0),
            0x17ab58 => { pb::pad_device_destruct(mm, e, a0, a1); none }
            0x17ad10 => { pb::ps2_device_destruct(mm, e, a0, a1); none }
            0x17aa68 => 0,
            0x17ad60 => 0x2c,
            _ => none, // the empty ones (vf2 0x17aa70 answers 0.0 in f0, checked below)
        };
        parsed += env.log.iter().filter(|c| c.0 == pb::PARSE_NAMES).count();
        ans.n.set(0);
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        for (k, v) in [a0, a1, a2].iter().enumerate() {
            r.cpu.set_gpr(4 + k, *v as i32 as i64 as u64);
        }
        r.cpu.set_f(0, 1.5);
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if ours != none {
            assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x}: the answer");
        }
        if func == 0x17aa70 {
            assert_eq!(r.cpu.f(0).to_bits(), 0, "case {case}: vf2's answer");
        }
        assert_eq!(*log.borrow(), env.log, "case {case} {func:#x}: the calls");
        for (at, b) in &m.regions {
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    println!("{parsed} names parsed");
    assert!(parsed > 2000);
}
