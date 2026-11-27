//! The speech system's glue (board row G10c2): the speech system object (`cSpeechSys`, built on the stream system
//! at `gSpeechSys` 0x343700), its two slots (+0x3718: the announcer, the riders), each slot's banks, and the calls
//! that start a line against the stream playing, on the game's bytes (`pf::Mem`), checked in the function runner
//! (`tests/speechsys.rs`). Everything outside (the allocator, the stream objects, the speech library, the bank
//! loader `Speech_LoadEvents`) is a call through [`SysEnv`], answered by address.
//!
//! A slot (0x36c bytes, filled with 0xdeadc0de when allocated): +0 its stream, +4 the stream it follows (−1 none), +8
//! a byte, +0xc8 four pairs (a block, its flag), +0xe8 the count of open archives at +0x358, +0xec the count of banks
//! at +0xf0, +0x348 the follow flag, +0x34c / +0x350 / +0x354 the play call's last three words, +0x368 its float. A
//! bank's +0 is its id, +4 its name.

use crate::pf::Mem;
use crate::ps2f::F;
use crate::replaymgr::GAPP;

/// What the speech system calls out to
pub trait SysEnv {
    /// The call to the function at `target` with its arguments (a0.., then t0..); its answer (v0)
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32;
    /// The same for a call answering a float (f0)
    fn call_f(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> f32;
}

/// `gSpeechSys`, the speech system (and the stream system its slots play on)
pub const SYS: u32 = 0x343700;
/// How many banks are loaded
pub const BANK_COUNT: u32 = 0x343704;
/// The front end's lines (two words each)
pub const FE_LINES: u32 = 0x343710;
/// `cSpeechSys`'s vtable
pub const VTABLE: u32 = 0x3a0470;

pub const ALLOC: u32 = 0x2ccf70;
pub const FREE: u32 = 0x2ccfc0;
pub const MEMSET: u32 = 0x2cc808;
pub const DELETE: u32 = 0x114f80;
pub const SPRINTF: u32 = 0x2fca28;
pub const LOAD_EVENTS: u32 = 0x227d70;
pub const MUSIC_INIT: u32 = 0x224a98;
pub const SCHEDULE: u32 = 0x222cc0;
pub const STREAM_DELETE: u32 = 0x229268;
pub const STREAM_STATE: u32 = 0x229870;
pub const STREAM_TIME_LEFT: u32 = 0x229578;
pub const STREAM_PLAY_TIME: u32 = 0x2296c0;
pub const STREAM_PLAYING: u32 = 0x229628;
pub const STREAM_WAIT: u32 = 0x2295c0;
pub const STREAM_STOP: u32 = 0x229fa0;
pub const STREAM_OPEN: u32 = 0x229ae8;
pub const LIB_SERVICE: u32 = 0x2bc548;
pub const LIB_UNLOAD_BANK: u32 = 0x2bb2b0;
pub const LIB_SHUTDOWN: u32 = 0x2bb018;
pub const LIB_REGISTER_BANK: u32 = 0x2bb4f0;
pub const FILE_EXISTS: u32 = 0x2c7e38;
pub const FILE_LOAD: u32 = 0x2c8018;
pub const STRCPY: u32 = 0x2fd14c;

fn cvt(v: f32) -> i32 {
    F(v).ee() as i32
}

fn stream(m: &Mem, sys: u32, slot: u32) -> u32 {
    m.r32(m.r32(sys + 0xa50).wrapping_add(slot.wrapping_mul(4)))
}

/// `StreamSys_GetSlotState` 0x229f60 (the stream system, a slot): its stream's state, 0 with none
pub fn slot_state(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32) -> u32 {
    let s = stream(m, sys, slot);
    if s == 0 { 0 } else { e.call(m, STREAM_STATE, &[s]) }
}

/// `StreamSys_GetSlotTimeLeftMs` 0x229fc8: the milliseconds left, 0.0 with no stream
pub fn time_left(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32) -> f32 {
    let s = stream(m, sys, slot);
    if s == 0 { 0.0 } else { e.call_f(m, STREAM_TIME_LEFT, &[s]) }
}

/// `StreamSys_GetPlayTime` 0x22a008: the milliseconds played, 0.0 with no stream
pub fn play_time(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32) -> f32 {
    let s = stream(m, sys, slot);
    if s == 0 { 0.0 } else { e.call_f(m, STREAM_PLAY_TIME, &[s]) }
}

/// `StreamSys_IsSlotPlaying` 0x22a178
pub fn is_playing(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32) -> u32 {
    let s = stream(m, sys, slot);
    if s == 0 { 0 } else { e.call(m, STREAM_PLAYING, &[s]) }
}

/// `StreamSys_HasReached` 0x22a048 (the stream system, a slot, milliseconds): the stream's answer, 0 with none
pub fn slot_wait(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32, ms: u32) -> u32 {
    let s = stream(m, sys, slot);
    if s == 0 { 0 } else { e.call(m, STREAM_WAIT, &[s, ms]) }
}

/// `StreamSys_DestroyStream` 0x229d40: the slot's stream deleted, its entry cleared
pub fn destroy_stream(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32) {
    let s = stream(m, sys, slot);
    if s != 0 {
        e.call(m, STREAM_DELETE, &[s, 3]);
    }
    let p = m.r32(sys + 0xa50).wrapping_add(slot.wrapping_mul(4));
    m.w32(p, 0);
}

/// `SpeechBank_Delete` 0x227c40 (a bank, the delete flags): its library bank unloaded, its block freed when it owns
/// one (+0x88 set, the block at +0x84), and itself with flag 1
pub fn bank_delete(m: &mut Mem, e: &mut dyn SysEnv, b: u32, flags: u32) {
    let id = m.r32(b);
    e.call(m, LIB_UNLOAD_BANK, &[id]);
    if m.r32(b + 0x88) != 0 {
        let p = m.r32(b + 0x84);
        e.call(m, FREE, &[p]);
    }
    if flags & 1 != 0 {
        e.call(m, DELETE, &[b]);
    }
}

/// `SpeechBank_HasId` 0x227ca8
pub fn bank_has_id(m: &Mem, b: u32, id: u32) -> bool {
    id == m.r32(b)
}

/// `SpeechBank_Init` 0x227cb8 (the slot, a1, its stream, a3, the stream's size, the stream it follows): the slot's
/// words cleared, its stream opened (`StreamSys_Open`, 0x20000 bytes) and its events loaded (`Speech_LoadEvents`
/// with a1 and a3)
#[allow(clippy::too_many_arguments)]
pub fn slot_init(m: &mut Mem, e: &mut dyn SysEnv, b: u32, a1: u32, stream: u32, a3: u32, size: u32, follows: u32) -> u32 {
    m.w32(b, stream);
    m.w32(b + 4, follows);
    for o in [0xe8, 0xec, 0x348, 0x34c, 0x350, 0x354, 0x368] {
        m.w32(b + o, 0);
    }
    m.w8(b + 8, 0);
    for k in 0..4 {
        m.w32(b + 0xc8 + 8 * k, 0);
        m.w32(b + 0xcc + 8 * k, 0);
    }
    let (sys, s) = (m.r32(SYS), m.r32(b));
    e.call(m, STREAM_OPEN, &[sys, s, size, 0, 0x20000]);
    e.call(m, LOAD_EVENTS, &[b, a1, a3]);
    b
}

/// `SpeechSys_FreeBanks` 0x228168 (a slot, the delete flags): its banks deleted (the loaded count less by them),
/// its archives and blocks freed, its stream destroyed, and itself with flag 1
pub fn slot_free(m: &mut Mem, e: &mut dyn SysEnv, b: u32, flags: u32) {
    let mut n = m.r32(b + 0xec) as i32;
    let mut i = 0;
    while i < n {
        let k = m.r32(b + 0xf0 + 4 * i as u32);
        if k != 0 {
            bank_delete(m, e, k, 3);
            n = m.r32(b + 0xec) as i32;
        }
        i += 1;
    }
    m.w32(BANK_COUNT, m.r32(BANK_COUNT).wrapping_sub(n as u32));
    let mut i = 0u32;
    while (i as i32) < m.r32(b + 0xe8) as i32 {
        let a = m.r32(b + 0x358 + 4 * i);
        if a != 0 {
            e.call(m, FREE, &[a]);
        }
        if m.r32(b + 0xcc + 8 * i) != 0 {
            let p = m.r32(b + 0xc8 + 8 * i);
            e.call(m, FREE, &[p]);
            m.w32(b + 0xcc + 8 * i, 0);
            m.w32(b + 0xc8 + 8 * i, 0);
        }
        i += 1;
    }
    let (sys, s) = (m.r32(SYS), m.r32(b));
    destroy_stream(m, e, sys, s);
    if flags & 1 != 0 {
        e.call(m, DELETE, &[b]);
    }
}

/// `SpeechSys_IsIdle` 0x2282e0 (a slot): its stream is free, or playing (state 1) with under 500 ms left
pub fn slot_idle(m: &mut Mem, e: &mut dyn SysEnv, b: u32) -> bool {
    let (sys, s) = (m.r32(SYS), m.r32(b));
    let st = slot_state(m, e, sys, s);
    if st == 0 {
        return true;
    }
    if st != 1 {
        return false;
    }
    let (sys, s) = (m.r32(SYS), m.r32(b));
    if is_playing(m, e, sys, s) == 0 {
        return false;
    }
    let (sys, s) = (m.r32(SYS), m.r32(b));
    cvt(time_left(m, e, sys, s)) < 500
}

/// `SpeechSys_ServiceIfIdle` 0x228370 (a slot, its number): the library serviced for it when the slot is idle
pub fn service_if_idle(m: &mut Mem, e: &mut dyn SysEnv, b: u32, slot: u32) {
    if slot_idle(m, e, b) {
        e.call(m, LIB_SERVICE, &[slot]);
    }
}

/// `Speech_PlayEvent` 0x228400 (a slot, the event: its bank id, its line; `sp` the frame, 0x1a0 bytes): the line of
/// the first bank with the id, named "<bank's name>" into the frame (the format at 0x3a0418), played by the stream
/// system's play call (vtable +0x7c: the slot's stream, the name, the line, +0x350, the start, +0x354, the float
/// +0x368). With the slot following another stream (+0x348 set, +4 not −1) the line starts at once (−1) and is
/// scheduled (`SndSys_ScheduleEvent`, the doubleword at 0x3a0420) to the other's time left less 30 ms, plus its play
/// time when that is under 300 ms. While the slot's own stream is busy that holds only when it is playing, and with
/// 300 ms or more left the line is played as usual (start +0x34c, not scheduled) unless the two streams' times left
/// are 40 ms or more apart, when the play time is added only if the other's time less 30 ms is under the slot's own.
/// The play call's answer, −1 when no bank has the id
pub fn play_event(m: &mut Mem, e: &mut dyn SysEnv, b: u32, ev: u32, sp: u32) -> i32 {
    let n = m.r32(b + 0xec) as i32;
    let mut i = 0;
    while i < n {
        let bank = m.r32(b + 0xf0 + 4 * i as u32);
        if !bank_has_id(m, bank, m.r32(ev)) {
            i += 1;
            continue;
        }
        e.call(m, SPRINTF, &[sp, 0x3a0418, bank.wrapping_add(4)]);
        let sys = m.r32(SYS);
        let pt = cvt(play_time(m, e, sys, m.r32(b + 4)));
        let sys = m.r32(SYS);
        let mut delay = cvt(time_left(m, e, sys, m.r32(b + 4)));
        let sys = m.r32(SYS);
        let own = cvt(time_left(m, e, sys, m.r32(b)));
        let gap = own.wrapping_sub(delay).wrapping_abs();
        let sys = m.r32(SYS);
        let busy = slot_state(m, e, sys, m.r32(b)) != 0;
        let follow = m.r32(b + 0x348) != 0 && m.r32(b + 4) != u32::MAX;
        let scheduled = if !follow {
            false
        } else if !busy {
            delay = delay.wrapping_sub(30);
            if delay <= 299 {
                delay = delay.wrapping_add(pt);
            }
            true
        } else {
            let sys = m.r32(SYS);
            if is_playing(m, e, sys, m.r32(b)) == 0 {
                false
            } else if delay < 0x12d {
                delay = delay.wrapping_sub(30).wrapping_add(pt);
                true
            } else if gap < 0x28 {
                false
            } else {
                let behind = (delay.wrapping_sub(30) < own) as i32;
                delay = behind.wrapping_mul(pt).wrapping_add(delay).wrapping_sub(30);
                true
            }
        };
        let start = if scheduled { u32::MAX } else { m.r32(b + 0x34c) };
        let sys = m.r32(SYS);
        let vt = m.r32(sys + 4);
        let f = m.r32(vt + 0x7c);
        let this = sys.wrapping_add(m.r16(vt + 0x78) as i16 as u32);
        let args = [this, m.r32(b), sp, m.r32(ev + 4), m.r32(b + 0x350), start, m.r32(b + 0x354), m.r32(b + 0x368)];
        let h = e.call(m, f, &args) as i32;
        if !scheduled || h < 0 {
            return h;
        }
        let sys = m.r32(SYS);
        let (lo, hi) = (m.r32(0x3a0420), m.r32(0x3a0424));
        e.call(m, SCHEDULE, &[sys, delay as u32, lo, hi, 0, h as u32, 0, 0, 0]);
        return h;
    }
    -1
}

/// `SpeechBank_Clear` 0x227c20 (a bank): no library bank (+0 −1), no name, no block (+0x84) and not owning one (+0x88)
pub fn bank_clear(m: &mut Mem, b: u32) -> u32 {
    m.w32(b + 0x88, 0);
    m.w32(b, u32::MAX);
    m.w32(b + 0x84, 0);
    m.w8(b + 4, 0);
    b
}

/// `SpeechBank_Load` 0x227b38 (the bank's name, its data or 0; `sp` the frame, 0x150 bytes): with no data the file
/// named by the format at 0x3a03b0 is loaded (`File_LoadAlloc`) when it is there (else 0) and the bank owns it
/// (+0x88); a bank record (0x8c bytes, 0xdeadc0de) cleared, holding the data (+0x84), the library's bank
/// (`SpeechLib_RegisterBank`, +0) and the name (+4)
pub fn bank_load(m: &mut Mem, e: &mut dyn SysEnv, name: u32, data: u32, sp: u32) -> u32 {
    let owns = (data == 0) as u32;
    e.call(m, SPRINTF, &[sp, 0x3a03b0, name]);
    let mut data = data;
    if data == 0 {
        if e.call(m, FILE_EXISTS, &[sp]) == 0 {
            return 0;
        }
        data = e.call(m, FILE_LOAD, &[sp, 0]);
    }
    let p = e.call(m, ALLOC, &[0x3a03b8, 0x8c, 0]);
    if p != 0 {
        e.call(m, MEMSET, &[p, 0xdead_c0de, 0x8c]);
    }
    let b = bank_clear(m, p);
    m.w32(b + 0x88, owns);
    m.w32(b + 0x84, data);
    let id = e.call(m, LIB_REGISTER_BANK, &[data]);
    m.w32(b, id);
    e.call(m, STRCPY, &[b + 4, name]);
    b
}

/// `SpeechSys_LoadBank` 0x2283a8 (a slot, the bank's name, its data or 0; `sp` the frame, 0x20 bytes): `SpeechBank_Load` into
/// the slot's next bank (+0xf0), counted (+0xec and the loaded count 0x343704) when it loaded
pub fn load_bank(m: &mut Mem, e: &mut dyn SysEnv, b: u32, name: u32, data: u32, sp: u32) {
    let bank = bank_load(m, e, name, data, sp.wrapping_sub(0x150));
    let n = m.r32(b + 0xec);
    m.w32(b.wrapping_add(n.wrapping_mul(4)).wrapping_add(0xf0), bank);
    if bank != 0 {
        m.w32(b + 0xec, n.wrapping_add(1));
        let c = m.r32(BANK_COUNT).wrapping_add(1);
        m.w32(BANK_COUNT, c);
    }
}

/// `SpeechSys_PlayEventAnySlot` 0x228ae8 (the speech system, the event; `sp` the frame, 0x50 bytes): `Speech_PlayEvent` on the
/// first of the two slots that plays it; when that slot is the one +0x3724 names, its handle goes to the function
/// at +0x3720. The handle, −1 when neither plays it
pub fn play_event_any_slot(m: &mut Mem, e: &mut dyn SysEnv, this: u32, ev: u32, sp: u32) -> i32 {
    for i in 0..2u32 {
        let slot = m.r32(m.r32(this + 0x3718).wrapping_add(4 * i));
        if slot == 0 {
            continue;
        }
        let h = play_event(m, e, slot, ev, sp.wrapping_sub(0x1a0));
        if h < 0 {
            continue;
        }
        if m.r32(this + 0x3724) == i {
            let f = m.r32(this + 0x3720);
            e.call(m, f, &[h as u32]);
        }
        return h;
    }
    -1
}

/// 0x227b00, the speech library's play callback (`Speech_InitLibrary` gives it to `SpeechLib_Init`; the event; `sp`
/// the frame, 0x10 bytes):
/// −1 while 0x343708 is set, else `SpeechSys_PlayEventAnySlot` on `gSpeechSys`
pub fn lib_play_callback(m: &mut Mem, e: &mut dyn SysEnv, ev: u32, sp: u32) -> i32 {
    if m.r32(0x343708) != 0 {
        return -1;
    }
    let sys = m.r32(SYS);
    play_event_any_slot(m, e, sys, ev, sp.wrapping_sub(0x50))
}

/// `SpeechSys_StopStream` 0x2286e8 (a slot)
pub fn slot_stop(m: &mut Mem, e: &mut dyn SysEnv, b: u32) {
    let (sys, s) = (m.r32(SYS), m.r32(b));
    e.call(m, STREAM_STOP, &[sys, s]);
}

/// `SpeechSlot_IsPlaying` 0x228700 (a slot)
pub fn slot_playing(m: &mut Mem, e: &mut dyn SysEnv, b: u32) -> u32 {
    let (sys, s) = (m.r32(SYS), m.r32(b));
    is_playing(m, e, sys, s)
}

/// `SpeechSys_Init` 0x228728 (the speech system): the stream system built (`MusicSys_Init`), `gSpeechSys` set, the
/// vtable, +0x371c / +0x3720 0, +0x3724 −1, and the two slots' table (8 bytes, 0xdeadc0ed, then cleared) at +0x3718
pub fn init(m: &mut Mem, e: &mut dyn SysEnv, this: u32) -> u32 {
    e.call(m, MUSIC_INIT, &[this]);
    m.w32(SYS, this);
    m.w32(this + 0x371c, 0);
    m.w32(this + 0x3720, 0);
    m.w32(this + 4, VTABLE);
    m.w32(this + 0x3724, u32::MAX);
    let p = e.call(m, ALLOC, &[0x3a0428, 8, 0]);
    if p != 0 {
        e.call(m, MEMSET, &[p, 0xdead_c0ed, 8]);
    }
    m.w32(this + 0x3718, p);
    m.w32(p.wrapping_add(4), 0);
    m.w32(p, 0);
    this
}

/// 0x2bae08 (a rate, a width, a kind): the bytes of `rate × width` bits, a tenth of them for kind 1, two sevenths for
/// kind 2
pub fn buffer_bytes(rate: i32, width: i32, kind: i32) -> i32 {
    let bits = rate.wrapping_mul(width);
    let bytes = (if bits >= 0 { bits } else { bits.wrapping_add(7) }) >> 3;
    match kind {
        1 => bytes / 10,
        2 => (bytes << 1) / 7,
        _ => bytes,
    }
}

/// `Speech_InitLibrary` 0x228830 (the speech system): the library started (`SpeechLib_Init` with the callback
/// 0x227b00, the system's own answer from its vtable +0x24, and a tenth of a second at 22050 Hz, 16 bits), then 300
/// event slots allocated (+0x371c) and handed to the library
pub fn init_library(m: &mut Mem, e: &mut dyn SysEnv, this: u32) {
    let bytes = buffer_bytes(0x5622, 0x10, 1);
    let vt = m.r32(this + 4);
    let f = m.r32(vt + 0x24);
    let adj = this.wrapping_add(m.r16(vt + 0x20) as i16 as u32);
    let a = e.call(m, f, &[adj]);
    e.call(m, 0x188990, &[0x3a0438, a]);
    lib_init(m, e, 0x227b00, a, bytes as u32);
    let n = e.call(m, 0x2bb090, &[300]);
    let p = e.call(m, ALLOC, &[0x3a0450, n, 0]);
    m.w32(this + 0x371c, p);
    e.call(m, 0x2bb098, &[300, p]);
}

/// `SpeechSys_FreeAll` 0x2288d8: both slots freed and cleared, the library shut down, the event slots freed
pub fn free_all(m: &mut Mem, e: &mut dyn SysEnv, this: u32) {
    for i in 0..2u32 {
        let b = m.r32(m.r32(this + 0x3718) + 4 * i);
        if b != 0 {
            slot_free(m, e, b, 3);
        }
        let t = m.r32(this + 0x3718);
        m.w32(t + 4 * i, 0);
    }
    e.call(m, LIB_SHUTDOWN, &[]);
    let p = m.r32(this + 0x371c);
    e.call(m, FREE, &[p]);
    m.w32(this + 0x371c, 0);
}

/// `SpeechSys_UpdateAll` 0x228998: each slot serviced when idle
pub fn update_all(m: &mut Mem, e: &mut dyn SysEnv, this: u32) {
    for i in 0..2u32 {
        let b = m.r32(m.r32(this + 0x3718) + 4 * i);
        if b != 0 {
            service_if_idle(m, e, b, i);
        }
    }
}

/// `SpeechSys_AddBank` 0x228a00 (the speech system, the slot's number, then `SpeechBank_Init`'s stream, a3, size,
/// follows): a new slot (0x36c bytes, 0xdeadc0de) made and put in the table
#[allow(clippy::too_many_arguments)]
pub fn add_slot(m: &mut Mem, e: &mut dyn SysEnv, this: u32, slot: u32, stream: u32, a3: u32, size: u32, follows: u32) {
    let at = slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718));
    let p = e.call(m, ALLOC, &[0x3a0460, 0x36c, 0]);
    if p != 0 {
        e.call(m, MEMSET, &[p, 0xdead_c0de, 0x36c]);
    }
    let v = slot_init(m, e, p, slot, stream, a3, size, follows);
    m.w32(at, v);
}

/// `Speech_LoadEventsForSlot` 0x228ac0 (the speech system, the slot's number, a2): `Speech_LoadEvents` on the slot
/// (with the number and a2) when it exists
pub fn load_events_for_slot(m: &mut Mem, e: &mut dyn SysEnv, this: u32, slot: u32, a2: u32) {
    let b = m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718)));
    if b != 0 {
        e.call(m, LOAD_EVENTS, &[b, slot, a2]);
    }
}

/// `SpeechSys_IsSlotPlaying` 0x228dd0 (the speech system, the slot's number)
pub fn is_slot_playing(m: &mut Mem, e: &mut dyn SysEnv, this: u32, slot: u32) -> u32 {
    let b = m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718)));
    if b == 0 { 0 } else { slot_playing(m, e, b) }
}

/// `SpeechSys_CanPlay` 0x228958 (the speech system, the slot's number): the slot exists and is idle
/// (`SpeechSys_IsIdle`)
pub fn can_play(m: &mut Mem, e: &mut dyn SysEnv, this: u32, slot: u32) -> bool {
    let b = m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718)));
    b != 0 && slot_idle(m, e, b)
}

/// `SpeechSys_SetSlotHold` 0x228bb0 (the speech system, the slot's number, the value): the slot's +0x34c, the start
/// `Speech_PlayEvent` gives a line it does not schedule
pub fn set_slot_hold(m: &mut Mem, this: u32, slot: u32, v: u32) {
    let b = m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718)));
    if b != 0 {
        m.w32(b + 0x34c, v);
    }
}

/// `SpeechSys_Trigger` 0x228bd8 (f12 the gain, the speech system, the slot's number, the position, the callback, the
/// voice): with the slot there, its +0x350 the position, +0x354 the voice, +0x368 the gain when above 0; the callback
/// (when given) at +0x3720 with the slot's number at +0x3724 (`SpeechSys_PlayEventAnySlot` hands it the handle);
/// the library serviced for the slot (`SpeechLib_Service`); then +0x3724 −1, +0x3720 0, and the slot's gain, voice and
/// position 0
#[allow(clippy::too_many_arguments)]
pub fn trigger(m: &mut Mem, e: &mut dyn SysEnv, this: u32, gain: f32, slot: u32, at: u32, cb: u32, voice: u32) {
    let at_slot = slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718));
    let b = m.r32(at_slot);
    if b == 0 {
        return;
    }
    m.w32(b + 0x350, at);
    m.w32(b + 0x354, voice);
    if 0.0 < gain {
        m.w32(b + 0x368, gain.to_bits());
    }
    let b = m.r32(at_slot);
    m.w32(b + 0x354, voice);
    if cb != 0 {
        m.w32(this + 0x3720, cb);
        m.w32(this + 0x3724, slot);
    }
    e.call(m, LIB_SERVICE, &[slot]);
    m.w32(this + 0x3724, u32::MAX);
    m.w32(this + 0x3720, 0);
    let b = m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718)));
    m.w32(b + 0x368, 0);
    m.w32(b + 0x354, 0);
    m.w32(b + 0x350, 0);
}

/// `SpeechSys_FlushTriggers` 0x228c80 (the speech system, the slot's number): with the slot there, four bare
/// triggers while 0x343708 is set (the library's play callback refuses them: what is queued is let go)
pub fn flush_triggers(m: &mut Mem, e: &mut dyn SysEnv, this: u32, slot: u32) {
    if m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718))) == 0 {
        return;
    }
    m.w32(0x343708, 1);
    for _ in 0..4 {
        trigger(m, e, this, 0.0, slot, 0, 0, 0);
    }
    m.w32(0x343708, 0);
}

/// `SpeechSys_ClearEvent` 0x228d10 (the speech system, the slot's number): the triggers flushed as
/// `SpeechSys_FlushTriggers` does, then the slot's stream stopped (`SpeechSys_StopStream`)
pub fn clear_event(m: &mut Mem, e: &mut dyn SysEnv, this: u32, slot: u32) {
    if m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718))) == 0 {
        return;
    }
    flush_triggers(m, e, this, slot);
    let b = m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718)));
    slot_stop(m, e, b);
}

/// `SpeechSys_GetBankId` 0x228e10 (the speech system, the slot's number): the slot's stream (its +0), −1 with no slot
pub fn slot_stream(m: &Mem, this: u32, slot: u32) -> u32 {
    let b = m.r32(slot.wrapping_mul(4).wrapping_add(m.r32(this + 0x3718)));
    if b == 0 { u32::MAX } else { m.r32(b) }
}

/// `Speech_GetFEModeMask` 0x22e550: the game mode (gApp+0x58) as the front end's speech bit
pub fn fe_mode_mask(m: &Mem) -> u32 {
    match m.r32(m.r32(GAPP) + 0x58) {
        2 => 2,
        3 => 8,
        4 => 4,
        _ => 1,
    }
}

/// `Speech_PlayFELine` 0x22e5a8 (the speech system, a line or −1; `sp` its frame): the speech stopped (0x228d10
/// with 0), then for a line its two words (0x343710 + 8·line) and the mode's bit posted (`SpeechEvt_Post` 0x22e4e0)
/// and the slots serviced
pub fn play_fe_line(m: &mut Mem, e: &mut dyn SysEnv, this: u32, line: u32, sp: u32) {
    e.call(m, 0x228d10, &[this, 0]);
    if line == u32::MAX {
        return;
    }
    let at = FE_LINES + 8 * line;
    m.w32(sp + 4, m.r32(at + 4));
    m.w32(sp, m.r32(at));
    let mask = fe_mode_mask(m);
    m.w32(sp + 8, mask);
    e.call(m, 0x22e4e0, &[sp, sp | 4, sp | 8]);
    update_all(m, e, this);
}

/// `Speech_FreeNarration` 0x230d78: the narration's object (+0x7dbc) deleted and its block (+0x7dc0) freed, +0x7dc4
/// cleared
pub fn free_narration(m: &mut Mem, e: &mut dyn SysEnv, this: u32) {
    let o = m.r32(this + 0x7dbc);
    if o != 0 {
        e.call(m, 0x209b40, &[o, 3]);
        m.w32(this + 0x7dbc, 0);
    }
    let p = m.r32(this + 0x7dc0);
    if p == 0 {
        m.w32(this + 0x7dc4, 0);
    } else {
        e.call(m, 0x14a040, &[p]);
        m.w32(this + 0x7dc0, 0);
        m.w32(this + 0x7dc4, 0);
    }
}

/// `SpeechSys_IsCharScriptReady` 0x235710: 1 unless a script is set (+0x6e64) and its slot (0x228e10 with 1) has a
/// stream, then `StreamSys_HasReached` at 500 ms
pub fn char_script_ready(m: &mut Mem, e: &mut dyn SysEnv, this: u32) -> u32 {
    if m.r32(this + 0x6e64) == 0 {
        return 1;
    }
    let slot = e.call(m, 0x228e10, &[this, 1]);
    if slot_state(m, e, this, slot) == 0 {
        return 1;
    }
    slot_wait(m, e, this, slot, 500)
}

/// `SpeechLib_Init` 0x2baf38 (the callback, a1, the buffer's bytes): the library's parts reset in its order, its
/// globals set (0x348de0 the callback, 0x3494f0 a1, 0x3494f4 the bytes, 0x348dd4 0x2bbcf8, 0x348dc8 0x1789a34),
/// eight words at 0x3494f8 cleared; 1
pub fn lib_init(m: &mut Mem, e: &mut dyn SysEnv, cb: u32, a1: u32, bytes: u32) -> u32 {
    e.call(m, 0x2baed0, &[]);
    e.call(m, 0x2baeb8, &[]);
    m.w32(0x348de0, cb);
    m.w32(0x3494f4, bytes);
    m.w32(0x348dd4, 0x2bbcf8);
    m.w32(0x3494f0, a1);
    e.call(m, 0x2be8c8, &[a1]);
    e.call(m, 0x2bdae0, &[]);
    e.call(m, 0x2be208, &[0]);
    for k in (0..8u32).rev() {
        m.w32(0x3494f8 + 4 * k, 0);
    }
    e.call(m, 0x2bb7a0, &[]);
    e.call(m, 0x2badc0, &[]);
    e.call(m, 0x2bb060, &[]);
    e.call(m, 0x2bb810, &[]);
    m.w32(0x348dc8, 0x1789a34);
    1
}

/// 0x2bebe8: the library's fatal stop (0x3495d3 = 'B', then 0x2f5450 with 0, which does not come back)
pub fn lib_fatal(m: &mut Mem, e: &mut dyn SysEnv) {
    m.w8(0x3495d3, 0x42);
    e.call(m, 0x2f5450, &[0]);
}

pub const MEM_FREE: u32 = 0x14a040;
pub const MIDI_SYS_INIT: u32 = 0x222080;
pub const MUSIC_DTOR: u32 = 0x224c90;
pub const SLOT_SET_FOLLOW: u32 = 0x2286d8;
pub const STREAM_SET_VOLUME: u32 = 0x229398;
pub const STREAM_STOP_OBJ: u32 = 0x229868;
pub const MEMPOOL_ALLOC: u32 = 0x23d3b0;
pub const FILE_OPEN_BIG: u32 = 0x2c8668;
pub const BIG_FIND: u32 = 0x2cc010;
pub const SND_LOCK: u32 = 0x2d0908;
pub const SND_UNLOCK: u32 = 0x2d0938;
pub const SND_STREAM_OPEN: u32 = 0x2d2888;
pub const SND_STREAM_QUEUE_FILE: u32 = 0x2d2948;
pub const SND_STREAM_CALC_MEM_SIZE: u32 = 0x2d2ac0;
pub const SND_STREAM_GET_STATUS: u32 = 0x2d2b08;
pub const SND_HANDLE_GET_STATE: u32 = 0x2d2c10;
pub const SND_STREAM_SET_PREBUFFER_MS: u32 = 0x2d2d50;
pub const SND_STREAM_SET_VOLUME: u32 = 0x2d2e10;
pub const SND_STREAM_SET_PITCH: u32 = 0x2d2f10;
pub const STREAM_SYS_VT: u32 = 0x3a0598;
pub const MEMPOOL_GLOBAL: u32 = 0x345e48;
pub const TAG_STREAM_BUF: u32 = 0x3a0570;
pub const TAG_STREAM_ARRAY: u32 = 0x3a0578;
pub const TAG_STREAM_OBJ: u32 = 0x3a0588;

/// `cSpeechSys::~cSpeechSys` 0x2287d8(this, flags): restores `VTABLE` (`0x3a0470`) at `+4`, frees `*(this+0x3718)`
/// via `Mem_Free` (`0x14a040`) if non-null, clears `gSpeechSys` (`0x343700`), and tail-calls `cMusicSys::~cMusicSys`
/// (`0x224c90`)
pub fn speech_sys_destruct(m: &mut Mem, e: &mut dyn SysEnv, this: u32, flags: u32) {
    m.w32(this + 4, VTABLE);
    let p = m.r32(this + 0x3718);
    if p != 0 {
        e.call(m, MEM_FREE, &[p]);
    }
    m.w32(SYS, 0);
    e.call(m, MUSIC_DTOR, &[this, flags]);
}

/// `SpeechSys_SetFollowStream` 0x228b88(this, slot, val): calls `0x2286d8(entry, val)` when slot `slot`'s entry
/// (`*(this+0x3718)[slot]`) is non-null
pub fn speech_sys_set_follow(m: &mut Mem, e: &mut dyn SysEnv, this: u32, slot: u32, val: u32) {
    let p = m.r32(m.r32(this + 0x3718).wrapping_add(slot.wrapping_mul(4)));
    if p != 0 {
        e.call(m, SLOT_SET_FOLLOW, &[p, val]);
    }
}

/// `Stream_Init` 0x228fb0(st, sys, flags, extra_bytes): initializes stream fields, captures base volume byte (`+8`)
/// and scale pointer (`+0xc`) from `sys`'s top queue entry, scales and clamps the queued volume when the scale
/// pointer is set, allocates `SndStream_CalcMemSize(8, 300) + extra_bytes` (via `MemPool_Alloc` when bit 30 is set
/// else `EAMem_Alloc`, filling with `0` if bit 31 is set else `0xdeadc0ed`), opens the stream (`SndStream_Open`
/// `0x2d2888`) into `st+4`, and returns `st`
pub fn stream_init(m: &mut Mem, e: &mut dyn SysEnv, st: u32, sys: u32, flags: u32, extra_bytes: u32) -> u32 {
    m.w32(st + 0x74, 0);
    m.w32(st + 4, u32::MAX);
    m.w32(st + 0x78, flags);
    m.w32(st + 0x7c, 0);
    let idx = m.r32(sys + 0x20);
    let scale_ptr = m.r32(m.r32(sys + 0x40).wrapping_add(idx << 2));
    let vol = m.r32(m.r32(sys + 0x3c).wrapping_add(idx << 2));
    m.w32(st + 0x70, 0);
    m.w8(st + 8, vol as u8);
    m.w32(st + 0xc, scale_ptr);
    if scale_ptr != 0 {
        let scale = F(f32::from_bits(m.r32(scale_ptr)));
        let t3c = m.r32(sys + 0x3c);
        let at_vol = |m: &Mem| t3c.wrapping_add(m.r32(sys + 0x20) << 2);
        let scaled = |m: &Mem| crate::animseq::cvt_s_w(m.r32(at_vol(m)) as i32) * scale;
        let v = if scaled(m) < F(127.0) {
            if scaled(m) < F(0.0) {
                0u32
            } else {
                scaled(m).ee() as i32 as u32
            }
        } else {
            0x7f
        };
        m.w32(at_vol(m), v);
        let rec = m.r32(sys + 0x24).wrapping_add(m.r32(sys + 0x20).wrapping_mul(0x18));
        let sv = m.r32(at_vol(m)) as i32;
        let b = if sv < 0x7f {
            if sv < 0 { 0u8 } else { sv as u8 }
        } else {
            0x7f
        };
        m.w8(rec, b);
    }
    let sz = e.call(m, SND_STREAM_CALC_MEM_SIZE, &[8, 0x12c]).wrapping_add(extra_bytes);
    let buf = if flags & 0x4000_0000 != 0 {
        e.call(m, MEMPOOL_ALLOC, &[MEMPOOL_GLOBAL, sz, flags, TAG_STREAM_BUF, 0, 0])
    } else {
        e.call(m, ALLOC, &[TAG_STREAM_BUF, sz, flags & 0x03ff_ffff])
    };
    if buf != 0 {
        if (flags as i32) < 0 {
            e.call(m, MEMSET, &[buf, 0, sz]);
        } else {
            e.call(m, MEMSET, &[buf, 0xdead_c0ed, sz]);
        }
    }
    m.w32(st, buf);
    let rec = m.r32(sys + 0x24).wrapping_add(m.r32(sys + 0x20).wrapping_mul(0x18));
    let h = e.call(m, SND_STREAM_OPEN, &[rec, 8, 0x12c, buf, sz]);
    m.w32(st + 4, h);
    st
}

/// `Stream_Open` 0x2292e8(st, path): if `FileSys_Exists(path)` is non-zero, copies `path` into `st+0x10` and opens
/// the BIG archive via `File_OpenBig(st+0x10, *(st+0x78))` into `st+0x70`
pub fn stream_open(m: &mut Mem, e: &mut dyn SysEnv, st: u32, path: u32) {
    if e.call(m, FILE_EXISTS, &[path]) != 0 {
        e.call(m, STRCPY, &[st + 0x10, path]);
        let big = e.call(m, FILE_OPEN_BIG, &[st + 0x10, m.r32(st + 0x78)]);
        m.w32(st + 0x70, big);
    }
}

/// `Stream_UpdateVolume` 0x2293e0(st): if `*(st+0x7c) >= 121`, calls `Stream_Stop(st)` (`0x229868`) and clears
/// `+0x7c`; otherwise, if `*(st+0xc)` is non-null, multiplies signed byte `*(st+8)` by `**(st+0xc)`, clamps to
/// `0..=127`, and applies it under `SND_Lock`/`SND_Unlock` via `SndStream_SetVolume` (`0x2d2e10`)
pub fn stream_update_volume(m: &mut Mem, e: &mut dyn SysEnv, st: u32) {
    if (m.r32(st + 0x7c) as i32) >= 0x79 {
        e.call(m, STREAM_STOP_OBJ, &[st]);
        m.w32(st + 0x7c, 0);
        return;
    }
    let sp = m.r32(st + 0xc);
    if sp == 0 {
        return;
    }
    let base = (m.r8(st + 8) as i8) as f32;
    let scale = f32::from_bits(m.r32(sp));
    let mut vol = (F(base) * F(scale)).ee() as i32;
    if vol >= 0x80 {
        vol = 0x7f;
    }
    if vol < 0 {
        vol = 0;
    }
    e.call(m, SND_LOCK, &[]);
    let h = m.r32(st + 4);
    e.call(m, SND_STREAM_SET_VOLUME, &[h, vol as u32]);
    e.call(m, SND_UNLOCK, &[]);
}

/// `Stream_GetStatus` 0x229490(st, out): `SND_Lock`, `SndStream_GetStatus(*(st+4), out)`, `SND_Unlock`
pub fn stream_get_status(m: &mut Mem, e: &mut dyn SysEnv, st: u32, out: u32) -> u32 {
    e.call(m, SND_LOCK, &[]);
    let h = m.r32(st + 4);
    let rv = e.call(m, SND_STREAM_GET_STATUS, &[h, out]);
    e.call(m, SND_UNLOCK, &[]);
    rv
}

/// `Stream_GetPlayInfo` 0x2294d8(st, info_out, status_out; sp): calls [`stream_get_status`] into `status_out` (or
/// `sp - 0x40` when null), then under `SND_Lock`/`SND_Unlock` calls `SndHandle_GetState(*(sbuf+4), info_out)`
pub fn stream_get_play_info(m: &mut Mem, e: &mut dyn SysEnv, st: u32, info_out: u32, status_out: u32, sp: u32) -> u32 {
    let sbuf = if status_out != 0 { status_out } else { sp.wrapping_sub(0x40) };
    stream_get_status(m, e, st, sbuf);
    e.call(m, SND_LOCK, &[]);
    let h = m.r32(sbuf + 4);
    let rv = e.call(m, SND_HANDLE_GET_STATE, &[h, info_out]);
    e.call(m, SND_UNLOCK, &[]);
    rv
}

/// `Stream_GetPlayedMs` 0x229530(st; sp): queries [`stream_get_play_info`] into `sp - 0x20`; returns `*(ibuf+4)` as
/// float when `*(ibuf) == 2`, else `0.0`
pub fn stream_get_played_ms(m: &mut Mem, e: &mut dyn SysEnv, st: u32, sp: u32) -> f32 {
    let ibuf = sp.wrapping_sub(0x20);
    stream_get_play_info(m, e, st, ibuf, 0, ibuf);
    if m.r32(ibuf) == 2 {
        crate::animseq::cvt_s_w(m.r32(ibuf + 4) as i32).0
    } else {
        0.0
    }
}

/// `Stream_IsQueued` 0x2295f8(st; sp): queries [`stream_get_play_info`] into `sp - 0x20`; returns `1` if `*(ibuf) == 1`
pub fn stream_is_queued(m: &mut Mem, e: &mut dyn SysEnv, st: u32, sp: u32) -> u32 {
    let ibuf = sp.wrapping_sub(0x20);
    stream_get_play_info(m, e, st, ibuf, 0, ibuf);
    (m.r32(ibuf) == 1) as u32
}

/// `Stream_ClearPrebuffer` 0x229658(st; sp): gets status into `sp - 0x20` and calls `SndStream_SetPrebufferMs(*(sbuf+4), 0)`
pub fn stream_clear_prebuffer(m: &mut Mem, e: &mut dyn SysEnv, st: u32, sp: u32) {
    let sbuf = sp.wrapping_sub(0x20);
    stream_get_status(m, e, st, sbuf);
    let req = m.r32(sbuf + 4);
    e.call(m, SND_STREAM_SET_PREBUFFER_MS, &[req, 0]);
}

/// `Stream_GetHandle` 0x229680(st; sp): gets status into `sp - 0x20`; returns `-1` if `*(sbuf) == 0`, else `*(sbuf+4)`
pub fn stream_get_handle(m: &mut Mem, e: &mut dyn SysEnv, st: u32, sp: u32) -> u32 {
    let sbuf = sp.wrapping_sub(0x20);
    stream_get_status(m, e, st, sbuf);
    if m.r32(sbuf) == 0 {
        u32::MAX
    } else {
        m.r32(sbuf + 4)
    }
}

/// `Stream_ClearRequestPrebuffer` 0x2296b0(_st, req): tail-calls `SndStream_SetPrebufferMs(req, 0)` (`0x2d2d50`)
pub fn stream_clear_request_prebuffer(m: &mut Mem, e: &mut dyn SysEnv, req: u32) {
    e.call(m, SND_STREAM_SET_PREBUFFER_MS, &[req, 0]);
}

/// `Stream_Resume` 0x229710(st): `SND_Lock`, `SndStream_SetPitch(*(st+4), 0)`, `SND_Unlock`
pub fn stream_resume(m: &mut Mem, e: &mut dyn SysEnv, st: u32) {
    e.call(m, SND_LOCK, &[]);
    let h = m.r32(st + 4);
    e.call(m, SND_STREAM_SET_PITCH, &[h, 0]);
    e.call(m, SND_UNLOCK, &[]);
}

/// `Stream_Pause` 0x229740(st): `SND_Lock`, `SndStream_SetPitch(*(st+4), 0x1000)`, `SND_Unlock`
pub fn stream_pause(m: &mut Mem, e: &mut dyn SysEnv, st: u32) {
    e.call(m, SND_LOCK, &[]);
    let h = m.r32(st + 4);
    e.call(m, SND_STREAM_SET_PITCH, &[h, 0x1000]);
    e.call(m, SND_UNLOCK, &[]);
}

/// `Stream_PlayFile` 0x229770(st, a1, name, off; sp): if `*(st+0x70)` is an open BIG, looks up `name` via
/// `BIG_Find` (`0x2cc010`) into `sp - 0x50` and queues `st+0x10` at `off + *out`; else queues `name` at `off`
pub fn stream_play_file(m: &mut Mem, e: &mut dyn SysEnv, st: u32, a1: u32, name: u32, off: u32, sp: u32) -> u32 {
    let big = m.r32(st + 0x70);
    if big != 0 {
        let out = sp.wrapping_sub(0x50);
        e.call(m, BIG_FIND, &[big, name, 0, out, 0]);
        let h = m.r32(st + 4);
        let pos = off.wrapping_add(m.r32(out));
        e.call(m, SND_STREAM_QUEUE_FILE, &[h, a1, st + 0x10, pos])
    } else {
        let h = m.r32(st + 4);
        e.call(m, SND_STREAM_QUEUE_FILE, &[h, a1, name, off])
    }
}

/// `Stream_OpenInBig` 0x2297f0(st, a1, idx, off; sp): returns `0` if `*(st+0x70)` is null, else looks up entry `idx`
/// via `BIG_Find(big, 0, idx, sp - 0x50, 0)` and queues `st+0x10` at `off + *out`
pub fn stream_open_in_big(m: &mut Mem, e: &mut dyn SysEnv, st: u32, a1: u32, idx: u32, off: u32, sp: u32) -> u32 {
    let big = m.r32(st + 0x70);
    if big == 0 {
        return 0;
    }
    let out = sp.wrapping_sub(0x50);
    e.call(m, BIG_FIND, &[big, 0, idx, out, 0]);
    let h = m.r32(st + 4);
    let pos = off.wrapping_add(m.r32(out));
    e.call(m, SND_STREAM_QUEUE_FILE, &[h, a1, st + 0x10, pos])
}

/// `StreamSys_Init` 0x229890(sys, a1, count, a3): initializes `MidiSys_Init(sys, a1, a3)`, stores `count` at
/// `+0xa4c` and `STREAM_SYS_VT` (`0x3a0598`) at `+4`, allocates `4 * count` bytes (`EAMem_Alloc`, filled with
/// `0xdeadc0ed` if non-null), stores the array at `+0xa50`, zeroes its `count` slots, and returns `sys`
pub fn stream_sys_init(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, a1: u32, count: u32, a3: u32) -> u32 {
    e.call(m, MIDI_SYS_INIT, &[sys, a1, a3]);
    m.w32(sys + 0xa4c, count);
    m.w32(sys + 4, STREAM_SYS_VT);
    let bytes = count << 2;
    let arr = e.call(m, ALLOC, &[TAG_STREAM_ARRAY, bytes, 0]);
    if arr != 0 {
        e.call(m, MEMSET, &[arr, 0xdead_c0ed, bytes]);
    }
    m.w32(sys + 0xa50, arr);
    let n = m.r32(sys + 0xa4c) as i32;
    for i in 0..n {
        m.w32(arr.wrapping_add((i as u32) << 2), 0);
    }
    sys
}

/// `StreamSys_CreateStream` 0x2299f8(sys, slot, flags, extra_bytes): allocates a `0x80`-byte stream object (via
/// `MemPool_Alloc` when bit 30 is set else `EAMem_Alloc`, filling with `0` if bit 31 is set else `0xdeadc0de`),
/// initializes it via [`stream_init`], and stores it in `*(sys+0xa50)[slot]`
pub fn stream_sys_create_stream(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32, flags: u32, extra_bytes: u32) {
    let slot_ptr = m.r32(sys + 0xa50).wrapping_add(slot << 2);
    let p = if flags & 0x4000_0000 != 0 {
        e.call(m, MEMPOOL_ALLOC, &[MEMPOOL_GLOBAL, 0x80, flags, TAG_STREAM_OBJ, 0, 0])
    } else {
        e.call(m, ALLOC, &[TAG_STREAM_OBJ, 0x80, flags & 0x03ff_ffff])
    };
    if p != 0 {
        if (flags as i32) < 0 {
            e.call(m, MEMSET, &[p, 0, 0x80]);
        } else {
            e.call(m, MEMSET, &[p, 0xdead_c0de, 0x80]);
        }
    }
    let st = stream_init(m, e, p, sys, flags, extra_bytes);
    m.w32(slot_ptr, st);
}

/// `StreamSys_StopAll` 0x229d98(sys): calls [`stream_update_volume`] on every non-null stream in `0..*(sys+0xa4c)`
pub fn stream_sys_stop_all(m: &mut Mem, e: &mut dyn SysEnv, sys: u32) {
    let mut i = 0u32;
    while (i as i32) < m.r32(sys + 0xa4c) as i32 {
        let s = stream(m, sys, i);
        i += 1;
        if s != 0 {
            stream_update_volume(m, e, s);
        }
    }
}

/// `StreamSys_ResumeAll` 0x229e00(sys): calls [`stream_resume`] on every non-null stream in `0..*(sys+0xa4c)`
pub fn stream_sys_resume_all(m: &mut Mem, e: &mut dyn SysEnv, sys: u32) {
    let mut i = 0u32;
    while (i as i32) < m.r32(sys + 0xa4c) as i32 {
        let s = stream(m, sys, i);
        i += 1;
        if s != 0 {
            stream_resume(m, e, s);
        }
    }
}

/// `StreamSys_PauseAll` 0x229e68(sys): calls [`stream_pause`] on every non-null stream in `0..*(sys+0xa4c)`
pub fn stream_sys_pause_all(m: &mut Mem, e: &mut dyn SysEnv, sys: u32) {
    let mut i = 0u32;
    while (i as i32) < m.r32(sys + 0xa4c) as i32 {
        let s = stream(m, sys, i);
        i += 1;
        if s != 0 {
            stream_pause(m, e, s);
        }
    }
}

/// `StreamSys_PlayFile` 0x229ed0(sys, slot, a2, a3, t0; sp): calls [`stream_play_file`] on slot's stream, or `-1`
#[allow(clippy::too_many_arguments)]
pub fn stream_sys_play_file(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32, a2: u32, a3: u32, t0: u32, sp: u32) -> u32 {
    let s = stream(m, sys, slot);
    if s == 0 {
        u32::MAX
    } else {
        stream_play_file(m, e, s, a2, a3, t0, sp.wrapping_sub(0x10))
    }
}

/// `StreamSys_Play` 0x229f18(sys, slot, a2, a3, t0; sp): calls [`stream_open_in_big`] on slot's stream, or `-1`
#[allow(clippy::too_many_arguments)]
pub fn stream_sys_play(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32, a2: u32, a3: u32, t0: u32, sp: u32) -> u32 {
    let s = stream(m, sys, slot);
    if s == 0 {
        u32::MAX
    } else {
        stream_open_in_big(m, e, s, a2, a3, t0, sp.wrapping_sub(0x10))
    }
}

/// `StreamSys_IsSlotQueued` 0x22a088(sys, slot; sp): calls [`stream_is_queued`] on slot's stream, or `0`
pub fn stream_sys_is_slot_queued(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32, sp: u32) -> u32 {
    let s = stream(m, sys, slot);
    if s == 0 {
        0
    } else {
        stream_is_queued(m, e, s, sp.wrapping_sub(0x10))
    }
}

/// `SndHandle_IsPlaying` 0x22a120(_sys, handle; sp): queries `SndHandle_GetState(handle, sp - 0x30)` under
/// `SND_Lock`/`SND_Unlock`; returns `1` iff state is `2` and return value is non-negative
pub fn snd_handle_is_playing(m: &mut Mem, e: &mut dyn SysEnv, handle: u32, sp: u32) -> u32 {
    let ibuf = sp.wrapping_sub(0x30);
    e.call(m, SND_LOCK, &[]);
    let rc = e.call(m, SND_HANDLE_GET_STATE, &[handle, ibuf]) as i32;
    e.call(m, SND_UNLOCK, &[]);
    (m.r32(ibuf) == 2 && rc >= 0) as u32
}

/// `StreamSys_ClearRequestPrebuffer` 0x22a1e0(sys, slot, req): calls [`stream_clear_request_prebuffer`] if slot has a stream
pub fn stream_sys_clear_request_prebuffer(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32, req: u32) {
    if stream(m, sys, slot) != 0 {
        stream_clear_request_prebuffer(m, e, req);
    }
}

/// `StreamSys_SetVolume` 0x22a228(sys, slot, vol): calls `Stream_SetVolume(s, vol)` (`0x229398`) if slot has a stream
pub fn stream_sys_set_volume(m: &mut Mem, e: &mut dyn SysEnv, sys: u32, slot: u32, vol: u32) {
    let s = stream(m, sys, slot);
    if s != 0 {
        e.call(m, STREAM_SET_VOLUME, &[s, vol]);
    }
}

/// `StreamSys_GetStream` 0x22a2b8(sys, slot): returns `*(sys+0xa50)[slot]`
pub fn stream_sys_get_stream(m: &Mem, sys: u32, slot: u32) -> u32 {
    stream(m, sys, slot)
}

/// `SndEvent_ClearStreamPrebuffer` 0x22a2d0(_sys, ev): calls `SndStream_SetPrebufferMs(*(ev+0x1c), 0)` and returns `1`
pub fn snd_event_clear_stream_prebuffer(m: &mut Mem, e: &mut dyn SysEnv, ev: u32) -> u32 {
    let req = m.r32(ev + 0x1c);
    e.call(m, SND_STREAM_SET_PREBUFFER_MS, &[req, 0]);
    1
}
