//! The PS2 memory card driver `cPS2MemCard` (0x23d908..0x2400f8) on the game's bytes (board rows E12f1, E12f2a,
//! E12f2b): its constructor, reset, destructor, virtuals and helpers, `MemCard_Process` 0x23e0d0 (the state machine
//! the starters hand a state to), the per-frame step vf2 0x23edc8, the SJIS converters, and the front end's
//! `MemCard_FormatSaveTitle` 0x1ce8f8.
//!
//! The driver (`c` below): +0 its table, +4 the state, +0xc, +0x10 the port, +0x14 the files' count (4), +0x18,
//! +0x1c the file picked, +0x20 / +0x2c the buffer and size handed to a save or load, +0x24, +0x30 a buffer,
//! +0x34..+0x4c flags; +0x71, +0xb1, +0xf1, +0x131 strings of 0x40 (the name, the directory, the path, the
//! game's directory); the two ports' words from +0x174 (0x14 a port: +0x174, +0x178 the status, +0x17c the
//! result, +0x180 formatted, +0x184 changed); the files from +0x19c (0x70 a file: +0x19c, +0x1a0 flags, +0x1a4,
//! +0x1a8 the name); +0x380 a directory entry, +0x3c0 the directory read, +0x4c0 a name. The `sceMc*` calls,
//! `MemCard_Process` and the text calls go through [`FeEnv`]; `memset`, `strcpy` and `strlen` (library functions
//! the app hosts) are done here.

use crate::fe3dstate::FeEnv;
use crate::pf::Mem;

pub const VT: u32 = 0x3a4598;
pub const MEMCARD_VT: u32 = 0x3a47f0;
/// The last `sceMc*` result and the info words `sceMcGetInfo` fills
pub const RESULT: u32 = 0x345f54;
pub const INFO_TYPE: u32 = 0x345f4c;
pub const INFO_FREE: u32 = 0x345f50;
pub const INFO_FORMAT: u32 = 0x345f58;
/// The path patterns: "%s/%s" style (0x3a42a0) and the directory's (0x3a42a8)
pub const PATH_FMT: u32 = 0x3a42a0;
pub const DIR_FMT: u32 = 0x3a42a8;

pub const PROCESS: u32 = 0x23e0d0;
pub const MC_GET_DIR: u32 = 0x2f1d88;
pub const MC_FORMAT: u32 = 0x2f20b8;
pub const MC_GET_INFO: u32 = 0x2f1c00;
pub const MC_SYNC: u32 = 0x2f1ac8;
pub const SPRINTF: u32 = 0x2fca28;
pub const LOC_STRING: u32 = 0x2c5160;
pub const WSTR_COPY: u32 = 0x17cc80;
pub const WSTR_FORMAT: u32 = 0x17ce38;
pub const WSTR_TO_ASCII: u32 = 0x17ccf8;
pub const EA_FREE: u32 = 0x2ccfc0;
pub const MEM_DELETE: u32 = 0x114f80;
pub const APP: u32 = 0x337c58;

fn port(c: u32, i: u32) -> u32 {
    c.wrapping_add(i.wrapping_mul(0x14))
}
fn file(c: u32, i: u32) -> u32 {
    c.wrapping_add(i.wrapping_mul(0x70))
}
fn memset(m: &mut Mem, at: u32, n: u32) {
    for k in 0..n {
        m.w8(at + k, 0);
    }
}
fn strcpy(m: &mut Mem, dst: u32, src: u32) {
    let mut k = 0;
    loop {
        let b = m.r8(src + k);
        m.w8(dst + k, b);
        if b == 0 {
            break;
        }
        k += 1;
    }
}
fn strlen(m: &Mem, s: u32) -> u32 {
    (0..).find(|&k| m.r8(s + k) == 0).unwrap()
}
fn process(m: &mut Mem, e: &mut dyn FeEnv, c: u32, state: u32) {
    e.call(m, PROCESS, &[c, state]);
}
/// A virtual of the driver: the adjust halfword at `off` of its table, the function at `off + 4`
fn vcall(m: &mut Mem, e: &mut dyn FeEnv, c: u32, off: u32, args: &[u32]) -> u32 {
    let vt = m.r32(c);
    let me = c.wrapping_add(m.r16(vt + off) as i16 as u32);
    let mut a = vec![me];
    a.extend_from_slice(args);
    e.call(m, m.r32(vt + off + 4), &a)
}
fn loc(m: &mut Mem, e: &mut dyn FeEnv, id: u32) -> u32 {
    let l = m.r32(m.r32(APP) + 0x50);
    e.call(m, LOC_STRING, &[l, id])
}
/// The directory and path of the picked file: its name to +0xb1, then `sprintf(+0xf1, PATH_FMT, +0xb1, +0xb1)`
fn path_of_name(m: &mut Mem, e: &mut dyn FeEnv, c: u32, name: u32) {
    strcpy(m, c + 0xb1, name);
    e.call(m, SPRINTF, &[c + 0xf1, PATH_FMT, c + 0xb1, c + 0xb1]);
}

/// `cPS2MemCard::cPS2MemCard` 0x23d908(c): the table, then [`reset`]; `c`
pub fn construct(m: &mut Mem, c: u32) -> u32 {
    m.w32(c, VT);
    reset(m, c);
    c
}

/// `cPS2MemCard::vf1` 0x23d998(c): every field to its start: +0xc 0x16, the port 0, the shared result and info
/// words 0, the flags 0, +0x38 / +0x18 / +0x1c -1, the files' count 4, the strings, both ports' words, the files,
/// the directory and the name cleared
pub fn reset(m: &mut Mem, c: u32) {
    m.w32(c + 0xc, 0x16);
    m.w32(c + 0x10, 0);
    m.w32(INFO_TYPE, 0);
    m.w32(RESULT, 0);
    m.w32(INFO_FREE, 0);
    for at in [4, 0x3c, 0x4c, 0x20, 0x24, 0x2c, 0x30, 0x34, 0x40, 0x44] {
        m.w32(c + at, 0);
    }
    m.w32(c + 0x38, u32::MAX);
    m.w32(c + 0x14, 4);
    m.w32(c + 0x18, u32::MAX);
    m.w32(c + 0x1c, u32::MAX);
    for at in [0xb1, 0xf1, 0x71, 0x131] {
        memset(m, c + at, 0x40);
    }
    for i in 0..2 {
        for at in [0x174, 0x178, 0x17c, 0x180, 0x184] {
            m.w32(port(c, i) + at, 0);
        }
    }
    memset(m, c + 0x380, 0x40);
    memset(m, c + 0x19c, 0x1c0);
    memset(m, c + 0x3c0, 0x100);
    memset(m, c + 0x4c0, 0x20);
}

/// `cPS2MemCard::~cPS2MemCard` 0x23d940(c, how): the table back, the card closed ([`close`]), its buffer (+0x30)
/// freed if any, then `cMemCard::~cMemCard` 0x2400f8 (its table 0x3a47f0; deleted when `how` has bit 0)
pub fn destruct(m: &mut Mem, e: &mut dyn FeEnv, c: u32, how: u32) {
    m.w32(c, VT);
    close(m, e, c);
    let buf = m.r32(c + 0x30);
    if buf != 0 {
        e.call(m, EA_FREE, &[buf]);
    }
    memcard_destruct(m, e, c, how);
}

/// `cMemCard::~cMemCard` 0x2400f8(c, how)
pub fn memcard_destruct(m: &mut Mem, e: &mut dyn FeEnv, c: u32, how: u32) {
    m.w32(c, MEMCARD_VT);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[c]);
    }
}

/// `cPS2MemCard::vf5` 0x23db10(c, name): the game's directory name to +0x131, state 5
pub fn set_dir(m: &mut Mem, e: &mut dyn FeEnv, c: u32, name: u32) {
    strcpy(m, c + 0x131, name);
    process(m, e, c, 5);
}

/// `MemCard_ReadDir` 0x23db40(c): the directory (+0x3c0) cleared and read (`sceMcGetDir(port, 0, +0x131, 0, 4,
/// +0x3c0)`), the result kept; state 1 on an error, else 6
pub fn read_dir(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    memset(m, c + 0x3c0, 0x100);
    let r = e.call(m, MC_GET_DIR, &[m.r32(c + 0x10), 0, c + 0x131, 0, 4, c + 0x3c0]);
    m.w32(RESULT, r);
    process(m, e, c, if (r as i32) < 0 { 1 } else { 6 });
}

/// `cPS2MemCard::vf12` 0x23dbc8(c): how many of the files (+0x14, read again each time) have a name
pub fn named_files(m: &mut Mem, c: u32) -> u32 {
    let mut n = 0;
    let mut i = 0;
    if (m.r32(c + 0x14) as i32) > 0 {
        loop {
            let l = strlen(m, file(c, i) + 0x1a8);
            i += 1;
            if l != 0 {
                n += 1;
            }
            if (i as i32) >= (m.r32(c + 0x14) as i32) {
                break;
            }
        }
    }
    n
}

/// `cPS2MemCard::vf61` 0x23dc40(c, i): file `i` picked, state 0x22
pub fn pick_22(m: &mut Mem, e: &mut dyn FeEnv, c: u32, i: u32) {
    m.w32(c + 0x1c, i);
    process(m, e, c, 0x22);
}

/// 0x23dc50(c) (no function in the index): the picked file's path built and its entry read (`sceMcGetDir(port, 0,
/// +0xf1, 0, 1, +0x380)`); state 1 on an error, else 0x23
pub fn stat_picked(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    let name = file(c, m.r32(c + 0x1c)) + 0x1a8;
    path_of_name(m, e, c, name);
    let r = e.call(m, MC_GET_DIR, &[m.r32(c + 0x10), 0, c + 0xf1, 0, 1, c + 0x380]);
    m.w32(RESULT, r);
    process(m, e, c, if (r as i32) < 0 { 1 } else { 0x23 });
}

/// `cPS2MemCard::vf7` 0x23dd10(c): state 0x11
pub fn state_11(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    process(m, e, c, 0x11);
}

/// 0x23dd18(c) (no function in the index): `sceMcFormat(port, 0)`; state 1 on an error, else 0x12
pub fn format(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    let r = e.call(m, MC_FORMAT, &[m.r32(c + 0x10), 0]);
    m.w32(RESULT, r);
    process(m, e, c, if (r as i32) < 0 { 1 } else { 0x12 });
}

/// `cPS2MemCard::vf9` 0x23dd70(c, i): file `i` picked, state 0x13
pub fn pick_13(m: &mut Mem, e: &mut dyn FeEnv, c: u32, i: u32) {
    m.w32(c + 0x1c, i);
    process(m, e, c, 0x13);
}

/// `cPS2MemCard::vf14` 0x23dd80(c): the directory entry's word +0x10 (+0x390)
pub fn entry_word(m: &Mem, c: u32) -> u32 {
    m.r32(c + 0x390)
}

/// `cPS2MemCard::vf15` 0x23dd88(c, v): +0x24 = v
pub fn set_24(m: &mut Mem, c: u32, v: u32) {
    m.w32(c + 0x24, v);
}

/// `MemCard_BuildSavePath` 0x23dd90(c): the picked file's path, state 0x36
pub fn build_save_path(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    let name = file(c, m.r32(c + 0x1c)) + 0x1a8;
    path_of_name(m, e, c, name);
    process(m, e, c, 0x36);
}

/// `cPS2MemCard::vf4` 0x23ddf8(c, port): the port, state 3
pub fn set_port(m: &mut Mem, e: &mut dyn FeEnv, c: u32, p: u32) {
    m.w32(c + 0x10, p);
    process(m, e, c, 3);
}

/// `cPS2MemCard::vf39` 0x23de08(c): the port's changed flag cleared, its result -1 made 0
pub fn clear_changed(m: &mut Mem, c: u32) {
    m.w32(port(c, m.r32(c + 0x10)) + 0x184, 0);
    let at = port(c, m.r32(c + 0x10)) + 0x17c;
    if m.r32(at) == u32::MAX {
        m.w32(at, 0);
    }
}

/// `cPS2MemCard::vf27` 0x23de48(c, name): the name to +0xb1, `sprintf(+0xf1, DIR_FMT, +0xb1)`, state 0x14
pub fn open_dir(m: &mut Mem, e: &mut dyn FeEnv, c: u32, name: u32) {
    strcpy(m, c + 0xb1, name);
    e.call(m, SPRINTF, &[c + 0xf1, DIR_FMT, c + 0xb1]);
    process(m, e, c, 0x14);
}

/// `cPS2MemCard::BeginSave` 0x23dea8(c, name, size, buf): the path, size +0x2c, buffer +0x20, state 0x15
pub fn begin_save(m: &mut Mem, e: &mut dyn FeEnv, c: u32, name: u32, size: u32, buf: u32) {
    path_of_name(m, e, c, name);
    m.w32(c + 0x2c, size);
    m.w32(c + 0x20, buf);
    process(m, e, c, 0x15);
}

/// 0x23ecf0(c, i, dst) (no function in the index): file `i`'s name copied to `dst`
pub fn copy_name(m: &mut Mem, c: u32, i: u32, dst: u32) {
    strcpy(m, dst, file(c, i) + 0x1a8);
}

/// `cPS2MemCard::BeginSaveSlot` 0x23df20(c, i, size, buf): file `i`'s name (on the stack, `sp` the stack pointer
/// on entry) handed to the driver's own vf at +0xf0 (`BeginSave`)
pub fn begin_save_slot(m: &mut Mem, e: &mut dyn FeEnv, c: u32, i: u32, size: u32, buf: u32, sp: u32) {
    let name = sp - 0x80;
    copy_name(m, c, i, name);
    vcall(m, e, c, 0xf0, &[name, size, buf]);
}

/// `cPS2MemCard::SaveMore` 0x23df80(c, size, buf): state 0x17
pub fn save_more(m: &mut Mem, e: &mut dyn FeEnv, c: u32, size: u32, buf: u32) {
    m.w32(c + 0x2c, size);
    m.w32(c + 0x20, buf);
    process(m, e, c, 0x17);
}

/// `cPS2MemCard::vf34` 0x23df98(c): the state is 0x34 (saving)
pub fn saving(m: &Mem, c: u32) -> u32 {
    (m.r32(c + 4) == 0x34) as u32
}

/// `cPS2MemCard::vf36` 0x23dfa8(c): +0x44 = 1
pub fn flag_44(m: &mut Mem, c: u32) {
    m.w32(c + 0x44, 1);
}

/// `cPS2MemCard::EndSave` 0x23dfb8(c): state 0x18
pub fn end_save(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    process(m, e, c, 0x18);
}

/// 0x23dfc0(c, name, size, buf) (no function in the index; the load's `BeginSave`): state 0x1b
pub fn begin_load(m: &mut Mem, e: &mut dyn FeEnv, c: u32, name: u32, size: u32, buf: u32) {
    path_of_name(m, e, c, name);
    m.w32(c + 0x2c, size);
    m.w32(c + 0x20, buf);
    process(m, e, c, 0x1b);
}

/// `cPS2MemCard::BeginLoadSlot` 0x23e038(c, i, size, buf): file `i`'s name (on the stack) to [`begin_load`]
pub fn begin_load_slot(m: &mut Mem, e: &mut dyn FeEnv, c: u32, i: u32, size: u32, buf: u32, sp: u32) {
    let name = sp - 0x80;
    copy_name(m, c, i, name);
    begin_load(m, e, c, name, size, buf);
}

/// `cPS2MemCard::LoadMore` 0x23e090(c, size, buf): state 0x1d
pub fn load_more(m: &mut Mem, e: &mut dyn FeEnv, c: u32, size: u32, buf: u32) {
    m.w32(c + 0x2c, size);
    m.w32(c + 0x20, buf);
    process(m, e, c, 0x1d);
}

/// `cPS2MemCard::vf24` 0x23e0a8(c): the state is 0x35 (loading)
pub fn loading(m: &Mem, c: u32) -> u32 {
    (m.r32(c + 4) == 0x35) as u32
}

/// `cPS2MemCard::vf26` 0x23e0b8(c): +0x40 = 1
pub fn flag_40(m: &mut Mem, c: u32) {
    m.w32(c + 0x40, 1);
}

/// `cPS2MemCard::EndLoad` 0x23e0c8(c): state 0x1e
pub fn end_load(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    process(m, e, c, 0x1e);
}

/// `MemCard_StartGetInfo` 0x23e928(c): the info words and +0x20 cleared, `sceMcGetInfo(port, 0, type, free,
/// format)` started; +0x3c whether it was (answer 0)
pub fn start_get_info(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    m.w32(INFO_TYPE, 0);
    m.w32(c + 0x20, 0);
    m.w32(INFO_FORMAT, 0);
    m.w32(INFO_FREE, 0);
    let r = e.call(m, MC_GET_INFO, &[m.r32(c + 0x10), 0, INFO_TYPE, INFO_FREE, INFO_FORMAT]);
    m.w32(c + 0x3c, (r == 0) as u32);
}

/// `cPS2MemCard::vf10` 0x23e988(c): busy (a state, or the info pending)
pub fn busy(m: &Mem, c: u32) -> u32 {
    (m.r32(c + 4) != 0 || m.r32(c + 0x3c) != 0) as u32
}

/// `cPS2MemCard::vf8` 0x23e9b0(c): the current port's result is one of -1, -3, -8..-10, -10001, -10002
pub fn result_is_card_error(m: &Mem, c: u32) -> u32 {
    let s = m.r32(port(c, m.r32(c + 0x10)) + 0x17c) as i32;
    let yes = if s < -7 {
        if s < -10 {
            (-10002..=-10001).contains(&s)
        } else {
            true
        }
    } else {
        s == -3 || s == -1
    };
    yes as u32
}

/// `MemCard_GetSlotError` 0x23ea18(c, i): port `i`'s result when it is an error the game names (≤ 0 above -8, or
/// -10001 / -10002), else -10
pub fn slot_error(m: &Mem, c: u32, i: u32) -> u32 {
    let s = m.r32(port(c, i) + 0x17c) as i32;
    let keep = if s < -7 { (-10002..=-10001).contains(&s) } else { s <= 0 };
    if keep {
        s as u32
    } else {
        -10i32 as u32
    }
}

/// `cPS2MemCard::vf60` 0x23ea78(c): the current port has an error
pub fn has_error(m: &Mem, c: u32) -> u32 {
    (slot_error(m, c, m.r32(c + 0x10)) != 0) as u32
}

/// `MemCard_ClearCurrentSlotStatus` 0x23ea98(c): the current port's +0x174 and status cleared
pub fn clear_current_status(m: &mut Mem, c: u32) {
    m.w32(port(c, m.r32(c + 0x10)) + 0x174, 0);
    m.w32(port(c, m.r32(c + 0x10)) + 0x178, 0);
}

/// `MemCard_ResetState` 0x23eac0(c): +0x4c, the entry, the files and the directory cleared, then the name
/// ([`clear_name`])
pub fn reset_state(m: &mut Mem, c: u32) {
    m.w32(c + 0x4c, 0);
    memset(m, c + 0x380, 0x40);
    memset(m, c + 0x19c, 0x1c0);
    memset(m, c + 0x3c0, 0x100);
    clear_name(m, c);
}

/// `MemCard_ClearName` 0x23eb18(c): the entry at +0x380 cleared
pub fn clear_name(m: &mut Mem, c: u32) {
    memset(m, c + 0x380, 0x40);
}

/// `cPS2MemCard::vf40` 0x23eb28(c, i): port `i`'s result is -10002
pub fn no_card(m: &Mem, c: u32, i: u32) -> u32 {
    (m.r32(port(c, i) + 0x17c) as i32 == -10002) as u32
}

/// `MemCard_GetSlotStatus` 0x23eb48(c, i): port `i`'s status
pub fn slot_status(m: &Mem, c: u32, i: u32) -> u32 {
    m.r32(port(c, i) + 0x178)
}

/// `cPS2MemCard::vf16` 0x23eb60(c): the current port's status
pub fn current_status(m: &Mem, c: u32) -> u32 {
    slot_status(m, c, m.r32(c + 0x10))
}

/// `cPS2MemCard::vf44` 0x23eb80(c, i): port `i`'s result is -7, -10001 or 0..-5
pub fn usable(m: &Mem, c: u32, i: u32) -> u32 {
    let s = m.r32(port(c, i) + 0x17c) as i32;
    (s == -7 || s == -10001 || (-5..=0).contains(&s)) as u32
}

/// `cPS2MemCard::vf47` 0x23ebd8(c, i): port `i` is formatted (+0x180)
pub fn formatted(m: &Mem, c: u32, i: u32) -> u32 {
    (m.r32(port(c, i) + 0x180) != 0) as u32
}

/// 0x23ebf0(c, i) (no function in the index): port `i` changed (+0x184)
pub fn changed(m: &Mem, c: u32, i: u32) -> u32 {
    (m.r32(port(c, i) + 0x184) != 0) as u32
}

/// 0x23ec08(c, i, out) (no function in the index): port `i`'s line: unformatted or result -2 the string 0x1d0,
/// result 0 the string 0x138 with the status (negative as 0), else an empty string
pub fn slot_text(m: &mut Mem, e: &mut dyn FeEnv, c: u32, i: u32, out: u32) {
    let p = port(c, i);
    let unformatted = m.r32(p + 0x180) == 0;
    let s = m.r32(p + 0x17c) as i32;
    if unformatted || s == -2 {
        let t = loc(m, e, 0x1d0);
        e.call(m, WSTR_COPY, &[out, t]);
    } else if s == 0 {
        let n = slot_status(m, c, i) as i32;
        let n = if n > -1 { n } else { 0 };
        let t = loc(m, e, 0x138);
        e.call(m, WSTR_FORMAT, &[out, t, n as u32]);
    } else {
        m.w16(out, 0);
    }
}

/// `cPS2MemCard::vf37` 0x23ed08(c, ?): `WStr_ToAscii(c + 0x50, ..)` (the rest of the arguments passed on)
pub fn vf37(m: &mut Mem, e: &mut dyn FeEnv, c: u32, a1: u32, a2: u32) -> u32 {
    e.call(m, WSTR_TO_ASCII, &[c + 0x50, a1, a2])
}

/// `cPS2MemCard::vf56` 0x23ed18(c, i): file `i` has a name
pub fn has_name(m: &Mem, c: u32, i: u32) -> u32 {
    (strlen(m, file(c, i) + 0x1a8) != 0) as u32
}

/// `MemCard_FlagSlot` 0x23ed48(c, i), `MemCard_MarkSlotDirty` 0x23ed60(c, i): file `i`'s +0x19c / +0x1a0 = 1
pub fn flag_file(m: &mut Mem, c: u32, i: u32, at: u32) {
    m.w32(file(c, i) + at, 1);
}

/// `cPS2MemCard::vf54` 0x23ed78(c, i): file `i` has either flag
pub fn file_flagged(m: &Mem, c: u32, i: u32) -> u32 {
    let f = file(c, i);
    (m.r32(f + 0x1a0) != 0 || m.r32(f + 0x19c) != 0) as u32
}

/// `cPS2MemCard::vf55` 0x23edb0(c, i): file `i`'s +0x1a4 set
pub fn file_1a4(m: &Mem, c: u32, i: u32) -> u32 {
    (m.r32(file(c, i) + 0x1a4) != 0) as u32
}

/// `cPS2MemCard::vf11` 0x23fda8(c): a pending info waited for (`sceMcSync(0, 0, &result)`: -1 marks the port
/// changed); then the port 0, port 0's words cleared and [`reset_state`]
pub fn close(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    if m.r32(c + 0x3c) != 0 {
        m.w32(RESULT, 0);
        e.call(m, MC_SYNC, &[0, 0, RESULT]);
        if m.r32(RESULT) == u32::MAX {
            m.w32(port(c, m.r32(c + 0x10)) + 0x184, 1);
        }
    }
    m.w32(c + 0x3c, 0);
    m.w32(c + 0x10, 0);
    for at in [0x174, 0x178, 0x17c, 0x180] {
        m.w32(c + at, 0);
    }
    reset_state(m, c);
}

/// `MemCard_IsErrorCode` 0x23fe38(_, r): -9999 ≤ r ≤ -10
pub fn is_error_code(r: u32) -> u32 {
    (r.wrapping_add(0x270f) < 0x2706) as u32
}

/// `cPS2MemCard::vf59` 0x23fe48(c, n, out): for n 0 or 1 the string 0xeb1 copied to `out`
pub fn vf59(m: &mut Mem, e: &mut dyn FeEnv, _c: u32, n: u32, out: u32) {
    if (n as i32) < 2 && (n as i32) >= 0 {
        let t = loc(m, e, 0xeb1);
        e.call(m, WSTR_COPY, &[out, t]);
    }
}

/// `cPS2MemCard::vf32` 0x23fea8(c, name, keep): unless `keep`, the name to +0x4c0
pub fn set_name(m: &mut Mem, c: u32, name: u32, keep: u32) {
    if keep == 0 {
        strcpy(m, c + 0x4c0, name);
    }
}

/// `cPS2MemCard::vf18` 0x23ff18(c, bytes): the card blocks a save of `bytes` takes: KB rounded up, + 0x83
pub fn blocks_for(bytes: u32) -> u32 {
    let n = bytes as i32;
    let v = n.wrapping_add(0x3ff);
    let a = if -1 < v { v } else { n.wrapping_add(0x7fe) };
    ((a >> 10) + 0x83) as u32
}

/// The virtual forwards on the current port (c, then +0x10): vf51 0x23fed8 → vf at +0x198, vf43 0x23ff70 → +0x158,
/// vf46 0x240010 → +0x180, vf45 0x240040 → +0x168, vf41 0x240070 → +0x148; vf19 0x23ff38 passes +0x24 to +0x98
pub fn forward(m: &mut Mem, e: &mut dyn FeEnv, c: u32, off: u32, field: u32) -> u32 {
    let v = m.r32(c + field);
    vcall(m, e, c, off, &[v])
}

/// The constant and field answers: vf48 0x23fec0 / vf49 0x23fec8 / vf50 0x23fed0 1, vf20 0x23ff08 0x4000, vf17
/// 0x23ff10 and vf21 0x23ffa0 -1, vf42 0x23ff68 0, vf58 0x23ffd0 2; vf57 0x23ffd8 the files' count, vf3 0x2400a0
/// the port, 0x23ffa8 the state; vf6 0x23db08, vf28 0x23de98, vf30 0x23dea0, vf38 0x23ed10 nothing
pub fn constant(m: &Mem, c: u32, func: u32) -> Option<u32> {
    Some(match func {
        0x23fec0 | 0x23fec8 | 0x23fed0 => 1,
        0x23ff08 => 0x4000,
        0x23ff10 | 0x23ffa0 => u32::MAX,
        0x23ff68 => 0,
        0x23ffd0 => 2,
        0x23ffd8 => m.r32(c + 0x14),
        0x2400a0 => m.r32(c + 0x10),
        0x23ffa8 => m.r32(c + 4),
        _ => return None,
    })
}

/// `MemCard_GetCurrentSlotError` 0x23ffb0(c), `cPS2MemCard::vf52` 0x23fff0(c): the current port's error, changed
pub fn current_error(m: &Mem, c: u32) -> u32 {
    slot_error(m, c, m.r32(c + 0x10))
}
pub fn current_changed(m: &Mem, c: u32) -> u32 {
    changed(m, c, m.r32(c + 0x10))
}

pub const MC_OPEN: u32 = 0x2f1428;
pub const MC_CLOSE: u32 = 0x2f1598;
pub const MC_MKDIR: u32 = 0x2f1560;
pub const MC_READ: u32 = 0x2f17b8;
pub const MC_WRITE: u32 = 0x2f18d8;
pub const MC_DELETE: u32 = 0x2f2180;
pub const ASYNC_LOAD: u32 = 0x2c5e20;
pub const ASCII_TO_SJIS: u32 = 0x23d628;
/// The icon file names (three pointers, 0x345f40), "icon.sys" (0x3a49f0), the "%s/%s" pattern without the
/// game's own (0x3a42b0), "PS2D" (0x3a42b8)
pub const ICONS: u32 = 0x345f40;
pub const ICON_SYS: u32 = 0x3a49f0;
pub const SUB_FMT: u32 = 0x3a42b0;
pub const PS2D: u32 = 0x3a42b8;
/// The icon.sys block written (0x3c4 bytes at 0x346010) and its parts' sources
pub const ICON_BLOCK: u32 = 0x346010;

fn memcpy(m: &mut Mem, dst: u32, src: u32, n: u32) {
    let b: Vec<u8> = (0..n).map(|k| m.r8(src + k)).collect();
    for (k, x) in b.into_iter().enumerate() {
        m.w8(dst + k as u32, x);
    }
}
fn icon(m: &Mem, state: u32, one: u32, two: u32) -> u32 {
    let k = if state == one { 1 } else if state == two { 2 } else { 0 };
    m.r32(ICONS + 4 * k)
}

/// `MemCard_Process` 0x23e0d0(c, state) (`sp` the stack pointer on entry): the state kept at +4, then its step:
/// the `sceMc*` call it starts (open, close, read, write, mkdir, delete, getdir), its result kept in [`RESULT`],
/// the file handle (+0x18) back to -1 once a call is under way, and on an error the state it falls to (1, or
/// 0x10 / 0xf while reading the directory). State 1 records the result as the port's, frees the buffer and
/// closes the file (state 2 when that worked); 7 looks for the first file the driver's vf +0x1c8 wants; 0x26
/// builds and writes the icon.sys block; 0x28 / 0x2c / 0x30 load an icon file. States out of the table and the
/// waiting ones (5, 0x11, 0x13, 0x14, 0x1a, 0x20..0x22) restart the card info unless it is pending; 2, 4, 6, 0x10,
/// 0x12, 0x15, 0x19, 0x1b, 0x1f, 0x23, 0x34, 0x35 only set the state.
pub fn process_state(m: &mut Mem, e: &mut dyn FeEnv, c: u32, state: u32, sp: u32) {
    let frame = sp - 0x90;
    m.w32(c + 4, state);
    let port_no = |m: &Mem| m.r32(c + 0x10);
    let fd = |m: &Mem| m.r32(c + 0x18);
    let name_i = |m: &Mem| file(c, m.r32(c + 0x28)) + 0x1a8;
    // the ends the cases share: the result kept; ≥ 0 done, else state 1 (`fail`)
    macro_rules! keep {
        ($r:expr) => {{
            let r: u32 = $r;
            m.w32(RESULT, r);
            r
        }};
    }
    macro_rules! or_fail {
        ($r:expr) => {{
            if ($r as i32) < 0 {
                process_state(m, e, c, 1, frame);
            }
        }};
    }
    // an open or close: either way the handle is -1 after; on an error state 1
    macro_rules! handle_call {
        ($r:expr) => {{
            let r = keep!($r);
            m.w32(c + 0x18, u32::MAX);
            or_fail!(r);
        }};
    }
    match state {
        1 => {
            let p = port(c, port_no(m));
            let res = m.r32(RESULT);
            m.w32(p + 0x17c, res);
            let buf = m.r32(c + 0x30);
            if buf != 0 {
                e.call(m, EA_FREE, &[buf]);
                m.w32(c + 0x30, 0);
            }
            if (fd(m) as i32) >= 0 {
                let r = keep!(e.call(m, MC_CLOSE, &[fd(m)]));
                if (r as i32) >= 0 {
                    process_state(m, e, c, 2, frame);
                }
                m.w32(c + 0x18, u32::MAX);
            }
            m.w32(c + 0x20, 0);
            m.w32(c + 0x1c, u32::MAX);
            m.w32(c + 0x2c, 0);
        }
        3 => start_get_info(m, e, c),
        7 => {
            m.w32(c + 0x28, 0);
            if (m.r32(c + 0x14) as i32) > 0 {
                loop {
                    let i = m.r32(c + 0x28);
                    if vcall(m, e, c, 0x1c8, &[i]) != 0 {
                        let n = name_i(m);
                        e.call(m, SPRINTF, &[c + 0x71, PATH_FMT, n, ICON_SYS]);
                        process_state(m, e, c, 8, frame);
                        return;
                    }
                    let i = m.r32(c + 0x28).wrapping_add(1);
                    m.w32(c + 0x28, i);
                    if (i as i32) >= (m.r32(c + 0x14) as i32) {
                        break;
                    }
                }
            }
        }
        8 => {
            let r = keep!(e.call(m, MC_OPEN, &[port_no(m), 0, c + 0x71, 1]));
            m.w32(c + 0x18, u32::MAX);
            if (r as i32) < 0 {
                let i = m.r32(c + 0x28);
                flag_file(m, c, i, 0x1a0);
                process_state(m, e, c, 0x10, frame);
            }
        }
        9 => {
            let r = keep!(e.call(m, MC_CLOSE, &[fd(m)]));
            if (r as i32) >= 0 {
                m.w32(c + 0x18, u32::MAX);
            } else {
                process_state(m, e, c, 0x10, frame);
            }
        }
        0xa => {
            let r = keep!(e.call(m, MC_READ, &[fd(m), ICON_BLOCK, 0x3c4]));
            if (r as i32) < 0 {
                let i = m.r32(c + 0x28);
                flag_file(m, c, i, 0x1a0);
                process_state(m, e, c, 0xf, frame);
            }
        }
        0xb | 0xc | 0xd | 0xe => {
            let n = name_i(m);
            let (dst, last) = if state == 0xb { (c + 0xf1, n) } else { (c + 0x71, icon(m, state, 0xd, 0xe)) };
            e.call(m, SPRINTF, &[dst, PATH_FMT, n, last]);
            memset(m, c + 0x380, 0x40);
            let r = keep!(e.call(m, MC_GET_DIR, &[port_no(m), 0, dst, 0, 1, c + 0x380]));
            if (r as i32) < 0 {
                process_state(m, e, c, 0x10, frame);
            }
        }
        0xf => {
            let r = keep!(e.call(m, MC_CLOSE, &[fd(m)]));
            m.w32(c + 0x18, u32::MAX);
            if (r as i32) < 0 {
                process_state(m, e, c, 0x10, frame);
            }
        }
        0x16 => handle_call!(e.call(m, MC_OPEN, &[port_no(m), 0, c + 0xf1, 0x203])),
        0x17 => {
            let r = keep!(e.call(m, MC_WRITE, &[fd(m), m.r32(c + 0x2c), m.r32(c + 0x20)]));
            m.w32(c + 0x2c, 0);
            m.w32(c + 0x20, 0);
            or_fail!(r);
        }
        0x18 | 0x1e => {
            if (fd(m) as i32) < 0 {
                process_state(m, e, c, state + 1, frame);
            } else {
                handle_call!(e.call(m, MC_CLOSE, &[fd(m)]));
            }
        }
        0x1c => {
            m.w32(c + 0x40, 0);
            let r = keep!(e.call(m, MC_OPEN, &[port_no(m), 0, c + 0xf1, 1]));
            m.w32(c + 0x18, u32::MAX);
            if r != 0 {
                process_state(m, e, c, 1, frame);
            }
        }
        0x1d => {
            let r = keep!(e.call(m, MC_READ, &[fd(m), m.r32(c + 0x2c), m.r32(c + 0x20)]));
            if r != 0 {
                process_state(m, e, c, 1, frame);
            }
        }
        0x24 => {
            m.w32(c + 0x44, 0);
            let r = keep!(e.call(m, MC_MKDIR, &[port_no(m), 0, c + 0xf1]));
            or_fail!(r);
        }
        0x25 => {
            e.call(m, SPRINTF, &[c + 0x71, SUB_FMT, c + 0xf1, ICON_SYS]);
            let r = keep!(e.call(m, MC_OPEN, &[port_no(m), 0, c + 0x71, 0x203]));
            m.w32(c + 0x18, u32::MAX);
            or_fail!(r);
        }
        0x26 => {
            memset(m, ICON_BLOCK, 0x3c4);
            let w = m.r32(PS2D);
            let b = m.r8(PS2D + 4);
            m.w32(ICON_BLOCK, w);
            m.w8(ICON_BLOCK + 4, b);
            e.call(m, ASCII_TO_SJIS, &[c, ICON_BLOCK + 0xc0, c + 0x50]);
            let h = if (m.r32(c + 0xc) as i32) > 0 { m.r16(c + 0xc) } else { (strlen(m, c + 0x50) << 1) as u16 };
            m.w16(ICON_BLOCK + 6, h);
            m.w32(ICON_BLOCK + 0xc, 0x60);
            memcpy(m, ICON_BLOCK + 0x10, 0x345f60, 0x40);
            memcpy(m, ICON_BLOCK + 0x50, 0x345fa0, 0x30);
            memcpy(m, ICON_BLOCK + 0x80, 0x345fd0, 0x30);
            memcpy(m, ICON_BLOCK + 0xb0, 0x346000, 0x10);
            for k in 0..3 {
                let s = m.r32(ICONS + 4 * k);
                strcpy(m, ICON_BLOCK + 0x104 + 0x40 * k, s);
            }
            let r = keep!(e.call(m, MC_WRITE, &[fd(m), ICON_BLOCK, 0x3c4]));
            or_fail!(r);
        }
        0x27 => handle_call!(e.call(m, MC_CLOSE, &[fd(m)])),
        0x28 | 0x2c | 0x30 => {
            let buf = m.r32(c + 0x30);
            if buf != 0 {
                e.call(m, EA_FREE, &[buf]);
                m.w32(c + 0x30, 0);
            }
            let ic = icon(m, m.r32(c + 4), 0x2c, 0x30);
            e.call(m, SPRINTF, &[frame, SUB_FMT, c + 0x4c0, ic]);
            let h = e.call(m, ASYNC_LOAD, &[frame, 0x10]);
            m.w32(c + 0x38, h);
        }
        0x29 | 0x2d | 0x31 => {
            let ic = icon(m, m.r32(c + 4), 0x2d, 0x31);
            e.call(m, SPRINTF, &[c + 0x71, PATH_FMT, c + 0xb1, ic]);
            handle_call!(e.call(m, MC_OPEN, &[port_no(m), 0, c + 0x71, 0x203]));
        }
        0x2a | 0x2e | 0x32 => {
            let r = keep!(e.call(m, MC_WRITE, &[fd(m), m.r32(c + 0x30), m.r32(c + 0x34)]));
            or_fail!(r);
        }
        0x2b | 0x2f | 0x33 => {
            let buf = m.r32(c + 0x30);
            if buf != 0 {
                e.call(m, EA_FREE, &[buf]);
                m.w32(c + 0x30, 0);
            }
            handle_call!(e.call(m, MC_CLOSE, &[fd(m)]));
        }
        0x36..=0x3b => {
            let path = match state {
                0x36 => {
                    m.w32(port(c, port_no(m)) + 0x17c, 0);
                    c + 0xf1
                }
                0x3b => c + 0xb1,
                _ => {
                    let last = if state == 0x3a { ICON_SYS } else { icon(m, m.r32(c + 4), 0x38, 0x39) };
                    e.call(m, SPRINTF, &[c + 0x71, PATH_FMT, c + 0xb1, last]);
                    c + 0x71
                }
            };
            let r = keep!(e.call(m, MC_DELETE, &[port_no(m), 0, path]));
            or_fail!(r);
        }
        2 | 4 | 6 | 0x10 | 0x12 | 0x15 | 0x19 | 0x1b | 0x1f | 0x23 | 0x34 | 0x35 => {}
        _ => {
            if m.r32(c + 0x3c) == 0 {
                start_get_info(m, e, c);
            }
        }
    }
}

/// The SJIS tables: per letter group (0x345e88: SJIS start, ASCII start), the punctuation's codes (0x345e98),
/// back: the 0x81 row's ASCII (0x345ee8 - 0x40) and per group (0x345ee0: SJIS start, ASCII start)
pub const SJIS_GROUPS: u32 = 0x345e88;
pub const SJIS_PUNCT: u32 = 0x345e98;
pub const ASCII_GROUPS: u32 = 0x345ee0;
pub const ASCII_81: u32 = 0x345ee8;

/// `Ascii_ToSjis` 0x23d770(c): the full-width SJIS code of an ASCII character (space and punctuation from the
/// table, digits and letters by group), its two bytes swapped as they are stored; 0 for anything else
pub fn ascii_to_sjis(m: &Mem, ch: u32) -> u32 {
    let c = ch & 0xff;
    let (punct, group) = if c.wrapping_sub(0x20) < 0x10 {
        (1, 0)
    } else if c.wrapping_sub(0x30) < 10 {
        (0, 0)
    } else if c.wrapping_sub(0x3a) < 7 {
        (0xb, 0)
    } else if c.wrapping_sub(0x41) < 26 {
        (0, 1)
    } else if c.wrapping_sub(0x5b) < 6 {
        (0x25, 0)
    } else if c.wrapping_sub(0x61) < 26 {
        (0, 2)
    } else if c.wrapping_sub(0x7b) < 4 {
        (0x3f, 0)
    } else {
        return 0;
    };
    let v = if punct != 0 {
        m.r16(SJIS_PUNCT + 2 * c.wrapping_sub(punct + 0x1f)) as u32
    } else {
        let t = SJIS_GROUPS + 4 * group;
        (m.r16(t) as u32).wrapping_add(c).wrapping_sub(m.r16(t + 2) as u32) & 0xffff
    };
    (((v << 8) | (v >> 8)) as u16 as i16) as i32 as u32
}

/// `Sjis_ToAscii` 0x23d868(h): a stored SJIS character (first byte low) back to ASCII; 0 when it is not in the
/// 0x81 or 0x82 rows
pub fn sjis_to_ascii(m: &Mem, h: u32) -> u32 {
    let hi = (h >> 8) & 0xff;
    let lo = h & 0xff;
    if lo == 0x81 {
        return m.r8(ASCII_81 + hi - 0x40) as u32;
    }
    if lo != 0x82 {
        return 0;
    }
    let k = if hi.wrapping_sub(0x4f) < 10 {
        0
    } else if hi.wrapping_sub(0x60) < 26 {
        1
    } else if (hi + 0x7f) & 0xff > 0x19 {
        0
    } else {
        2
    };
    let t = ASCII_GROUPS + 2 * k;
    (m.r8(t + 1) as u32).wrapping_add(hi.wrapping_sub(m.r8(t) as u32)) & 0xff
}

/// `MemCard_AsciiToSjis` 0x23d628(_, dst, src): each character of `src` as a halfword, then a 0 halfword
pub fn ascii_string_to_sjis(m: &mut Mem, dst: u32, src: u32) {
    let (mut d, mut s) = (dst, src);
    while m.r8(s) != 0 {
        let v = ascii_to_sjis(m, m.r8(s) as u32);
        s += 1;
        m.w16(d, v as u16);
        d += 2;
    }
    m.w16(d, 0);
}

/// `MemCard_SjisToAscii` 0x23d680(_, dst, src): each halfword of `src` (up to a 0) as a byte, then a 0
pub fn sjis_string_to_ascii(m: &mut Mem, dst: u32, src: u32) {
    let (mut d, mut s) = (dst, src);
    while m.r16(s) != 0 {
        let h = m.r16(s) as i16 as i32 as u32;
        s += 2;
        let b = sjis_to_ascii(m, h);
        m.w8(d, b as u8);
        d += 1;
    }
    m.w8(d, 0);
}

/// `MemCard_CheckRoomForFiles` 0x23fcb8(c): the card's free blocks (vf +0x88) and a save's (vf +0x98 with +0x24);
/// each file the driver's vf +0x1c8 does not have yet takes its blocks from the free ones, or is marked (+0x1a4)
/// when they are too few
pub fn check_room(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    let mut free = vcall(m, e, c, 0x88, &[]) as i32;
    let size = m.r32(c + 0x24);
    let need = vcall(m, e, c, 0x98, &[size]) as i32;
    let mut i = 0u32;
    if (m.r32(c + 0x14) as i32) > 0 {
        loop {
            let at = file(c, i) + 0x1a4;
            m.w32(at, 0);
            if vcall(m, e, c, 0x1c8, &[i]) == 0 {
                if free < need {
                    m.w32(at, 1);
                } else {
                    free = free.wrapping_sub(need);
                }
            }
            i += 1;
            if (i as i32) >= (m.r32(c + 0x14) as i32) {
                break;
            }
        }
    }
}

/// `cPS2MemCard::vf53` 0x23ffe0(c, out): the current port's line ([`slot_text`])
pub fn current_text(m: &mut Mem, e: &mut dyn FeEnv, c: u32, out: u32) {
    let p = m.r32(c + 0x10);
    slot_text(m, e, c, p, out);
}

pub const WSTR_SET_ASCII: u32 = 0x17ccb8;
/// The busy line's ASCII (0x3a4590)
pub const BUSY_TEXT: u32 = 0x3a4590;

/// `cPS2MemCard::vf13` 0x23fb60(c, i, out) (`sp` the stack pointer on entry): file `i`'s line into `out` (cut at
/// 32 characters): busy (states 5..0x10) the text at 0x3a4590; no card (vf +0x170) or none usable (vf +0x178) the
/// strings 0x15f / 0x1cf; a flagged file 0x148, one without room 0x18b; a file the card lacks (vf +0x1c8) 0x15f;
/// else the file's title (+0x1e8)
pub fn file_line(m: &mut Mem, e: &mut dyn FeEnv, c: u32, i: u32, out: u32, sp: u32) {
    let buf = sp - 0xd0;
    let f = file(c, i);
    let id = if m.r32(c + 4).wrapping_sub(5) < 0xc {
        e.call(m, WSTR_SET_ASCII, &[buf, BUSY_TEXT]);
        None
    } else if vcall(m, e, c, 0x170, &[]) == 0 {
        Some(0x15f)
    } else if vcall(m, e, c, 0x178, &[]) == 0 {
        Some(0x1cf)
    } else if m.r32(f + 0x19c) != 0 || m.r32(f + 0x1a0) != 0 {
        Some(0x148)
    } else if m.r32(f + 0x1a4) != 0 {
        Some(0x18b)
    } else if vcall(m, e, c, 0x1c8, &[i]) == 0 {
        Some(0x15f)
    } else {
        e.call(m, WSTR_SET_ASCII, &[buf, f + 0x1e8]);
        None
    };
    if let Some(id) = id {
        let t = loc(m, e, id);
        e.call(m, WSTR_COPY, &[buf, t]);
    }
    m.w16(buf + 0x40, 0);
    e.call(m, WSTR_COPY, &[out, buf]);
}

pub const ASYNC_IS_DONE: u32 = 0x2c6108;
pub const ASYNC_FINISH: u32 = 0x2c6160;
/// The words a save's directory entry (+0x390) is checked against, one per icon (0x3a49e0)
pub const ICON_WORDS: u32 = 0x3a49e0;

/// `sceMcSync(1, 0, RESULT)` with the result word cleared first; 0 while the card is still busy
fn sync(m: &mut Mem, e: &mut dyn FeEnv) -> u32 {
    m.w32(RESULT, 0);
    e.call(m, MC_SYNC, &[1, 0, RESULT])
}

/// `cPS2MemCard::vf2` 0x23edc8(c): the driver's per-frame step. An unformatted current port forgets its files;
/// a pending `sceMcGetInfo` (+0x3c) is waited on and its words kept in the port's (an unlisted error is -10, a
/// card not of type 2 -10002, unformatted -2); the states that started a card call wait for it (on an error:
/// -4 is no error for a make-directory or a delete, a file being read in states 8..0xf is flagged and skipped
/// (state 0x10), else state 1); a pending end of save (+0x40) or load (+0x44) is handed on; then the state's
/// own step, and when it leaves the state as it was the port's error clears its status (-10 or -1) or the
/// driver (-10 or -2) and an active driver goes back to state 0
pub fn update(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    let pn = |m: &Mem| m.r32(c + 0x10);
    if m.r32(port(c, pn(m)) + 0x180) == 0 {
        m.w32(c + 0x4c, 0);
        memset(m, c + 0x19c, 0x1c0);
    }
    if m.r32(c + 0x3c) != 0 {
        if sync(m, e) == 0 {
            return;
        }
        let r = m.r32(RESULT);
        if r == u32::MAX {
            m.w32(port(c, pn(m)) + 0x184, 1);
        }
        if r.wrapping_add(0x13) < 9
            || r.wrapping_add(0x31) < 10
            || r.wrapping_add(0x3b) < 10
            || r.wrapping_add(0x4f) < 10
        {
            m.w32(RESULT, -10i32 as u32);
        }
        if is_error_code(m.r32(RESULT)) == 0 {
            if m.r32(INFO_TYPE) != 2 {
                m.w32(RESULT, -10002i32 as u32);
            } else if m.r32(INFO_FORMAT) == 0 {
                m.w32(RESULT, -2i32 as u32);
            }
        }
        let (res, ty, fmt, free) = (m.r32(RESULT), m.r32(INFO_TYPE), m.r32(INFO_FORMAT), m.r32(INFO_FREE));
        m.w32(port(c, pn(m)) + 0x17c, res);
        m.w32(c + 0x3c, 0);
        m.w32(port(c, pn(m)) + 0x174, ty);
        m.w32(port(c, pn(m)) + 0x178, free);
        m.w32(port(c, pn(m)) + 0x180, fmt);
    }
    let st = m.r32(c + 4);
    let waits = matches!(st, 2 | 6 | 8..=0xf | 0x12 | 0x16..=0x18 | 0x1c..=0x1e | 0x23..=0x27 | 0x29..=0x2b
        | 0x2d..=0x2f | 0x31..=0x33 | 0x36..=0x3b);
    if waits {
        if sync(m, e) == 0 {
            return;
        }
        let r = m.r32(RESULT);
        if r == u32::MAX {
            m.w32(port(c, pn(m)) + 0x184, 1);
        }
        if (r as i32) < 0 {
            let st = m.r32(c + 4);
            if st == 0x24 || st.wrapping_sub(0x36) < 6 {
                if r == -4i32 as u32 {
                    m.w32(RESULT, 0);
                }
            } else if st.wrapping_sub(8) < 8 && r != u32::MAX && is_error_code(r) == 0 {
                let i = m.r32(c + 0x28);
                flag_file(m, c, i, 0x19c);
                process(m, e, c, 0x10);
                return;
            }
            if (m.r32(RESULT) as i32) < 0 {
                let mut st = m.r32(c + 4);
                if st == 0x23 || st == 7 {
                    clear_name(m, c);
                    st = m.r32(c + 4);
                }
                if st == 6 {
                    reset_state(m, c);
                    st = m.r32(c + 4);
                }
                if st == 0x13 {
                    m.w32(c + 0x1c, u32::MAX);
                }
                process(m, e, c, 1);
                return;
            }
        }
        m.w32(port(c, pn(m)) + 0x17c, 0);
    }
    if m.r32(c + 0x40) != 0 {
        m.w32(c + 0x40, 0);
        process(m, e, c, 0x1e);
        return;
    }
    if m.r32(c + 0x44) != 0 {
        m.w32(c + 0x44, 0);
        process(m, e, c, 0x18);
        return;
    }
    let st = m.r32(c + 4);
    // the vf at +0x1e8 answers 0 when the driver may go on
    let ready = |m: &mut Mem, e: &mut dyn FeEnv| vcall(m, e, c, 0x1e8, &[]) == 0;
    // the files from +0x28 on: the next the card has (vf +0x1c8) is opened (its icon.sys), else the room checked
    let next_file = |m: &mut Mem, e: &mut dyn FeEnv| {
        loop {
            let i = m.r32(c + 0x28).wrapping_add(1);
            m.w32(c + 0x28, i);
            if (i as i32) >= (m.r32(c + 0x14) as i32) {
                break;
            }
            let i = m.r32(c + 0x28);
            if vcall(m, e, c, 0x1c8, &[i]) != 0 {
                let n = file(c, m.r32(c + 0x28)) + 0x1a8;
                e.call(m, SPRINTF, &[c + 0x71, PATH_FMT, n, ICON_SYS]);
                process(m, e, c, 8);
                return false;
            }
        }
        check_room(m, e, c);
        true
    };
    match st {
        2 => {
            if sync(m, e) == 0 {
                return;
            }
            if m.r32(RESULT) == u32::MAX {
                m.w32(port(c, pn(m)) + 0x184, 1);
            }
        }
        3 => process(m, e, c, 4),
        5 => {
            if ready(m, e) {
                read_dir(m, e, c);
                return;
            }
        }
        6 => {
            let (n, r) = (m.r32(c + 0x14), m.r32(RESULT));
            m.w32(c + 0x4c, r);
            if (n as i32) < (r as i32) {
                m.w32(c + 0x4c, n);
            }
            memset(m, c + 0x19c, 0x1c0);
            let mut i = 0u32;
            let mut count = m.r32(c + 0x14);
            if (count as i32) > 0 {
                loop {
                    let want = (i + 0x31) & 0xff;
                    if (m.r32(c + 0x4c) as i32) > 0 {
                        let mut j = 0u32;
                        loop {
                            let name = c + j * 0x40 + 0x3e0;
                            let l = strlen(m, name);
                            if l != 0 && m.r8(c + 0x3e0 + j * 0x40 + l - 1) as u32 == want {
                                strcpy(m, file(c, i) + 0x1a8, name);
                                break;
                            }
                            j += 1;
                            if (j as i32) >= (m.r32(c + 0x4c) as i32) {
                                break;
                            }
                        }
                        count = m.r32(c + 0x14);
                    }
                    i += 1;
                    if (i as i32) >= (count as i32) {
                        break;
                    }
                }
            }
            m.w32(RESULT, 0);
            process(m, e, c, 7);
            return;
        }
        7 => {
            check_room(m, e, c);
        }
        8 => {
            let r = m.r32(RESULT);
            m.w32(c + 0x18, r);
            process(m, e, c, 0xa);
            return;
        }
        9 => {
            process(m, e, c, 0xb);
            return;
        }
        0xa => {
            let t = file(c, m.r32(c + 0x28)) + 0x1e8;
            sjis_string_to_ascii(m, t, ICON_BLOCK + 0xc0);
            process(m, e, c, 9);
            return;
        }
        0xb => {
            if m.r32(c + 0x390) != m.r32(c + 0x24) {
                let i = m.r32(c + 0x28);
                flag_file(m, c, i, 0x19c);
                process(m, e, c, 0x10);
            }
            process(m, e, c, 0xc);
            return;
        }
        0xc | 0xd | 0xe => {
            let k = match st {
                0xd => 1,
                0xe => 2,
                _ => 0,
            };
            if m.r32(c + 0x390) != m.r32(ICON_WORDS + 4 * k) {
                let i = m.r32(c + 0x28);
                flag_file(m, c, i, 0x1a0);
                process(m, e, c, 0x10);
                return;
            }
            if !next_file(m, e) {
                return;
            }
        }
        0xf | 0x10 => {
            if !next_file(m, e) {
                return;
            }
        }
        0x11 => {
            if m.r32(port(c, pn(m)) + 0x180) == 0 {
                format(m, e, c);
                return;
            }
        }
        0x13 => {
            if ready(m, e) {
                build_save_path(m, e, c);
                return;
            }
        }
        0x14 | 0x15 | 0x1b => {
            if ready(m, e) {
                let next = match st {
                    0x14 => 0x24,
                    0x15 => 0x16,
                    _ => 0x1c,
                };
                process(m, e, c, next);
                return;
            }
        }
        0x16 | 0x1c | 0x25 | 0x29 | 0x2d | 0x31 => {
            let r = m.r32(RESULT);
            m.w32(c + 0x18, r);
            process(m, e, c, st + 1);
            return;
        }
        0x17 => {
            process(m, e, c, 0x34);
            return;
        }
        0x19 => {
            let f = file(c, m.r32(c + 0x1c));
            m.w32(f + 0x1a0, 0);
            m.w32(f + 0x19c, 0);
            m.w32(f + 0x1a4, 0);
            strcpy(m, f + 0x1a8, c + 0xb1);
            let f = file(c, m.r32(c + 0x1c));
            strcpy(m, f + 0x1e8, c + 0x50);
        }
        0x1d => {
            if m.r32(RESULT) != m.r32(c + 0x20) {
                m.w32(RESULT, -10001i32 as u32);
                process(m, e, c, 1);
            } else {
                process(m, e, c, 0x35);
            }
            return;
        }
        0x22 => {
            if ready(m, e) {
                stat_picked(m, e, c);
                return;
            }
        }
        0x23 => {
            if m.r32(RESULT) != 1 {
                clear_name(m, c);
            }
            m.w32(RESULT, 0);
        }
        0x24 | 0x26 | 0x27 | 0x2a | 0x2e | 0x32 | 0x36 | 0x39 | 0x3a => {
            process(m, e, c, st + 1);
            return;
        }
        0x28 | 0x2c | 0x30 => {
            if e.call(m, ASYNC_IS_DONE, &[m.r32(c + 0x38)]) == 1 {
                m.w32(c + 0x30, 0);
                m.w32(c + 0x34, 0);
                e.call(m, ASYNC_FINISH, &[m.r32(c + 0x38), c + 0x30, c + 0x34]);
                let s = m.r32(c + 4);
                process(m, e, c, s + 1);
            }
            return;
        }
        0x34 | 0x35 => return,
        0x37 | 0x38 => {
            process(m, e, c, 0x3a);
            return;
        }
        _ => {}
    }
    let none = -10i32 as u32;
    if slot_error(m, c, pn(m)) == none || slot_error(m, c, pn(m)) == u32::MAX {
        clear_current_status(m, c);
    }
    if slot_error(m, c, pn(m)) == none || slot_error(m, c, pn(m)) == -2i32 as u32 {
        reset_state(m, c);
    }
    if m.r32(c + 4) != 0 {
        process(m, e, c, 0);
    }
}

pub const STRNCPY: u32 = 0x2fd630;
/// The course being played (0x32de88) and the course table (0x332f28, 0x1c a course, +0x10 its code)
pub const GAME_COURSE: u32 = 0x32de88;
pub const COURSES: u32 = 0x332f28;

/// `strncpy` (the library's, 0x2fd630): up to `n` bytes, the rest of them 0 after the end
fn strncpy(m: &mut Mem, dst: u32, src: u32, n: u32) {
    let mut k = 0;
    while k < n {
        let b = m.r8(src + k);
        m.w8(dst + k, b);
        k += 1;
        if b == 0 {
            break;
        }
    }
    while k < n {
        m.w8(dst + k, 0);
        k += 1;
    }
}

/// `MemCard_FormatSaveTitle` 0x1ce8f8(o, n) (`sp` the stack pointer on entry): a save's title on the stack: with
/// `o`'s +0 clear the string 0x1cc formatted with `n + 1` and the course's code (its first 4 letters), else the
/// string 0x1cb; handed to the card (`o`+0x224, its vf at +0x130) and copied to `o`+0xd8
pub fn format_save_title(m: &mut Mem, e: &mut dyn FeEnv, o: u32, n: u32, sp: u32) {
    let buf = sp - 0x260;
    if m.r32(o) == 0 {
        let code = buf + 0x200;
        let course = m.r32(GAME_COURSE);
        let src = m.r32(course.wrapping_mul(0x1c).wrapping_add(COURSES) + 0x10);
        strncpy(m, code, src, 4);
        m.w8(buf + 0x204, 0);
        let t = loc(m, e, 0x1cc);
        e.call(m, WSTR_COPY, &[buf + 0x100, t]);
        e.call(m, WSTR_FORMAT, &[buf, buf + 0x100, n.wrapping_add(1), code]);
    } else {
        let t = loc(m, e, 0x1cb);
        e.call(m, WSTR_COPY, &[buf, t]);
    }
    let card = m.r32(o + 0x224);
    vcall(m, e, card, 0x130, &[buf]);
    e.call(m, WSTR_COPY, &[o + 0xd8, buf]);
}
