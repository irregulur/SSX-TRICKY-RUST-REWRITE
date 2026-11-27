//! The last 7 `wipeout` functions (board row E8u) against `SLUS_203.26` in the function runner
//! (`TRICKY_ELF`; skipped without it):
//! - `CollBody_Construct` 0x235fc8
//! - `CollBody_ConstructCopy` 0x236000
//! - `CollBody_SetSkeleton` 0x236128
//! - `CollBody_Copy` 0x236130
//! - `CollBody_GetSphereBounds` 0x236758
//! - `Ragdoll_OnContact` 0x10ebc8
//! - `Ragdoll_HitNode` 0x10ebd0

use std::cell::RefCell;
use std::rc::Rc;

use r5900::ps2float::Rules;
use r5900::{Mem as RMem, Runner};
use tricky_game::collbody::{self, CollBody};
use tricky_game::pf::Mem;
use tricky_game::ragdoll::{self, CastOut, NodeHitEnv, RagRider, Ragdoll};
use tricky_game::rigidbody::RigidBody;
use tricky_game::walls::Contact;
use tricky_game::worldboarder::WorldBoarder;

type V4 = [f32; 4];

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
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
    fn v(&mut self, k: f32) -> V4 {
        [self.f(-k, k), self.f(-k, k), self.f(-k, k), self.pick(&[0.0f32, 0.0, 1.0])]
    }
    fn unit(&mut self) -> V4 {
        let v = [self.f(-1.0, 1.0), self.f(-1.0, 1.0), self.f(-1.0, 1.0) + 0.001];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l, self.pick(&[0.0f32, 0.0, 1.0])]
    }
}

fn w4(m: &mut RMem, a: u32, v: V4) {
    for k in 0..4u32 {
        m.write_f32(a + 4 * k, v[k as usize]);
    }
}
fn r4(m: &mut RMem, a: u32) -> V4 {
    [0, 1, 2, 3].map(|k| m.read_f32(a + 4 * k))
}
fn b4(v: V4) -> [u32; 4] {
    v.map(f32::to_bits)
}

fn put_body(m: &mut RMem, at: u32, b: &CollBody) {
    w4(m, at + 0x10, b.center);
    m.write_f32(at + 0x20, b.radius);
    m.write_u32(at + 0x24, b.x24 as u32);
    m.write_u32(at + 0x28, b.count as u32);
    for (k, &(c, r)) in b.spheres.iter().enumerate() {
        w4(m, at + 0x30 + 0x20 * k as u32, c);
        m.write_f32(at + 0x40 + 0x20 * k as u32, r);
    }
}

fn put_rb(m: &mut RMem, at: u32, rb: &RigidBody) {
    m.write_f32(at + 0x8, rb.inv_mass);
    for k in 0..9u32 {
        m.write_f32(at + 0xc + 4 * k, rb.inv_inertia[k as usize]);
    }
    w4(m, at + 0x30, rb.pos);
    w4(m, at + 0x40, rb.q);
    w4(m, at + 0x50, rb.p);
    w4(m, at + 0x60, rb.l);
    w4(m, at + 0x70, rb.v);
    w4(m, at + 0x80, rb.w);
}

fn get_rb(m: &mut RMem, at: u32) -> RigidBody {
    let mut inv_inertia = [0.0f32; 9];
    for k in 0..9u32 {
        inv_inertia[k as usize] = m.read_f32(at + 0xc + 4 * k);
    }
    RigidBody {
        inv_mass: m.read_f32(at + 0x8),
        inv_inertia,
        pos: r4(m, at + 0x30),
        q: r4(m, at + 0x40),
        p: r4(m, at + 0x50),
        l: r4(m, at + 0x60),
        v: r4(m, at + 0x70),
        w: r4(m, at + 0x80),
    }
}

fn put_cast_out(m: &mut RMem, at: u32, out: &CastOut) {
    m.write_u32(at, out.kind);
    w4(m, at + 0x10, out.pos);
    w4(m, at + 0x20, out.normal);
    for k in 0..3u32 {
        m.write_u32(at + 0x30 + 4 * k, out.x30[k as usize]);
    }
}

fn read_cast_out(m: &mut RMem, at: u32) -> CastOut {
    CastOut {
        kind: m.read_u32(at),
        pos: r4(m, at + 0x10),
        normal: r4(m, at + 0x20),
        x30: [m.read_u32(at + 0x30), m.read_u32(at + 0x34), m.read_u32(at + 0x38)],
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    for a in [
        0x235fc8u32, 0x236000, 0x236128, 0x236130, 0x236190, 0x236758, 0x236830, 0x102f50, 0x2ce114, 0x10ebc8,
        0x10ebd0, 0x153ee8, 0x154338, 0x257810, 0x257ca0,
    ] {
        r.allow(a);
    }
    Some(r)
}

#[test]
fn collbody_construct_copy_skeleton_and_bounds_match_the_original() {
    let Some(mut r) = runner() else { return };
    let base = 0x0100_0000u32;
    let dst = base;
    let src = base + 0x300;
    let out = base + 0x600;
    let mut g = Rng(0x235f_c800_0001);

    // 1. CollBody_Construct (0x235fc8) and CollBody_SetSkeleton (0x236128)
    for case in 0..500u32 {
        let mut init = vec![0u8; 0x280];
        for b in &mut init {
            *b = g.next() as u8;
        }
        r.mem.write(dst, &init).unwrap();
        r.cpu.set_gpr(4, dst as u64);
        r.call(0x235fc8).unwrap();
        let ret = r.cpu.gpr(2) as u32;
        assert_eq!(collbody::construct_bytes(dst), ret, "Construct ret case {case}");
        let mut after = vec![0u8; 0x280];
        r.mem.read(dst, &mut after).unwrap();
        assert_eq!(after, init, "Construct must leave bytes untouched case {case}");

        let skel = g.next();
        let mut m = Mem { regions: vec![(dst, init.clone())] };
        collbody::set_skeleton(&mut m, dst, skel);
        r.cpu.set_gpr(4, dst as u64);
        r.cpu.set_gpr(5, skel as u64);
        r.call(0x236128).unwrap();
        r.mem.read(dst, &mut after).unwrap();
        assert_eq!(m.regions[0].1, after, "SetSkeleton case {case}");
    }

    // 2. CollBody_Copy (0x236130) and CollBody_ConstructCopy (0x236000)
    for case in 0..1_000u32 {
        let func = if case < 500 { 0x236130 } else { 0x236000 };
        let mut dst_init = vec![0u8; 0x280];
        let mut src_init = vec![0u8; 0x280];
        for b in &mut dst_init {
            *b = g.next() as u8;
        }
        for b in &mut src_init {
            *b = g.next() as u8;
        }
        let mut m = Mem { regions: vec![(dst, dst_init.clone()), (src, src_init.clone())] };
        let ours_ret = if func == 0x236130 {
            collbody::copy_bytes(&mut m, dst, src)
        } else {
            collbody::construct_copy_bytes(&mut m, dst, src)
        };
        r.mem.write(dst, &dst_init).unwrap();
        r.mem.write(src, &src_init).unwrap();
        r.cpu.set_gpr(4, dst as u64);
        r.cpu.set_gpr(5, src as u64);
        r.call(func).unwrap();
        let orig_ret = r.cpu.gpr(2) as u32;
        assert_eq!(ours_ret, orig_ret, "{func:#x} ret case {case}");
        let mut orig_dst = vec![0u8; 0x280];
        r.mem.read(dst, &mut orig_dst).unwrap();
        assert_eq!(m.regions[0].1, orig_dst, "{func:#x} mem case {case}");
    }

    // 3. CollBody_GetSphereBounds (0x236758): counts -2..=18, random spheres & radii
    for case in 0..5_000u32 {
        let mut body = CollBody::new();
        body.center = g.v(1000.0);
        body.radius = g.f(0.0, 200.0);
        body.x24 = g.next() as i32;
        body.count = match case % 25 {
            0 => -2,
            1 => -1,
            2 => 0,
            3 => 1,
            _ => (g.below(19)) as i32,
        };
        for s in &mut body.spheres {
            s.0 = g.v(1200.0);
            s.1 = g.f(-50.0, 200.0);
        }
        let copied = CollBody::construct_copy(&body);
        let mut copied2 = CollBody::new();
        copied2.copy_from(&body);
        assert_eq!(copied, body);
        assert_eq!(copied2, body);

        put_body(&mut r.mem, src, &body);
        w4(&mut r.mem, out, [123.0; 4]);
        w4(&mut r.mem, out + 0x10, [456.0; 4]);
        r.cpu.set_gpr(4, src as u64);
        r.cpu.set_gpr(5, out as u64);
        r.call(0x236758).unwrap();
        let orig_lo = r4(&mut r.mem, out);
        let orig_hi = r4(&mut r.mem, out + 0x10);
        let (our_lo, our_hi) = body.sphere_bounds();
        assert_eq!(b4(our_lo), b4(orig_lo), "GetSphereBounds min case {case} count={}", body.count);
        assert_eq!(b4(our_hi), b4(orig_hi), "GetSphereBounds max case {case} count={}", body.count);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum HitEv {
    RayCast(u32, u32, [u32; 4], [u32; 4], [u32; 4]),
    CanContact(u32),
    OnContact(u32, [u32; 4], [u32; 4], [u32; 4], u32, u32, [u32; 4], [u32; 4], [u32; 3]),
}

fn shape_ev(node: u32, shape: &WorldBoarder) -> HitEv {
    HitEv::RayCast(
        node,
        shape.kind,
        b4(shape.hi),
        b4(shape.lo),
        b4(shape.mid),
    )
}

#[test]
fn ragdoll_on_contact_and_hit_node_match_the_original() {
    let Some(mut r) = runner() else { return };

    // 1. Ragdoll_OnContact (0x10ebc8): empty no-op
    let base = 0x0100_0000u32;
    let rag_ptr = base;
    let rider_ptr = base + 0x200;
    let node_ptr = base + 0xc00;
    let node_vt = base + 0xd00;
    let mut g = Rng(0x10eb_d000_0002);

    let mut before = vec![0u8; 0x100];
    for b in &mut before {
        *b = g.next() as u8;
    }
    r.mem.write(rag_ptr, &before).unwrap();
    r.cpu.set_gpr(4, rag_ptr as u64);
    r.call(0x10ebc8).unwrap();
    ragdoll::on_contact();
    let mut after = vec![0u8; 0x100];
    r.mem.read(rag_ptr, &mut after).unwrap();
    assert_eq!(after, before, "Ragdoll_OnContact must not modify memory");

    // 2. Ragdoll_HitNode (0x10ebd0): 2,000 random cases covering t <= 0, t > 0 non-closing,
    // t > 0 closing with can_contact = 0, and t > 0 closing with can_contact != 0
    const STUB_CAN: u32 = 0x01f0_0100;
    const STUB_ON: u32 = 0x01f0_0110;
    let rlog: Rc<RefCell<Vec<HitEv>>> = Rc::default();
    let cast_ans: Rc<RefCell<(f32, CastOut)>> = Rc::new(RefCell::new((0.0, CastOut::default())));
    let can_ans: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

    {
        let log = rlog.clone();
        let ans = cast_ans.clone();
        r.stub(0x13aad0, move |c, mem| {
            let node = c.gpr(4) as u32;
            let shape_ptr = c.gpr(5) as u32;
            let out_ptr = c.gpr(6) as u32;
            let kind = mem.read_u32(shape_ptr);
            let hi = r4(mem, shape_ptr + 0x10);
            let lo = r4(mem, shape_ptr + 0x20);
            let mid = r4(mem, shape_ptr + 0x30);
            log.borrow_mut().push(HitEv::RayCast(node, kind, b4(hi), b4(lo), b4(mid)));
            let (t, out) = *ans.borrow();
            put_cast_out(mem, out_ptr, &out);
            c.set_f(0, t);
        });
    }
    {
        let log = rlog.clone();
        let can = can_ans.clone();
        r.stub(STUB_CAN, move |c, _| {
            let this = c.gpr(4) as u32;
            log.borrow_mut().push(HitEv::CanContact(this));
            c.set_gpr(2, *can.borrow() as i32 as i64 as u64);
        });
    }
    {
        let log = rlog.clone();
        r.stub(STUB_ON, move |c, mem| {
            let this = c.gpr(4) as u32;
            let cp = c.gpr(5) as u32;
            let op = c.gpr(6) as u32;
            let out = read_cast_out(mem, op);
            log.borrow_mut().push(HitEv::OnContact(
                this,
                b4(r4(mem, cp)),
                b4(r4(mem, cp + 0x10)),
                b4(r4(mem, cp + 0x20)),
                mem.read_f32(cp + 0x30).to_bits(),
                out.kind,
                b4(out.pos),
                b4(out.normal),
                out.x30,
            ));
        });
    }

    let (mut n_miss, mut n_nonclose, mut n_no_deliver, mut n_deliver) = (0usize, 0, 0, 0);
    for case in 0..2_000u32 {
        let mut body = CollBody::new();
        body.init_rider(g.f(0.8, 2.2));
        body.center = g.v(500.0);
        let rmask = g.next() as i32;
        body.x24 = g.pick(&[0, -1, 0x3ffff, rmask]);
        for s in &mut body.spheres {
            s.0 = g.v(600.0);
        }

        let inv_m = g.f(0.005, 0.05);
        let d0 = g.f(1e-6, 1e-4);
        let d1 = g.f(1e-6, 1e-4);
        let d2 = g.f(1e-6, 1e-4);
        let rb = RigidBody {
            inv_mass: inv_m,
            inv_inertia: [d0, 0.0, 0.0, 0.0, d1, 0.0, 0.0, 0.0, d2],
            pos: g.v(500.0),
            q: [1.0, 0.0, 0.0, 0.0],
            p: g.v(10_000.0),
            l: g.v(50_000.0),
            v: g.v(1500.0),
            w: g.v(10.0),
        };
        let mut rag = Ragdoll {
            t_air: g.f(0.0, 1.0),
            crash_at: g.v(500.0),
            crash_n: g.unit(),
            rb,
        };
        let rider = RagRider {
            id: 0,
            pos: rb.pos,
            vel: rb.v,
            q: rb.q,
            up: [0.0, 0.0, 1.0, 0.0],
            height: 0.0,
            surface: 0,
            ground: [0.0, 0.0, 1.0, 0.0],
            fall: [0.0, 1.0, 0.0, 0.0],
            ground_at: [0.0; 4],
            ground_kind: 0,
            crash_n: [0.0; 4],
            side: [1.0, 0.0, 0.0, 0.0],
            course: [0.0; 4],
            body,
            hip: [0.0; 4],
        };

        let t = match case % 5 {
            0 => -1.0,
            1 => 0.0,
            _ => g.f(0.01, 50.0),
        };
        let mut out = CastOut {
            kind: g.below(4),
            pos: [rb.pos[0] + g.f(-80.0, 80.0), rb.pos[1] + g.f(-80.0, 80.0), rb.pos[2] + g.f(-80.0, 80.0), 0.0],
            normal: g.unit(),
            x30: [g.next(), g.next(), g.next()],
        };
        out.normal[3] = 0.0;
        let can = g.pick(&[0u32, 0, 1, 7]);

        // Set up memory for runner
        r.mem.write_u32(rag_ptr + 0x60, rider_ptr);
        put_rb(&mut r.mem, rag_ptr + 0x70, &rag.rb);
        put_body(&mut r.mem, rider_ptr + 0x470, &rider.body);
        r.mem.write_u32(node_ptr + 8, node_vt);
        r.mem.write_u16(node_vt + 0x60, 0);
        r.mem.write_u32(node_vt + 0x64, STUB_CAN);
        r.mem.write_u16(node_vt + 0x98, 0);
        r.mem.write_u32(node_vt + 0x9c, STUB_ON);

        *cast_ans.borrow_mut() = (t, out);
        *can_ans.borrow_mut() = can;
        rlog.borrow_mut().clear();

        r.cpu.set_gpr(4, rag_ptr as u64);
        r.cpu.set_gpr(5, node_ptr as u64);
        r.call(0x10ebd0).unwrap_or_else(|e| panic!("HitNode case {case}: {e}"));
        let orig_rb = get_rb(&mut r.mem, rag_ptr + 0x70);

        struct OursEnv {
            t: f32,
            out: CastOut,
            can: u32,
            destroyed: u32,
            log: Vec<HitEv>,
        }
        impl NodeHitEnv for OursEnv {
            fn ray_cast(&mut self, node: u32, shape: &WorldBoarder) -> (f32, CastOut) {
                self.log.push(shape_ev(node, shape));
                (self.t, self.out)
            }
            fn can_contact(&mut self, node: u32) -> u32 {
                self.log.push(HitEv::CanContact(node));
                self.can
            }
            fn on_contact(&mut self, node: u32, c: &Contact, out: &CastOut) {
                self.log.push(HitEv::OnContact(
                    node,
                    b4(c.pos),
                    b4(c.dir),
                    b4(c.normal),
                    c.speed_in.to_bits(),
                    out.kind,
                    b4(out.pos),
                    b4(out.normal),
                    out.x30,
                ));
            }
            fn destroy_shape(&mut self, _shape: &WorldBoarder, how: u32) {
                assert_eq!(how, 2);
                self.destroyed += 1;
            }
        }
        let mut env = OursEnv { t, out, can, destroyed: 0, log: Vec::new() };
        rag.hit_node(&rider, node_ptr, &mut env);

        assert_eq!(env.destroyed, 1, "HitNode must always destroy shape case {case}");
        assert_eq!(env.log, *rlog.borrow(), "HitNode events case {case}");
        assert_eq!(format!("{:?}", rag.rb), format!("{orig_rb:?}"), "HitNode rb case {case}");

        if !(0.0 < t) {
            n_miss += 1;
        } else if env.log.len() == 1 {
            n_nonclose += 1;
        } else if env.log.len() == 2 {
            n_no_deliver += 1;
        } else {
            n_deliver += 1;
        }
    }
    assert!(
        n_miss > 200 && n_nonclose > 200 && n_no_deliver > 100 && n_deliver > 100,
        "coverage: miss={n_miss} nonclose={n_nonclose} no_deliver={n_no_deliver} deliver={n_deliver}"
    );
}
