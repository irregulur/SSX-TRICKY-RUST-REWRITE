//! `tricky_game::particleman` (board row E1b2g3a) against the original in the function runner (TRICKY_ELF;
//! skipped without it): `ParticleMan_Init` 0x1d8e30, `cParticleMan::~cParticleMan` 0x1d8f50, `PoolList_Pop`
//! 0x1d8fd8, `PoolList_Push` 0x1d9018, `ParticleGroup_Add` 0x1d9060, `ParticleGroup_Get` 0x1d9108,
//! `ParticleGroup_Remove` 0x1d9138, `ParticleGroup_UpdateTrail` 0x1d9208, `ParticleMan_DrawSlot` 0x1d9700,
//! and `cParticleMan::vf2` 0x1d9900.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::particleman as pm;
use tricky_game::pf::Mem;
use tricky_game::riderdraw::{RiderDrawEnv, APP};

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
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x1_0000;
const RENDER: u32 = ARENA;
const RVT: u32 = ARENA + 0x400;
const APPOBJ: u32 = ARENA + 0x800;
const TEX: u32 = ARENA + 0xc00;
const PMAN: u32 = ARENA + 0x1000;
const CFG: u32 = ARENA + 0x2000;
const POS: u32 = ARENA + 0x2200;
const VEL: u32 = ARENA + 0x2210;
const POOL: u32 = ARENA + 0x3000; // 32,000 bytes (0x7d00) -> ends at 0xad00 < 0x10000
const SCRATCH: u32 = 0x0180_0000;

const VSLOTS: [u32; 6] = [0x1c0, 0x1c8, 0x1e0, 0x1e8, 0x1f0, 0x240];
fn vaddr(off: u32) -> u32 {
    0x01ff_0000 + off
}

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn logged_vcall(off: u32, w: &[u32], f12: u32, read: &mut dyn FnMut(u32) -> u32) -> Vec<u32> {
    let mut v = vec![w[0]];
    match off {
        0x1c8 => v.extend([w[1], w[2], f12]),
        0x1c0 | 0x1e0 => v.push(w[1]),
        0x240 => {
            // (this, pos[4], size[2], uv0[2], uv1[2], colour[4])
            v.extend((0..4).map(|k| read(w[1] + 4 * k)));
            v.extend((0..2).map(|k| read(w[2] + 4 * k)));
            v.extend((0..2).map(|k| read(w[3] + 4 * k)));
            v.extend((0..2).map(|k| read(w[4] + 4 * k)));
            v.extend((0..4).map(|k| read(w[5] + 4 * k)));
        }
        _ => v.extend([w[1], w[2]]),
    }
    v
}

struct OursDraw(Vec<Call>);
impl RiderDrawEnv for OursDraw {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.0.push(Call(func, args.to_vec()));
        0
    }
    fn vcall(&mut self, m: &mut Mem, func: u32, this: u32, args: &[u32]) -> u32 {
        let off = func - vaddr(0);
        let mut w = vec![this];
        w.extend(args);
        let f12 = if off == 0x1c8 { args[2] } else { 0 };
        w.resize(6, 0);
        let mut read = |a: u32| m.r32(a);
        self.0.push(Call(func, logged_vcall(off, &w, f12, &mut read)));
        0
    }
}

struct OursMem {
    pool: u32,
    log: Vec<Call>,
}
impl pm::ParticleMemEnv for OursMem {
    fn bx_ctor(&mut self, _m: &mut Mem, this: u32) {
        self.log.push(Call(pm::BX_OBJ_NODE_CTOR, vec![this]));
    }
    fn bx_dtor(&mut self, _m: &mut Mem, this: u32, flags: u32) {
        self.log.push(Call(pm::BX_OBJ_NODE_DTOR, vec![this, flags]));
    }
    fn alloc(&mut self, _m: &mut Mem, tag: u32, bytes: u32, align: u32) -> u32 {
        self.log.push(Call(pm::EA_MEM_ALLOC, vec![tag, bytes, align]));
        self.pool
    }
    fn mem_set(&mut self, m: &mut Mem, ptr: u32, pat: u32, bytes: u32) {
        self.log.push(Call(pm::EA_MEM_SET, vec![ptr, pat, bytes]));
        for off in (0..bytes).step_by(4) {
            m.w32(ptr + off, pat);
        }
    }
    fn free(&mut self, _m: &mut Mem, ptr: u32) {
        self.log.push(Call(pm::MEM_FREE, vec![ptr]));
    }
}

fn fresh(g: &mut Rng) -> Mem {
    let mut m = Mem {
        regions: vec![
            (ARENA, vec![0; SIZE as usize]),
            (APP, vec![0; 4]),
            (SCRATCH, vec![0; 0x200]),
        ],
    };
    for k in 0..SIZE / 4 {
        m.w32(ARENA + 4 * k, g.next());
    }
    m.w32(RENDER + 0x10, RVT);
    for off in VSLOTS {
        m.w16(RVT + off, [0u16, 0x10, 0xfff0][g.below(3) as usize]);
        m.w32(RVT + off + 4, vaddr(off));
    }
    m.w32(APP, APPOBJ);
    m.w32(APPOBJ + 0x724, RENDER);
    m.w32(APPOBJ + 0x30, TEX);
    m
}

fn sync_to_runner(r: &mut Runner, m: &Mem) {
    for (at, b) in &m.regions {
        if *at != SCRATCH {
            r.mem.write(*at, b).unwrap();
        }
    }
}

fn assert_mem_eq(r: &mut Runner, m: &Mem, label: &str) {
    for (at, b) in &m.regions {
        if *at == SCRATCH {
            continue;
        }
        let mut th = vec![0u8; b.len()];
        r.mem.read(*at, &mut th).unwrap();
        if &th != b {
            let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
            let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
            panic!("{label}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
        }
    }
}

fn wf(m: &mut Mem, a: u32, v: f32) {
    m.w32(a, v.to_bits());
}

fn rand_cfg(m: &mut Mem, g: &mut Rng, at: u32) {
    wf(m, at + 0x00, g.f(-0.5, 4.0)); // rate
    wf(m, at + 0x04, g.f(0.0, 2.0)); // rate_var
    wf(m, at + 0x08, g.f(-0.5, 1.5)); // accum
    m.w32(at + 0x0c, g.below(10)); // life (including 0 to hit PS2 div-by-zero!)
    m.w32(at + 0x10, g.below(5)); // life_var
    for off in [0x20, 0x30, 0x40, 0x50, 0x80, 0xa0, 0xb0, 0xc0] {
        for k in 0..4u32 {
            wf(m, at + off + 4 * k, g.f(-20.0, 20.0));
        }
    }
    for off in [0x60, 0x64, 0x68, 0x6c, 0x70, 0xd0] {
        wf(m, at + off, g.f(-2.0, 5.0));
    }
    m.w32(at + 0x90, g.next()); // texture ptr
}

fn setup_runner_base() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    r.allow(pm::VEC4_SCALE);
    r.allow(0x001d_8fd8); // PoolList_Pop
    r.allow(0x001d_9018); // PoolList_Push
    r.allow(pm::EA_MEM_COPY);
    Some(r)
}

#[test]
fn init_destroy_pool_and_groups_match_the_original() {
    let Some(mut r) = setup_runner_base() else { return };
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let pool_ret = Rc::new(RefCell::new(POOL));

    {
        let l = log.clone();
        r.stub(pm::BX_OBJ_NODE_CTOR, move |c, _| l.borrow_mut().push(Call(pm::BX_OBJ_NODE_CTOR, vec![c.gpr(4) as u32])));
    }
    {
        let l = log.clone();
        r.stub(pm::BX_OBJ_NODE_DTOR, move |c, _| l.borrow_mut().push(Call(pm::BX_OBJ_NODE_DTOR, vec![c.gpr(4) as u32, c.gpr(5) as u32])));
    }
    {
        let l = log.clone();
        let p = pool_ret.clone();
        r.stub(pm::EA_MEM_ALLOC, move |c, _| {
            l.borrow_mut().push(Call(pm::EA_MEM_ALLOC, vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32]));
            c.set_gpr(2, *p.borrow() as u64);
        });
    }
    {
        let l = log.clone();
        r.stub(pm::EA_MEM_SET, move |c, mem| {
            let (ptr, pat, bytes) = (c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32);
            l.borrow_mut().push(Call(pm::EA_MEM_SET, vec![ptr, pat, bytes]));
            for off in (0..bytes).step_by(4) {
                mem.write_u32(ptr + off, pat);
            }
        });
    }
    {
        let l = log.clone();
        r.stub(pm::MEM_FREE, move |c, _| l.borrow_mut().push(Call(pm::MEM_FREE, vec![c.gpr(4) as u32])));
    }

    let mut g = Rng(0x1d8e_3000_0000_0001);
    // 1. ParticleMan_Init + pool pop/push + group add/get/remove + destroy
    let mut m = fresh(&mut g);
    sync_to_runner(&mut r, &m);
    log.borrow_mut().clear();
    let mut env = OursMem { pool: POOL, log: Vec::new() };
    let ret_ours = pm::init(&mut m, &mut env, PMAN);
    r.cpu.set_gpr(4, PMAN as u64);
    r.call(0x001d_8e30).unwrap();
    assert_eq!(r.cpu.gpr(2) as u32, ret_ours);
    assert_eq!(*log.borrow(), env.log);
    assert_mem_eq(&mut r, &m, "init");

    // Exhaust the entire 250-node pool with PoolList_Pop, then pop once more when empty
    let mut popped = Vec::new();
    for step in 0..255 {
        let o = pm::pool_pop(&mut m, PMAN);
        r.cpu.set_gpr(4, PMAN as u64);
        r.call(0x001d_8fd8).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, o, "pop {step}");
        assert_mem_eq(&mut r, &m, &format!("pop {step}"));
        if o != 0 {
            popped.push(o);
        }
    }
    assert_eq!(popped.len(), 250);

    // Push all 250 back in shuffled order
    for (step, &node) in popped.iter().rev().enumerate() {
        let o = pm::pool_push(&mut m, PMAN, node);
        r.cpu.set_gpr(4, PMAN as u64);
        r.cpu.set_gpr(5, node as u64);
        r.call(0x001d_9018).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, o, "push {step}");
        if step < 10 || step > 245 {
            assert_mem_eq(&mut r, &m, &format!("push {step}"));
        }
    }
    assert_mem_eq(&mut r, &m, "push all");

    // Add 14 groups (12 succeed, 2 return -1), test get on 0..15, remove random order
    for step in 0..14 {
        rand_cfg(&mut m, &mut g, CFG);
        sync_to_runner(&mut r, &m);
        let o = pm::group_add(&mut m, PMAN, CFG);
        r.cpu.set_gpr(4, PMAN as u64);
        r.cpu.set_gpr(5, CFG as u64);
        r.call(0x001d_9060).unwrap();
        assert_eq!(r.cpu.gpr(2) as i32, o, "add {step}");
        assert_mem_eq(&mut r, &m, &format!("add {step}"));
    }
    for slot in 0..15u32 {
        let o = pm::group_get(&m, PMAN, slot);
        r.cpu.set_gpr(4, PMAN as u64);
        r.cpu.set_gpr(5, slot as u64);
        r.call(0x001d_9108).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, o, "get {slot}");
    }
    // Remove slot 5, re-add (should pick slot 5), remove all
    for slot in [5, 0, 11, 5, 1, 2, 3, 4, 6, 7, 8, 9, 10, 0] {
        let o = pm::group_remove(&mut m, PMAN, slot);
        r.cpu.set_gpr(4, PMAN as u64);
        r.cpu.set_gpr(5, slot as u64);
        r.call(0x001d_9138).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, o, "remove {slot}");
        assert_mem_eq(&mut r, &m, &format!("remove {slot}"));
    }

    // Destroy both non-null and null pool
    for (case, null_pool) in [(0, false), (1, true)] {
        if null_pool {
            m.w32(PMAN + 0xb50, 0);
            sync_to_runner(&mut r, &m);
        }
        log.borrow_mut().clear();
        let mut env = OursMem { pool: POOL, log: Vec::new() };
        pm::destroy(&mut m, &mut env, PMAN, 3);
        r.cpu.set_gpr(4, PMAN as u64);
        r.cpu.set_gpr(5, 3);
        r.call(0x001d_8f50).unwrap();
        assert_eq!(*log.borrow(), env.log, "destroy {case}");
        assert_mem_eq(&mut r, &m, &format!("destroy {case}"));
    }
}

#[test]
fn update_trail_matches_the_original() {
    let Some(mut r) = setup_runner_base() else { return };
    let draws: Rc<RefCell<Vec<u32>>> = Rc::default();
    let d_stub = draws.clone();
    r.stub(pm::RAND_FX, move |c, _| {
        let v = d_stub.borrow_mut().remove(0);
        c.set_gpr(2, v as u64);
    });
    // Stub init helpers to build a clean ParticleMan
    r.stub(pm::BX_OBJ_NODE_CTOR, |_, _| {});
    r.stub(pm::EA_MEM_ALLOC, move |c, _| c.set_gpr(2, POOL as u64));
    r.stub(pm::EA_MEM_SET, |c, mem| {
        let (ptr, pat, bytes) = (c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32);
        for off in (0..bytes).step_by(4) {
            mem.write_u32(ptr + off, pat);
        }
    });

    let mut g = Rng(0x1d92_0800_cafe_babe);
    let mut m = fresh(&mut g);
    let mut menv = OursMem { pool: POOL, log: Vec::new() };
    pm::init(&mut m, &mut menv, PMAN);
    sync_to_runner(&mut r, &m);

    // Add 4 active slots with varied rates, lifetimes (including 0 and 1), colours, drag, gravity
    for s in 0..4 {
        rand_cfg(&mut m, &mut g, CFG);
        if s == 0 {
            // high rate to test pool exhaustion!
            wf(&mut m, CFG + 0x00, 35.0);
            wf(&mut m, CFG + 0x04, 5.0);
            m.w32(CFG + 0x0c, 6);
            m.w32(CFG + 0x10, 3);
        } else if s == 1 {
            // low rate, life 0/1 to test immediate retirement and div-by-zero
            wf(&mut m, CFG + 0x00, 1.5);
            wf(&mut m, CFG + 0x04, 1.5);
            m.w32(CFG + 0x0c, 1);
            m.w32(CFG + 0x10, 1);
        }
        pm::group_add(&mut m, PMAN, CFG);
    }
    sync_to_runner(&mut r, &m);

    let mut total_spawned = 0usize;
    let mut hit_exhaustion = false;
    let mut hit_inactive = 0usize;
    for tick in 0..160 {
        let slot = (tick % 6) as i32; // slots 4..5 are inactive
        for k in 0..4u32 {
            wf(&mut m, POS + 4 * k, g.f(-500.0, 500.0));
            wf(&mut m, VEL + 4 * k, g.f(-50.0, 50.0));
        }
        sync_to_runner(&mut r, &m);

        // Generate a batch of random draws shared identically by both
        let seq: Vec<u32> = (0..1500).map(|_| g.next()).collect();
        *draws.borrow_mut() = seq.clone();
        let mut idx = 0usize;
        let free_before = m.r32(PMAN + 0xb54);
        let ret_ours = pm::update_trail(&mut m, PMAN, slot, POS, VEL, &mut || {
            let v = seq[idx];
            idx += 1;
            v
        });
        let free_after = m.r32(PMAN + 0xb54);
        if free_after == 0 {
            hit_exhaustion = true;
        }
        if free_before > free_after {
            total_spawned += (free_before - free_after) as usize;
        }
        if ret_ours == 0 {
            hit_inactive += 1;
        }

        r.cpu.set_gpr(4, PMAN as u64);
        r.cpu.set_gpr(5, slot as u64);
        r.cpu.set_gpr(6, POS as u64);
        r.cpu.set_gpr(7, VEL as u64);
        r.call(0x001d_9208).unwrap_or_else(|e| panic!("tick {tick}: {e}"));

        assert_eq!(r.cpu.gpr(2) as u32, ret_ours, "tick {tick} ret");
        assert_eq!(seq.len() - draws.borrow().len(), idx, "tick {tick} draw count");
        assert_mem_eq(&mut r, &m, &format!("tick {tick} slot {slot}"));

        // Periodically remove and re-add a slot with live particles (tests ParticleGroup_Remove with live list!)
        if tick == 80 || tick == 130 {
            let rm_slot = if tick == 80 { 0 } else { 2 };
            let ro = pm::group_remove(&mut m, PMAN, rm_slot);
            r.cpu.set_gpr(4, PMAN as u64);
            r.cpu.set_gpr(5, rm_slot as u64);
            r.call(0x001d_9138).unwrap();
            assert_eq!(r.cpu.gpr(2) as u32, ro, "remove live {tick}");
            assert_mem_eq(&mut r, &m, &format!("remove live {tick}"));
        }
    }
    assert!(total_spawned > 200, "spawned {total_spawned}");
    assert!(hit_exhaustion, "pool exhaustion was exercised");
    assert!(hit_inactive > 0, "inactive guard was exercised");
}

#[test]
fn draw_slot_and_vf2_match_the_original() {
    let Some(mut r) = setup_runner_base() else { return };
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for off in VSLOTS {
        let l = log.clone();
        r.stub(vaddr(off), move |c, mem| {
            let w: Vec<u32> = (4..10).map(|k| c.gpr(k) as u32).collect();
            let mut read = |a: u32| mem.read_u32(a);
            l.borrow_mut().push(Call(vaddr(off), logged_vcall(off, &w, c.fpr[12], &mut read)));
        });
    }

    let mut g = Rng(0x1d97_0000_0000_0001);
    let mut m = fresh(&mut g);
    let mut menv = OursMem { pool: POOL, log: Vec::new() };
    pm::init(&mut m, &mut menv, PMAN);
    for _s in 0..3 {
        rand_cfg(&mut m, &mut g, CFG);
        wf(&mut m, CFG + 0x00, 4.0);
        wf(&mut m, CFG + 0x04, 1.0);
        m.w32(CFG + 0x0c, 5);
        m.w32(CFG + 0x10, 1);
        pm::group_add(&mut m, PMAN, CFG);
    }
    // Spawn some particles into slots 0..3
    for tick in 0..10 {
        for k in 0..4u32 {
            wf(&mut m, POS + 4 * k, g.f(-100.0, 100.0));
            wf(&mut m, VEL + 4 * k, g.f(-10.0, 10.0));
        }
        pm::update_trail(&mut m, PMAN, tick % 3, POS, VEL, &mut || g.next());
    }
    sync_to_runner(&mut r, &m);

    // 1. ParticleMan_DrawSlot on active (0..3) and inactive (3..5) slots
    for slot in 0..5 {
        log.borrow_mut().clear();
        let mut o = OursDraw(Vec::new());
        let ro = pm::draw_slot(&mut m, &mut o, PMAN, slot, SCRATCH);
        r.cpu.set_gpr(4, PMAN as u64);
        r.cpu.set_gpr(5, slot as u64);
        r.call(0x001d_9700).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, ro, "draw_slot {slot} ret");
        assert_eq!(*log.borrow(), o.0, "draw_slot {slot} calls");
        assert_mem_eq(&mut r, &m, &format!("draw_slot {slot}"));
    }

    // 2. cParticleMan::vf2 (0x1d9900) with DrawSlot stubbed to log slot calls
    let l = log.clone();
    r.stub(pm::DRAW_SLOT, move |c, _| {
        l.borrow_mut().push(Call(pm::DRAW_SLOT, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
    });
    for (case, clear_active) in [(0, false), (1, true)] {
        if clear_active {
            m.w32(PMAN + 0xc, 0);
            sync_to_runner(&mut r, &m);
        }
        log.borrow_mut().clear();
        let mut o = OursDraw(Vec::new());
        pm::draw(&mut m, &mut o, PMAN);
        r.cpu.set_gpr(4, PMAN as u64);
        r.call(0x001d_9900).unwrap();
        assert_eq!(*log.borrow(), o.0, "vf2 case {case}");
        assert_mem_eq(&mut r, &m, &format!("vf2 case {case}"));
    }
}
