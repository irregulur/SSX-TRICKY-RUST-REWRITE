//! Function-runner check for row G10v (the last 9 `not started` `audio` functions, bringing `audio` to 0):
//! `Audio_StopQueuedSounds` 0x215c28, `Sfx_PlayRiderBump` 0x215f18, `Sfx_PlayTakeoff` 0x216d58,
//! `Music_UpdateMeterCues` 0x21c6d0, `Sfx_PlayClipBoardSound51` 0x21d680, `Sfx_PlayClipBoardSound50` 0x21d9b0,
//! `Music_PlayMeterCue1` 0x21ddc0, `Music_PlayMeterCue2` 0x21dfe8, `Music_RestartMeterCue` 0x21e248.

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::sndqueue::{self as sq, SoundMgrEnv};

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
    let mut r = Runner::from_env()?;
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    for a in [
        0x21ddc0, // Music_PlayMeterCue1
        0x21dfe8, // Music_PlayMeterCue2
        0x21e248, // Music_RestartMeterCue
        0x2fb468, // memset
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
    let z = vec![0u8; 0x30000];
    r.mem.write(BASE, &z);
    Mem {
        regions: vec![(BASE, vec![0u8; 0x30000])],
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
    find_near: i32,
    any_ahead: u32,
    is_queued: u32,
    eval_curve: i32,
    slot_ret: u32,
    nearer_p2: u32,
    play_ret: u32,
    impact_speed: i32,
    surf_type: u32,
    rider_listener: u32,
    handle_dead: u32,
}

impl SoundMgrEnv for EnvState {
    fn call(&mut self, _m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        self.calls.push((target, args.to_vec()));
        match target {
            sq::AUDIO_FIND_RIDER_NEAR => self.find_near as u32,
            sq::AUDIO_ANY_RIDER_NEAR_AHEAD => self.any_ahead,
            sq::SND_SYS_IS_QUEUED => self.is_queued,
            sq::SFX_EVAL_CURVE => self.eval_curve as u32,
            sq::SND_MGR_GET_SLOT => self.slot_ret,
            sq::AUDIO_IS_NEARER_PLAYER2 => self.nearer_p2,
            sq::SND_SYS_PLAY_PENDING => self.play_ret,
            sq::SFX_IMPACT_SPEED => self.impact_speed as u32,
            sq::SFX_SURFACE_SOUND_TYPE => self.surf_type,
            sq::AUDIO_GET_RIDER_LISTENER => self.rider_listener,
            sq::SND_HANDLE_IS_DEAD => self.handle_dead,
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    let gpr_stubs: &[(u32, usize)] = &[
        (sq::SND_SYS_PLAY_PENDING, 1),
        (sq::SND_MGR_GET_SLOT, 2),
        (sq::SFX_EVAL_CURVE, 2),
        (sq::SFX_SURFACE_SOUND_TYPE, 2),
        (sq::AUDIO_IS_NEARER_PLAYER2, 2),
        (sq::AUDIO_GET_RIDER_LISTENER, 2),
        (sq::SND_SYS_IS_QUEUED, 2),
        (sq::SND_SYS_QUEUE_SOUNDS, 5),
        (sq::SFX_IMPACT_SPEED, 1),
        (sq::MUSIC_CANCEL_TRICKY, 1),
        (sq::SND_LOCK, 0),
        (sq::SND_UNLOCK, 0),
        (sq::SND_HANDLE_IS_DEAD, 1),
    ];
    for &(addr, nargs) in gpr_stubs {
        let s = st.clone();
        r.stub(addr, move |cpu, _| {
            let mut a = Vec::with_capacity(nargs);
            for k in 0..nargs {
                a.push(cpu.gpr(4 + k) as u32);
            }
            let mut e = s.borrow_mut();
            e.calls.push((addr, a));
            let rv = match addr {
                sq::SND_SYS_IS_QUEUED => e.is_queued,
                sq::SFX_EVAL_CURVE => e.eval_curve as u32,
                sq::SND_MGR_GET_SLOT => e.slot_ret,
                sq::AUDIO_IS_NEARER_PLAYER2 => e.nearer_p2,
                sq::SND_SYS_PLAY_PENDING => e.play_ret,
                sq::SFX_IMPACT_SPEED => e.impact_speed as u32,
                sq::SFX_SURFACE_SOUND_TYPE => e.surf_type,
                sq::AUDIO_GET_RIDER_LISTENER => e.rider_listener,
                sq::SND_HANDLE_IS_DEAD => e.handle_dead,
                _ => 0,
            };
            cpu.set_gpr(2, rv as i32 as i64 as u64);
        });
    }
    // Float-arg stubs
    {
        let s = st.clone();
        r.stub(sq::SND_SYS_STOP_BANK_SOUND, move |cpu, _| {
            let mut e = s.borrow_mut();
            e.calls.push((
                sq::SND_SYS_STOP_BANK_SOUND,
                vec![cpu.gpr(4) as u32, cpu.gpr(5) as u32, cpu.gpr(6) as u32, cpu.fpr[12]],
            ));
            cpu.set_gpr(2, 0);
        });
    }
    {
        let s = st.clone();
        r.stub(sq::AUDIO_FIND_RIDER_NEAR, move |cpu, _| {
            let mut e = s.borrow_mut();
            e.calls.push((
                sq::AUDIO_FIND_RIDER_NEAR,
                vec![cpu.gpr(4) as u32, cpu.gpr(5) as u32, cpu.fpr[12]],
            ));
            let rv = e.find_near as u32;
            cpu.set_gpr(2, rv as i32 as i64 as u64);
        });
    }
    {
        let s = st.clone();
        r.stub(sq::AUDIO_ANY_RIDER_NEAR_AHEAD, move |cpu, _| {
            let mut e = s.borrow_mut();
            e.calls.push((
                sq::AUDIO_ANY_RIDER_NEAR_AHEAD,
                vec![cpu.gpr(4) as u32, cpu.gpr(5) as u32, cpu.fpr[12]],
            ));
            let rv = e.any_ahead;
            cpu.set_gpr(2, rv as i32 as i64 as u64);
        });
    }
    {
        let s = st.clone();
        r.stub(sq::SND_SYS_SET_PENDING_VOLUME, move |cpu, _| {
            let mut e = s.borrow_mut();
            e.calls.push((
                sq::SND_SYS_SET_PENDING_VOLUME,
                vec![cpu.gpr(4) as u32, cpu.fpr[12], cpu.fpr[13]],
            ));
            cpu.set_gpr(2, 0);
        });
    }
}

fn setup_audio_arrays(m: &mut Mem, r: &mut Runner, this: u32, arr: u32) {
    let pairs = [
        (0x24u32, arr),
        (0x38, arr + 0x200),
        (0x3c, arr + 0x280),
        (0x40, arr + 0x300),
        (0x64, arr + 0x380),
        (0x68, arr + 0x400),
        (0x6c, arr + 0x480),
        (0x70, arr + 0x500),
        (0x74, arr + 0x580),
        (0x78, arr + 0x780),
        (0x7c, arr + 0x800),
        (0xa40, arr + 0x880),
        (0xa44, arr + 0x900),
    ];
    for (off, p) in pairs {
        m.w32(this + off, p);
        r.mem.write_u32(this + off, p);
    }
}

#[test]
fn stop_queued_bump_takeoff_clipboard_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x610a_f001);

    // 1. Audio_StopQueuedSounds 0x215c28
    for case in 0..80 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let q = this + 0x6e24;
        let buf = BASE + 0x10000;
        let cap = 32u32;
        let read_idx = rng.next() % cap;
        let nrecs = (rng.next() % 6) as u32;
        let write_idx = (read_idx + nrecs * 4) % cap;
        for &(a, v) in &[(q, cap), (q + 4, buf), (q + 8, write_idx), (q + 0xc, read_idx)] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        let target_key = 0x100 + (rng.next() % 3);
        let mut cur = read_idx;
        for _ in 0..nrecs {
            let k = 0x100 + (rng.next() % 3);
            let h = rng.next() % 200;
            let exp = rng.next();
            let flag = rng.next() % 2;
            for w in [k, h, exp, flag] {
                m.w32(buf + cur * 4, w);
                r.mem.write_u32(buf + cur * 4, w);
                cur = (cur + 1) % cap;
            }
        }
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();
        sq::audio_stop_queued_sounds(&mut m, &mut st_rs, this, target_key);
        call_fn(&mut r, 0x215c28, &[this, target_key]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls, "stop_queued case {case}");
        assert_eq!(mem_slice(&m, q, 16), copy_out(&mut r, q, 16), "q hdr {case}");
        assert_eq!(mem_slice(&m, buf, (cap * 4) as usize), copy_out(&mut r, buf, (cap * 4) as usize), "q buf {case}");
    }

    // 2. Sfx_PlayRiderBump 0x215f18
    for case in 0..100 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let arr = BASE + 0x10000;
        let r1 = BASE + 0x12000;
        let r2 = BASE + 0x13000;
        setup_audio_arrays(&mut m, &mut r, this, arr);
        let idx = rng.next() % 3;
        m.w32(this + 0x20, idx);
        r.mem.write_u32(this + 0x20, idx);
        let tick = rng.next();
        m.w32(this + 0x80, tick);
        r.mem.write_u32(this + 0x80, tick);
        for k in 0..4u32 {
            let v1 = rng.f(-500.0, 500.0).to_bits();
            let v2 = rng.f(-500.0, 500.0).to_bits();
            m.w32(r1 + 0x150 + k * 4, v1);
            r.mem.write_u32(r1 + 0x150 + k * 4, v1);
            m.w32(r2 + 0x150 + k * 4, v2);
            r.mem.write_u32(r2 + 0x150 + k * 4, v2);
        }
        let init_st = EnvState {
            find_near: if case % 7 == 0 { -1 } else { 0 },
            any_ahead: if case % 5 == 0 { 0 } else { 1 },
            is_queued: if case % 6 == 0 { 1 } else { 0 },
            eval_curve: (rng.next() as i32 % 180) - 20,
            slot_ret: BASE + 0x3900,
            nearer_p2: rng.next() % 2,
            play_ret: 0x123 + case,
            ..Default::default()
        };
        let st_ps = Rc::new(RefCell::new(init_st.clone()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = init_st;

        sq::sfx_play_rider_bump(&mut m, &mut st_rs, this, r1, r2);
        call_fn(&mut r, 0x215f18, &[this, r1, r2]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls, "bump calls {case}");
        assert_eq!(mem_slice(&m, arr, 0x980), copy_out(&mut r, arr, 0x980), "bump arr {case}");
    }

    // 3. Sfx_PlayTakeoff 0x216d58, Sfx_PlayClipBoardSound51 0x21d680, Sfx_PlayClipBoardSound50 0x21d9b0
    for is_player in [0u32, 1] {
        for speed in [-500i32, 0, 250, 503, 504, 750, 999, 1000, 1500] {
            let mut m = fresh_mem(&mut r);
            let this = BASE + 0x100;
            let arr = BASE + 0x10000;
            let rider = BASE + 0x12000;
            setup_audio_arrays(&mut m, &mut r, this, arr);
            m.w32(rider + 0x41c, is_player);
            r.mem.write_u32(rider + 0x41c, is_player);
            let init_st = EnvState {
                impact_speed: speed,
                surf_type: rng.next() % 8,
                slot_ret: BASE + 0x3900,
                rider_listener: rng.next() % 2,
                play_ret: 0x55,
                ..Default::default()
            };
            let st_ps = Rc::new(RefCell::new(init_st.clone()));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = init_st;

            m.w32(this + 0x20, 0);
            r.mem.write_u32(this + 0x20, 0);
            sq::sfx_play_takeoff(&mut m, &mut st_rs, this, rider);
            call_fn(&mut r, 0x216d58, &[this, rider]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls, "takeoff p={is_player} spd={speed}");
            assert_eq!(mem_slice(&m, arr, 0x980), copy_out(&mut r, arr, 0x980));

            for &(id, addr) in &[(0x51u32, 0x21d680u32), (0x50, 0x21d9b0)] {
                m.w32(this + 0x20, 0);
                r.mem.write_u32(this + 0x20, 0);
                st_rs.calls.clear();
                st_ps.borrow_mut().calls.clear();
                if id == 0x51 {
                    sq::sfx_play_clipboard_sound_51(&mut m, &mut st_rs, this, rider);
                } else {
                    sq::sfx_play_clipboard_sound_50(&mut m, &mut st_rs, this, rider);
                }
                call_fn(&mut r, addr, &[this, rider]);
                assert_eq!(st_rs.calls, st_ps.borrow().calls, "clip {id:#x} p={is_player}");
                assert_eq!(mem_slice(&m, arr, 0x980), copy_out(&mut r, arr, 0x980));
            }
        }
    }
}

#[test]
fn music_meter_cues_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x610a_f002);

    // 4. Music_UpdateMeterCues 0x21c6d0 + Music_PlayMeterCue1 0x21ddc0 + Cue2 0x21dfe8 + Restart 0x21e248
    for case in 0..120 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let arr = BASE + 0x10000;
        let obj = BASE + 0x12000;
        let rider = BASE + 0x13000;
        setup_audio_arrays(&mut m, &mut r, this, arr);

        let tricky_on = rng.next() % 2;
        let obj_val = rng.f(-1.0, 2.0).to_bits();
        let has_rider = if case % 8 == 0 { 0 } else { rider };
        let f90 = rng.next() % 2;
        let f88 = rng.next() % 2;
        let f8c = rng.next() % 2;
        let prev_cnt = (rng.next() % 5) as u32;
        let cur_cnt = (rng.next() % 5) as u32;
        let rider_t = match case % 4 {
            0 => -0.5f32,
            1 => 0.0f32,
            2 => 2.69f32,
            _ => 3.5f32,
        }
        .to_bits();
        let old_handle = 0x700 + case;

        let writes = [
            (this + 0x20, 0),
            (this + 0x3708, tricky_on),
            (this + 0x6e7c, obj),
            (obj + 0x24, obj_val),
            (this + 0x6e80, has_rider),
            (this + 0x6e88, f88),
            (this + 0x6e8c, f8c),
            (this + 0x6e90, f90),
            (this + 0x6e94, prev_cnt),
            (this + 0x7458, old_handle),
            (rider + 0x24, rider_t),
            (rider + 0x58a8, cur_cnt),
        ];
        for (a, v) in writes {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }

        let init_st = EnvState {
            slot_ret: BASE + 0x3910,
            play_ret: 0x900 + case,
            handle_dead: rng.next() % 2,
            ..Default::default()
        };
        let st_ps = Rc::new(RefCell::new(init_st.clone()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = init_st;

        sq::music_update_meter_cues(&mut m, &mut st_rs, this);
        call_fn(&mut r, 0x21c6d0, &[this]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls, "meter_cues calls {case}");
        assert_eq!(mem_slice(&m, this + 0x6e7c, 0x20), copy_out(&mut r, this + 0x6e7c, 0x20), "meter_cues state {case}");
        assert_eq!(m.r32(this + 0x7458), r.mem.read_u32(this + 0x7458), "cue handle {case}");
        assert_eq!(mem_slice(&m, arr, 0x980), copy_out(&mut r, arr, 0x980), "meter_cues arr {case}");
    }
}
