//! `tricky_game::course_resolve::{resolve_data_header, free_collision}` (board row E3j) against the original in the
//! function runner (TRICKY_ELF; skipped without it): a resolver with a table of stand-in virtuals and a random
//! header (its record lists real, for the index tables, which run on both sides), and a collision owner with random
//! arrays; every call out logged on both sides and the answers and memory compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::course_resolve::{self as cr, ResolveEnv};
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
const SIZE: u32 = 0x8000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const R: u32 = ARENA + 0x100;
const VTAB: u32 = ARENA + 0x200;
const ELVT: u32 = ARENA + 0x380;
const H: u32 = ARENA + 0x400;
const TABLE: u32 = ARENA + 0x800;
const RECS: u32 = ARENA + 0x1000;
const C: u32 = ARENA + 0x3000;
const A8: u32 = ARENA + 0x4010;
const A150: u32 = ARENA + 0x5010;
fn vf(off: u32) -> u32 {
    0x01f0_0000 + off
}
const ELDTOR: [u32; 2] = [0x01f1_0000, 0x01f1_0100];
/// The virtuals the header pass calls, with how many arguments each gets (the resolver's included)
const VOFFS: [(u32, usize); 13] = [
    (0x98, 3),
    (0x108, 3),
    (0x68, 4),
    (0x70, 3),
    (0xb8, 4),
    (0xc0, 3),
    (0x60, 3),
    (0xb0, 3),
    (0xe0, 4),
    (0xe8, 4),
    (0xf0, 4),
    (0xf8, 3),
    (0x118, 2),
];
const HEADER: u32 = 0x252e18;
const FREE: u32 = 0x25fd40;

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32, a0: u32) -> usize {
    if let Some(&(_, n)) = VOFFS.iter().find(|(o, _)| vf(*o) == f) {
        return n;
    }
    match f {
        cr::DEBUG_PRINT if a0 == 0x3a6568 => 4,
        cr::DEBUG_PRINT if a0 == 0x3a67d0 => 3,
        cr::SPHERE_CACHE_DESTRUCT => 2,
        f if ELDTOR.contains(&f) => 2,
        _ => 1,
    }
}
fn answer(f: u32, w: &[u32]) -> u32 {
    (f ^ w.first().copied().unwrap_or(0)).wrapping_mul(0x9e37) & 7
}

struct Env {
    log: Vec<Call>,
}
impl ResolveEnv for Env {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func, w.first().copied().unwrap_or(0)), 0);
        let v = answer(func, &w);
        self.log.push(Call(func, w));
        v
    }
    fn call_f(&mut self, _m: &mut Mem, _func: u32, _args: &[u32], _f12: f32) -> u32 {
        unreachable!()
    }
}

/// A list of `n` records (a count of words, then the words) at `at`
fn records(m: &mut Mem, g: &mut Rng, at: u32, n: u32) {
    let mut p = at;
    for _ in 0..n {
        let len = g.below(4);
        m.w32(p, len);
        p += 4 * (len + 1);
    }
}

#[test]
fn course_header_and_collision_free_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [HEADER, FREE, 0x254e58, 0x254e98, 0x260fa0] {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let mut stubs = vec![cr::DEBUG_PRINT, cr::MEM_FREE, cr::SPHERE_CACHE_FREE_ALL, cr::SPHERE_CACHE_DESTRUCT];
    stubs.extend(ELDTOR);
    stubs.extend(VOFFS.map(|(o, _)| vf(o)));
    for func in stubs {
        let l = log.clone();
        r.stub(func, move |c, _| {
            let a0 = c.gpr(4) as u32;
            let w: Vec<u32> = (4..4 + arity(func, a0)).map(|k| c.gpr(k) as u32).collect();
            let v = answer(func, &w);
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut g = Rng(0xe3_0000_0000_000a);
    let (mut calls, mut whole) = (0, 0);
    for case in 0..4000u32 {
        let func = if case % 2 == 0 { HEADER } else { FREE };
        let mut m = Mem { regions: vec![(ARENA, vec![0; SIZE as usize]), (STACK.0, vec![0; STACK.1 as usize])] };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        if func == HEADER {
            m.w32(R + 4, H);
            m.w32(R + 8, TABLE);
            m.w32(R + 0xc, TABLE + 0x40);
            m.w32(R + 0x10, VTAB);
            for (off, _) in VOFFS {
                m.w16(VTAB + off, g.pick(&[0u16, 0, 0x10, 0xfff8]));
                m.w32(VTAB + off + 4, vf(off));
            }
            let kind = g.next() as u8;
            m.w8(H + 3, g.pick(&[0, 1, 2, 3, 0, 1, 2, 3, 4, 0xff, kind]));
            for at in (0x40..=0x80).step_by(4) {
                let v = g.next();
                m.w32(H + at, g.pick(&[0, v]));
            }
            // the two record lists the index tables walk (counts +0x18 / +0x28, offsets +0x54 / +0x64)
            for (n_at, o_at, list) in [(0x18, 0x54, RECS), (0x28, 0x64, RECS + 0x400)] {
                let n = g.pick(&[0, 1, 2, 3, 5, u32::MAX]);
                let o = if n == 0 || n == u32::MAX { g.pick(&[0, list - H]) } else { list - H };
                m.w32(H + n_at, n);
                m.w32(H + o_at, o);
                records(&mut m, &mut g, list, (n as i32).max(0) as u32);
            }
        } else {
            for at in [0x14, 0x20, 0x24] {
                let v = g.next();
                m.w32(C + at, g.pick(&[0, v]));
            }
            let n8 = g.below(4);
            m.w32(A8 - 0x10, n8);
            m.w32(C + 0x18, g.pick(&[0, A8, A8]));
            let n150 = g.below(4);
            m.w32(A150 - 0x10, n150);
            m.w32(C + 0x1c, g.pick(&[0, A150, A150]));
            for (k, d) in ELDTOR.iter().enumerate() {
                let vt = ELVT + 0x40 * k as u32;
                m.w16(vt + 8, g.pick(&[0u16, 0x10, 0xfff8]));
                m.w32(vt + 0xc, *d);
            }
            for k in 0..n150 {
                let vt = ELVT + 0x40 * g.below(2);
                m.w32(A150 + 0x150 * k, vt);
            }
        }
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let (args, ours) = match func {
            HEADER => ([R, H], Some(cr::resolve_data_header(&mut m, &mut env, R, H))),
            _ => ([C, 0], {
                cr::free_collision(&mut m, &mut env, C, TOP);
                None
            }),
        };
        calls += env.log.len();
        whole += (ours == Some(1)) as u32;
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
    println!("{calls} calls, {whole} whole headers");
    assert!(calls > 50000 && whole > 1000);
}
