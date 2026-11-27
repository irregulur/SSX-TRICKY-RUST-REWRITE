//! Function-runner check for row G1z (the last 13 `not started` `boarder` functions):
//! - `marker`: `PlayerMarker_Construct` 0x133808, `PlayerMarker_Reset` 0x133890, `PlayerMarker_Update` 0x133940
//! - `attitude`: `AttitudeIcon_Construct` 0x134288, `AttitudeIcon_Update` 0x1343d0
//! - `riderdraw`: `cBoarderRender::cBoarderRender` 0x135278, `cBoarderRender::~cBoarderRender` 0x1355c8
//! - `aiobj`: `cPlayer::~cPlayer` 0x151080
//! - `camera_misc`: `cCamControl::~cCamControl` 0x178240, `cCamControl::vf1_1784d0` 0x1784d0,
//!   `cCamControl::vf2_178dc0` 0x178dc0
//! - `boarder_life`: `cBoarderHandler::cBoarderHandler` 0x11b348, `cBoarderHandler::~cBoarderHandler` 0x11f770

use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::camera_misc::CamMiscEnv;
use tricky_game::fe3dstate::{FeEnv, GAPP};
use tricky_game::pf::Mem;
use tricky_game::riderdraw::RiderDrawEnv;
use tricky_game::{aiobj, attitude, boarder_life, camera_misc, marker, riderdraw};

const BASE: u32 = 0x0140_0000;

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
    let p = std::env::var("TRICKY_ELF").ok()?;
    let bytes = std::fs::read(&p).expect("read ELF");
    Some(Runner::from_elf(&bytes).expect("load ELF"))
}

fn call_fn(r: &mut Runner, addr: u32, args: &[u32]) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn copy_out(r: &mut Runner, at: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    r.mem.read(at, &mut b).unwrap();
    b
}

fn fresh_mem() -> Mem {
    Mem {
        regions: vec![
            (0x0000_0000, vec![0u8; 0x200]),
            (0x0032_0000, vec![0u8; 0x000a_0000]),
            (BASE, vec![0u8; 0x20000]),
            (0x01fe_0000, vec![0u8; 0x10000]),
        ],
    }
}

fn mem_write(m: &mut Mem, at: u32, bytes: &[u8]) {
    for (i, &b) in bytes.iter().enumerate() {
        m.w8(at + i as u32, b);
    }
}

fn mem_slice(m: &Mem, at: u32, len: usize) -> Vec<u8> {
    (0..len).map(|i| m.r8(at + i as u32)).collect()
}

fn copy_elf_vt(r: &mut Runner, m: &mut Mem, vt: u32) {
    let bytes = copy_out(r, vt, 0xb0);
    mem_write(m, vt, &bytes);
}

#[derive(Default)]
struct FeLog {
    calls: Vec<(u32, Vec<u32>)>,
    alloc_seq: Vec<u32>,
    alloc_i: usize,
}
impl FeEnv for FeLog {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.calls.push((func, args.to_vec()));
        match func {
            riderdraw::EAMEM_ALLOC => {
                let v = self.alloc_seq.get(self.alloc_i).copied().unwrap_or(0);
                self.alloc_i += 1;
                v
            }
            riderdraw::BOARDER_SHADOW_INIT
            | riderdraw::PLAYER_MARKER_CTOR
            | riderdraw::ATTITUDE_ICON_CTOR => args[0].wrapping_add(0x10),
            _ => 0,
        }
    }
}

#[derive(Default)]
struct RiderLog {
    calls: Vec<(u32, Vec<u32>)>,
    rider_ret: [u32; 2],
}
impl RiderDrawEnv for RiderLog {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.calls.push((func, args.to_vec()));
        if func == attitude::FIND_RIDER_OF_PLAYER {
            self.rider_ret[(args[1] & 1) as usize]
        } else {
            0
        }
    }
    fn vcall(&mut self, _m: &mut Mem, func: u32, this: u32, args: &[u32]) -> u32 {
        let mut v = vec![this];
        v.extend_from_slice(args);
        self.calls.push((func, v));
        0
    }
}

#[derive(Default)]
struct CamLog {
    calls: Vec<(u32, Vec<u32>)>,
    menu_active: u32,
    overlay_active: u32,
}
impl CamMiscEnv for CamLog {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.calls.push((func, args.to_vec()));
        if func == camera_misc::MENU_IS_ACTIVE {
            self.menu_active
        } else if func == camera_misc::OVERLAY_IS_ANY_ACTIVE {
            self.overlay_active
        } else {
            0
        }
    }
    fn call_f(&mut self, _m: &mut Mem, func: u32, args: &[u32], f12: f32) -> u32 {
        let mut v = args.to_vec();
        v.push(f12.to_bits());
        self.calls.push((func, v));
        0
    }
}

#[test]
fn marker_construct_reset_update() {
    let Some(mut r) = runner() else { return };
    r.allow(0x1230e0); // EaseRecord_Init
    r.allow(0x133890); // PlayerMarker_Reset
    let mut rng = Rng(0x1338_0801);
    let mk = BASE;
    let rider = BASE + 0x1000;
    let skel = BASE + 0x6000;
    let mats = BASE + 0x6100;
    let part = BASE + 0x6400;

    // 1. PlayerMarker_Construct 0x133808 & PlayerMarker_Reset 0x133890
    for case in 0..400 {
        let mut buf = vec![0u8; 0x260];
        for b in &mut buf {
            *b = rng.next() as u8;
        }
        r.mem.write(mk, &buf).unwrap();
        let mut m = fresh_mem();
        mem_write(&mut m, mk, &buf);
        if case < 200 {
            let rv_o = call_fn(&mut r, 0x133808, &[mk]);
            let rv_p = marker::construct(&mut m, mk);
            assert_eq!(rv_p, rv_o, "construct rv case {case}");
        } else {
            call_fn(&mut r, 0x133890, &[mk]);
            marker::reset(&mut m, mk);
        }
        assert_eq!(mem_slice(&m, mk, 0x260), copy_out(&mut r, mk, 0x260), "marker mem case {case}");
    }

    // 2. PlayerMarker_Update 0x133940
    for case in 0..1000 {
        let mut mk_buf = vec![0u8; 0x260];
        for b in &mut mk_buf {
            *b = rng.next() as u8;
        }
        let mut m = fresh_mem();
        mem_write(&mut m, mk, &mk_buf);
        r.mem.write(mk, &mk_buf).unwrap();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };

        let has_skel = case % 7 != 0;
        w(&mut m, &mut r, rider + 0x4494, if has_skel { skel } else { 0 });
        w(&mut m, &mut r, skel + 4, mats);
        if has_skel {
            for k in 0..4u32 {
                let fv = rng.f(-5000.0, 5000.0).to_bits();
                w(&mut m, &mut r, mats + 0x130 + 4 * k, fv);
            }
        } else {
            let zbytes = copy_out(&mut r, 0x130, 0x10);
            mem_write(&mut m, 0x130, &zbytes);
        }

        let f240 = if case % 5 == 0 { 0x469c_4000 } else { rng.f(10.0, 25000.0).to_bits() };
        let f244 = rng.f(-500.0, 500.0).to_bits();
        let f248 = rng.f(-500.0, 500.0).to_bits();
        let f250 = rng.f(50.0, 1000.0).to_bits();
        w(&mut m, &mut r, mk + 0x240, f240);
        w(&mut m, &mut r, mk + 0x244, f244);
        w(&mut m, &mut r, mk + 0x248, f248);
        w(&mut m, &mut r, mk + 0x250, f250);

        let cur = match case % 6 {
            0 => 20.0f32,
            1 => 40.0,
            2 => rng.f(0.0, 20.0),
            3 => rng.f(40.0, 60.0),
            _ => rng.f(20.1, 39.9),
        };
        let rate = rng.f(0.0, 120.0);
        let target = match case % 3 {
            0 => 20.0f32,
            1 => 40.0,
            _ => cur + rng.f(-1.5, 1.5),
        };
        w(&mut m, &mut r, mk + 0x254, cur.to_bits());
        w(&mut m, &mut r, mk + 0x258, rate.to_bits());
        w(&mut m, &mut r, mk + 0x25c, target.to_bits());

        w(&mut m, &mut r, rider + 0x464, part);
        let pid = match case % 4 {
            0 => 0u8,
            1 => 1,
            2 => 0xff,
            _ => (rng.next() & 0xff) as u8,
        };
        m.w8(part + 0x65, pid);
        r.mem.write(part + 0x65, &[pid]).unwrap();

        call_fn(&mut r, 0x133940, &[mk, rider]);
        marker::update(&mut m, mk, rider);
        assert_eq!(mem_slice(&m, mk, 0x260), copy_out(&mut r, mk, 0x260), "update case {case}");
    }
}

#[test]
fn attitude_construct_update() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x1342_8802);
    let icon = BASE;
    let app = BASE + 0x1000;
    let game = BASE + 0x1800;
    let ai = BASE + 0x2000;
    let replay = BASE + 0x2400;
    let rider = BASE + 0x3000;
    let skel = BASE + 0x8000;
    let mats = BASE + 0x8100;

    // 1. AttitudeIcon_Construct 0x134288
    for case in 0..200 {
        let mut buf = vec![0u8; 0x150];
        for b in &mut buf {
            *b = rng.next() as u8;
        }
        r.mem.write(icon, &buf).unwrap();
        let mut m = fresh_mem();
        mem_write(&mut m, icon, &buf);
        let rv_o = call_fn(&mut r, 0x134288, &[icon]);
        let rv_p = attitude::construct(&mut m, icon);
        assert_eq!(rv_p, rv_o, "att construct rv case {case}");
        assert_eq!(mem_slice(&m, icon, 0x150), copy_out(&mut r, icon, 0x150), "att construct mem case {case}");
    }

    // 2. AttitudeIcon_Update 0x1343d0
    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>)>::new()));
    let rret = Rc::new(RefCell::new([0u32; 2]));
    {
        let log = log.clone();
        let rret = rret.clone();
        r.stub(attitude::FIND_RIDER_OF_PLAYER, move |c, _| {
            let a0 = c.gpr(4) as u32;
            let a1 = c.gpr(5) as u32;
            log.borrow_mut().push((attitude::FIND_RIDER_OF_PLAYER, vec![a0, a1]));
            let v = rret.borrow()[(a1 & 1) as usize];
            c.set_gpr(2, v as u64);
        });
    }

    for case in 0..1500 {
        log.borrow_mut().clear();
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        let wb = |m: &mut Mem, r: &mut Runner, a: u32, v: u8| {
            m.w8(a, v);
            r.mem.write(a, &[v]).unwrap();
        };

        let mut ibuf = vec![0u8; 0x150];
        for b in &mut ibuf {
            *b = rng.next() as u8;
        }
        r.mem.write(icon, &ibuf).unwrap();
        mem_write(&mut m, icon, &ibuf);

        for s in 0..2u32 {
            let sl = icon + 0x80 * s;
            w(&mut m, &mut r, sl + 0x9c, rng.next() % 4);
            w(&mut m, &mut r, sl + 0xa0, (rng.next() % 3 == 0) as u32);
            let a4 = match rng.next() % 4 {
                0 => 1.0f32,
                1 => 1.2,
                2 => 0.99,
                _ => rng.f(0.0, 0.98),
            };
            w(&mut m, &mut r, sl + 0xa4, a4.to_bits());
            wb(&mut m, &mut r, sl + 0xa8, (rng.next() & 0xff) as u8);
            w(&mut m, &mut r, sl + 0xac, (rng.next() % 3 == 0) as u32);
            let b0 = match rng.next() % 4 {
                0 => 1.0f32,
                1 => 1.1,
                2 => 0.995,
                _ => rng.f(0.0, 0.98),
            };
            w(&mut m, &mut r, sl + 0xb0, b0.to_bits());
            w(&mut m, &mut r, sl + 0xb4, rng.next() & 1);
            let b8 = match rng.next() % 5 {
                0 => 0.02f32,
                1 => 0.97,
                2 => 0.0,
                3 => 1.0,
                _ => rng.f(0.0, 1.0),
            };
            w(&mut m, &mut r, sl + 0xb8, b8.to_bits());
        }

        let mode = match case % 10 {
            0 => 0,
            1 => 1,
            2 => 3,
            3..=6 => 2,
            _ => 4,
        };
        w(&mut m, &mut r, riderdraw::CAREER + 0x2afc, mode);
        w(&mut m, &mut r, riderdraw::APP, app);
        w(&mut m, &mut r, app + 0x730, game);
        let has_rep = case % 4 != 0;
        w(&mut m, &mut r, game + 0x40, if has_rep { replay } else { 0 });
        let rep_state = match case % 7 {
            0 => 0,
            1 => 5,
            2 => 9,
            3 => 10,
            4 => 15,
            5 => 0xffff_ffff,
            _ => 0,
        };
        w(&mut m, &mut r, replay + 0x20, rep_state);
        w(&mut m, &mut r, game + 0x1c, ai);
        w(&mut m, &mut r, ai + 0x1c, if case % 9 == 0 { 3 } else { 4 });
        w(&mut m, &mut r, rider + 0x41c, if case % 11 == 0 { 1 } else { 0 });
        w(&mut m, &mut r, game + 0x298, if case % 3 == 0 { 2 } else { 1 });
        w(&mut m, &mut r, ai + 0x7c, if case % 2 == 0 { 2 } else { 1 });

        let r0 = rng.next() % 6;
        let r1 = rng.next() % 6;
        *rret.borrow_mut() = [r0, r1];
        for ridx in 0..6u32 {
            let ab = rider + 0x10 + (ridx << 5);
            let att_byte = match rng.next() % 5 {
                0 => (rng.next() % 60) as u8,
                1 => (60 + rng.next() % 21) as u8,
                2 => (81 + rng.next() % 47) as u8,
                3 => 0x80u8.wrapping_add((rng.next() % 128) as u8),
                _ => (rng.next() & 0xff) as u8,
            };
            wb(&mut m, &mut r, ab + 0x40, att_byte);
            wb(&mut m, &mut r, ab + 0x43, (rng.next() & 0xff) as u8);
        }
        w(&mut m, &mut r, rider + 0x118, (rng.next() % 3 != 0) as u32);
        w(&mut m, &mut r, rider + 0x120, if rng.next() & 1 == 0 { r0 } else { r1 });
        w(&mut m, &mut r, rider + 0x124, rng.next() % 3);

        let has_skel = case % 8 != 0;
        w(&mut m, &mut r, rider + 0x4494, if has_skel { skel } else { 0 });
        w(&mut m, &mut r, skel + 4, mats);
        for k in 0..4u32 {
            w(&mut m, &mut r, rider + 0x4ac0 + 4 * k, rng.f(-1.0, 1.0).to_bits());
        }
        if has_skel {
            for k in 0..4u32 {
                w(&mut m, &mut r, mats + 0x130 + 4 * k, rng.f(-5000.0, 5000.0).to_bits());
            }
        } else {
            let zbytes = copy_out(&mut r, 0x130, 0x10);
            mem_write(&mut m, 0x130, &zbytes);
        }

        call_fn(&mut r, 0x1343d0, &[icon, rider]);
        let mut env = RiderLog { calls: vec![], rider_ret: [r0, r1] };
        attitude::update(&mut m, &mut env, icon, rider);
        assert_eq!(env.calls, *log.borrow(), "att update calls case {case}");
        assert_eq!(mem_slice(&m, icon, 0x150), copy_out(&mut r, icon, 0x150), "att update mem case {case}");
    }
}

#[test]
fn boarder_render_ctor_dtor() {
    let Some(mut r) = runner() else { return };
    r.allow(0x1230e0); // EaseRecord_Init
    let mut rng = Rng(0x1352_7803);
    let br = BASE;
    let vb_ext = BASE + 0x1000;

    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>)>::new()));
    let aseq = Rc::new(RefCell::new((Vec::<u32>::new(), 0usize)));

    for &(addr, nargs) in &[
        (riderdraw::BOARDER_CTOR, 1usize),
        (riderdraw::BOARDER_DTOR, 2),
        (riderdraw::MEM_DELETE, 1),
        (riderdraw::EAMEM_SET, 3),
        (riderdraw::FX_BOARD_PUFF_INIT, 1),
        (riderdraw::FX_BOARD_TRAIL_INIT, 1),
        (riderdraw::FX_SURFACE_SPRAY_INIT, 1),
        (riderdraw::FX_BOARD_TRACK_INIT, 1),
        (riderdraw::FX_BRAKE_SPRAY_INIT, 1),
        (riderdraw::FX_SPEED_DUST_INIT, 1),
        (riderdraw::DESTRUCT_EFFECTS, 2),
    ] {
        let log = log.clone();
        r.stub(addr, move |c, _| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            log.borrow_mut().push((addr, args));
            c.set_gpr(2, 0);
        });
    }
    for &addr in &[
        riderdraw::BOARDER_SHADOW_INIT,
        riderdraw::PLAYER_MARKER_CTOR,
        riderdraw::ATTITUDE_ICON_CTOR,
    ] {
        let log = log.clone();
        r.stub(addr, move |c, _| {
            let a0 = c.gpr(4) as u32;
            log.borrow_mut().push((addr, vec![a0]));
            c.set_gpr(2, a0.wrapping_add(0x10) as u64);
        });
    }
    {
        let log = log.clone();
        let aseq = aseq.clone();
        r.stub(riderdraw::EAMEM_ALLOC, move |c, _| {
            let args = vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32];
            log.borrow_mut().push((riderdraw::EAMEM_ALLOC, args));
            let mut s = aseq.borrow_mut();
            let v = s.0.get(s.1).copied().unwrap_or(0);
            s.1 += 1;
            c.set_gpr(2, v as u64);
        });
    }

    // 1. cBoarderRender::cBoarderRender 0x135278
    for case in 0..400 {
        log.borrow_mut().clear();
        let allocs = vec![
            if rng.next() % 4 == 0 { 0 } else { 0x0160_0000 },
            if rng.next() % 4 == 0 { 0 } else { 0x0161_0000 },
            if rng.next() % 4 == 0 { 0 } else { 0x0162_0000 },
            if rng.next() % 4 == 0 { 0 } else { 0x0163_0000 },
        ];
        *aseq.borrow_mut() = (allocs.clone(), 0);

        let mut buf = vec![0u8; 0x500];
        for b in &mut buf {
            *b = rng.next() as u8;
        }
        r.mem.write(br, &buf).unwrap();
        let mut m = fresh_mem();
        mem_write(&mut m, br, &buf);
        copy_elf_vt(&mut r, &mut m, riderdraw::BOARDER_RENDER_VT);

        let in_charge = (case & 1) as u32;
        let vb = if in_charge != 0 { br + 0x480 } else { vb_ext + (rng.next() & 0xf0) };
        if in_charge == 0 {
            m.w32(br, vb);
            r.mem.write_u32(br, vb);
        }
        for k in 0..3u32 {
            let v = rng.next();
            m.w32(riderdraw::RENDER_INIT_FLOATS + 4 * k, v);
            r.mem.write_u32(riderdraw::RENDER_INIT_FLOATS + 4 * k, v);
        }

        let rv_o = call_fn(&mut r, 0x135278, &[br, in_charge]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = FeLog { calls: vec![], alloc_seq: allocs, alloc_i: 0 };
        let rv_p = riderdraw::construct(&mut m, &mut env, br, in_charge, sp);
        assert_eq!(rv_p, rv_o, "br ctor rv case {case}");
        assert_eq!(env.calls, *log.borrow(), "br ctor calls case {case}");
        assert_eq!(mem_slice(&m, br, 0x500), copy_out(&mut r, br, 0x500), "br ctor mem case {case}");
        assert_eq!(m.r32(vb), r.mem.read_u32(vb), "br ctor vb vt case {case}");
        if in_charge == 0 {
            let copy = sp.wrapping_sub(0x160);
            assert_eq!(mem_slice(&m, copy, 0xb0), copy_out(&mut r, copy, 0xb0), "br ctor stack vt case {case}");
        }
    }

    // 2. cBoarderRender::~cBoarderRender 0x1355c8
    for case in 0..400 {
        log.borrow_mut().clear();
        let mut buf = vec![0u8; 0x500];
        for b in &mut buf {
            *b = rng.next() as u8;
        }
        r.mem.write(br, &buf).unwrap();
        let mut m = fresh_mem();
        mem_write(&mut m, br, &buf);
        copy_elf_vt(&mut r, &mut m, riderdraw::BOARDER_RENDER_VT);

        let how = (case % 4) as u32;
        let vb = vb_ext + (rng.next() & 0xf0);
        m.w32(br, vb);
        r.mem.write_u32(br, vb);
        let fx = if case % 3 == 0 { 0 } else { 0x0160_0000 | (rng.next() & 0xff0) };
        m.w32(br + 0x3e0, fx);
        r.mem.write_u32(br + 0x3e0, fx);

        call_fn(&mut r, 0x1355c8, &[br, how]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = FeLog::default();
        riderdraw::destruct(&mut m, &mut env, br, how, sp);
        assert_eq!(env.calls, *log.borrow(), "br dtor calls case {case}");
        assert_eq!(mem_slice(&m, br, 0x500), copy_out(&mut r, br, 0x500), "br dtor mem case {case}");
        assert_eq!(m.r32(vb), r.mem.read_u32(vb), "br dtor vb vt case {case}");
        if how == 0 {
            let copy = sp.wrapping_sub(0xf0);
            assert_eq!(mem_slice(&m, copy, 0xb0), copy_out(&mut r, copy, 0xb0), "br dtor stack vt case {case}");
        }
    }
}

#[test]
fn player_destruct_runner() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x1510_8004);
    let app = BASE;
    let game = BASE + 0x800;
    let o = BASE + 0x1000;
    let vb_ext = BASE + 0x2000;

    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>)>::new()));
    for &(addr, nargs) in &[
        (aiobj::REPLAY_REMOVE_INPUT_STREAM, 2usize),
        (aiobj::REPLAY_REMOVE_PIN, 2),
        (aiobj::DELETE, 1),
        (aiobj::BOARDER_HANDLER_DTOR, 2),
        (aiobj::BOARDER_DTOR, 2),
    ] {
        let log = log.clone();
        r.stub(addr, move |c, _| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            log.borrow_mut().push((addr, args));
            c.set_gpr(2, 0);
        });
    }

    for case in 0..400 {
        log.borrow_mut().clear();
        let mut buf = vec![0u8; 0x900];
        for b in &mut buf {
            *b = rng.next() as u8;
        }
        r.mem.write(o, &buf).unwrap();
        let mut m = fresh_mem();
        mem_write(&mut m, o, &buf);
        copy_elf_vt(&mut r, &mut m, aiobj::PLAYER_VBASE_VT);

        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        w(&mut m, &mut r, GAPP, app);
        w(&mut m, &mut r, app + 0x730, game);
        w(&mut m, &mut r, game + 0x40, 0x0170_0000 | (rng.next() & 0xff0));
        let vb = vb_ext + (rng.next() & 0x1f0);
        w(&mut m, &mut r, o, vb);
        w(&mut m, &mut r, o + 0x878, if case % 3 == 0 { 0 } else { rng.next() | 1 });
        w(&mut m, &mut r, o + 0x87c, if case % 5 == 0 { 0 } else { rng.next() | 1 });
        w(&mut m, &mut r, o + 0x874, rng.next());

        let how = (case % 4) as u32;
        call_fn(&mut r, 0x151080, &[o, how]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = FeLog::default();
        aiobj::player_destruct(&mut m, &mut env, o, how, sp);
        assert_eq!(env.calls, *log.borrow(), "player dtor calls case {case}");
        assert_eq!(mem_slice(&m, o, 0x900), copy_out(&mut r, o, 0x900), "player dtor mem case {case}");
        assert_eq!(m.r32(vb), r.mem.read_u32(vb), "player dtor vb vt case {case}");
        if how == 0 {
            let copy = sp.wrapping_sub(0x120);
            assert_eq!(mem_slice(&m, copy, 0xb0), copy_out(&mut r, copy, 0xb0), "player dtor stack vt case {case}");
        }
    }
}

#[test]
fn cam_control_runner() {
    let Some(mut r) = runner() else { return };
    r.allow(0x179068); // Cml_FreeHandle
    r.allow(0x1782d0); // Camera_LoadCmlOnce
    let mut rng = Rng(0x1782_4005);
    let app = BASE;
    let game = BASE + 0x800;
    let obj = BASE + 0x1000;
    let cams = BASE + 0x2000;
    let vts = BASE + 0x8000;

    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>)>::new()));
    let menu_flag = Rc::new(RefCell::new(0u32));
    let overlay_flag = Rc::new(RefCell::new(0u32));

    for &(addr, nargs) in &[
        (camera_misc::MEM_DELETE, 1usize),
        (camera_misc::BX_OBJ_NODE_DTOR, 2),
        (camera_misc::LOAD_CML_FILES, 0),
        (camera_misc::BX_CAMERA_UPDATE, 1),
        (camera_misc::CAM_SET_SHOT, 1),
    ] {
        let log = log.clone();
        r.stub(addr, move |c, _| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            log.borrow_mut().push((addr, args));
            c.set_gpr(2, 0);
        });
    }
    {
        let log = log.clone();
        let mf = menu_flag.clone();
        r.stub(camera_misc::MENU_IS_ACTIVE, move |c, _| {
            log.borrow_mut().push((camera_misc::MENU_IS_ACTIVE, vec![c.gpr(4) as u32]));
            c.set_gpr(2, *mf.borrow() as u64);
        });
    }
    {
        let log = log.clone();
        let of = overlay_flag.clone();
        r.stub(camera_misc::OVERLAY_IS_ANY_ACTIVE, move |c, _| {
            log.borrow_mut().push((camera_misc::OVERLAY_IS_ANY_ACTIVE, vec![c.gpr(4) as u32]));
            c.set_gpr(2, *of.borrow() as u64);
        });
    }

    // 1. cCamControl::~cCamControl 0x178240
    for case in 0..200 {
        log.borrow_mut().clear();
        let mut m = fresh_mem();
        for &h in &camera_misc::CML_HANDLES {
            let v = rng.next();
            m.w32(h, v);
            r.mem.write_u32(h, v);
        }
        let init_vt = rng.next();
        m.w32(obj + 8, init_vt);
        r.mem.write_u32(obj + 8, init_vt);
        let how = (case % 4) as u32;
        call_fn(&mut r, 0x178240, &[obj, how]);
        let mut env = CamLog::default();
        camera_misc::cam_control_destruct(&mut m, &mut env, obj, how);
        assert_eq!(env.calls, *log.borrow(), "cam dtor calls case {case}");
        assert_eq!(m.r32(obj + 8), r.mem.read_u32(obj + 8), "cam dtor vt case {case}");
    }

    // 2. cCamControl::vf1_1784d0 0x1784d0
    for case in 0..500 {
        log.borrow_mut().clear();
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        w(&mut m, &mut r, camera_misc::GAPP, app);
        w(&mut m, &mut r, app + 0x730, game);
        w(&mut m, &mut r, game + 0x38, 0x0180_0000 | (rng.next() & 0xff0));
        let pending = (case % 3) as u32;
        w(&mut m, &mut r, camera_misc::CML_PENDING, pending);
        let ma = (case % 7 == 0) as u32;
        let oa = (case % 5 == 0) as u32;
        *menu_flag.borrow_mut() = ma;
        *overlay_flag.borrow_mut() = oa;
        let use_static = (case % 4 == 0) as u32;
        w(&mut m, &mut r, camera_misc::CAM_USE_STATIC, use_static);
        let count = ((case % 6) as i32 - 1) as u32;
        w(&mut m, &mut r, camera_misc::CAM_LIST_COUNT, count);
        for i in 0..4u32 {
            let cam = cams + i * 0x1000;
            let vt = vts + i * 0x40;
            w(&mut m, &mut r, camera_misc::CAM_LIST_PTRS + 4 * i, cam);
            w(&mut m, &mut r, cam + 0xeb8, vt);
            let adj = ((rng.next() % 0x200) as i32 - 0x100) as i16 as u16;
            m.w16(vt + 0x10, adj);
            r.mem.write(vt + 0x10, &adj.to_le_bytes()).unwrap();
            let target = if i & 1 == 0 { camera_misc::BX_CAMERA_UPDATE } else { camera_misc::CAM_SET_SHOT };
            w(&mut m, &mut r, vt + 0x14, target);
        }

        call_fn(&mut r, 0x1784d0, &[obj]);
        let mut env = CamLog { calls: vec![], menu_active: ma, overlay_active: oa };
        camera_misc::cam_control_update(&mut m, &mut env);
        assert_eq!(env.calls, *log.borrow(), "cam vf1 calls case {case}");
        assert_eq!(m.r32(camera_misc::CML_PENDING), r.mem.read_u32(camera_misc::CML_PENDING), "cam vf1 pending case {case}");
    }

    // 3. cCamControl::vf2_178dc0 0x178dc0
    for case in 0..50 {
        call_fn(&mut r, 0x178dc0, &[obj, case]);
        camera_misc::cam_control_vf2();
    }
}

#[test]
fn boarder_handler_ctor_dtor() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x11b3_4806);
    let app = BASE;
    let game = BASE + 0x800;
    let o = BASE + 0x2000;
    let vb_ext = BASE + 0x10000;

    let log = Rc::new(RefCell::new(Vec::<(u32, Vec<u32>)>::new()));
    let observed_vb = Rc::new(RefCell::new(Vec::<(u32, u32)>::new()));
    let cur_vb = Rc::new(RefCell::new(0u32));

    for &(addr, nargs) in &[
        (boarder_life::BOARDER_CTOR, 1usize),
        (boarder_life::BOARDER_DTOR, 2),
        (boarder_life::BOARDER_RENDER_CTOR, 2),
        (boarder_life::BOARDER_RENDER_DTOR, 2),
        (boarder_life::RIGID_BODY_CTOR, 1),
        (boarder_life::SORT_OBJ_NODE_CTOR, 3),
        (boarder_life::SORT_OBJ_NODE_DTOR, 2),
        (boarder_life::BROADPHASE_SET_BOX, 2),
        (boarder_life::BROADPHASE_FIT_BOX, 2),
        (boarder_life::MEM_DELETE, 1),
    ] {
        let log = log.clone();
        let obs = observed_vb.clone();
        let cvb = cur_vb.clone();
        r.stub(addr, move |c, mem| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            log.borrow_mut().push((addr, args));
            let vb = *cvb.borrow();
            if vb != 0 {
                obs.borrow_mut().push((addr, mem.read_u32(vb)));
            }
            c.set_gpr(2, 0);
        });
    }

    struct HandlerLog {
        calls: Vec<(u32, Vec<u32>)>,
        obs: Vec<(u32, u32)>,
        vb: u32,
    }
    impl FeEnv for HandlerLog {
        fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
            self.calls.push((func, args.to_vec()));
            if self.vb != 0 {
                self.obs.push((func, m.r32(self.vb)));
            }
            0
        }
    }

    let mut vts = vec![boarder_life::BOARDER_HANDLER_VBASE_VT];
    for &(_, _, _, vt) in &boarder_life::HANDLER_STATES {
        vts.push(vt);
    }

    // 1. cBoarderHandler::cBoarderHandler 0x11b348
    for case in 0..400 {
        log.borrow_mut().clear();
        observed_vb.borrow_mut().clear();
        let mut m = fresh_mem();
        for &vt in &vts {
            copy_elf_vt(&mut r, &mut m, vt);
        }
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        w(&mut m, &mut r, boarder_life::GAPP, app);
        w(&mut m, &mut r, app + 0x730, game);
        w(&mut m, &mut r, game + 0x30, 0x0190_0000 | (rng.next() & 0xff0));

        let in_charge = (case & 1) as u32;
        let vb = if in_charge != 0 { o + 0x860 } else { vb_ext + (rng.next() & 0x1f0) };
        *cur_vb.borrow_mut() = vb;
        w(&mut m, &mut r, vb, 0);
        if in_charge == 0 {
            w(&mut m, &mut r, o, vb);
            for &(state_off, at, _, _) in &boarder_life::HANDLER_STATES {
                w(&mut m, &mut r, o + state_off + at, vb);
            }
        }
        let rider_idx = (rng.next() as i32) as i16;

        let rv_o = call_fn(&mut r, 0x11b348, &[o, in_charge, rider_idx as i32 as u32]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = HandlerLog { calls: vec![], obs: vec![], vb };
        let rv_p = boarder_life::boarder_handler_construct(&mut m, &mut env, o, in_charge, rider_idx, sp);
        assert_eq!(rv_p, rv_o, "bh ctor rv case {case}");
        assert_eq!(env.calls, *log.borrow(), "bh ctor calls case {case}");
        assert_eq!(env.obs, *observed_vb.borrow(), "bh ctor intermediate vb case {case}");
        assert_eq!(m.r32(o + 0x834), r.mem.read_u32(o + 0x834), "bh ctor own vt case {case}");
        assert_eq!(m.r32(vb), r.mem.read_u32(vb), "bh ctor vb vt case {case}");
        assert_eq!(m.r32(vb + 0x784), r.mem.read_u32(vb + 0x784), "bh ctor proxy node case {case}");
        assert_eq!(m.r32(vb + 0x5ae0), r.mem.read_u32(vb + 0x5ae0), "bh ctor backptr case {case}");
        for &(state_off, at, _, _) in &boarder_life::HANDLER_STATES {
            let slot = o + state_off + at;
            assert_eq!(m.r32(slot), r.mem.read_u32(slot), "bh ctor slot {slot:#x} case {case}");
        }
        assert_eq!(m.r32(o), r.mem.read_u32(o), "bh ctor slot 0 case {case}");
        if in_charge == 0 {
            let copy = sp.wrapping_sub(0x220);
            assert_eq!(mem_slice(&m, copy, 0xb0), copy_out(&mut r, copy, 0xb0), "bh ctor stack vt case {case}");
        }
    }

    // 2. cBoarderHandler::~cBoarderHandler 0x11f770
    for case in 0..400 {
        log.borrow_mut().clear();
        observed_vb.borrow_mut().clear();
        let mut m = fresh_mem();
        for &vt in &vts {
            copy_elf_vt(&mut r, &mut m, vt);
        }
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        let vb = vb_ext + (rng.next() & 0x1f0);
        *cur_vb.borrow_mut() = vb;
        w(&mut m, &mut r, o, vb);
        for &(state_off, at, _, _) in &boarder_life::HANDLER_STATES {
            w(&mut m, &mut r, o + state_off + at, vb);
        }
        let how = (case % 4) as u32;
        call_fn(&mut r, 0x11f770, &[o, how]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = HandlerLog { calls: vec![], obs: vec![], vb };
        boarder_life::boarder_handler_destruct(&mut m, &mut env, o, how, sp);
        assert_eq!(env.calls, *log.borrow(), "bh dtor calls case {case}");
        assert_eq!(env.obs, *observed_vb.borrow(), "bh dtor intermediate vb case {case}");
        assert_eq!(m.r32(o + 0x834), r.mem.read_u32(o + 0x834), "bh dtor own vt case {case}");
        assert_eq!(m.r32(vb), r.mem.read_u32(vb), "bh dtor vb vt case {case}");
        let last_copy = sp.wrapping_sub(0x2c0);
        assert_eq!(mem_slice(&m, last_copy, 0xb0), copy_out(&mut r, last_copy, 0xb0), "bh dtor last state stack vt case {case}");
        if how == 0 {
            let own_copy = sp.wrapping_sub(0x210);
            assert_eq!(mem_slice(&m, own_copy, 0xb0), copy_out(&mut r, own_copy, 0xb0), "bh dtor own stack vt case {case}");
        }
    }
}
