//! The race object's (`cAI`) object glue (board row G4k) on the game's bytes: its reset (vf10 0x1108f0), the riders
//! made (vf11 0x110bf8), torn down (vf13 0x110fa0) and set out at the start of a race (vf16 0x111560). Allocation,
//! the career's race roster and virtual calls: every call out goes through [`FeEnv`]. Checked in the function runner
//! (`tests/aiobj.rs`).
//!
//! The race object: +8 its table (the race's own virtuals), +0x1c / +0x20 / +0x60 / +0x68 words, +0x64 an object
//! (its table at +0), +0x6c..+0x78 the four inputs (`cInput`, 0x274 bytes), +0x7c the players, +0x80 the computer
//! riders, +0x88 the riders, +0x8c.. the players' controllers, +0x9c.. the computers', +0xb4.., +0xc4.. each rider's
//! boarder (in roster order), +0xdc / +0xe0 how the race was entered, +0xfc / +0x110 the two async archives, +0x124
//! their loader, +0x4f8..+0x500 words.

use crate::fe3dstate::{FeEnv, ALLOC, FILL, GAPP};
use crate::fe_characcept::CAREER;
use crate::pf::Mem;

/// The career's race roster: the count (+0x27dc) and the entries (0x84 bytes from +0x27e0; +0x65 a signed byte,
/// negative for a computer rider)
pub const ROSTER_COUNT: u32 = CAREER + 0x27dc;
pub const ROSTER: u32 = CAREER + 0x27e0;
pub const ENTRY_SIZE: u32 = 0x84;
/// The career's effects seed (+0x27dc + 0x324, 0x32de90)
pub const FX_SEED: u32 = CAREER + 0x27dc + 0x324;
/// The world lists (0x337c60) and the two words before them vf10 sets (0x337c68, 0x337c6c)
pub const LISTS: u32 = 0x337c60;
/// `cInput`'s table (0x385750)
pub const INPUT_VT: u32 = 0x385750;
/// The names: an input's ("0x364488"), a computer's controller, a player's, the two archives' paths
pub const INPUT_NAME: u32 = 0x364488;
pub const COMPUTER_NAME: u32 = 0x364498;
pub const PLAYER_NAME: u32 = 0x3644a8;
pub const ARCHIVE_PATHS: [u32; 2] = [0x3644b0, 0x3644c8];
/// The calls: `cComputer::cComputer` 0x138210, `Race_CreateBoarders` 0x150e38 (a player's controller),
/// `Boarder_InitSkinMem` 0x116a88, `Boarder_SetRiderInfo` 0x116a68, `Input_GetDevice` 0x17a830,
/// `Input_InitDefaults` 0x179238, `Pad_SetupVibration` 0x152770, `cAsyncBigFile` vf1 0x23be90 (open) and vf2
/// 0x23bea0 (close), `AsyncBdrLoad_SetManagers` 0x201c10, `Mem_Delete` 0x114f80, `AnimData_Reset` 0x159070 on
/// 0x3211a8, `BxList_DeleteAll` 0x17da50, `BxList_First` 0x17d860, `BxList_Unlink` 0x17d6e0, `BxList_Add` 0x17d4e8,
/// `Rand_Fx` 0x23d4a8
pub const COMPUTER_CTOR: u32 = 0x138210;
pub const PLAYER_CTOR: u32 = 0x150e38;
pub const SKIN_MEM: u32 = 0x116a88;
pub const RIDER_INFO: u32 = 0x116a68;
pub const GET_DEVICE: u32 = 0x17a830;
pub const INPUT_DEFAULTS: u32 = 0x179238;
pub const VIBRATION: u32 = 0x152770;
pub const ARCHIVE_OPEN: u32 = 0x23be90;
pub const ARCHIVE_CLOSE: u32 = 0x23bea0;
pub const LOADER_MANAGERS: u32 = 0x201c10;
pub const DELETE: u32 = 0x114f80;
pub const ANIM_RESET: u32 = 0x159070;
pub const ANIM_DATA: u32 = 0x3211a8;
pub const LIST_DELETE_ALL: u32 = 0x17da50;
pub const LIST_FIRST: u32 = 0x17d860;
pub const LIST_UNLINK: u32 = 0x17d6e0;
pub const LIST_ADD: u32 = 0x17d4e8;
pub const RAND_FX: u32 = 0x23d4a8;

/// A virtual of an object whose table is at `vt_at`: the adjust halfword at `off`, the function at `off` + 4
fn vcall_at(m: &mut Mem, e: &mut dyn FeEnv, obj: u32, vt_at: u32, off: u32, args: &[u32]) -> u32 {
    let vt = m.r32(obj.wrapping_add(vt_at));
    let adj = m.r16(vt.wrapping_add(off)) as i16 as u32;
    let f = m.r32(vt.wrapping_add(off + 4));
    let mut a = vec![obj.wrapping_add(adj)];
    a.extend_from_slice(args);
    e.call(m, f, &a)
}

fn entry(k: u32) -> u32 {
    ROSTER + ENTRY_SIZE * k
}

fn is_computer(m: &Mem, k: u32) -> bool {
    (m.r8(entry(k) + 0x65) as i8) < 0
}

fn count(m: &Mem) -> i32 {
    m.r32(ROSTER_COUNT) as i32
}

/// The boarder a controller holds (its +0), or 0 without one
fn boarder_of(m: &Mem, ctrl: u32) -> u32 {
    if ctrl != 0 {
        m.r32(ctrl)
    } else {
        0
    }
}

/// `cAI` vf10 0x1108f0(ai), the reset: the two words before the world lists 1, +0x500 and +0x4fc -1, +0x1c, +0x20,
/// +0x60, +0x64, +0x68, +0xdc, +0xe0, +0x4f8 and the four inputs (+0x6c..+0x78) cleared, then its table's +0x60
pub fn reset(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    m.w32(LISTS + 8, 1);
    m.w32(LISTS + 0xc, 1);
    m.w32(ai + 0x500, 0xffff_ffff);
    for at in [0x20u32, 0x60, 0x1c, 0x64, 0x68, 0xdc, 0xe0, 0x4f8] {
        m.w32(ai + at, 0);
    }
    m.w32(ai + 0x4fc, 0xffff_ffff);
    for k in (0..4u32).rev() {
        m.w32(ai + 0x6c + 4 * k, 0);
    }
    vcall_at(m, e, ai, 8, 0x60, &[]);
}

/// `cAI` vf11 0x110bf8(ai), the riders made: the counts cleared; the four inputs (0x274 bytes, `cInput`'s table at
/// +0x270); for each rider of the career's roster a controller (zeroed): a computer's (`cComputer`, 0x63b0 bytes,
/// +0x9c..) or a player's (0x6380 bytes, +0x8c.., its input first given the app's device's defaults, and after its
/// pad's vibration bound), its boarder's skin memory and rider info (the rider's number and roster entry), the
/// boarder kept at +0xc4 + 4 × the rider's number. Then the two async archives opened (+0x110 and +0xfc on the two
/// paths, 0x10) and their loader (+0x124) given them
pub fn make_riders(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    for at in [0x88u32, 0x84, 0x80, 0x7c] {
        m.w32(ai + at, 0);
    }
    for k in 0..4u32 {
        let o = alloc(m, e, INPUT_NAME, 0x274, 0xdead_c0de);
        m.w32(ai + 0x6c + 4 * k, o);
        m.w32(o + 0x270, INPUT_VT);
    }
    let mut k = 0u32;
    while (k as i32) < count(m) {
        let ent = entry(k);
        let (list, n_at) = if is_computer(m, k) {
            let list = ai + 0x9c;
            let slot = list + 4 * m.r32(ai + 0x80);
            let o = alloc(m, e, COMPUTER_NAME, 0x63b0, 0);
            let c = e.call(m, COMPUTER_CTOR, &[o, 1, 8]);
            m.w32(slot, c);
            (list, 0x80)
        } else {
            let list = ai + 0x8c;
            let n = m.r32(ai + 0x7c);
            let input = m.r32(ai + 0x6c + 4 * n);
            let dev = e.call(m, GET_DEVICE, &[m.r32(m.r32(GAPP) + 0x24)]);
            e.call(m, INPUT_DEFAULTS, &[input, dev]);
            let slot = list + 4 * m.r32(ai + 0x7c);
            let o = alloc(m, e, PLAYER_NAME, 0x6380, 0);
            let c = e.call(m, PLAYER_CTOR, &[o, 1, 8]);
            m.w32(slot, c);
            (list, 0x7c)
        };
        let ctrl = |m: &Mem| m.r32(list + 4 * m.r32(ai + n_at));
        let b = m.r32(ctrl(m));
        e.call(m, SKIN_MEM, &[b, 0]);
        let b = m.r32(ctrl(m));
        let n = m.r32(ai + 0x88);
        e.call(m, RIDER_INFO, &[b, n, ent]);
        if n_at == 0x7c {
            let p = m.r32(ai + 0x7c);
            let (c, input) = (m.r32(list + 4 * p), m.r32(ai + 0x6c + 4 * p));
            e.call(m, VIBRATION, &[c, input]);
        }
        let n = m.r32(ai + 0x88);
        m.w32(ai + 0x88, n + 1);
        let c = ctrl(m);
        m.w32(ai + n_at, m.r32(ai + n_at) + 1);
        let b = boarder_of(m, c);
        m.w32(ai + 0xc4 + 4 * n, b);
        k += 1;
    }
    e.call(m, ARCHIVE_OPEN, &[ai + 0x110, ARCHIVE_PATHS[0], 0x10]);
    e.call(m, ARCHIVE_OPEN, &[ai + 0xfc, ARCHIVE_PATHS[1], 0x10]);
    e.call(m, LOADER_MANAGERS, &[ai + 0x124, ai + 0xfc, 0, ai + 0x110]);
}

/// An allocation filled with `fill` when it came
fn alloc(m: &mut Mem, e: &mut dyn FeEnv, tag: u32, size: u32, fill: u32) -> u32 {
    let at = e.call(m, ALLOC, &[tag, size, 0]);
    if at != 0 {
        e.call(m, FILL, &[at, fill, size]);
    }
    at
}

/// `cAI` vf13 0x110fa0(ai), the tear-down: the object at +0x64 told (its table's +0x10, with the race object);
/// each rider's boarder destroyed (its table's +8 with 3) and the four lists' places cleared; the four inputs deleted
/// and cleared; the two archives closed (+0x110, +0xfc) and the animations reset
pub fn tear_down(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    let o = m.r32(ai + 0x64);
    if o != 0 {
        vcall_at(m, e, o, 0, 0x10, &[ai]);
    }
    let mut k = 0u32;
    while (k as i32) < m.r32(ai + 0x88) as i32 {
        let b = m.r32(ai + 0xc4 + 4 * k);
        if b != 0 {
            vcall_at(m, e, b, 0, 8, &[3]);
        }
        for at in [0x8cu32, 0x9c, 0xb4, 0xc4] {
            m.w32(ai + at + 4 * k, 0);
        }
        k += 1;
    }
    for k in 0..4u32 {
        let at = ai + 0x6c + 4 * k;
        let o = m.r32(at);
        if o != 0 {
            e.call(m, DELETE, &[o]);
        }
        m.w32(at, 0);
    }
    e.call(m, ARCHIVE_CLOSE, &[ai + 0x110]);
    e.call(m, ARCHIVE_CLOSE, &[ai + 0xfc]);
    e.call(m, ANIM_RESET, &[ANIM_DATA]);
}

/// `cAI` vf16 0x111560(ai), the start of a race: the world lists' kinds 3, 0xd and 4 emptied; the career's effects
/// seed from `Rand_Fx`; entered fresh (+0xe0 1, +0xdc 0); every node of kind 8 unlinked; each rider's controller in
/// roster order (computers' and players' counted apart) given its rider info again (the rider's number and roster
/// entry: the game leaves the entry in a2), its boarder kept at +0xc4.., and the boarder's +0x5ae0 object (at its
/// +0x82c, or none) added to the lists as kind 8; then its table's +0x78 with 1, 1
pub fn start_race(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    for kind in [3u32, 0xd, 4] {
        e.call(m, LIST_DELETE_ALL, &[LISTS, kind]);
    }
    let seed = e.call(m, RAND_FX, &[]);
    m.w32(FX_SEED, seed);
    m.w32(ai + 0xe0, 1);
    m.w32(ai + 0xdc, 0);
    loop {
        let n = e.call(m, LIST_FIRST, &[LISTS, 8]);
        if n == 0 {
            break;
        }
        e.call(m, LIST_UNLINK, &[LISTS, n]);
    }
    let (mut computers, mut players) = (0u32, 0u32);
    let mut k = 0u32;
    while (k as i32) < count(m) {
        let ent = entry(k);
        let at = if is_computer(m, k) {
            computers += 1;
            ai + 0x9c + 4 * (computers - 1)
        } else {
            players += 1;
            ai + 0x8c + 4 * (players - 1)
        };
        let b = m.r32(m.r32(at));
        e.call(m, RIDER_INFO, &[b, k, ent]);
        let c = m.r32(at);
        m.w32(ai + 0xc4 + 4 * k, boarder_of(m, c));
        let b = m.r32(ai + 0xc4 + 4 * k);
        let o = m.r32(b + 0x5ae0);
        let node = if o != 0 { o + 0x82c } else { 0 };
        e.call(m, LIST_ADD, &[LISTS, node, 8]);
        k += 1;
    }
    vcall_at(m, e, ai, 8, 0x78, &[1, 1]);
}
