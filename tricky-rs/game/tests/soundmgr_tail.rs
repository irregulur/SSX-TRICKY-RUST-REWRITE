//! Function-runner check for row G10s (20 `audio` `cPS2SoundManager` / `cSoundSys` / `cSndPlayOpts` / `SndSys`
//! functions): `Audio_GetSongState` 0x225e98, `cPS2SoundManager` (`0x225fa8..0x226678`), `Audio_CreateSoundManager`
//! 0x226370, `SpuAlloc_PopRecord` 0x2266a8, `cSndPlayOpts::~cSndPlayOpts` 0x226cc0, `SndSys_SetEntryCallback`
//! 0x226e58, `SndSys_PushEntryCopy2` 0x227350, `SndSys_ResetEntry` 0x227508, `SndPlayOpts_InitCurrent` 0x227708,
//! `cSoundSys` (`0x227960..0x227a48`).

use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::sndqueue::{self as sq, SoundMgrEnv};

const BASE: u32 = 0x0140_0000;
const FAKE_VF0: u32 = 0x002f_f010;
const FAKE_VF1: u32 = 0x002f_f020;
const FAKE_VF2: u32 = 0x002f_f030;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    for a in [
        0x225fa8, // cPS2SoundManager::cPS2SoundManager
        0x226370, // Audio_CreateSoundManager
        0x2266a8, // SpuAlloc_PopRecord
        0x2266e8, // SpuAlloc_ReclaimRecord
        0x2279c8, // cSoundSys::~cSoundSys
        0x2cc808, // EAMem_Set
        0x2d4a60, // SndPlayOpts_Init
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
    let z = vec![0u8; 0x60000];
    r.mem.write(BASE, &z);
    Mem {
        regions: vec![
            (0x0034_0000, vec![0u8; 0x0001_0000]),
            (BASE, vec![0u8; 0x60000]),
        ],
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
    alloc_ret: u32,
    song_state: u32,
    spu_base: u32,
    spu_largest: u32,
    vf1_ret: u32,
    cfg_seed: u8,
    last_cfg_bytes: Vec<u8>,
}

impl SoundMgrEnv for EnvState {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        match target {
            sq::SND_GET_CONFIG => {
                self.calls.push((target, vec![]));
                let p = args[0];
                for i in 0..0x100u32 {
                    m.w8(p + i, self.cfg_seed.wrapping_add(i as u8));
                }
                0
            }
            sq::SND_SET_CONFIG => {
                self.calls.push((target, vec![]));
                self.last_cfg_bytes = mem_slice(m, args[0], 0x100);
                0
            }
            sq::SND_GET_LARGEST_SPU_FREE => {
                self.calls.push((target, vec![]));
                m.w32(args[0], self.spu_base);
                self.spu_largest
            }
            _ => {
                self.calls.push((target, args.to_vec()));
                match target {
                    sq::EAMEM_ALLOC => self.alloc_ret,
                    sq::PATHX_GET_SONG_STATE => self.song_state,
                    FAKE_VF1 => self.vf1_ret,
                    _ => 0,
                }
            }
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    let stubs: &[(u32, usize)] = &[
        (sq::MEM_DELETE, 1),
        (sq::MEM_FREE, 1),
        (sq::PATHX_GET_SONG_STATE, 1),
        (sq::EAMEM_ALLOC, 3),
        (sq::SND_GET_CONFIG, 1),
        (sq::SND_SET_CONFIG, 1),
        (sq::SND_INIT, 3),
        (sq::SND_SHUTDOWN, 0),
        (sq::SND_START_STREAM_POLL, 0),
        (sq::SND_SET_SPU_RANGE, 2),
        (sq::SND_GET_LARGEST_SPU_FREE, 1),
        (sq::DTS_INSTALL_ENCODER, 0),
        (FAKE_VF0, 2),
        (FAKE_VF1, 2),
        (FAKE_VF2, 1),
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
                sq::SND_GET_CONFIG => {
                    e.calls.push((addr, vec![]));
                    let p = a[0];
                    let seed = e.cfg_seed;
                    let buf: Vec<u8> = (0..0x100u32).map(|i| seed.wrapping_add(i as u8)).collect();
                    mem.write(p, &buf);
                    0
                }
                sq::SND_SET_CONFIG => {
                    e.calls.push((addr, vec![]));
                    let mut buf = vec![0u8; 0x100];
                    mem.read(a[0], &mut buf).unwrap();
                    e.last_cfg_bytes = buf;
                    0
                }
                sq::SND_GET_LARGEST_SPU_FREE => {
                    e.calls.push((addr, vec![]));
                    mem.write_u32(a[0], e.spu_base);
                    e.spu_largest
                }
                _ => {
                    e.calls.push((addr, a.clone()));
                    match addr {
                        sq::EAMEM_ALLOC => e.alloc_ret,
                        sq::PATHX_GET_SONG_STATE => e.song_state,
                        FAKE_VF1 => e.vf1_ret,
                        _ => 0,
                    }
                }
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    }
}

#[test]
fn ps2_sound_manager_functions_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x6105_0001);

    // 1. Audio_GetSongState 0x225e98
    for case in 0..40 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let ptr = if case % 2 == 0 { 0 } else { BASE + 0x5000 + case * 4 };
        m.w32(this + 0x36f0, ptr);
        r.mem.write_u32(this + 0x36f0, ptr);
        let song_st = rng.next();
        let st_ps = Rc::new(RefCell::new(EnvState { song_state: song_st, ..Default::default() }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState { song_state: song_st, ..Default::default() };
        assert_eq!(sq::audio_get_song_state(&mut m, &mut st_rs, this), call_fn(&mut r, 0x225e98, &[this]));
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }

    // 2. Audio_CreateSoundManager 0x226370, cPS2SoundManager::cPS2SoundManager 0x225fa8, vf7_225fd8, vf2_226320
    for flags in 0..4u32 {
        for has_heap in [false, true] {
            let mut m = fresh_mem(&mut r);
            let mgr_ptr = BASE + 0x1000;
            let st_ps = Rc::new(RefCell::new(EnvState { alloc_ret: mgr_ptr, ..Default::default() }));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState { alloc_ret: mgr_ptr, ..Default::default() };

            let ret_rs = sq::audio_create_sound_manager(&mut m, &mut st_rs);
            let ret_ps = call_fn(&mut r, 0x226370, &[]);
            assert_eq!(ret_rs, ret_ps);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(mem_slice(&m, mgr_ptr, 0x320), copy_out(&mut r, mgr_ptr, 0x320));

            let heap = if has_heap { BASE + 0x10000 } else { 0 };
            m.w32(mgr_ptr + 4, heap);
            r.mem.write_u32(mgr_ptr + 4, heap);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            sq::ps2_sound_mgr_dtor(&mut m, &mut st_rs, mgr_ptr, flags);
            call_fn(&mut r, 0x225fd8, &[mgr_ptr, flags]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);

            m.w32(mgr_ptr + 4, heap);
            r.mem.write_u32(mgr_ptr + 4, heap);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            sq::ps2_sound_mgr_shutdown(&mut m, &mut st_rs, mgr_ptr);
            call_fn(&mut r, 0x226320, &[mgr_ptr]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(mgr_ptr + 4), r.mem.read_u32(mgr_ptr + 4));
        }
    }

    // 3. cPS2SoundManager::vf1_226168 (all modes & prior states)
    for prior in 0..4u32 {
        for mode in [-1i32, 0, 1, 2, 3, 4, 99] {
            let mut m = fresh_mem(&mut r);
            let mgr = BASE + 0x1000;
            let cfg_buf = BASE + 0x2000;
            m.w32(mgr + 8, prior);
            r.mem.write_u32(mgr + 8, prior);
            let seed = rng.next() as u8;
            let st_ps = Rc::new(RefCell::new(EnvState { cfg_seed: seed, ..Default::default() }));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState { cfg_seed: seed, ..Default::default() };

            let ret_rs = sq::ps2_sound_mgr_set_mode(&mut m, &mut st_rs, mgr, mode as u32, cfg_buf);
            let ret_ps = call_fn(&mut r, 0x226168, &[mgr, mode as u32]);
            assert_eq!(ret_rs, ret_ps, "prior={prior} mode={mode}");
            assert_eq!(st_rs.calls, st_ps.borrow().calls, "calls prior={prior} mode={mode}");
            assert_eq!(st_rs.last_cfg_bytes, st_ps.borrow().last_cfg_bytes, "cfg prior={prior} mode={mode}");
            assert_eq!(m.r32(mgr + 8), r.mem.read_u32(mgr + 8), "state prior={prior} mode={mode}");
        }
    }

    // 4. cPS2SoundManager::vf0_226040 + SPU alloc/free/lock/unlock/pop sequence
    {
        let mut m = fresh_mem(&mut r);
        let mgr = BASE + 0x1000;
        let fake_vt = BASE + 0x200;
        let heap = BASE + 0x10000;
        let sp = BASE + 0x400;
        let init_vt = |w: &mut dyn FnMut(u32, u32)| {
            w(fake_vt + 0x08, 0);
            w(fake_vt + 0x0c, FAKE_VF0);
            w(fake_vt + 0x10, 0);
            w(fake_vt + 0x14, FAKE_VF1);
            w(fake_vt + 0x18, 0);
            w(fake_vt + 0x1c, FAKE_VF2);
            w(mgr, fake_vt);
        };
        init_vt(&mut |a, v| m.w32(a, v));
        init_vt(&mut |a, v| r.mem.write_u32(a, v));

        let st_ps = Rc::new(RefCell::new(EnvState {
            alloc_ret: heap,
            spu_base: 0x0002_0000,
            spu_largest: 0x0018_0000,
            ..Default::default()
        }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState {
            alloc_ret: heap,
            spu_base: 0x0002_0000,
            spu_largest: 0x0018_0000,
            ..Default::default()
        };

        sq::ps2_sound_mgr_init(&mut m, &mut st_rs, mgr, 2, sp);
        call_fn(&mut r, 0x226040, &[mgr, 2]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, mgr, 0x320), copy_out(&mut r, mgr, 0x320));
        assert_eq!(mem_slice(&m, heap, 0x32000), copy_out(&mut r, heap, 0x32000));

        // Drive 300 random SPU alloc / free / lock / unlock steps
        let mut live: Vec<u32> = Vec::new();
        for step in 0..300 {
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            let op = rng.next() % 10;
            if op < 5 && live.len() < 18 && m.r32(mgr + 0x310) != 0xffff_ffff {
                let sz = ((rng.next() % 8) + 1) * 0x1000;
                let avail = if step % 17 == 0 { 0x800 } else { 0x0010_0000 };
                st_rs.spu_largest = avail;
                st_ps.borrow_mut().spu_largest = avail;
                let h_rs = sq::ps2_sound_mgr_alloc_spu(&mut m, &mut st_rs, mgr, sz, sp);
                let h_ps = call_fn(&mut r, 0x2263c8, &[mgr, sz]);
                assert_eq!(h_rs, h_ps, "alloc step {step}");
                assert_eq!(st_rs.calls, st_ps.borrow().calls, "alloc calls step {step}");
                assert_eq!(mem_slice(&m, mgr, 0x320), copy_out(&mut r, mgr, 0x320), "alloc mem step {step}");
                if (h_rs as i32) >= 0 {
                    // Preserve original vf3_2263c8 exact-head-reuse bug: avoid re-freeing a dangling-head handle
                    // (which would create a self-cycle in both original MIPS and Rust).
                    if m.r32(mgr + 0x318) != h_rs {
                        live.push(h_rs);
                    }
                }
            } else if op < 8 && !live.is_empty() {
                let idx = (rng.next() as usize) % live.len();
                let h = live.swap_remove(idx);
                sq::ps2_sound_mgr_free_spu(&mut m, &mut st_rs, mgr, h);
                call_fn(&mut r, 0x226520, &[mgr, h]);
                assert_eq!(st_rs.calls, st_ps.borrow().calls, "free calls step {step}");
                assert_eq!(mem_slice(&m, mgr, 0x320), copy_out(&mut r, mgr, 0x320), "free mem step {step}");
            } else if op == 8 && !live.is_empty() {
                let h = live[(rng.next() as usize) % live.len()];
                sq::ps2_sound_mgr_lock_spu(&mut m, &mut st_rs, mgr, h);
                call_fn(&mut r, 0x226650, &[mgr, h]);
                assert_eq!(st_rs.calls, st_ps.borrow().calls, "lock step {step}");
                assert_eq!(mem_slice(&m, mgr, 0x320), copy_out(&mut r, mgr, 0x320));
            } else {
                sq::ps2_sound_mgr_unlock_spu(&mut m, &mut st_rs, mgr);
                call_fn(&mut r, 0x226678, &[mgr]);
                assert_eq!(st_rs.calls, st_ps.borrow().calls, "unlock step {step}");
                assert_eq!(mem_slice(&m, mgr, 0x320), copy_out(&mut r, mgr, 0x320));
            }
        }

        // Also explicitly check SpuAlloc_PopRecord 0x2266a8 until empty (and once while empty)
        for _ in 0..35 {
            let p_rs = sq::spu_alloc_pop_record(&mut m, mgr);
            let p_ps = call_fn(&mut r, 0x2266a8, &[mgr]);
            assert_eq!(p_rs, p_ps);
            assert_eq!(mem_slice(&m, mgr, 0x320), copy_out(&mut r, mgr, 0x320));
        }
    }
}

#[test]
fn sound_sys_and_play_opts_functions_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x6105_0002);

    // 5. cSoundSys::cSoundSys 0x227960, vf1_227a48, ~cSoundSys 0x2279c8, cSndPlayOpts::~cSndPlayOpts 0x226cc0
    for mask in 0..16u32 {
        for flags in 0..4u32 {
            let mut m = fresh_mem(&mut r);
            let sys = BASE + 0x100;
            let mgr = BASE + 0x2000;
            let fake_vt = BASE + 0x3000;
            let mode = rng.next() % 4;
            let vf1_ret = rng.next();

            let st_ps = Rc::new(RefCell::new(EnvState { alloc_ret: mgr, vf1_ret, ..Default::default() }));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState { alloc_ret: mgr, vf1_ret, ..Default::default() };

            // Install fake vtable on the real VT_PS2_SOUND_MANAGER slot? No, Audio_CreateSoundManager writes
            // VT_PS2_SOUND_MANAGER (0x3a01f8), whose +0xc/+0x14/+0x1c are 0x226040/0x226168/0x226320!
            // Stub those 3 methods directly for cSoundSys tests so we test dispatch cleanly.
            for &(a, n) in &[(0x226040u32, 2usize), (0x226168, 2), (0x226320, 1)] {
                let s = st_ps.clone();
                r.stub(a, move |cpu, _| {
                    let mut args = Vec::with_capacity(n);
                    for k in 0..n {
                        args.push(cpu.gpr(4 + k) as u32);
                    }
                    let mut e = s.borrow_mut();
                    e.calls.push((a, args));
                    let rv = if a == 0x226168 { e.vf1_ret } else { 0 };
                    cpu.set_gpr(2, rv as i32 as i64 as u64);
                });
            }
            // Mirror the ROM vtable 0x3a01f8 into our pf::Mem so sound_sys_* resolves the same slots!
            let mut vt_bytes = vec![0u8; 0x30];
            r.mem.read(sq::VT_PS2_SOUND_MANAGER, &mut vt_bytes).unwrap();
            m.regions.push((sq::VT_PS2_SOUND_MANAGER, vt_bytes));
            // Also intercept 0x226168 return in EnvState
            let ret_rs = sq::sound_sys_ctor(&mut m, &mut st_rs, sys, mode);
            let ret_ps = call_fn(&mut r, 0x227960, &[sys, mode]);
            assert_eq!(ret_rs, ret_ps);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(sq::G_SOUND_SYS), r.mem.read_u32(sq::G_SOUND_SYS));
            assert_eq!(mem_slice(&m, sys, 8), copy_out(&mut r, sys, 8));

            // vf1_227a48 with custom fake_vt (+0x10 adjust = 4, +0x14 = FAKE_VF1)
            let set_vt = |w: &mut dyn FnMut(u32, u32)| {
                w(fake_vt + 0x10, 4);
                w(fake_vt + 0x14, FAKE_VF1);
                w(fake_vt + 0x18, 0);
                w(fake_vt + 0x1c, FAKE_VF2);
                w(mgr, fake_vt);
            };
            set_vt(&mut |a, v| m.w32(a, v));
            set_vt(&mut |a, v| r.mem.write_u32(a, v));
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            assert_eq!(sq::sound_sys_vf1(&mut m, &mut st_rs, sys, mode), call_fn(&mut r, 0x227a48, &[sys, mode]));
            assert_eq!(st_rs.calls, st_ps.borrow().calls);

            // cSndPlayOpts::~cSndPlayOpts 0x226cc0 (also exercises cSoundSys::~cSoundSys 0x2279c8)
            let ptr_offs = [0x24u32, 0x38, 0x3c, 0x40, 0x64, 0x68, 0x6c, 0x70, 0x74, 0x78, 0x7c];
            for (k, &off) in ptr_offs.iter().enumerate() {
                let v = if ((mask >> (k % 4)) & 1) != 0 { BASE + 0x8000 + off * 4 } else { 0 };
                m.w32(sys + off, v);
                r.mem.write_u32(sys + off, v);
            }
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            sq::snd_play_opts_dtor(&mut m, &mut st_rs, sys, flags);
            call_fn(&mut r, 0x226cc0, &[sys, flags]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls, "dtor mask={mask} flags={flags}");
            assert_eq!(m.r32(sys + 4), r.mem.read_u32(sys + 4));
            assert_eq!(m.r32(sq::G_SOUND_SYS), r.mem.read_u32(sq::G_SOUND_SYS));
        }
    }

    // 6. SndSys_SetEntryCallback 0x226e58, SndSys_PushEntryCopy2 0x227350, SndSys_ResetEntry 0x227508,
    //    SndPlayOpts_InitCurrent 0x227708
    {
        let mut m = fresh_mem(&mut r);
        let sys = BASE + 0x100;
        let arr_base = BASE + 0x1000;
        let ptrs: &[(u32, u32)] = &[
            (0x24, arr_base),
            (0x38, arr_base + 0x200),
            (0x3c, arr_base + 0x280),
            (0x40, arr_base + 0x300),
            (0x64, arr_base + 0x380),
            (0x68, arr_base + 0x400),
            (0x6c, arr_base + 0x480),
            (0x70, arr_base + 0x500),
            (0x74, arr_base + 0x580),
            (0x78, arr_base + 0x780),
            (0x7c, arr_base + 0x800),
        ];
        for &(off, p) in ptrs {
            m.w32(sys + off, p);
            r.mem.write_u32(sys + off, p);
        }

        for case in 0..200 {
            let idx = (rng.next() % 3) as u32;
            m.w32(sys + 0x20, idx);
            r.mem.write_u32(sys + 0x20, idx);
            // Random 24-byte header at +8..+0x20 (including high-bit first byte to test sign extension!)
            for k in 0..6u32 {
                let w = rng.next();
                m.w32(sys + 8 + k * 4, w);
                r.mem.write_u32(sys + 8 + k * 4, w);
            }

            // SndSys_PushEntryCopy2 0x227350
            sq::snd_sys_push_entry_copy2(&mut m, sys);
            call_fn(&mut r, 0x227350, &[sys]);
            assert_eq!(mem_slice(&m, sys, 0x80), copy_out(&mut r, sys, 0x80), "push2 sys {case}");
            assert_eq!(mem_slice(&m, arr_base, 0x900), copy_out(&mut r, arr_base, 0x900), "push2 arr {case}");

            // SndSys_SetEntryCallback 0x226e58 (64-bit $a1)
            let cb = ((rng.next() as u64) << 32) | (rng.next() as u64);
            sq::snd_sys_set_entry_callback(&mut m, sys, cb);
            r.cpu.set_gpr(4, sys as i32 as i64 as u64);
            r.cpu.set_gpr(5, cb);
            r.call(0x226e58).unwrap();
            assert_eq!(mem_slice(&m, sys, 0x80), copy_out(&mut r, sys, 0x80), "set_cb {case}");

            // SndPlayOpts_InitCurrent 0x227708
            sq::snd_play_opts_init_current(&mut m, sys);
            call_fn(&mut r, 0x227708, &[sys]);
            assert_eq!(mem_slice(&m, arr_base, 0x200), copy_out(&mut r, arr_base, 0x200), "init_cur {case}");

            // SndSys_ResetEntry 0x227508
            sq::snd_sys_reset_entry(&mut m, sys);
            call_fn(&mut r, 0x227508, &[sys]);
            assert_eq!(mem_slice(&m, sys, 0x80), copy_out(&mut r, sys, 0x80), "reset sys {case}");
            assert_eq!(mem_slice(&m, arr_base, 0x900), copy_out(&mut r, arr_base, 0x900), "reset arr {case}");
        }
    }
}
