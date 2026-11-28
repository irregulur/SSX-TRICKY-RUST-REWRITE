//! Function-runner check for row G7u (32 `render` state-pool, mesh-cache, display-mode, texture-upload/projection,
//! and grid-mesh/strip-packet functions).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::rendertail::{self as rt, RenderEnv};

const BASE: u32 = 0x0140_0000;
const FAKE_VF2C: u32 = 0x002f_f800;
const FAKE_VF3C: u32 = 0x002f_f810;
const FAKE_VF144: u32 = 0x002f_f820;
const FAKE_VF15C: u32 = 0x002f_f830;
const FAKE_VF1B4: u32 = 0x002f_f840;
const FAKE_MESH_DEL: u32 = 0x002f_f850;
const FAKE_GM_VF44: u32 = 0x002f_f860;

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
        0x1c4158, // PS2Draw_FreeTables
        0x1c41a0, // Gfx_InitStatePool
        0x1deb40, // PS2Render_LinkMesh
        0x1debd0, // PS2Render_UnlinkMesh
        0x1dec38, // PS2Render_FlushMeshCache
        0x1e6670, // cGridMesh::~cGridMesh
    ] {
        r.allow(a);
    }
    Some(r)
}

fn call_fn(r: &mut Runner, addr: u32, args: &[u32]) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        if k < 8 {
            r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
        }
    }
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn fresh_mem(r: &mut Runner) -> Mem {
    let z = vec![0u8; 0x40000];
    r.mem.write(BASE, &z);
    Mem {
        regions: vec![(BASE, vec![0u8; 0x40000])],
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

#[derive(Default)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
    vf3c_ret: i32,
}

struct TestEnv(Rc<RefCell<EnvState>>);

impl RenderEnv for TestEnv {
    fn call(&mut self, _m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        let mut s = self.0.borrow_mut();
        s.calls.push((target, args.to_vec()));
        match target {
            FAKE_VF3C => s.vf3c_ret as u32,
            FAKE_VF2C | FAKE_VF15C | FAKE_GM_VF44 | rt::TEXMAN_REGISTER_TEXTURE | rt::TEXMAN_INIT_LOAD_NODES | rt::TEXMAN_UPLOAD_TEXTURES | rt::TEXMAN_FREE_ALL => {
                0x2468_0000 | (target & 0xffff)
            }
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: &Rc<RefCell<EnvState>>) {
    let stub = |r: &mut Runner, addr: u32, n_gpr: usize, n_stk: usize, fixed_ret: Option<u32>| {
        let st = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut args = Vec::with_capacity(n_gpr + n_stk);
            for k in 0..n_gpr {
                args.push(cpu.gpr(4 + k) as u32);
            }
            let sp = cpu.gpr(29) as u32;
            for k in 0..n_stk {
                args.push(mem.read_u32(sp + (k as u32) * 8));
            }
            let mut s = st.borrow_mut();
            s.calls.push((addr, args));
            let ret = if addr == FAKE_VF3C {
                s.vf3c_ret as u32
            } else {
                fixed_ret.unwrap_or(0)
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    };

    stub(r, rt::MEM_DELETE, 1, 0, Some(0));
    stub(r, rt::MEM_FREE, 1, 0, Some(0));
    stub(r, rt::GFX_STOP_VIF1_IRQ, 1, 0, Some(0));
    stub(r, rt::GFX_UPDATE_VIF1_IRQ, 1, 0, Some(0));
    stub(r, rt::TEXMAN_SHUTDOWN, 1, 0, Some(0));
    stub(r, rt::TEXMAN_REGISTER_TEXTURE, 8, 2, Some(0x2468_0ef8));
    stub(r, rt::TEXMAN_UPLOAD_TEXTURES, 5, 0, Some(0x2468_1f50));
    stub(r, rt::TEXMAN_UPDATE_LOD, 1, 0, Some(0));
    stub(r, rt::TEXMAN_INIT_LOAD_NODES, 1, 0, Some(0x2468_3480));
    stub(r, rt::TEXMAN_FREE_ALL, 1, 0, Some(0x2468_3560));
    stub(r, rt::PS2_DRAW_STATE_INIT, 2, 0, Some(0));
    stub(r, rt::PS2_DRAW_CLEAR_BUCKETS, 1, 0, Some(0));
    stub(r, rt::PS2_DRAW_FLUSH_BUCKETS, 2, 0, Some(0));
    stub(r, rt::GFX_SET_DISPLAY_MODE, 8, 0, Some(0));
    stub(r, rt::REMOVE_INTC_HANDLER, 2, 0, Some(0));
    stub(r, rt::REMOVE_DMAC_HANDLER, 2, 0, Some(0));
    stub(r, rt::DELETE_THREAD, 1, 0, Some(0));
    stub(r, rt::TERMINATE_THREAD, 1, 0, Some(0));
    stub(r, rt::DELETE_SEMA, 1, 0, Some(0));
    stub(r, rt::SIGNAL_SEMA, 1, 0, Some(0));
    stub(r, rt::WAIT_SEMA, 1, 0, Some(0));
    stub(r, rt::DISABLE_INTC, 1, 0, Some(0));
    stub(r, rt::DISABLE_DMAC, 1, 0, Some(0));

    stub(r, FAKE_VF2C, 2, 0, Some(0x2468_f800));
    stub(r, FAKE_VF3C, 5, 0, None);
    stub(r, FAKE_VF144, 1, 0, Some(0));
    stub(r, FAKE_VF15C, 8, 2, Some(0x2468_f830));
    stub(r, FAKE_VF1B4, 1, 0, Some(0));
    stub(r, FAKE_MESH_DEL, 2, 0, Some(0));
    stub(r, FAKE_GM_VF44, 3, 0, Some(0x2468_f860));
}

#[test]
fn state_pool_mesh_cache_display_modes_and_grid_mesh_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0x6700_0001);

    for case in 0..40u32 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x1000;
        let vt = BASE + 0x4000;
        let draw = BASE + 0x10000; // needs 0x10000 bytes for Gfx_InitStatePool (+0xfa64)
        let texman = BASE + 0x22000;
        let tex_entries = BASE + 0x23000;
        let mode_tbl = BASE + 0x25000;
        let aux = BASE + 0x28000;

        let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
            r.mem.write_u32(a, v);
            m.w32(a, v);
        };
        let w16 = |r: &mut Runner, m: &mut Mem, a: u32, v: u16| {
            r.mem.write(a, &v.to_le_bytes());
            m.w16(a, v);
        };

        w32(&mut r, &mut m, this + 0x10, vt);
        w16(&mut r, &mut m, vt + 0x28, 4);
        w32(&mut r, &mut m, vt + 0x2c, FAKE_VF2C);
        w16(&mut r, &mut m, vt + 0x38, 8);
        w32(&mut r, &mut m, vt + 0x3c, FAKE_VF3C);
        w16(&mut r, &mut m, vt + 0x140, 12);
        w32(&mut r, &mut m, vt + 0x144, FAKE_VF144);
        w16(&mut r, &mut m, vt + 0x158, 16);
        w32(&mut r, &mut m, vt + 0x15c, FAKE_VF15C);
        w16(&mut r, &mut m, vt + 0x1b0, 20);
        w32(&mut r, &mut m, vt + 0x1b4, FAKE_VF1B4);
        w32(&mut r, &mut m, this + 0x47c, texman);
        w32(&mut r, &mut m, this + 0x480, draw);
        w32(&mut r, &mut m, texman + 0x10, tex_entries);

        // 1. PS2Draw_FreeTables 0x1c4158 & Gfx_InitStatePool 0x1c41a0
        {
            w32(&mut r, &mut m, draw + 4, if case % 2 == 0 { BASE + 0x30000 } else { 0 });
            w32(&mut r, &mut m, draw + 0xc, if case % 3 == 0 { BASE + 0x30100 } else { 0 });
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1c4158, &[draw]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::ps2_draw_free_tables(&mut m, &mut TestEnv(st.clone()), draw);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1c41a0, &[draw]);
            let mc = st.borrow().calls.clone();
            let md0 = copy_out(&mut r, draw, 0x20);
            let md1 = copy_out(&mut r, draw + 0x3020, 0x10);
            let md2 = copy_out(&mut r, draw + 0x3a20, 0x10);
            let md3 = copy_out(&mut r, draw + 0xfa40, 0x30);
            st.borrow_mut().calls.clear();
            rt::gfx_init_state_pool(&mut m, &mut TestEnv(st.clone()), draw);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(mem_slice(&m, draw, 0x20), md0);
            assert_eq!(mem_slice(&m, draw + 0x3020, 0x10), md1);
            assert_eq!(mem_slice(&m, draw + 0x3a20, 0x10), md2);
            assert_eq!(mem_slice(&m, draw + 0xfa40, 0x30), md3);
        }

        // 2. cPS2GraphicsMan::vf2_1dc168 (shutdown)
        {
            w32(&mut r, &mut m, this + 0x3f8, if case % 2 == 0 { BASE + 0x31000 } else { 0 });
            w32(&mut r, &mut m, this + 0x6f8, 10 + case);
            w32(&mut r, &mut m, this + 0x6fc, 20 + case);
            w32(&mut r, &mut m, this + 0x700, 30 + case);
            w32(&mut r, &mut m, this + 0x704, 40 + case);
            w32(&mut r, &mut m, this + 0x708, 50 + case);
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1dc168, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::ps2_gfx_shutdown(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "vf2_1dc168 {case}");
        }

        // 3. vf6_1dc210, vf5_1dc2c8, vf4_1dc328, vf25_1dc3b0, vf35_1dd450
        {
            w32(&mut r, &mut m, this + 0x64c, 3);
            w32(&mut r, &mut m, this + 0x650, case % 3);
            w32(&mut r, &mut m, this + 0x654, mode_tbl);
            for (i, &(w, h, psm, zpsm, il)) in [(640u32, 448u32, 0u32, 0x31u32, 1u32), (640, 224, 1, 0x31, 0), (512, 448, 0, 0x32, 1)].iter().enumerate() {
                let e = mode_tbl + (i as u32) * 0x94;
                w32(&mut r, &mut m, e + 0x80, w);
                w32(&mut r, &mut m, e + 0x84, h);
                w32(&mut r, &mut m, e + 0x88, psm);
                w32(&mut r, &mut m, e + 0x8c, zpsm);
                w32(&mut r, &mut m, e + 0x90, il);
            }
            let (qw, qh, qb, qz) = match case % 4 {
                0 => (640, 448, 32, 24),
                1 => (640, 224, 24, 24),
                2 => (512, 448, 32, 24),
                _ => (640, 448, 16, 16),
            };
            assert_eq!(
                call_fn(&mut r, 0x1dc210, &[this, qw, qh, qb, qz]) as i32,
                rt::ps2_gfx_find_mode(&m, this, qw, qh, qb, qz)
            );

            st.borrow_mut().vf3c_ret = if case % 2 == 0 { 1 } else { -1 };
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1dc2c8, &[this, qw, qh, qb, qz]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_select_mode(&mut m, &mut TestEnv(st.clone()), this, qw, qh, qb, qz);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            let new_idx = (case + 1) % 3;
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1dc328, &[this, new_idx]);
            let mc = st.borrow().calls.clone();
            let mi = r.mem.read_u32(this + 0x650);
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_set_mode_index(&mut m, &mut TestEnv(st.clone()), this, new_idx);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(m.r32(this + 0x650), mi);

            w32(&mut r, &mut m, this + 0xe4, case % 3);
            let e = this + 0x14 + (case % 3) * 0x34;
            w32(&mut r, &mut m, e + 0x14, rng.next());
            w32(&mut r, &mut m, e + 0x18, rng.next());
            w32(&mut r, &mut m, e + 0x1c, rng.next());
            call_fn(&mut r, 0x1dc3b0, &[this, aux, aux + 4, aux + 8]);
            let mo = copy_out(&mut r, aux, 12);
            rt::ps2_gfx_get_viewport_params(&mut m, this, aux, aux + 4, aux + 8);
            assert_eq!(mem_slice(&m, aux, 12), mo);

            call_fn(&mut r, 0x1dd450, &[this, case % 4]);
            let mb = copy_out(&mut r, this + 0x17d4, 20);
            rt::ps2_gfx_set_blend_preset(&mut m, this, case % 4);
            assert_eq!(mem_slice(&m, this + 0x17d4, 20), mb);
        }

        // 4. Mesh cache: ResetMeshCache 0x1dea88, LinkMesh 0x1deb40, UnlinkMesh 0x1debd0,
        //    FlushMeshCache 0x1dec38, vf69_1dee58, vf15_1dd4f0
        {
            call_fn(&mut r, 0x1dea88, &[this]);
            let mc_mem = copy_out(&mut r, this + 0x1870, 0x520);
            rt::ps2_render_reset_mesh_cache(&mut m, this);
            assert_eq!(mem_slice(&m, this + 0x1870, 0x520), mc_mem, "reset_mesh_cache {case}");

            let mesh0 = aux + 0x100;
            let mesh1 = aux + 0x200;
            let mesh_vt = aux + 0x300;
            let node0 = this + 0x1870;
            let node1 = this + 0x1870 + 0x14;
            w32(&mut r, &mut m, mesh0, mesh_vt);
            w32(&mut r, &mut m, mesh1, mesh_vt);
            w32(&mut r, &mut m, mesh0 + 0x54, node0);
            w32(&mut r, &mut m, mesh1 + 0x54, node1);
            w16(&mut r, &mut m, mesh_vt + 8, 4);
            w32(&mut r, &mut m, mesh_vt + 0xc, FAKE_MESH_DEL);
            w32(&mut r, &mut m, node0 + 8, mesh0);
            w32(&mut r, &mut m, node0 + 0x10, 10);
            w32(&mut r, &mut m, node1 + 8, mesh1);
            w32(&mut r, &mut m, node1 + 0x10, 20);
            w32(&mut r, &mut m, this + 0x6c4, 15);

            assert_eq!(call_fn(&mut r, 0x1dee58, &[this, mesh0]), rt::ps2_gfx_release_mesh(&mut m, this, mesh0));
            assert_eq!(call_fn(&mut r, 0x1dee58, &[this, mesh1]), rt::ps2_gfx_release_mesh(&mut m, this, mesh1));
            assert_eq!(mem_slice(&m, this + 0x1870, 0x520), copy_out(&mut r, this + 0x1870, 0x520));

            // Flush non-forced (only node0 expires) then forced (node1 expires)
            for force in [0u32, 1u32] {
                st.borrow_mut().calls.clear();
                call_fn(&mut r, 0x1dec38, &[this, force]);
                let mc = st.borrow().calls.clone();
                let mm = copy_out(&mut r, this + 0x1870, 0x520);
                st.borrow_mut().calls.clear();
                rt::ps2_render_flush_mesh_cache(&mut m, &mut TestEnv(st.clone()), this, force);
                assert_eq!(st.borrow().calls, mc, "flush_mesh_cache force={force}");
                assert_eq!(mem_slice(&m, this + 0x1870, 0x520), mm);
            }

            // vf15_1dd4f0 (end frame)
            w32(&mut r, &mut m, this + 0x680, 0x0100_1234);
            w32(&mut r, &mut m, this + 0x68c, 0x0200_0000);
            w32(&mut r, &mut m, this + 0x690, case % 2);
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1dd4f0, &[this]);
            let mc = st.borrow().calls.clone();
            let mf = copy_out(&mut r, this + 0x680, 0x80);
            st.borrow_mut().calls.clear();
            rt::ps2_gfx_end_frame(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "vf15_1dd4f0 {case}");
            assert_eq!(mem_slice(&m, this + 0x680, 0x80), mf);
        }

        // 5. Render_CalcTexVramSize 0x1e05d0 & Gfx_DispatchDisplayMode 0x1e0680
        {
            let psms = [0u32, 1, 2, 10, 48, 49, 50, 58, 5, 60];
            let zpsms = [0x2f, 0x30, 0x31, 0x32, 0x3a, 0x3b];
            let psm = psms[(case as usize) % psms.len()];
            let zpsm = zpsms[(case as usize / 2) % zpsms.len()];
            let w = 64 * ((case % 10) as i32) + 17;
            let h = 32 * ((case % 8) as i32) + 9;
            assert_eq!(
                call_fn(&mut r, 0x1e05d0, &[this, w as u32, h as u32, psm, zpsm as u32]),
                rt::render_calc_tex_vram_size(w, h, psm, zpsm),
                "calc_tex_vram {case} psm={psm} zpsm={zpsm}"
            );
            assert_eq!(
                call_fn(&mut r, 0x1e0680, &[this, psm, zpsm as u32]),
                rt::gfx_dispatch_display_mode(psm, zpsm as u32),
                "dispatch_display_mode {case} psm={psm} zpsm={zpsm}"
            );
        }

        // 6. vf38..vf42, vf49, vf50
        {
            for k in 0..4u32 {
                w32(&mut r, &mut m, this + 0x1714 + k * 4, 0x700 + k + case);
            }
            st.borrow_mut().calls.clear();
            assert_eq!(call_fn(&mut r, 0x1e1088, &[this]), rt::ps2_gfx_vf38(&mut m, &mut TestEnv(st.clone()), this));
            let mc = st.borrow().calls.clone();
            assert_eq!(mc[0], mc[1]);

            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e1090, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_vf39(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e10b0, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_vf40(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            let shp = aux + 0x400;
            let ty = [2u8, 3, 5, 1][(case as usize) % 4];
            let clut_off: u32 = if case % 2 == 0 { 0x120 << 8 } else { 0 };
            w32(&mut r, &mut m, shp, (ty as u32) | clut_off);
            w16(&mut r, &mut m, shp + 4, 128);
            w16(&mut r, &mut m, shp + 6, 64);
            w32(&mut r, &mut m, shp + 0xc, 3 << 28);
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e10b8, &[this, shp, 0xaa, 0xbb]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_register_shape(&mut m, &mut TestEnv(st.clone()), this, shp, 0xaa, 0xbb);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc, "vf41_1e10b8 {case}");

            // vf42_1e1188 (10 args: 8 regs + 2 stack at sp+0, sp+8 before prologue)
            call_fn(&mut r, 0x1e7ab0, &[this, 1, 2]); // ensures sp initialized
            let sp = r.cpu.gpr(29) as u32;
            r.mem.write_u32(sp, 0x9001);
            r.mem.write_u32(sp + 8, 0x9002);
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e1188, &[this, 11, 22, 33, 44, 55, 66, 77]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_register_texture(&mut m, &mut TestEnv(st.clone()), this, 11, 22, 33, 44, 55, 66, 77, 0x9001, 0x9002);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc, "vf42_1e1188 {case}");

            let tidx = case % 5;
            w32(&mut r, &mut m, tex_entries + tidx * 0x5c + 0x10, 0xabc0 + case);
            w32(&mut r, &mut m, tex_entries + tidx * 0x5c + 0x18, 0xdef0 + case);
            assert_eq!(call_fn(&mut r, 0x1e16f0, &[this, tidx]), rt::ps2_gfx_get_tex_x10(&m, this, tidx));
            assert_eq!(call_fn(&mut r, 0x1e1710, &[this, tidx]), rt::ps2_gfx_get_tex_x18(&m, this, tidx));
        }

        // 7. ProjectToScreen 0x1e1ec8 & ClipTransform 0x1e1f28
        {
            for k in 0..16u32 {
                w32(&mut r, &mut m, this + 0x5b0 + k * 4, rng.f(-3.0, 3.0).to_bits());
            }
            for k in 0..4u32 {
                w32(&mut r, &mut m, this + 0x5f0 + k * 4, rng.f(10.0, 300.0).to_bits());
                w32(&mut r, &mut m, this + 0x600 + k * 4, rng.f(-500.0, 500.0).to_bits());
                w32(&mut r, &mut m, aux + k * 4, rng.f(0.5, 5.0).to_bits());
            }
            let out = aux + 0x500;
            let mr = call_fn(&mut r, 0x1e1ec8, &[out, this, aux]);
            let mo = copy_out(&mut r, out, 16);
            let rr = rt::ps2_render_project_to_screen(&mut m, out, this, aux);
            assert_eq!(rr, mr);
            assert_eq!(mem_slice(&m, out, 16), mo, "project_to_screen {case}");

            let mr = call_fn(&mut r, 0x1e1f28, &[out, this, aux]);
            let mo = copy_out(&mut r, out, 16);
            let rr = rt::ps2_render_clip_transform(&mut m, out, this, aux);
            assert_eq!(rr, mr);
            assert_eq!(mem_slice(&m, out, 16), mo, "clip_transform {case}");
        }

        // 8. cGridMesh dtor, cPS2GridMesh dtor/vf6, vf98, GridMesh_PackUV/PackNormal, StripPacket_Init
        {
            let gm = aux + 0x600;
            let gm_vt = aux + 0x700;
            let arg2 = aux + 0x800;
            let val_ptr = aux + 0x820;
            w32(&mut r, &mut m, gm, gm_vt);
            w16(&mut r, &mut m, gm_vt + 0x40, 4);
            w32(&mut r, &mut m, gm_vt + 0x44, FAKE_GM_VF44);
            w32(&mut r, &mut m, arg2 + 4, val_ptr);
            w16(&mut r, &mut m, val_ptr, (-50i16 + case as i16) as u16);

            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e6df8, &[gm, 0x42, arg2]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_grid_mesh_vf6(&mut m, &mut TestEnv(st.clone()), gm, 0x42, arg2);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            w32(&mut r, &mut m, gm + 0x24, if case % 2 == 0 { BASE + 0x35000 } else { 0 });
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e6698, &[gm, case % 4]);
            let mc = st.borrow().calls.clone();
            let mv = r.mem.read_u32(gm);
            st.borrow_mut().calls.clear();
            rt::ps2_grid_mesh_dtor(&mut m, &mut TestEnv(st.clone()), gm, case % 4);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(m.r32(gm), mv);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e6670, &[gm, case % 4]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::grid_mesh_dtor(&mut m, &mut TestEnv(st.clone()), gm, case % 4);
            assert_eq!(st.borrow().calls, mc);

            call_fn(&mut r, 0x1e7ab0, &[this, 0x1111 + case, 0x2222 + case]);
            let m98 = copy_out(&mut r, this + 0x1d88, 8);
            rt::ps2_gfx_vf98(&mut m, this, 0x1111 + case, 0x2222 + case);
            assert_eq!(mem_slice(&m, this + 0x1d88, 8), m98);

            let dst = aux + 0x900;
            let src = aux + 0xa00;
            for k in 0..3u32 {
                w32(&mut r, &mut m, src + k * 4, rng.f(-1.0, 1.0).to_bits());
            }
            call_fn(&mut r, 0x1e7cf8, &[this, dst, case % 5, src]);
            let muv = copy_out(&mut r, dst, 32);
            rt::grid_mesh_pack_uv(&mut m, dst, case % 5, src);
            assert_eq!(mem_slice(&m, dst, 32), muv);

            call_fn(&mut r, 0x1e7d38, &[this, dst, case % 5, src]);
            let mn = copy_out(&mut r, dst, 48);
            rt::grid_mesh_pack_normal(&mut m, dst, case % 5, src);
            assert_eq!(mem_slice(&m, dst, 48), mn);

            let pkt = aux + 0xb00;
            let mr = call_fn(&mut r, 0x1e81f8, &[pkt, 0x5555 + case]);
            let mp = copy_out(&mut r, pkt, 12);
            let rr = rt::strip_packet_init(&mut m, pkt, 0x5555 + case);
            assert_eq!(rr, mr);
            assert_eq!(mem_slice(&m, pkt, 12), mp);
        }
    }
}
