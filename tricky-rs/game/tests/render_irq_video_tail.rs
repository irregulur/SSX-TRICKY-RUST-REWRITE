//! Function-runner check for row G7v (18 `render` VIF1 IRQ, vblank/buffer, texture block swizzlers, and
//! `cPS2VideoPlayer` functions).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::rendertail::{self as rt, RenderEnv};

const BASE: u32 = 0x0140_0000;
const FAKE_GFX_2C: u32 = 0x002f_f900;
const FAKE_GFX_3C: u32 = 0x002f_f910;
const FAKE_GFX_44: u32 = 0x002f_f920;
const FAKE_GFX_6C: u32 = 0x002f_f930;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    for a in [
        0x1c08a8, // Gfx_InstallVif1Handler
        0x1c08f0, // Gfx_RemoveVif1Handler
        0x1c0990, // Gfx_ResetVif1IrqCounts
        0x1deb40, // PS2Render_LinkMesh
        0x1debd0, // PS2Render_UnlinkMesh
        0x1dec38, // PS2Render_FlushMeshCache
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
    r.mem.write(0x0033_7000, &zg);
    // Preserve the real swizzle tables at 0x340b10..0x341110 from SLUS_203.26!
    let mut tbl = vec![0u8; 0x2000];
    r.mem.read(0x0034_0000, &mut tbl).unwrap();
    Mem {
        regions: vec![
            (0x0033_7000, vec![0u8; 0x1000]),
            (0x0034_0000, tbl),
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
    calls: Vec<(u32, Vec<u32>, Vec<u8>)>,
    playback_left: u32,
    dev0: u32,
    dev1: u32,
}

struct TestEnv(Rc<RefCell<EnvState>>);

impl RenderEnv for TestEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        let mut s = self.0.borrow_mut();
        let snap = if target == FAKE_GFX_6C {
            mem_slice(m, args[1], 12)
        } else {
            Vec::new()
        };
        s.calls.push((target, args.to_vec(), snap));
        match target {
            rt::ADD_INTC_HANDLER => 0x55,
            FAKE_GFX_44 => 2,
            FAKE_GFX_3C => 7,
            rt::EAMEM_ALLOC_ALIGNED => BASE + 0x12000,
            rt::INPUT_RECORDER_PLAYBACK => {
                if s.playback_left > 0 {
                    s.playback_left -= 1;
                    1
                } else {
                    0
                }
            }
            rt::INPUT_GET_DEVICE => if args[1] == 0 { s.dev0 } else { s.dev1 },
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: &Rc<RefCell<EnvState>>) {
    let stub = |r: &mut Runner, addr: u32, n: usize, snap_rgb: bool| {
        let st = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut args = Vec::with_capacity(n);
            for k in 0..n {
                args.push(cpu.gpr(4 + k) as u32);
            }
            let snap = if snap_rgb {
                let mut b = vec![0u8; 12];
                mem.read(args[1], &mut b).unwrap();
                b
            } else {
                Vec::new()
            };
            let mut s = st.borrow_mut();
            s.calls.push((addr, args.clone(), snap));
            let ret = match addr {
                rt::ADD_INTC_HANDLER => 0x55,
                FAKE_GFX_44 => 2,
                FAKE_GFX_3C => 7,
                rt::EAMEM_ALLOC_ALIGNED => BASE + 0x12000,
                rt::INPUT_RECORDER_PLAYBACK => {
                    if s.playback_left > 0 {
                        s.playback_left -= 1;
                        1
                    } else {
                        0
                    }
                }
                rt::INPUT_GET_DEVICE => if args[1] == 0 { s.dev0 } else { s.dev1 },
                _ => 0,
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    };

    stub(r, rt::DISABLE_INTC, 1, false);
    stub(r, rt::ENABLE_INTC, 1, false);
    stub(r, rt::ADD_INTC_HANDLER, 3, false);
    stub(r, rt::REMOVE_INTC_HANDLER, 2, false);
    stub(r, rt::I_SIGNAL_SEMA, 1, false);
    stub(r, rt::GFX_RENDER_THREAD, 1, false);
    stub(r, rt::EAMEM_ALLOC_ALIGNED, 5, false);
    stub(r, rt::EAMEM_FREE, 1, false);
    stub(r, rt::VIDEO_PLAYER_DTOR, 2, false);
    stub(r, rt::PAD_LATCH_BUTTONS, 1, false);
    stub(r, rt::INPUT_RECORDER_PLAYBACK, 1, false);
    stub(r, rt::INPUT_GET_DEVICE, 2, false);

    stub(r, FAKE_GFX_2C, 2, false);
    stub(r, FAKE_GFX_3C, 5, false);
    stub(r, FAKE_GFX_44, 1, false);
    stub(r, FAKE_GFX_6C, 2, true);
}

#[test]
fn vif1_vblank_swizzle_and_videoplayer_match() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, &st);
    let mut rng = Rng(0x67F0_0001);

    call_fn(&mut r, 0x1c09c8, &[]);
    let sp_init = r.cpu.gpr(29) as u32;
    let stack_base = (sp_init - 0x200) & !0xf;

    for case in 0..32u32 {
        let mut m = fresh_mem(&mut r);
        let zstack = vec![0u8; 0x200];
        r.mem.write(stack_base, &zstack);
        m.regions.push((stack_base, zstack));

        let this = BASE + 0x1000;
        let app = BASE + 0x4000;
        let gfx = BASE + 0x5000;
        let gfx_vt = BASE + 0x7000;
        let dev0 = BASE + 0x8000;
        let dev1 = BASE + 0x8100;

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
        w32(&mut r, &mut m, app + 0x24, BASE + 0x4800);
        w32(&mut r, &mut m, app + 0x724, gfx);
        w32(&mut r, &mut m, gfx + 0x10, gfx_vt);
        w16(&mut r, &mut m, gfx_vt + 0x28, 4);
        w32(&mut r, &mut m, gfx_vt + 0x2c, FAKE_GFX_2C);
        w16(&mut r, &mut m, gfx_vt + 0x38, 8);
        w32(&mut r, &mut m, gfx_vt + 0x3c, FAKE_GFX_3C);
        w16(&mut r, &mut m, gfx_vt + 0x40, 12);
        w32(&mut r, &mut m, gfx_vt + 0x44, FAKE_GFX_44);
        w16(&mut r, &mut m, gfx_vt + 0x68, 16);
        w32(&mut r, &mut m, gfx_vt + 0x6c, FAKE_GFX_6C);

        // 1..6: VIF1 IRQ helpers
        {
            for k in 0..10u32 {
                w32(&mut r, &mut m, this + k * 4, 0xaaaa_0000 + k);
            }
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1c0920, &[this]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x28);
            st.borrow_mut().calls.clear();
            rt::gfx_start_vif1_irq(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(mem_slice(&m, this, 0x28), mn);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1c0958, &[this]);
            let mc = st.borrow().calls.clone();
            let mn = copy_out(&mut r, this, 0x28);
            st.borrow_mut().calls.clear();
            rt::gfx_stop_vif1_irq(&mut m, &mut TestEnv(st.clone()), this);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(mem_slice(&m, this, 0x28), mn);

            call_fn(&mut r, 0x1c09c8, &[this]);
            rt::gfx_update_vif1_irq();
        }

        // 7..11: VblankStart, Vif1DmaDone, BeginBuffer, FlushIfIdle, RenderThreadEntry
        {
            w32(&mut r, &mut m, gfx + 0x6d8, case % 6);
            w32(&mut r, &mut m, gfx + 0x6f0, 2);
            w32(&mut r, &mut m, gfx + 0x6f4, case % 3);
            w32(&mut r, &mut m, gfx + 0x700, 0x77);
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e3fe0, &[gfx]);
            let mc = st.borrow().calls.clone();
            let mg = copy_out(&mut r, gfx + 0x6d0, 0x30);
            st.borrow_mut().calls.clear();
            rt::gfx_on_vblank_start(&mut m, &mut TestEnv(st.clone()), gfx);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(mem_slice(&m, gfx + 0x6d0, 0x30), mg);

            w32(&mut r, &mut m, gfx + 0x6d8, case % 5);
            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e4060, &[gfx]);
            let mc = st.borrow().calls.clone();
            let mg = r.mem.read_u32(gfx + 0x6d8);
            st.borrow_mut().calls.clear();
            rt::gfx_on_vif1_dma_done(&mut m, &mut TestEnv(st.clone()), gfx);
            assert_eq!(st.borrow().calls, mc);
            assert_eq!(m.r32(gfx + 0x6d8), mg);

            let idx = case % 2;
            w32(&mut r, &mut m, gfx + 0x690, idx);
            w32(&mut r, &mut m, gfx + 0x6dc + idx * 4, case % 4);
            let mr = call_fn(&mut r, 0x1e40a0, &[gfx]);
            let ms = r.mem.read_u32(gfx + 0x6dc + idx * 4);
            let rr = rt::gfx_begin_buffer(&mut m, gfx);
            assert_eq!(rr, mr);
            assert_eq!(m.r32(gfx + 0x6dc + idx * 4), ms);

            w32(&mut r, &mut m, gfx + 0x6d8, case % 2);
            w32(&mut r, &mut m, gfx + 0x1d84, 0);
            let mr = call_fn(&mut r, 0x1e40f0, &[gfx]);
            let rr = rt::gfx_flush_if_idle(&mut m, &mut TestEnv(st.clone()), gfx);
            assert_eq!(rr, mr);

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x1e4120, &[gfx]);
            let mc = st.borrow().calls.clone();
            st.borrow_mut().calls.clear();
            rt::gfx_render_thread_entry(&mut m, &mut TestEnv(st.clone()), gfx);
            assert_eq!(st.borrow().calls, mc);
        }

        // 12 & 13: Tex_SwizzleBlock4 0x1fb4e0 & Tex_SwizzleBlock8 0x1fb600
        {
            let src = BASE + 0xa000;
            let dst = BASE + 0xa200;
            for i in 0..256u32 {
                w8(&mut r, &mut m, src + i, (rng.next() & 0xff) as u8);
            }
            let mr = call_fn(&mut r, 0x1fb4e0, &[this, src, dst]);
            let mb = copy_out(&mut r, dst, 256);
            let rr = rt::tex_swizzle_block4(&mut m, src, dst);
            assert_eq!(rr, mr);
            assert_eq!(mem_slice(&m, dst, 256), mb, "swizzle_block4 {case}");

            let mr = call_fn(&mut r, 0x1fb600, &[this, src, dst]);
            let mb = copy_out(&mut r, dst, 256);
            let rr = rt::tex_swizzle_block8(&mut m, src, dst);
            assert_eq!(rr, mr);
            assert_eq!(mem_slice(&m, dst, 256), mb, "swizzle_block8 {case}");
        }

        // 14..18: PS2VideoPlayer Init / dtor / vf2 / vf4 / vf5
        {
            let vp = BASE + 0x9000;
            st.borrow_mut().calls.clear();
            let mr = call_fn(&mut r, 0x2ba098, &[vp]);
            let mc = st.borrow().calls.clone();
            let mv = copy_out(&mut r, vp, 0x28);
            let mg1 = r.mem.read_u32(gfx + 0x668);
            let mg2 = r.mem.read_u32(gfx + 0x6f0);
            st.borrow_mut().calls.clear();
            let rr = rt::ps2_video_player_init(&mut m, &mut TestEnv(st.clone()), vp, sp_init - 0x40);
            assert_eq!(rr, mr);
            assert_eq!(st.borrow().calls, mc, "video_player_init {case}");
            assert_eq!(mem_slice(&m, vp, 0x28), mv);
            assert_eq!(m.r32(gfx + 0x668), mg1);
            assert_eq!(m.r32(gfx + 0x6f0), mg2);

            // vf2_2ba9f8
            w32(&mut r, &mut m, vp + 8, 0);
            w32(&mut r, &mut m, vp + 0xc, case % 2);
            w32(&mut r, &mut m, vp + 0x10, case % 3);
            let keys = [0u32, 0xd, 0x64, 5];
            w32(&mut r, &mut m, dev0 + 0x2c, keys[(case as usize) % 4]);
            w32(&mut r, &mut m, dev1 + 0x2c, keys[((case / 4) as usize) % 4]);
            let ev_cnt = match case % 4 {
                0 => 0,
                1 => 3,
                2 => 10,
                _ => 15,
            };
            {
                let mut s = st.borrow_mut();
                s.calls.clear();
                s.playback_left = ev_cnt;
                s.dev0 = dev0;
                s.dev1 = dev1;
            }
            call_fn(&mut r, 0x2ba9f8, &[vp]);
            let mc = st.borrow().calls.clone();
            let m8 = r.mem.read_u32(vp + 8);
            {
                let mut s = st.borrow_mut();
                s.calls.clear();
                s.playback_left = ev_cnt;
                s.dev0 = dev0;
                s.dev1 = dev1;
            }
            rt::ps2_video_player_poll_input(&mut m, &mut TestEnv(st.clone()), vp);
            assert_eq!(st.borrow().calls, mc, "video_player_vf2 {case}");
            assert_eq!(m.r32(vp + 8), m8);

            assert_eq!(call_fn(&mut r, 0x2baaf8, &[vp]), rt::ps2_video_player_vf4(&m, vp));
            assert_eq!(call_fn(&mut r, 0x2bab00, &[vp]), rt::ps2_video_player_vf5(&m, vp));

            st.borrow_mut().calls.clear();
            call_fn(&mut r, 0x2ba190, &[vp, case % 4]);
            let mc = st.borrow().calls.clone();
            let mg1 = r.mem.read_u32(gfx + 0x668);
            let mg2 = r.mem.read_u32(gfx + 0x6f0);
            st.borrow_mut().calls.clear();
            rt::ps2_video_player_dtor(&mut m, &mut TestEnv(st.clone()), vp, case % 4, sp_init - 0x60);
            assert_eq!(st.borrow().calls, mc, "video_player_dtor {case}");
            assert_eq!(m.r32(gfx + 0x668), mg1);
            assert_eq!(m.r32(gfx + 0x6f0), mg2);
        }
    }
}
