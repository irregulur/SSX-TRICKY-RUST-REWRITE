//! Function-runner check for row G10q (30 `audio` stream & speech functions):
//! - `speechsys`: `cSpeechSys::~cSpeechSys` 0x2287d8, `SpeechSys_SetFollowStream` 0x228b88,
//!   14 `Stream_*` (`0x228fb0..0x2297f0`), 11 `StreamSys_*` (`0x229890..0x22a2b8`),
//!   `SndHandle_IsPlaying` 0x22a120, `SndEvent_ClearStreamPrebuffer` 0x22a2d0
//! - `speechlib`: `SpeechLib_RegisterEventBank0` 0x2bada0

use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::speechlib::{self, Mem as LibMem};
use tricky_game::speechsys::{self, SysEnv};

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
            (0x0034_0000, vec![0u8; 0x0001_0000]),
            (BASE, vec![0u8; 0x20000]),
            (0x01fe_0000, vec![0u8; 0x10000]),
        ],
    }
}

fn mem_slice(m: &Mem, at: u32, len: usize) -> Vec<u8> {
    (0..len).map(|i| m.r8(at + i as u32)).collect()
}

struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
    alloc_ret: u32,
    pool_ret: u32,
    calc_size: u32,
    open_ret: u32,
    file_exists: u32,
    open_big_ret: u32,
    status_w0: u32,
    status_w1: u32,
    status_ret: u32,
    state_w0: u32,
    state_w1: u32,
    state_ret: u32,
    big_find_off: u32,
    queue_ret: u32,
}
impl Default for EnvState {
    fn default() -> Self {
        Self {
            calls: vec![],
            alloc_ret: 0,
            pool_ret: 0,
            calc_size: 0x400,
            open_ret: 7,
            file_exists: 1,
            open_big_ret: 0x0155_0000,
            status_w0: 0,
            status_w1: 0,
            status_ret: 0,
            state_w0: 0,
            state_w1: 0,
            state_ret: 0,
            big_find_off: 0,
            queue_ret: 0,
        }
    }
}
impl SysEnv for EnvState {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        self.calls.push((target, args.to_vec()));
        match target {
            speechsys::ALLOC => self.alloc_ret,
            speechsys::MEMPOOL_ALLOC => self.pool_ret,
            speechsys::SND_STREAM_CALC_MEM_SIZE => self.calc_size,
            speechsys::SND_STREAM_OPEN => self.open_ret,
            speechsys::FILE_EXISTS => self.file_exists,
            speechsys::FILE_OPEN_BIG => self.open_big_ret,
            speechsys::SND_STREAM_GET_STATUS => {
                m.w32(args[1], self.status_w0);
                m.w32(args[1] + 4, self.status_w1);
                self.status_ret
            }
            speechsys::SND_HANDLE_GET_STATE => {
                m.w32(args[1], self.state_w0);
                m.w32(args[1] + 4, self.state_w1);
                self.state_ret
            }
            speechsys::BIG_FIND => {
                m.w32(args[3], self.big_find_off);
                1
            }
            speechsys::SND_STREAM_QUEUE_FILE => self.queue_ret,
            _ => 0,
        }
    }
    fn call_f(&mut self, _m: &mut Mem, target: u32, args: &[u32]) -> f32 {
        self.calls.push((target, args.to_vec()));
        0.0
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    for &a in &[
        0x228fb0u32, // Stream_Init
        0x2293e0,    // Stream_UpdateVolume
        0x229490,    // Stream_GetStatus
        0x2294d8,    // Stream_GetPlayInfo
        0x2295f8,    // Stream_IsQueued
        0x2296b0,    // Stream_ClearRequestPrebuffer
        0x229710,    // Stream_Resume
        0x229740,    // Stream_Pause
        0x229770,    // Stream_PlayFile
        0x2297f0,    // Stream_OpenInBig
    ] {
        r.allow(a);
    }
    for &(addr, nargs) in &[
        (speechsys::MEM_FREE, 1usize),
        (speechsys::MIDI_SYS_INIT, 3),
        (speechsys::MUSIC_DTOR, 2),
        (speechsys::SLOT_SET_FOLLOW, 2),
        (speechsys::STREAM_SET_VOLUME, 2),
        (speechsys::STREAM_STOP_OBJ, 1),
        (speechsys::MEMSET, 3),
        (speechsys::STRCPY, 2),
        (speechsys::SND_LOCK, 0),
        (speechsys::SND_UNLOCK, 0),
        (speechsys::SND_STREAM_SET_PREBUFFER_MS, 2),
        (speechsys::SND_STREAM_SET_VOLUME, 2),
        (speechsys::SND_STREAM_SET_PITCH, 2),
    ] {
        let st = st.clone();
        r.stub(addr, move |c, _| {
            let args: Vec<u32> = (0..nargs).map(|k| c.gpr(4 + k) as u32).collect();
            st.borrow_mut().calls.push((addr, args));
            c.set_gpr(2, 0);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::ALLOC, move |c, _| {
            let args = vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32];
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::ALLOC, args));
            c.set_gpr(2, s.alloc_ret as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::MEMPOOL_ALLOC, move |c, _| {
            let args: Vec<u32> = (0..6).map(|k| c.gpr(4 + k) as u32).collect();
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::MEMPOOL_ALLOC, args));
            c.set_gpr(2, s.pool_ret as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::SND_STREAM_CALC_MEM_SIZE, move |c, _| {
            let args = vec![c.gpr(4) as u32, c.gpr(5) as u32];
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::SND_STREAM_CALC_MEM_SIZE, args));
            c.set_gpr(2, s.calc_size as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::SND_STREAM_OPEN, move |c, _| {
            let args: Vec<u32> = (0..5).map(|k| c.gpr(4 + k) as u32).collect();
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::SND_STREAM_OPEN, args));
            c.set_gpr(2, s.open_ret as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::FILE_EXISTS, move |c, _| {
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::FILE_EXISTS, vec![c.gpr(4) as u32]));
            c.set_gpr(2, s.file_exists as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::FILE_OPEN_BIG, move |c, _| {
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::FILE_OPEN_BIG, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, s.open_big_ret as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::SND_STREAM_GET_STATUS, move |c, mem| {
            let a0 = c.gpr(4) as u32;
            let a1 = c.gpr(5) as u32;
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::SND_STREAM_GET_STATUS, vec![a0, a1]));
            mem.write_u32(a1, s.status_w0);
            mem.write_u32(a1 + 4, s.status_w1);
            c.set_gpr(2, s.status_ret as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::SND_HANDLE_GET_STATE, move |c, mem| {
            let a0 = c.gpr(4) as u32;
            let a1 = c.gpr(5) as u32;
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::SND_HANDLE_GET_STATE, vec![a0, a1]));
            mem.write_u32(a1, s.state_w0);
            mem.write_u32(a1 + 4, s.state_w1);
            c.set_gpr(2, s.state_ret as u64);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::BIG_FIND, move |c, mem| {
            let args: Vec<u32> = (0..5).map(|k| c.gpr(4 + k) as u32).collect();
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::BIG_FIND, args.clone()));
            mem.write_u32(args[3], s.big_find_off);
            c.set_gpr(2, 1);
        });
    }
    {
        let st = st.clone();
        r.stub(speechsys::SND_STREAM_QUEUE_FILE, move |c, _| {
            let args: Vec<u32> = (0..4).map(|k| c.gpr(4 + k) as u32).collect();
            let mut s = st.borrow_mut();
            s.calls.push((speechsys::SND_STREAM_QUEUE_FILE, args));
            c.set_gpr(2, s.queue_ret as u64);
        });
    }
}

#[test]
fn speech_sys_and_stream_methods() {
    let Some(mut r) = runner() else { return };
    let st = Rc::new(RefCell::new(EnvState::default()));
    install_stubs(&mut r, st.clone());
    let mut rng = Rng(0x2287_d801);

    let sys = BASE;
    let slots_ptr = BASE + 0x4000;
    let stream_obj = BASE + 0x4200;
    let rec_base = BASE + 0x4400;
    let t3c = BASE + 0x4800;
    let t40 = BASE + 0x4900;
    let scale_addr = BASE + 0x4a00;
    let out_buf = BASE + 0x4b00;
    let path_addr = BASE + 0x4c00;
    let arr_addr = BASE + 0x4d00;

    // 1. cSpeechSys::~cSpeechSys 0x2287d8 & SpeechSys_SetFollowStream 0x228b88
    for case in 0..200 {
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        w(&mut m, &mut r, speechsys::SYS, 0x1234_5678);
        w(&mut m, &mut r, sys + 4, 0xdead_0000);
        w(&mut m, &mut r, sys + 0x3718, if case % 3 == 0 { 0 } else { slots_ptr });
        st.borrow_mut().calls.clear();
        let flags = (case % 4) as u32;
        call_fn(&mut r, 0x2287d8, &[sys, flags]);
        let mut env = EnvState::default();
        speechsys::speech_sys_destruct(&mut m, &mut env, sys, flags);
        assert_eq!(env.calls, st.borrow().calls, "speech dtor calls {case}");
        assert_eq!(m.r32(sys + 4), r.mem.read_u32(sys + 4));
        assert_eq!(m.r32(speechsys::SYS), r.mem.read_u32(speechsys::SYS));

        w(&mut m, &mut r, sys + 0x3718, slots_ptr);
        let slot = rng.next() % 4;
        w(&mut m, &mut r, slots_ptr + 4 * slot, if case & 1 == 0 { 0 } else { 0x0152_0000 + slot * 0x10 });
        let val = rng.next();
        st.borrow_mut().calls.clear();
        call_fn(&mut r, 0x228b88, &[sys, slot, val]);
        let mut env = EnvState::default();
        speechsys::speech_sys_set_follow(&mut m, &mut env, sys, slot, val);
        assert_eq!(env.calls, st.borrow().calls, "set follow calls {case}");
    }

    // 2. Stream_Init 0x228fb0 (covers all flag bits, null/non-null scale, negative/clamp/normal volumes, alloc null/non-null)
    for case in 0..400 {
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        let idx = rng.next() % 8;
        w(&mut m, &mut r, sys + 0x20, idx);
        w(&mut m, &mut r, sys + 0x24, rec_base);
        w(&mut m, &mut r, sys + 0x3c, t3c);
        w(&mut m, &mut r, sys + 0x40, t40);
        let vol = ((rng.next() % 300) as i32 - 50) as u32;
        w(&mut m, &mut r, t3c + 4 * idx, vol);
        let has_scale = case % 3 != 0;
        w(&mut m, &mut r, t40 + 4 * idx, if has_scale { scale_addr } else { 0 });
        w(&mut m, &mut r, scale_addr, rng.f(-0.5, 2.5).to_bits());
        let rec = rec_base + idx * 0x18;
        m.w8(rec, 0x55);
        r.mem.write(rec, &[0x55]).unwrap();

        let flags = (if case % 2 == 0 { 0x4000_0000 } else { 0 })
            | (if case % 4 < 2 { 0x8000_0000 } else { 0 })
            | (rng.next() & 0x03ff_ffff);
        let extra = rng.next() & 0xffff;
        let alloc_r = if case % 5 == 0 { 0 } else { 0x0160_0000 };
        let pool_r = if case % 7 == 0 { 0 } else { 0x0161_0000 };
        {
            let mut s = st.borrow_mut();
            s.calls.clear();
            s.alloc_ret = alloc_r;
            s.pool_ret = pool_r;
            s.calc_size = rng.next() & 0xfff;
            s.open_ret = rng.next();
        }
        let rv_o = call_fn(&mut r, 0x228fb0, &[stream_obj, sys, flags, extra]);
        let mut env = EnvState {
            alloc_ret: alloc_r,
            pool_ret: pool_r,
            calc_size: st.borrow().calc_size,
            open_ret: st.borrow().open_ret,
            ..Default::default()
        };
        let rv_p = speechsys::stream_init(&mut m, &mut env, stream_obj, sys, flags, extra);
        assert_eq!(rv_p, rv_o, "stream_init rv {case}");
        assert_eq!(env.calls, st.borrow().calls, "stream_init calls {case}");
        assert_eq!(mem_slice(&m, stream_obj, 0x80), copy_out(&mut r, stream_obj, 0x80), "stream_init obj {case}");
        assert_eq!(m.r32(t3c + 4 * idx), r.mem.read_u32(t3c + 4 * idx), "stream_init vol {case}");
        let rec = rec_base + idx * 0x18;
        assert_eq!(m.r8(rec), copy_out(&mut r, rec, 1)[0], "stream_init rec {case}");
    }

    // 3. Stream_Open, UpdateVolume, GetStatus, GetPlayInfo, GetPlayedMs, IsQueued, ClearPrebuffer, GetHandle,
    //    ClearRequestPrebuffer, Resume, Pause, PlayFile, OpenInBig
    for case in 0..300 {
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        w(&mut m, &mut r, stream_obj + 4, rng.next());
        let b8 = (rng.next() & 0xff) as u8;
        m.w8(stream_obj + 8, b8);
        r.mem.write(stream_obj + 8, &[b8]).unwrap();
        w(&mut m, &mut r, stream_obj + 0xc, if case % 4 == 0 { 0 } else { scale_addr });
        w(&mut m, &mut r, scale_addr, rng.f(-1.0, 3.0).to_bits());
        w(&mut m, &mut r, stream_obj + 0x70, if case % 3 == 0 { 0 } else { 0x0170_0000 });
        w(&mut m, &mut r, stream_obj + 0x78, rng.next());
        w(&mut m, &mut r, stream_obj + 0x7c, match case % 5 { 0 => 0x78, 1 => 0x79, 2 => 0x80, _ => rng.next() % 200 });

        let fe = (case & 1) as u32;
        let ob = rng.next();
        let sw0 = (case % 3) as u32;
        let sw1 = rng.next();
        let sr = rng.next();
        let stw0 = (case % 4) as u32;
        let stw1 = rng.next();
        let str_r = ((rng.next() % 5) as i32 - 2) as u32;
        let bfo = rng.next() & 0xffff;
        let qr = rng.next();
        let mk = || EnvState {
            file_exists: fe,
            open_big_ret: ob,
            status_w0: sw0,
            status_w1: sw1,
            status_ret: sr,
            state_w0: stw0,
            state_w1: stw1,
            state_ret: str_r,
            big_find_off: bfo,
            queue_ret: qr,
            ..Default::default()
        };
        let sync = |st: &Rc<RefCell<EnvState>>| {
            *st.borrow_mut() = mk();
        };

        // Stream_Open 0x2292e8
        sync(&st);
        call_fn(&mut r, 0x2292e8, &[stream_obj, path_addr]);
        let mut env = mk();
        speechsys::stream_open(&mut m, &mut env, stream_obj, path_addr);
        assert_eq!(env.calls, st.borrow().calls, "open calls {case}");
        assert_eq!(m.r32(stream_obj + 0x70), r.mem.read_u32(stream_obj + 0x70));

        // Stream_UpdateVolume 0x2293e0
        sync(&st);
        call_fn(&mut r, 0x2293e0, &[stream_obj]);
        let mut env = mk();
        speechsys::stream_update_volume(&mut m, &mut env, stream_obj);
        assert_eq!(env.calls, st.borrow().calls, "updvol calls {case}");
        assert_eq!(m.r32(stream_obj + 0x7c), r.mem.read_u32(stream_obj + 0x7c));

        // Stream_GetStatus 0x229490
        sync(&st);
        let rv_o = call_fn(&mut r, 0x229490, &[stream_obj, out_buf]);
        let mut env = mk();
        let rv_p = speechsys::stream_get_status(&mut m, &mut env, stream_obj, out_buf);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "getstatus {case}");

        // Stream_GetPlayInfo 0x2294d8
        let sout = if case & 1 == 0 { 0 } else { out_buf + 0x20 };
        sync(&st);
        let rv_o = call_fn(&mut r, 0x2294d8, &[stream_obj, out_buf, sout]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_get_play_info(&mut m, &mut env, stream_obj, out_buf, sout, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "getplayinfo {case}");

        // Stream_GetPlayedMs 0x229530
        sync(&st);
        call_fn(&mut r, 0x229530, &[stream_obj]);
        let f_o = r.cpu.fpr[0];
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let f_p = speechsys::stream_get_played_ms(&mut m, &mut env, stream_obj, sp).to_bits();
        assert_eq!((f_p, env.calls), (f_o, st.borrow().calls.clone()), "played_ms {case}");

        // Stream_IsQueued 0x2295f8
        sync(&st);
        let rv_o = call_fn(&mut r, 0x2295f8, &[stream_obj]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_is_queued(&mut m, &mut env, stream_obj, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "isqueued {case}");

        // Stream_ClearPrebuffer 0x229658
        sync(&st);
        call_fn(&mut r, 0x229658, &[stream_obj]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        speechsys::stream_clear_prebuffer(&mut m, &mut env, stream_obj, sp);
        assert_eq!(env.calls, st.borrow().calls, "clrprebuf {case}");

        // Stream_GetHandle 0x229680
        sync(&st);
        let rv_o = call_fn(&mut r, 0x229680, &[stream_obj]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_get_handle(&mut m, &mut env, stream_obj, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "gethandle {case}");

        // Stream_ClearRequestPrebuffer 0x2296b0
        let req = rng.next();
        sync(&st);
        call_fn(&mut r, 0x2296b0, &[stream_obj, req]);
        let mut env = mk();
        speechsys::stream_clear_request_prebuffer(&mut m, &mut env, req);
        assert_eq!(env.calls, st.borrow().calls, "clrreq {case}");

        // Stream_Resume 0x229710 & Stream_Pause 0x229740
        sync(&st);
        call_fn(&mut r, 0x229710, &[stream_obj]);
        let mut env = mk();
        speechsys::stream_resume(&mut m, &mut env, stream_obj);
        assert_eq!(env.calls, st.borrow().calls, "resume {case}");

        sync(&st);
        call_fn(&mut r, 0x229740, &[stream_obj]);
        let mut env = mk();
        speechsys::stream_pause(&mut m, &mut env, stream_obj);
        assert_eq!(env.calls, st.borrow().calls, "pause {case}");

        // Stream_PlayFile 0x229770 & Stream_OpenInBig 0x2297f0
        let a1 = rng.next();
        let off = rng.next() & 0xffff;
        sync(&st);
        let rv_o = call_fn(&mut r, 0x229770, &[stream_obj, a1, path_addr, off]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_play_file(&mut m, &mut env, stream_obj, a1, path_addr, off, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "playfile {case}");

        sync(&st);
        let rv_o = call_fn(&mut r, 0x2297f0, &[stream_obj, a1, 5, off]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_open_in_big(&mut m, &mut env, stream_obj, a1, 5, off, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "openinbig {case}");
    }

    // 4. StreamSys_Init 0x229890, StreamSys_CreateStream 0x2299f8, StopAll/ResumeAll/PauseAll, PlayFile/Play,
    //    IsSlotQueued, SndHandle_IsPlaying, ClearRequestPrebuffer, SetVolume, GetStream, SndEvent_ClearStreamPrebuffer
    for case in 0..200 {
        let mut m = fresh_mem();
        let w = |m: &mut Mem, r: &mut Runner, a: u32, v: u32| {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        };
        let count = ((case % 5) as i32 - 1) as u32; // -1..=3
        let alloc_r = if count == 0 || (count as i32) < 0 { 0 } else { arr_addr };
        {
            let mut s = st.borrow_mut();
            *s = EnvState { alloc_ret: alloc_r, ..Default::default() };
        }
        let rv_o = call_fn(&mut r, 0x229890, &[sys, 0x11, count, 0x22]);
        let mut env = EnvState { alloc_ret: alloc_r, ..Default::default() };
        let rv_p = speechsys::stream_sys_init(&mut m, &mut env, sys, 0x11, count, 0x22);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "sys_init {case}");
        assert_eq!(m.r32(sys + 4), r.mem.read_u32(sys + 4));
        assert_eq!(m.r32(sys + 0xa4c), r.mem.read_u32(sys + 0xa4c));
        assert_eq!(m.r32(sys + 0xa50), r.mem.read_u32(sys + 0xa50));
        if (count as i32) > 0 {
            assert_eq!(mem_slice(&m, arr_addr, (count * 4) as usize), copy_out(&mut r, arr_addr, (count * 4) as usize));
        }

        // StreamSys_CreateStream 0x2299f8
        w(&mut m, &mut r, sys + 0xa50, arr_addr);
        w(&mut m, &mut r, sys + 0x20, 0);
        w(&mut m, &mut r, sys + 0x24, rec_base);
        w(&mut m, &mut r, sys + 0x3c, t3c);
        w(&mut m, &mut r, sys + 0x40, t40);
        w(&mut m, &mut r, t3c, 80);
        w(&mut m, &mut r, t40, 0);
        let flags = (if case % 2 == 0 { 0x4000_0000 } else { 0 })
            | (if case % 4 < 2 { 0x8000_0000 } else { 0 })
            | (rng.next() & 0x03ff_ffff);
        let slot = case as u32 % 4;
        {
            let mut s = st.borrow_mut();
            *s = EnvState { alloc_ret: stream_obj, pool_ret: stream_obj, calc_size: 0x200, open_ret: 9, ..Default::default() };
        }
        call_fn(&mut r, 0x2299f8, &[sys, slot, flags, 0x100]);
        let mut env = EnvState { alloc_ret: stream_obj, pool_ret: stream_obj, calc_size: 0x200, open_ret: 9, ..Default::default() };
        speechsys::stream_sys_create_stream(&mut m, &mut env, sys, slot, flags, 0x100);
        assert_eq!(env.calls, st.borrow().calls, "create_stream calls {case}");
        assert_eq!(m.r32(arr_addr + 4 * slot), r.mem.read_u32(arr_addr + 4 * slot));

        // Populate 3 slots (mix of null & valid streams) for StopAll/ResumeAll/PauseAll/PlayFile/Play/IsSlotQueued/Clear/SetVol/Get
        w(&mut m, &mut r, sys + 0xa4c, 3);
        w(&mut m, &mut r, sys + 0xa50, arr_addr);
        for k in 0..3u32 {
            let sp_k = if (case + k as usize) % 2 == 0 { 0 } else { stream_obj + k * 0x80 };
            w(&mut m, &mut r, arr_addr + 4 * k, sp_k);
            if sp_k != 0 {
                w(&mut m, &mut r, sp_k + 4, 10 + k);
                m.w8(sp_k + 8, 100);
                r.mem.write(sp_k + 8, &[100]).unwrap();
                w(&mut m, &mut r, sp_k + 0xc, scale_addr);
                w(&mut m, &mut r, scale_addr, 0.75f32.to_bits());
                w(&mut m, &mut r, sp_k + 0x70, if k == 1 { 0x0170_0000 } else { 0 });
                w(&mut m, &mut r, sp_k + 0x7c, if k == 2 { 0x80 } else { 0 });
            }
        }
        let mk = || EnvState {
            status_w0: 1,
            status_w1: 0x55,
            state_w0: (case % 3) as u32,
            state_w1: 1234,
            state_ret: ((case % 3) as i32 - 1) as u32,
            big_find_off: 0x300,
            queue_ret: 42,
            ..Default::default()
        };

        // StopAll 0x229d98, ResumeAll 0x229e00, PauseAll 0x229e68
        for &(addr, which) in &[(0x229d98u32, 0), (0x229e00, 1), (0x229e68, 2)] {
            *st.borrow_mut() = mk();
            call_fn(&mut r, addr, &[sys]);
            let mut env = mk();
            match which {
                0 => speechsys::stream_sys_stop_all(&mut m, &mut env, sys),
                1 => speechsys::stream_sys_resume_all(&mut m, &mut env, sys),
                _ => speechsys::stream_sys_pause_all(&mut m, &mut env, sys),
            }
            assert_eq!(env.calls, st.borrow().calls, "all {addr:#x} case {case}");
        }

        let sl = (case as u32) % 3;
        // StreamSys_PlayFile 0x229ed0
        *st.borrow_mut() = mk();
        let rv_o = call_fn(&mut r, 0x229ed0, &[sys, sl, 1, path_addr, 0x40]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_sys_play_file(&mut m, &mut env, sys, sl, 1, path_addr, 0x40, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "sys_playfile {case}");

        // StreamSys_Play 0x229f18
        *st.borrow_mut() = mk();
        let rv_o = call_fn(&mut r, 0x229f18, &[sys, sl, 2, 7, 0x80]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_sys_play(&mut m, &mut env, sys, sl, 2, 7, 0x80, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "sys_play {case}");

        // StreamSys_IsSlotQueued 0x22a088
        *st.borrow_mut() = mk();
        let rv_o = call_fn(&mut r, 0x22a088, &[sys, sl]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::stream_sys_is_slot_queued(&mut m, &mut env, sys, sl, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "sys_queued {case}");

        // SndHandle_IsPlaying 0x22a120
        *st.borrow_mut() = mk();
        let h = rng.next();
        let rv_o = call_fn(&mut r, 0x22a120, &[sys, h]);
        let sp = r.cpu.gpr(29) as u32;
        let mut env = mk();
        let rv_p = speechsys::snd_handle_is_playing(&mut m, &mut env, h, sp);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "handle_playing {case}");

        // StreamSys_ClearRequestPrebuffer 0x22a1e0
        *st.borrow_mut() = mk();
        call_fn(&mut r, 0x22a1e0, &[sys, sl, 0x99]);
        let mut env = mk();
        speechsys::stream_sys_clear_request_prebuffer(&mut m, &mut env, sys, sl, 0x99);
        assert_eq!(env.calls, st.borrow().calls, "sys_clrreq {case}");

        // StreamSys_SetVolume 0x22a228
        *st.borrow_mut() = mk();
        call_fn(&mut r, 0x22a228, &[sys, sl, 64]);
        let mut env = mk();
        speechsys::stream_sys_set_volume(&mut m, &mut env, sys, sl, 64);
        assert_eq!(env.calls, st.borrow().calls, "sys_setvol {case}");

        // StreamSys_GetStream 0x22a2b8
        assert_eq!(speechsys::stream_sys_get_stream(&m, sys, sl), call_fn(&mut r, 0x22a2b8, &[sys, sl]));

        // SndEvent_ClearStreamPrebuffer 0x22a2d0
        w(&mut m, &mut r, out_buf + 0x1c, 0x777);
        *st.borrow_mut() = mk();
        let rv_o = call_fn(&mut r, 0x22a2d0, &[sys, out_buf]);
        let mut env = mk();
        let rv_p = speechsys::snd_event_clear_stream_prebuffer(&mut m, &mut env, out_buf);
        assert_eq!((rv_p, env.calls), (rv_o, st.borrow().calls.clone()), "ev_clr {case}");
    }
}

#[test]
fn speech_lib_register_event_bank0() {
    let Some(mut r) = runner() else { return };
    r.allow(0x2bad70); // SpeechLib_RegisterEventBank
    r.allow(0x2bac78); // SpeechLib_AddEventBank
    r.allow(0x2bc738); // SpeechBank_GetKinds
    let mut rng = Rng(0x2bad_a002);

    struct FlatMem(Vec<(u32, Vec<u8>)>);
    impl FlatMem {
        fn new() -> Self {
            Self(vec![(0x0034_0000, vec![0u8; 0x10000]), (BASE, vec![0u8; 0x1000])])
        }
        fn at(&self, a: u32, n: u32) -> (usize, usize) {
            for (k, (b, v)) in self.0.iter().enumerate() {
                if *b <= a && a + n <= b + v.len() as u32 {
                    return (k, (a - b) as usize);
                }
            }
            panic!("oob {a:#x}");
        }
    }
    impl LibMem for FlatMem {
        fn r8(&self, a: u32) -> u8 { let (k, o) = self.at(a, 1); self.0[k].1[o] }
        fn r16(&self, a: u32) -> u16 { let (k, o) = self.at(a, 2); u16::from_le_bytes(self.0[k].1[o..o+2].try_into().unwrap()) }
        fn r32(&self, a: u32) -> u32 { let (k, o) = self.at(a, 4); u32::from_le_bytes(self.0[k].1[o..o+4].try_into().unwrap()) }
        fn w8(&mut self, a: u32, v: u8) { let (k, o) = self.at(a, 1); self.0[k].1[o] = v; }
        fn w16(&mut self, a: u32, v: u16) { let (k, o) = self.at(a, 2); self.0[k].1[o..o+2].copy_from_slice(&v.to_le_bytes()); }
        fn w32(&mut self, a: u32, v: u32) { let (k, o) = self.at(a, 4); self.0[k].1[o..o+4].copy_from_slice(&v.to_le_bytes()); }
    }

    const EVENT_BANKS: u32 = 0x348e08;
    let mut m = FlatMem::new();
    for k in 0..16u32 {
        m.w32(EVENT_BANKS + 4 * k, 0);
        r.mem.write_u32(EVENT_BANKS + 4 * k, 0);
    }
    for case in 0..20u32 {
        let bank = BASE + case * 0x40;
        let ver: u16 = if case % 5 == 0 { 0x802 } else { 0x803 };
        let k8 = (rng.next() % 4) as u8;
        let k9 = (rng.next() % 4) as u8;
        m.w16(bank, ver);
        r.mem.write(bank, &ver.to_le_bytes()).unwrap();
        m.w8(bank + 8, k8);
        r.mem.write(bank + 8, &[k8]).unwrap();
        m.w8(bank + 9, k9);
        r.mem.write(bank + 9, &[k9]).unwrap();
        let rv_o = call_fn(&mut r, 0x2bada0, &[bank]);
        let rv_p = speechlib::register_event_bank0(&mut m, bank);
        assert_eq!(rv_p, rv_o, "reg bank0 case {case}");
    }
    for k in 0..16u32 {
        assert_eq!(m.r32(EVENT_BANKS + 4 * k), r.mem.read_u32(EVENT_BANKS + 4 * k));
    }
}
