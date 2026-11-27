//! Function-runner check for row E9l (42 `world` base node / controller / counter / script virtuals & destructors).

use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::controllers::{self as ctrl, NodeEnv};
use tricky_game::pf::Mem;

const BASE: u32 = 0x0140_0000;
const FAKE_DEL: u32 = 0x002f_f300;
const FAKE_VF74: u32 = 0x002f_f310;
const FAKE_KEY: u32 = 0x002f_f320;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    for a in [
        0x13a8e0, // cControllerNode::~cControllerNode
        0x13aab0, // Instance_DetachController
        0x17dd00, // cBxSortObjNode::~cBxSortObjNode
        0x17ddc0, // cBxTypeObjNode::~cBxTypeObjNode
        0x2cc808, // EAMem_Set
        0x2ce114, // EAMem_Copy
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
    let z = vec![0u8; 0x10000];
    r.mem.write(BASE, &z);
    Mem {
        regions: vec![(BASE, vec![0u8; 0x10000])],
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

#[derive(Default)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
    vf74_ret: u32,
    key_ret: u32,
    draw_ret: u32,
}

impl NodeEnv for EnvState {
    fn call(&mut self, _m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        self.calls.push((target, args.to_vec()));
        match target {
            FAKE_VF74 => self.vf74_ret,
            FAKE_KEY => self.key_ret,
            ctrl::INSTANCE_DRAW_VISIBLE_CELLS => self.draw_ret,
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    for &(addr, nargs) in &[
        (ctrl::BX_OBJ_NODE_DTOR, 2usize),
        (ctrl::INSTANCE_DRAW_VISIBLE_CELLS, 1),
        (FAKE_DEL, 2),
        (FAKE_VF74, 1),
        (FAKE_KEY, 1),
    ] {
        let s = st.clone();
        r.stub(addr, move |cpu, _| {
            let mut a = Vec::with_capacity(nargs);
            for k in 0..nargs {
                a.push(cpu.gpr(4 + k) as u32);
            }
            let mut e = s.borrow_mut();
            e.calls.push((addr, a));
            let rv = match addr {
                FAKE_VF74 => e.vf74_ret,
                FAKE_KEY => e.key_ret,
                ctrl::INSTANCE_DRAW_VISIBLE_CELLS => e.draw_ret,
                _ => 0,
            };
            cpu.set_gpr(2, rv as i32 as i64 as u64);
        });
    }
}

#[test]
fn world_base_nodes_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x0e91_0001);

    // 1. Destructors & Instance_DetachController
    for flags in 0..4u32 {
        for inst_mode in 0..3u32 {
            let mut m = fresh_mem(&mut r);
            let this = BASE + 0x100;
            let inst = BASE + 0x1000;
            let st_ps = Rc::new(RefCell::new(EnvState::default()));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState::default();

            let inst_ptr = if inst_mode == 0 { 0 } else { inst };
            let cur_ctl = if inst_mode == 1 { this } else { BASE + 0x9999 };

            let dtor_cases: &[(u32, fn(&mut Mem, &mut dyn NodeEnv, u32, u32))] = &[
                (0x13a8e0, ctrl::controller_node_dtor),
                (0x13b060, ctrl::restore_node_dtor),
                (0x13b5f8, ctrl::counter_node_dtor),
                (0x13bd20, ctrl::script_node_dtor),
                (0x17dd00, ctrl::bx_sort_obj_node_dtor),
                (0x17ddc0, ctrl::bx_type_obj_node_dtor),
                (0x1c7d50, ctrl::models_obj_node_dtor),
                (0x1c7df0, ctrl::terrain_obj_node_dtor),
            ];
            for &(addr, func) in dtor_cases {
                m.w32(this + 0x28, inst_ptr);
                r.mem.write_u32(this + 0x28, inst_ptr);
                m.w32(inst + 0xe4, cur_ctl);
                r.mem.write_u32(inst + 0xe4, cur_ctl);
                st_rs.calls.clear();
                st_ps.borrow_mut().calls.clear();
                func(&mut m, &mut st_rs, this, flags);
                call_fn(&mut r, addr, &[this, flags]);
                assert_eq!(st_rs.calls, st_ps.borrow().calls, "dtor {addr:#x} f={flags} m={inst_mode}");
                assert_eq!(m.r32(this + 8), r.mem.read_u32(this + 8));
                assert_eq!(m.r32(inst + 0xe4), r.mem.read_u32(inst + 0xe4));
            }
        }
    }

    // 2. cCounterNode::Kill 0x13b218
    for life in [-5i32, -1, 0, 1, 100] {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let inst = BASE + 0x1000;
        let vt = BASE + 0x2000;
        let fl = rng.next();
        for &(a, v) in &[
            (this + 8, vt),
            (this + 0x28, inst),
            (this + 0x38, life as u32),
            (inst + 0xe8, fl),
            (vt + 8, 4),
            (vt + 0xc, FAKE_DEL),
        ] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();
        ctrl::counter_node_kill(&mut m, &mut st_rs, this);
        call_fn(&mut r, 0x13b218, &[this]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(m.r32(inst + 0xe8), r.mem.read_u32(inst + 0xe8));
    }

    // 3. TriggerScript_SetContext 0x13ba30
    for mask in 0..4u32 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let m1 = if (mask & 1) != 0 { BASE + 0x1000 } else { 0 };
        let m2 = if (mask & 2) != 0 { BASE + 0x1100 } else { 0 };
        for off in (0..0x100).step_by(4) {
            let v = rng.next();
            m.w32(this + off, v);
            r.mem.write_u32(this + off, v);
        }
        for base in [BASE + 0x1000, BASE + 0x1100] {
            for off in (0..0x40).step_by(4) {
                let v = rng.next();
                m.w32(base + off, v);
                r.mem.write_u32(base + off, v);
            }
        }
        let a1 = rng.next();
        ctrl::trigger_script_set_context(&mut m, this, a1, m1, m2);
        call_fn(&mut r, 0x13ba30, &[this, a1, m1, m2]);
        assert_eq!(mem_slice(&m, this, 0x100), copy_out(&mut r, this, 0x100), "ctx mask={mask}");
    }

    // 4. Virtual accessors, comparators, dispatchers & cModelsObjNode::Draw
    {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let other = BASE + 0x200;
        let vt = BASE + 0x300;
        for &(a, v) in &[
            (this + 8, vt),
            (other + 8, vt),
            (vt + 0x30, 8),
            (vt + 0x34, FAKE_KEY),
            (vt + 0x70, 4),
            (vt + 0x74, FAKE_VF74),
        ] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        let st_ps = Rc::new(RefCell::new(EnvState {
            vf74_ret: 0xdead,
            key_ret: 0,
            draw_ret: 0x77,
            ..Default::default()
        }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState {
            vf74_ret: 0xdead,
            key_ret: 0,
            draw_ret: 0x77,
            ..Default::default()
        };

        assert_eq!(ctrl::counter_node_vf18(&mut m, &mut st_rs, this), call_fn(&mut r, 0x13b770, &[this]));
        assert_eq!(st_rs.calls, st_ps.borrow().calls);

        for _ in 0..50 {
            let v20 = rng.next();
            let v24 = rng.next();
            let v28 = rng.next();
            for &(a, v) in &[(this + 0x20, v20), (this + 0x24, v24), (this + 0x28, v28)] {
                m.w32(a, v);
                r.mem.write_u32(a, v);
            }
            assert_eq!(ctrl::counter_node_vf10(&m, this), call_fn(&mut r, 0x13b7a8, &[this]));
            assert_eq!(ctrl::counter_node_vf6(&m, this), call_fn(&mut r, 0x17dc60, &[this]));
            assert_eq!(ctrl::counter_node_vf5(&m, this), call_fn(&mut r, 0x17dc68, &[this]));

            for key in [v20.wrapping_sub(1), v20, v20.wrapping_add(1), 0, u32::MAX] {
                st_rs.key_ret = key;
                st_ps.borrow_mut().key_ret = key;
                st_rs.calls.clear();
                st_ps.borrow_mut().calls.clear();
                assert_eq!(ctrl::counter_node_vf4(&mut m, &mut st_rs, this, other), call_fn(&mut r, 0x17dc70, &[this, other]));
                assert_eq!(ctrl::counter_node_vf3(&mut m, &mut st_rs, this, other), call_fn(&mut r, 0x17dcb0, &[this, other]));
                assert_eq!(st_rs.calls, st_ps.borrow().calls);
            }
        }

        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        assert_eq!(ctrl::models_obj_node_draw(&mut m, &mut st_rs), call_fn(&mut r, 0x1c78f0, &[this]));
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }

    // 5. All 24 trivial no-op / constant-return stubs
    for addr in [
        0x13b670u32, 0x13b678, 0x13b6d8, 0x13b6e0, 0x13b6e8,
        0x13b750, 0x13b758, 0x13b760, 0x13b768, 0x13b798, 0x13b7a0,
        0x13b7c0, 0x13b7c8, 0x17dcf0, 0x17dcf8, 0x17ddb0, 0x17ddb8,
        0x17ded0, 0x17df08,
    ] {
        ctrl::node_noop();
        call_fn(&mut r, addr, &[BASE]);
    }
    for addr in [0x13b748u32, 0x13b7b0, 0x13b7d0, 0x17dd98] {
        assert_eq!(ctrl::node_ret_zero(), call_fn(&mut r, addr, &[BASE]), "zero {addr:#x}");
    }
    assert_eq!(ctrl::node_ret_one(), call_fn(&mut r, 0x13b7b8, &[BASE]));
}
