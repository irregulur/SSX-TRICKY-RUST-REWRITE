//! `tricky_game::course_resolve` (board row E3g) against the original in the function runner (TRICKY_ELF; skipped
//! without it): a resolver with a table of stand-in virtuals, random records for each step, the course's getters
//! stood in (answering pointers into the arena from their arguments), every call out logged on both sides and the
//! answers and memory compared.

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

const RODATA: (u32, u32) = (0x3a7000, 0x1000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x8000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const R: u32 = ARENA + 0x100;
const VTAB: u32 = ARENA + 0x200;
const COURSE: u32 = ARENA + 0x400;
const P: u32 = ARENA + 0x800;
const TABLE: u32 = ARENA + 0x1800;
const GOT: u32 = ARENA + 0x2000;
fn vf(off: u32) -> u32 {
    0x01f0_0000 + off
}
const VOFFS: [u32; 7] = [0x18, 0x58, 0x78, 0xc8, 0xd8, 0xa0, 0x110];
const GETTERS: [u32; 6] =
    [cr::LOOKUP_INSTANCE, cr::LOOKUP_PARTICLE, cr::GET_INSTANCE, cr::GET_MATERIAL, cr::GET_SPLINE_SEGMENT, cr::GET_SPLINE];

const FUNCS: [u32; 21] = [
    0x2521e8, 0x252220, 0x252278, 0x2522b8, 0x2522a0, 0x252408, 0x253400, 0x2534c8, 0x253540, 0x253b60, 0x2535b0,
    0x253bd0, 0x253b58, 0x253d30, 0x253e80, 0x253fb0, 0x254160, 0x2541e8, 0x254600, 0x254678, 0x253400,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32, a0: u32) -> usize {
    match f {
        cr::DEBUG_PRINT if a0 == 0x3a65a0 => 5,
        cr::DEBUG_PRINT if a0 == 0x3a6610 => 3,
        cr::EXTENDSFDF2 => 0,
        f if f == vf(0x18) => 1,
        cr::MEM_DELETE | cr::DEBUG_PRINT => 1,
        _ => 2,
    }
}
/// What a stand-in answers: the getters a record in the arena from their number, the double a mix of the float
fn answer(f: u32, w: &[u32], f12: u32) -> u32 {
    if GETTERS.contains(&f) {
        GOT + (w[1] & 0x1f) * 0x80
    } else if f == cr::EXTENDSFDF2 {
        f12.rotate_left(7) ^ 0x1234_5678
    } else {
        (f ^ w.first().copied().unwrap_or(0)).wrapping_mul(0x9e37) & 7
    }
}

struct Env {
    log: Vec<Call>,
}
impl ResolveEnv for Env {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        if func == 0x2522a0 {
            // the real table's +0x18: `Reset`, run (not stood in) on both sides
            cr::reset(m, args[0]);
            return 0;
        }
        let mut w = args.to_vec();
        w.resize(arity(func, w.first().copied().unwrap_or(0)), 0);
        let v = answer(func, &w, 0);
        self.log.push(Call(func, w));
        v
    }
    fn call_f(&mut self, _m: &mut Mem, func: u32, _args: &[u32], f12: f32) -> u32 {
        let w = vec![f12.to_bits()];
        let v = answer(func, &[], f12.to_bits());
        self.log.push(Call(func, w));
        v
    }
}

#[test]
fn course_resolver_steps_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let mut stubs = vec![cr::MEM_DELETE, cr::DEBUG_PRINT, cr::EXTENDSFDF2];
    stubs.extend(GETTERS);
    stubs.extend(VOFFS.map(vf));
    for func in stubs {
        let l = log.clone();
        r.stub(func, move |c, _| {
            let a0 = c.gpr(4) as u32;
            let mut w: Vec<u32> = (4..4 + arity(func, a0)).map(|k| c.gpr(k) as u32).collect();
            let f12 = c.f(12).to_bits();
            let v = answer(func, &w, f12);
            if func == cr::EXTENDSFDF2 {
                w.push(f12);
            }
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut rodata = vec![0u8; RODATA.1 as usize];
    r.mem.read(RODATA.0, &mut rodata).unwrap();
    r.allow(0x2522a0);
    let mut g = Rng(0xc0_e5e0_0000_0001);
    let mut calls = 0;
    for case in 0..FUNCS.len() as u32 * 400 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (RODATA.0, rodata.clone()),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        m.w32(R + 0x10, VTAB);
        for off in VOFFS {
            m.w16(VTAB + off, g.pick(&[0u16, 0, 0x10]));
            m.w32(VTAB + off + 4, vf(off));
        }
        m.w32(R + 8, TABLE);
        m.w32(R + 0xc, TABLE + 0x40);
        let big = g.next();
        m.w32(COURSE + 0x2c, g.pick(&[0, 7, 4000, 4001, u32::MAX, big]));
        let n = g.pick(&[0, 1, 2, 3, u32::MAX]);
        // the records each step walks
        let small = |g: &mut Rng| g.below(16);
        let neg_or = |g: &mut Rng| {
            let v = g.below(16);
            g.pick(&[u32::MAX, -5i32 as u32, v, v])
        };
        match func {
            0x253400 => {
                for k in 0..3 {
                    let p = P + 0x100 * k;
                    m.w32(p + 0xf0, small(&mut g));
                    for at in [0xc4, 0xc8] {
                        let v = neg_or(&mut g);
                        m.w32(p + at, v);
                    }
                }
            }
            0x2535b0 | 0x253bd0 => {
                let mut p = P;
                for _ in 0..3 {
                    let s = 4 * (g.below(16) + 1);
                    m.w32(p, s);
                    p += s;
                }
            }
            0x253d30 => {
                for k in 0..3 {
                    let p = P + 0x10 * k;
                    for at in [4, 8] {
                        let v = 4 * g.below(64);
                        m.w32(p + at, g.pick(&[0, v]));
                    }
                    let v = neg_or(&mut g);
                    m.w32(p + 0xc, v);
                }
            }
            0x253fb0 => {
                let mut p = P;
                for _ in 0..3 {
                    let c = g.pick(&[0, 1, 2, 4, u32::MAX]);
                    m.w32(p, c);
                    p += 4 * (c.min(8) + 1);
                }
            }
            0x2541e8 => {
                for k in 0..0x20 {
                    for at in [0x50, 0x54] {
                        let v = neg_or(&mut g);
                        m.w32(GOT + 0x80 * k + at, v);
                    }
                }
            }
            0x254160 => {
                for k in 0..0x20 {
                    m.w32(GOT + 0x80 * k + 0x20, small(&mut g));
                }
            }
            0x254678 => {
                let v = neg_or(&mut g);
                m.w32(P + 0x44, v);
            }
            _ => {}
        }
        let how = g.below(4);
        let base = g.next();
        let course = g.pick(&[0, COURSE, COURSE]);
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mm = &mut m;
        let (args, ours): ([u32; 4], Option<u32>) = match func {
            0x2521e8 => ([R, 0, 0, 0], Some(cr::construct(mm, e, R))),
            0x252220 => ([R, how, 0, 0], { cr::destruct(mm, e, R, how); None }),
            0x252278 => ([R, 0, 0, 0], { cr::vf1(mm, e, R); None }),
            0x2522b8 => ([R, 0, 0, 0], { cr::vf3(mm, e, R); None }),
            0x2522a0 => ([R, 0, 0, 0], { cr::reset(mm, R); None }),
            0x252408 => ([R, course, 0, 0], Some(cr::resolve_data_only(mm, e, R, course, TOP))),
            0x253400 => ([R, n, P, 0], Some(cr::resolve_instances(mm, e, R, n, P))),
            0x2534c8 => ([R, n, P, 0], Some(cr::resolve_particle_instances(mm, e, R, n, P))),
            0x253540 => ([R, n, P, base], Some(cr::resolve_pointers(mm, e, 0x3a6bb0, n, P, base))),
            0x253b60 => ([R, n, P, base], Some(cr::resolve_pointers(mm, e, 0x3a6d88, n, P, base))),
            0x2535b0 => ([R, n, P, 0], Some(cr::resolve_records(mm, e, R, 0x3a6bd0, 0x78, n, P))),
            0x253bd0 => ([R, n, P, 0], Some(cr::resolve_records(mm, e, R, 0x3a6db0, 0xc8, n, P))),
            0x253b58 => ([R, 0, 0, 0], Some(cr::resolve_mesh_context())),
            0x253d30 => ([R, n, P, 0], Some(cr::resolve_particle_hierarchies(mm, e, R, n, P))),
            0x253e80 => ([R, P, 0, 0], Some(cr::resolve_particle_mesh(mm, e, P))),
            0x253fb0 => ([R, n, P, 0], Some(cr::resolve_material_blocks(mm, e, R, n, P))),
            0x254160 => ([R, n, 0, 0], Some(cr::resolve_splines(mm, e, R, n))),
            0x2541e8 => ([R, n, 0, 0], Some(cr::resolve_spline_segments(mm, e, R, n))),
            0x254600 => ([R, n, P, 0], Some(cr::resolve_materials(mm, e, R, n, P))),
            0x254678 => ([R, P, 0, 0], Some(cr::vf33(mm, R, P))),
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
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    println!("{calls} calls");
    assert!(calls > 10000);
}
