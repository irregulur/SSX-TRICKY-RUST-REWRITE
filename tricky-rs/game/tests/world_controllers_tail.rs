//! Function-runner check for row E9m (35 `world` roller / debounce / timer / boost / node destructor functions).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::controllers::{self as ctrl, NodeEnv};
use tricky_game::pf::Mem;

const BASE: u32 = 0x0140_0000;
const FAKE_DEL: u32 = 0x002f_f400;
const FAKE_FIN: u32 = 0x002f_f410;
const FAKE_VF60: u32 = 0x002f_f420;
const FAKE_VF98: u32 = 0x002f_f430;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        let u = (self.next() >> 8) as f32 / (1u32 << 24) as f32;
        lo + (hi - lo) * u
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    for a in [
        0x13a8e0, // cControllerNode::~cControllerNode
        0x13aab0, // Instance_DetachController
        0x140630, // cBoostNode::~cBoostNode
        0x17dd00, // cBxSortObjNode::~cBxSortObjNode
        0x17ddc0, // cBxTypeObjNode::~cBxTypeObjNode
        0x2cc808, // EAMem_Set
    ] {
        r.allow(a);
    }
    Some(r)
}

fn call_fn(r: &mut Runner, addr: u32, args: &[u32]) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn fresh_mem(r: &mut Runner) -> Mem {
    let z = vec![0u8; 0x20000];
    r.mem.write(BASE, &z);
    Mem {
        regions: vec![
            (0x0033_7000, vec![0u8; 0x1000]),
            (BASE, vec![0u8; 0x20000]),
        ],
    }
}

fn mem_slice(m: &Mem, at: u32, len: usize) -> Vec<u8> {
    (0..len).map(|i| m.r8(at + i as u32)).collect()
}

fn copy_out(r: &mut Runner, at: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    r.mem.read(at, &mut b).unwrap();
    b
}

#[derive(Default, Clone)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
    ray_ret: f32,
    ray_w10: [u32; 4],
    ray_w20: [u32; 4],
    vf60_ret: u32,
    vf98_payload: Vec<u8>,
    run_fin_ret: u32,
    ev_script_ret: i32,
    pool_ret: u32,
}

impl NodeEnv for EnvState {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        match target {
            ctrl::WORLD_SPH_TREE_CTOR => {
                self.calls.push((target, vec![args[1], args[2]]));
                0
            }
            ctrl::WORLD_SPH_TREE_VF10 => {
                self.calls.push((target, vec![args[1]]));
                0
            }
            ctrl::WORLD_NODE_RAY_CAST => {
                self.calls.push((target, vec![args[0]]));
                let out = args[2];
                for k in 0..4u32 {
                    m.w32(out + 0x10 + k * 4, self.ray_w10[k as usize]);
                    m.w32(out + 0x20 + k * 4, self.ray_w20[k as usize]);
                }
                self.ray_ret.to_bits()
            }
            FAKE_VF98 => {
                self.calls.push((target, vec![args[0]]));
                self.vf98_payload = mem_slice(m, args[1], 0x34);
                0
            }
            ctrl::RIGID_BODY_CONTACT_IMPULSE => {
                self.calls.push((target, vec![args[0], args[3], args[4]]));
                0
            }
            _ => {
                self.calls.push((target, args.to_vec()));
                match target {
                    FAKE_VF60 => self.vf60_ret,
                    ctrl::CONTROLLER_RUN_FINISH_SCRIPT => self.run_fin_ret,
                    ctrl::INSTANCE_GET_EVENT_SCRIPT => self.ev_script_ret as u32,
                    ctrl::MEMPOOL_ALLOC => self.pool_ret,
                    _ => 0,
                }
            }
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    let stubs: &[(u32, usize)] = &[
        (ctrl::MEM_DELETE, 1),
        (ctrl::BX_OBJ_NODE_DTOR, 2),
        (ctrl::CONTROLLER_NODE_CTOR, 4),
        (ctrl::CONTROLLER_RUN_FINISH_SCRIPT, 1),
        (ctrl::SCRIPT_NODE_CTOR, 7),
        (ctrl::BOOST_NODE_CTOR, 5),
        (ctrl::UV_SCROLL_NODE_DTOR, 2),
        (ctrl::TEX_FLIP_DTOR, 2),
        (ctrl::CONTACT_REMOVE_ALL_FOR_NODE, 2),
        (ctrl::BX_SPHERE_TREE_DTOR, 2),
        (ctrl::MEMPOOL_ALLOC, 6),
        (ctrl::INSTANCE_GET_EVENT_SCRIPT, 3),
        (ctrl::WORLD_SPH_TREE_CTOR, 3),
        (ctrl::WORLD_SPH_TREE_VF10, 2),
        (FAKE_DEL, 2),
        (FAKE_FIN, 2),
        (FAKE_VF60, 1),
        (FAKE_VF98, 3),
    ];
    for &(addr, nargs) in stubs {
        let s = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut a = Vec::with_capacity(nargs);
            for k in 0..nargs {
                a.push(cpu.gpr(4 + k) as u32);
            }
            let mut e = s.borrow_mut();
            let rv = match addr {
                ctrl::WORLD_SPH_TREE_CTOR => {
                    e.calls.push((addr, vec![a[1], a[2]]));
                    0
                }
                ctrl::WORLD_SPH_TREE_VF10 => {
                    e.calls.push((addr, vec![a[1]]));
                    0
                }
                FAKE_VF98 => {
                    e.calls.push((addr, vec![a[0]]));
                    let mut b = vec![0u8; 0x34];
                    mem.read(a[1], &mut b).unwrap();
                    e.vf98_payload = b;
                    0
                }
                _ => {
                    e.calls.push((addr, a));
                    match addr {
                        FAKE_VF60 => e.vf60_ret,
                        ctrl::CONTROLLER_RUN_FINISH_SCRIPT => e.run_fin_ret,
                        ctrl::INSTANCE_GET_EVENT_SCRIPT => e.ev_script_ret as u32,
                        ctrl::MEMPOOL_ALLOC => e.pool_ret,
                        _ => 0,
                    }
                }
            };
            cpu.set_gpr(2, rv as i32 as i64 as u64);
        });
    }
    // Float-return / float-arg stubs
    {
        let s = st.clone();
        r.stub(ctrl::WORLD_NODE_RAY_CAST, move |cpu, mem| {
            let other = cpu.gpr(4) as u32;
            let out = cpu.gpr(6) as u32;
            let mut e = s.borrow_mut();
            e.calls.push((ctrl::WORLD_NODE_RAY_CAST, vec![other]));
            for k in 0..4u32 {
                mem.write_u32(out + 0x10 + k * 4, e.ray_w10[k as usize]);
                mem.write_u32(out + 0x20 + k * 4, e.ray_w20[k as usize]);
            }
            cpu.fpr[0] = e.ray_ret.to_bits();
        });
    }
    {
        let s = st.clone();
        r.stub(ctrl::RIGID_BODY_CONTACT_IMPULSE, move |cpu, _| {
            let mut e = s.borrow_mut();
            e.calls.push((
                ctrl::RIGID_BODY_CONTACT_IMPULSE,
                vec![cpu.gpr(4) as u32, cpu.fpr[12], cpu.fpr[13]],
            ));
            cpu.set_gpr(2, 0);
        });
    }
    {
        let s = st.clone();
        r.stub(ctrl::CONTROLLER_NODE_ON_MESSAGE, move |cpu, _| {
            let mut e = s.borrow_mut();
            e.calls.push((
                ctrl::CONTROLLER_NODE_ON_MESSAGE,
                vec![cpu.gpr(4) as u32, cpu.gpr(5) as u32, cpu.fpr[12]],
            ));
            cpu.set_gpr(2, 0);
        });
    }
}

#[test]
fn roller_debounce_timer_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x0e9d_0001);

    // 1. RollerNode_HitNode 0x13f330, vf12_149e18, Kill 0x149e48
    for &(ray_ret, vf60) in &[(-1.0f32, 0u32), (0.0, 1), (0.5, 0), (1.25, 1)] {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let other = BASE + 0x1000;
        let vt = BASE + 0x2000;
        let sp = BASE + 0x4000;
        for &(a, v) in &[
            (this + 8, vt),
            (other + 8, vt),
            (vt + 8, 4),
            (vt + 0xc, FAKE_DEL),
            (vt + 0x60, 0),
            (vt + 0x64, FAKE_VF60),
            (vt + 0x98, 8),
            (vt + 0x9c, FAKE_VF98),
        ] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        for k in 0..4u32 {
            let f = rng.f(-500.0, 500.0).to_bits();
            m.w32(this + 0xa0 + k * 4, f);
            r.mem.write_u32(this + 0xa0 + k * 4, f);
        }
        let init = EnvState {
            ray_ret,
            ray_w10: [rng.next(), rng.next(), rng.next(), rng.next()],
            ray_w20: [rng.next(), rng.next(), rng.next(), rng.next()],
            vf60_ret: vf60,
            ..Default::default()
        };
        let st_ps = Rc::new(RefCell::new(init.clone()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = init;

        ctrl::roller_node_hit_node(&mut m, &mut st_rs, this, other, sp);
        call_fn(&mut r, 0x13f330, &[this, other]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(st_rs.vf98_payload, st_ps.borrow().vf98_payload);

        ctrl::roller_node_vf12(&mut m, this);
        call_fn(&mut r, 0x149e18, &[this]);
        assert_eq!(m.r32(this + 0x140), r.mem.read_u32(this + 0x140));

        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        ctrl::roller_node_kill(&mut m, &mut st_rs, this);
        call_fn(&mut r, 0x149e48, &[this]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }

    // 2. cDebounceNode & cTimerNode lifecycle
    for case in 0..40 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let inst = BASE + 0x1000;
        let params = BASE + 0x1200;
        let vt = BASE + 0x2000;
        let pool_obj = BASE + 0x3000;
        let p3 = rng.f(-0.1, 0.2);
        let p4 = rng.f(-0.1, 0.2);
        for &(a, v) in &[
            (params + 0xc, p3.to_bits()),
            (params + 0x10, p4.to_bits()),
            (vt + 8, 4),
            (vt + 0xc, FAKE_DEL),
            (vt + 0xb0, 0),
            (vt + 0xb4, FAKE_FIN),
            (this + 0x28, inst),
        ] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        let init = EnvState {
            run_fin_ret: (case & 1) as u32,
            ev_script_ret: if case % 3 == 0 { -1 } else { 7 },
            pool_ret: if case % 5 == 0 { 0 } else { pool_obj },
            ..Default::default()
        };
        let st_ps = Rc::new(RefCell::new(init.clone()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = init;

        // cDebounceNode::cDebounceNode
        assert_eq!(
            ctrl::debounce_node_ctor(&mut m, &mut st_rs, this, 3, inst, params),
            call_fn(&mut r, 0x13fbf0, &[this, 3, inst, params])
        );
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, this, 0x40), copy_out(&mut r, this, 0x40));

        // Update / Kill / Finish with custom vtable
        m.w32(this + 8, vt);
        r.mem.write_u32(this + 8, vt);
        for ticks in [-1i32, 0, 1, 2] {
            m.w32(this + 0x2c, ticks as u32);
            r.mem.write_u32(this + 0x2c, ticks as u32);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            ctrl::debounce_node_update(&mut m, &mut st_rs, this);
            call_fn(&mut r, 0x13fca8, &[this]);
            ctrl::debounce_node_kill(&mut m, &mut st_rs, this);
            call_fn(&mut r, 0x13fcf0, &[this]);
            ctrl::debounce_node_finish(&mut m, &mut st_rs, this, (case & 1) as u32);
            call_fn(&mut r, 0x13fd30, &[this, (case & 1) as u32]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(this + 0x2c), r.mem.read_u32(this + 0x2c));
        }

        // cTimerNode::cTimerNode, Update, Kill, Finish, OnMessage
        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        assert_eq!(
            ctrl::timer_node_ctor(&mut m, &mut st_rs, this, 3, inst, params),
            call_fn(&mut r, 0x13fdd0, &[this, 3, inst, params])
        );
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, this, 0x40), copy_out(&mut r, this, 0x40));

        m.w32(this + 8, vt);
        r.mem.write_u32(this + 8, vt);
        for (fin, life) in [(-1i32, 0i32), (1, 1), (2, 2)] {
            m.w32(this + 0x34, fin as u32);
            r.mem.write_u32(this + 0x34, fin as u32);
            m.w32(this + 0x38, life as u32);
            r.mem.write_u32(this + 0x38, life as u32);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            ctrl::timer_node_update(&mut m, &mut st_rs, this);
            call_fn(&mut r, 0x13fea0, &[this]);
            ctrl::timer_node_kill(&mut m, &mut st_rs, this);
            call_fn(&mut r, 0x13ffa8, &[this]);
            ctrl::timer_node_finish(&mut m, &mut st_rs, this, (case & 1) as u32);
            call_fn(&mut r, 0x13ffe8, &[this, (case & 1) as u32]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(mem_slice(&m, this, 0x40), copy_out(&mut r, this, 0x40));
            if st_rs.pool_ret != 0 {
                assert_eq!(mem_slice(&m, pool_obj, 0xf0), copy_out(&mut r, pool_obj, 0xf0));
            }
        }
        for kind in [0u32, 1, 2] {
            let val = rng.f(0.0, 5.0);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            ctrl::timer_node_on_message(&mut m, &mut st_rs, this, kind, val);
            r.cpu.set_gpr(4, this as i32 as i64 as u64);
            r.cpu.set_gpr(5, kind as i32 as i64 as u64);
            r.cpu.fpr[12] = val.to_bits();
            r.call(0x140048).unwrap();
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(this + 0x38), r.mem.read_u32(this + 0x38));
        }
    }
}

#[test]
fn boost_and_world_node_dtors_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x0e9d_0002);

    // 3. RandomBoostNode & TubeEndBoostNode ctors, vf28, vf29
    for nriders in 0..=6u32 {
        for flag5c in [-1i32, 0, 1] {
            let mut m = fresh_mem(&mut r);
            let app = BASE + 0x100;
            let game = BASE + 0x1000;
            let mgr = BASE + 0x1200;
            let this = BASE + 0x2000;
            let inst = BASE + 0x3000;
            let params = BASE + 0x3200;
            for &(a, v) in &[
                (ctrl::G_APP, app),
                (app + 0x730, game),
                (game + 0x1c, mgr),
                (mgr + 0x88, nriders),
            ] {
                m.w32(a, v);
                r.mem.write_u32(a, v);
            }
            for off in (0x28..0x58).step_by(4) {
                let f = rng.f(-100.0, 100.0).to_bits();
                m.w32(params + off, f);
                r.mem.write_u32(params + off, f);
            }
            let st_ps = Rc::new(RefCell::new(EnvState::default()));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState::default();

            // RandomBoostNode
            assert_eq!(
                ctrl::random_boost_node_ctor(&mut m, &mut st_rs, this, 4, inst, params),
                call_fn(&mut r, 0x141208, &[this, 4, inst, params])
            );
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(mem_slice(&m, this, 0x190), copy_out(&mut r, this, 0x190));

            m.w32(this + 0x5c, flag5c as u32);
            r.mem.write_u32(this + 0x5c, flag5c as u32);
            for i in 0..6u32 {
                let b = (rng.next() & 1) as u8;
                m.w8(this + 0x180 + i, b);
                r.mem.write(this + 0x180 + i, &[b]);
                m.w32(this + 0x120 + i * 0x10, 0x3f80_0000);
                r.mem.write_u32(this + 0x120 + i * 0x10, 0x3f80_0000);
            }
            ctrl::random_boost_node_vf29(&mut m, this);
            call_fn(&mut r, 0x1415b0, &[this]);
            assert_eq!(mem_slice(&m, this, 0x190), copy_out(&mut r, this, 0x190));
            ctrl::random_boost_node_vf28(&mut m, this);
            call_fn(&mut r, 0x1415a0, &[this]);
            assert_eq!(mem_slice(&m, this, 0x190), copy_out(&mut r, this, 0x190));

            // TubeEndBoostNode
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            assert_eq!(
                ctrl::tube_end_boost_node_ctor(&mut m, &mut st_rs, this, 5, inst, params),
                call_fn(&mut r, 0x141688, &[this, 5, inst, params])
            );
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(mem_slice(&m, this, 0x1c0), copy_out(&mut r, this, 0x1c0));

            m.w32(this + 0x5c, flag5c as u32);
            r.mem.write_u32(this + 0x5c, flag5c as u32);
            for i in 0..6u32 {
                let b = (rng.next() & 1) as u8;
                m.w8(this + 0x1ac + i, b);
                r.mem.write(this + 0x1ac + i, &[b]);
                m.w32(this + 0x140 + i * 0x10, 0x3f80_0000);
                r.mem.write_u32(this + 0x140 + i * 0x10, 0x3f80_0000);
            }
            ctrl::tube_end_boost_node_vf29(&mut m, this);
            call_fn(&mut r, 0x141a08, &[this]);
            assert_eq!(mem_slice(&m, this, 0x1c0), copy_out(&mut r, this, 0x1c0));
            ctrl::tube_end_boost_node_vf28(&mut m, this);
            call_fn(&mut r, 0x1419f8, &[this]);
            assert_eq!(mem_slice(&m, this, 0x1c0), copy_out(&mut r, this, 0x1c0));
        }
    }

    // 4. All 12 destructors + LapBoostNode::Finish + 4 no-ops
    for flags in 0..4u32 {
        let mut m = fresh_mem(&mut r);
        let app = BASE + 0x100;
        let game = BASE + 0x1000;
        let cm = BASE + 0x1400;
        let this = BASE + 0x2000;
        let inst = BASE + 0x3000;
        for &(a, v) in &[
            (ctrl::G_APP, app),
            (app + 0x730, game),
            (game + 0x30, cm),
        ] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();

        let dtors: &[(u32, fn(&mut Mem, &mut dyn NodeEnv, u32, u32))] = &[
            (0x13e0b8, ctrl::roller_node_dtor),
            (0x140630, ctrl::boost_node_dtor),
            (0x140b38, ctrl::lap_boost_node_dtor),
            (0x141c80, ctrl::z_boost_node_dtor),
            (0x149610, ctrl::crowd_box_node_dtor),
            (0x149688, ctrl::debounce_node_dtor),
            (0x1496f8, ctrl::cracked_node_dtor),
            (0x149868, ctrl::uv_scroll_tex_flip_node_dtor),
            (0x149938, ctrl::tex_flip_node_dtor),
            (0x149b78, ctrl::tube_end_boost_node_dtor),
            (0x149bf0, ctrl::random_boost_node_dtor),
            (0x149d98, ctrl::timer_node_dtor),
        ];
        for &(addr, func) in dtors {
            let fl = rng.next();
            m.w32(this + 0x28, inst);
            r.mem.write_u32(this + 0x28, inst);
            m.w32(inst + 0xe4, this);
            r.mem.write_u32(inst + 0xe4, this);
            m.w32(inst + 0xe8, fl);
            r.mem.write_u32(inst + 0xe8, fl);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            func(&mut m, &mut st_rs, this, flags);
            call_fn(&mut r, addr, &[this, flags]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls, "dtor {addr:#x} flags={flags}");
            assert_eq!(m.r32(inst + 0xe4), r.mem.read_u32(inst + 0xe4));
            assert_eq!(m.r32(inst + 0xe8), r.mem.read_u32(inst + 0xe8));
        }

        // cLapBoostNode::Finish 0x141198
        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        ctrl::lap_boost_node_finish(&mut m, &mut st_rs, this, flags & 1);
        call_fn(&mut r, 0x141198, &[this, flags & 1]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }

    for addr in [0x141188u32, 0x141190, 0x149cc8, 0x149cd0] {
        ctrl::boost_noop();
        call_fn(&mut r, addr, &[BASE]);
    }
}
