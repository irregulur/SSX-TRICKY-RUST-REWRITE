//! Function-runner check for row G7s (26 `render` boarder-effects, tex-flip, model/mesh-anim, particle,
//! mesh-scaling, owned-buffer, masked-list, and sphere-tree-cache/RLE functions).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::rendertail::{self as rt, RenderEnv};

const BASE: u32 = 0x0140_0000;
const FAKE_GFX_26C: u32 = 0x002f_f600;
const FAKE_GFX_284: u32 = 0x002f_f610;
const FAKE_GFX_2AC: u32 = 0x002f_f620;

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
        0x102f50, // Vec4_Scale
        0x135200, // BoarderRender_ResetEffects
        0x142d40, // TexFlip_ApplyFrame
        0x2049b0, // MeshChunk_ScalePositions
        0x2cc808, // EAMem_Set
    ] {
        r.allow(a);
    }
    Some(r)
}

fn call_fn(r: &mut Runner, addr: u32, args: &[u32], f12: Option<f32>) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    if let Some(f) = f12 {
        r.cpu.fpr[12] = f.to_bits();
    }
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn fresh_mem(r: &mut Runner) -> Mem {
    let z = vec![0u8; 0x30000];
    r.mem.write(BASE, &z);
    let zg = vec![0u8; 0x1000];
    r.mem.write(0x0033_7000, &zg);
    r.mem.write(0x0034_3000, &zg);
    r.mem.write(0x003b_c000, &zg);
    Mem {
        regions: vec![
            (0x0033_7000, vec![0u8; 0x1000]),
            (0x0034_3000, vec![0u8; 0x1000]),
            (0x003b_c000, vec![0u8; 0x1000]),
            (BASE, vec![0u8; 0x30000]),
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
    heap_cursor: u32,
    alloc_fail_every: u32,
    alloc_seq: u32,
}

impl EnvState {
    fn bump_alloc(&mut self, bytes: u32) -> u32 {
        self.alloc_seq += 1;
        if self.alloc_fail_every != 0 && self.alloc_seq % self.alloc_fail_every == 0 {
            return 0;
        }
        let p = self.heap_cursor;
        self.heap_cursor = (self.heap_cursor + bytes + 0xf) & !0xf;
        p
    }
}

struct TestEnv(Rc<RefCell<EnvState>>);

impl RenderEnv for TestEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        let mut s = self.0.borrow_mut();
        let snap = match target {
            FAKE_GFX_284 => {
                let mut v = mem_slice(m, args[1], 16);
                v.extend_from_slice(&mem_slice(m, args[2], 16));
                v
            }
            FAKE_GFX_26C => mem_slice(m, args[2], 16),
            _ => Vec::new(),
        };
        s.calls.push((target, args.to_vec(), snap));
        match target {
            FAKE_GFX_284 => s.cull_ret,
            rt::MEMPOOL_ALLOC => s.bump_alloc(args[1]),
            rt::EAMEM_ALLOC => s.bump_alloc(args[1]),
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: &Rc<RefCell<EnvState>>) {
    let stub_n = |r: &mut Runner, addr: u32, n: usize, has_f12: bool, snap_kind: u8| {
        let st = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut args = Vec::with_capacity(n + usize::from(has_f12));
            for k in 0..n {
                args.push(cpu.gpr(4 + k) as u32);
            }
            if has_f12 {
                args.push(cpu.fpr[12]);
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
                    let mut a = vec![0u8; 16];
                    mem.read(args[2], &mut a).unwrap();
                    a
                }
                _ => Vec::new(),
            };
            let mut s = st.borrow_mut();
            s.calls.push((addr, args.clone(), snap));
            let ret = match addr {
                FAKE_GFX_284 => s.cull_ret,
                rt::MEMPOOL_ALLOC => s.bump_alloc(args[1]),
                rt::EAMEM_ALLOC => s.bump_alloc(args[1]),
                _ => 0,
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    };

    stub_n(r, rt::MEM_DELETE, 1, false, 0);
    stub_n(r, rt::FX_BRAKE_SPRAY_RESET, 1, false, 0);
    stub_n(r, rt::FX_SPAWN_LANDING_PUFFS, 4, false, 0);
    stub_n(r, rt::FX_BOARD_PUFF_DESTRUCT, 2, false, 0);
    stub_n(r, rt::FX_BOARD_PUFF_RESET, 1, false, 0);
    stub_n(r, rt::FX_SPEED_DUST_DESTRUCT, 2, false, 0);
    stub_n(r, rt::FX_SPEED_DUST_RESET, 1, false, 0);
    stub_n(r, rt::FX_BOARD_TRAIL_RESET, 1, false, 0);
    stub_n(r, rt::FX_SURFACE_SPRAY_RESET, 1, false, 0);
    stub_n(r, rt::FX_BOARD_TRACK_RESET, 1, false, 0);
    stub_n(r, rt::BOARDER_RENDER_GET_HAND_STREAKS, 2, false, 0);
    stub_n(r, rt::BOARDER_RENDER_COPY_MATRIX, 2, false, 0);
    stub_n(r, rt::MEM_FREE, 1, false, 0);
    stub_n(r, rt::ANIM_FRAME_SET_KEY, 2, true, 0);
    stub_n(r, rt::ANIM_FRAME_CLEAR, 1, false, 0);
    stub_n(r, rt::ANIM_FRAME_SET_IDENTITY, 1, false, 0);
    stub_n(r, rt::SKEL_SEQ_VF1, 1, false, 0);
    stub_n(r, rt::MDF_MEM_BANK_INIT, 2, false, 0);
    stub_n(r, rt::MEMPOOL_ALLOC, 6, false, 0);
    stub_n(r, rt::EAMEM_ALLOC, 3, false, 0);

    stub_n(r, FAKE_GFX_26C, 4, true, 2);
    stub_n(r, FAKE_GFX_284, 3, false, 1);
    stub_n(r, FAKE_GFX_2AC, 7, false, 0);
}

fn setup_globals(r: &mut Runner, m: &mut Mem) -> (u32, u32) {
    let app = BASE + 0x100;
    let gfx = BASE + 0x900;
    let gfx_vt = BASE + 0xa00;
    let game = BASE + 0xe00;
    let race = BASE + 0xf00;

    let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
        r.mem.write_u32(a, v);
        m.w32(a, v);
    };
    let w16 = |r: &mut Runner, m: &mut Mem, a: u32, v: u16| {
        r.mem.write(a, &v.to_le_bytes());
        m.w16(a, v);
    };
    w32(r, m, rt::G_APP, app);
    w32(r, m, app + 0x724, gfx);
    w32(r, m, app + 0x730, game);
    w32(r, m, game + 0x1c, race);
    w32(r, m, gfx + 0x10, gfx_vt);
    w16(r, m, gfx_vt + 0x268, 4);
    w32(r, m, gfx_vt + 0x26c, FAKE_GFX_26C);
    w16(r, m, gfx_vt + 0x280, 8);
    w32(r, m, gfx_vt + 0x284, FAKE_GFX_284);
    w16(r, m, gfx_vt + 0x2a8, 12);
    w32(r, m, gfx_vt + 0x2ac, FAKE_GFX_2AC);
    (gfx, race)
}

#[test]
fn boarder_texflip_model_particle_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0x6750_0001);

    call_fn(&mut r, 0x1d98f8, &[], None);
    let sp_init = r.cpu.gpr(29) as u32;
    let stack_base = (sp_init - 0x400) & !0xf;

    for case in 0..80u32 {
        let mut m = fresh_mem(&mut r);
        let (_gfx, race) = setup_globals(&mut r, &mut m);
        let zstack = vec![0u8; 0x400];
        r.mem.write(stack_base, &zstack);
        m.regions.push((stack_base, zstack));

        let this = BASE + 0x2000;
        let rider = BASE + 0x3000;
        let inst = BASE + 0x3800;

        let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
            r.mem.write_u32(a, v);
            m.w32(a, v);
        };
        let w16 = |r: &mut Runner, m: &mut Mem, a: u32, v: u16| {
            r.mem.write(a, &v.to_le_bytes());
            m.w16(a, v);
        };

        // 1. RiderSkin_InitMem 0x123060
        {
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x123060, &[this, 0x111 + case, 0x222 + case], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::rider_skin_init_mem(&mut m, &mut TestEnv(st.clone()), this, 0x111 + case, 0x222 + case);
            assert_eq!(st.borrow().calls, mc, "rider_skin_init_mem {case}");
        }

        // 2,3,5,6: BoarderRender_ResetEffects / SpawnLandingPuffs / Reset / DestructEffects
        {
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x135200, &[this], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::boarder_render_reset_effects(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x135268, &[this, 10 + case, 20, 30], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::boarder_render_spawn_landing_puffs(&mut m, &mut TestEnv(st.clone()), this, 10 + case, 20, 30);
            assert_eq!(st.borrow().calls, mc);

            w32(&mut r, &mut m, this + 0x3e0, BASE + 0x5000);
            for off in [0x1e0u32, 0x1e4, 0x3c0, 0x3c4] {
                w32(&mut r, &mut m, this + off, 77);
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x136ea0, &[this], None);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x3f0);
            st.borrow_mut().calls.clear();
            rt::boarder_render_reset(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(mem_slice(&m, this, 0x3f0), mn);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x136f88, &[this, case % 4], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::boarder_render_destruct_effects(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);
        }

        // 4. BoarderRender_RecordStreaks 0x136148
        {
            for (k, base) in [rt::G_STREAK_COLOR_LOW, rt::G_STREAK_COLOR_MID, rt::G_STREAK_COLOR_HIGH]
                .into_iter()
                .enumerate()
            {
                for j in 0..3u32 {
                    w32(&mut r, &mut m, base + j * 4, ((k as u32 + 1) * 0x100 + j).to_bits_u32_dummy());
                }
            }
            w32(&mut r, &mut m, race + 0x18, case % 5);
            w32(&mut r, &mut m, this, rider);
            let f1: f32 = match case % 5 {
                0 => -0.5,
                1 => 0.0,
                2 => 0.2,
                3 => 0.5,
                _ => 0.9,
            };
            let f0: f32 = if (case / 5) % 3 == 0 { 0.3 } else { 0.0 };
            w32(&mut r, &mut m, rider + 0x130, f1.to_bits());
            w32(&mut r, &mut m, rider + 0x134, f0.to_bits());
            w32(&mut r, &mut m, rider + 0x428, if case % 2 == 0 { 0x10 } else { 0x05 });
            w32(&mut r, &mut m, this + 0x1e0, case % 7);
            w32(&mut r, &mut m, this + 0x1e4, case % 9);
            w32(&mut r, &mut m, this + 0x3c0, (case + 3) % 7);
            w32(&mut r, &mut m, this + 0x3c4, (case + 2) % 9);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x136148, &[this], None);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x3e0);
            st.borrow_mut().calls.clear();
            rt::boarder_render_record_streaks(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "record_streaks calls {case}");
            assert_eq!(mem_slice(&m, this, 0x3e0), mn, "record_streaks mem {case}");
        }

        // 7,8,9: TexFlip_ApplyFrame 0x142d40, TexFlip_Draw 0x142da8, cTexFlip::~cTexFlip 0x1499e8
        {
            let tbl = BASE + 0x6000;
            w32(&mut r, &mut m, this, inst);
            w32(&mut r, &mut m, this + 4, case % 4);
            w32(&mut r, &mut m, this + 8, case % 4);
            w32(&mut r, &mut m, inst + 0xc0, 0x4444_0000 + case);
            w32(&mut r, &mut m, inst + 0xf0, tbl);
            for k in 0..6u32 {
                w32(&mut r, &mut m, inst + 0xcc + k * 4, rng.f(-100.0, 100.0).to_bits());
            }
            for i in 0..4u32 {
                let mat = BASE + 0x6100 + i * 0x80;
                let anim = BASE + 0x6400 + i * 0x40;
                w32(&mut r, &mut m, tbl + 4 + i * 4, mat);
                w32(&mut r, &mut m, mat + 0x44, if (case + i) % 3 == 0 { 0 } else { anim });
                for fr in 0..4u32 {
                    w16(&mut r, &mut m, anim + 4 + fr * 4, (0x100 + i * 16 + fr) as u16);
                }
            }
            st.borrow_mut().cull_ret = case % 2;
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x142da8, &[this], None);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x280);
            st.borrow_mut().calls.clear();
            rt::tex_flip_draw(&mut m, &mut TestEnv(st.clone()), this, sp_init - 0x50);
            assert_eq!(st.borrow().calls, mc, "tex_flip_draw calls {case}");
            assert_eq!(mem_slice(&m, this, 0x280), mn, "tex_flip_draw mem {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1499e8, &[this, case % 4], None);
            let mc = st.borrow().calls.clone();
            let mv = r.mem.read_u32(this + 0x274);
            st.borrow_mut().calls.clear();
            rt::tex_flip_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(m.r32(this + 0x274), mv);
        }

        // 10,11: Model_IsBoxCulled 0x19aa18, MeshAnim_AllocFrames 0x19aab0
        {
            w32(&mut r, &mut m, this + 0x58, inst);
            st.borrow_mut().cull_ret = case % 3;
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x19aa18, &[this], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::model_is_box_culled(&mut m, &mut TestEnv(st.clone()), this, sp_init - 0x30);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            let model = BASE + 0x7000;
            let parts = BASE + 0x7100;
            w32(&mut r, &mut m, inst + 0xc0, model);
            w32(&mut r, &mut m, this + 0x24, rng.f(0.1, 4.0).to_bits());
            let n_parts = case % 6;
            w32(&mut r, &mut m, model + 4, n_parts);
            w32(&mut r, &mut m, model + 8, parts);
            for i in 0..n_parts {
                let key = if ((case >> i) & 1) != 0 { 0x8800_0000 + i } else { 0 };
                w32(&mut r, &mut m, parts + i * 0x18 + 0x10, key);
            }
            {
                let mut s = st.borrow_mut();
                s.calls.clear();
                s.heap_cursor = BASE + 0x10000;
                s.alloc_fail_every = 0;
                s.alloc_seq = 0;
            }
            call_fn(&mut r, 0x19aab0, &[this], None);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x70);
            let mheap = copy_out(&mut r, BASE + 0x10000, 0x600);

            {
                let mut s = st.borrow_mut();
                s.calls.clear();
                s.heap_cursor = BASE + 0x10000;
                s.alloc_fail_every = 0;
                s.alloc_seq = 0;
            }
            rt::mesh_anim_alloc_frames(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "mesh_anim_alloc_frames calls {case}");
            assert_eq!(mem_slice(&m, this, 0x70), mn);
            assert_eq!(mem_slice(&m, BASE + 0x10000, 0x600), mheap);
        }

        // 12,13,14,15,16: ParticleSys_SetSpawnPoint / SetForce / SetVelocityCone / ParticleNode_FreeMem / cParticleMan::vf1
        {
            let pt = BASE + 0x8000;
            for k in 0..4u32 {
                w32(&mut r, &mut m, pt + k * 4, rng.f(-50.0, 50.0).to_bits());
            }
            call_fn(&mut r, 0x1d5338, &[this, pt], None);
            let mn = copy_out(&mut r, this + 0xa0, 0x30);
            rt::particle_sys_set_spawn_point(&mut m, this, pt);
            assert_eq!(mem_slice(&m, this + 0xa0, 0x30), mn);

            w32(&mut r, &mut m, this + 0xc, rng.f(0.1, 10.0).to_bits());
            w32(&mut r, &mut m, this + 0x3c, rng.f(0.2, 5.0).to_bits());
            let fscale = rng.f(0.2, 5.0);
            call_fn(&mut r, 0x1d54e0, &[this, pt], Some(fscale));
            let mn = copy_out(&mut r, this + 0xd0, 0x10);
            rt::particle_sys_set_force(&mut m, this, pt, fscale);
            assert_eq!(mem_slice(&m, this + 0xd0, 0x10), mn, "set_force {case}");

            let vx = if case % 4 == 0 { 0.0f32 } else { rng.f(-20.0, 20.0) };
            let vy = rng.f(0.5, 20.0);
            let vz = rng.f(-20.0, 20.0);
            w32(&mut r, &mut m, pt, vx.to_bits());
            w32(&mut r, &mut m, pt + 4, vy.to_bits());
            w32(&mut r, &mut m, pt + 8, vz.to_bits());
            w32(&mut r, &mut m, pt + 12, 0.0f32.to_bits());
            let spread = rng.f(0.0, 2.0);
            call_fn(&mut r, 0x1d5570, &[this, pt], Some(spread));
            let mn = copy_out(&mut r, this + 0x60, 0x40);
            rt::particle_sys_set_velocity_cone(&mut m, this, pt, spread);
            assert_eq!(mem_slice(&m, this + 0x60, 0x40), mn, "set_velocity_cone {case}");

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1d8b00, &[this, case % 4], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::particle_node_free_mem(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);

            call_fn(&mut r, 0x1d98f8, &[this], None);
            rt::particle_man_vf1();
        }
    }
}

trait DummyBits {
    fn to_bits_u32_dummy(self) -> u32;
}
impl DummyBits for u32 {
    fn to_bits_u32_dummy(self) -> u32 {
        (self as f32).to_bits()
    }
}

#[test]
fn mesh_scale_ownedbuf_masked_sphtree_rle_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0x6750_0002);

    call_fn(&mut r, 0x1d98f8, &[], None);
    let sp_init = r.cpu.gpr(29) as u32;
    let stack_base = (sp_init - 0x400) & !0xf;

    for case in 0..60u32 {
        let mut m = fresh_mem(&mut r);
        setup_globals(&mut r, &mut m);
        let zstack = vec![0u8; 0x400];
        r.mem.write(stack_base, &zstack);
        m.regions.push((stack_base, zstack));

        let this = BASE + 0x2000;
        let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
            r.mem.write_u32(a, v);
            m.w32(a, v);
        };
        let w8 = |r: &mut Runner, m: &mut Mem, a: u32, v: u8| {
            r.mem.write(a, &[v]);
            m.w8(a, v);
        };

        // 17. SkelSeq_DtorMember34 0x203d40
        {
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x203d40, &[this], None);
            let mc = st.borrow().calls.clone();
            let mv = r.mem.read_u32(this + 0x34);
            st.borrow_mut().calls.clear();
            let rr = rt::skel_seq_dtor_member34(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(m.r32(this + 0x34), mv);
        }

        // 18 & 19. MeshChunk_ScalePositions 0x2049b0 & Model_ScalePositions 0x204f28
        {
            let build_chunk = |r: &mut Runner, m: &mut Mem, base: u32, n_strips: u32, verts_per: u32, num_morphs: u32, rng: &mut Rng| -> u32 {
                let qwc0 = 4 + n_strips;
                w32(r, m, base, qwc0);
                w32(r, m, base + 0x10 + 0x20, n_strips);
                for i in 0..n_strips {
                    w32(r, m, base + 0x10 + 0x30 + i * 0x10, verts_per);
                }
                let p1 = base + 0x10 + qwc0 * 0x10;
                let total = n_strips * verts_per;
                let qwc1 = 0x20 + total;
                w32(r, m, p1, qwc1);
                for off in (0x10..((qwc1 + 1) * 0x10)).step_by(4) {
                    w32(r, m, p1 + off, rng.f(-100.0, 100.0).to_bits());
                }
                let p2 = p1 + 0x10 + qwc1 * 0x10;
                let qwc2 = 2;
                w32(r, m, p2, qwc2);
                let mut pm = p2 + 0x10 + qwc2 * 0x10;
                for k in 0..num_morphs {
                    let qwcm = 1 + (k % 3);
                    w32(r, m, pm, qwcm);
                    pm += 0x10 + qwcm * 0x10;
                }
                pm - base
            };

            let chunk = BASE + 0x5000;
            let n_strips = case % 4;
            let verts_per = 1 + (case % 5);
            let num_morphs = case % 3;
            let span = build_chunk(&mut r, &mut m, chunk, n_strips, verts_per, num_morphs, &mut rng);
            let scale = rng.f(-3.0, 3.0);

            let mr = call_fn(&mut r, 0x2049b0, &[chunk, num_morphs], Some(scale));
            let mbytes = copy_out(&mut r, chunk, span as usize);
            let rr = rt::mesh_chunk_scale_positions(&mut m, chunk, num_morphs as i32, scale);
            assert_eq!(rr, mr, "mesh_chunk_scale_positions ret {case}");
            assert_eq!(mem_slice(&m, chunk, span as usize), mbytes, "mesh_chunk_scale_positions mem {case}");

            // Model_ScalePositions
            let model = BASE + 0x9000;
            let groups = BASE + 0x9100;
            let arr0 = BASE + 0x9200;
            let c0 = BASE + 0xa000;
            let c1 = BASE + 0xb000;
            let s0 = build_chunk(&mut r, &mut m, c0, 2, 3, 1, &mut rng);
            let s1 = build_chunk(&mut r, &mut m, c1, 1, 4, 1, &mut rng);
            w32(&mut r, &mut m, model, if case % 3 == 0 { 0 } else { 0x100 });
            w32(&mut r, &mut m, model + 0xc, 2);
            w32(&mut r, &mut m, model + 0x10, groups);
            w32(&mut r, &mut m, groups, arr0);
            w32(&mut r, &mut m, groups + 4, 2);
            w32(&mut r, &mut m, groups + 8, arr0);
            w32(&mut r, &mut m, groups + 12, 0);
            w32(&mut r, &mut m, arr0, c0);
            w32(&mut r, &mut m, arr0 + 0xc, c1);

            call_fn(&mut r, 0x204f28, &[model, 0, 1], Some(scale));
            let mb0 = copy_out(&mut r, c0, s0 as usize);
            let mb1 = copy_out(&mut r, c1, s1 as usize);
            rt::model_scale_positions(&mut m, model, 1, scale);
            assert_eq!(mem_slice(&m, c0, s0 as usize), mb0);
            assert_eq!(mem_slice(&m, c1, s1 as usize), mb1);
        }

        // 20 & 21. OwnedBuf_Delete1 0x21e788 & OwnedBuf_Delete2 0x235e70
        for &addr in &[0x21e788u32, 0x235e70] {
            w32(&mut r, &mut m, this + 4, if case % 2 == 0 { BASE + 0x1234 } else { 0 });
            st.borrow_mut().calls.clear();
            call_fn(&mut r, addr, &[this, case % 4], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::owned_buf_delete(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc, "owned_buf_delete 0x{addr:x} {case}");
        }

        // 22. MaskedList_Draw 0x238980
        {
            w32(&mut r, &mut m, this + 0x20, rng.f(1.0, 20.0).to_bits());
            w32(&mut r, &mut m, this + 0x24, rng.next());
            let cnt = case % 6;
            w32(&mut r, &mut m, this + 0x28, cnt);
            for i in 0..cnt {
                w32(&mut r, &mut m, this + 0x40 + i * 0x20, rng.f(0.5, 10.0).to_bits());
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x238980, &[this], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::masked_list_draw(&mut m, &mut TestEnv(st.clone()), this, sp_init - 0x80);
            assert_eq!(st.borrow().calls, mc, "masked_list_draw {case}");
        }

        // 23,24,26: SphereTreeCache_Init 0x238af0, SphereTreeBufCache_FreeAll 0x238dd0, ~SphereTreeBufCache 0x23b520
        {
            let na = (case % 4) as i32;
            let da = ((case / 4) % 3) as i32 - 1;
            let nb = ((case / 3) % 3) as i32;
            let db = ((case / 2) % 3) as i32 - 1;
            {
                let mut s = st.borrow_mut();
                s.calls.clear();
                s.heap_cursor = BASE + 0x10000;
                s.alloc_fail_every = 0;
                s.alloc_seq = 0;
            }
            call_fn(&mut r, 0x238af0, &[this, na as u32, da as u32, nb as u32, db as u32], None);
            let mc = st.borrow().calls.clone();
            let mg = copy_out(&mut r, 0x0034_39a0, 0x24);
            let mheap = copy_out(&mut r, BASE + 0x10000, 0x400);

            {
                let mut s = st.borrow_mut();
                s.calls.clear();
                s.heap_cursor = BASE + 0x10000;
                s.alloc_fail_every = 0;
                s.alloc_seq = 0;
            }
            rt::sphere_tree_cache_init(&mut m, &mut TestEnv(st.clone()), this, na, da, nb, db);
            assert_eq!(st.borrow().calls, mc, "sphere_tree_cache_init calls {case}");
            assert_eq!(mem_slice(&m, 0x0034_39a0, 0x24), mg);
            assert_eq!(mem_slice(&m, BASE + 0x10000, 0x400), mheap);

            // FreeAll
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x238dd0, &[], None);
            let mc = st.borrow().calls.clone();
            let mg = copy_out(&mut r, 0x0034_39a0, 0x24);
            st.borrow_mut().calls.clear();
            rt::sphere_tree_buf_cache_free_all(&mut m, &mut TestEnv(st.clone()));
            assert_eq!(st.borrow().calls, mc, "sphere_tree_cache_free_all calls {case}");
            assert_eq!(mem_slice(&m, 0x0034_39a0, 0x24), mg);

            // ~SphereTreeBufCache
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x23b520, &[this, case % 4], None);
            let mc = st.borrow().calls.clone();
            let mv = r.mem.read_u32(this);
            st.borrow_mut().calls.clear();
            rt::sphere_tree_buf_cache_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(m.r32(this), mv);
        }

        // 25. Rle_Encode 0x239960
        {
            let src = BASE + 0x20000;
            let dst = BASE + 0x21000;
            let len = match case % 8 {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 140,
                _ => 5 + (case % 40),
            };
            let mut v = 0u8;
            for i in 0..len {
                if rng.next() % 3 == 0 {
                    v = (rng.next() & 7) as u8;
                }
                w8(&mut r, &mut m, src + i, v);
            }
            // Write sentinel byte right past src+len to test the exact end-of-buffer read quirk.
            w8(&mut r, &mut m, src + len, if case % 2 == 0 { v } else { v ^ 1 });
            let min_run = (case % 5) as i32;
            let mr = call_fn(&mut r, 0x239960, &[dst, src, len, min_run as u32], None);
            let mbytes = copy_out(&mut r, dst, mr as usize);
            let rr = rt::rle_encode(&mut m, dst, src, len, min_run);
            assert_eq!(rr, mr, "rle_encode len {case} (input len={len}, min_run={min_run})");
            assert_eq!(mem_slice(&m, dst, rr as usize), mbytes, "rle_encode bytes {case}");
        }
    }
}
