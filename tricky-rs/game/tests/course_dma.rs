//! `tricky_game::course_dma` (board row E3i) against the original in the function runner (TRICKY_ELF; skipped
//! without it): random DMA chains (a CNT first, then CNTs and REFs to the three slots, an END last) patched and
//! rebuilt, random models, hierarchies, meshes and contexts resolved (their chains a lone END, which both
//! rewrites leave as it was, so a context may be met twice), the world's resolver made and destroyed; the copy,
//! the prints and the other calls out stood in and logged on both sides, the answers and memory compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::course_dma::{self as cd, DmaEnv};
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

const RODATA: (u32, u32) = (0x388000, 0x1000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x1_0000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const CHAIN: u32 = ARENA + 0x1000;
const MODELS: u32 = ARENA + 0x2000;
const HIER: u32 = ARENA + 0x3000;
const MESH: u32 = ARENA + 0x4000;
const LISTS: u32 = ARENA + 0x4800;
const CTX: u32 = ARENA + 0x5000;
const COURSE: u32 = ARENA + 0x6000;
const VTAB: u32 = ARENA + 0x6400;
const R: u32 = ARENA + 0x8000;
const VF_WORLD: u32 = 0x01f0_0024;
const VF1: u32 = 0x252278;
const MEM_FREE: u32 = 0x14a040;

const FUNCS: [u32; 12] = [
    0x255df0, 0x255e48, 0x255eb8, 0x255f08, 0x255ff0, 0x256080, 0x240a98, 0x240c78, 0x25ece8, 0x25ed70, 0x25ec60,
    0x25ecb8,
];
/// Run as the original inside the ones above
const INNER: [u32; 6] = [0x251ea8, 0x251ba0, 0x25ec88, 0x251ee0, 0x251bc8, 0x251d18];
const STUBS: [u32; 10] = [
    cd::DEBUG_PRINT,
    cd::EAMEM_COPY,
    cd::MEM_DELETE,
    MEM_FREE,
    cd::RESOLVE_PS2_CONSTRUCT,
    cd::COURSE_RESOLVE_DESTRUCT,
    cd::CELLLIST_DESTRUCT,
    VF1,
    VF_WORLD,
    0,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32) -> usize {
    match f {
        cd::EAMEM_COPY => 3,
        cd::COURSE_RESOLVE_DESTRUCT | cd::CELLLIST_DESTRUCT => 2,
        _ => 1,
    }
}

/// The copy, forward a byte at a time, the same on both sides
fn copy(get: &mut dyn FnMut(u32) -> u8, put: &mut dyn FnMut(u32, u8), w: &[u32]) {
    for k in 0..w[2] {
        let b = get(w[1].wrapping_add(k));
        put(w[0].wrapping_add(k), b);
    }
}

struct Env {
    log: Vec<Call>,
}
impl DmaEnv for Env {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func), 0);
        if func == cd::EAMEM_COPY {
            let mm = RefCell::new(&mut *m);
            copy(&mut |a| mm.borrow().r8(a), &mut |a, b| mm.borrow_mut().w8(a, b), &w);
        }
        self.log.push(Call(func, w));
        0
    }
}

/// A random DMA tag: its id, count and address, with random bits elsewhere in its low word
fn tag(g: &mut Rng, id: u32, qwc: u32, addr: u32) -> u64 {
    let junk = g.next() & 0x8fff_0000;
    ((addr as u64) << 32) | (junk | id << 28 | qwc) as u64
}
fn w64(m: &mut Mem, a: u32, v: u64) {
    m.w32(a, v as u32);
    m.w32(a + 4, (v >> 32) as u32);
}

/// A chain the rewrites end on: a CNT first, then CNTs, REFs to `a` / `b`, REFs to `c` each followed by a CNT or
/// the END, then the END
fn chain(m: &mut Mem, g: &mut Rng, at: u32, slots: [u32; 3]) {
    let mut p = at;
    let mut put = |m: &mut Mem, g: &mut Rng, id: u32, addr: u32| {
        let qwc = if id == 3 { g.below(0x10000) } else { g.pick(&[0, 1, 2, 3]) };
        let t = tag(g, id, qwc, addr);
        w64(m, p, t);
        p += 0x10 + if id == 3 { 0 } else { qwc * 0x10 };
    };
    let junk = g.next();
    put(m, g, 1, junk);
    for _ in 0..g.below(7) {
        match g.below(4) {
            0 => {
                let junk = g.next();
                put(m, g, 1, junk)
            }
            1 => put(m, g, 3, slots[0]),
            2 => put(m, g, 3, slots[1]),
            _ => {
                // what follows a REF to `c` becomes the next run's tag: a CNT, or the END
                put(m, g, 3, slots[2]);
                if g.below(3) == 0 {
                    break;
                }
                let junk = g.next();
                put(m, g, 1, junk);
            }
        }
    }
    let junk = g.next();
    put(m, g, 6, junk);
}

#[test]
fn course_dma_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS.iter().chain(INNER.iter()) {
        r.allow(*f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for func in STUBS.into_iter().filter(|&f| f != 0) {
        let l = log.clone();
        r.stub(func, move |c, mem| {
            let w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            if func == cd::EAMEM_COPY {
                let mm = RefCell::new(&mut *mem);
                let get = &mut |a: u32| {
                    let mut b = [0u8];
                    mm.borrow_mut().read(a, &mut b).unwrap();
                    b[0]
                };
                copy(get, &mut |a, b| mm.borrow_mut().write(a, &[b]).unwrap(), &w);
            }
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, 0);
        });
    }
    let mut rodata = vec![0u8; RODATA.1 as usize];
    r.mem.read(RODATA.0, &mut rodata).unwrap();
    let mut g = Rng(0xd3a0_c4a1_0000_0001);
    let mut calls = 0;
    for case in 0..FUNCS.len() as u32 * 500 {
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
        // the three slots (some equal)
        let s0 = g.below(0x400) * 0x10;
        let o = g.below(0x400) * 0x10;
        let s1 = g.pick(&[s0, o, 0x10]);
        let o = g.below(0x400) * 0x10;
        let s2 = g.pick(&[s0, s1, o, 0x20]);
        let slots = [s0, s1, s2];
        chain(&mut m, &mut g, CHAIN, slots);
        // models → hierarchies → meshes → contexts, each context's chain a lone END
        for k in 0..4 {
            let ctx = CTX + 0x40 * k;
            let end = CTX + 0x40 * k + 0x20;
            m.w32(ctx + 0xc, end);
            let junk = g.next();
            let t = tag(&mut g, 6, 0, junk);
            w64(&mut m, end, t);
        }
        for j in 0..4 {
            let mesh = MESH + 0x40 * j;
            m.w32(mesh + 0x20, g.pick(&[0, 1, 2, 3, u32::MAX]));
            m.w32(mesh + 0x28, LISTS + 0x10 * j);
            for k in 0..3 {
                let ctx = CTX + 0x40 * g.below(4);
                m.w32(LISTS + 0x10 * j + 4 * k, ctx);
            }
        }
        for h in 0..12 {
            for at in [4, 8, 0xc] {
                let mesh = MESH + 0x40 * g.below(4);
                m.w32(HIER + 0x18 * h + at, g.pick(&[0, mesh, mesh]));
            }
        }
        let mut model = MODELS;
        for k in 0..4 {
            let size = g.pick(&[0x20, 0x30]);
            m.w32(model, size);
            m.w32(model + 4, g.pick(&[0, 1, 2, 3, u32::MAX]));
            m.w32(model + 8, HIER + 0x18 * 3 * k);
            let flags = g.next();
            m.w32(model + 0x10, flags);
            model += size;
        }
        m.w32(COURSE + 0x2c, g.pick(&[0, 1, 2, 3, 4, 4000, 4001, u32::MAX]));
        m.w32(COURSE + 0x6c, g.pick(&[0, MODELS, MODELS]));
        if m.r32(COURSE + 0x2c) == 4000 {
            // 4000 is still resolved (over 4000 is refused): no models then, so as not to walk 4000
            m.w32(COURSE + 0x6c, 0);
        }
        // the world: a table of a stand-in virtual
        m.w32(R + 0x10, VTAB);
        m.w16(VTAB + 0x20, g.pick(&[0u16, 0x10]));
        m.w32(VTAB + 0x24, VF_WORLD);
        let n = g.pick(&[0, 1, 2, 3, 4, u32::MAX]);
        let flags = g.next();
        let how = g.below(4);
        let course = g.pick(&[0, COURSE, COURSE]);
        let ctx = CTX + 0x40 * g.below(4);
        let i = g.below(0x100);
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mm = &mut m;
        let (args, ours): ([u32; 6], Option<u32>) = match func {
            0x255df0 => ([R, course, 0, 0, 0, 0], Some(cd::try_resolve_models(mm, e, R, course))),
            0x255e48 => ([R, n, MODELS, 0, 0, 0], Some(cd::resolve_models(mm, e, R, n, MODELS))),
            0x255eb8 => ([R, MODELS, 0, 0, 0, 0], Some(cd::resolve_model(mm, e, R, MODELS))),
            0x255f08 => ([R, n, HIER, flags, 0, 0], Some(cd::resolve_model_hierarchies(mm, e, R, n, HIER, flags))),
            0x255ff0 => ([R, MESH, flags, 0, 0, 0], Some(cd::resolve_mesh(mm, e, R, MESH, flags))),
            0x256080 => ([R, ctx, flags, 0, 0, 0], Some(cd::resolve_mesh_context(mm, e, R, ctx, flags))),
            0x240a98 => ([CHAIN, s0, s1, s2, 0, 0], { cd::patch_context(mm, e, CHAIN, s0, s1, s2); None }),
            0x240c78 => ([CHAIN, s0, s1, s2, 0, 0], { cd::rebuild_chain(mm, e, CHAIN, s0, s1, s2); None }),
            0x25ece8 => ([R, 0, 0, 0, 0, 0], Some(cd::construct(mm, e, R))),
            0x25ed70 => ([R, how, 0, 0, 0, 0], { cd::world_destruct(mm, e, R, how); None }),
            0x25ec60 => ([R + 0x3c, 0, 0, 0, 0, 0], Some(cd::vis_construct(mm, R + 0x3c))),
            0x25ecb8 => ([R, i, 0, 0, 0, 0], Some(cd::vis_get(mm, R, i))),
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
