//! Function-runner check for row E9n (28 `world` visual, emitter, particle, movie, and rail-manager node functions).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::controllers::{self as ctrl, NodeEnv};
use tricky_game::pf::Mem;

const BASE: u32 = 0x0140_0000;
const FAKE_GFX_1E4: u32 = 0x002f_f500;
const FAKE_GFX_234: u32 = 0x002f_f510;
const FAKE_GFX_284: u32 = 0x002f_f520;
const FAKE_GFX_2AC: u32 = 0x002f_f530;
const FAKE_MESH_3C: u32 = 0x002f_f540;
const FAKE_NODE_DEL: u32 = 0x002f_f550;
const FAKE_NODE_FIN: u32 = 0x002f_f560;

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
    let zg = vec![0u8; 0x1000];
    r.mem.write(0x0031_d000, &zg);
    r.mem.write(0x0033_7000, &zg);
    Mem {
        regions: vec![
            (0x0031_d000, vec![0u8; 0x1000]),
            (0x0033_7000, vec![0u8; 0x1000]),
            (BASE, vec![0u8; 0x20000]),
        ],
    }
}

fn copy_out(r: &mut Runner, at: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    r.mem.read(at, &mut b).unwrap();
    b
}

fn mem_slice(m: &Mem, at: u32, len: usize) -> Vec<u8> {
    (0..len).map(|i| m.r8(at + i as u32)).collect()
}

#[derive(Default, Clone)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>, Vec<u8>)>,
    cull_ret: u32,
    movie_tex: u32,
    script_ret: u32,
    alloc_ret: u32,
    particle_life_after: Option<u32>,
    spline_base: u32,
}

struct TestEnv(Rc<RefCell<EnvState>>);

impl NodeEnv for TestEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        let mut s = self.0.borrow_mut();
        let snap = match target {
            FAKE_GFX_284 => {
                let mut v = mem_slice(m, args[1], 16);
                v.extend_from_slice(&mem_slice(m, args[2], 16));
                v
            }
            FAKE_GFX_2AC => {
                if args[6] != 0 {
                    mem_slice(m, args[6], 0x20)
                } else {
                    Vec::new()
                }
            }
            FAKE_MESH_3C => mem_slice(m, args[1], 0x40),
            _ => Vec::new(),
        };
        s.calls.push((target, args.to_vec(), snap));
        match target {
            FAKE_GFX_284 => s.cull_ret,
            ctrl::MOVIE_GET_FRAME_TEXTURE => s.movie_tex,
            ctrl::INSTANCE_GET_EVENT_SCRIPT => s.script_ret,
            ctrl::MEMPOOL_ALLOC => s.alloc_ret,
            ctrl::OBJ_HAS_ID => {
                let contact = args[0];
                let node = args[1];
                if m.r32(contact) == node {
                    0
                } else {
                    1
                }
            }
            ctrl::PARTICLE_NODE_UPDATE => {
                if let Some(bits) = s.particle_life_after {
                    m.w32(args[0], bits);
                }
                0
            }
            ctrl::RAIL_NODE_GET_SPLINE => s.spline_base.wrapping_add((args[1] & 0x3f) * 0x40),
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: &Rc<RefCell<EnvState>>) {
    let stub_n = |r: &mut Runner, addr: u32, n: usize, snap_kind: u8| {
        let st = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut args = Vec::with_capacity(n);
            for k in 0..n {
                args.push(cpu.gpr(4 + k) as u32);
            }
            let snap = match snap_kind {
                1 => {
                    let mut a = vec![0u8; 16];
                    let mut b = vec![0u8; 16];
                    mem.read(args[1], &mut a).unwrap();
                    mem.read(args[2], &mut b).unwrap();
                    a.extend_from_slice(&b);
                    a
                }
                2 => {
                    if args[6] != 0 {
                        let mut a = vec![0u8; 0x20];
                        mem.read(args[6], &mut a).unwrap();
                        a
                    } else {
                        Vec::new()
                    }
                }
                3 => {
                    let mut a = vec![0u8; 0x40];
                    mem.read(args[1], &mut a).unwrap();
                    a
                }
                _ => Vec::new(),
            };
            let mut s = st.borrow_mut();
            s.calls.push((addr, args.clone(), snap));
            let ret = match addr {
                FAKE_GFX_284 => s.cull_ret,
                ctrl::MOVIE_GET_FRAME_TEXTURE => s.movie_tex,
                ctrl::INSTANCE_GET_EVENT_SCRIPT => s.script_ret,
                ctrl::MEMPOOL_ALLOC => s.alloc_ret,
                ctrl::OBJ_HAS_ID => {
                    if mem.read_u32(args[0]) == args[1] {
                        0
                    } else {
                        1
                    }
                }
                ctrl::PARTICLE_NODE_UPDATE => {
                    if let Some(bits) = s.particle_life_after {
                        mem.write_u32(args[0], bits);
                    }
                    0
                }
                ctrl::RAIL_NODE_GET_SPLINE => s.spline_base.wrapping_add((args[1] & 0x3f) * 0x40),
                _ => 0,
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    };

    stub_n(r, ctrl::BX_OBJ_NODE_DTOR, 2, 0);
    stub_n(r, ctrl::CONTROLLER_NODE_CTOR, 4, 0);
    stub_n(r, ctrl::SCRIPT_NODE_CTOR, 7, 0);
    stub_n(r, ctrl::TEX_FLIP_DRAW, 1, 0);
    stub_n(r, ctrl::COLLIDE_EMITTER_CREATE_PARTICLES, 1, 0);
    stub_n(r, ctrl::MOVIE_NODE_OPEN_VIDEO, 1, 0);
    stub_n(r, ctrl::BROADPHASE_FIT_BOX, 2, 0);
    stub_n(r, ctrl::CONTACT_REMOVE_ALL_FOR_NODE, 2, 0);
    stub_n(r, ctrl::BROADPHASE_PROXY_INIT, 1, 0);
    stub_n(r, ctrl::BROADPHASE_PROXY_SET_BOX_B, 3, 0);
    stub_n(r, ctrl::BX_TYPE_OBJ_NODE_CTOR, 3, 0);
    stub_n(r, ctrl::BX_SORT_OBJ_NODE_CTOR, 3, 0);
    stub_n(r, ctrl::PARTICLE_NODE_INIT_MEM, 1, 0);
    stub_n(r, ctrl::PARTICLE_NODE_FREE_MEM, 2, 0);
    stub_n(r, ctrl::PARTICLE_NODE_UPDATE, 1, 0);
    stub_n(r, ctrl::PARTICLE_SYS_DRAW, 1, 0);
    stub_n(r, ctrl::VIDEO_PLAYER_RELEASE, 2, 0);
    stub_n(r, ctrl::MOVIE_GET_FRAME_TEXTURE, 2, 0);
    stub_n(r, ctrl::MEMPOOL_ALLOC, 6, 0);
    stub_n(r, ctrl::INSTANCE_GET_EVENT_SCRIPT, 3, 0);
    stub_n(r, ctrl::RAIL_NODE_GET_SPLINE, 2, 0);
    stub_n(r, ctrl::OBJ_HAS_ID, 2, 0);

    stub_n(r, FAKE_GFX_1E4, 2, 0);
    stub_n(r, FAKE_GFX_234, 2, 0);
    stub_n(r, FAKE_GFX_284, 3, 1);
    stub_n(r, FAKE_GFX_2AC, 7, 2);
    stub_n(r, FAKE_MESH_3C, 4, 3);
    stub_n(r, FAKE_NODE_DEL, 2, 0);
    stub_n(r, FAKE_NODE_FIN, 2, 0);
}

fn setup_globals(r: &mut Runner, m: &mut Mem) -> (u32, u32, u32) {
    let app = BASE + 0x100;
    let gfx = BASE + 0x900;
    let gfx_vt = BASE + 0xa00;
    let game = BASE + 0xe00;
    let cm = BASE + 0xf00;
    let vid = BASE + 0xf80;

    let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
        r.mem.write_u32(a, v);
        m.w32(a, v);
    };
    let w16 = |r: &mut Runner, m: &mut Mem, a: u32, v: u16| {
        r.mem.write(a, &v.to_le_bytes());
        m.w16(a, v);
    };
    w32(r, m, ctrl::G_APP, app);
    w32(r, m, app + 0x724, gfx);
    w32(r, m, app + 0x730, game);
    w32(r, m, game + 0x30, cm);
    w32(r, m, game + 0x44, vid);
    w32(r, m, gfx + 0x10, gfx_vt);

    w16(r, m, gfx_vt + 0x1e0, 4);
    w32(r, m, gfx_vt + 0x1e4, FAKE_GFX_1E4);
    w16(r, m, gfx_vt + 0x230, 8);
    w32(r, m, gfx_vt + 0x234, FAKE_GFX_234);
    w16(r, m, gfx_vt + 0x280, 12);
    w32(r, m, gfx_vt + 0x284, FAKE_GFX_284);
    w16(r, m, gfx_vt + 0x2a8, 16);
    w32(r, m, gfx_vt + 0x2ac, FAKE_GFX_2AC);
    (app, gfx, game)
}

#[test]
fn visual_draw_and_dtor_nodes_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0xE9E0_0001);

    call_fn(&mut r, 0x148d20, &[]);
    let sp_init = r.cpu.gpr(29) as u32;
    let stack_base = (sp_init - 0x400) & !0xf;
    for case in 0..80 {
        let mut m = fresh_mem(&mut r);
        setup_globals(&mut r, &mut m);
        // Zero runner stack slice in both r.mem and m so sp-relative buffers match bit-for-bit.
        let zstack = vec![0u8; 0x400];
        r.mem.write(stack_base, &zstack);
        m.regions.push((stack_base, zstack));

        let this = BASE + 0x2000;
        let inst = BASE + 0x3000;
        let mesh = BASE + 0x3400;
        let mesh_vt = BASE + 0x3500;

        let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
            r.mem.write_u32(a, v);
            m.w32(a, v);
        };
        let w16 = |r: &mut Runner, m: &mut Mem, a: u32, v: u16| {
            r.mem.write(a, &v.to_le_bytes());
            m.w16(a, v);
        };

        w32(&mut r, &mut m, this + 0x28, inst);
        w32(&mut r, &mut m, inst + 0xc0, 0x1111_0000 + case);
        w32(&mut r, &mut m, inst + 0xf0, 0x2222_0000 + case);
        w32(&mut r, &mut m, inst + 0xe4, if case % 2 == 0 { this } else { 0 });
        for k in 0..16u32 {
            let bits = rng.f(-10.0, 10.0).to_bits();
            w32(&mut r, &mut m, inst + k * 4, bits);
        }
        for k in 0..6u32 {
            let bits = rng.f(-500.0, 500.0).to_bits();
            w32(&mut r, &mut m, inst + 0xcc + k * 4, bits);
        }
        w32(&mut r, &mut m, mesh, mesh_vt);
        w16(&mut r, &mut m, mesh_vt + 0x38, 4);
        w32(&mut r, &mut m, mesh_vt + 0x3c, FAKE_MESH_3C);

        // 1. cUVScrollNode::~cUVScrollNode 0x142128
        {
            let slot: i32 = if case % 4 == 0 { -1 } else { (case % 5) as i32 };
            let rc: i32 = (case % 3) as i32;
            let flags = (case % 4) as u32;
            w32(&mut r, &mut m, this + 0x68, slot as u32);
            if slot >= 0 {
                let e = ctrl::G_UV_SCROLL_TABLE + (slot as u32) * 12;
                w32(&mut r, &mut m, e, 0xaaaa_0000 + case);
                w32(&mut r, &mut m, e + 4, rc as u32);
                w32(&mut r, &mut m, e + 8, 0);
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x142128, &[this, flags]);
            let mips_calls = st.borrow().calls.clone();
            let mips_tbl = copy_out(&mut r, ctrl::G_UV_SCROLL_TABLE, 12 * 6);
            let mips_node = copy_out(&mut r, this, 0x70);

            st.borrow_mut().calls.clear();
            ctrl::uv_scroll_node_dtor(&mut m, &mut TestEnv(st.clone()), this, flags);
            assert_eq!(st.borrow().calls, mips_calls, "uv_scroll_node_dtor calls case {case}");
            assert_eq!(mem_slice(&m, ctrl::G_UV_SCROLL_TABLE, 12 * 6), mips_tbl);
            assert_eq!(mem_slice(&m, this, 0x70), mips_node);
        }

        // 2. cUVScrollNode::Draw 0x1426d0 & 3. UVScrollNode_DrawMesh 0x1427b8
        {
            let count = (case % 5) as i32;
            let slot: i32 = if case % 3 == 0 { -1 } else { (case % 4) as i32 };
            let prev_flag = (case / 3) % 2;
            w32(&mut r, &mut m, this + 0x3c, rng.f(-5.0, 5.0).to_bits());
            w32(&mut r, &mut m, this + 0x40, rng.f(-5.0, 5.0).to_bits());
            w32(&mut r, &mut m, this + 0x64, count as u32);
            w32(&mut r, &mut m, this + 0x68, slot as u32);
            if slot >= 0 {
                let e = ctrl::G_UV_SCROLL_TABLE + (slot as u32) * 12;
                w32(&mut r, &mut m, e + 8, prev_flag);
            }
            // cUVScrollNode::Draw
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1426d0, &[this]);
            let mips_calls = st.borrow().calls.clone();
            let mips_tbl = copy_out(&mut r, ctrl::G_UV_SCROLL_TABLE, 12 * 6);

            st.borrow_mut().calls.clear();
            ctrl::uv_scroll_node_draw(&mut m, &mut TestEnv(st.clone()), this, sp_init - 0xd0);
            assert_eq!(st.borrow().calls, mips_calls, "uv_scroll_node_draw case {case}");
            assert_eq!(mem_slice(&m, ctrl::G_UV_SCROLL_TABLE, 12 * 6), mips_tbl);

            // UVScrollNode_DrawMesh
            if slot >= 0 {
                let e = ctrl::G_UV_SCROLL_TABLE + (slot as u32) * 12;
                w32(&mut r, &mut m, e + 8, prev_flag);
            }
            st.borrow_mut().cull_ret = case % 2;
            let tex = 0x5555_0000 + case;
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1427b8, &[this, tex]);
            let mips_calls = st.borrow().calls.clone();
            let mips_tbl = copy_out(&mut r, ctrl::G_UV_SCROLL_TABLE, 12 * 6);

            st.borrow_mut().calls.clear();
            ctrl::uv_scroll_node_draw_mesh(&mut m, &mut TestEnv(st.clone()), this, tex, sp_init - 0x140);
            assert_eq!(st.borrow().calls, mips_calls, "uv_scroll_node_draw_mesh case {case}");
            assert_eq!(mem_slice(&m, ctrl::G_UV_SCROLL_TABLE, 12 * 6), mips_tbl);
        }

        // 4. cTexFlipNode::Draw 0x1433b8 & 9. cCrackedNode::Draw 0x1457a0
        {
            w32(&mut r, &mut m, this + 0x2d0, case % 2);
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1433b8, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::tex_flip_node_draw(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "tex_flip_node_draw case {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1457a0, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::cracked_node_draw(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "cracked_node_draw case {case}");
        }

        // 5,6,7,8: cFenceNode / cFlagNode dtor & Draw
        {
            let use_mesh = if case % 2 == 0 { mesh } else { 0 };
            w32(&mut r, &mut m, this + 0x54, use_mesh);
            w32(&mut r, &mut m, this + 0x4c, use_mesh);
            for k in 0..3u32 {
                let s = rng.f(0.1, 3.0).to_bits();
                w32(&mut r, &mut m, this + 0x1a4 + k * 4, s);
                w32(&mut r, &mut m, this + 0x190 + k * 4, s);
            }
            st.borrow_mut().cull_ret = (case / 2) % 2;

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x144578, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::fence_node_draw(&mut m, &mut TestEnv(st.clone()), this, sp_init - 0xe0);
            assert_eq!(st.borrow().calls, mc, "fence_node_draw case {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x145258, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::flag_node_draw(&mut m, &mut TestEnv(st.clone()), this, sp_init - 0xe0);
            assert_eq!(st.borrow().calls, mc, "flag_node_draw case {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x143800, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x60);
            st.borrow_mut().calls.clear();
            ctrl::fence_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc, "fence_node_dtor case {case}");
            assert_eq!(mem_slice(&m, this, 0x60), mn);

            w32(&mut r, &mut m, inst + 0xe4, this);
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1448c8, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x60);
            st.borrow_mut().calls.clear();
            ctrl::flag_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc, "flag_node_dtor case {case}");
            assert_eq!(mem_slice(&m, this, 0x60), mn);
        }

        // 10. cCrowdBoxNode::Draw 0x145fc0
        {
            st.borrow_mut().cull_ret = case % 2;
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x145fc0, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::crowd_box_node_draw(&mut m, &mut TestEnv(st.clone()), this, sp_init - 0x50);
            assert_eq!(st.borrow().calls, mc, "crowd_box_node_draw case {case}");
        }
    }
}

#[test]
fn trick_emitter_movie_railman_nodes_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0xE9E0_0002);

    for case in 0..80 {
        let mut m = fresh_mem(&mut r);
        setup_globals(&mut r, &mut m);
        let this = BASE + 0x2000;
        let inst = BASE + 0x3000;
        let params = BASE + 0x3400;
        let rider = BASE + 0x4000; // rider+0x5910 = BASE+0x9910
        let node_vt = BASE + 0x3600;

        let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
            r.mem.write_u32(a, v);
            m.w32(a, v);
        };
        let w16 = |r: &mut Runner, m: &mut Mem, a: u32, v: u16| {
            r.mem.write(a, &v.to_le_bytes());
            m.w16(a, v);
        };

        w32(&mut r, &mut m, this + 8, node_vt);
        w16(&mut r, &mut m, node_vt + 8, 4);
        w32(&mut r, &mut m, node_vt + 0xc, FAKE_NODE_DEL);
        w16(&mut r, &mut m, node_vt + 0xb0, 8);
        w32(&mut r, &mut m, node_vt + 0xb4, FAKE_NODE_FIN);

        w32(&mut r, &mut m, this + 0x28, inst);
        w32(&mut r, &mut m, inst + 0xc0, 0x3333_0000 + case);
        w32(&mut r, &mut m, inst + 0xe8, (case & 3) * 0x100);
        w32(&mut r, &mut m, rider + 0x460, case % 6);
        w32(&mut r, &mut m, rider + 0x5910, 1000 + case * 50);

        // 11. cTrickTriggerNode::cTrickTriggerNode 0x1478f0 & 12. dtor 0x147a80
        {
            w16(&mut r, &mut m, params + 0xc, (case as i16).wrapping_sub(20) as u16);
            w32(&mut r, &mut m, params + 0x10, 500 + case * 10);
            w32(&mut r, &mut m, params + 0x14, rng.f(0.0, 5.0).to_bits());
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1478f0, &[this, 0x77, inst, params, rider]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x100);

            st.borrow_mut().calls.clear();
            let rr = ctrl::trick_trigger_node_ctor(
                &mut m,
                &mut TestEnv(st.clone()),
                this,
                0x77,
                inst,
                params,
                rider,
            );
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc, "trick_trigger_node_ctor case {case}");
            assert_eq!(mem_slice(&m, this, 0x100), mn);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x147a80, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x60);
            st.borrow_mut().calls.clear();
            ctrl::trick_trigger_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc, "trick_trigger_node_dtor case {case}");
            assert_eq!(mem_slice(&m, this, 0x60), mn);
        }

        // 13. cTrickTriggerNode::Update 0x147ad8
        {
            w32(&mut r, &mut m, this + 8, node_vt);
            w32(&mut r, &mut m, this + 0x28, inst);
            w32(&mut r, &mut m, this + 0x38, 200);
            w32(&mut r, &mut m, this + 0x3c, 1000);
            w32(&mut r, &mut m, this + 0x48, case % 3);
            let mode = case % 6;
            if mode == 0 {
                w32(&mut r, &mut m, this + 0x4c, 0);
            } else {
                w32(&mut r, &mut m, this + 0x4c, rider);
                let c1 = BASE + 0xb000;
                let c2 = BASE + 0xb040;
                let p_other1 = BASE + 0xb100;
                let p_other2 = BASE + 0xb200;
                let o1 = BASE + 0xc000; // non-rider owner
                let o2_base = BASE + 0xd000;
                let o2 = o2_base + 0x82c; // rider owner
                w32(&mut r, &mut m, o1 + 0x14, 0x111);
                w32(&mut r, &mut m, p_other1 + 0x84, o1);
                w32(&mut r, &mut m, o2_base, if mode == 1 { 0 } else { rider });
                w32(&mut r, &mut m, o2 + 0x14, 0x3ef);
                w32(&mut r, &mut m, p_other2 + 0x84, o2);

                w32(&mut r, &mut m, c1, this + 0x50);
                w32(&mut r, &mut m, c1 + 4, p_other1);
                w32(&mut r, &mut m, c1 + 8, c2);
                w32(&mut r, &mut m, c2, p_other2);
                w32(&mut r, &mut m, c2 + 4, this + 0x50);
                w32(&mut r, &mut m, c2 + 12, 0);
                w32(&mut r, &mut m, this + 0x50, if mode == 5 { 0 } else { c1 });
                w32(&mut r, &mut m, rider + 0x424, if mode == 2 { 1 } else { 0 });
                w32(&mut r, &mut m, rider + 0x5910, if mode == 3 { 1100 } else { 1500 });
                st.borrow_mut().script_ret = if case % 12 == 4 { 0xffff_ffff } else { 9 };
                st.borrow_mut().alloc_ret = if case % 2 == 0 { BASE + 0xe000 } else { 0 };
            }

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x147ad8, &[this]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x60);
            let ms = copy_out(&mut r, BASE + 0xe000, 0xf0);

            st.borrow_mut().calls.clear();
            ctrl::trick_trigger_node_update(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "trick_trigger_node_update case {case}");
            assert_eq!(mem_slice(&m, this, 0x60), mn);
            assert_eq!(mem_slice(&m, BASE + 0xe000, 0xf0), ms);
        }

        // 14,15,16,17,18: cEmitterNode & cParticleNode
        {
            w32(&mut r, &mut m, this + 8, node_vt);
            w32(&mut r, &mut m, this + 0x18, inst);
            let inst_flags = match case % 4 {
                0 => 0x100,
                1 => 0x900,
                2 => 0x000,
                _ => 0x1ff,
            };
            w32(&mut r, &mut m, inst + 0xe8, inst_flags);
            let life_before: f32 = match (case / 4) % 3 {
                0 => -1.0,
                1 => 0.0,
                _ => 2.5,
            };
            let life_after: f32 = if case % 2 == 0 { 0.0 } else { 1.0 };
            w32(&mut r, &mut m, this + 0x20, life_before.to_bits());
            st.borrow_mut().particle_life_after = Some(life_after.to_bits());

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x147e60, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::emitter_node_update(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "emitter_node_update case {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x147f28, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::emitter_node_draw(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x147e10, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::emitter_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1481c8, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::particle_node_draw(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1480b0, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::particle_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);
        }

        // 19,20,21,22: cCollideEmitterNode
        {
            let mat = BASE + 0x3800;
            let vel = BASE + 0x3900;
            for k in 0..16u32 {
                w32(&mut r, &mut m, mat + k * 4, rng.f(-5.0, 5.0).to_bits());
            }
            for k in 0..3u32 {
                w32(&mut r, &mut m, vel + 0x54 + k * 4, rng.f(-50.0, 50.0).to_bits());
            }
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x148560, &[this, 0x12, inst, rider, mat, vel]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x80);
            let mcnt = copy_out(&mut r, ctrl::G_COLLIDE_EMITTER_COUNTS, 24);

            st.borrow_mut().calls.clear();
            let rr = ctrl::collide_emitter_node_ctor(
                &mut m,
                &mut TestEnv(st.clone()),
                this,
                0x12,
                inst,
                rider,
                mat,
                vel,
            );
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc, "collide_emitter_node_ctor case {case}");
            assert_eq!(mem_slice(&m, this, 0x80), mn);
            assert_eq!(mem_slice(&m, ctrl::G_COLLIDE_EMITTER_COUNTS, 24), mcnt);

            w32(&mut r, &mut m, this + 8, node_vt);
            let inst_flags = if case % 3 == 0 { 0x100 } else { 0x900 };
            w32(&mut r, &mut m, inst + 0xe8, inst_flags);
            st.borrow_mut().particle_life_after = Some((if case % 2 == 0 { -0.1f32 } else { 0.5f32 }).to_bits());
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x148948, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::collide_emitter_node_update(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "collide_emitter_node_update case {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1489e8, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::collide_emitter_node_draw(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x148890, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            let mcnt = copy_out(&mut r, ctrl::G_COLLIDE_EMITTER_COUNTS, 24);
            st.borrow_mut().calls.clear();
            ctrl::collide_emitter_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(mem_slice(&m, ctrl::G_COLLIDE_EMITTER_COUNTS, 24), mcnt);
        }

        // 23,24,25,26: cMovieNode
        {
            w32(&mut r, &mut m, this + 0x28, inst);
            w32(&mut r, &mut m, this + 0x2c, 0x40 + case);
            w32(&mut r, &mut m, inst + 0xe8, 0x1234_567f);
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x148a70, &[this, 0x33, inst]);
            let mc = st.borrow().calls.clone();
            let mi = copy_out(&mut r, inst + 0xe8, 4);
            st.borrow_mut().calls.clear();
            let rr = ctrl::movie_node_ctor(&mut m, &mut TestEnv(st.clone()), this, 0x33, inst);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc, "movie_node_ctor case {case}");
            assert_eq!(mem_slice(&m, inst + 0xe8, 4), mi);

            call_fn(&mut r, 0x148d20, &[this]);
            ctrl::movie_node_update();

            st.borrow_mut().movie_tex = 0x7777_0000 + case;
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x148d28, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::movie_node_draw(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "movie_node_draw case {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x148cc8, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            ctrl::movie_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc, "movie_node_dtor case {case}");
        }

        // 27,28: cRailManNode
        {
            let spl_base = BASE + 0x10000;
            st.borrow_mut().spline_base = spl_base;
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x148dc8, &[this, 0x99]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x230);
            let mg = r.mem.read_u32(ctrl::G_RAIL_MAN_NODE);

            st.borrow_mut().calls.clear();
            let rr = ctrl::rail_man_node_ctor(&mut m, &mut TestEnv(st.clone()), this, 0x99);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc, "rail_man_node_ctor case {case}");
            assert_eq!(mem_slice(&m, this, 0x230), mn);
            assert_eq!(m.r32(ctrl::G_RAIL_MAN_NODE), mg);

            // Populate several active spline entries and verify dtor restores flags
            for k in 0..8u32 {
                let idx = (k * 13 + case) % 128;
                let sid = (k * 3 + case) % 64;
                w16(&mut r, &mut m, this + 0x28 + idx * 4, sid as u16);
                let sp_addr = spl_base + sid * 0x40;
                w32(&mut r, &mut m, sp_addr + 0x18, (0xabcd_0000 + (sid << 16)) | 0x1234);
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x148f70, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            let mspl = copy_out(&mut r, spl_base, 64 * 0x40);
            let mg = r.mem.read_u32(ctrl::G_RAIL_MAN_NODE);

            st.borrow_mut().calls.clear();
            ctrl::rail_man_node_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc, "rail_man_node_dtor case {case}");
            assert_eq!(mem_slice(&m, spl_base, 64 * 0x40), mspl);
            assert_eq!(m.r32(ctrl::G_RAIL_MAN_NODE), mg);
        }
    }
}
