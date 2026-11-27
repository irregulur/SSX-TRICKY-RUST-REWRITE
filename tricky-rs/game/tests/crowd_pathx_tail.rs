//! Function-runner check for row G10t (16 `audio` `cController`, `cCrowdSys`, `IniReader`, `Monitor`, and `PathX`
//! functions): `cController::cController` 0x21e8a0, `cController::~cController` 0x21e8d8, `CrowdSoundList_Destruct`
//! 0x21ece0, `cCrowdSys::cCrowdSys` 0x220850, `cCrowdSys::~cCrowdSys` 0x2208e0, `SoundId_Split` 0x220f80,
//! `cCrowdSys::vf5_2213c8` 0x2213c8, `cCrowdSys::vf6_2214a8` 0x2214a8, `CrowdSoundSet_Destruct` 0x221538,
//! `Audio_SetHandlePitchBend` 0x2215a8, `IniReader_ParseFloat` 0x221d58, `Monitor_TickCallback` 0x222ae0,
//! `PathX_FindFreeSlot` 0x222fb0, `PathX_LoadBank` 0x222fe8, `PathX_ReadFile` 0x223098, `PathX_PollTask` 0x223128.

use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::crowdbox as cb;
use tricky_game::pf::Mem;
use tricky_game::speechlib::Mem as LibMem;
use tricky_game::speechload::LoadEnv;

const BASE: u32 = 0x0140_0000;
const FAKE_TICK: u32 = 0x002f_f100;
const FAKE_TASK: u32 = 0x002f_f110;

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
        0x21ece0, // CrowdSoundList_Destruct
        0x220850, // cCrowdSys::cCrowdSys
        0x2208e0, // cCrowdSys::~cCrowdSys
        0x221538, // CrowdSoundSet_Destruct
        0x223098, // PathX_ReadFile
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
    r.mem.write(0x0034_3000, &zg);
    Mem {
        regions: vec![
            (0x0034_3000, vec![0u8; 0x1000]),
            (BASE, vec![0u8; 0x20000]),
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
    vf6_ret: u32,
    sscanf_float: u32,
    file_size: u32,
    alloc_bnk_ret: u32,
    alloc_bkh_ret: u32,
    file_handle: u32,
    reg_rc: u32,
    reg_bank_id: u32,
    res_size: u32,
}

impl LoadEnv for EnvState {
    fn call(&mut self, m: &mut dyn LibMem, func: u32, args: &[u32]) -> u32 {
        match func {
            cb::SSCANF => {
                self.calls.push((func, vec![args[0], args[1]]));
                m.w32(args[2], self.sscanf_float);
                1
            }
            cb::FILE_OPEN_SYNC => {
                self.calls.push((func, vec![args[0], args[1], args[2]]));
                m.w32(args[3], self.file_handle);
                0
            }
            cb::SND_BANK_REGISTER => {
                self.calls.push((func, vec![args[1]]));
                m.w32(args[0], self.reg_bank_id);
                self.reg_rc
            }
            cb::EAMEM_ALLOC0 => {
                self.calls.push((func, args.to_vec()));
                if args[0] == cb::NAME_PATHX_BNK {
                    self.alloc_bnk_ret
                } else {
                    self.alloc_bkh_ret
                }
            }
            _ => {
                self.calls.push((func, args.to_vec()));
                match func {
                    cb::BANK_MONITOR_VF6 => self.vf6_ret,
                    cb::FILE_GET_SIZE => self.file_size,
                    cb::SND_BANK_GET_RESIDENT_SIZE => self.res_size,
                    _ => 0,
                }
            }
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    let stubs: &[(u32, usize)] = &[
        (cb::MEM_DELETE, 1),
        (cb::BANK_MONITOR_VF5, 1),
        (cb::CROWD_SOUND_LIST_CTOR, 1),
        (cb::PTR_ARRAY_FREE_ALL, 1),
        (cb::CROWD_SYS_FREE, 1),
        (cb::CROWD_SYS_STOP_ALL, 1),
        (cb::INI_SKIP_SPACE, 1),
        (cb::INI_READ_TOKEN, 1),
        (cb::SPEECH_SYS_INIT, 1),
        (cb::SPEECH_SYS_DTOR, 2),
        (cb::FILE_GET_SIZE, 1),
        (cb::FILE_OPEN_SYNC, 4),
        (cb::FILE_READ_SYNC, 5),
        (cb::FILE_CLOSE_SYNC, 2),
        (cb::EAMEM_ALLOC0, 3),
        (cb::SND_LOCK, 0),
        (cb::SND_UNLOCK, 0),
        (cb::SND_BANK_REGISTER, 2),
        (cb::SND_BANK_GET_RESIDENT_SIZE, 1),
        (cb::SND_BANK_RELOCATE, 2),
        (cb::SND_HANDLE_SET_PITCH_BEND, 2),
        (cb::SSCANF, 3),
        (FAKE_TICK, 1),
        (FAKE_TASK, 0),
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
                cb::SSCANF => {
                    e.calls.push((addr, vec![a[0], a[1]]));
                    mem.write_u32(a[2], e.sscanf_float);
                    1
                }
                cb::FILE_OPEN_SYNC => {
                    e.calls.push((addr, vec![a[0], a[1], a[2]]));
                    mem.write_u32(a[3], e.file_handle);
                    0
                }
                cb::SND_BANK_REGISTER => {
                    e.calls.push((addr, vec![a[1]]));
                    mem.write_u32(a[0], e.reg_bank_id);
                    e.reg_rc
                }
                cb::EAMEM_ALLOC0 => {
                    e.calls.push((addr, a.clone()));
                    if a[0] == cb::NAME_PATHX_BNK {
                        e.alloc_bnk_ret
                    } else {
                        e.alloc_bkh_ret
                    }
                }
                _ => {
                    e.calls.push((addr, a.clone()));
                    match addr {
                        cb::FILE_GET_SIZE => e.file_size,
                        cb::SND_BANK_GET_RESIDENT_SIZE => e.res_size,
                        _ => 0,
                    }
                }
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    }
    // Special 64-bit stub for cBankMonitor::vf6
    let s6 = st.clone();
    r.stub(cb::BANK_MONITOR_VF6, move |cpu, _| {
        let this = cpu.gpr(4) as u32;
        let arg64 = cpu.gpr(5);
        let mut e = s6.borrow_mut();
        e.calls.push((cb::BANK_MONITOR_VF6, vec![this, arg64 as u32, (arg64 >> 32) as u32]));
        let rv = e.vf6_ret;
        cpu.set_gpr(2, rv as i32 as i64 as u64);
    });
}

#[test]
fn crowd_controller_and_monitor_functions_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x610a_0001);

    // 1. cController::cController 0x21e8a0 & ~cController 0x21e8d8 (also exercises cCrowdSys::cCrowdSys / ~cCrowdSys /
    //    CrowdSoundSet_Destruct / CrowdSoundList_Destruct)
    for flags in 0..4u32 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();

        let ret_rs = cb::controller_ctor(&mut m, &mut st_rs, this);
        let ret_ps = call_fn(&mut r, 0x21e8a0, &[this]);
        assert_eq!(ret_rs, ret_ps);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, this, 0x3834), copy_out(&mut r, this, 0x3834));
        assert_eq!(m.r32(0x343120), r.mem.read_u32(0x343120));

        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        cb::controller_dtor(&mut m, &mut st_rs, this, flags);
        call_fn(&mut r, 0x21e8d8, &[this, flags]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls, "dtor flags={flags}");
        assert_eq!(m.r32(this + 4), r.mem.read_u32(this + 4));
        assert_eq!(m.r32(0x343120), r.mem.read_u32(0x343120));

        // Direct CrowdSoundSet_Destruct & CrowdSoundList_Destruct with flags 0..4
        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        cb::crowd_sound_set_destruct(&mut m, &mut st_rs, BASE + 0x4000, flags);
        call_fn(&mut r, 0x221538, &[BASE + 0x4000, flags]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);

        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        cb::crowd_sound_list_destruct(&mut m, &mut st_rs, BASE + 0x4100, flags);
        call_fn(&mut r, 0x21ece0, &[BASE + 0x4100, flags]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }

    // 2. SoundId_Split 0x220f80
    for _ in 0..500 {
        let mut m = fresh_mem(&mut r);
        let packed = rng.next();
        let lo = BASE + 0x100;
        let hi = BASE + 0x104;
        cb::sound_id_split(&mut m, packed, lo, hi);
        call_fn(&mut r, 0x220f80, &[BASE, packed, lo, hi]);
        assert_eq!(m.r32(lo), r.mem.read_u32(lo));
        assert_eq!(m.r32(hi), r.mem.read_u32(hi));
    }

    // 3. cCrowdSys::vf5_2213c8, cCrowdSys::vf6_2214a8, Audio_SetHandlePitchBend 0x2215a8
    {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let st_ps = Rc::new(RefCell::new(EnvState { vf6_ret: 0x1234_5678, ..Default::default() }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState { vf6_ret: 0x1234_5678, ..Default::default() };

        cb::crowd_sys_vf5(&mut m, &mut st_rs, this);
        call_fn(&mut r, 0x2213c8, &[this]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);

        for _ in 0..50 {
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            let a64 = ((rng.next() as u64) << 32) | (rng.next() as u64);
            let ret_rs = cb::crowd_sys_vf6(&mut m, &mut st_rs, this, a64);
            r.cpu.set_gpr(4, this as i32 as i64 as u64);
            r.cpu.set_gpr(5, a64);
            r.call(0x2214a8).unwrap();
            let ret_ps = r.cpu.gpr(2) as u32;
            assert_eq!(ret_rs, ret_ps);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
        }

        for _ in 0..50 {
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            let h = rng.next();
            let bend = rng.next();
            m.w32(this + 4, h);
            r.mem.write_u32(this + 4, h);
            cb::audio_set_handle_pitch_bend(&mut m, &mut st_rs, this, bend);
            call_fn(&mut r, 0x2215a8, &[this, bend]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
        }
    }

    // 4. IniReader_ParseFloat 0x221d58
    for _ in 0..50 {
        let mut m = fresh_mem(&mut r);
        let reader = BASE + 0x100;
        let out_sp = BASE + 0x200;
        let fbits = rng.next();
        let st_ps = Rc::new(RefCell::new(EnvState { sscanf_float: fbits, ..Default::default() }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState { sscanf_float: fbits, ..Default::default() };

        let ret_rs = cb::ini_reader_parse_float(&mut m, &mut st_rs, reader, out_sp);
        call_fn(&mut r, 0x221d58, &[reader]);
        let ret_ps = f32::from_bits(r.cpu.fpr[0]);
        assert_eq!(ret_rs.to_bits(), ret_ps.to_bits());
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }

    // 5. Monitor_TickCallback 0x222ae0
    for adj in [0i16, 4, -8, 16] {
        let mut m = fresh_mem(&mut r);
        let mon = BASE + 0x100;
        let vt = BASE + 0x200;
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();

        for w in [&mut m as &mut dyn LibMem] {
            w.w32(cb::G_MONITOR, mon);
            w.w32(mon + 4, vt);
            w.w16(vt + 0x28, adj as u16);
            w.w32(vt + 0x2c, FAKE_TICK);
        }
        r.mem.write_u32(cb::G_MONITOR, mon);
        r.mem.write_u32(mon + 4, vt);
        r.mem.write_u16(vt + 0x28, adj as u16);
        r.mem.write_u32(vt + 0x2c, FAKE_TICK);

        cb::monitor_tick_callback(&mut m, &mut st_rs);
        call_fn(&mut r, 0x222ae0, &[]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }
}

#[test]
fn pathx_functions_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x610a_0002);

    // 6. PathX_FindFreeSlot 0x222fb0 (including all-full and every single free position 0..24)
    for target_free in 0..=24u32 {
        let mut m = fresh_mem(&mut r);
        for i in 0..24u32 {
            let v = if i < target_free { i } else if i == target_free { 0xffff_ffff } else { rng.next() };
            let a = cb::G_PATHX_SLOTS + i * 0x3c;
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        assert_eq!(cb::pathx_find_free_slot(&m), call_fn(&mut r, 0x222fb0, &[]), "target_free={target_free}");
    }

    // 7. PathX_ReadFile 0x223098 & PathX_LoadBank 0x222fe8
    for alloc_bnk in [0u32, BASE + 0x5000] {
        for reg_rc in [0u32, 7] {
            for alloc_bkh in [0u32, BASE + 0x9000] {
                let mut m = fresh_mem(&mut r);
                let slot = rng.next() % 24;
                let path = BASE + 0x80;
                let sp = BASE + 0x200;
                let st_rs_init = EnvState {
                    calls: vec![],
                    vf6_ret: 0,
                    sscanf_float: 0,
                    file_size: 0x4321,
                    alloc_bnk_ret: alloc_bnk,
                    alloc_bkh_ret: alloc_bkh,
                    file_handle: 0x77,
                    reg_rc,
                    reg_bank_id: 0x15,
                    res_size: 0x220,
                };
                let st_ps = Rc::new(RefCell::new(EnvState { ..st_rs_init }));
                install_stubs(&mut r, st_ps.clone());
                let mut st_rs = EnvState {
                    calls: vec![],
                    vf6_ret: 0,
                    sscanf_float: 0,
                    file_size: 0x4321,
                    alloc_bnk_ret: alloc_bnk,
                    alloc_bkh_ret: alloc_bkh,
                    file_handle: 0x77,
                    reg_rc,
                    reg_bank_id: 0x15,
                    res_size: 0x220,
                };

                let ret_rs = cb::pathx_load_bank(&mut m, &mut st_rs, slot, path, sp);
                let ret_ps = call_fn(&mut r, 0x222fe8, &[slot, path]);
                assert_eq!(ret_rs, ret_ps);
                assert_eq!(st_rs.calls, st_ps.borrow().calls);
                let dst = cb::G_PATHX_SLOTS + slot * 0x3c + 0x20;
                assert_eq!(m.r32(dst), r.mem.read_u32(dst));
            }
        }
    }

    // 8. PathX_PollTask 0x223128
    for cb_ptr in [0u32, FAKE_TASK] {
        let mut m = fresh_mem(&mut r);
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();
        m.w32(cb::G_PATHX_TASK, cb_ptr);
        r.mem.write_u32(cb::G_PATHX_TASK, cb_ptr);
        assert_eq!(cb::pathx_poll_task(&mut m, &mut st_rs), call_fn(&mut r, 0x223128, &[]));
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }
}
