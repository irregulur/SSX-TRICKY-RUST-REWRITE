//! `tricky_game::sndmidi` (board row E1b2g7d3b7b1) against `SLUS_203.26` in the function runner
//! (`TRICKY_ELF`; skipped without it, and disc `.eam` replay also needs `TRICKY_DISC`).
//!
//! Checks every `SndMidi_*` (0x2d37e8..0x2d4a08), `Snd_MidiPanToPan` (0x2d6910), `SndPlayOpts_Init` (0x2d4a60),
//! and `MidiSys_*` (0x222080..0x222798) function bit-for-bit: return values, every external call (target,
//! arguments, and `SndPlayOpts` bytes), and all globals / struct memory.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::ps2float::Rules;
use r5900::Runner;
use tricky_game::musicsong::SongEnv;
use tricky_game::pf::Mem;
use tricky_game::sndmidi::*;

#[derive(Clone)]
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    Some(r)
}

const GLOBALS_BASE: u32 = 0x0035_07e0;
const GLOBALS_SIZE: usize = 0x0a20; // 0x3507e0..0x351200 (covers cursor, 16 slots, 24 voices, next_gen, shutdown_cb)
const ARENA: u32 = 0x0100_0000;
const ARENA_SIZE: usize = 0x4000;
const SP_SCRATCH: u32 = 0x0102_0000;
const SP_SIZE: usize = 0x100;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Call {
    target: u32,
    args: Vec<u32>,
    opts: Option<[u8; 24]>,
}

struct Oracle {
    g: Rng,
    log: Vec<Call>,
    next_voice: i32,
    live_voices: std::collections::BTreeSet<i32>,
    deterministic_voices: bool,
}

impl Oracle {
    fn new(seed: u64) -> Self {
        Oracle {
            g: Rng(seed),
            log: Vec::new(),
            next_voice: 1,
            live_voices: std::collections::BTreeSet::new(),
            deterministic_voices: false,
        }
    }

    fn handle_call(&mut self, target: u32, args: &[u32], opts: Option<[u8; 24]>) -> u32 {
        let mut logged = args.to_vec();
        // `args[2]` of `SND_PlayBankSound` is the caller's stack pointer; the 24-byte struct is checked via `opts`.
        if target == SND_PLAY_BANK_SOUND && logged.len() >= 3 {
            logged[2] = 0;
        }
        self.log.push(Call { target, args: logged, opts });
        match target {
            SND_PLAY_BANK_SOUND => {
                if self.deterministic_voices {
                    let h = self.next_voice;
                    self.next_voice += 1;
                    self.live_voices.insert(h);
                    h as u32
                } else if self.g.below(6) == 0 {
                    u32::MAX
                } else {
                    let h = (self.g.next() & 0x7fff) as i32;
                    self.live_voices.insert(h);
                    h as u32
                }
            }
            SND_VOICE_VALIDATE => {
                let h = args[0] as i32;
                if self.deterministic_voices {
                    if self.live_voices.contains(&h) {
                        h as u32
                    } else {
                        u32::MAX
                    }
                } else if h < 0 || self.g.below(3) == 0 {
                    u32::MAX
                } else {
                    (h & 0x1f) as u32
                }
            }
            SND_HANDLE_RELEASE | SND_HANDLE_STOP => {
                self.live_voices.remove(&(args[0] as i32));
                0
            }
            EAMEM_ALLOC => {
                if self.g.below(4) == 0 {
                    0
                } else {
                    ARENA + 0x2000
                }
            }
            BANK_MONITOR_ADD_INSTANCE => {
                if self.g.below(3) == 0 {
                    0
                } else {
                    ARENA + 0x2800
                }
            }
            SND_MIDI_PLAY_ADDR => {
                [0u32, 0x20, 0x41, (-9i32) as u32][self.g.below(4) as usize]
            }
            _ => 0,
        }
    }
}

struct PortEnv(Rc<RefCell<Oracle>>);
impl SongEnv for PortEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        if target == MEMSET {
            let (dst, val, len) = (args[0], args[1] as u8, args[2]);
            for k in 0..len {
                m.w8(dst.wrapping_add(k), val);
            }
            return dst;
        }
        let opts = if target == SND_PLAY_BANK_SOUND {
            let mut b = [0u8; 24];
            for k in 0..24 {
                b[k] = m.r8(args[2] + k as u32);
            }
            Some(b)
        } else {
            None
        };
        self.0.borrow_mut().handle_call(target, args, opts)
    }
    fn call_f(&mut self, _: &mut Mem, target: u32, _: &[u32]) -> f32 {
        panic!("unexpected call_f {target:#x}")
    }
    fn call_out(&mut self, _: &mut Mem, target: u32, _: &[u32], _: usize) -> Vec<u32> {
        panic!("unexpected call_out {target:#x}")
    }
}

fn hook_sndmidi(r: &mut Runner, oracle: Rc<RefCell<Oracle>>) {
    for &(addr, nargs) in &[
        (SND_LOCK, 0usize),
        (SND_UNLOCK, 0),
        (SND_ADD_TICK_CB, 1),
        (SND_REMOVE_TICK_CB, 1),
        (SND_PLAY_BANK_SOUND, 3),
        (SND_HANDLE_SET_VOLUME, 2),
        (SND_HANDLE_SET_PITCH, 2),
        (SND_HANDLE_STOP, 1),
        (SND_HANDLE_SET_PAN, 3),
        (SND_VOICE_VALIDATE, 1),
        (SND_HANDLE_SET_PITCH_BEND, 2),
        (SND_HANDLE_RELEASE, 1),
    ] {
        let o = oracle.clone();
        r.hook(addr, move |cpu, mem| {
            let mut args = Vec::with_capacity(nargs);
            for k in 0..nargs {
                args.push(cpu.gpr(4 + k) as u32);
            }
            let opts = if addr == SND_PLAY_BANK_SOUND {
                let mut b = [0u8; 24];
                mem.read(args[2], &mut b).unwrap();
                Some(b)
            } else {
                None
            };
            let ret = o.borrow_mut().handle_call(addr, &args, opts);
            cpu.set_gpr(2, ret as i32 as i64 as u64);
            true
        });
    }
    // Allow internal calls within SndMidi
    for addr in [
        0x002d_37e8, // Validate
        0x002d_3838, // UpdateFade
        0x002d_3938, // VoicesDone
        0x002d_39c8, // NoteOn
        0x002d_3b90, // NoteOff
        0x002d_3c30, // OnController
        0x002d_3e18, // OnPitchBend
        0x002d_3f00, // RunEvents
        0x002d_4108, // Tick
        0x002d_4228, // Init
        0x002d_42c8, // Shutdown
        0x002d_4308, // StopAll
        0x002d_4368, // Stop
        0x002d_4440, // Play
        0x002d_45a8, // SetPitch
        0x002d_4660, // SetPan
        0x002d_4740, // Pause
        0x002d_4808, // Resume
        0x002d_4888, // SetVolume
        0x002d_4978, // Fade
        0x002d_4a08, // GetState
        0x002d_4a60, // SndPlayOpts_Init
        0x002d_6910, // Snd_MidiPanToPan
    ] {
        r.allow(addr);
    }
}

fn read_globals(r: &mut Runner) -> Vec<u8> {
    let mut b = vec![0u8; GLOBALS_SIZE];
    r.mem.read(GLOBALS_BASE, &mut b).unwrap();
    b
}

fn write_globals(r: &mut Runner, m: &mut Mem, bytes: &[u8]) {
    r.mem.write(GLOBALS_BASE, bytes);
    for (k, &v) in bytes.iter().enumerate() {
        m.w8(GLOBALS_BASE + k as u32, v);
    }
}

fn port_globals(m: &Mem) -> Vec<u8> {
    (0..GLOBALS_SIZE).map(|k| m.r8(GLOBALS_BASE + k as u32)).collect()
}

#[test]
fn midi_pan_to_pan_matches_original() {
    let Some(mut r) = runner() else { return };
    for p in 0u8..128 {
        r.cpu.set_gpr(4, p as u64);
        r.call(0x002d_6910).unwrap();
        assert_eq!(r.cpu.gpr(2) as u16, midi_pan_to_pan(p), "pan {p}");
    }
}

#[test]
fn play_opts_init_matches_original() {
    let Some(mut r) = runner() else { return };
    let buf = r.mem.alloc(0x40);
    let mut g = Rng(0x51a0_beef_cafe_1234);
    for case in 0..200 {
        let mut init_bytes = [0u8; 24];
        for b in &mut init_bytes {
            *b = g.byte();
        }
        r.mem.write(buf, &init_bytes);
        let mut m = Mem { regions: vec![(buf, init_bytes.to_vec())] };
        r.cpu.set_gpr(4, buf as u64);
        r.call(0x002d_4a60).unwrap();
        let ret_r = r.cpu.gpr(2) as u32;
        let ret_p = play_opts_init(&mut m, buf);
        assert_eq!(ret_r, ret_p, "case {case} ret");
        let mut after_r = [0u8; 24];
        r.mem.read(buf, &mut after_r).unwrap();
        let after_p: Vec<u8> = (0..24).map(|k| m.r8(buf + k)).collect();
        assert_eq!(&after_r[..], &after_p[..], "case {case} bytes");
    }
}

/// Build a synthetic `.eam` byte stream exercising every event kind:
/// program change, CC 7/10/0x14/0x1e/other, pitch bend, note on (normal + ch9), note off, wait extension, EOT.
fn make_stream(g: &mut Rng, header_kind: u32) -> Vec<u8> {
    let mut s = Vec::new();
    match header_kind % 3 {
        0 => s.extend_from_slice(b"MIDx"),
        1 => s.extend_from_slice(b"MD\x00\x00\x01\x02\x03\x04"),
        _ => {}
    }
    // Program change on ch 0 and ch 9
    s.extend_from_slice(&[0, 0xc0, g.byte() & 0x7f]);
    s.extend_from_slice(&[0, 0xc9, g.byte() & 0x7f]);
    // Pan & volume
    s.extend_from_slice(&[0, 0xb0, 10, g.byte() & 0x7f]);
    s.extend_from_slice(&[1, 0xb0, 7, g.byte() & 0x7f]);
    // Loop start
    s.extend_from_slice(&[0, 0xb0, 0x14, 0]);
    // Note on ch 0 + pitch bend + ch 9 percussion + unknown CC + note off
    let n0 = 48 + (g.byte() % 24);
    s.extend_from_slice(&[0, 0x90, n0, 64 + (g.byte() & 0x3f)]);
    s.extend_from_slice(&[1, 0xe0, g.byte() & 0x7f, g.byte() & 0x7f]);
    s.extend_from_slice(&[0, 0x99, 36 + (g.byte() % 12), 100]);
    s.extend_from_slice(&[1, 0xb0, 0x40, 0x7f]); // unknown CC
    s.extend_from_slice(&[0, 0x80, n0, 0]);
    // Loop end with repeat count 0..2
    s.extend_from_slice(&[1, 0xb0, 0x1e, g.below(3) as u8]);
    // Wait extension + EOT
    s.extend_from_slice(&[2, 0xff, 0xf8]);
    s.extend_from_slice(&[0, 0xff, 0x2f, 0]);
    s
}

#[test]
fn sndmidi_api_and_sequencer_match_original() {
    let Some(mut r) = runner() else { return };
    let r_oracle = Rc::new(RefCell::new(Oracle::new(1)));
    let p_oracle = Rc::new(RefCell::new(Oracle::new(1)));
    hook_sndmidi(&mut r, r_oracle.clone());
    r.mem.write(ARENA, &vec![0u8; ARENA_SIZE]);

    let mut m = Mem {
        regions: vec![
            (GLOBALS_BASE, vec![0u8; GLOBALS_SIZE]),
            (ARENA, vec![0u8; ARENA_SIZE]),
            (SP_SCRATCH, vec![0u8; SP_SIZE]),
        ],
    };

    let mut g = Rng(0x2d41_0800_cafe_babe);
    let mut streams_at = Vec::new();
    let mut off = 0x400u32;
    for k in 0..24 {
        let bytes = make_stream(&mut g, k);
        r.mem.write(ARENA + off, &bytes);
        for (j, &b) in bytes.iter().enumerate() {
            m.w8(ARENA + off + j as u32, b);
        }
        streams_at.push(ARENA + off);
        off += ((bytes.len() as u32) + 0x10) & !0xf;
    }

    // Seed both sides identically with SndMidi_Init
    let sync_oracles = |seed: u64, ro: &Rc<RefCell<Oracle>>, po: &Rc<RefCell<Oracle>>| {
        *ro.borrow_mut() = Oracle::new(seed);
        *po.borrow_mut() = Oracle::new(seed);
    };
    let check_eq = |tag: &str, r: &mut Runner, m: &Mem, ro: &Rc<RefCell<Oracle>>, po: &Rc<RefCell<Oracle>>| {
        assert_eq!(ro.borrow().log, po.borrow().log, "{tag}: calls");
        assert_eq!(read_globals(r), port_globals(m), "{tag}: globals");
    };

    // Randomize globals first to make sure Init clears everything identically
    let dirty: Vec<u8> = (0..GLOBALS_SIZE).map(|_| g.byte()).collect();
    write_globals(&mut r, &mut m, &dirty);

    sync_oracles(0x100, &r_oracle, &p_oracle);
    r.call(0x002d_4228).unwrap();
    let mut penv = PortEnv(p_oracle.clone());
    let ret_p = init(&mut m, &mut penv);
    assert_eq!(r.cpu.gpr(2) as u32, ret_p, "init ret");
    check_eq("init", &mut r, &m, &r_oracle, &p_oracle);

    let mut handles: Vec<i32> = vec![-1, 0, 0x10, 0x25, 0x7fff_ffe0];
    for step in 0..800u32 {
        let seed = 0x1000 + step as u64;
        sync_oracles(seed, &r_oracle, &p_oracle);
        let op = g.below(13);
        let h = handles[g.below(handles.len() as u32) as usize];
        match op {
            // Play (also hits >16 busy slots and NEXT_GEN wrap)
            0 | 1 => {
                if step == 50 {
                    // Test NEXT_GEN negative wrap branch
                    let wrap = 0x7fff_ffe0u32;
                    r.mem.write_u32(NEXT_GEN, wrap);
                    m.w32(NEXT_GEN, wrap);
                }
                let opts_at = ARENA + (g.below(8) * 0x20);
                let mut ob = [0u8; 24];
                for b in &mut ob {
                    *b = g.byte();
                }
                ob[0] = g.below(128) as u8;
                let speed: u16 = [0x1000, 0x0800, 0x2000, 0x1800, 0x0400][g.below(5) as usize];
                ob[0x10..0x12].copy_from_slice(&speed.to_le_bytes());
                r.mem.write(opts_at, &ob);
                for (k, &b) in ob.iter().enumerate() {
                    m.w8(opts_at + k as u32, b);
                }
                let data = streams_at[g.below(streams_at.len() as u32) as usize];
                let bank = g.below(6) as i16;
                let drum = [-1i16, 2, 4][g.below(3) as usize];
                r.cpu.set_gpr(4, opts_at as u64);
                r.cpu.set_gpr(5, data as u64);
                r.cpu.set_gpr(6, bank as i64 as u64);
                r.cpu.set_gpr(7, drum as i64 as u64);
                r.call(0x002d_4440).unwrap();
                let rr = r.cpu.gpr(2) as i32;
                let rp = play(&mut m, opts_at, data, bank, drum);
                assert_eq!(rr, rp, "step {step} play ret");
                if rp >= 0 {
                    handles.push(rp);
                    if handles.len() > 32 {
                        handles.remove(5);
                    }
                }
                check_eq(&format!("step {step} play"), &mut r, &m, &r_oracle, &p_oracle);
            }
            // Tick
            2 | 3 | 4 => {
                r.call(0x002d_4108).unwrap();
                tick(&mut m, &mut penv, SP_SCRATCH);
                check_eq(&format!("step {step} tick"), &mut r, &m, &r_oracle, &p_oracle);
            }
            // Validate + GetState
            5 => {
                r.cpu.set_gpr(4, h as i64 as u64);
                r.call(0x002d_37e8).unwrap();
                assert_eq!(r.cpu.gpr(2) as i32, validate(&m, h), "step {step} validate");
                r.cpu.set_gpr(4, h as i64 as u64);
                r.call(0x002d_4a08).unwrap();
                assert_eq!(r.cpu.gpr(2) as i32, get_state(&m, h), "step {step} get_state");
            }
            // SetVolume
            6 => {
                let vol = g.below(128) as i32;
                r.cpu.set_gpr(4, h as i64 as u64);
                r.cpu.set_gpr(5, vol as i64 as u64);
                r.call(0x002d_4888).unwrap();
                let rp = set_volume(&mut m, &mut penv, h, vol);
                assert_eq!(r.cpu.gpr(2) as i32, rp, "step {step} set_volume");
                check_eq(&format!("step {step} set_volume"), &mut r, &m, &r_oracle, &p_oracle);
            }
            // Fade
            7 => {
                let ticks = [-5i32, 0, 1, 2, 5, 10][g.below(6) as usize];
                let target = [-1i32, 0, 32, 96, 127][g.below(5) as usize];
                r.cpu.set_gpr(4, h as i64 as u64);
                r.cpu.set_gpr(5, ticks as i64 as u64);
                r.cpu.set_gpr(6, target as i64 as u64);
                r.call(0x002d_4978).unwrap();
                let rp = fade(&mut m, h, ticks, target);
                assert_eq!(r.cpu.gpr(2) as i32, rp, "step {step} fade");
                check_eq(&format!("step {step} fade"), &mut r, &m, &r_oracle, &p_oracle);
            }
            // SetPitch
            8 => {
                let pitch = [0x0800u32, 0x1000, 0x1800, 0x2000][g.below(4) as usize];
                r.cpu.set_gpr(4, h as i64 as u64);
                r.cpu.set_gpr(5, pitch as u64);
                r.call(0x002d_45a8).unwrap();
                let rp = set_pitch(&mut m, &mut penv, h, pitch);
                assert_eq!(r.cpu.gpr(2) as i32, rp, "step {step} set_pitch");
                check_eq(&format!("step {step} set_pitch"), &mut r, &m, &r_oracle, &p_oracle);
            }
            // SetPan
            9 => {
                let pan = [0u32, 0x2000, 0xc000, 0x3fff][g.below(4) as usize];
                r.cpu.set_gpr(4, h as i64 as u64);
                r.cpu.set_gpr(5, pan as u64);
                r.call(0x002d_4660).unwrap();
                let rp = set_pan(&mut m, &mut penv, h, pan);
                assert_eq!(r.cpu.gpr(2) as i32, rp, "step {step} set_pan");
                check_eq(&format!("step {step} set_pan"), &mut r, &m, &r_oracle, &p_oracle);
            }
            // Pause / Resume
            10 => {
                if g.below(2) == 0 {
                    r.cpu.set_gpr(4, h as i64 as u64);
                    r.call(0x002d_4740).unwrap();
                    let rp = pause(&mut m, &mut penv, h);
                    assert_eq!(r.cpu.gpr(2) as i32, rp, "step {step} pause");
                    check_eq(&format!("step {step} pause"), &mut r, &m, &r_oracle, &p_oracle);
                } else {
                    r.cpu.set_gpr(4, h as i64 as u64);
                    r.call(0x002d_4808).unwrap();
                    let rp = resume(&mut m, &mut penv, h);
                    assert_eq!(r.cpu.gpr(2) as i32, rp, "step {step} resume");
                    check_eq(&format!("step {step} resume"), &mut r, &m, &r_oracle, &p_oracle);
                }
            }
            // Stop
            11 => {
                r.cpu.set_gpr(4, h as i64 as u64);
                r.call(0x002d_4368).unwrap();
                let rp = stop(&mut m, &mut penv, h);
                assert_eq!(r.cpu.gpr(2) as i32, rp, "step {step} stop");
                check_eq(&format!("step {step} stop"), &mut r, &m, &r_oracle, &p_oracle);
            }
            // StopAll / Shutdown + re-Init occasionally
            _ => {
                if g.below(4) == 0 {
                    r.call(0x002d_42c8).unwrap();
                    let rp = shutdown(&mut m, &mut penv);
                    assert_eq!(r.cpu.gpr(2) as u32, rp, "step {step} shutdown");
                    check_eq(&format!("step {step} shutdown"), &mut r, &m, &r_oracle, &p_oracle);
                    sync_oracles(seed + 9999, &r_oracle, &p_oracle);
                    r.call(0x002d_4228).unwrap();
                    init(&mut m, &mut penv);
                    check_eq(&format!("step {step} reinit"), &mut r, &m, &r_oracle, &p_oracle);
                } else {
                    r.call(0x002d_4308).unwrap();
                    let rp = stop_all(&mut m, &mut penv);
                    assert_eq!(r.cpu.gpr(2) as u32, rp, "step {step} stop_all");
                    check_eq(&format!("step {step} stop_all"), &mut r, &m, &r_oracle, &p_oracle);
                }
            }
        }
    }
}

#[test]
fn midisys_matches_original() {
    let Some(mut r) = runner() else { return };
    let r_oracle = Rc::new(RefCell::new(Oracle::new(1)));
    let p_oracle = Rc::new(RefCell::new(Oracle::new(1)));

    // Hook external calls made by MidiSys_Init / ~cMidiSys / PushEvent / PlayPending / PlayEventCallback
    for &(addr, nargs) in &[
        (BANK_SYS_INIT, 1usize),
        (BANK_SYS_DTOR, 2),
        (SND_MIDI_INIT_ADDR, 0),
        (SND_MIDI_SHUTDOWN_ADDR, 0),
        (EAMEM_ALLOC, 3),
        (EAMEM_SET, 3),
        (MEM_FREE, 1),
        (SND_LOCK, 1),
        (SND_UNLOCK, 0),
        (SND_MIDI_PLAY_ADDR, 4),
        (BANK_MONITOR_ADD_INSTANCE, 4),
        (BANK_INSTANCE_SET_COPY1, 3),
    ] {
        let o = r_oracle.clone();
        r.hook(addr, move |cpu, _mem| {
            let mut args = Vec::with_capacity(nargs);
            for k in 0..nargs {
                args.push(cpu.gpr(4 + k) as u32);
            }
            let ret = o.borrow_mut().handle_call(addr, &args, None);
            cpu.set_gpr(2, ret as i32 as i64 as u64);
            true
        });
    }
    // SndSys_ScheduleEvent(a0=sys, a1=delay, a2=64bit cb, a3=slot, t0=kind, t1=file, t2=vol, t3=0)
    {
        let o = r_oracle.clone();
        r.hook(SND_SYS_SCHEDULE_EVENT, move |cpu, _mem| {
            let cb = cpu.gpr(6);
            let args = vec![
                cpu.gpr(4) as u32,
                cpu.gpr(5) as u32,
                cb as u32,
                (cb >> 32) as u32,
                cpu.gpr(7) as u32,
                cpu.gpr(8) as u32,
                cpu.gpr(9) as u32,
                cpu.gpr(10) as u32,
                cpu.gpr(11) as u32,
            ];
            let ret = o.borrow_mut().handle_call(SND_SYS_SCHEDULE_EVENT, &args, None);
            cpu.set_gpr(2, ret as i64 as u64);
            true
        });
    }
    r.allow(MEMSET);
    r.allow(0x0022_2160); // MidiSys_PushEvent
    r.allow(0x0022_2358); // MidiSys_PlayPending

    const SYS: u32 = ARENA;
    const SYS_SIZE: usize = 0x1000;
    const TABLES: u32 = ARENA + 0x1000;
    const TABLES_SIZE: usize = 0x1000;
    const EV_AT: u32 = ARENA + 0x2400;
    const SCALE_AT: u32 = ARENA + 0x2500;
    const BANKS_AT: u32 = ARENA + 0x2600;
    const REGION_SIZE: usize = 0x3000;

    // Read the 8-byte echo callback descriptor from the ELF at EV_MIDI_ECHO
    let mut ev_desc = [0u8; 8];
    r.mem.read(EV_MIDI_ECHO, &mut ev_desc).unwrap();

    let mut g = Rng(0x2223_5800_dead_beef);
    let mut penv = PortEnv(p_oracle.clone());

    let sync = |r: &mut Runner, m: &mut Mem, buf: &[u8], g_midisys: u32| {
        r.mem.write(ARENA, buf);
        r.mem.write_u32(MIDI_SYS_GLOBAL, g_midisys);
        m.regions.clear();
        m.regions.push((ARENA, buf.to_vec()));
        m.regions.push((0x0034_3000, vec![0u8; 0x200]));
        m.regions.push((0x0039_fd00, vec![0u8; 0x100]));
        m.w32(MIDI_SYS_GLOBAL, g_midisys);
        for (k, &b) in ev_desc.iter().enumerate() {
            m.w8(EV_MIDI_ECHO + k as u32, b);
        }
    };
    let cmp_arena = |tag: &str, r: &mut Runner, m: &Mem, ro: &Rc<RefCell<Oracle>>, po: &Rc<RefCell<Oracle>>| {
        assert_eq!(ro.borrow().log, po.borrow().log, "{tag}: calls");
        let mut rb = vec![0u8; REGION_SIZE];
        r.mem.read(ARENA, &mut rb).unwrap();
        let pb: Vec<u8> = (0..REGION_SIZE).map(|k| m.r8(ARENA + k as u32)).collect();
        assert_eq!(rb, pb, "{tag}: arena");
        let mut rg = [0u8; 4];
        r.mem.read(MIDI_SYS_GLOBAL, &mut rg).unwrap();
        assert_eq!(u32::from_le_bytes(rg), m.r32(MIDI_SYS_GLOBAL), "{tag}: gMidiSys");
    };

    let mut m = Mem { regions: vec![] };

    // 1. MidiSys_Init & ~cMidiSys
    for case in 0..40u32 {
        let mut buf = vec![0u8; REGION_SIZE];
        for b in &mut buf[..SYS_SIZE] {
            *b = g.byte();
        }
        sync(&mut r, &mut m, &buf, 0x1234_5678);
        *r_oracle.borrow_mut() = Oracle::new(case as u64 + 1);
        *p_oracle.borrow_mut() = Oracle::new(case as u64 + 1);
        r.cpu.set_gpr(4, SYS as u64);
        r.call(0x0022_2080).unwrap();
        let ret_p = midisys_init(&mut m, &mut penv, SYS);
        assert_eq!(r.cpu.gpr(2) as u32, ret_p, "init {case} ret");
        cmp_arena(&format!("midisys_init {case}"), &mut r, &m, &r_oracle, &p_oracle);

        *r_oracle.borrow_mut() = Oracle::new(case as u64 + 100);
        *p_oracle.borrow_mut() = Oracle::new(case as u64 + 100);
        let flags = g.below(4);
        r.cpu.set_gpr(4, SYS as u64);
        r.cpu.set_gpr(5, flags as u64);
        r.call(0x0022_2100).unwrap();
        midisys_dtor(&mut m, &mut penv, SYS, flags);
        cmp_arena(&format!("midisys_dtor {case}"), &mut r, &m, &r_oracle, &p_oracle);
    }

    // 2. MidiSys_PushEvent, MidiSys_PlayPending, MidiSys_PlayEventCallback
    for case in 0..400u32 {
        let mut buf = vec![0u8; REGION_SIZE];
        let w32 = |buf: &mut [u8], addr: u32, v: u32| {
            let o = (addr - ARENA) as usize;
            buf[o..o + 4].copy_from_slice(&v.to_le_bytes());
        };
        // Play options header at SYS+8..+0x20
        for k in 0..24usize {
            buf[8 + k] = g.byte();
        }
        // Queue index 0..2
        let qi = g.below(3);
        w32(&mut buf, SYS + 0x20, qi);
        for (off, ptr) in [
            (0x24u32, TABLES),
            (0x38, TABLES + 0x100),
            (0x3c, TABLES + 0x140),
            (0x40, TABLES + 0x180),
            (0x64, TABLES + 0x1c0),
            (0x68, TABLES + 0x200),
            (0x6c, TABLES + 0x240),
            (0x70, TABLES + 0x280),
            (0x74, TABLES + 0x2c0),
            (0x78, TABLES + 0x340),
            (0x7c, TABLES + 0x380),
            (0xa3c, BANKS_AT),
            (0xa48, TABLES + 0x400),
        ] {
            w32(&mut buf, SYS + off, ptr);
        }
        // Bank table (4 slots * 0x60)
        for bk in 0..4u32 {
            let rec = BANKS_AT + bk * 0x60;
            w32(&mut buf, rec, [0, 1, 2][g.below(3) as usize]);
            w32(&mut buf, rec + 4, [0, 1, 3, u32::MAX][g.below(4) as usize]);
        }
        // Slot scale float
        let scale: f32 = [-0.5, 0.0, 0.5, 1.0, 1.5, 2.5][g.below(6) as usize];
        w32(&mut buf, SCALE_AT, scale.to_bits());

        // Populate current queue slot qi for PlayPending
        let emitter = if g.below(4) == 0 { ARENA + 0x2540 } else { 0 };
        w32(&mut buf, SYS + 0x28 + qi * 4, emitter);
        let vol = [-10i32, 0, 50, 100, 127, 200][g.below(6) as usize] as u32;
        w32(&mut buf, TABLES + 0x140 + qi * 4, vol);
        let slot_ptr = if g.below(3) == 0 { 0 } else { SCALE_AT };
        w32(&mut buf, TABLES + 0x180 + qi * 4, slot_ptr);
        let cull: f32 = [0.0, -0.0, 1.0, 40.0][g.below(4) as usize];
        w32(&mut buf, TABLES + 0x280 + qi * 4, cull.to_bits());
        let row = TABLES + 0x2c0 + qi * 0x10;
        w32(&mut buf, row, [50, 100, 250][g.below(3) as usize]);
        w32(&mut buf, row + 4, g.below(4)); // delay count 0..3
        w32(&mut buf, row + 8, [0, 50, 90, 120][g.below(4) as usize]);
        w32(&mut buf, row + 0x0c, [0, 50, 100, 150][g.below(4) as usize]);
        let bk = g.below(4);
        let fh = 0x5000 + g.below(0x100);
        w32(&mut buf, TABLES + 0x400 + qi * 8, bk);
        w32(&mut buf, TABLES + 0x400 + qi * 8 + 4, fh);

        // Event record for PlayEventCallback
        w32(&mut buf, EV_AT + 0x18, slot_ptr);
        w32(&mut buf, EV_AT + 0x1c, g.below(4));
        w32(&mut buf, EV_AT + 0x20, 0x6000 + g.below(0x100));
        w32(&mut buf, EV_AT + 0x24, [-5i32, 0, 64, 127, 150][g.below(5) as usize] as u32);

        sync(&mut r, &mut m, &buf, SYS);
        *r_oracle.borrow_mut() = Oracle::new(0x5000 + case as u64);
        *p_oracle.borrow_mut() = Oracle::new(0x5000 + case as u64);

        match case % 3 {
            0 => {
                let kind = g.below(4);
                let file = 0x4000 + g.below(0x100);
                r.cpu.set_gpr(4, SYS as u64);
                r.cpu.set_gpr(5, kind as u64);
                r.cpu.set_gpr(6, file as u64);
                r.call(0x0022_2160).unwrap();
                midisys_push_event(&mut m, &mut penv, SYS, kind, file);
                cmp_arena(&format!("push_event {case}"), &mut r, &m, &r_oracle, &p_oracle);
            }
            1 => {
                r.cpu.set_gpr(4, SYS as u64);
                r.call(0x0022_2358).unwrap();
                let rp = midisys_play_pending(&mut m, &mut penv, SYS);
                assert_eq!(r.cpu.gpr(2) as u32, rp, "play_pending {case} ret");
                cmp_arena(&format!("play_pending {case}"), &mut r, &m, &r_oracle, &p_oracle);
            }
            _ => {
                r.cpu.set_gpr(4, SYS as u64);
                r.cpu.set_gpr(5, EV_AT as u64);
                r.call(0x0022_2798).unwrap();
                let rp = midisys_play_event_callback(&mut m, &mut penv, EV_AT);
                assert_eq!(r.cpu.gpr(2) as u32, rp, "play_event_cb {case} ret");
                cmp_arena(&format!("play_event_cb {case}"), &mut r, &m, &r_oracle, &p_oracle);
            }
        }
    }
    let _ = TABLES_SIZE;
}

#[test]
fn disc_eam_files_run_bit_exact_against_original() {
    let Some(mut r) = runner() else { return };
    let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };

    let disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
    let mut files = tricky_data::big::Files::new(disc);
    files.mount_game_archives().expect("mount");
    let inf = files.load("data/config/crowd.inf").expect("load").expect("crowd.inf missing");
    let (sets, _) = tricky_data::audiocfg::crowd_config(&inf, b"GARI");
    let mut names = std::collections::BTreeSet::new();
    for set in &sets {
        for list in [&set.trick, &set.fall, &set.anticipate] {
            for raw in list {
                names.insert(String::from_utf8_lossy(raw).into_owned());
            }
        }
    }

    let r_oracle = Rc::new(RefCell::new(Oracle::new(7)));
    let p_oracle = Rc::new(RefCell::new(Oracle::new(7)));
    r_oracle.borrow_mut().deterministic_voices = true;
    p_oracle.borrow_mut().deterministic_voices = true;
    hook_sndmidi(&mut r, r_oracle.clone());

    let mut m = Mem {
        regions: vec![
            (GLOBALS_BASE, vec![0u8; GLOBALS_SIZE]),
            (ARENA, vec![0u8; ARENA_SIZE]),
            (SP_SCRATCH, vec![0u8; SP_SIZE]),
        ],
    };
    let mut penv = PortEnv(p_oracle.clone());

    for path in &names {
        let bytes = files.load(path).expect("load eam").unwrap_or_else(|| panic!("missing {path}"));
        assert!(bytes.len() <= 0x1000);
        r_oracle.borrow_mut().log.clear();
        p_oracle.borrow_mut().log.clear();
        r_oracle.borrow_mut().next_voice = 1;
        p_oracle.borrow_mut().next_voice = 1;
        r_oracle.borrow_mut().live_voices.clear();
        p_oracle.borrow_mut().live_voices.clear();

        write_globals(&mut r, &mut m, &vec![0u8; GLOBALS_SIZE]);
        r.call(0x002d_4228).unwrap();
        init(&mut m, &mut penv);

        let opts_at = ARENA;
        let data_at = ARENA + 0x100;
        r.mem.write(data_at, &bytes);
        for (k, &b) in bytes.iter().enumerate() {
            m.w8(data_at + k as u32, b);
        }
        r.cpu.set_gpr(4, opts_at as u64);
        r.call(0x002d_4a60).unwrap();
        play_opts_init(&mut m, opts_at);

        r.cpu.set_gpr(4, opts_at as u64);
        r.cpu.set_gpr(5, data_at as u64);
        r.cpu.set_gpr(6, 3); // Crowd.bnk handle = 3
        r.cpu.set_gpr(7, (-1i32) as i64 as u64);
        r.call(0x002d_4440).unwrap();
        let hr = r.cpu.gpr(2) as i32;
        let hp = play(&mut m, opts_at, data_at, 3, -1);
        assert_eq!(hr, hp, "{path} play handle");
        assert!(hp >= 0, "{path} failed to start");

        // Run at 100 Hz until EOT finishes (all 9 crowd files are ~2000 ticks = 20s)
        let mut ticks = 0u32;
        while get_state(&m, hp) >= 0 && ticks < 2500 {
            r.call(0x002d_4108).unwrap();
            tick(&mut m, &mut penv, SP_SCRATCH);
            ticks += 1;
        }
        assert!(ticks > 1500 && ticks < 2500, "{path} unexpected tick count {ticks}");
        assert_eq!(get_state(&m, hp), -8, "{path} did not stop cleanly at EOT");
        assert_eq!(r_oracle.borrow().log, p_oracle.borrow().log, "{path} call log");
        assert_eq!(read_globals(&mut r), port_globals(&m), "{path} globals");
        // Confirm at least one NoteOn (SND_PlayBankSound) and NoteOff (SndHandle_Release) ran
        assert!(
            p_oracle.borrow().log.iter().any(|c| c.target == SND_PLAY_BANK_SOUND),
            "{path} played no notes"
        );
        assert!(
            p_oracle.borrow().log.iter().any(|c| c.target == SND_HANDLE_RELEASE),
            "{path} released no notes"
        );
    }
}
