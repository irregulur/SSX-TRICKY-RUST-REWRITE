//! The crowd boxes hosted (board rows E1b2g7d3b7a, E1b2g7d3b7b2): the audio system's `+0x372c` crowd boxes
//! (`Audio_LoadCrowd` 0x2209c8, 400 = `0x190` bytes each) running the ported, runner-checked [`crate::crowdbox`]
//! and [`crate::sndmidi`] functions in engine memory (`pf::Mem`) from `DATA/CONFIG/CROWD.INF`
//! (`tricky_data::audiocfg::crowd_config`) and the disc's `.eam` files (`DATA/AUDIO/AUDIO.BIG`).
//!
//! `Audio_PlayCrowdCheer` 0x221068 (`play_cheer`), `CrowdSys_StartBoxCheer` 0x221088 (`start_cheer`),
//! `Audio_ArmCrowdBox` 0x2210a8 (`arm`), `Audio_StopCrowdBox` 0x2210c8 (`stop`), and `CrowdSys_UpdateBoxes`
//! 0x220968 (`update`) run the game's exact box state machine, queue MIDI events through `MidiSys_PushEvent`
//! 0x222160 / `MidiSys_PlayPending` 0x222358 -> `SndMidi_Play` 0x2d4440, and step the 100 Hz sequencer
//! (`SndMidi_Tick` 0x2d4108) to emit [`MidiVoiceAction`]s for `Crowd.bnk`.

use crate::crowdbox::{
    self as cb, BOX_UPDATE, ENTRY_BEGIN, ENTRY_PLAY, MEMSET, MONITOR_FIND, MONITOR_SET, OPEN_FILE, PLAY_PENDING, RAND,
    VOICE_STOP,
};
use crate::musicsong::SongEnv;
use crate::pf::Mem;
use crate::sndlib;
use crate::sndmidi::{
    self as sm, BANK_INSTANCE_SET_COPY1, BANK_MONITOR_ADD_INSTANCE, EV_MIDI_ECHO, MIDI_SYS_GLOBAL, SND_ADD_TICK_CB,
    SND_HANDLE_RELEASE, SND_HANDLE_SET_PAN, SND_HANDLE_SET_PITCH, SND_HANDLE_SET_PITCH_BEND, SND_HANDLE_SET_VOLUME,
    SND_HANDLE_STOP, SND_LOCK, SND_MIDI_PLAY_ADDR, SND_PLAY_BANK_SOUND, SND_REMOVE_TICK_CB, SND_SYS_SCHEDULE_EVENT,
    SND_UNLOCK, SND_VOICE_VALIDATE,
};
use crate::sndqueue;
use crate::speechlib::Mem as LibMem;
use crate::speechload::LoadEnv;
use crate::worldemitter;
use std::collections::{HashMap, HashSet};

const AUDIO_GLOBAL: u32 = 0x0034_3120;
const AUDIO_VTABLE: u32 = 0x0039_f200;
const REQ_WORDS_AT: u32 = 0x0039_fbd8;
/// Exact 8 bytes at `0x39fbd8` in `SLUS_203.26` copied into every crowd voice request's `+0x44` row.
const REQ_WORDS: [u8; 8] = [0x00, 0x00, 0xff, 0xff, 0xf0, 0x13, 0x22, 0x00];
/// Exact 8 bytes at `0x39fd80` in `SLUS_203.26` (`MidiSys_PlayEventCallback` 0x222798 member pointer).
const EV_MIDI_ECHO_BYTES: [u8; 8] = [0x00, 0x00, 0xff, 0xff, 0x98, 0x27, 0x22, 0x00];
/// `cBXAudio::vf15` 0x20ea38 (vtable `+0x84`, `this` shift `+0x80` = 0): anticipation level 0..2 or 3 (none).
const VF15_ANTICIPATE: u32 = 0x0020_ea38;

const AUDIO_AT: u32 = 0x0108_0000;
const AUDIO_SIZE: usize = 0x4000;
const TABLES: u32 = 0x010c_0000;
const TABLES_SIZE: usize = 0x4000;
const SP: u32 = 0x010d_0000;
const SP_SIZE: usize = 0x200;
const EAM_ARENA: u32 = 0x0110_0000;
const EAM_ARENA_SIZE: usize = 0x10000;

const Q_RECS: u32 = TABLES;
const Q_38: u32 = TABLES + 0x100;
const Q_3C: u32 = TABLES + 0x140;
const Q_40: u32 = TABLES + 0x180;
const Q_64: u32 = TABLES + 0x1c0;
const Q_68: u32 = TABLES + 0x200;
const Q_6C: u32 = TABLES + 0x240;
const Q_70: u32 = TABLES + 0x280;
const Q_74: u32 = TABLES + 0x2c0;
const Q_78: u32 = TABLES + 0x340;
const Q_7C: u32 = TABLES + 0x380;
const Q_A3C: u32 = TABLES + 0x0500; // bank slots table (8 * 0x60 = 0x300)
const Q_A40: u32 = TABLES + 0x3c0;
const Q_A44: u32 = TABLES + 0x400;
const Q_A48: u32 = TABLES + 0x440;
const BOXES_AT: u32 = TABLES + 0x0800;
const MAX_BOXES: usize = 4;
const STR_SCRATCH: u32 = TABLES + 0x1000;
const MON_SCRATCH: u32 = TABLES + 0x2000;
const EMITTERS: u32 = TABLES + 0x2200;
const MAX_EMITTERS: usize = 64;

/// High-level crowd box events (`CrowdBox_*` -> `MidiSys_*` / `SndSys_*`), kept for diagnostics and tests.
#[derive(Clone, Debug, PartialEq)]
pub enum CrowdAction {
    /// `MidiSys_PushEvent` 0x222160 + `MidiSys_PlayPending` 0x222358: play cheer MIDI file `file` on `bank_kind`.
    PlayCheer { bank_kind: u32, file: String, volume: u32, voice: i32 },
    /// `SndSys_PlayPending` 0x20c948: start a crowd bed sound (`id` in `bank_kind`).
    PlaySound { bank_kind: u32, id: u32, volume: u32, voice: u32 },
    /// `SndSys_StopBankSound` 0x20c2a0: stop `voice` on `engine` (1 bank sound, 2 MIDI) with `fade`.
    StopVoice { voice: i32, engine: u32, fade: f32 },
    /// `BankInstance_SetPitch` 0x20ad38: update playing cheer pitch scale (`0x1000` = 1.0).
    SetPitch { voice: i32, scale: u32 },
}

/// Low-level `lib-snd` voice commands emitted by `SndMidi_*` (0x2d37e8..0x2d4a08) for the host audio mixer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidiVoiceAction {
    /// `SND_PlayBankSound` 0x2d1718 called from `SndMidi_NoteOn` 0x2d39c8.
    Start {
        handle: i32,
        bank: i16,
        program: i32,
        note: u8,
        volume: u8,
        velocity: u8,
        pan: u16,
        pitch: u16,
        bend: u8,
    },
    /// `SndHandle_Release` 0x2dfda0 called from `SndMidi_NoteOff` 0x2d3b90.
    Release { handle: i32 },
    /// `SndHandle_Stop` 0x2d3260 called from `SndMidi_Stop` 0x2d4368 / `SndMidi_Pause` 0x2d4740.
    Stop { handle: i32 },
    /// `SndHandle_SetVolume` 0x2d3108 called from `SndMidi_SetVolume` / `UpdateFade` / CC 7.
    SetVolume { handle: i32, volume: i32 },
    /// `SndHandle_SetPitch` 0x2d31b8 called from `SndMidi_SetPitch` 0x2d45a8.
    SetPitch { handle: i32, pitch: u16 },
    /// `SndHandle_SetPitchBend` 0x2d6a18 called from `SndMidi_OnPitchBend` 0x2d3e18.
    SetPitchBend { handle: i32, bend: u8 },
    /// `SndHandle_SetPan` 0x2d3480 called from `SndMidi_SetPan` / CC 10.
    SetPan { handle: i32, pan: u16 },
}

/// `cBXAudio::vf15` 0x20ea38: given the listener rider's time to land (`rider+0x57b8`) and trick multiplier
/// (`Score_GetMultiplier` 0x155410 on `rider+0x5820`), returns the anticipation tier `0..=2` or `3` (none).
pub fn anticipate_level(t_land: f32, mult: f32) -> u32 {
    if t_land <= 2.0 && 0.0 < mult {
        if mult < 0.25 {
            0
        } else if mult < 0.5 {
            1
        } else {
            2
        }
    } else {
        3
    }
}

/// Playback speed multiplier for a bank voice using `SNDVoice_CalcPitchMult` 0x2d5288 / `0x2d51f8`
/// (`crate::sndlib::pitch_ratio`): combines MIDI note vs `root` (100 cents/semitone), `detune` cents,
/// `bend` (`0..=127`, center `64`) * `bend_range` semitones, and master `pitch_scale` (`0x1000` = 1.0).
pub fn pitch_speed(root: u8, note: u8, detune: i32, bend_range: u8, bend: u8, pitch_scale: u16) -> f32 {
    let base_cents = (note as i32 - root as i32) * 100 + detune;
    let bend_cents = ((bend as i32 - 64) * (bend_range as i32) * 100) >> 6;
    let ratio = sndlib::pitch_ratio(base_cents + bend_cents);
    let mult = ((ratio as u32).wrapping_mul(pitch_scale as u32) as i32) >> 12;
    (mult.max(1) as f32) / 4096.0
}

/// Hosted `cCrowdSys` (`audio+0x3728..+0x382c`), its `cCrowdBox` array (`audio+0x372c`), and EA's `SndMidi`
/// 100 Hz sequencer (`0x3507e8..0x351200`).
pub struct CrowdHost {
    mem: Mem,
    st: CrowdState,
    next_emitter: usize,
    loaded: bool,
}

#[derive(Default)]
struct CrowdState {
    /// Handle (`EAM_ARENA` pointer when real `.eam` bytes loaded, or synthetic `1..` id) -> file path string.
    files: HashMap<u32, String>,
    /// File path -> cached `EAM_ARENA` pointer when already loaded.
    file_ptrs: HashMap<String, u32>,
    /// Next free byte offset inside `EAM_ARENA`.
    eam_bump: u32,
    /// Next synthetic file handle when no loader bytes are available.
    next_synth_file: u32,
    /// Active engine-1 (bank bed sound) voice handles.
    active_bank: HashSet<u32>,
    next_bank_voice: u32,
    /// Active `lib-snd` voice handles allocated by `SndMidi_NoteOn` -> `SND_PlayBankSound`.
    live_midi_voices: HashSet<i32>,
    next_midi_voice: i32,
    actions: Vec<CrowdAction>,
    voice_actions: Vec<MidiVoiceAction>,
}

impl CrowdState {
    fn midi_env(&mut self) -> MidiEnv<'_> {
        MidiEnv { st: self }
    }
}

struct MidiEnv<'a> {
    st: &'a mut CrowdState,
}

impl SongEnv for MidiEnv<'_> {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        match target {
            SND_LOCK | SND_UNLOCK | SND_ADD_TICK_CB | SND_REMOVE_TICK_CB | BANK_INSTANCE_SET_COPY1
            | SND_SYS_SCHEDULE_EVENT => 0,
            BANK_MONITOR_ADD_INSTANCE => 0,
            MEMSET => {
                let (dst, val, len) = (args[0], args[1] as u8, args[2]);
                for k in 0..len {
                    m.w8(dst.wrapping_add(k), val);
                }
                dst
            }
            SND_MIDI_PLAY_ADDR => {
                let (opts, data, bank, drum) = (args[0], args[1], args[2] as i16, args[3] as i16);
                if data >= EAM_ARENA && data < EAM_ARENA + EAM_ARENA_SIZE as u32 {
                    sm::play(m, opts, data, bank, drum) as u32
                } else {
                    // Synthetic test without `.eam` bytes: return a non-negative handle without starting SndMidi.
                    self.st.next_bank_voice += 1;
                    self.st.next_bank_voice
                }
            }
            SND_VOICE_VALIDATE => {
                let h = args[0] as i32;
                if self.st.live_midi_voices.contains(&h) {
                    h as u32
                } else {
                    u32::MAX
                }
            }
            SND_PLAY_BANK_SOUND => {
                let (bank, program, opts) = (args[0] as i16, args[1] as i32, args[2]);
                let volume = m.r8(opts);
                let bend = m.r8(opts + 1);
                let note = m.r8(opts + 3);
                let velocity = m.r8(opts + 4);
                let pan = m.r16(opts + 8);
                let pitch = m.r16(opts + 0x0c);
                self.st.next_midi_voice += 1;
                let handle = self.st.next_midi_voice;
                self.st.live_midi_voices.insert(handle);
                self.st.voice_actions.push(MidiVoiceAction::Start {
                    handle,
                    bank,
                    program,
                    note,
                    volume,
                    velocity,
                    pan,
                    pitch,
                    bend,
                });
                handle as u32
            }
            SND_HANDLE_RELEASE => {
                let handle = args[0] as i32;
                self.st.live_midi_voices.remove(&handle);
                self.st.voice_actions.push(MidiVoiceAction::Release { handle });
                0
            }
            SND_HANDLE_STOP => {
                let handle = args[0] as i32;
                self.st.live_midi_voices.remove(&handle);
                self.st.voice_actions.push(MidiVoiceAction::Stop { handle });
                0
            }
            SND_HANDLE_SET_VOLUME => {
                self.st.voice_actions.push(MidiVoiceAction::SetVolume {
                    handle: args[0] as i32,
                    volume: args[1] as i32,
                });
                0
            }
            SND_HANDLE_SET_PITCH => {
                self.st.voice_actions.push(MidiVoiceAction::SetPitch {
                    handle: args[0] as i32,
                    pitch: args[1] as u16,
                });
                0
            }
            SND_HANDLE_SET_PITCH_BEND => {
                self.st.voice_actions.push(MidiVoiceAction::SetPitchBend {
                    handle: args[0] as i32,
                    bend: args[1] as u8,
                });
                0
            }
            SND_HANDLE_SET_PAN => {
                self.st.voice_actions.push(MidiVoiceAction::SetPan {
                    handle: args[0] as i32,
                    pan: args[1] as u16,
                });
                0
            }
            other => panic!("crowd midi env: unexpected call {other:#x}"),
        }
    }

    fn call_f(&mut self, _: &mut Mem, target: u32, _: &[u32]) -> f32 {
        panic!("crowd midi env: unexpected call_f {target:#x}")
    }

    fn call_out(&mut self, _: &mut Mem, target: u32, _: &[u32], _: usize) -> Vec<u32> {
        panic!("crowd midi env: unexpected call_out {target:#x}")
    }
}

impl Default for CrowdHost {
    fn default() -> Self {
        Self::new()
    }
}

struct HostEnv<'a, F: FnMut(u32) -> u32, L: FnMut(&str) -> Option<Vec<u8>>> {
    st: &'a mut CrowdState,
    anticipate: F,
    loader: L,
}

fn cstr(m: &dyn LibMem, at: u32) -> String {
    let bytes: Vec<u8> = (0..0x200).map(|k| m.r8(at + k)).take_while(|&b| b != 0).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn put_cstr(m: &mut Mem, at: u32, s: &[u8]) {
    for (k, &b) in s.iter().enumerate() {
        m.w8(at + k as u32, b);
    }
    m.w8(at + s.len() as u32, 0);
}

fn is_eam_ptr(ptr: u32) -> bool {
    ptr >= EAM_ARENA && ptr < EAM_ARENA + EAM_ARENA_SIZE as u32
}

impl<F: FnMut(u32) -> u32, L: FnMut(&str) -> Option<Vec<u8>>> LoadEnv for HostEnv<'_, F, L> {
    fn call(&mut self, m: &mut dyn LibMem, target: u32, args: &[u32]) -> u32 {
        match target {
            OPEN_FILE => {
                let name = cstr(m, args[0]);
                if name.is_empty() {
                    return 0;
                }
                if let Some(&ptr) = self.st.file_ptrs.get(&name) {
                    return ptr;
                }
                let handle = if let Some(bytes) = (self.loader)(&name) {
                    let aligned = (self.st.eam_bump + 3) & !3;
                    if (aligned as usize) + bytes.len() <= EAM_ARENA_SIZE {
                        let ptr = EAM_ARENA + aligned;
                        for (k, &b) in bytes.iter().enumerate() {
                            m.w8(ptr + k as u32, b);
                        }
                        self.st.eam_bump = aligned + bytes.len() as u32;
                        ptr
                    } else {
                        self.st.next_synth_file += 1;
                        self.st.next_synth_file
                    }
                } else {
                    self.st.next_synth_file += 1;
                    self.st.next_synth_file
                };
                self.st.files.insert(handle, name.clone());
                self.st.file_ptrs.insert(name, handle);
                handle
            }
            RAND => crate::rand::rand_fx(),
            MEMSET => {
                let (dst, val, len) = (args[0], args[1] as u8, args[2]);
                for k in 0..len {
                    m.w8(dst.wrapping_add(k), val);
                }
                dst
            }
            // `SndSys_StopBankSound` 0x20c2a0(fade, audio, voice, engine)
            VOICE_STOP => {
                let (voice, engine, fade) = (args[1] as i32, args[2], f32::from_bits(args[3]));
                if engine == 1 {
                    self.st.active_bank.remove(&(voice as u32));
                } else if engine == 2 {
                    let slot = midi_validate_lib(m, voice);
                    if slot >= 0 {
                        if fade == 0.0 {
                            midi_stop_lib(m, self.st, voice);
                        } else {
                            // `BankInstance_Stop` 0x20ba00: `SndMidi_Fade(voice, (int)(fade * 100.0), -1)`
                            midi_fade_lib(m, voice, (fade * 100.0) as i32, -1);
                        }
                    }
                }
                self.st.actions.push(CrowdAction::StopVoice { voice, engine, fade });
                0
            }
            // `BankMonitor_Find` 0x20c200(audio, voice, engine)
            MONITOR_FIND => {
                let (voice, engine) = (args[1] as i32, args[2]);
                let found = if engine == 1 {
                    self.st.active_bank.contains(&(voice as u32))
                } else if engine == 2 {
                    midi_validate_lib(m, voice) >= 0 || (voice > 0 && self.st.active_bank.contains(&(voice as u32)))
                } else {
                    false
                };
                if found {
                    m.w32(MON_SCRATCH, engine);
                    m.w32(MON_SCRATCH + 4, voice as u32);
                    MON_SCRATCH
                } else {
                    0
                }
            }
            // `BankInstance_SetPitch` 0x20ad38(inst, scale)
            MONITOR_SET => {
                let engine = m.r32(args[0]);
                let voice = m.r32(args[0].wrapping_add(4)) as i32;
                let scale = (args[1] as i32).clamp(0, 0x3fff) as u32;
                if engine == 2 && midi_validate_lib(m, voice) >= 0 {
                    midi_set_pitch_lib(m, self.st, voice, scale);
                }
                self.st.actions.push(CrowdAction::SetPitch { voice, scale });
                0
            }
            // `MidiSys_PushEvent` 0x222160(audio, bank_kind, file_handle)
            ENTRY_BEGIN => {
                let (a, bank_kind, file) = (args[0], args[1], args[2]);
                let i = m.r32(a + 0x20).wrapping_add(1);
                m.w32(a + 0x20, i);
                let rec = m.r32(a + 0x24).wrapping_add(i.wrapping_mul(0x18));
                for k in 0..0x18 {
                    let b = m.r8(a + 8 + k);
                    m.w8(rec.wrapping_add(k), b);
                }
                m.w32(a.wrapping_add(0x28).wrapping_add(i.wrapping_mul(4)), 0);
                m.w32(m.r32(a + 0x38).wrapping_add(i.wrapping_mul(4)), 0);
                let first = m.r8(rec) as i8 as i32 as u32;
                m.w32(m.r32(a + 0x3c).wrapping_add(i.wrapping_mul(4)), first);
                m.w32(m.r32(a + 0x40).wrapping_add(i.wrapping_mul(4)), 0);
                for k in 0..8 {
                    m.w8(a.wrapping_add(0x44).wrapping_add(i.wrapping_mul(8)).wrapping_add(k), 0);
                }
                for off in [0x64, 0x68, 0x6c, 0x70, 0x78] {
                    m.w32(m.r32(a + off).wrapping_add(i.wrapping_mul(4)), 0);
                }
                let row = m.r32(a + 0x74).wrapping_add(i.wrapping_mul(16));
                m.w32(row, 100);
                m.w32(row + 4, 0);
                m.w32(row + 8, 0x5a);
                m.w32(row + 0x0c, 0x32);
                m.w32(m.r32(a + 0x7c).wrapping_add(i.wrapping_mul(4)), 1);
                let midi = m.r32(a + 0xa48).wrapping_add(i.wrapping_mul(8));
                m.w32(midi, bank_kind);
                m.w32(midi + 4, file);
                0
            }
            // `MidiSys_PlayPending` 0x222358(audio) -> `SndMidi_Play` 0x2d4440
            ENTRY_PLAY => {
                let a = args[0];
                let i = m.r32(a + 0x20);
                let rec = m.r32(a + 0x24).wrapping_add(i.wrapping_mul(0x18));
                let volume = m.r32(m.r32(a + 0x3c).wrapping_add(i.wrapping_mul(4)));
                let midi = m.r32(a + 0xa48).wrapping_add(i.wrapping_mul(8));
                let bank_kind = m.r32(midi);
                let file_h = m.r32(midi + 4);
                let bank_rec = bank_kind.wrapping_mul(0x60).wrapping_add(m.r32(a + 0xa3c));
                let bank_h = if m.r32(bank_rec) == 1 { m.r32(bank_rec + 4) as i16 } else { -1 };
                let voice = if is_eam_ptr(file_h) && bank_h >= 0 {
                    midi_play_lib(m, rec, file_h, bank_h, -1)
                } else {
                    self.st.next_bank_voice += 1;
                    let v = self.st.next_bank_voice;
                    self.st.active_bank.insert(v);
                    v as i32
                };
                m.w32(a + 0x20, i.wrapping_sub(1));
                let file = self.st.files.get(&file_h).cloned().unwrap_or_default();
                self.st.actions.push(CrowdAction::PlayCheer { bank_kind, file, volume, voice });
                voice as u32
            }
            // `SndSys_PlayPending` 0x20c948(audio)
            PLAY_PENDING => {
                let a = args[0];
                let i = m.r32(a + 0x20);
                let bank_kind = m.r32(m.r32(a + 0xa40).wrapping_add(i.wrapping_mul(4)));
                let id = m.r32(m.r32(a + 0xa44).wrapping_add(i.wrapping_mul(4)));
                let volume = m.r32(m.r32(a + 0x3c).wrapping_add(i.wrapping_mul(4)));
                m.w32(a + 0x20, i.wrapping_sub(1));
                self.st.next_bank_voice += 1;
                let voice = self.st.next_bank_voice;
                self.st.active_bank.insert(voice);
                self.st.actions.push(CrowdAction::PlaySound { bank_kind, id, volume, voice });
                voice
            }
            BOX_UPDATE => {
                cb::update(m, self, args[0], SP);
                0
            }
            VF15_ANTICIPATE => (self.anticipate)(args[1]),
            other => panic!("crowd host: unexpected call {other:#x}"),
        }
    }
}

// Thin `LibMem` forwards into the exact `sndmidi` globals at `0x3507e8..0x351200` (matching `sndmidi.rs`).
fn midi_validate_lib(m: &dyn LibMem, handle: i32) -> i32 {
    if handle < 0 {
        return -8;
    }
    let idx = (handle as u32) & 0x1f;
    if idx < sm::SLOT_COUNT as u32 {
        let s = sm::SLOTS_AT + idx * sm::SLOT_SIZE;
        if m.r32(s + 0x10) == handle as u32 {
            return idx as i32;
        }
    }
    -8
}

fn midi_play_lib(m: &mut dyn LibMem, opts: u32, mut data: u32, bank: i16, drum_bank: i16) -> i32 {
    if m.r16(data) as i16 == 0x444d {
        data = data.wrapping_add(8);
    } else if m.r32(data) == 0x7844_494d {
        data = data.wrapping_add(4);
    }
    let gen = (m.r32(sm::NEXT_GEN) as i32).wrapping_add(0x20);
    m.w32(sm::NEXT_GEN, if gen < 0 { 0 } else { gen as u32 });
    for i in 0..sm::SLOT_COUNT {
        let s = sm::SLOTS_AT + (i as u32) * sm::SLOT_SIZE;
        if (m.r32(s + 0x10) as i32) < 0 {
            m.w32(s + 4, 0);
            m.w32(s, 0);
            let tempo = (((m.r16(opts + 0x10) as u32).wrapping_mul(100)) as i32 >> 12) as i16;
            m.w16(s + 0x24, tempo as u16);
            let h = m.r32(sm::NEXT_GEN) | (i as u32);
            m.w32(s + 8, data);
            m.w16(s + 0x2c, bank as u16);
            m.w32(s + 0x10, h);
            m.w16(s + 0x2e, drum_bank as u16);
            m.w8(s + 0x26, 0xff);
            m.w8(s + 0x27, 1);
            m.w32(s + 0x18, 0);
            m.w32(s + 0x0c, 0);
            let vol = m.r8(opts) as i8 as i32;
            m.w8(s + 0x28, 3);
            m.w32(s + 0x14, (vol << 16) as u32);
            m.w16(s + 0x20, m.r16(opts + 8));
            m.w16(s + 0x22, m.r16(opts + 0x0c));
            for ch in 0..16u32 {
                let c = s + 0x30 + ch * 4;
                let w = ((m.r32(c) & 0xffff_fc00) | 0x0001_fc00) & 0xff01_ffff | 0x0080_0000;
                m.w32(c, w);
                m.w8(c + 3, 0xff);
            }
            return h as i32;
        }
    }
    -9
}

fn midi_stop_lib(m: &mut dyn LibMem, st: &mut CrowdState, handle: i32) {
    let slot = midi_validate_lib(m, handle);
    if slot < 0 {
        return;
    }
    let s = sm::SLOTS_AT + (slot as u32) * sm::SLOT_SIZE;
    if m.r8(s + 0x28) as i8 == 0 {
        return;
    }
    for i in 0..sm::VOICE_COUNT {
        let v = sm::VOICES_AT + (i as u32) * 8;
        if m.r8(v + 4) as i8 as i32 == slot {
            let vh = m.r32(v) as i32;
            if vh >= 0 {
                st.live_midi_voices.remove(&vh);
                st.voice_actions.push(MidiVoiceAction::Stop { handle: vh });
            }
            m.w8(v + 5, 0xff);
            m.w8(v + 6, 0);
            m.w8(v + 7, 0);
            m.w32(v, u32::MAX);
        }
    }
    m.w8(s + 0x28, 2);
    m.w32(s + 0x10, u32::MAX);
}

fn midi_fade_lib(m: &mut dyn LibMem, handle: i32, ticks: i32, target: i32) {
    let slot = midi_validate_lib(m, handle);
    if slot < 0 {
        return;
    }
    let s = sm::SLOTS_AT + (slot as u32) * sm::SLOT_SIZE;
    if m.r8(s + 0x28) as i8 == 3 {
        let n = if ticks < 1 { 1 } else { ticks };
        let goal = target.wrapping_shl(16);
        let cur = m.r32(s + 0x14) as i32;
        m.w32(s + 0x1c, goal as u32);
        m.w32(s + 0x18, (goal.wrapping_sub(cur) / n) as u32);
    }
}

fn midi_set_pitch_lib(m: &mut dyn LibMem, st: &mut CrowdState, handle: i32, pitch: u32) {
    let slot = midi_validate_lib(m, handle);
    if slot < 0 {
        return;
    }
    let s = sm::SLOTS_AT + (slot as u32) * sm::SLOT_SIZE;
    if pitch != m.r16(s + 0x22) as u32 && m.r8(s + 0x28) as i8 == 3 {
        m.w16(s + 0x22, pitch as u16);
        for i in 0..sm::VOICE_COUNT {
            let v = sm::VOICES_AT + (i as u32) * 8;
            if m.r8(v + 4) as i8 as i32 == slot {
                let vh = m.r32(v) as i32;
                if vh >= 0 {
                    st.voice_actions.push(MidiVoiceAction::SetPitch { handle: vh, pitch: pitch as u16 });
                }
            }
        }
    }
}

impl CrowdHost {
    pub fn new() -> Self {
        let mut mem = Mem {
            regions: vec![
                (0x0034_3000, vec![0; 0x1000]),
                (0x0035_07e0, vec![0; 0x0a20]),
                (0x0039_f000, vec![0; 0x1000]),
                (AUDIO_AT, vec![0; AUDIO_SIZE]),
                (TABLES, vec![0; TABLES_SIZE]),
                (SP, vec![0; SP_SIZE]),
                (EAM_ARENA, vec![0; EAM_ARENA_SIZE]),
            ],
        };
        mem.w32(AUDIO_GLOBAL, AUDIO_AT);
        mem.w32(MIDI_SYS_GLOBAL, AUDIO_AT);
        for (k, &b) in REQ_WORDS.iter().enumerate() {
            mem.w8(REQ_WORDS_AT + k as u32, b);
        }
        for (k, &b) in EV_MIDI_ECHO_BYTES.iter().enumerate() {
            mem.w8(EV_MIDI_ECHO + k as u32, b);
        }
        mem.w16(AUDIO_VTABLE + 0x80, 0);
        mem.w32(AUDIO_VTABLE + 0x84, VF15_ANTICIPATE);
        mem.w32(AUDIO_AT + 4, AUDIO_VTABLE);
        let hdr = sndqueue::play_opts_header();
        for (k, &b) in hdr.iter().enumerate() {
            mem.w8(AUDIO_AT + 8 + k as u32, b);
        }
        mem.w32(AUDIO_AT + 0x20, u32::MAX);
        for (off, ptr) in [
            (0x24, Q_RECS), (0x38, Q_38), (0x3c, Q_3C), (0x40, Q_40), (0x64, Q_64), (0x68, Q_68),
            (0x6c, Q_6C), (0x70, Q_70), (0x74, Q_74), (0x78, Q_78), (0x7c, Q_7C),
            (0xa3c, Q_A3C), (0xa40, Q_A40), (0xa44, Q_A44), (0xa48, Q_A48),
        ] {
            mem.w32(AUDIO_AT + off, ptr);
        }
        let mut st = CrowdState::default();
        let mut menv = st.midi_env();
        sm::init(&mut mem, &mut menv);
        CrowdHost {
            mem,
            st,
            next_emitter: 0,
            loaded: false,
        }
    }

    /// `Audio_LoadCrowd` 0x2209c8 without disc `.eam` files (keeps cheer actions logged, no `SndMidi` voice events).
    pub fn load_crowd(&mut self, crowd_inf: &[u8], course: &[u8], count: usize, bank_kind: u32) {
        self.load_crowd_with(crowd_inf, course, count, bank_kind, |_| None);
    }

    /// `Audio_LoadCrowd` 0x2209c8 (from `Audio_LoadInGameConfig` 0x211098): create `count` crowd boxes (400 bytes
    /// each at `audio+0x372c`, bank kind `bank_kind` = 3), mark bank slot `bank_kind` loaded (`state = 1`,
    /// handle = `bank_kind`), and load `CROWD.INF` (`[GLOBAL]` then `[course]`) plus each `.eam` file via `loader`.
    pub fn load_crowd_with(
        &mut self,
        crowd_inf: &[u8],
        course: &[u8],
        count: usize,
        bank_kind: u32,
        mut loader: impl FnMut(&str) -> Option<Vec<u8>>,
    ) {
        let count = count.min(MAX_BOXES);
        {
            let mut menv = self.st.midi_env();
            sm::stop_all(&mut self.mem, &mut menv);
            sm::init(&mut self.mem, &mut menv);
        }
        self.st.files.clear();
        self.st.file_ptrs.clear();
        self.st.eam_bump = 0;
        self.st.next_synth_file = 0;
        self.st.active_bank.clear();
        self.st.live_midi_voices.clear();
        self.next_emitter = 0;
        self.mem.w32(AUDIO_AT + 0x20, u32::MAX);
        // Mark the crowd bank slot loaded (`state = 1`, bank handle = `bank_kind`)
        let bank_rec = Q_A3C + bank_kind * 0x60;
        self.mem.w32(bank_rec, 1);
        self.mem.w32(bank_rec + 4, bank_kind);

        self.mem.w32(AUDIO_AT + 0x3728, count as u32);
        self.mem.w32(AUDIO_AT + 0x372c, if count == 0 { 0 } else { BOXES_AT });
        for i in 0..count {
            let b = BOXES_AT + (i as u32) * 0x190;
            cb::box_init(&mut self.mem, b);
            self.mem.w32(b + 0xd8, bank_kind);
        }
        let (sets, _) = tricky_data::audiocfg::crowd_config(crowd_inf, course);
        for (n, set) in sets.iter().enumerate() {
            let row = AUDIO_AT + 0x3730 + (n as u32) * 0x54;
            for (off, list) in [(0x00u32, &set.trick), (0x18, &set.fall), (0x30, &set.anticipate)] {
                let l = row + off;
                cb::list_construct(&mut self.mem, l);
                for name in list {
                    put_cstr(&mut self.mem, STR_SCRATCH, name);
                    let mut env = HostEnv { st: &mut self.st, anticipate: |_| 3, loader: &mut loader };
                    cb::list_add(&mut self.mem, &mut env, l, STR_SCRATCH, 0);
                }
            }
            self.mem.w32(row + 0x48, set.attack.to_bits());
            self.mem.w32(row + 0x4c, set.decay.to_bits());
            self.mem.w32(row + 0x50, set.max_bend.to_bits());
        }
        self.loaded = true;
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Number of loaded files in set `n` (`0..3`): `(trick, fall, anticipate)`.
    pub fn set_counts(&self, n: usize) -> (u32, u32, u32) {
        let row = AUDIO_AT + 0x3730 + (n as u32) * 0x54;
        (self.mem.r32(row), self.mem.r32(row + 0x18), self.mem.r32(row + 0x30))
    }

    /// Inspect box `who`: `(slots_used +0xd4, cheer_state +0x180, cheer_level +0x184, cheer_voice +0x174)`.
    pub fn box_state(&self, who: u32) -> Option<(u32, u32, u32, i32)> {
        let b = self.box_addr(who)?;
        Some((
            self.mem.r32(b + 0xd4),
            self.mem.r32(b + 0x180),
            self.mem.r32(b + 0x184),
            self.mem.r32(b + 0x174) as i32,
        ))
    }

    fn box_addr(&self, who: u32) -> Option<u32> {
        let boxes = self.mem.r32(AUDIO_AT + 0x372c);
        let count = self.mem.r32(AUDIO_AT + 0x3728);
        if boxes == 0 || who >= count {
            None
        } else {
            Some(boxes + who * 0x190)
        }
    }

    /// `Audio_PlayCrowdCheer` 0x221068(audio, level, who) -> `CrowdBox_PlayCheer` 0x2203a8.
    pub fn play_cheer(&mut self, level: u32, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = HostEnv { st: &mut self.st, anticipate: |_| 3, loader: |_| None };
        cb::play_cheer(&mut self.mem, &mut env, b, level as i32);
    }

    /// `CrowdSys_StartBoxCheer` 0x221088(audio, level, who) -> `CrowdBox_StartCheer` 0x220538.
    pub fn start_cheer(&mut self, level: u32, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = HostEnv { st: &mut self.st, anticipate: |_| 3, loader: |_| None };
        cb::start_cheer(&mut self.mem, &mut env, b, level as i32);
    }

    /// `Audio_ArmCrowdBox` 0x2210a8(audio, who) -> `CrowdBox_Arm` 0x2206c8.
    pub fn arm(&mut self, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        cb::arm(&mut self.mem, b);
    }

    /// `Audio_StopCrowdBox` 0x2210c8(audio, who) -> `CrowdBox_Stop` 0x2206e0.
    pub fn stop(&mut self, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = HostEnv { st: &mut self.st, anticipate: |_| 3, loader: |_| None };
        cb::stop(&mut self.mem, &mut env, b);
    }

    /// `CrowdBox_StopAll` 0x220338 + `SndMidi_StopAll` 0x2d4308 on every box (when a run ends or restarts).
    pub fn stop_all(&mut self, fade: f32) {
        let count = self.mem.r32(AUDIO_AT + 0x3728);
        for who in 0..count {
            if let Some(b) = self.box_addr(who) {
                let mut env = HostEnv { st: &mut self.st, anticipate: |_| 3, loader: |_| None };
                cb::stop_all(&mut self.mem, &mut env, b, fade);
                cb::stop(&mut self.mem, &mut env, b);
            }
        }
        let mut menv = self.st.midi_env();
        sm::stop_all(&mut self.mem, &mut menv);
    }

    /// `CrowdSys_UpdateBoxes` 0x220968 (from `Audio_UpdateSystems` 0x21eb70): ticks every crowd box (`CrowdBox_Update`
    /// 0x21efb0), calling `anticipate(listener)` for `cBXAudio::vf15` 0x20ea38 when a box is armed.
    pub fn update(&mut self, anticipate: impl FnMut(u32) -> u32) {
        if !self.loaded {
            return;
        }
        let mut env = HostEnv { st: &mut self.st, anticipate, loader: |_| None };
        cb::update_boxes(&mut self.mem, &mut env, AUDIO_AT);
    }

    /// Step EA's MIDI sequencer (`SndMidi_Tick` 0x2d4108) `ticks_100hz` times at 100 Hz.
    pub fn tick_midi(&mut self, ticks_100hz: u32) {
        if !self.loaded || ticks_100hz == 0 {
            return;
        }
        let mut menv = self.st.midi_env();
        for _ in 0..ticks_100hz {
            sm::tick(&mut self.mem, &mut menv, SP);
        }
    }

    /// Inform `SndVoice_Validate` 0x2d50f0 that voice `handle` has finished playing (so EOT can stop its MIDI slot).
    pub fn mark_voice_done(&mut self, handle: i32) {
        self.st.live_midi_voices.remove(&handle);
    }

    /// `SndQueue_StartLastVoice` 0x2212a0 -> `CrowdBox_StartSound` 0x21fc98: join a world crowd emitter at `pos`
    /// (cm) with initial volume `vol` (0..127) and sound `id` to box `who`; returns its slot index (or -1 if full).
    pub fn start_sound(&mut self, who: u32, pos: [f32; 3], vol: u32, id: u32) -> i32 {
        let Some(b) = self.box_addr(who) else { return -1 };
        let em = EMITTERS + ((self.next_emitter % MAX_EMITTERS) as u32) * 0x10;
        self.next_emitter += 1;
        for (k, &v) in [pos[0], pos[1], pos[2], 1.0].iter().enumerate() {
            self.mem.w32(em + 4 * k as u32, v.to_bits());
        }
        let i = self.mem.r32(AUDIO_AT + 0x20).wrapping_add(1);
        self.mem.w32(AUDIO_AT + 0x20, i);
        self.mem.w32(AUDIO_AT + 0x28 + i * 4, em);
        self.mem.w32(Q_3C + i * 4, vol);
        self.mem.w32(Q_40 + i * 4, 0);
        self.mem.w32(Q_68 + i * 4, who);
        let mut env = HostEnv { st: &mut self.st, anticipate: |_| 3, loader: |_| None };
        let slot = cb::start_sound(&mut self.mem, &mut env, b, AUDIO_AT, id);
        self.mem.w32(AUDIO_AT + 0x20, i.wrapping_sub(1));
        slot
    }

    /// `Audio_SetVoiceVolume` 0x221000 -> `CrowdBox_SetSlotValue` 0x21fc68.
    pub fn set_slot_value(&mut self, who: u32, slot: i32, vol: u32) {
        let Some(b) = self.box_addr(who) else { return };
        cb::set_slot_value(&mut self.mem, b, slot, vol);
    }

    /// `CrowdBox_StopSlot` 0x2201f8.
    pub fn stop_slot(&mut self, who: u32, slot: i32, fade: f32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = HostEnv { st: &mut self.st, anticipate: |_| 3, loader: |_| None };
        cb::stop_slot(&mut self.mem, &mut env, b, slot, fade);
    }

    /// Drain high-level crowd box actions since the last call.
    pub fn take_actions(&mut self) -> Vec<CrowdAction> {
        std::mem::take(&mut self.st.actions)
    }

    /// Drain `SndMidi` -> `lib-snd` voice actions emitted since the last call.
    pub fn take_voice_actions(&mut self) -> Vec<MidiVoiceAction> {
        std::mem::take(&mut self.st.voice_actions)
    }
}

// ---- World crowd emitters (`WorldEmitter_GatherForListener` 0x22b370 / `WorldEmitter_Update` 0x22bf60 keys 0x61..=0x63, row E1b2g7d3b7c) ----

/// `SoundId_ToBankProgram` 0x22d8a8 / `WorldEmitter_IsSpecialKey` 0x22c638:
/// crowd keys `0x61..=0x63` map to bank kind 3 (`Crowd.bnk`) programs `0..=2`.
pub fn crowd_key_to_program(key: u32) -> Option<u32> {
    match key {
        0x61 => Some(0),
        0x62 => Some(1),
        0x63 => Some(2),
        _ => None,
    }
}

/// One bound `.adl` crowd emitter shape (`key` in `0x61..=0x63`) attached to a `.pbd` instance.
#[derive(Clone, Debug)]
pub struct CrowdEmitter {
    pub inst: u32,
    pub shape_idx: u32,
    pub key: u32,
    pub kind: u32,
    pub words: Vec<u32>,
    /// Instance world position (`inst+0x30`, cm, game Z-up axes).
    pub inst_pos: [f32; 3],
}

/// `AudioTGroups_Bind` 0x22a5f0 + `Course_FindInstanceByHash` 0x2550c8 + `WorldEmitter_IsSpecialKey` 0x22c638:
/// bind `.adl` entries to `.pbd` instances (`pbd.hash.lists[1]`) and keep all crowd emitters (`0x61..=0x63`).
pub fn extract_crowd_emitters(pbd: &tricky_data::pbd::Pbd, adl: &tricky_data::adl::Adl) -> Vec<CrowdEmitter> {
    let Some(ht) = &pbd.hash else { return Vec::new() };
    let by_hash: HashMap<u32, u32> = ht.lists[1].iter().copied().collect();
    let mut out = Vec::new();
    for &(hash, zone_off) in &adl.entries {
        let Some(&inst_idx) = by_hash.get(&hash) else { continue };
        let Some(inst) = pbd.instances.get(inst_idx as usize) else { continue };
        let Some(zone) = adl.zone(zone_off) else { continue };
        let inst_pos = [
            f32::from_bits(inst[12]),
            f32::from_bits(inst[13]),
            f32::from_bits(inst[14]),
        ];
        for (shape_idx, s) in zone.shapes.iter().enumerate() {
            let Some(&key) = s.words.get(1) else { continue };
            if crowd_key_to_program(key).is_some() {
                out.push(CrowdEmitter {
                    inst: inst_idx,
                    shape_idx: shape_idx as u32,
                    key,
                    kind: s.kind,
                    words: s.words.clone(),
                    inst_pos,
                });
            }
        }
    }
    out
}

/// `WorldEmitter_GatherForListener` 0x22b370 + `WorldEmitter_Falloff` 0x22bc40:
/// test `listener_cm` against `e`'s shape (`0` sphere, `1` box, `2` cone, `3` radius) and return
/// `Some((falloff_0_to_1, center_cm))` when inside.
pub fn eval_crowd_emitter(e: &CrowdEmitter, listener_cm: [f32; 3]) -> Option<(f32, [f32; 3])> {
    let f = |k: usize| e.words.get(k).copied().map(f32::from_bits);
    let c = [e.inst_pos[0] + f(2)?, e.inst_pos[1] + f(3)?, e.inst_pos[2] + f(4)?];
    let d = [listener_cm[0] - c[0], listener_cm[1] - c[1], listener_cm[2] - c[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    match e.kind {
        0 => {
            let dist = dot(d, d).sqrt().abs();
            let rad = f(5)?;
            (dist < rad).then(|| (worldemitter::falloff(f(6).unwrap_or(2.0), dist / rad, 0.0), c))
        }
        1 => {
            let a = [f(8)?, f(9)?, f(10)?];
            let (r0, r1, r2) = (-a[1], a[0], 0.0f32);
            let q = (r0 * r0 + r1 * r1 + r2 * r2).sqrt();
            if q == 0.0 {
                return None;
            }
            let inv = 1.0 / q;
            let n = [r0 * inv, r1 * inv, r2 * inv];
            let s = [a[1] * n[2] - a[2] * n[1], a[2] * n[0] - a[0] * n[2], a[0] * n[1] - a[1] * n[0]];
            let (e5, e6, e7) = (f(5)?, f(6)?, f(7)?);
            let ta = if e5 == 0.0 { 0.0 } else { let v = dot(a, d); (v * v) / (e5 * e5) };
            let tn = if e6 == 0.0 { 0.0 } else { let v = dot(n, d); (v * v) / (e6 * e6) };
            let ts = if e7 == 0.0 { 0.0 } else { let v = dot(s, d); (v * v) / (e7 * e7) };
            let total = ta + tn + ts;
            (total < 1.0).then(|| (worldemitter::falloff(f(11).unwrap_or(2.0), total, 0.0), c))
        }
        2 => {
            let dist = dot(d, d).sqrt().abs();
            let rad = f(5)?;
            if dist >= rad || dist == 0.0 {
                return None;
            }
            let inv = 1.0 / dist;
            let dn = [d[0] * inv, d[1] * inv, d[2] * inv];
            let dir = [f(6)?, f(7)?, f(8)?];
            let cosine = dot(dn, dir);
            let cone = f(10)?;
            if cone >= cosine {
                return None;
            }
            let half_pi = f32::from_bits(0x3fc9_0fdb);
            let f1 = worldemitter::falloff(f(9).unwrap_or(2.0), dist / rad, 0.0);
            let wide = half_pi - crate::math::asin(cone);
            let ang = half_pi - crate::math::asin(cosine);
            let f2 = worldemitter::falloff(f(11).unwrap_or(2.0), ang / wide, 0.0);
            Some((f1 * f2, c))
        }
        3 => {
            let dist = dot(d, d).sqrt().abs();
            let rad = f(5)?;
            (dist < rad).then_some((1.0, c))
        }
        _ => None,
    }
}

/// Active world crowd emitter tracker (`WorldEmitter_UpdateAll` 0x22d370 / `WorldEmitter_Update` 0x22bf60 /
/// `WorldEmitter_Stop` 0x22c5a0 for keys `0x61..=0x63`) wired into [`CrowdHost`].
#[derive(Default)]
pub struct WorldCrowdTrack {
    emitters: Vec<CrowdEmitter>,
    /// Active `(inst, shape_idx)` -> `CrowdBox` slot index (`0..15`).
    active: HashMap<(u32, u32), i32>,
}

impl WorldCrowdTrack {
    pub fn load(&mut self, emitters: Vec<CrowdEmitter>) {
        self.emitters = emitters;
        self.active.clear();
    }

    pub fn emitters(&self) -> &[CrowdEmitter] {
        &self.emitters
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Clear active emitter slots (e.g. when `CrowdHost::stop_all` resets all boxes).
    pub fn reset(&mut self) {
        self.active.clear();
    }

    /// `Boarder_GatherWorldEmitters` 0x1166f0 -> `WorldEmitter_UpdateAll` 0x22d370 -> `WorldEmitter_Update` 0x22bf60
    /// for keys `0x61..=0x63`: joins in-range crowd emitters to `crowd` box `who` (`CrowdBox_StartSound` 0x21fc98),
    /// updates volumes (`Audio_SetVoiceVolume` 0x221000), and stops out-of-range emitters (`WorldEmitter_StopById`
    /// 0x220f98 with fade `0.25` s -> `CrowdBox_StopSlot` 0x2201f8).
    pub fn tick(&mut self, crowd: &mut CrowdHost, listener_cm: [f32; 3], who: u32) {
        if !crowd.is_loaded() {
            return;
        }
        let mut seen: HashMap<(u32, u32), (f32, [f32; 3], u32)> = HashMap::new();
        for e in &self.emitters {
            if let Some((vol_f, center)) = eval_crowd_emitter(e, listener_cm) {
                let id = (e.inst, e.shape_idx);
                seen.entry(id)
                    .and_modify(|prev| {
                        if vol_f > prev.0 {
                            *prev = (vol_f, center, e.key);
                        }
                    })
                    .or_insert((vol_f, center, e.key));
            }
        }
        // Stop emitters no longer wanted (`WorldEmitter_Stop` 0x22c5a0 -> `WorldEmitter_StopById` 0x220f98, fade 0.25).
        let gone: Vec<(u32, u32)> = self.active.keys().filter(|k| !seen.contains_key(k)).copied().collect();
        for k in gone {
            if let Some(slot) = self.active.remove(&k) {
                crowd.stop_slot(who, slot, 0.25);
            }
        }
        // Start new or update active emitters (`SndQueue_StartLastVoice` 0x2212a0 / `Audio_SetVoiceVolume` 0x221000).
        for (id, (vol_f, center, key)) in seen {
            let vol = (vol_f * 127.0).clamp(0.0, 127.0) as u32;
            if let Some(&slot) = self.active.get(&id) {
                crowd.set_slot_value(who, slot, vol);
            } else if let Some(prog) = crowd_key_to_program(key) {
                let slot = crowd.start_sound(who, center, vol, prog);
                if slot >= 0 {
                    self.active.insert(id, slot);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CROWD_INF: &[u8] = b"\
[GLOBAL]\n\
TRICK1 = \"|data\\audio\\Cheer10.eam\"\n\
TRICK2 = \"|data\\audio\\Cheer20.eam\"\n\
TRICK3 = \"|data\\audio\\Cheer30.eam\"\n\
FALL1 = \"|data\\audio\\Ahh10.eam\"\n\
FALL2 = \"|data\\audio\\Ahh20.eam\"\n\
FALL3 = \"|data\\audio\\Ahh30.eam\"\n\
ANTICIPATE1 = \"|data\\audio\\Anticipation10.eam\"\n\
ANTICIPATE2 = \"|data\\audio\\Anticipation20.eam\"\n\
ANTICIPATE3 = \"|data\\audio\\Anticipation30.eam\"\n\
ATTACK1 = 0.25\n\
DECAY1 = 0.5\n\
MAXBEND1 = 1.0\n";

    #[test]
    fn crowd_host_cheer_wipeout_and_anticipation_flow() {
        crate::rand::streams().seed_fx(1);
        let mut h = CrowdHost::new();
        h.load_crowd(SAMPLE_CROWD_INF, b"GARI", 1, 3);
        assert!(h.is_loaded());
        assert_eq!(h.set_counts(0), (1, 1, 1));
        assert_eq!(h.set_counts(1), (1, 1, 1));
        assert_eq!(h.set_counts(2), (1, 1, 1));

        // With no crowd emitters in box 0 (+0xd4 == 0), PlayCheer is a no-op (exact game rule).
        h.play_cheer(2, 0);
        assert!(h.take_actions().is_empty());

        // Join one crowd emitter (as `WorldEmitter_Update` -> `SndQueue_StartLastVoice` does).
        let slot = h.start_sound(0, [100.0, 200.0, 300.0], 96, 0x61);
        assert_eq!(slot, 0);
        let acts = h.take_actions();
        assert!(matches!(acts.as_slice(), [CrowdAction::PlaySound { bank_kind: 3, id: 0x61, volume: 96, .. }]));

        // `CrowdSys_UpdateBoxes` updates box+0x178 loudness to 96.
        h.update(|_| 3);
        assert!(h.take_actions().is_empty());

        // `Audio_PlayCrowdCheer` (level 2, box 0) plays TRICK3 (Cheer30.eam).
        h.play_cheer(2, 0);
        let acts = h.take_actions();
        assert!(matches!(
            acts.as_slice(),
            [CrowdAction::PlayCheer { bank_kind: 3, file, volume: 96, .. }] if file == "|data\\audio\\Cheer30.eam"
        ));

        // `CrowdSys_StartBoxCheer` (wipeout level 1) stops the previous cheer and plays FALL2 (Ahh20.eam).
        h.start_cheer(1, 0);
        let acts = h.take_actions();
        assert_eq!(acts.len(), 2);
        assert!(matches!(acts[0], CrowdAction::StopVoice { engine: 2, .. }));
        assert!(matches!(
            &acts[1],
            CrowdAction::PlayCheer { bank_kind: 3, file, volume: 96, .. } if file == "|data\\audio\\Ahh20.eam"
        ));

        // `Audio_ArmCrowdBox` + `CrowdSys_UpdateBoxes` with vf15 returning tier 2 plays ANTICIPATE3.
        h.stop(0);
        let _ = h.take_actions();
        h.arm(0);
        h.update(|_| anticipate_level(1.2, 0.75));
        let acts = h.take_actions();
        assert!(acts.iter().any(|a| matches!(
            a,
            CrowdAction::PlayCheer { bank_kind: 3, file, .. } if file == "|data\\audio\\Anticipation30.eam"
        )));
        assert_eq!(h.box_state(0).map(|s| s.1), Some(2));

        // `Audio_StopCrowdBox` clears cheer state back to 0 and stops the MIDI voice.
        h.stop(0);
        assert_eq!(h.box_state(0).map(|s| s.1), Some(0));
        assert!(h.take_actions().iter().any(|a| matches!(a, CrowdAction::StopVoice { engine: 2, .. })));
    }

    #[test]
    fn crowd_host_drives_real_sndmidi_on_eam_bytes() {
        crate::rand::streams().seed_fx(1);
        let eam: Vec<u8> = vec![
            b'M', b'I', b'D', b'x',
            1, 0xc0, 18,         // Program 18 (Cheer on Crowd.bnk)
            0, 0xb0, 7, 127,     // Volume 127
            5, 0x90, 64, 120,    // Note 64 vel 120, hold 5 ticks
            0, 0x80, 64, 0,      // Release
            0, 0xff, 0x2f, 0,    // EOT
        ];
        let mut h = CrowdHost::new();
        h.load_crowd_with(SAMPLE_CROWD_INF, b"GARI", 1, 3, |_| Some(eam.clone()));
        h.start_sound(0, [0.0; 3], 100, 0x61);
        h.update(|_| 3);
        let _ = h.take_actions();
        let _ = h.take_voice_actions();

        h.play_cheer(2, 0);
        h.tick_midi(10);
        let vacts = h.take_voice_actions();
        assert!(vacts.iter().any(|a| matches!(
            a,
            MidiVoiceAction::Start { bank: 3, program: 18, note: 64, velocity: 120, .. }
        )), "expected NoteOn on Crowd.bnk prog 18: {vacts:?}");
        assert!(vacts.iter().any(|a| matches!(a, MidiVoiceAction::Release { .. })));
    }

    #[test]
    fn anticipate_level_matches_vf15_20ea38() {
        assert_eq!(anticipate_level(2.1, 1.0), 3);
        assert_eq!(anticipate_level(1.0, 0.0), 3);
        assert_eq!(anticipate_level(2.0, 0.1), 0);
        assert_eq!(anticipate_level(1.0, 0.25), 1);
        assert_eq!(anticipate_level(1.0, 0.49), 1);
        assert_eq!(anticipate_level(1.0, 0.5), 2);
        assert_eq!(anticipate_level(0.5, 1.5), 2);
    }

    #[test]
    fn pitch_speed_matches_sndlib_ratio() {
        assert!((pitch_speed(60, 60, 0, 7, 64, 0x1000) - 1.0).abs() < 1e-3);
        assert!((pitch_speed(60, 72, 0, 7, 64, 0x1000) - 2.0).abs() < 1e-3);
        assert!((pitch_speed(60, 48, 0, 7, 64, 0x1000) - 0.5).abs() < 1e-3);
        assert!((pitch_speed(60, 60, 0, 7, 64, 0x2000) - 2.0).abs() < 1e-3);
    }

    #[test]
    fn crowd_host_loads_and_plays_disc_crowd_inf_and_eams() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
        let mut files = tricky_data::big::Files::new(disc);
        files.mount_game_archives().expect("mount");
        let inf = files.load("data/config/crowd.inf").expect("load").expect("crowd.inf missing");
        let bank_raw = files.load(r"|data\audio\Crowd.bnk").expect("load").expect("Crowd.bnk missing");
        let bank = tricky_data::bnk::parse(&bank_raw).expect("parse Crowd.bnk");

        crate::rand::streams().seed_fx(1);
        let mut h = CrowdHost::new();
        h.load_crowd_with(&inf, b"GARI", 1, 3, |p| files.load(p).ok().flatten());
        let (t0, f0, a0) = h.set_counts(0);
        let (t2, f2, a2) = h.set_counts(2);
        assert!(t0 > 0 && f0 > 0 && a0 > 0, "expected GLOBAL crowd lists populated: {:?}", (t0, f0, a0));
        assert!(t2 > 0 && f2 > 0 && a2 > 0, "expected tier 2 crowd lists populated: {:?}", (t2, f2, a2));
        h.start_sound(0, [0.0; 3], 100, 0x61);
        h.update(|_| 3);
        let _ = h.take_actions();
        let _ = h.take_voice_actions();

        // Trigger tier 2 trick cheer and step 10 ticks (0.1 s): Cheer30.eam starts notes on Crowd.bnk prog 18!
        h.play_cheer(2, 0);
        h.tick_midi(10);
        let vacts = h.take_voice_actions();
        let starts: Vec<_> = vacts
            .iter()
            .filter_map(|a| match a {
                MidiVoiceAction::Start { bank: 3, program, note, .. } => Some((*program as usize, *note)),
                _ => None,
            })
            .collect();
        assert!(!starts.is_empty(), "expected disc .eam to trigger Crowd.bnk notes: {vacts:?}");
        for (prog, _note) in starts {
            assert!(bank.programs.contains_key(&prog), "Crowd.bnk missing program {prog}");
        }
    }

    #[test]
    fn world_crowd_emitters_from_disc_drive_crowd_box_and_cheers() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
        let mut files = tricky_data::big::Files::new(disc);
        files.mount_game_archives().expect("mount");
        files.mount("data/models/gari.big").expect("mount gari.big");
        let pbd_bytes = files.load("|data/models/gari.pbd").expect("load").expect("gari.pbd");
        let adl_bytes = files.load("|data/models/gari.adl").expect("load").expect("gari.adl");
        let pbd = tricky_data::pbd::Pbd::parse(&pbd_bytes).expect("parse gari.pbd");
        let adl = tricky_data::adl::Adl::parse(&adl_bytes).expect("parse gari.adl");
        let emitters = extract_crowd_emitters(&pbd, &adl);
        assert!(!emitters.is_empty(), "expected Garibaldi .adl to contain 0x61..=0x63 crowd emitters");

        let inf = files.load("data/config/crowd.inf").expect("load").expect("crowd.inf");
        crate::rand::streams().seed_fx(1);
        let mut crowd = CrowdHost::new();
        crowd.load_crowd_with(&inf, b"GARI", 1, 3, |p| files.load(p).ok().flatten());
        let mut track = WorldCrowdTrack::default();
        track.load(emitters.clone());

        // Far away: box+0xd4 == 0, cheers do not play.
        track.tick(&mut crowd, [1e7, 1e7, 1e7], 0);
        assert_eq!(crowd.box_state(0).map(|s| s.0), Some(0));
        crowd.play_cheer(2, 0);
        assert!(crowd.take_actions().is_empty());

        // Move listener to the center of the first real crowd emitter on Garibaldi!
        let e0 = &emitters[0];
        let center = [
            e0.inst_pos[0] + f32::from_bits(e0.words[2]),
            e0.inst_pos[1] + f32::from_bits(e0.words[3]),
            e0.inst_pos[2] + f32::from_bits(e0.words[4]),
        ];
        track.tick(&mut crowd, center, 0);
        assert!(track.active_count() > 0);
        assert!(crowd.box_state(0).map(|s| s.0).unwrap_or(0) > 0, "expected box+0xd4 > 0 near spectator area");
        crowd.update(|_| 3);
        let _ = crowd.take_actions();
        let _ = crowd.take_voice_actions();

        // Now a trick cheer fires real `.eam` notes on Crowd.bnk!
        crowd.play_cheer(2, 0);
        crowd.tick_midi(10);
        assert!(
            crowd.take_voice_actions().iter().any(|a| matches!(a, MidiVoiceAction::Start { bank: 3, .. })),
            "expected cheer to trigger SndMidi voice near Garibaldi crowd emitter"
        );

        // Move listener away again: `CrowdBox_StopSlot` drops `box+0xd4` back to 0.
        track.tick(&mut crowd, [1e7, 1e7, 1e7], 0);
        assert_eq!(track.active_count(), 0);
        assert_eq!(crowd.box_state(0).map(|s| s.0), Some(0));
    }
}
