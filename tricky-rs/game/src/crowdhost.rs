//! The crowd boxes hosted (board row E1b2g7d3b7a): the audio system's `+0x372c` crowd boxes (`Audio_LoadCrowd`
//! 0x2209c8, 400 = `0x190` bytes each) running the ported, runner-checked [`crate::crowdbox`] functions in engine
//! memory (`pf::Mem`) from `DATA/CONFIG/CROWD.INF` (`tricky_data::audiocfg::crowd_config`).
//!
//! `Audio_PlayCrowdCheer` 0x221068 (`play_cheer`), `CrowdSys_StartBoxCheer` 0x221088 (`start_cheer`),
//! `Audio_ArmCrowdBox` 0x2210a8 (`arm`), `Audio_StopCrowdBox` 0x2210c8 (`stop`), and `CrowdSys_UpdateBoxes`
//! 0x220968 (`update`) all run the game's exact box state machine and draw from `Rand_Fx` 0x23d4a8.
//!
//! STANDIN: the EA MIDI player (`MidiSys_PlayPending` 0x222358 / `SndMidi_Play`, `.eam` files on `Crowd.bnk`,
//! row E1b2g7d3b7b) and bank monitor (`BankMonitor_Find` 0x20c200 / `SndSys_StopBankSound` 0x20c2a0) are the
//! host's: every voice/MIDI call is logged as a [`CrowdAction`] with no audio output yet.

use crate::crowdbox::{self as cb, BOX_UPDATE, ENTRY_BEGIN, ENTRY_PLAY, MEMSET, MONITOR_FIND, MONITOR_SET, OPEN_FILE, PLAY_PENDING, RAND, VOICE_STOP};
use crate::pf::Mem;
use crate::sndqueue;
use crate::speechlib::Mem as LibMem;
use crate::speechload::LoadEnv;
use std::collections::HashSet;

const AUDIO_GLOBAL: u32 = 0x0034_3120;
const AUDIO_VTABLE: u32 = 0x0039_f200;
const REQ_WORDS_AT: u32 = 0x0039_fbd8;
/// Exact 8 bytes at `0x39fbd8` in `SLUS_203.26` copied into every crowd voice request's `+0x44` row.
const REQ_WORDS: [u8; 8] = [0x00, 0x00, 0xff, 0xff, 0xf0, 0x13, 0x22, 0x00];
/// `cBXAudio::vf15` 0x20ea38 (vtable `+0x84`, `this` shift `+0x80` = 0): anticipation level 0..2 or 3 (none).
const VF15_ANTICIPATE: u32 = 0x0020_ea38;

const AUDIO_AT: u32 = 0x0108_0000;
const AUDIO_SIZE: usize = 0x4000;
const TABLES: u32 = 0x010c_0000;
const TABLES_SIZE: usize = 0x4000;
const SP: u32 = 0x010d_0000;
const SP_SIZE: usize = 0x200;

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
const Q_A40: u32 = TABLES + 0x3c0;
const Q_A44: u32 = TABLES + 0x400;
const Q_A48: u32 = TABLES + 0x440;
const BOXES_AT: u32 = TABLES + 0x0800;
const MAX_BOXES: usize = 4;
const STR_SCRATCH: u32 = TABLES + 0x1000;
const MON_SCRATCH: u32 = TABLES + 0x2000;
const EMITTERS: u32 = TABLES + 0x2200;
const MAX_EMITTERS: usize = 64;

/// A call the crowd boxes made into the audio/MIDI driver (logged until E1b2g7d3b7b plays `.eam` files).
#[derive(Clone, Debug, PartialEq)]
pub enum CrowdAction {
    /// `MidiSys_PushEvent` 0x222160 + `MidiSys_PlayPending` 0x222358: play cheer MIDI file `file` on `bank_kind`.
    PlayCheer { bank_kind: u32, file: String, volume: u32, voice: u32 },
    /// `SndSys_PlayPending` 0x20c948: start a crowd bed sound (`id` in `bank_kind`).
    PlaySound { bank_kind: u32, id: u32, volume: u32, voice: u32 },
    /// `SndSys_StopBankSound` 0x20c2a0: stop `voice` on `engine` (1 bank sound, 2 MIDI) with `fade`.
    StopVoice { voice: u32, engine: u32, fade: f32 },
    /// `BankMonitor_SetPitch` 0x20ad38: update playing cheer pitch scale (`v70`, 0x1000 = 1.0).
    SetPitch { voice: u32, scale: u32 },
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

/// Hosted `cCrowdSys` (`audio+0x3728..+0x382c`) and its `cCrowdBox` array (`audio+0x372c`).
pub struct CrowdHost {
    mem: Mem,
    st: CrowdState,
    next_emitter: usize,
    loaded: bool,
}

#[derive(Default)]
struct CrowdState {
    files: Vec<String>,
    active: HashSet<(u32, u32)>,
    next_voice: u32,
    actions: Vec<CrowdAction>,
}

impl CrowdState {
    fn env<F: FnMut(u32) -> u32>(&mut self, anticipate: F) -> HostEnv<'_, F> {
        HostEnv {
            files: &mut self.files,
            active: &mut self.active,
            next_voice: &mut self.next_voice,
            actions: &mut self.actions,
            anticipate,
        }
    }
}

impl Default for CrowdHost {
    fn default() -> Self {
        Self::new()
    }
}

struct HostEnv<'a, F: FnMut(u32) -> u32> {
    files: &'a mut Vec<String>,
    active: &'a mut HashSet<(u32, u32)>,
    next_voice: &'a mut u32,
    actions: &'a mut Vec<CrowdAction>,
    anticipate: F,
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

impl<F: FnMut(u32) -> u32> LoadEnv for HostEnv<'_, F> {
    fn call(&mut self, m: &mut dyn LibMem, target: u32, args: &[u32]) -> u32 {
        match target {
            OPEN_FILE => {
                let name = cstr(m, args[0]);
                if name.is_empty() {
                    return 0;
                }
                self.files.push(name);
                self.files.len() as u32
            }
            RAND => crate::rand::rand_fx(),
            MEMSET => {
                let (dst, val, len) = (args[0], args[1] as u8, args[2]);
                for k in 0..len {
                    m.w8(dst.wrapping_add(k), val);
                }
                dst
            }
            VOICE_STOP => {
                let (voice, engine, fade) = (args[1], args[2], f32::from_bits(args[3]));
                self.active.remove(&(voice, engine));
                self.actions.push(CrowdAction::StopVoice { voice, engine, fade });
                0
            }
            MONITOR_FIND => {
                let (voice, engine) = (args[1], args[2]);
                if self.active.contains(&(voice, engine)) {
                    m.w32(MON_SCRATCH + 4, voice);
                    MON_SCRATCH
                } else {
                    0
                }
            }
            MONITOR_SET => {
                let voice = m.r32(args[0].wrapping_add(4));
                self.actions.push(CrowdAction::SetPitch { voice, scale: args[1] });
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
                m.w32(row + 0xc, 0x32);
                m.w32(m.r32(a + 0x7c).wrapping_add(i.wrapping_mul(4)), 1);
                let midi = m.r32(a + 0xa48).wrapping_add(i.wrapping_mul(8));
                m.w32(midi, bank_kind);
                m.w32(midi + 4, file);
                0
            }
            // `MidiSys_PlayPending` 0x222358(audio)
            ENTRY_PLAY => {
                let a = args[0];
                let i = m.r32(a + 0x20);
                let volume = m.r32(m.r32(a + 0x3c).wrapping_add(i.wrapping_mul(4)));
                let midi = m.r32(a + 0xa48).wrapping_add(i.wrapping_mul(8));
                let bank_kind = m.r32(midi);
                let file_h = m.r32(midi + 4);
                m.w32(a + 0x20, i.wrapping_sub(1));
                *self.next_voice += 1;
                let voice = *self.next_voice;
                self.active.insert((voice, 2));
                let file = file_h.checked_sub(1).and_then(|k| self.files.get(k as usize)).cloned().unwrap_or_default();
                self.actions.push(CrowdAction::PlayCheer { bank_kind, file, volume, voice });
                voice
            }
            // `SndSys_PlayPending` 0x20c948(audio)
            PLAY_PENDING => {
                let a = args[0];
                let i = m.r32(a + 0x20);
                let bank_kind = m.r32(m.r32(a + 0xa40).wrapping_add(i.wrapping_mul(4)));
                let id = m.r32(m.r32(a + 0xa44).wrapping_add(i.wrapping_mul(4)));
                let volume = m.r32(m.r32(a + 0x3c).wrapping_add(i.wrapping_mul(4)));
                m.w32(a + 0x20, i.wrapping_sub(1));
                *self.next_voice += 1;
                let voice = *self.next_voice;
                self.active.insert((voice, 1));
                self.actions.push(CrowdAction::PlaySound { bank_kind, id, volume, voice });
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

impl CrowdHost {
    pub fn new() -> Self {
        let mut mem = Mem {
            regions: vec![
                (0x0034_3000, vec![0; 0x1000]),
                (0x0039_f000, vec![0; 0x1000]),
                (AUDIO_AT, vec![0; AUDIO_SIZE]),
                (TABLES, vec![0; TABLES_SIZE]),
                (SP, vec![0; SP_SIZE]),
            ],
        };
        mem.w32(AUDIO_GLOBAL, AUDIO_AT);
        for (k, &b) in REQ_WORDS.iter().enumerate() {
            mem.w8(REQ_WORDS_AT + k as u32, b);
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
            (0xa40, Q_A40), (0xa44, Q_A44), (0xa48, Q_A48),
        ] {
            mem.w32(AUDIO_AT + off, ptr);
        }
        CrowdHost {
            mem,
            st: CrowdState::default(),
            next_emitter: 0,
            loaded: false,
        }
    }

    /// `Audio_LoadCrowd` 0x2209c8 (from `Audio_LoadInGameConfig` 0x211098): create `count` crowd boxes (400 bytes
    /// each at `audio+0x372c`, bank kind `bank_kind` = 3) and load `CROWD.INF` (`[GLOBAL]` then `[course]`) into
    /// the three sets at `audio+0x3730 + 0x54 * n`.
    pub fn load_crowd(&mut self, crowd_inf: &[u8], course: &[u8], count: usize, bank_kind: u32) {
        let count = count.min(MAX_BOXES);
        self.st.files.clear();
        self.st.active.clear();
        self.next_emitter = 0;
        self.mem.w32(AUDIO_AT + 0x20, u32::MAX);
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
                    let mut env = self.st.env(|_| 3);
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
        Some((self.mem.r32(b + 0xd4), self.mem.r32(b + 0x180), self.mem.r32(b + 0x184), self.mem.r32(b + 0x174) as i32))
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

    /// `Audio_PlayCrowdCheer` 0x221068(audio, level, who) → `CrowdBox_PlayCheer` 0x2203a8.
    pub fn play_cheer(&mut self, level: u32, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = self.st.env(|_| 3);
        cb::play_cheer(&mut self.mem, &mut env, b, level as i32);
    }

    /// `CrowdSys_StartBoxCheer` 0x221088(audio, level, who) → `CrowdBox_StartCheer` 0x220538.
    pub fn start_cheer(&mut self, level: u32, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = self.st.env(|_| 3);
        cb::start_cheer(&mut self.mem, &mut env, b, level as i32);
    }

    /// `Audio_ArmCrowdBox` 0x2210a8(audio, who) → `CrowdBox_Arm` 0x2206c8.
    pub fn arm(&mut self, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        cb::arm(&mut self.mem, b);
    }

    /// `Audio_StopCrowdBox` 0x2210c8(audio, who) → `CrowdBox_Stop` 0x2206e0.
    pub fn stop(&mut self, who: u32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = self.st.env(|_| 3);
        cb::stop(&mut self.mem, &mut env, b);
    }

    /// `CrowdBox_StopAll` 0x220338 on every box (e.g. when a run ends or restarts).
    pub fn stop_all(&mut self, fade: f32) {
        let count = self.mem.r32(AUDIO_AT + 0x3728);
        for who in 0..count {
            if let Some(b) = self.box_addr(who) {
                let mut env = self.st.env(|_| 3);
                cb::stop_all(&mut self.mem, &mut env, b, fade);
                cb::stop(&mut self.mem, &mut env, b);
            }
        }
    }

    /// `CrowdSys_UpdateBoxes` 0x220968 (from `Audio_UpdateSystems` 0x21eb70): ticks every crowd box (`CrowdBox_Update`
    /// 0x21efb0), calling `anticipate(listener)` for `cBXAudio::vf15` 0x20ea38 when a box is armed.
    pub fn update(&mut self, anticipate: impl FnMut(u32) -> u32) {
        if !self.loaded {
            return;
        }
        let mut env = self.st.env(anticipate);
        cb::update_boxes(&mut self.mem, &mut env, AUDIO_AT);
    }

    /// `SndQueue_StartLastVoice` 0x2212a0 → `CrowdBox_StartSound` 0x21fc98: join a world crowd emitter at `pos`
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
        let mut env = self.st.env(|_| 3);
        let slot = cb::start_sound(&mut self.mem, &mut env, b, AUDIO_AT, id);
        self.mem.w32(AUDIO_AT + 0x20, i.wrapping_sub(1));
        slot
    }

    /// `Audio_SetVoiceVolume` 0x221000 → `CrowdBox_SetSlotValue` 0x21fc68.
    pub fn set_slot_value(&mut self, who: u32, slot: i32, vol: u32) {
        let Some(b) = self.box_addr(who) else { return };
        cb::set_slot_value(&mut self.mem, b, slot, vol);
    }

    /// `CrowdBox_StopSlot` 0x2201f8.
    pub fn stop_slot(&mut self, who: u32, slot: i32, fade: f32) {
        let Some(b) = self.box_addr(who) else { return };
        let mut env = self.st.env(|_| 3);
        cb::stop_slot(&mut self.mem, &mut env, b, slot, fade);
    }

    /// Drain logged voice/MIDI actions since the last call.
    pub fn take_actions(&mut self) -> Vec<CrowdAction> {
        std::mem::take(&mut self.st.actions)
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
    fn crowd_host_loads_disc_crowd_inf() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
        let mut files = tricky_data::big::Files::new(disc);
        files.mount_game_archives().expect("mount");
        let inf = files.load("data/config/crowd.inf").expect("load").expect("crowd.inf missing");
        crate::rand::streams().seed_fx(1);
        let mut h = CrowdHost::new();
        h.load_crowd(&inf, b"GARI", 1, 3);
        let (t0, f0, a0) = h.set_counts(0);
        let (t2, f2, a2) = h.set_counts(2);
        assert!(t0 > 0 && f0 > 0 && a0 > 0, "expected GLOBAL crowd lists populated: {:?}", (t0, f0, a0));
        assert!(t2 > 0 && f2 > 0 && a2 > 0, "expected tier 2 crowd lists populated: {:?}", (t2, f2, a2));
        h.start_sound(0, [0.0; 3], 100, 0x61);
        h.update(|_| 3);
        let _ = h.take_actions();
        h.play_cheer(2, 0);
        let acts = h.take_actions();
        assert!(matches!(acts.as_slice(), [CrowdAction::PlayCheer { bank_kind: 3, file, .. }] if !file.is_empty()));
    }
}
