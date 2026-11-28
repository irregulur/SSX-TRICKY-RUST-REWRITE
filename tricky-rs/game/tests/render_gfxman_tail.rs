//! Function-runner check for row G7t (52 `render` `cPS2GraphicsMan`, `cGraphicsMan`, `cPS2LightMan`, and
//! `cPS2GridMesh` virtuals, matrix stack, fog, and ctors/dtors).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::rendertail::{self as rt, RenderEnv};

const BASE: u32 = 0x0140_0000;
const FAKE_VF84: u32 = 0x002f_f700;
const FAKE_ST_0C: u32 = 0x002f_f710;
const FAKE_GFX_9C: u32 = 0x002f_f720;

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
        0x1dbab0, // cGraphicsMan::~cGraphicsMan (tail-called by cPS2GraphicsMan::~cPS2GraphicsMan)
        0x2f7018, // __unpack_f (called by __fixunssfsi)
        0x2f79a8, // __fixunssfsi
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
    r.mem.write(0x0034_6000, &zg);
    Mem {
        regions: vec![
            (0x0034_6000, vec![0u8; 0x1000]),
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

#[derive(Default)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
}

struct TestEnv(Rc<RefCell<EnvState>>);

impl RenderEnv for TestEnv {
    fn call(&mut self, _m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        self.0.borrow_mut().calls.push((target, args.to_vec()));
        match target {
            rt::GFX_PROGRAM_DISPLAY | rt::GFX_FLUSH_IF_IDLE | rt::GFX_BEGIN_BUFFER | FAKE_VF84 | rt::PS2_RENDER_FLUSH_STATE | FAKE_GFX_9C => {
                0x1357_0000 | (target & 0xffff)
            }
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: &Rc<RefCell<EnvState>>) {
    let stub = |r: &mut Runner, addr: u32, n: usize, n_fpr: usize, ret_val: u32| {
        let st = st.clone();
        r.stub(addr, move |cpu, _mem| {
            let mut args = Vec::with_capacity(n + n_fpr);
            for k in 0..n {
                args.push(cpu.gpr(4 + k) as u32);
            }
            for k in 0..n_fpr {
                args.push(cpu.fpr[12 + k]);
            }
            st.borrow_mut().calls.push((addr, args));
            cpu.set_gpr(2, ret_val as i32 as i64 as u64);
        });
    };

    stub(r, rt::MEM_DELETE, 1, 0, 0);
    stub(r, rt::LIGHT_MAN_DTOR, 2, 0, 0);
    stub(r, rt::GFX_BEGIN_BUFFER, 1, 0, 0x1357_40a0);
    stub(r, rt::GFX_FLUSH_IF_IDLE, 1, 0, 0x1357_40f0);
    stub(r, rt::GFX_PROGRAM_DISPLAY, 3, 0, 0x1357_4288);
    stub(r, rt::PS2_RENDER_UPDATE_MODEL_MATRICES, 1, 0, 0);
    stub(r, rt::PS2_RENDER_FLUSH_STATE, 1, 0, 0x1357_88c8);
    stub(r, FAKE_VF84, 2, 0, 0x1357_f700);
    stub(r, FAKE_ST_0C, 1, 0, 0);
    stub(r, FAKE_GFX_9C, 1, 4, 0x1357_f720);
}

#[test]
fn ps2_gfxman_matrix_fog_and_virtuals_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0x6770_0001);

    for case in 0..60u32 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x1000;
        let vt = BASE + 0x4000;
        let mode_tbl = BASE + 0x5000;
        let mat_stack = BASE + 0x6000;
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

        // 1. cPS2GraphicsMan::cPS2GraphicsMan 0x1ea0c0 & dtors 0x1dbab0, 0x1ea130
        {
            let mr = call_fn(&mut r, 0x1ea0c0, &[this]);
            let mn = copy_out(&mut r, this, 0x500);
            let rr = rt::ps2_graphics_man_ctor(&mut m, this);
            assert_eq!(rr, mr);
            assert_eq!(mem_slice(&m, this, 0x500), mn);

            for &addr in &[0x1dbab0u32, 0x1ea130] {
                st.borrow_mut().calls.clear();
                call_fn(&mut r, addr, &[this, case % 4]);
                let mc = st.borrow().calls.clone();
                let mv = r.mem.read_u32(this + 0x10);
                st.borrow_mut().calls.clear();
                rt::graphics_man_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
                assert_eq!(st.borrow().calls, mc);
                assert_eq!(m.r32(this + 0x10), mv);
            }
        }

        // 2. vf36_1e8928, vf37_1ea018, vf23_1e8970, vf16_1e8cd8
        {
            for k in 0..5u32 {
                w32(&mut r, &mut m, aux + k * 4, rng.next());
            }
            call_fn(&mut r, 0x1e8928, &[this, aux]);
            let mn = copy_out(&mut r, this + 0x17d4, 20);
            rt::ps2_gfx_set_x17d4(&mut m, this, aux);
            assert_eq!(mem_slice(&m, this + 0x17d4, 20), mn);

            assert_eq!(call_fn(&mut r, 0x1ea018, &[this]), rt::ps2_gfx_vf37(this));

            let idx = case % 3;
            w32(&mut r, &mut m, this + 0xe4, idx);
            let entry = this + 0x14 + idx * 0x34;
            for k in 0..13u32 {
                w32(&mut r, &mut m, entry + k * 4, rng.f(-500.0, 500.0).to_bits());
            }
            let out = aux + 0x100;
            let mr = call_fn(&mut r, 0x1e8970, &[out, this]);
            let mo = copy_out(&mut r, out, 16);
            let rr = rt::ps2_gfx_get_viewport_i32(&mut m, out, this);
            assert_eq!(rr, mr);
            assert_eq!(mem_slice(&m, out, 16), mo);

            call_fn(&mut r, 0x1e8cd8, &[this]);
            let mn = copy_out(&mut r, this, 0x100);
            rt::ps2_gfx_push_viewport(&mut m, this);
            assert_eq!(mem_slice(&m, this, 0x100), mn);
        }

        // 3. Mode table getters (vf7..vf11), GridMesh::vf1, vf12 clear color
        {
            w32(&mut r, &mut m, this + 0x64c, 4);
            w32(&mut r, &mut m, this + 0x650, case % 4);
            w32(&mut r, &mut m, this + 0x654, mode_tbl);
            for i in 0..4u32 {
                w32(&mut r, &mut m, mode_tbl + i * 0x94 + 0x80, 0x1000 + i * 17);
                w32(&mut r, &mut m, mode_tbl + i * 0x94 + 0x84, 0x2000 + i * 23);
            }
            assert_eq!(call_fn(&mut r, 0x1e8d60, &[this]), rt::ps2_gfx_get_mode_x84(&m, this));
            assert_eq!(call_fn(&mut r, 0x1e8d80, &[this]), rt::ps2_gfx_get_mode_x80(&m, this));
            assert_eq!(call_fn(&mut r, 0x1e8da0, &[this]), rt::ps2_gfx_get_mode_index(&m, this));
            assert_eq!(call_fn(&mut r, 0x1e8da8, &[this, case % 4]), rt::ps2_gfx_get_mode_entry(&m, this, case % 4));
            assert_eq!(call_fn(&mut r, 0x1e8dc0, &[this]), rt::ps2_gfx_get_mode_count(&m, this));

            w32(&mut r, &mut m, this + 0x50, rng.next());
            assert_eq!(call_fn(&mut r, 0x1e8dc8, &[this]), rt::ps2_grid_mesh_vf1(&m, this));

            for k in 0..3u32 {
                w32(&mut r, &mut m, aux + k * 4, rng.f(0.0, 1.0).to_bits());
            }
            call_fn(&mut r, 0x1e8e20, &[this, aux]);
            let mv = r.mem.read_u32(this + 0x1724);
            rt::ps2_gfx_set_clear_color(&mut m, this, aux);
            assert_eq!(m.r32(this + 0x1724), mv);
        }

        // 4. FlipDisplay, vf88, vf14, vf13, vf53, vf19, vf96, vf65, vf64
        {
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e8ea0, &[this, case]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::gfx_flip_display(&mut m, &mut TestEnv(st.clone()), this, case);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            let obj = aux + 0x200;
            let obj_vt = aux + 0x300;
            w32(&mut r, &mut m, this + 0x484, obj);
            w32(&mut r, &mut m, obj, obj_vt);
            w16(&mut r, &mut m, obj_vt + 0x80, 4);
            w32(&mut r, &mut m, obj_vt + 0x84, FAKE_VF84);
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e9038, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_vf88(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e90e8, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_vf14(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e9108, &[this]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_vf13(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);

            let state_obj = aux + 0x400;
            let state_vt = aux + 0x500;
            w32(&mut r, &mut m, this + 0x470, state_obj);
            w32(&mut r, &mut m, state_obj + 0x18, state_vt);
            w16(&mut r, &mut m, state_vt + 8, 8);
            w32(&mut r, &mut m, state_vt + 0xc, FAKE_ST_0C);
            w8(&mut r, &mut m, state_obj + 0xe, (case & 0xff) as u8);
            w8(&mut r, &mut m, state_obj + 0x15, ((case * 7) & 0xff) as u8);
            w32(&mut r, &mut m, this + 0x6c4, rng.next());

            assert_eq!(call_fn(&mut r, 0x1e9ff0, &[this]), rt::ps2_gfx_vf96(&m, this));
            assert_eq!(call_fn(&mut r, 0x1e9ff8, &[this]), rt::ps2_gfx_vf65(&m, this));
            assert_eq!(call_fn(&mut r, 0x1ea008, &[this]), rt::ps2_gfx_vf64(&m, this));

            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e9628, &[this]);
            let mc = st.borrow().calls.clone();
            let mf = copy_out(&mut r, this + 0x474, 8);
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_vf53(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(mem_slice(&m, this + 0x474, 8), mf);

            w32(&mut r, &mut m, this + 0x10, vt);
            w16(&mut r, &mut m, vt + 0x98, 4);
            w32(&mut r, &mut m, vt + 0x9c, FAKE_GFX_9C);
            let rect = aux + 0x600;
            for k in 0..4u32 {
                w32(&mut r, &mut m, rect + k * 4, ((rng.next() as i32) % 2000) as u32);
            }
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1ea020, &[this, rect]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_gfx_vf19(&mut m, &mut TestEnv(st.clone()), this, rect);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc);
        }

        // 5. PS2Render_CalcFog 0x1e9128
        {
            for k in 0..16u32 {
                w32(&mut r, &mut m, this + 0x5b0 + k * 4, rng.f(-5.0, 5.0).to_bits());
            }
            w32(&mut r, &mut m, this + 0x630, rng.f(10.0, 255.0).to_bits());
            w32(&mut r, &mut m, this + 0x63c, rng.f(-20.0, 20.0).to_bits());
            for k in 0..4u32 {
                w32(&mut r, &mut m, aux + k * 4, rng.f(-10.0, 10.0).to_bits());
            }
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x1e9128, &[this, aux]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_render_calc_fog(&mut m, &mut TestEnv(st.clone()), this, aux);
            assert_eq!(rr, mr, "calc_fog {case}");
            assert_eq!(st.borrow().calls, mc);
        }

        // 6. Matrix stack: vf33, vf32, vf31, vf29, vf27, vf26
        {
            for k in 0..16u32 {
                w32(&mut r, &mut m, rt::G_IDENTITY_MAT4 + k * 4, if k % 5 == 0 { 1.0f32 } else { 0.0f32 }.to_bits());
                w32(&mut r, &mut m, mat_stack + 0x40 + k * 4, rng.f(-5.0, 5.0).to_bits());
                w32(&mut r, &mut m, aux + k * 4, rng.f(-5.0, 5.0).to_bits());
            }
            w32(&mut r, &mut m, this + 0x3f0, 1);
            w32(&mut r, &mut m, this + 0x3f4, mat_stack + 0x40);
            w32(&mut r, &mut m, this + 0x17d0, 1);

            call_fn(&mut r, 0x1e9880, &[this, aux]);
            let mm = copy_out(&mut r, mat_stack, 0xc0);
            let md = r.mem.read_u32(this + 0x17d0);
            rt::ps2_gfx_mat_translate(&mut m, this, aux);
            assert_eq!(mem_slice(&m, mat_stack, 0xc0), mm);
            assert_eq!(m.r32(this + 0x17d0), md);

            w32(&mut r, &mut m, this + 0x17d0, 1);
            call_fn(&mut r, 0x1e98f0, &[this, aux]);
            let mm = copy_out(&mut r, mat_stack, 0xc0);
            rt::ps2_gfx_mat_mul(&mut m, this, aux);
            assert_eq!(mem_slice(&m, mat_stack, 0xc0), mm);
            assert_eq!(m.r32(this + 0x17d0), 0);

            call_fn(&mut r, 0x1e9ea8, &[this]);
            let mm = copy_out(&mut r, mat_stack, 0xc0);
            let mp = copy_out(&mut r, this + 0x3f0, 8);
            rt::ps2_gfx_mat_push(&mut m, this);
            assert_eq!(mem_slice(&m, mat_stack, 0xc0), mm);
            assert_eq!(mem_slice(&m, this + 0x3f0, 8), mp);

            w32(&mut r, &mut m, this + 0x17d0, 1);
            call_fn(&mut r, 0x1e98b8, &[this]);
            let mm = copy_out(&mut r, mat_stack, 0xc0);
            rt::ps2_gfx_mat_load_identity(&mut m, this);
            assert_eq!(mem_slice(&m, mat_stack, 0xc0), mm);
            assert_eq!(m.r32(this + 0x17d0), 0);

            w32(&mut r, &mut m, this + 0x17d0, 1);
            call_fn(&mut r, 0x1e9978, &[this, aux]);
            let mm = copy_out(&mut r, mat_stack, 0xc0);
            rt::ps2_gfx_mat_load(&mut m, this, aux);
            assert_eq!(mem_slice(&m, mat_stack, 0xc0), mm);
            assert_eq!(m.r32(this + 0x17d0), 0);

            w32(&mut r, &mut m, this + 0x17d0, 1);
            call_fn(&mut r, 0x1e9e88, &[this]);
            let mp = copy_out(&mut r, this + 0x3f0, 8);
            rt::ps2_gfx_mat_pop(&mut m, this);
            assert_eq!(mem_slice(&m, this + 0x3f0, 8), mp);
            assert_eq!(m.r32(this + 0x17d0), 0);
        }

        // 7. cPS2LightMan::~cPS2LightMan 0x1ea290 + all 21 trivial virtual stubs
        {
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1ea290, &[this, case % 4]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::ps2_light_man_dtor(&mut m, &mut TestEnv(st.clone()), this, case % 4);
            assert_eq!(st.borrow().calls, mc);

            for &addr in &[
                0x1ea1d8u32, 0x1ea1e0, 0x1ea1e8, 0x1ea1f0, 0x1ea1f8, 0x1ea200, 0x1ea208,
                0x1ea218, 0x1ea220, 0x1ea228, 0x1ea230, 0x1ea2e0, 0x1ea2e8, 0x1ea2f0,
            ] {
                call_fn(&mut r, addr, &[this]);
                rt::gfx_noop();
            }
            for &addr in &[0x1ea210u32, 0x1ea238, 0x1ea2d8, 0x1ea2f8, 0x1ea300] {
                assert_eq!(call_fn(&mut r, addr, &[this]), rt::gfx_ret0(), "ret0 0x{addr:x}");
            }
            for &addr in &[0x1ea308u32, 0x1ea310] {
                assert_eq!(call_fn(&mut r, addr, &[this]), rt::gfx_ret1(), "ret1 0x{addr:x}");
            }
        }
    }
}
