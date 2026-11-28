//! Function-runner check for row G7w (12 `render` `cPS2GraphicsMan` mesh/texture/state/matrix/viewport,
//! `Render_FormatVramStats`, `Gfx_LoadImageToFrame`, and `StripPacket` functions).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::rendertail::{self as rt, RenderEnv};

const BASE: u32 = 0x0140_0000;
const FAKE_MESH_0C: u32 = 0x002f_fa00;
const FAKE_MESH_14: u32 = 0x002f_fa10;
const FAKE_GFX_9C: u32 = 0x002f_fa20;
const FAKE_GFX_AC: u32 = 0x002f_fa30;
const FAKE_GFX_B4: u32 = 0x002f_fa40;
const FAKE_GFX_1C4: u32 = 0x002f_fa50;
const FAKE_GFX_1CC: u32 = 0x002f_fa60;
const FAKE_GFX_1D4: u32 = 0x002f_fa70;
const FAKE_GFX_1E4: u32 = 0x002f_fa80;
const FAKE_GFX_1EC: u32 = 0x002f_fa90;
const FAKE_GFX_1F4: u32 = 0x002f_faa0;
const FAKE_GFX_1FC: u32 = 0x002f_fab0;

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
        0x1deb40, // PS2Render_LinkMesh
        0x1debd0, // PS2Render_UnlinkMesh
        0x1e05d0, // Render_CalcTexVramSize
        0x2cc808, // EAMem_Set
        0x2f5ab8, // __pack_d
        0x2f6728, // __make_dp
        0x2f7018, // __unpack_f
        0x2f7aa8, // __extendsfdf2
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
    let zg = vec![0u8; 0x2000];
    r.mem.write(0x0033_7000, &zg);
    r.mem.write(0x0033_f000, &zg);
    Mem {
        regions: vec![
            (0x0033_7000, vec![0u8; 0x2000]),
            (0x0033_f000, vec![0u8; 0x2000]),
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

#[derive(Default)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
    alloc_ret: u32,
    init_mesh_ret: u32,
    mesh_vt: u32,
    mesh_ok_ret: u32,
    sin_val: f32,
    cos_val: f32,
}

struct TestEnv(Rc<RefCell<EnvState>>);

impl RenderEnv for TestEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        let mut s = self.0.borrow_mut();
        s.calls.push((target, args.to_vec()));
        match target {
            rt::MEMPOOL_ALLOC | rt::EAMEM_ALLOC => s.alloc_ret,
            rt::PS2_GRID_MESH_INIT => {
                if s.init_mesh_ret != 0 {
                    m.w32(s.init_mesh_ret, s.mesh_vt);
                }
                s.init_mesh_ret
            }
            FAKE_MESH_14 => s.mesh_ok_ret,
            rt::MEM_MOVE => {
                let dst = args[0];
                let src = args[1];
                let len = args[2];
                let tmp: Vec<u8> = (0..len).map(|i| m.r8(src + i)).collect();
                for (i, b) in tmp.into_iter().enumerate() {
                    m.w8(dst + i as u32, b);
                }
                dst
            }
            rt::MATH_SIN_COS => {
                m.w32(args[0], s.sin_val.to_bits());
                m.w32(args[1], s.cos_val.to_bits());
                0
            }
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: &Rc<RefCell<EnvState>>) {
    let stub = |r: &mut Runner, addr: u32, n_gpr: usize, n_stk: usize, n_fpr: usize, is_sprintf: bool| {
        let st = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut args = Vec::new();
            for k in 0..n_gpr {
                args.push(cpu.gpr(4 + k) as u32);
            }
            if is_sprintf {
                let d64 = cpu.gpr(9);
                args.push(d64 as u32);
                args.push((d64 >> 32) as u32);
            }
            let sp = cpu.gpr(29) as u32;
            for k in 0..n_stk {
                args.push(mem.read_u32(sp + (k as u32) * 8));
            }
            for k in 0..n_fpr {
                args.push(cpu.fpr[12 + k]);
            }
            let mut s = st.borrow_mut();
            s.calls.push((addr, args.clone()));
            let ret = match addr {
                rt::MEMPOOL_ALLOC | rt::EAMEM_ALLOC => s.alloc_ret,
                rt::PS2_GRID_MESH_INIT => {
                    if s.init_mesh_ret != 0 {
                        mem.write_u32(s.init_mesh_ret, s.mesh_vt);
                    }
                    s.init_mesh_ret
                }
                FAKE_MESH_14 => s.mesh_ok_ret,
                rt::MEM_MOVE => {
                    let dst = args[0];
                    let src = args[1];
                    let len = args[2] as usize;
                    let mut tmp = vec![0u8; len];
                    mem.read(src, &mut tmp).unwrap();
                    mem.write(dst, &tmp);
                    dst
                }
                rt::MATH_SIN_COS => {
                    mem.write_u32(args[0], s.sin_val.to_bits());
                    mem.write_u32(args[1], s.cos_val.to_bits());
                    0
                }
                _ => 0,
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    };

    stub(r, rt::MEM_DELETE, 1, 0, 0, false);
    stub(r, rt::MEMPOOL_ALLOC, 6, 0, 0, false);
    stub(r, rt::EAMEM_ALLOC, 3, 0, 0, false);
    stub(r, rt::EAMEM_FREE, 1, 0, 0, false);
    stub(r, rt::PS2_GRID_MESH_INIT, 5, 0, 0, false);
    stub(r, rt::SPRINTF, 5, 0, 0, true);
    stub(r, rt::TEXMAN_UPLOAD_PIXELS, 4, 0, 0, false);
    stub(r, rt::REFPACK_DECODE, 3, 0, 0, false);
    stub(r, rt::MEM_MOVE, 3, 0, 0, false);
    stub(r, rt::FLUSH_CACHE, 1, 0, 0, false);
    stub(r, rt::GFX_LOAD_IMAGE, 8, 1, 0, false);
    stub(r, rt::PS2_RENDER_SUBMIT, 4, 0, 0, false);
    stub(r, rt::MATH_SIN_COS, 2, 0, 1, false);

    stub(r, FAKE_MESH_0C, 2, 0, 0, false);
    stub(r, FAKE_MESH_14, 1, 0, 0, false);
    stub(r, FAKE_GFX_9C, 1, 0, 4, false);
    stub(r, FAKE_GFX_AC, 1, 0, 3, false);
    stub(r, FAKE_GFX_B4, 1, 0, 6, false);
    stub(r, FAKE_GFX_1C4, 2, 0, 0, false);
    stub(r, FAKE_GFX_1CC, 3, 0, 1, false);
    stub(r, FAKE_GFX_1D4, 2, 0, 0, false);
    stub(r, FAKE_GFX_1E4, 2, 0, 0, false);
    stub(r, FAKE_GFX_1EC, 3, 0, 0, false);
    stub(r, FAKE_GFX_1F4, 3, 0, 0, false);
    stub(r, FAKE_GFX_1FC, 3, 0, 0, false);
}

#[test]
fn gfxman_strippacket_refpack_and_rotate_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0x6700_7777);

    call_fn(&mut r, 0x1c09c8, &[], None);
    let sp_init = r.cpu.gpr(29) as u32;
    let stack_base = (sp_init - 0x200) & !0xf;

    for case in 0..32u32 {
        let mut m = fresh_mem(&mut r);
        let zstack = vec![0u8; 0x200];
        r.mem.write(stack_base, &zstack);
        m.regions.push((stack_base, zstack));

        let this = BASE + 0x1000;
        let vt = BASE + 0x4000;
        let app = BASE + 0x5000;
        let aux = BASE + 0x8000;

        let w32 = |r: &mut Runner, m: &mut Mem, a: u32, v: u32| {
            r.mem.write_u32(a, v);
            m.w32(a, v);
        };
        let w16 = |r: &mut Runner, m: &mut Mem, a: u32, v: u16| {
            r.mem.write(a, &v.to_le_bytes());
            m.w16(a, v);
        };
        let w8 = |r: &mut Runner, m: &mut Mem, a: u32, v: u8| {
            r.mem.write(a, &[v]);
            m.w8(a, v);
        };

        w32(&mut r, &mut m, rt::G_APP, app);
        w32(&mut r, &mut m, app + 0x724, this);
        w32(&mut r, &mut m, this + 0x10, vt);
        w32(&mut r, &mut m, this + 0x47c, BASE + 0x5800);
        for &(off, adj, func) in &[
            (0x98u32, 4u16, FAKE_GFX_9C),
            (0xa8, 8, FAKE_GFX_AC),
            (0xb0, 12, FAKE_GFX_B4),
            (0x1c0, 4, FAKE_GFX_1C4),
            (0x1c8, 8, FAKE_GFX_1CC),
            (0x1d0, 12, FAKE_GFX_1D4),
            (0x1e0, 16, FAKE_GFX_1E4),
            (0x1e8, 20, FAKE_GFX_1EC),
            (0x1f0, 24, FAKE_GFX_1F4),
            (0x1f8, 28, FAKE_GFX_1FC),
        ] {
            w16(&mut r, &mut m, vt + off, adj);
            w32(&mut r, &mut m, vt + off + 4, func);
        }

        // 1. vf68_1decf0 (create grid mesh)
        {
            rt::ps2_render_reset_mesh_cache(&mut m, this);
            let mc_init = mem_slice(&m, this + 0x1870, 0x520);
            r.mem.write(this + 0x1870, &mc_init);
            if case % 5 == 0 {
                w32(&mut r, &mut m, this + 0x1d7c, 0);
            }
            let mesh_buf = aux + 0x100;
            let mesh_vt = aux + 0x200;
            w32(&mut r, &mut m, mesh_buf, mesh_vt);
            w16(&mut r, &mut m, mesh_vt + 8, 4);
            w32(&mut r, &mut m, mesh_vt + 0xc, FAKE_MESH_0C);
            w16(&mut r, &mut m, mesh_vt + 0x10, 8);
            w32(&mut r, &mut m, mesh_vt + 0x14, FAKE_MESH_14);
            let flags = match case % 4 {
                0 => 0x4000_0001,
                1 => 0x8000_0002,
                2 => 0x2000_0003,
                _ => 0x6000_0004,
            };
            {
                let mut s = st.borrow_mut();
                s.calls.clear();
                s.alloc_ret = mesh_buf;
                s.init_mesh_ret = if (flags & 0x2000_0000) != 0 && case % 8 == 2 { 0 } else { mesh_buf };
                s.mesh_vt = mesh_vt;
                s.mesh_ok_ret = (case / 2) % 2;
            }
            let mr = call_fn(&mut r, 0x1decf0, &[this, 0x11, 0x22, flags], None);
            let mc = st.borrow().calls.clone();
            let mb = copy_out(&mut r, mesh_buf, 0x58);
            let mmc = copy_out(&mut r, this + 0x1870, 0x520);

            // Re-init mesh_buf vtable since EAMem_Set overwrote it in MIPS before stubbed Init
            w32(&mut r, &mut m, mesh_buf, mesh_vt);
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_create_grid_mesh(&mut m, &mut TestEnv(st.clone()), this, 0x11, 0x22, flags);
            assert_eq!(rr, mr, "vf68_1decf0 ret {case}");
            assert_eq!(st.borrow().calls, mc, "vf68_1decf0 calls {case}");
            assert_eq!(mem_slice(&m, mesh_buf, 0x58), mb);
            assert_eq!(mem_slice(&m, this + 0x1870, 0x520), mmc);
        }

        // 2. Render_FormatVramStats 0x1e04b8
        {
            let e = rt::G_DISPLAY_MODES;
            w32(&mut r, &mut m, e + 0x80, 640);
            w32(&mut r, &mut m, e + 0x84, 448);
            w32(&mut r, &mut m, e + 0x88, case % 3);
            w32(&mut r, &mut m, e + 0x8c, [0x30, 0x31, 0x32][(case as usize) % 3]);
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e04b8, &[this], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::render_format_vram_stats(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "format_vram_stats {case}");
        }

        // 3 & 4. vf43_1e11d0 & vf44_1e1358
        {
            let shp = aux + 0x400;
            let tmp_buf = aux + 0x1000;
            let clut_off: u32 = if case % 2 == 0 { 0x80 << 8 } else { 0 };
            w32(&mut r, &mut m, shp, 0x82 | clut_off);
            w32(&mut r, &mut m, shp + 4, 0x1122_3344);
            w32(&mut r, &mut m, shp + 8, 0x5566_7788);
            w32(&mut r, &mut m, shp + 0xc, 0x99aa_bbcc);
            let compressed = (case / 2) % 2 == 0;
            w8(&mut r, &mut m, shp + 0x10, if compressed { 0x10 } else { 0x00 });
            w8(&mut r, &mut m, shp + 0x11, 0xfb);
            w8(&mut r, &mut m, shp + 0x12, 0x00);
            w8(&mut r, &mut m, shp + 0x13, 0x01);
            w8(&mut r, &mut m, shp + 0x14, 0x20);
            st.borrow_mut().alloc_ret = tmp_buf;

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e11d0, &[this, case, shp], None);
            let mc = st.borrow().calls.clone();
            let mt = copy_out(&mut r, tmp_buf, 0x20);
            st.borrow_mut().calls.clear();
            rt::ps2_gfx_upload_shape(&mut m, &mut TestEnv(st.clone()), this, case, shp);
            assert_eq!(st.borrow().calls, mc, "vf43_1e11d0 {case}");
            assert_eq!(mem_slice(&m, tmp_buf, 0x20), mt);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e1358, &[this, case, shp, 0xdead_0000 + case], None);
            let mc = st.borrow().calls.clone();
            let mt = copy_out(&mut r, tmp_buf, 0x20);
            st.borrow_mut().calls.clear();
            rt::ps2_gfx_upload_shape_with_clut(&mut m, &mut TestEnv(st.clone()), this, case, shp, 0xdead_0000 + case);
            assert_eq!(st.borrow().calls, mc, "vf44_1e1358 {case}");
            assert_eq!(mem_slice(&m, tmp_buf, 0x20), mt);
        }

        // 5. vf85_1e2948 (unpack 4 quad verts)
        {
            let obj = aux + 0x2000;
            let p1 = aux + 0x2100;
            let p2 = aux + 0x2200;
            let data = aux + 0x2400;
            let out_n = aux + 0x3000;
            let out_p = aux + 0x3100;
            let out_uv = aux + 0x3200;
            w32(&mut r, &mut m, obj + 0x28, p1);
            w32(&mut r, &mut m, p1, p2);
            w32(&mut r, &mut m, p2 + 0xc, data);
            for off in (0x90..0x120u32).step_by(2) {
                w16(&mut r, &mut m, data + off, (rng.next() & 0xffff) as u16);
            }
            call_fn(&mut r, 0x1e2948, &[this, obj, out_n, out_p, out_uv], None);
            let mn = copy_out(&mut r, out_n, 48);
            let mp = copy_out(&mut r, out_p, 48);
            let mu = copy_out(&mut r, out_uv, 32);
            rt::ps2_gfx_unpack_quad_verts(&mut m, obj, out_n, out_p, out_uv);
            assert_eq!(mem_slice(&m, out_n, 48), mn);
            assert_eq!(mem_slice(&m, out_p, 48), mp);
            assert_eq!(mem_slice(&m, out_uv, 32), mu);
        }

        // 6. Gfx_LoadImageToFrame 0x1e4e20
        {
            w32(&mut r, &mut m, this + 0x698, 640);
            w32(&mut r, &mut m, this + 0x6a0, 448);
            w32(&mut r, &mut m, this + 0x6d0, 12 + case);
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e4e20, &[this, 1, 2, 3, 4, 5], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::gfx_load_image_to_frame(&mut m, &mut TestEnv(st.clone()), this, 1, 2, 3, 4, 5);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);
        }

        // 7,8,9: StripPacket_Begin / AddVertex / Destroy
        {
            let spkt = aux + 0x4000;
            let cursor_box = aux + 0x4100;
            let dma_buf = aux + 0x5000;
            let vtx = aux + 0x4200;
            let pos = aux + 0x4300;
            w32(&mut r, &mut m, cursor_box, dma_buf);
            w32(&mut r, &mut m, spkt, cursor_box);
            w32(&mut r, &mut m, spkt + 4, 0);
            w32(&mut r, &mut m, spkt + 8, 0);

            for step in 0..3u32 {
                w32(&mut r, &mut m, vtx, rng.f(-1.0, 1.0).to_bits());
                w32(&mut r, &mut m, vtx + 4, rng.f(-1.0, 1.0).to_bits());
                for k in 0..4u32 {
                    w32(&mut r, &mut m, vtx + 0x10 + k * 4, rng.next() & 0xff);
                }
                w32(&mut r, &mut m, pos, rng.f(0.0, 4096.0).to_bits());
                w32(&mut r, &mut m, pos + 4, rng.f(0.0, 4096.0).to_bits());
                w32(&mut r, &mut m, pos + 8, rng.f(0.0, 1000.0).to_bits());
                w32(&mut r, &mut m, pos + 0xc, rng.f(0.01, 2.0).to_bits());

                if step == 0 || step == 2 {
                    call_fn(&mut r, 0x1e7e08, &[spkt, vtx, pos], None);
                    let mb = copy_out(&mut r, dma_buf, 0x200);
                    let ms = copy_out(&mut r, spkt, 0x10);
                    let mc = r.mem.read_u32(cursor_box);
                    rt::strip_packet_begin(&mut m, spkt, vtx, pos);
                    assert_eq!(mem_slice(&m, dma_buf, 0x200), mb, "strip_begin dma {case}/{step}");
                    assert_eq!(mem_slice(&m, spkt, 0x10), ms);
                    assert_eq!(m.r32(cursor_box), mc);
                } else {
                    call_fn(&mut r, 0x1e7fc0, &[spkt, vtx, pos], None);
                    let mb = copy_out(&mut r, dma_buf, 0x200);
                    let ms = copy_out(&mut r, spkt, 0x10);
                    let mc = r.mem.read_u32(cursor_box);
                    rt::strip_packet_add_vertex(&mut m, spkt, vtx, pos);
                    assert_eq!(mem_slice(&m, dma_buf, 0x200), mb, "strip_add dma {case}/{step}");
                    assert_eq!(mem_slice(&m, spkt, 0x10), ms);
                    assert_eq!(m.r32(cursor_box), mc);
                }
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e80a0, &[spkt, case % 4], None);
            let mc = st.borrow().calls.clone();
            let mb = copy_out(&mut r, dma_buf, 0x200);
            st.borrow_mut().calls.clear();
            rt::strip_packet_destroy(&mut m, &mut TestEnv(st.clone()), spkt, case % 4);
            assert_eq!(st.borrow().calls, mc, "strip_destroy calls {case}");
            assert_eq!(mem_slice(&m, dma_buf, 0x200), mb, "strip_destroy dma {case}");
        }

        // 10. vf17_1e8c18 (pop viewport)
        {
            w32(&mut r, &mut m, this + 0xe4, 2);
            let e = this + 0x14 + 0x34;
            w32(&mut r, &mut m, e, case % 2);
            for k in 1..13u32 {
                w32(&mut r, &mut m, e + k * 4, rng.f(-100.0, 100.0).to_bits());
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e8c18, &[this], None);
            let mc = st.borrow().calls.clone();
            let mi = r.mem.read_u32(this + 0xe4);
            st.borrow_mut().calls.clear();
            rt::ps2_gfx_pop_viewport(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc, "vf17_1e8c18 {case}");
            assert_eq!(m.r32(this + 0xe4), mi);
        }

        // 11. vf54_1e9508 (apply state block)
        {
            let sb = aux + 0x6000;
            for k in 0..12u32 {
                w32(&mut r, &mut m, sb + k * 4, rng.next());
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e9508, &[this, sb], None);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::ps2_gfx_apply_state_block(&mut m, &mut TestEnv(st.clone()), this, sb);
            assert_eq!(st.borrow().calls, mc, "vf54_1e9508 {case}");
        }

        // 12. vf34_1e9708 (axis-angle rotate)
        {
            let mat = aux + 0x7000;
            let axis = aux + 0x7100;
            for k in 0..16u32 {
                w32(&mut r, &mut m, mat + k * 4, rng.f(-3.0, 3.0).to_bits());
            }
            for k in 0..3u32 {
                w32(&mut r, &mut m, axis + k * 4, rng.f(-1.0, 1.0).to_bits());
            }
            w32(&mut r, &mut m, this + 0x3f4, mat);
            w32(&mut r, &mut m, this + 0x17d0, 1);
            let ang = rng.f(-3.14, 3.14);
            st.borrow_mut().sin_val = ang.sin();
            st.borrow_mut().cos_val = ang.cos();

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e9708, &[this, axis], Some(ang));
            let mc = st.borrow().calls.clone();
            let mm = copy_out(&mut r, mat, 0x40);
            st.borrow_mut().calls.clear();
            rt::ps2_gfx_mat_rotate_axis(&mut m, &mut TestEnv(st.clone()), this, axis, ang, sp_init - 0x90);
            assert_eq!(st.borrow().calls, mc, "vf34_1e9708 calls {case}");
            assert_eq!(mem_slice(&m, mat, 0x40), mm, "vf34_1e9708 mat {case}");
            assert_eq!(m.r32(this + 0x17d0), 0);
        }
    }
}
