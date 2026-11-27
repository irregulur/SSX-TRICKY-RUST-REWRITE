//! Function-runner check for row G10u (18 `audio` `cBXAudio` / `Audio_*` / `Sfx_*` functions):
//! `cBXAudio::~cBXAudio` 0x20ddb0, `cBXAudio::PlayFESound` 0x20eaf0, `Audio_PlayBigAirWind` 0x20f630,
//! `Audio_CancelMusicDuck` 0x20f8c0, `Audio_SetMusicMode` 0x20f998, `Audio_GetVolumeSlot` 0x211f50,
//! `Audio_FadeMusicTo` 0x211fb8, `Audio_UpdateMusicFade` 0x212018, `cBXAudio::vf1_212280` 0x212280,
//! `Audio_FreeRaceStream` 0x2151c8, `Sfx_InstanceControllerVf26` 0x2168f8, `Sfx_MeterFullEvent` 0x219258,
//! `Audio_ListBankNames` 0x21a498, `cBXAudio::vf2_21bc60` 0x21bc60, `Audio_LoadFEGlobalBankList` 0x21ce78,
//! `Audio_SetMusicState` 0x21d610, `Audio_ScheduleWorldTriggersOn` 0x21e498, `Audio_GetMusicFxThunk` 0x21e690.

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::sndqueue::{self as sq, SoundMgrEnv};

const BASE: u32 = 0x0140_0000;
const FAKE_CANCEL_VF: u32 = 0x002f_f200;
const FAKE_MGR_VF1: u32 = 0x002f_f210;
const FAKE_CTL_VF26: u32 = 0x002f_f220;

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
        0x20d9f0, // Audio_MusicStateToMode
        0x211fb8, // Audio_FadeMusicTo
        0x21e688, // Audio_GetMusicFx
        0x227a48, // cSoundSys::vf1_227a48
        0x2fb468, // memset
        0x2fd14c, // strcpy
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
        regions: vec![(BASE, vec![0u8; 0x20000])],
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
    slot_ret: u32,
    play_ret: u32,
    vf1_ret: u32,
    sched_ret: u32,
    ini_found: u32,
    ini_items: Vec<String>,
    ini_pos: usize,
}

impl SoundMgrEnv for EnvState {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        match target {
            sq::INI_OPEN => {
                self.calls.push((target, vec![args[1], args[2]]));
                0
            }
            sq::INI_FIND_SECTION => {
                self.calls.push((target, vec![args[1]]));
                self.ini_found
            }
            sq::INI_AT_SECTION_END => {
                self.calls.push((target, vec![]));
                u32::from(self.ini_pos >= self.ini_items.len())
            }
            sq::INI_READ_TOKEN => {
                self.calls.push((target, vec![]));
                0x100 + self.ini_pos as u32
            }
            sq::INI_READ_STRING => {
                self.calls.push((target, vec![args[1]]));
                let s = &self.ini_items[self.ini_pos];
                self.ini_pos += 1;
                let dst = args[2];
                for (k, &b) in s.as_bytes().iter().enumerate() {
                    m.w8(dst + k as u32, b);
                }
                m.w8(dst + s.len() as u32, 0);
                0
            }
            sq::INI_CLOSE => {
                self.calls.push((target, vec![args[1]]));
                0
            }
            sq::SPRINTF => {
                self.calls.push((target, args.to_vec()));
                let txt = format!("Unused bank {}", args[2] as i32);
                for (k, &b) in txt.as_bytes().iter().enumerate() {
                    m.w8(args[0] + k as u32, b);
                }
                m.w8(args[0] + txt.len() as u32, 0);
                txt.len() as u32
            }
            _ => {
                self.calls.push((target, args.to_vec()));
                match target {
                    sq::SND_MGR_GET_SLOT => self.slot_ret,
                    sq::SND_SYS_PLAY_PENDING => self.play_ret,
                    FAKE_MGR_VF1 => self.vf1_ret,
                    sq::SND_SYS_SCHEDULE_EVENT => self.sched_ret,
                    _ => 0,
                }
            }
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    let stubs: &[(u32, usize)] = &[
        (sq::MEM_DELETE, 1),
        (sq::RING_QUEUE_DESTROY, 2),
        (sq::SND_SYS_PLAY_PENDING, 1),
        (sq::SND_MGR_GET_SLOT, 2),
        (sq::SNOW_PROGRAM_DESTRUCT, 2),
        (sq::CONTROLLER_DTOR, 2),
        (sq::INI_OPEN, 3),
        (sq::INI_CLOSE, 2),
        (sq::INI_FIND_SECTION, 2),
        (sq::INI_READ_TOKEN, 1),
        (sq::INI_AT_SECTION_END, 1),
        (sq::INI_READ_STRING, 3),
        (sq::SPEECH_SYS_SET_FOLLOW_STREAM, 3),
        (sq::STREAM_SYS_DESTROY_STREAM, 2),
        (sq::SPEECH_FREE_NARRATION, 1),
        (sq::SPEECH_ON_METER_FULL, 2),
        (sq::SPEECH_FREE_CHAR_SCRIPT_SAMPLES, 1),
        (sq::EAMEM_FREE, 1),
        (sq::SPRINTF, 3),
        (FAKE_MGR_VF1, 2),
        (FAKE_CTL_VF26, 1),
    ];
    for &(addr, nargs) in stubs {
        let s = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut a = Vec::with_capacity(nargs);
            for k in 0..nargs {
                a.push(cpu.gpr(4 + k) as u32);
            }
            let mut e = s.borrow_mut();
            let ret = match addr {
                sq::INI_OPEN => {
                    e.calls.push((addr, vec![a[1], a[2]]));
                    0
                }
                sq::INI_FIND_SECTION => {
                    e.calls.push((addr, vec![a[1]]));
                    e.ini_found
                }
                sq::INI_AT_SECTION_END => {
                    e.calls.push((addr, vec![]));
                    u32::from(e.ini_pos >= e.ini_items.len())
                }
                sq::INI_READ_TOKEN => {
                    e.calls.push((addr, vec![]));
                    0x100 + e.ini_pos as u32
                }
                sq::INI_READ_STRING => {
                    e.calls.push((addr, vec![a[1]]));
                    let txt = e.ini_items[e.ini_pos].clone();
                    e.ini_pos += 1;
                    let mut bytes = txt.into_bytes();
                    bytes.push(0);
                    mem.write(a[2], &bytes);
                    0
                }
                sq::INI_CLOSE => {
                    e.calls.push((addr, vec![a[1]]));
                    0
                }
                sq::SPRINTF => {
                    e.calls.push((addr, a.clone()));
                    let txt = format!("Unused bank {}", a[2] as i32);
                    let len = txt.len() as u32;
                    let mut bytes = txt.into_bytes();
                    bytes.push(0);
                    mem.write(a[0], &bytes);
                    len
                }
                _ => {
                    e.calls.push((addr, a.clone()));
                    match addr {
                        sq::SND_MGR_GET_SLOT => e.slot_ret,
                        sq::SND_SYS_PLAY_PENDING => e.play_ret,
                        FAKE_MGR_VF1 => e.vf1_ret,
                        _ => 0,
                    }
                }
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    }
    // 64-bit arg stubs
    let sc = st.clone();
    r.stub(FAKE_CANCEL_VF, move |cpu, _| {
        let a0 = cpu.gpr(4) as u32;
        let a1 = cpu.gpr(5);
        sc.borrow_mut()
            .calls
            .push((FAKE_CANCEL_VF, vec![a0, a1 as u32, (a1 >> 32) as u32]));
        cpu.set_gpr(2, 0);
    });
    let ss = st.clone();
    r.stub(sq::SND_SYS_SCHEDULE_EVENT, move |cpu, _| {
        let a0 = cpu.gpr(4) as u32;
        let a1 = cpu.gpr(5) as u32;
        let a2 = cpu.gpr(6);
        let a3 = cpu.gpr(7) as u32;
        let t0 = cpu.gpr(8) as u32;
        let t1 = cpu.gpr(9) as u32;
        let t2 = cpu.gpr(10) as u32;
        let t3 = cpu.gpr(11) as u32;
        let mut e = ss.borrow_mut();
        e.calls.push((
            sq::SND_SYS_SCHEDULE_EVENT,
            vec![a0, a1, a2 as u32, (a2 >> 32) as u32, a3, t0, t1, t2, t3],
        ));
        let rv = e.sched_ret;
        cpu.set_gpr(2, rv as i32 as i64 as u64);
    });
}

#[test]
fn bxaudio_dtor_fe_wind_duck_fade_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x6100_0001);

    // 1. cBXAudio::~cBXAudio 0x20ddb0
    for flags in 0..4u32 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        for i in 0..10u32 {
            let b = this + 0x745c + i * 0x50;
            for off in [0u32, 0x320, 0x640] {
                let p = if (rng.next() & 1) != 0 { BASE + 0x10000 + i * 16 + off } else { 0 };
                m.w32(b + off, p);
                r.mem.write_u32(b + off, p);
            }
        }
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();
        sq::bx_audio_dtor(&mut m, &mut st_rs, this, flags);
        call_fn(&mut r, 0x20ddb0, &[this, flags]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, this, 0x7dc0), copy_out(&mut r, this, 0x7dc0));
    }

    // 2. cBXAudio::PlayFESound 0x20eaf0, Audio_PlayBigAirWind 0x20f630, Audio_CancelMusicDuck 0x20f8c0
    {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let arr = BASE + 0x9000;
        let vt = BASE + 0x8000;
        let setup = |w: &mut dyn FnMut(u32, u32)| {
            w(this + 4, vt);
            w(vt + 0x38, 4);
            w(vt + 0x3c, FAKE_CANCEL_VF);
            w(this + 0x24, arr);
            w(this + 0x38, arr + 0x200);
            w(this + 0x3c, arr + 0x280);
            w(this + 0x40, arr + 0x300);
            w(this + 0x64, arr + 0x380);
            w(this + 0x68, arr + 0x400);
            w(this + 0x6c, arr + 0x480);
            w(this + 0x70, arr + 0x500);
            w(this + 0x74, arr + 0x580);
            w(this + 0x78, arr + 0x780);
            w(this + 0x7c, arr + 0x800);
            w(this + 0xa40, arr + 0x880);
            w(this + 0xa44, arr + 0x900);
        };
        setup(&mut |a, v| m.w32(a, v));
        setup(&mut |a, v| r.mem.write_u32(a, v));

        let st_ps = Rc::new(RefCell::new(EnvState {
            slot_ret: BASE + 0x3900,
            play_ret: 0x42,
            ..Default::default()
        }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState {
            slot_ret: BASE + 0x3900,
            play_ret: 0x42,
            ..Default::default()
        };

        for case in 0..40 {
            let idx = (rng.next() % 3) as u32;
            m.w32(this + 0x20, idx);
            r.mem.write_u32(this + 0x20, idx);
            for k in 0..6u32 {
                let w = rng.next();
                m.w32(this + 8 + k * 4, w);
                r.mem.write_u32(this + 8 + k * 4, w);
            }
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            let kind = rng.next() % 10;
            let id = rng.next() % 50;
            assert_eq!(
                sq::bx_audio_play_fe_sound(&mut m, &mut st_rs, this, kind, id),
                call_fn(&mut r, 0x20eaf0, &[this, kind, id])
            );
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(mem_slice(&m, arr, 0x980), copy_out(&mut r, arr, 0x980));

            // Audio_PlayBigAirWind (both +0x38a4 == 0 and != 0)
            let wind_on = (case & 1) as u32;
            m.w32(this + 0x20, idx);
            r.mem.write_u32(this + 0x20, idx);
            m.w32(this + 0x38a4, wind_on);
            r.mem.write_u32(this + 0x38a4, wind_on);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            sq::audio_play_big_air_wind(&mut m, &mut st_rs, this);
            call_fn(&mut r, 0x20f630, &[this]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(this + 0x38a4), r.mem.read_u32(this + 0x38a4));
            assert_eq!(mem_slice(&m, arr, 0x980), copy_out(&mut r, arr, 0x980));

            // Audio_CancelMusicDuck
            m.w32(this + 0x38a0, 0x1234);
            r.mem.write_u32(this + 0x38a0, 0x1234);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            sq::audio_cancel_music_duck(&mut m, &mut st_rs, this);
            call_fn(&mut r, 0x20f8c0, &[this]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(this + 0x38a0), r.mem.read_u32(this + 0x38a0));
            assert_eq!(m.r32(this + 0x38a4), r.mem.read_u32(this + 0x38a4));
        }
    }

    // 3. Audio_SetMusicMode 0x20f998, Audio_GetVolumeSlot 0x211f50, Audio_FadeMusicTo 0x211fb8,
    //    Audio_UpdateMusicFade 0x212018, Audio_SetMusicState 0x21d610
    {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        for mode in [0u32, 1, 1, 2, 0] {
            sq::audio_set_music_mode(&mut m, this, mode);
            call_fn(&mut r, 0x20f998, &[this, mode]);
            assert_eq!(m.r32(this + 0x6e34), r.mem.read_u32(this + 0x6e34));
        }
        for idx in 0..20u32 {
            assert_eq!(sq::audio_get_volume_slot(this, idx), call_fn(&mut r, 0x211f50, &[this, idx]));
        }

        for mode in 0..6u32 {
            for dir in [false, true] {
                let start = if dir { 0.2f32 } else { 1.0f32 };
                let target = if dir { 1.0f32 } else { 0.1f32 };
                let secs = if mode == 5 { 0.0f32 } else { 0.1f32 };
                m.w32(this + 0x38d4, start.to_bits());
                r.mem.write_u32(this + 0x38d4, start.to_bits());
                for off in [0x3858u32, 0x3864, 0x3870, 0x387c, 0x3888, 0x3894] {
                    let v = rng.f(0.1, 1.0).to_bits();
                    m.w32(this + off, v);
                    r.mem.write_u32(this + off, v);
                }
                sq::audio_fade_music_to(&mut m, this, mode, target, secs);
                r.cpu.set_gpr(4, this as i32 as i64 as u64);
                r.cpu.set_gpr(5, mode as i32 as i64 as u64);
                r.cpu.fpr[12] = target.to_bits();
                r.cpu.fpr[13] = secs.to_bits();
                r.call(0x211fb8).unwrap();
                assert_eq!(mem_slice(&m, this + 0x3850, 0x90), copy_out(&mut r, this + 0x3850, 0x90));

                for _ in 0..10 {
                    sq::audio_update_music_fade(&mut m, this);
                    call_fn(&mut r, 0x212018, &[this]);
                    assert_eq!(mem_slice(&m, this + 0x3850, 0x90), copy_out(&mut r, this + 0x3850, 0x90));
                }
            }
        }

        // Audio_SetMusicState 0x21d610
        for prev_state in 0..6u32 {
            for flag in [0u32, 1] {
                for state in [0u32, 2, 4] {
                    let param = rng.f(0.0, 10.0);
                    m.w32(this + 0x743c, prev_state);
                    r.mem.write_u32(this + 0x743c, prev_state);
                    m.w32(this + 0x7444, flag);
                    r.mem.write_u32(this + 0x7444, flag);
                    sq::audio_set_music_state(&mut m, this, state, param);
                    r.cpu.set_gpr(4, this as i32 as i64 as u64);
                    r.cpu.set_gpr(5, state as i32 as i64 as u64);
                    r.cpu.fpr[12] = param.to_bits();
                    r.call(0x21d610).unwrap();
                    assert_eq!(mem_slice(&m, this + 0x38c0, 0x18), copy_out(&mut r, this + 0x38c0, 0x18));
                    assert_eq!(mem_slice(&m, this + 0x743c, 0x10), copy_out(&mut r, this + 0x743c, 0x10));
                }
            }
        }
    }
}

#[test]
fn bxaudio_misc_and_bank_helpers_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x6100_0002);

    // 4. cBXAudio::vf1_212280, Audio_FreeRaceStream 0x2151c8, Sfx_InstanceControllerVf26 0x2168f8,
    //    Sfx_MeterFullEvent 0x219258, cBXAudio::vf2_21bc60, Audio_ScheduleWorldTriggersOn 0x21e498,
    //    Audio_GetMusicFxThunk 0x21e690
    {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let mgr = BASE + 0x8000;
        let vt = BASE + 0x8100;
        let set_w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        set_w(&mut m, &mut r, this, mgr);
        set_w(&mut m, &mut r, mgr, vt);
        set_w(&mut m, &mut r, vt + 0x10, 0);
        set_w(&mut m, &mut r, vt + 0x14, FAKE_MGR_VF1);

        for rv in [1u32, 2] {
            let st_ps = Rc::new(RefCell::new(EnvState { vf1_ret: rv, sched_ret: 77, ..Default::default() }));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState { vf1_ret: rv, sched_ret: 77, ..Default::default() };
            set_w(&mut m, &mut r, this + 0x3838, 99);
            assert_eq!(sq::bx_audio_vf1(&mut m, &mut st_rs, this, 3), call_fn(&mut r, 0x212280, &[this, 3]));
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(this + 0x3838), r.mem.read_u32(this + 0x3838));
        }

        for p in [0u32, BASE + 0x9000] {
            let st_ps = Rc::new(RefCell::new(EnvState::default()));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState::default();
            set_w(&mut m, &mut r, this + 0x38d8, p);
            set_w(&mut m, &mut r, this + 0x7448, 5);
            sq::audio_free_race_stream(&mut m, &mut st_rs, this);
            call_fn(&mut r, 0x2151c8, &[this]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(this + 0x38d8), r.mem.read_u32(this + 0x38d8));
            assert_eq!(m.r32(this + 0x7448), r.mem.read_u32(this + 0x7448));
        }

        // Sfx_InstanceControllerVf26 0x2168f8
        let inst = BASE + 0x8200;
        let ctl = BASE + 0x8400;
        let ctl_vt = BASE + 0x8500;
        set_w(&mut m, &mut r, ctl + 8, ctl_vt);
        set_w(&mut m, &mut r, ctl_vt + 0xd8, 8);
        set_w(&mut m, &mut r, ctl_vt + 0xdc, FAKE_CTL_VF26);
        for cptr in [0u32, ctl] {
            let st_ps = Rc::new(RefCell::new(EnvState::default()));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState::default();
            set_w(&mut m, &mut r, inst + 0xe4, cptr);
            sq::sfx_instance_controller_vf26(&mut m, &mut st_rs, inst);
            call_fn(&mut r, 0x2168f8, &[this, inst]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
        }

        // Sfx_MeterFullEvent 0x219258
        let rider = BASE + 0x8600;
        let score = BASE + 0x8700;
        set_w(&mut m, &mut r, rider + 0x18, score);
        for flag in [0u32, 1] {
            for val in [0.5f32, 0.875, 0.875001, 1.0] {
                let st_ps = Rc::new(RefCell::new(EnvState::default()));
                install_stubs(&mut r, st_ps.clone());
                let mut st_rs = EnvState::default();
                set_w(&mut m, &mut r, this + 0x6e3c, flag);
                set_w(&mut m, &mut r, score + 0x1c, val.to_bits());
                assert_eq!(
                    sq::sfx_meter_full_event(&mut m, &mut st_rs, this, rider),
                    call_fn(&mut r, 0x219258, &[this, rider])
                );
                assert_eq!(st_rs.calls, st_ps.borrow().calls);
            }
        }

        // cBXAudio::vf2_21bc60
        sq::bx_audio_vf2();
        call_fn(&mut r, 0x21bc60, &[this]);

        // Audio_ScheduleWorldTriggersOn 0x21e498
        {
            let st_ps = Rc::new(RefCell::new(EnvState { sched_ret: 0x99, ..Default::default() }));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState { sched_ret: 0x99, ..Default::default() };
            assert_eq!(
                sq::audio_schedule_world_triggers_on(&mut m, &mut st_rs, this),
                call_fn(&mut r, 0x21e498, &[this])
            );
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
        }

        // Audio_GetMusicFxThunk 0x21e690
        let fx = rng.next();
        set_w(&mut m, &mut r, this + 0x3844, fx);
        assert_eq!(sq::audio_get_music_fx_thunk(&m, this), call_fn(&mut r, 0x21e690, &[this]));
    }

    // 5. Audio_ListBankNames 0x21a498
    for count in [0u32, 3] {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let out = BASE + 0x9000;
        m.w32(this + 0x38dc, count);
        r.mem.write_u32(this + 0x38dc, count);
        for i in 0..count {
            let name = format!("BANK_{i}\0");
            let dst = this + 0x38e0 + i * 0x40;
            for (k, &b) in name.as_bytes().iter().enumerate() {
                m.w8(dst + k as u32, b);
            }
            r.mem.write(dst, name.as_bytes());
        }
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();
        sq::audio_list_bank_names(&mut m, &mut st_rs, this, out);
        call_fn(&mut r, 0x21a498, &[this, out]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, out, 200 * 0x40), copy_out(&mut r, out, 200 * 0x40));
    }

    // 6. Audio_LoadFEGlobalBankList 0x21ce78
    for found in [0u32, 1] {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let path = BASE + 0x40;
        let sp = BASE + 0x9000;
        let items = vec!["FE_GLOBAL".to_string(), "FE_UI".to_string(), "FE_MUSIC".to_string()];
        let st_ps = Rc::new(RefCell::new(EnvState {
            ini_found: found,
            ini_items: items.clone(),
            ..Default::default()
        }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState {
            ini_found: found,
            ini_items: items,
            ..Default::default()
        };
        sq::audio_load_fe_global_bank_list(&mut m, &mut st_rs, this, path, sp);
        call_fn(&mut r, 0x21ce78, &[this, path]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, this + 0x6e98, 0x120), copy_out(&mut r, this + 0x6e98, 0x120));
    }
}
