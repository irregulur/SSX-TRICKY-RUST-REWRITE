//! EA's MIDI sequencer (`SndMidi_*` 0x2d37e8..0x2d4a08, `Snd_MidiPanToPan` 0x2d6910, `SndPlayOpts_Init`
//! 0x2d4a60) and the game's `cMidiSys` queue layer (`MidiSys_Init` 0x222080, `cMidiSys::~cMidiSys` 0x222100,
//! `MidiSys_PushEvent` 0x222160, `MidiSys_PlayPending` 0x222358, `MidiSys_PlayEventCallback` 0x222798) on the
//! game's bytes (`pf::Mem`), board row E1b2g7d3b7b1. Every function is checked word-for-word against
//! `SLUS_203.26` in `tests/sndmidi.rs`.
//!
//! Layout of `SndMidi` globals in `SLUS_203.26`:
//! - `0x003507e8`: `i32` round-robin cursor into the 24 MIDI voices (`0..24`).
//! - `0x003507f0`: `slots[16]`, `0x70` bytes each (`0x3507f0..0x350ef0`):
//!   - `+0x00` `u32` tick counter (at 100 Hz from `SND_RunTicks` 0x2d05a0 -> `SND_Tick` 0x2d0698)
//!   - `+0x04` `u32` MIDI steps executed
//!   - `+0x08` `u32` stream read pointer into `.eam` bytes
//!   - `+0x0c` `u32` saved loop start pointer (CC `0x14`)
//!   - `+0x10` `i32` handle (`< 0` free, else `gen | slot`)
//!   - `+0x14` `i32` master volume 16.16 fixed point (`+0x16` `i16` integer 0..127)
//!   - `+0x18` `i32` fade step per tick (16.16; `0` = not fading)
//!   - `+0x1c` `i32` fade target (16.16)
//!   - `+0x20` `u16` master pan offset
//!   - `+0x22` `u16` master pitch scale
//!   - `+0x24` `u16` tempo (`(opts_speed * 100) >> 12`; `100` at default `0x1000`)
//!   - `+0x26` `u8` loop counter (`0xff` initially; `>= 0x7f` reloads from CC `0x1e` value)
//!   - `+0x27` `u8` wait ticks remaining until next event (`1` initially)
//!   - `+0x28` `i8` state (`0` uninit, `2` stopped, `3` playing, `5` paused; `GetState` returns `6` when fading)
//!   - `+0x2c` `i16` main bank handle
//!   - `+0x2e` `i16` percussion bank handle for channel 9 (`-1` if none)
//!   - `+0x30..+0x70`: `channels[16]` packed `u32`: bits `0..10` pan (`>> 6`), bits `10..17` volume (`0..127`),
//!     bits `17..24` pitch bend (`0..127`), byte `+3` program (`i8`, `-1` initially)
//! - `0x00350ef0`: `voices[24]`, 8 bytes each (`0x350ef0..0x350fb0`):
//!   - `+0x00` `i32` sound handle (`-1` free)
//!   - `+0x04` `i8` owning MIDI slot (`0..15`)
//!   - `+0x05` `i8` MIDI channel (`0..15`, or `-1`)
//!   - `+0x06` `i8` MIDI note
//!   - `+0x07` `i8` velocity (`-1` after `NoteOff` release)
//! - `0x00350fb0`: `i32` handle generation counter (steps by `0x20`, wraps to `0` on negative)
//! - `0x003511ec`: `u32` shutdown callback pointer (`SndMidi_Shutdown` `0x2d42c8`)
//! - `0x00351998`: `[u16; 128]` MIDI pan (`0..127`) to EA pan (`0xc000..0x3fff`) table.

use crate::musicsong::SongEnv;
use crate::pf::Mem;
use crate::ps2f::F;

pub const VOICE_CURSOR: u32 = 0x0035_07e8;
pub const SLOTS_AT: u32 = 0x0035_07f0;
pub const SLOT_SIZE: u32 = 0x70;
pub const SLOT_COUNT: usize = 16;
pub const VOICES_AT: u32 = 0x0035_0ef0;
pub const VOICE_COUNT: usize = 24;
pub const NEXT_GEN: u32 = 0x0035_0fb0;
pub const SHUTDOWN_CB: u32 = 0x0035_11ec;
pub const MIDI_PAN_TABLE_AT: u32 = 0x0035_1998;

pub const MIDI_SYS_GLOBAL: u32 = 0x0034_3124;
pub const MIDI_BANKS_TAG: u32 = 0x0039_fd70;
pub const EV_MIDI_ECHO: u32 = 0x0039_fd80;
pub const MIDI_SYS_VTABLE: u32 = 0x0039_fd88;

// External targets called by `SndMidi_*` and `MidiSys_*`
pub const SND_LOCK: u32 = 0x002d_0908;
pub const SND_UNLOCK: u32 = 0x002d_0938;
pub const SND_ADD_TICK_CB: u32 = 0x002d_0968;
pub const SND_REMOVE_TICK_CB: u32 = 0x002d_09a0;
pub const SND_PLAY_BANK_SOUND: u32 = 0x002d_1718;
pub const SND_HANDLE_SET_VOLUME: u32 = 0x002d_3108;
pub const SND_HANDLE_SET_PITCH: u32 = 0x002d_31b8;
pub const SND_HANDLE_STOP: u32 = 0x002d_3260;
pub const SND_HANDLE_SET_PAN: u32 = 0x002d_3480;
pub const SND_VOICE_VALIDATE: u32 = 0x002d_50f0;
pub const SND_HANDLE_SET_PITCH_BEND: u32 = 0x002d_6a18;
pub const SND_HANDLE_RELEASE: u32 = 0x002d_fda0;
pub const SND_MIDI_TICK_ADDR: u32 = 0x002d_4108;
pub const SND_MIDI_SHUTDOWN_ADDR: u32 = 0x002d_42c8;
pub const SND_MIDI_INIT_ADDR: u32 = 0x002d_4228;
pub const SND_MIDI_PLAY_ADDR: u32 = 0x002d_4440;

pub const MEM_FREE: u32 = 0x0014_a040;
pub const BANK_INSTANCE_SET_COPY1: u32 = 0x0020_abf8;
pub const BANK_MONITOR_ADD_INSTANCE: u32 = 0x0020_bd68;
pub const BANK_SYS_INIT: u32 = 0x0020_c560;
pub const BANK_SYS_DTOR: u32 = 0x0020_c610;
pub const SND_SYS_SCHEDULE_EVENT: u32 = 0x0022_2cc0;
pub const EAMEM_SET: u32 = 0x002c_c808;
pub const EAMEM_ALLOC: u32 = 0x002c_cf70;
pub const MEMSET: u32 = 0x002f_b468;

/// `0x00351998` in `SLUS_203.26`: 128-entry MIDI pan (0..127) to EA pan angle (`0xc000..=0x3fff`) table.
pub const MIDI_PAN_TABLE: [u16; 128] = [
    0xc000, 0xc007, 0xc016, 0xc02f, 0xc052, 0xc07e, 0xc0b5, 0xc0f5,
    0xc13f, 0xc192, 0xc1ef, 0xc256, 0xc2c6, 0xc340, 0xc3c3, 0xc44f,
    0xc4e4, 0xc583, 0xc62a, 0xc6db, 0xc794, 0xc856, 0xc921, 0xc9f4,
    0xcacf, 0xcbb3, 0xcc9e, 0xcd92, 0xce8d, 0xcf90, 0xd09a, 0xd1ac,
    0xd2c5, 0xd3e4, 0xd50b, 0xd638, 0xd76c, 0xd8a6, 0xd9e5, 0xdb2b,
    0xdc77, 0xddc8, 0xdf1e, 0xe079, 0xe1d9, 0xe33e, 0xe4a7, 0xe615,
    0xe786, 0xe8fb, 0xea74, 0xebf0, 0xed6f, 0xeef1, 0xf076, 0xf1fd,
    0xf386, 0xf511, 0xf69e, 0xf82c, 0xf9bb, 0xfb4c, 0xfcdd, 0xfe6f,
    0x0000, 0x0198, 0x0330, 0x04c7, 0x065e, 0x07f4, 0x0988, 0x0b1b,
    0x0cac, 0x0e3b, 0x0fc8, 0x1152, 0x12da, 0x145f, 0x15e0, 0x175e,
    0x18d8, 0x1a4e, 0x1bc0, 0x1d2e, 0x1e97, 0x1ffb, 0x215a, 0x22b4,
    0x2408, 0x2557, 0x269f, 0x27e2, 0x291e, 0x2a53, 0x2b82, 0x2caa,
    0x2dcb, 0x2ee4, 0x2ff7, 0x3101, 0x3204, 0x32fe, 0x33f1, 0x34db,
    0x35bd, 0x3697, 0x3767, 0x382f, 0x38ee, 0x39a4, 0x3a51, 0x3af4,
    0x3b8e, 0x3c1f, 0x3ca6, 0x3d24, 0x3d97, 0x3e01, 0x3e61, 0x3eb7,
    0x3f04, 0x3f46, 0x3f7e, 0x3fac, 0x3fd0, 0x3fea, 0x3ff9, 0x3fff,
];

#[inline]
fn slot_addr(slot: i32) -> u32 {
    SLOTS_AT.wrapping_add((slot as u32).wrapping_mul(SLOT_SIZE))
}

#[inline]
fn voice_addr(i: usize) -> u32 {
    VOICES_AT.wrapping_add((i as u32) * 8)
}

/// `Snd_MidiPanToPan` 0x2d6910: look up a 0..127 MIDI pan byte in the table at `0x351998`.
pub fn midi_pan_to_pan(pan: u8) -> u16 {
    MIDI_PAN_TABLE[(pan as usize) & 0x7f]
}

/// `SndPlayOpts_Init` 0x2d4a60: fill a 24-byte `SndPlayOpts` struct with defaults.
pub fn play_opts_init(m: &mut Mem, opts: u32) -> u32 {
    m.w8(opts + 3, 0x3c);
    m.w16(opts + 0x10, 0x1000);
    m.w16(opts + 0x14, 0xffff);
    m.w8(opts + 1, 0x40);
    m.w8(opts + 2, 0x7f);
    m.w16(opts + 0x0a, 0);
    m.w16(opts + 0x0c, 0x1000);
    m.w16(opts + 0x0e, 0x1000);
    m.w16(opts + 0x12, 0);
    m.w16(opts + 0x16, 0);
    m.w8(opts + 4, 0x7f);
    m.w8(opts, 0x7f);
    m.w16(opts + 8, 0);
    0
}

/// `SndMidi_Validate` 0x2d37e8: verify a MIDI handle and return its slot index (`0..15`), or `-8`.
pub fn validate(m: &Mem, handle: i32) -> i32 {
    if handle < 0 {
        return -8;
    }
    let idx = (handle as u32) & 0x1f;
    if idx < SLOT_COUNT as u32 {
        if m.r32(slot_addr(idx as i32) + 0x10) == handle as u32 {
            return idx as i32;
        }
    }
    -8
}

/// `SndMidi_Init` 0x2d4228: clear the 24 MIDI voices and 16 MIDI slots and register `SndMidi_Tick`.
pub fn init(m: &mut Mem, e: &mut dyn SongEnv) -> u32 {
    e.call(m, SND_LOCK, &[]);
    for i in 0..VOICE_COUNT {
        let v = voice_addr(i);
        m.w8(v + 5, 0xff);
        m.w8(v + 6, 0);
        m.w8(v + 7, 0);
        m.w32(v, u32::MAX);
    }
    for i in 0..SLOT_COUNT {
        let s = slot_addr(i as i32);
        m.w32(s + 0x10, u32::MAX);
        m.w8(s + 0x28, 0);
    }
    e.call(m, SND_ADD_TICK_CB, &[SND_MIDI_TICK_ADDR]);
    m.w32(SHUTDOWN_CB, SND_MIDI_SHUTDOWN_ADDR);
    e.call(m, SND_UNLOCK, &[]);
    0
}

/// `SndMidi_Shutdown` 0x2d42c8: stop every active MIDI slot and unregister `SndMidi_Tick`.
pub fn shutdown(m: &mut Mem, e: &mut dyn SongEnv) -> u32 {
    e.call(m, SND_LOCK, &[]);
    stop_all(m, e);
    e.call(m, SND_REMOVE_TICK_CB, &[SND_MIDI_TICK_ADDR]);
    m.w32(SHUTDOWN_CB, 0);
    e.call(m, SND_UNLOCK, &[]);
    0
}

/// `SndMidi_StopAll` 0x2d4308: lock and stop all 16 MIDI slots.
pub fn stop_all(m: &mut Mem, e: &mut dyn SongEnv) -> u32 {
    e.call(m, SND_LOCK, &[]);
    for i in 0..SLOT_COUNT {
        let h = m.r32(slot_addr(i as i32) + 0x10) as i32;
        stop(m, e, h);
    }
    e.call(m, SND_UNLOCK, &[]);
    0
}

/// `SndMidi_Stop` 0x2d4368: stop one MIDI handle, release all its active voices, and free the slot.
pub fn stop(m: &mut Mem, e: &mut dyn SongEnv, handle: i32) -> i32 {
    let slot = validate(m, handle);
    if slot < 0 {
        return slot;
    }
    let s = slot_addr(slot);
    if m.r8(s + 0x28) as i8 == 0 {
        return -1;
    }
    for i in 0..VOICE_COUNT {
        let v = voice_addr(i);
        if m.r8(v + 4) as i8 as i32 == slot {
            let vh = m.r32(v) as i32;
            if vh >= 0 {
                e.call(m, SND_HANDLE_STOP, &[vh as u32]);
            }
            m.w8(v + 5, 0xff);
            m.w8(v + 6, 0);
            m.w8(v + 7, 0);
            m.w32(v, u32::MAX);
        }
    }
    m.w8(s + 0x28, 2);
    m.w32(s + 0x10, u32::MAX);
    0
}

/// `SndMidi_Play` 0x2d4440: start a MIDI sequence from `data` with play options `opts`, `bank`, and `drum_bank`.
/// Returns the new handle or `-9` if all 16 slots are busy.
pub fn play(m: &mut Mem, opts: u32, mut data: u32, bank: i16, drum_bank: i16) -> i32 {
    if m.r16(data) as i16 == 0x444d {
        data = data.wrapping_add(8);
    } else if m.r32(data) == 0x7844_494d {
        data = data.wrapping_add(4);
    }
    let gen = (m.r32(NEXT_GEN) as i32).wrapping_add(0x20);
    m.w32(NEXT_GEN, if gen < 0 { 0 } else { gen as u32 });
    for i in 0..SLOT_COUNT {
        let s = slot_addr(i as i32);
        if (m.r32(s + 0x10) as i32) < 0 {
            m.w32(s + 4, 0);
            m.w32(s, 0);
            let tempo = (((m.r16(opts + 0x10) as u32).wrapping_mul(100)) as i32 >> 12) as i16;
            m.w16(s + 0x24, tempo as u16);
            let h = m.r32(NEXT_GEN) | (i as u32);
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

/// `SndMidi_VoicesDone` 0x2d3938: returns `1` if every voice owned by `slot` has finished (`SndVoice_Validate < 0`).
pub fn voices_done(m: &mut Mem, e: &mut dyn SongEnv, slot: i8) -> u32 {
    for i in 0..VOICE_COUNT {
        let v = voice_addr(i);
        if m.r8(v + 4) as i8 == slot {
            let vh = m.r32(v) as i32;
            if vh >= 0 {
                if (e.call(m, SND_VOICE_VALIDATE, &[vh as u32]) as i32) >= 0 {
                    return 0;
                }
            }
        }
    }
    1
}

/// `SndMidi_UpdateFade` 0x2d3838: advance a slot's volume fade by one tick; stops slot and returns `1` if `< 0`.
pub fn update_fade(m: &mut Mem, e: &mut dyn SongEnv, slot: i32) -> u32 {
    let s = slot_addr(slot);
    let step = m.r32(s + 0x18) as i32;
    let next = (m.r32(s + 0x14) as i32).wrapping_add(step);
    m.w32(s + 0x14, next as u32);
    let target = m.r32(s + 0x1c) as i32;
    let keep = if step < 0 { target < next } else { next < target };
    let cur = if keep {
        m.r32(s + 0x14) as i32
    } else {
        m.w32(s + 0x14, target as u32);
        m.w32(s + 0x18, 0);
        m.r32(s + 0x14) as i32
    };
    if cur < 0 {
        let h = m.r32(s + 0x10) as i32;
        stop(m, e, h);
        return 1;
    }
    for i in 0..VOICE_COUNT {
        let v = voice_addr(i);
        if m.r8(v + 4) as i8 as i32 == slot {
            let vh = m.r32(v) as i32;
            if vh >= 0 {
                let ch = m.r8(v + 5) as i8 as i32;
                let ch_word = m.r32(s.wrapping_add((ch as u32).wrapping_mul(4)).wrapping_add(0x30));
                let master = m.r16(s + 0x16) as i16 as i32;
                let vol = (((ch_word >> 10) & 0x7f) as i32).wrapping_mul(master) >> 7;
                e.call(m, SND_HANDLE_SET_VOLUME, &[vh as u32, vol as u32]);
            }
        }
    }
    0
}

/// `SndMidi_NoteOn` 0x2d39c8: allocate a free MIDI voice slot, build `SndPlayOpts` at `sp`, and call `SND_PlayBankSound`.
pub fn note_on(m: &mut Mem, e: &mut dyn SongEnv, slot: i32, ch_ptr: u32, chan: i32, note: u8, sp: u32) {
    let s = slot_addr(slot);
    let old = m.r32(VOICE_CURSOR) as i32;
    let end = old.wrapping_add(0x19);
    let mut i = old.wrapping_add(1);
    m.w32(VOICE_CURSOR, i as u32);
    let mut found = false;
    let mut v_ptr = 0u32;
    while i < end {
        let rem = i % 0x18;
        m.w32(VOICE_CURSOR, rem as u32);
        v_ptr = VOICES_AT.wrapping_add((rem as u32).wrapping_mul(8));
        let vh = m.r32(v_ptr);
        if (e.call(m, SND_VOICE_VALIDATE, &[vh]) as i32) < 0 {
            found = true;
            break;
        }
        i = i.wrapping_add(1);
    }
    if found {
        play_opts_init(m, sp);
        let ch_word = m.r32(ch_ptr);
        let master = m.r16(s + 0x16) as i16 as i32;
        let vol = (((ch_word >> 10) & 0x7f) as i32).wrapping_mul(master) >> 7;
        let pan = (((ch_word & 0x3ff) << 6).wrapping_add(m.r16(s + 0x20) as u32)) & 0xffff;
        let bend = ((ch_word >> 17) & 0x7f) as u8;
        let pitch = m.r16(s + 0x22);
        let ptr = m.r32(s + 8);
        let vel = m.r8(ptr);
        m.w8(sp, vol as u8);
        m.w8(sp + 1, bend);
        m.w8(sp + 4, vel);
        m.w16(sp + 8, pan as u16);
        m.w16(sp + 0x0c, pitch);
        let (bank, prog) = if chan == 9 && (m.r16(s + 0x2e) as i16) >= 0 {
            m.w8(sp + 3, 0x3c);
            (m.r16(s + 0x2e) as i16 as i32, note as u32)
        } else {
            m.w8(sp + 3, note);
            (m.r16(s + 0x2c) as i16 as i32, m.r8(ch_ptr + 3) as i8 as i32 as u32)
        };
        let played = e.call(m, SND_PLAY_BANK_SOUND, &[bank as u32, prog, sp]);
        m.w32(v_ptr, played);
        let ptr2 = m.r32(s + 8);
        let vel2 = m.r8(ptr2);
        m.w8(v_ptr + 7, vel2);
        m.w32(s + 8, ptr2.wrapping_add(1));
        if (m.r32(v_ptr) as i32) >= 0 {
            m.w8(v_ptr + 6, note);
            m.w8(v_ptr + 5, chan as u8);
            m.w8(v_ptr + 4, slot as u8);
        }
    } else {
        m.w32(s + 8, m.r32(s + 8).wrapping_add(1));
    }
}

/// `SndMidi_NoteOff` 0x2d3b90: consume the velocity byte and release the first matching held voice.
pub fn note_off(m: &mut Mem, e: &mut dyn SongEnv, slot: i32, chan: i32, note: u8) {
    let s = slot_addr(slot);
    m.w32(s + 8, m.r32(s + 8).wrapping_add(1));
    for i in 0..VOICE_COUNT {
        let v = voice_addr(i);
        if m.r8(v + 4) as i8 as i32 == slot
            && m.r8(v + 5) as i8 as i32 == chan
            && m.r8(v + 6) as i8 as i32 == note as i32
            && (m.r8(v + 7) as i8) >= 0
        {
            let vh = m.r32(v);
            e.call(m, SND_HANDLE_RELEASE, &[vh]);
            m.w8(v + 7, 0xff);
            return;
        }
    }
}

/// `SndMidi_OnController` 0x2d3c30: handle CC `0x14` (loop start), `0x1e` (loop end), `7` (volume), `10` (pan).
pub fn on_controller(m: &mut Mem, e: &mut dyn SongEnv, slot: i32, ch_ptr: u32, chan: i32, cc: u8) {
    let s = slot_addr(slot);
    match cc {
        0x14 => {
            let next = m.r32(s + 8).wrapping_add(1);
            m.w32(s + 0x0c, next);
            m.w32(s + 8, next);
        }
        0x1e => {
            if m.r8(s + 0x26) >= 0x7f {
                let ptr = m.r32(s + 8);
                let cnt = m.r8(ptr);
                m.w8(s + 0x26, cnt);
            }
            let cnt = m.r8(s + 0x26);
            if cnt != 0 {
                m.w8(s + 0x26, cnt.wrapping_sub(1));
                let lp = m.r32(s + 0x0c);
                m.w32(s + 8, lp);
                return;
            }
            m.w32(s + 8, m.r32(s + 8).wrapping_add(1));
        }
        7 => {
            let ptr = m.r32(s + 8);
            let val = (m.r8(ptr) & 0x7f) as u32;
            let w = (m.r32(ch_ptr) & 0xfffe_03ff) | (val << 10);
            m.w32(ch_ptr, w);
            m.w32(s + 8, ptr.wrapping_add(1));
            for i in 0..VOICE_COUNT {
                let v = voice_addr(i);
                if m.r8(v + 4) as i8 as i32 == slot && m.r8(v + 5) as i8 as i32 == chan {
                    let vh = m.r32(v);
                    let master = m.r16(s + 0x16) as i16 as i32;
                    let ch_vol = ((m.r32(ch_ptr) >> 10) & 0x7f) as i32;
                    let out = ch_vol.wrapping_mul(master) >> 7;
                    e.call(m, SND_HANDLE_SET_VOLUME, &[vh, out as u32]);
                }
            }
        }
        10 => {
            let ptr = m.r32(s + 8);
            let val = m.r8(ptr);
            m.w32(s + 8, ptr.wrapping_add(1));
            let pan = midi_pan_to_pan(val) as i32;
            let w = (m.r32(ch_ptr) & 0xffff_fc00) | (((pan >> 6) as u32) & 0x3ff);
            m.w32(ch_ptr, w);
            for i in 0..VOICE_COUNT {
                let v = voice_addr(i);
                if m.r8(v + 4) as i8 as i32 == slot && m.r8(v + 5) as i8 as i32 == chan {
                    let vh = m.r32(v);
                    let p = (((m.r32(ch_ptr) & 0x3ff) << 6).wrapping_add(m.r16(s + 0x20) as u32)) & 0xffff;
                    e.call(m, SND_HANDLE_SET_PAN, &[vh, p, 0]);
                }
            }
        }
        _ => {
            m.w32(s + 8, m.r32(s + 8).wrapping_add(1));
        }
    }
}

/// `SndMidi_OnPitchBend` 0x2d3e18: read MSB byte, combine with `lsb`, update channel pitch bend and voices.
pub fn on_pitch_bend(m: &mut Mem, e: &mut dyn SongEnv, slot: i32, ch_ptr: u32, chan: i32, lsb: i32) {
    let s = slot_addr(slot);
    let ptr = m.r32(s + 8);
    let msb = m.r8(ptr) as u32;
    m.w32(s + 8, ptr.wrapping_add(1));
    let raw = ((msb << 7) as i32).wrapping_add(lsb);
    let adj = if raw < 0 { raw.wrapping_add(0x7f) } else { raw };
    let bend = ((adj >> 7) as u32) & 0x7f;
    let w = (m.r32(ch_ptr) & 0xff01_ffff) | (bend << 17);
    m.w32(ch_ptr, w);
    for i in 0..VOICE_COUNT {
        let v = voice_addr(i);
        if m.r8(v + 4) as i8 as i32 == slot && m.r8(v + 5) as i8 as i32 == chan {
            let vh = m.r32(v);
            let b = (m.r32(ch_ptr) >> 17) & 0x7f;
            e.call(m, SND_HANDLE_SET_PITCH_BEND, &[vh, b]);
        }
    }
}

/// `SndMidi_RunEvents` 0x2d3f00: advance one MIDI tick for `slot`, dispatching all zero-delta events.
pub fn run_events(m: &mut Mem, e: &mut dyn SongEnv, slot: i32, sp: u32) {
    let s = slot_addr(slot);
    let ptr0 = m.r32(s + 8);
    let wait = m.r8(s + 0x27).wrapping_sub(1);
    m.w8(s + 0x27, wait);
    let peek_status = m.r8(ptr0.wrapping_add(1));
    let peek_data1 = m.r8(ptr0.wrapping_add(2));
    m.w32(s + 8, ptr0);
    if (peek_status & 0xf0) == 0xf0 && peek_data1 == 0x2f {
        if voices_done(m, e, slot as i8) != 0 {
            let h = m.r32(s + 0x10) as i32;
            stop(m, e, h);
        }
        return;
    }
    if m.r8(s + 0x27) != 0 {
        return;
    }
    loop {
        let p = m.r32(s + 8);
        let mut delta = m.r8(p);
        let status = m.r8(p.wrapping_add(1));
        let data1 = m.r8(p.wrapping_add(2));
        m.w32(s + 8, p.wrapping_add(3));
        let chan = (status & 0x0f) as u32;
        let ch_ptr = s + 0x30 + chan * 4;
        match status & 0xf0 {
            0x80 => note_off(m, e, slot, chan as i32, data1),
            0x90 => note_on(m, e, slot, ch_ptr, chan as i32, data1, sp),
            0xb0 => on_controller(m, e, slot, ch_ptr, chan as i32, data1),
            0xc0 => m.w8(ch_ptr + 3, data1),
            0xe0 => on_pitch_bend(m, e, slot, ch_ptr, chan as i32, data1 as i32),
            0xf0 if data1 == 0x2f => {
                if voices_done(m, e, slot as i8) == 0 {
                    delta = 1;
                    m.w32(s + 8, m.r32(s + 8).wrapping_sub(3));
                    m.w8(s + 0x27, m.r8(s + 0x27).wrapping_add(1));
                } else {
                    let h = m.r32(s + 0x10) as i32;
                    stop(m, e, h);
                }
            }
            _ => {}
        }
        if delta != 0 {
            m.w8(s + 0x27, delta);
            return;
        }
        if m.r8(s + 0x28) as i8 != 3 {
            m.w8(s + 0x27, delta);
            return;
        }
    }
}

/// `SndMidi_Tick` 0x2d4108: 100 Hz callback registered by `SndMidi_Init`; updates fades and steps events.
pub fn tick(m: &mut Mem, e: &mut dyn SongEnv, sp: u32) {
    for i in 0..SLOT_COUNT {
        let s = slot_addr(i as i32);
        if m.r8(s + 0x28) as i8 != 3 {
            continue;
        }
        if m.r32(s + 0x18) != 0 && update_fade(m, e, i as i32) != 0 {
            continue;
        }
        let ticks = m.r32(s);
        let tempo = m.r16(s + 0x24) as u32;
        let target = ticks.wrapping_mul(tempo) / 100;
        m.w32(s, ticks.wrapping_add(1));
        if m.r32(s + 4) <= target {
            while m.r8(s + 0x28) as i8 == 3 {
                run_events(m, e, i as i32, sp);
                let step = m.r32(s + 4).wrapping_add(1);
                m.w32(s + 4, step);
                if target < step {
                    break;
                }
            }
        }
    }
}

/// `SndMidi_SetPitch` 0x2d45a8: update master pitch scale (`0x1000` = 1.0) on a playing MIDI slot and its voices.
pub fn set_pitch(m: &mut Mem, e: &mut dyn SongEnv, handle: i32, pitch: u32) -> i32 {
    let slot = validate(m, handle);
    if slot < 0 {
        return slot;
    }
    let s = slot_addr(slot);
    if pitch != m.r16(s + 0x22) as u32 {
        if m.r8(s + 0x28) as i8 != 3 {
            return -1;
        }
        m.w16(s + 0x22, pitch as u16);
        for i in 0..VOICE_COUNT {
            let v = voice_addr(i);
            if m.r8(v + 4) as i8 as i32 == slot {
                let vh = m.r32(v) as i32;
                if vh >= 0 {
                    e.call(m, SND_HANDLE_SET_PITCH, &[vh as u32, pitch]);
                }
            }
        }
    }
    0
}

/// `SndMidi_SetPan` 0x2d4660: update master pan offset on a playing MIDI slot and its voices.
pub fn set_pan(m: &mut Mem, e: &mut dyn SongEnv, handle: i32, pan: u32) -> i32 {
    let slot = validate(m, handle);
    if slot < 0 {
        return slot;
    }
    let s = slot_addr(slot);
    if pan != m.r16(s + 0x20) as u32 {
        if m.r8(s + 0x28) as i8 != 3 {
            return -1;
        }
        m.w16(s + 0x20, pan as u16);
        for i in 0..VOICE_COUNT {
            let v = voice_addr(i);
            if m.r8(v + 4) as i8 as i32 == slot {
                let vh = m.r32(v) as i32;
                if vh >= 0 {
                    let ch = m.r8(v + 5) as i8 as i32;
                    let ch_word = m.r32(s.wrapping_add((ch as u32).wrapping_mul(4)).wrapping_add(0x30));
                    let p = ((m.r16(s + 0x20) as u32).wrapping_add((ch_word & 0x3ff) << 6)) & 0xffff;
                    e.call(m, SND_HANDLE_SET_PAN, &[vh as u32, p, 0]);
                }
            }
        }
    }
    0
}

/// `SndMidi_Pause` 0x2d4740: pause a playing MIDI slot, stopping its active voices, and return its state.
pub fn pause(m: &mut Mem, e: &mut dyn SongEnv, handle: i32) -> i32 {
    e.call(m, SND_LOCK, &[]);
    let slot = validate(m, handle);
    if slot < 0 {
        e.call(m, SND_UNLOCK, &[]);
        return slot;
    }
    let s = slot_addr(slot);
    if m.r8(s + 0x28) as i8 == 3 {
        m.w8(s + 0x28, 5);
        for i in 0..VOICE_COUNT {
            let v = voice_addr(i);
            if m.r8(v + 4) as i8 as i32 == slot {
                let vh = m.r32(v) as i32;
                if vh >= 0 {
                    e.call(m, SND_HANDLE_STOP, &[vh as u32]);
                }
            }
        }
    }
    let st = get_state(m, handle);
    e.call(m, SND_UNLOCK, &[]);
    st
}

/// `SndMidi_Resume` 0x2d4808: resume a paused MIDI slot and return its state.
pub fn resume(m: &mut Mem, e: &mut dyn SongEnv, handle: i32) -> i32 {
    e.call(m, SND_LOCK, &[]);
    let slot = validate(m, handle);
    let ret = if slot < 0 {
        slot
    } else {
        let s = slot_addr(slot);
        if m.r8(s + 0x28) as i8 == 5 {
            m.w8(s + 0x28, 3);
        }
        get_state(m, handle)
    };
    e.call(m, SND_UNLOCK, &[]);
    ret
}

/// `SndMidi_SetVolume` 0x2d4888: cancel any fade and set master volume (`0..127`) on a playing MIDI slot.
pub fn set_volume(m: &mut Mem, e: &mut dyn SongEnv, handle: i32, vol: i32) -> i32 {
    let slot = validate(m, handle);
    if slot < 0 {
        return slot;
    }
    let s = slot_addr(slot);
    let fp = vol.wrapping_shl(16);
    let old = m.r32(s + 0x14) as i32;
    m.w32(s + 0x18, 0);
    if old == fp {
        return 0;
    }
    if m.r8(s + 0x28) as i8 != 3 {
        return -1;
    }
    m.w32(s + 0x14, fp as u32);
    for i in 0..VOICE_COUNT {
        let v = voice_addr(i);
        if m.r8(v + 4) as i8 as i32 == slot {
            let vh = m.r32(v) as i32;
            if vh >= 0 {
                let ch = m.r8(v + 5) as i8 as i32;
                let ch_word = m.r32(s.wrapping_add((ch as u32).wrapping_mul(4)).wrapping_add(0x30));
                let out = (((ch_word >> 10) & 0x7f) as i32).wrapping_mul(vol) >> 7;
                e.call(m, SND_HANDLE_SET_VOLUME, &[vh as u32, out as u32]);
            }
        }
    }
    0
}

/// `SndMidi_Fade` 0x2d4978: schedule a linear volume fade to `target` over `ticks` (clamped to `>= 1`).
pub fn fade(m: &mut Mem, handle: i32, ticks: i32, target: i32) -> i32 {
    let slot = validate(m, handle);
    if slot < 0 {
        return slot;
    }
    let s = slot_addr(slot);
    if m.r8(s + 0x28) as i8 == 3 {
        let n = if ticks < 1 { 1 } else { ticks };
        let goal = target.wrapping_shl(16);
        let cur = m.r32(s + 0x14) as i32;
        m.w32(s + 0x1c, goal as u32);
        m.w32(s + 0x18, (goal.wrapping_sub(cur) / n) as u32);
    }
    0
}

/// `SndMidi_GetState` 0x2d4a08: return slot state (`2` stopped, `3` playing, `5` paused, `6` fading), or `-8`.
pub fn get_state(m: &Mem, handle: i32) -> i32 {
    let slot = validate(m, handle);
    if slot < 0 {
        return slot;
    }
    let s = slot_addr(slot);
    let st = m.r8(s + 0x28) as i8 as i32;
    if st == 3 && m.r32(s + 0x18) != 0 {
        6
    } else {
        st
    }
}

// ---- cMidiSys (`MidiSys_*`, 0x222080..0x222798) ----

fn tab(m: &Mem, s: u32, o: u32, i: u32) -> u32 {
    i.wrapping_mul(4).wrapping_add(m.r32(s + o))
}

fn byte(v: i32) -> u8 {
    if v < 0x7f {
        if v < 0 {
            0
        } else {
            v as u8
        }
    } else {
        0x7f
    }
}

fn echo_clamp(v: i32) -> i32 {
    let v = if v < 0x80 { v } else { 0x7f };
    if -1 < v {
        v
    } else {
        0
    }
}

/// `MidiSys_Init` 0x222080: construct `cMidiSys` on top of `cBankSys`, init `SndMidi`, allocate the 32-byte
/// MIDI queue array (`+0xa48`), and store `this` in `gMidiSys` (`0x343124`).
pub fn midisys_init(m: &mut Mem, e: &mut dyn SongEnv, this: u32) -> u32 {
    e.call(m, BANK_SYS_INIT, &[this]);
    m.w32(this + 4, MIDI_SYS_VTABLE);
    e.call(m, SND_MIDI_INIT_ADDR, &[]);
    let buf = e.call(m, EAMEM_ALLOC, &[MIDI_BANKS_TAG, 0x20, 0]);
    if buf != 0 {
        e.call(m, EAMEM_SET, &[buf, 0xdead_c0ed, 0x20]);
    }
    m.w32(this + 0xa48, buf);
    m.w32(MIDI_SYS_GLOBAL, this);
    this
}

/// `cMidiSys::~cMidiSys` 0x222100: free `+0xa48`, clear `gMidiSys`, shut down `SndMidi`, destroy `cBankSys`.
pub fn midisys_dtor(m: &mut Mem, e: &mut dyn SongEnv, this: u32, flags: u32) {
    m.w32(this + 4, MIDI_SYS_VTABLE);
    let buf = m.r32(this + 0xa48);
    if buf != 0 {
        e.call(m, MEM_FREE, &[buf]);
    }
    m.w32(MIDI_SYS_GLOBAL, 0);
    e.call(m, SND_MIDI_SHUTDOWN_ADDR, &[]);
    e.call(m, BANK_SYS_DTOR, &[this, flags]);
}

/// `MidiSys_PushEvent` 0x222160: push a pending MIDI play entry (`bank_kind`, `file_data`) onto `this`'s queue.
pub fn midisys_push_event(m: &mut Mem, e: &mut dyn SongEnv, this: u32, bank_kind: u32, file_data: u32) {
    let i = m.r32(this + 0x20).wrapping_add(1);
    m.w32(this + 0x20, i);
    let rec = i.wrapping_mul(0x18).wrapping_add(m.r32(this + 0x24));
    let copy: Vec<u8> = (0..0x18).map(|k| m.r8(this + 8 + k)).collect();
    for (k, b) in copy.iter().enumerate() {
        m.w8(rec + k as u32, *b);
    }
    let i = m.r32(this + 0x20);
    m.w32(this.wrapping_add(i.wrapping_mul(4)).wrapping_add(0x28), 0);
    m.w32(tab(m, this, 0x38, i), 0);
    let vol = m.r8(i.wrapping_mul(0x18).wrapping_add(m.r32(this + 0x24))) as i8 as i32;
    m.w32(tab(m, this, 0x3c, i), vol as u32);
    m.w32(tab(m, this, 0x40, i), 0);
    e.call(m, MEMSET, &[this.wrapping_add(i.wrapping_mul(8)).wrapping_add(0x44), 0, 8]);
    let i = m.r32(this + 0x20);
    let row = m.r32(this + 0x74).wrapping_add(i.wrapping_mul(0x10));
    m.w32(tab(m, this, 0x68, i), 0);
    m.w32(row + 4, 0);
    m.w32(tab(m, this, 0x6c, i), 0);
    m.w32(row, 100);
    m.w32(tab(m, this, 0x64, i), 0);
    m.w32(tab(m, this, 0x70, i), 0);
    m.w32(row + 8, 0x5a);
    m.w32(row + 0x0c, 0x32);
    m.w32(tab(m, this, 0x78, i), 0);
    let midi = m.r32(this + 0xa48).wrapping_add(i.wrapping_mul(8));
    m.w32(midi, bank_kind);
    m.w32(tab(m, this, 0x7c, i), 1);
    m.w32(midi + 4, file_data);
}

/// `MidiSys_PlayPending` 0x222358: play the top queued MIDI entry via `SndMidi_Play`, register it with the
/// bank monitor (engine 2), schedule any delay echoes (`MidiSys_PlayEventCallback` 0x222798), and pop the queue.
pub fn midisys_play_pending(m: &mut Mem, e: &mut dyn SongEnv, s: u32) -> u32 {
    let t3c = m.r32(s + 0x3c);
    let i = m.r32(s + 0x20);
    let emitter = m.r32(s.wrapping_add(i.wrapping_mul(4)).wrapping_add(0x28));
    let vol0 = m.r32(i.wrapping_mul(4).wrapping_add(t3c)) as i32;
    let at = |m: &Mem| m.r32(s + 0x20).wrapping_mul(4).wrapping_add(t3c);
    let rec = |m: &Mem| m.r32(s + 0x20).wrapping_mul(0x18).wrapping_add(m.r32(s + 0x24));
    if emitter != 0 {
        let a = at(m);
        m.w32(a, 0);
        let r = rec(m);
        let b = byte(m.r32(at(m)) as i32);
        m.w8(r, b);
    } else {
        let slot = m.r32(tab(m, s, 0x40, m.r32(s + 0x20)));
        if slot != 0 {
            let scale = F(f32::from_bits(m.r32(slot)));
            let store = at(m);
            let scaled = |m: &Mem| F(m.r32(at(m)) as i32 as f32) * scale;
            let v = if scaled(m).ee() < 127.0 && scaled(m).ee() < 0.0 {
                0
            } else if scaled(m).ee() < 127.0 {
                scaled(m).ee() as i32 as u32
            } else {
                0x7f
            };
            m.w32(store, v);
            let r = rec(m);
            let b = byte(m.r32(at(m)) as i32);
            m.w8(r, b);
        }
    }
    let mut played = u32::MAX;
    let i8 = m.r32(s + 0x20).wrapping_mul(8);
    let midi = i8.wrapping_add(m.r32(s + 0xa48));
    let bank_kind = m.r32(midi);
    let bank_rec = bank_kind.wrapping_mul(0x60).wrapping_add(m.r32(s + 0xa3c));
    let bank_h = if m.r32(bank_rec) == 1 { m.r32(bank_rec + 4) as i32 } else { -1 };
    let cull = f32::from_bits(m.r32(tab(m, s, 0x70, m.r32(s + 0x20))));
    if cull == 0.0 && bank_h >= 0 {
        let file_data = m.r32(midi + 4);
        e.call(m, SND_LOCK, &[s]);
        let r = rec(m);
        played = e.call(m, SND_MIDI_PLAY_ADDR, &[r, file_data, bank_h as u32, u32::MAX]);
        e.call(m, SND_UNLOCK, &[]);
    }
    let inst = e.call(m, BANK_MONITOR_ADD_INSTANCE, &[s, played, vol0 as u32, 2]);
    if inst != 0 {
        let file_data = m.r32(i8.wrapping_add(m.r32(s + 0xa48)) + 4);
        e.call(m, BANK_INSTANCE_SET_COPY1, &[inst, file_data, bank_h as u32]);
    }
    let row = m.r32(s + 0x74).wrapping_add(m.r32(s + 0x20).wrapping_mul(0x10));
    let n = m.r32(row + 4) as i32;
    if 0 < n {
        let first = vol0.wrapping_mul(m.r32(row + 0x0c) as i32) / 100;
        let fb = m.r32(row + 8) as i32;
        m.w32(row + 4, n.wrapping_sub(1) as u32);
        let mut v = echo_clamp(first.wrapping_mul(fb) / 100);
        let mut delay = m.r32(row);
        let d = [m.r32(EV_MIDI_ECHO), m.r32(EV_MIDI_ECHO + 4)];
        while v != 0 {
            let slot = m.r32(tab(m, s, 0x40, m.r32(s + 0x20)));
            let mp = i8.wrapping_add(m.r32(s + 0xa48));
            let kind = m.r32(mp);
            let file_data = m.r32(mp + 4);
            e.call(
                m,
                SND_SYS_SCHEDULE_EVENT,
                &[s, delay, d[0], d[1], slot, kind, file_data, v as u32, 0],
            );
            let n = m.r32(row + 4) as i32;
            if n <= 0 {
                break;
            }
            let fb = m.r32(row + 8) as i32;
            let step = m.r32(row);
            m.w32(row + 4, n.wrapping_sub(1) as u32);
            v = echo_clamp(v.wrapping_mul(fb) / 100);
            delay = delay.wrapping_add(step);
        }
    }
    m.w32(s + 0x20, m.r32(s + 0x20).wrapping_sub(1));
    played
}

/// `MidiSys_PlayEventCallback` 0x222798: delayed-echo callback that re-queues and plays a MIDI event on `gMidiSys`.
pub fn midisys_play_event_callback(m: &mut Mem, e: &mut dyn SongEnv, ev: u32) -> u32 {
    let sys = m.r32(MIDI_SYS_GLOBAL);
    let (bank_kind, file_data) = (m.r32(ev + 0x1c), m.r32(ev + 0x20));
    midisys_push_event(m, e, sys, bank_kind, file_data);
    let sys = m.r32(MIDI_SYS_GLOBAL);
    let i = m.r32(sys + 0x20);
    let slot = m.r32(ev + 0x18);
    let vol = m.r32(ev + 0x24);
    m.w32(tab(m, sys, 0x3c, i), vol);
    m.w32(tab(m, sys, 0x40, i), slot);
    let r = m.r32(sys + 0x20).wrapping_mul(0x18).wrapping_add(m.r32(sys + 0x24));
    let b = byte(m.r32(tab(m, sys, 0x3c, m.r32(sys + 0x20))) as i32);
    m.w8(r, b);
    let sys = m.r32(MIDI_SYS_GLOBAL);
    midisys_play_pending(m, e, sys);
    1
}
