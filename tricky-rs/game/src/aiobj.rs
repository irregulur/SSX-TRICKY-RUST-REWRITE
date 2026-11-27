//! The race object's (`cAI`) object glue (board row G4k) on the game's bytes: its reset (vf10 0x1108f0), the riders
//! made (vf11 0x110bf8), torn down (vf13 0x110fa0) and set out at the start of a race (vf16 0x111560). Allocation,
//! the career's race roster and virtual calls: every call out goes through [`FeEnv`]. Checked in the function runner
//! (`tests/aiobj.rs`). Row G4k2 adds its constructor 0x114c00, destructor 0x114d30 and the node's default virtuals,
//! G4k3 the race reset (vf14 0x111108) and the restart (vf15 0x1114d0), G4k5 the mode machine (vf1 0x1126f0, vf9
//! 0x1125a0, `Race_CreateHandlers` 0x1125c8).
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

/// The race object's table (0x364738) and its handlers' (each handler one word, its table): +0x24, +0x2c, +0x30,
/// +0x34, +0x38, +0x48, +0x4c, +0x50, +0x54, +0x58, +0x5c, in the constructor's order
pub const AI_VT: u32 = 0x364738;
pub const HANDLER_VTS: [(u32, u32); 11] = [
    (0x24, 0x364860),
    (0x2c, 0x364820),
    (0x30, 0x364840),
    (0x34, 0x364800),
    (0x38, 0x3647e0),
    (0x48, 0x36e780),
    (0x4c, 0x36e760),
    (0x50, 0x36e740),
    (0x54, 0x364e48),
    (0x58, 0x36e720),
    (0x5c, 0x364e28),
];
/// `cBxObjNode::cBxObjNode` 0x17cf50, its destructor 0x17cfe0; `cAsyncBigFile`'s constructor 0x23bd10 and destructor
/// 0x23bd90; the bdr loader's 0x201b50 / 0x201b90
pub const NODE_CTOR: u32 = 0x17cf50;
pub const NODE_DTOR: u32 = 0x17cfe0;
pub const ARCHIVE_CTOR: u32 = 0x23bd10;
pub const ARCHIVE_DTOR: u32 = 0x23bd90;
pub const LOADER_CTOR: u32 = 0x201b50;
pub const LOADER_DTOR: u32 = 0x201b90;

/// `cAI::cAI` 0x114c00(ai, parent): the node made (with `parent`), the race's table at +8 and the handlers' tables,
/// the two async archives (+0xfc, +0x110) and their loader (+0x124) made; answers `ai`
pub fn construct(m: &mut Mem, e: &mut dyn FeEnv, ai: u32, parent: u32) -> u32 {
    e.call(m, NODE_CTOR, &[ai, parent]);
    m.w32(ai + 8, AI_VT);
    for (at, vt) in HANDLER_VTS {
        m.w32(ai + at, vt);
    }
    e.call(m, ARCHIVE_CTOR, &[ai + 0xfc]);
    e.call(m, ARCHIVE_CTOR, &[ai + 0x110]);
    e.call(m, LOADER_CTOR, &[ai + 0x124]);
    ai
}

/// `cAI::~cAI` 0x114d30(ai, how): the race's table back at +8, the loader, the archive at +0x110 then +0xfc destroyed
/// (each with 2: not freed), then the node's destructor with `how`
pub fn destruct(m: &mut Mem, e: &mut dyn FeEnv, ai: u32, how: u32) {
    m.w32(ai + 8, AI_VT);
    e.call(m, LOADER_DTOR, &[ai + 0x124, 2]);
    e.call(m, ARCHIVE_DTOR, &[ai + 0x110, 2]);
    e.call(m, ARCHIVE_DTOR, &[ai + 0xfc, 2]);
    e.call(m, NODE_DTOR, &[ai, how]);
}

/// The node's default virtuals the race object keeps: vf3..vf6 (0x17def8, 0x17def0, 0x17dee8, 0x17dee0) answer 0,
/// vf2 0x17df00 does nothing
pub fn default_virtual(_slot: u32) -> u32 {
    0
}

/// The calls of the race reset: `Overlay_HideAll` 0x1a7758, `Boarder_ResetSkin` 0x116a18, `BdrLoad_LoadRider` 0x2010c8,
/// `BdrLoad_DefaultRequest` 0x201a98, `BdrLoad_RequestRider` 0x201790, `Rand_SeedGame` 0x23d4c8,
/// `AIPathSet_GetMainPath` 0x197ef0 / `_GetPath` 0x197ef8 on the course's paths 0x337fb8, `Boarder_SetRestartPoint`
/// 0x118068, `Boarder_GetCamera` 0x11ca88, `Camera_SetFocusRider` 0x173c50, `Game_IsTutorial` 0x1148d0,
/// `EAMem_ValidateAll` 0x2cdfd8, `AnimData_OpenBig` 0x158ea0, `RaceScene_SetupFromPicks` 0x169698 and
/// `RaceScene_GetAnimList` 0x169948 on the race scene 0x32edc0, `Race_ResetRiderRenders` 0x1128b8
pub const HIDE_OVERLAYS: u32 = 0x1a7758;
pub const RESET_SKIN: u32 = 0x116a18;
pub const LOAD_RIDER: u32 = 0x2010c8;
pub const DEFAULT_REQUEST: u32 = 0x201a98;
pub const REQUEST_RIDER: u32 = 0x201790;
pub const SEED_GAME: u32 = 0x23d4c8;
pub const AI_PATHS: u32 = 0x337fb8;
pub const MAIN_PATH: u32 = 0x197ef0;
pub const GET_PATH: u32 = 0x197ef8;
pub const RESTART_POINT: u32 = 0x118068;
pub const GET_CAMERA: u32 = 0x11ca88;
pub const FOCUS_RIDER: u32 = 0x173c50;
pub const IS_TUTORIAL: u32 = 0x1148d0;
pub const VALIDATE_ALL: u32 = 0x2cdfd8;
pub const OPEN_BIG: u32 = 0x158ea0;
pub const SCENE: u32 = 0x32edc0;
pub const SCENE_SETUP: u32 = 0x169698;
pub const SCENE_ANIMS: u32 = 0x169948;
pub const RESET_RENDERS: u32 = 0x1128b8;

/// `cAI` vf14 0x111108(ai, reload, quick), the race reset: the game's overlays hidden; +0x4f8, +0x18, +0x14, +0x20,
/// +0x60, +0x1c, +0x64, +0x68 cleared and +0x4fc -1. With `reload`: each roster rider's skin reset (1); then with
/// `quick` +0x4fc -2, +0x4f8 1 and, on a race entered fresh by neither way (+0xe0 and +0xdc 0), each rider loaded at
/// once (1); without `quick` each rider's load requested (a default request, then 1 if the race was entered either
/// way, else 0, and the request's first word). The game's stream seeded from the career's effects seed; each of the
/// race's riders (+0x88) told its +0xa0, given its restart point (the main path and path `k + (6 − riders) / 2`), told
/// its +0x48, and its camera (if any) focused on it. +0xe4..+0xf8 cleared. In the tutorial: with `reload` and not
/// `quick` the tutorial's animations opened (2, 1, 0, 0), then mode 6; else the heap checked, with `reload` the
/// animations reset, the race scene set up from the roster and its animations opened (1, `quick` ^ 1, the list and its
/// count), then mode 1. Last the riders' renders reset. `scratch`: 8 bytes for the game's stack words (the request,
/// the list's count)
pub fn race_reset(m: &mut Mem, e: &mut dyn FeEnv, ai: u32, reload: u32, quick: u32, scratch: u32) {
    let game = m.r32(m.r32(GAPP) + 0x730);
    e.call(m, HIDE_OVERLAYS, &[m.r32(game + 0x38)]);
    m.w32(ai + 0x4f8, 0);
    m.w32(ai + 0x18, 0);
    m.w32(ai + 0x4fc, 0xffff_ffff);
    for at in [0x14u32, 0x20, 0x60, 0x1c, 0x64, 0x68] {
        m.w32(ai + at, 0);
    }
    let entered = |m: &Mem| m.r32(ai + 0xe0) != 0 || m.r32(ai + 0xdc) != 0;
    if reload != 0 {
        let mut k = 0u32;
        while (k as i32) < count(m) {
            e.call(m, RESET_SKIN, &[m.r32(ai + 0xc4 + 4 * k), 1]);
            k += 1;
        }
        if quick != 0 {
            m.w32(ai + 0x4fc, 0xffff_fffe);
            m.w32(ai + 0x4f8, 1);
            if !entered(m) {
                let mut k = 0u32;
                while (k as i32) < count(m) {
                    e.call(m, LOAD_RIDER, &[ai + 0x124, m.r32(ai + 0xc4 + 4 * k), 1]);
                    k += 1;
                }
            }
        } else {
            let mut k = 0u32;
            while (k as i32) < count(m) {
                e.call(m, DEFAULT_REQUEST, &[scratch]);
                let how = entered(m) as u32;
                e.call(m, REQUEST_RIDER, &[ai + 0x124, m.r32(ai + 0xc4 + 4 * k), how, m.r32(scratch)]);
                k += 1;
            }
        }
    }
    e.call(m, SEED_GAME, &[m.r32(FX_SEED)]);
    let main = e.call(m, MAIN_PATH, &[AI_PATHS]);
    let half = (6 - m.r32(ai + 0x88) as i32) / 2;
    let mut k = 0u32;
    while (k as i32) < m.r32(ai + 0x88) as i32 {
        let b = m.r32(ai + 0xc4 + 4 * k);
        vcall_at(m, e, b, 0, 0xa0, &[]);
        let path = e.call(m, GET_PATH, &[AI_PATHS, (k as i32 + half) as u32]);
        e.call(m, RESTART_POINT, &[m.r32(ai + 0xc4 + 4 * k), main, path]);
        let b = m.r32(ai + 0xc4 + 4 * k);
        vcall_at(m, e, b, 0, 0x48, &[]);
        let cam = e.call(m, GET_CAMERA, &[m.r32(ai + 0xc4 + 4 * k)]);
        if cam != 0 {
            e.call(m, FOCUS_RIDER, &[cam, m.r32(ai + 0xc4 + 4 * k)]);
        }
        k += 1;
    }
    for k in (0..6u32).rev() {
        m.w32(ai + 0xe4 + 4 * k, 0);
    }
    let mode = if e.call(m, IS_TUTORIAL, &[ai]) != 0 {
        if reload != 0 && quick == 0 {
            e.call(m, OPEN_BIG, &[ANIM_DATA, 2, 1, 0, 0]);
        }
        6
    } else {
        e.call(m, VALIDATE_ALL, &[]);
        if reload != 0 {
            e.call(m, ANIM_RESET, &[ANIM_DATA]);
            e.call(m, SCENE_SETUP, &[SCENE, ROSTER_COUNT]);
            let list = e.call(m, SCENE_ANIMS, &[SCENE, scratch + 4]);
            e.call(m, OPEN_BIG, &[ANIM_DATA, 1, quick ^ 1, list, m.r32(scratch + 4)]);
        }
        1
    };
    vcall_at(m, e, ai, 8, 0x50, &[mode]);
    e.call(m, RESET_RENDERS, &[ai]);
}

/// `cAI` vf15 0x1114d0(ai), the restart: the world lists' kinds 3, 0xd and 4 emptied, the career's effects seed from
/// `Rand_Fx`, entered by a restart (+0xdc 1, +0xe0 0), then its table's +0x78 with 0, 0
pub fn restart(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    for kind in [3u32, 0xd, 4] {
        e.call(m, LIST_DELETE_ALL, &[LISTS, kind]);
    }
    let seed = e.call(m, RAND_FX, &[]);
    m.w32(FX_SEED, seed);
    m.w32(ai + 0xdc, 1);
    m.w32(ai + 0xe0, 0);
    vcall_at(m, e, ai, 8, 0x78, &[0, 0]);
}

/// `Replay_SetMode11From10` 0x191278, `Race_HasStartGate` 0x112848, `World_RunScript` 0x13bad8 with "NoCountdown"
/// (0x364628), `Race_GetRider` 0x1814a8, `Boarder_FlushEndedClips` 0x117190, `Boarder_UpdateAnimLayers` 0x117180,
/// `Boarder_UpdatePose` 0x129628, `Boarder_UpdateBroadphase` 0x126748, `Race_UpdateLookBack` 0x113a88
pub const REPLAY_MODE: u32 = 0x191278;
pub const HAS_START_GATE: u32 = 0x112848;
pub const RUN_SCRIPT: u32 = 0x13bad8;
pub const NO_COUNTDOWN: u32 = 0x364628;
pub const GET_RIDER: u32 = 0x1814a8;
pub const FLUSH_CLIPS: u32 = 0x117190;
pub const ANIM_LAYERS: u32 = 0x117180;
pub const UPDATE_POSE: u32 = 0x129628;
pub const BROADPHASE: u32 = 0x126748;
pub const LOOK_BACK: u32 = 0x113a88;
/// `cPreAI`'s table (0x3646e8) and `cPostAI`'s (0x364698)
pub const PRE_AI_VT: u32 = 0x3646e8;
pub const POST_AI_VT: u32 = 0x364698;
/// `cAIWorld`'s table (0x365798; its destructor 0x116650 is [`ai_node_destruct`] too)
pub const AI_WORLD_VT: u32 = 0x365798;

/// `cStartRaceHandler::Enter` 0x113478: the game's replay (game +0x40), if any, moved on from mode 10
/// (`Replay_SetMode11From10`)
pub fn start_race_enter(m: &mut Mem, e: &mut dyn FeEnv) {
    let replay = m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x40);
    if replay != 0 {
        e.call(m, REPLAY_MODE, &[replay]);
    }
}

/// `cStartRaceHandler::Update` 0x1134a0(handler, ai): on to the countdown (the race's +0x50 with 3)
pub fn start_race_update(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    vcall_at(m, e, ai, 8, 0x50, &[3]);
}

/// `cStartRaceHandler::Exit` 0x1134d0(handler, ai): on a course without a start gate the "NoCountdown" world script
/// run and the race's +0x68
pub fn start_race_exit(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    if e.call(m, HAS_START_GATE, &[ai]) == 0 {
        e.call(m, RUN_SCRIPT, &[NO_COUNTDOWN]);
        vcall_at(m, e, ai, 8, 0x68, &[]);
    }
}

/// `cCountdownHandler::Exit` 0x113738: nothing
pub fn countdown_exit() {}

/// The game's race object (gApp+0x730 → +0x1c) and its rider count (+0x88), read anew each time as the game does
fn race_riders(m: &Mem) -> i32 {
    m.r32(m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x1c) + 0x88) as i32
}

/// `cPreAI` vf1 0x1146b0, before the riders' AI: each rider's ended clips flushed
pub fn pre_ai_tick(m: &mut Mem, e: &mut dyn FeEnv) {
    let mut k = 0u32;
    while (k as i32) < race_riders(m) {
        let game = m.r32(m.r32(GAPP) + 0x730);
        let b = e.call(m, GET_RIDER, &[game, k]);
        e.call(m, FLUSH_CLIPS, &[b]);
        k += 1;
    }
}

/// `cPostAI` vf1 0x114730, after the riders' AI: each rider's animation layers updated, its pose when it has one
/// (+0x46b0), its broad phase, and +0x42c = +0x16c; then the race's look-back (`Race_UpdateLookBack` on the race)
pub fn post_ai_tick(m: &mut Mem, e: &mut dyn FeEnv) {
    let mut k = 0u32;
    while (k as i32) < race_riders(m) {
        let game = m.r32(m.r32(GAPP) + 0x730);
        let b = e.call(m, GET_RIDER, &[game, k]);
        e.call(m, ANIM_LAYERS, &[b]);
        if m.r32(b + 0x46b0) != 0 {
            e.call(m, UPDATE_POSE, &[b]);
        }
        e.call(m, BROADPHASE, &[b]);
        m.w32(b + 0x42c, m.r32(b + 0x16c));
        k += 1;
    }
    let race = m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x1c);
    e.call(m, LOOK_BACK, &[race]);
}

/// `cPreAI::~cPreAI` 0x114b08, `cPostAI::~cPostAI` 0x114a68 and `cAIWorld::~cAIWorld` 0x116650(node, how): the class's table back at +8, then the
/// node's destructor with `how`
pub fn ai_node_destruct(m: &mut Mem, e: &mut dyn FeEnv, node: u32, vt: u32, how: u32) {
    m.w32(node + 8, vt);
    e.call(m, NODE_DTOR, &[node, how]);
}

/// The race's modes' handlers (`Race_CreateHandlers` 0x1125c8, its jump table 0x3645f0) by mode 1..11: the handler's
/// place in the race object and its name (0x364558..): 1 PreRace +0x24, 2 Countdown +0x2c, 3 Race +0x30, 4 EndRace
/// +0x34, 5 ReplayEndRace +0x38, 6 LoadLesson +0x48, 7 PreLesson +0x4c, 8 RestartLesson +0x50, 9 SelectLesson +0x54,
/// 10 EndLesson +0x58, 11 StartRace +0x5c
pub const MODE_HANDLERS: [(u32, u32); 11] = [
    (0x24, 0x364558),
    (0x2c, 0x364560),
    (0x30, 0x364570),
    (0x34, 0x364580),
    (0x38, 0x364588),
    (0x48, 0x364590),
    (0x4c, 0x3645a0),
    (0x50, 0x3645b0),
    (0x54, 0x3645c0),
    (0x58, 0x3645d0),
    (0x5c, 0x3645e0),
];

/// `Race_CreateHandlers` 0x1125c8(ai, mode): for a mode 1..11 its handler (+0x64), the mode (+0x1c) and the
/// handler's name (+0x68); any other mode clears the three
pub fn create_handlers(m: &mut Mem, ai: u32, mode: u32) {
    match MODE_HANDLERS.get(mode.wrapping_sub(1) as usize) {
        Some(&(at, name)) => {
            m.w32(ai + 0x64, ai + at);
            m.w32(ai + 0x1c, mode);
            m.w32(ai + 0x68, name);
        }
        None => {
            m.w32(ai + 0x1c, 0);
            m.w32(ai + 0x64, 0);
            m.w32(ai + 0x68, 0);
        }
    }
}

/// `cAI` vf9 0x1125a0(ai, mode), the mode asked for: a new mode keeps the one it leaves (+0x20) and that mode's
/// handler (+0x60), then [`create_handlers`]; the same mode does nothing
pub fn set_mode(m: &mut Mem, ai: u32, mode: u32) {
    let cur = m.r32(ai + 0x1c);
    if mode == cur {
        return;
    }
    m.w32(ai + 0x20, cur);
    m.w32(ai + 0x60, m.r32(ai + 0x64));
    create_handlers(m, ai, mode);
}

/// `cAI` vf1 0x1126f0(ai), the race's tick: on a mode change (+0x20 ≠ +0x1c) the old handler's Exit (its table
/// +0x10, if there is one), the new one's Enter (+8), the old one forgotten and the change noted; then the handler's
/// Update (+0x18) and the race's tick count (+0x18) up by one. Each handler gets (itself, the race).
pub fn tick(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    if m.r32(ai + 0x20) != m.r32(ai + 0x1c) {
        let old = m.r32(ai + 0x60);
        if old != 0 {
            vcall_at(m, e, old, 0, 0x10, &[ai]);
        }
        let new = m.r32(ai + 0x64);
        vcall_at(m, e, new, 0, 8, &[ai]);
        m.w32(ai + 0x60, 0);
        m.w32(ai + 0x20, m.r32(ai + 0x1c));
    }
    let h = m.r32(ai + 0x64);
    vcall_at(m, e, h, 0, 0x18, &[ai]);
    m.w32(ai + 0x18, m.r32(ai + 0x18).wrapping_add(1));
}

/// The game's mode word (the career's +0x2afc, 0x32de8c: 3 and 5 the show-offs, 6 the tutorial) and its course
/// (+0x2af8); the courses' records (0x332f28, 0x1c bytes each: +0xc the show-off time in hundredths)
pub const CAREER_AT: u32 = CAREER;
pub const GAME_MODE: u32 = CAREER + 0x2afc;
pub const GAME_COURSE: u32 = CAREER + 0x2af8;
pub const COURSE_TABLE: u32 = 0x332f28;
/// The speech system (its pointer at 0x343050)
pub const SPEECH: u32 = 0x343050;
/// `Boarder_ResetRender` 0x11a7a8, `Boarder_LaunchFromGate` 0x11a7b0, `Boarder_DrainMeter` 0x11b1f8,
/// `Boarder_Finish` 0x11da40, `Boarder_IsFinishedAndStopped` 0x11d998, `Race_UpdatePairDistances` 0x115a80,
/// `Race_UpdatePlacingsAndRubberBand` 0x115100, `Speech_OnTimeUp` 0x234058, `Audio_SetMusicState` 0x21d610,
/// `Circuit_StoreRiderAttitudes` 0x16c228
pub const RESET_RENDER: u32 = 0x11a7a8;
pub const LAUNCH: u32 = 0x11a7b0;
pub const DRAIN_METER: u32 = 0x11b1f8;
pub const FINISH: u32 = 0x11da40;
pub const FINISHED: u32 = 0x11d998;
pub const PAIR_DISTANCES: u32 = 0x115a80;
pub const PLACINGS: u32 = 0x115100;
pub const TIME_UP: u32 = 0x234058;
pub const MUSIC_STATE: u32 = 0x21d610;
pub const STORE_ATTITUDES: u32 = 0x16c228;
/// `AsyncBdrLoad_Request` 0x201e68, `_IsSlotReady` 0x201d30, `BdrLoad_BindTextures` 0x2021f8, `AsyncBdrLoad_FreeSlot`
/// 0x201c20, `AnimData_PollLoad` 0x158fc0, `SpeechSys_IsCharScriptReady` 0x235710, `HUD_HideItem` 0x1a7860 /
/// `HUD_ShowItem` 0x1a7848
pub const BDR_REQUEST: u32 = 0x201e68;
pub const SLOT_READY: u32 = 0x201d30;
pub const BIND_TEXTURES: u32 = 0x2021f8;
pub const FREE_SLOT: u32 = 0x201c20;
pub const ANIM_POLL: u32 = 0x158fc0;
pub const SPEECH_READY: u32 = 0x235710;
pub const HUD_HIDE: u32 = 0x1a7860;
pub const HUD_SHOW: u32 = 0x1a7848;
/// The show-off clock's step, 1/60 s (0x3c888889)
pub const CLOCK_STEP: f32 = 1.0 / 60.0;
/// The music state asked for when everyone is through: (0, 0.65)
pub const FINISH_MUSIC: f32 = 0.65;

/// `Race_GoToEndLesson` 0x1117f0(ai): the race's +0x50 with 10 (mode EndLesson)
pub fn go_to_end_lesson(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    vcall_at(m, e, ai, 8, 0x50, &[10]);
}

/// `Race_ResetRiderRenders` 0x1128b8(ai): each of the race's riders' render reset
pub fn reset_rider_renders(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    let mut k = 0u32;
    while (k as i32) < m.r32(ai + 0x88) as i32 {
        e.call(m, RESET_RENDER, &[m.r32(ai + 0xc4 + 4 * k)]);
        k += 1;
    }
}

/// `Game_IsTutorial` 0x1148d0: the game's mode is 6
pub fn is_tutorial(m: &Mem) -> bool {
    m.r32(GAME_MODE) == 6
}

/// `Race_FindRiderByPlayer` 0x1148e8(race, player): the first of the race's players (+0x7c of them, controllers at
/// +0x8c) whose boarder's info (+0x464) has `player` at +0x65 (signed). As the original: when none has, the answer is
/// the first controller all the same (and with no players the word at +0x8c)
pub fn find_rider_by_player(m: &Mem, race: u32, player: u32) -> u32 {
    let first = m.r32(race + 0x8c);
    let n = m.r32(race + 0x7c) as i32;
    let id = |c: u32| m.r8(m.r32(m.r32(c) + 0x464) + 0x65) as i8 as i32 as u32;
    if n <= 0 || id(first) == player {
        return first;
    }
    for k in 1..n as u32 {
        let c = m.r32(race + 0x8c + 4 * k);
        if id(c) == player {
            return c;
        }
    }
    first
}

/// `Career_StoreCircuitAttitudes` 0x114f70(career): `Circuit_StoreRiderAttitudes` on its +0x320c with the roster
/// (+0x27dc count, +0x27e0 entries)
pub fn store_circuit_attitudes(m: &mut Mem, e: &mut dyn FeEnv, career: u32) {
    e.call(m, STORE_ATTITUDES, &[career + 0x320c, m.r32(career + 0x27dc), career + 0x27e0]);
}

/// `cRaceHandler::Enter` 0x113740(handler, ai): +0xe4..+0xf8 cleared; the clock (+0x14) the course's show-off time in
/// seconds in the show-offs (modes 3, 5), else 0; each rider launched from the gate (the count read first: none
/// launched when it was 0)
pub fn race_enter(m: &mut Mem, e: &mut dyn FeEnv, ai: u32) {
    let n = m.r32(ai + 0x88) as i32;
    for k in (0..6u32).rev() {
        m.w32(ai + 0xe4 + 4 * k, 0);
    }
    let mode = m.r32(GAME_MODE);
    if mode == 3 || mode == 5 {
        let course = m.r32(GAME_COURSE);
        let t = m.r32(course.wrapping_mul(0x1c).wrapping_add(COURSE_TABLE + 0xc)) as i32;
        m.w32(ai + 0x14, (t as f32 / 100.0).to_bits());
    } else {
        m.w32(ai + 0x14, 0);
    }
    if n > 0 {
        let mut k = 0u32;
        loop {
            e.call(m, LAUNCH, &[m.r32(ai + 0xc4 + 4 * k)]);
            k += 1;
            if k as i32 >= m.r32(ai + 0x88) as i32 {
                break;
            }
        }
    }
}

/// `cRaceHandler::Update` 0x113820(handler, ai): each rider's meter drained, the pair distances and the placings
/// (both with the handler and the race); in the show-offs the clock counts down 1/60 s and at 0 or below is held at
/// 0, every rider finished (`Boarder_Finish` 1) and, when one of them had not (+0x418 negative), the speech's time-up;
/// in the other modes it counts up. Then, out of the tutorial (game +0x34 null): if every player (+0x7c of them;
/// with none, every rider) is finished and stopped, the music's state (0, 0.65) and the race's +0xa0 with -1
pub fn race_update(m: &mut Mem, e: &mut dyn FeEnv, handler: u32, ai: u32) {
    let mut k = 0u32;
    while (k as i32) < m.r32(ai + 0x88) as i32 {
        e.call(m, DRAIN_METER, &[m.r32(ai + 0xc4 + 4 * k)]);
        k += 1;
    }
    e.call(m, PAIR_DISTANCES, &[handler, ai]);
    e.call(m, PLACINGS, &[handler, ai]);
    let mode = m.r32(GAME_MODE);
    let clock = f32::from_bits(m.r32(ai + 0x14));
    if mode == 3 || mode == 5 {
        let left = clock - CLOCK_STEP;
        m.w32(ai + 0x14, left.to_bits());
        if left <= 0.0 {
            m.w32(ai + 0x14, 0f32.to_bits());
            let mut late = false;
            let mut k = 0u32;
            while (k as i32) < m.r32(ai + 0x88) as i32 {
                let b = m.r32(ai + 0xc4 + 4 * k);
                if (m.r32(b + 0x418) as i32) < 0 {
                    late = true;
                }
                e.call(m, FINISH, &[b, 1]);
                k += 1;
            }
            if late {
                e.call(m, TIME_UP, &[m.r32(SPEECH)]);
            }
        }
    } else {
        m.w32(ai + 0x14, (clock + CLOCK_STEP).to_bits());
    }
    if m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x34) != 0 {
        return;
    }
    let (count, list, deref) = if m.r32(ai + 0x7c) as i32 > 0 { (0x7c, 0x8c, true) } else { (0x88, 0xc4, false) };
    let mut all = 1u32;
    let mut k = 0u32;
    while (k as i32) < m.r32(ai + count) as i32 {
        let c = m.r32(ai + list + 4 * k);
        let b = if deref { m.r32(c) } else { c };
        all = e.call(m, FINISHED, &[b]);
        k += 1;
        if all == 0 {
            break;
        }
    }
    if all != 0 {
        e.call(m, MUSIC_STATE, &[m.r32(SPEECH), 0, FINISH_MUSIC.to_bits()]);
        vcall_at(m, e, ai, 8, 0xa0, &[0xffff_ffff]);
    }
}

/// `Race_UpdateRiderStreaming` 0x112920(ai), while the riders stream in (+0x4f8): a fresh start (+0x4fc -2) asks for
/// rider 0's files; a slot asked for (+0x500 not negative) that is ready has its textures bound and is freed, and the
/// next rider's asked for (or none left: +0x500 and +0x4fc -1); not ready, the HUD's loading item stays. When the
/// animations, the speech and the riders are all in, the loading item (HUD 8) is hidden, a slot still held is bound
/// and freed and the streaming ends; else it is shown. `scratch`: 8 bytes for the two request words
pub fn update_rider_streaming(m: &mut Mem, e: &mut dyn FeEnv, ai: u32, scratch: u32) {
    if m.r32(ai + 0x4f8) == 0 {
        return;
    }
    let slots = ai + 0x124;
    let mut riders_in = true;
    if m.r32(ai + 0x4fc) as i32 == -2 {
        m.w32(ai + 0x4fc, 0);
        e.call(m, DEFAULT_REQUEST, &[scratch]);
        let b = m.r32(ai + 0xc4 + 4 * m.r32(ai + 0x4fc));
        let s = e.call(m, BDR_REQUEST, &[slots, b, 1, m.r32(scratch)]);
        m.w32(ai + 0x500, s);
    }
    let slot = m.r32(ai + 0x500);
    if slot as i32 >= 0 {
        if e.call(m, SLOT_READY, &[slots, slot]) == 0 {
            riders_in = false;
        } else if m.r32(ai + 0x4fc) as i32 >= 0 {
            e.call(m, BIND_TEXTURES, &[slots, m.r32(ai + 0x500)]);
            e.call(m, FREE_SLOT, &[slots, m.r32(ai + 0x500)]);
            let next = m.r32(ai + 0x4fc).wrapping_add(1);
            m.w32(ai + 0x4fc, next);
            if (next as i32) < m.r32(ai + 0x88) as i32 {
                e.call(m, DEFAULT_REQUEST, &[scratch + 4]);
                riders_in = false;
                let b = m.r32(ai + 0xc4 + 4 * m.r32(ai + 0x4fc));
                let s = e.call(m, BDR_REQUEST, &[slots, b, 1, m.r32(scratch + 4)]);
                m.w32(ai + 0x500, s);
            } else {
                m.w32(ai + 0x500, 0xffff_ffff);
                m.w32(ai + 0x4fc, 0xffff_ffff);
            }
        }
    }
    let anims = e.call(m, ANIM_POLL, &[ANIM_DATA]);
    let speech = e.call(m, SPEECH_READY, &[m.r32(SPEECH)]);
    let hud = m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x38);
    if anims == 0 || !riders_in || speech == 0 {
        e.call(m, HUD_SHOW, &[hud, 8, 0xffff_ffff]);
        return;
    }
    e.call(m, HUD_HIDE, &[hud, 8, 0xffff_ffff]);
    // (the original asks first whether a slot was asked for this tick: it can only be held if so)
    if m.r32(ai + 0x500) as i32 >= 0 {
        e.call(m, SLOT_READY, &[slots, m.r32(ai + 0x500)]);
        e.call(m, BIND_TEXTURES, &[slots, m.r32(ai + 0x500)]);
        e.call(m, FREE_SLOT, &[slots, m.r32(ai + 0x500)]);
        m.w32(ai + 0x500, 0xffff_ffff);
    }
    m.w32(ai + 0x4f8, 0);
}

/// `cLessonMan`'s table (0x36e570), `BigArchive_Close` 0x23bcc0, `BigArchive_Delete` 0x23bb98,
/// `cGameTrickInfo::~cGameTrickInfo` 0x155058
pub const LESSON_VT: u32 = 0x36e570;
pub const ARCHIVE_CLOSE_BIG: u32 = 0x23bcc0;
pub const ARCHIVE_DELETE: u32 = 0x23bb98;
pub const TRICK_INFO_DTOR: u32 = 0x155058;

/// `cLessonMan::~cLessonMan` 0x14b520(lesson, how): its table back; the recording buffer (+0x46c), if any, deleted;
/// the lesson archive (+0xa8), if open, closed and (read again) deleted with 3 and forgotten; the trick info (+0x514)
/// destroyed with 2; the node's destructor with `how`
pub fn lesson_destruct(m: &mut Mem, e: &mut dyn FeEnv, l: u32, how: u32) {
    m.w32(l + 8, LESSON_VT);
    let buf = m.r32(l + 0x46c);
    if buf != 0 {
        e.call(m, DELETE, &[buf]);
    }
    if m.r32(l + 0xa8) != 0 {
        e.call(m, ARCHIVE_CLOSE_BIG, &[m.r32(l + 0xa8)]);
        let a = m.r32(l + 0xa8);
        if a != 0 {
            e.call(m, ARCHIVE_DELETE, &[a, 3]);
        }
        m.w32(l + 0xa8, 0);
    }
    e.call(m, TRICK_INFO_DTOR, &[l + 0x514, 2]);
    e.call(m, NODE_DTOR, &[l, how]);
}

/// `PadCfg_Destruct` 0x14f550(cfg, how): deleted when `how` has bit 0
pub fn pad_cfg_destruct(m: &mut Mem, e: &mut dyn FeEnv, cfg: u32, how: u32) {
    if how & 1 != 0 {
        e.call(m, DELETE, &[cfg]);
    }
}

/// `cComputer`'s virtual-base (`cBoarder`) table (0x36a910, 0xb0 bytes) and own table (0x36a9c0), plus callees
/// `cBoarder::cBoarder` 0x116828, `cBoarderHandler::cBoarderHandler` 0x11b348, `AIComputer_Reset` 0x1383b0,
/// `cBoarder::vf8` 0x116ab8, `cBoarderHandler::~cBoarderHandler` 0x11f770, `cBoarder::~cBoarder` 0x116990
pub const COMPUTER_VBASE_VT: u32 = 0x36a910;
pub const COMPUTER_VT: u32 = 0x36a9c0;
pub const BOARDER_CTOR: u32 = 0x116828;
pub const BOARDER_HANDLER_CTOR: u32 = 0x11b348;
pub const COMPUTER_RESET: u32 = 0x1383b0;
pub const BOARDER_VF8: u32 = 0x116ab8;
pub const BOARDER_HANDLER_DTOR: u32 = 0x11f770;
pub const BOARDER_DTOR: u32 = 0x116990;

const COMPUTER_VBASE_SLOTS: [u32; 29] = [
    0x828, 0x80c, 0x7f0, 0x7e0, 0x7d0, 0x7c0, 0x7b0, 0x734, 0x71c, 0x70c, 0x6fc, 0x6e0, 0x6d0, 0x6c0, 0x6b4,
    0x690, 0x680, 0x660, 0x650, 0x644, 0x620, 0x600, 0x5fc, 0x550, 0x4e0, 0x4d0, 0x4bc, 0x4a0, 0x000,
];
const COMPUTER_VBASE_ADJ: [u32; 5] = [0x08, 0x40, 0x48, 0x50, 0x70];

fn install_computer_vbase_vt(m: &mut Mem, o: u32, b: u32, copy: u32) {
    for k in 0..0xb0 / 4 {
        let w = m.r32(COMPUTER_VBASE_VT + 4 * k);
        m.w32(copy + 4 * k, w);
    }
    let delta = o.wrapping_sub(b.wrapping_sub(0x8b0)) as u16;
    for off in COMPUTER_VBASE_ADJ {
        let adj = m.r16(COMPUTER_VBASE_VT + off).wrapping_add(delta);
        m.w16(copy + off, adj);
    }
    m.w32(b, copy);
}

/// `cComputer::cComputer` 0x138210(o, in_charge, rider_idx; sp on entry): when `in_charge != 0`, fills the 29
/// virtual-base pointers with `o + 0x8b0` and constructs `cBoarder` there (`0x116828`); then constructs
/// `cBoarderHandler` (`0x11b348`, `0`, sign-extended `rider_idx`), installs `cComputer`'s virtual-base vtable
/// (`0x36a910`, or its adjusted 0xb0-byte stack copy at `sp - 0xf0` when `in_charge == 0`) and own vtable at
/// `+0x834` (`0x36a9c0`), runs `AIComputer_Reset` (`0x1383b0`), and returns `o`
pub fn computer_construct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, in_charge: u32, rider_idx: i16, sp: u32) -> u32 {
    if in_charge != 0 {
        let vb = o.wrapping_add(0x8b0);
        for off in COMPUTER_VBASE_SLOTS {
            m.w32(o.wrapping_add(off), vb);
        }
        e.call(m, BOARDER_CTOR, &[vb]);
    }
    e.call(m, BOARDER_HANDLER_CTOR, &[o, 0, rider_idx as i32 as u32]);
    let b = m.r32(o);
    m.w32(b, COMPUTER_VBASE_VT);
    if in_charge == 0 {
        install_computer_vbase_vt(m, o, b, sp.wrapping_sub(0xf0));
    }
    m.w32(o.wrapping_add(0x834), COMPUTER_VT);
    e.call(m, COMPUTER_RESET, &[o]);
    o
}

/// `cComputer::vf8_138438` 0x138438(o): calls `AIComputer_Reset` (`0x1383b0`) on `o`, then tail-calls
/// `cBoarder::vf8` (`0x116ab8`) on `*(u32*)o`
pub fn computer_vf8(m: &mut Mem, e: &mut dyn FeEnv, o: u32) {
    e.call(m, COMPUTER_RESET, &[o]);
    let b = m.r32(o);
    e.call(m, BOARDER_VF8, &[b]);
}

/// `cComputer::~cComputer` 0x13a628(o, how; sp on entry): restores `cComputer`'s vtables (`0x36a9c0` at `+0x834`,
/// `0x36a910` at `*o`, or an adjusted 0xb0-byte stack copy at `sp - 0xf0` when `how == 0`), destroys
/// `cBoarderHandler` (`0x11f770`, `0`), destroys `cBoarder` (`0x116990`, `0`) when `how & 2 != 0`, and deletes
/// `o` (`Mem_Delete` `0x114f80`) when `how & 1 != 0`
pub fn computer_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32, sp: u32) {
    m.w32(o.wrapping_add(0x834), COMPUTER_VT);
    let b = m.r32(o);
    m.w32(b, COMPUTER_VBASE_VT);
    let (destroy_base, delete_self) = if how == 0 {
        install_computer_vbase_vt(m, o, b, sp.wrapping_sub(0xf0));
        (false, false)
    } else {
        (how & 2 != 0, how & 1 != 0)
    };
    e.call(m, BOARDER_HANDLER_DTOR, &[o, 0]);
    if destroy_base {
        let b = m.r32(o);
        e.call(m, BOARDER_DTOR, &[b, 0]);
    }
    if delete_self {
        e.call(m, DELETE, &[o]);
    }
}

/// Constants and callees for row G11f (the last 15 `race` functions):
/// `cPlayer` virtual-base table (`0x36f168`, 0xb0 bytes), own table (`0x36f218`), globals (`gCareer` 0x32b390,
/// `gPreRaceSel` 0x32ecf0, `gRaceScene` 0x32edc0, `gCharTable` 0x331ea8, `gRelTablePtr` 0x3352b4), and callees
pub const PLAYER_VBASE_VT: u32 = 0x36f168;
pub const PLAYER_VT: u32 = 0x36f218;
pub const G_CAREER: u32 = 0x32b390;
pub const G_PRERACE_SEL: u32 = 0x32ecf0;
pub const G_RACE_SCENE: u32 = 0x32edc0;
pub const G_CHAR_TABLE: u32 = 0x331ea8;
pub const G_REL_TABLE_PTR: u32 = 0x3352b4;
pub const RACE_GET_RIDER: u32 = 0x1814a8;
pub const BOARDER_FIND_NEAREST_LINE: u32 = 0x119d30;
pub const REPLAY_ADD_INPUT_STREAM: u32 = 0x191ca0;
pub const REPLAY_ADD_PIN: u32 = 0x191a18;
pub const REPLAY_REMOVE_INPUT_STREAM: u32 = 0x191d30;
pub const REPLAY_REMOVE_PIN: u32 = 0x191ac0;
pub const RIVALS_INIT_PAIRS: u32 = 0x167ea0;
pub const PRERACE_SEL_CLEAR: u32 = 0x1686a8;
pub const PRERACE_SEL_CHOOSE: u32 = 0x168720;
pub const CAREER_FINALIZE_ROSTER: u32 = 0x168b88;
pub const CAREER_SETUP_PLAYER_RIDER: u32 = 0x16a9d8;
pub const CAREER_SETUP_AI_RIDER: u32 = 0x16ae68;
pub const CIRCUIT_GET_MEDAL_THRESHOLDS: u32 = 0x16ba60;

const PLAYER_VBASE_ADJ: [u32; 10] = [0x08, 0x40, 0x50, 0x60, 0x68, 0x78, 0x80, 0x88, 0x98, 0xa0];

/// `Race_FindLineAhead` 0x11a020(out; delta, f13): fetches `Race_GetRider(gApp+0x730)` (`0x1814a8`), adds `delta`
/// to `rider+0x374` (clamping below 0.0 to +0.0), and tail-calls `Boarder_FindNearestLine(out, 1; clamped, f13)`
/// (`0x119d30`, float bits passed as args 2 and 3)
pub fn race_find_line_ahead(m: &mut Mem, e: &mut dyn FeEnv, out: u32, delta: f32, f13: f32) -> u32 {
    use crate::ps2f::F;
    let game = m.r32(m.r32(GAPP) + 0x730);
    let rider = e.call(m, RACE_GET_RIDER, &[game]);
    let d = F(f32::from_bits(m.r32(rider + 0x374))) + F(delta);
    let clamped = if d.0 < 0.0 { 0.0f32 } else { d.0 };
    e.call(m, BOARDER_FIND_NEAREST_LINE, &[out, 1, clamped.to_bits(), f13.to_bits()])
}

/// `Race_CreateBoarders` 0x150e38 (`cPlayer::cPlayer`: o, in_charge, rider_idx; sp on entry): fills the 29
/// virtual-base pointers with `o + 0x880` and constructs `cBoarder` (`0x116828`) when `in_charge != 0`; constructs
/// `cBoarderHandler` (`0x11b348`, `0`, sign-extended `rider_idx`); installs `PLAYER_VBASE_VT` (or its 10-entry
/// adjusted 0xb0-byte stack copy at `sp - 0x120` when `in_charge == 0`) and `PLAYER_VT` at `+0x834`; zeroes `+0x878`
/// and `+0x87c`, registers replay input stream (`0x191ca0`) and pin (`0x191a18`) when `gApp+0x730 -> +0x40` is set;
/// sets `(*o)+0x41c = 1` and returns `o`
pub fn player_construct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, in_charge: u32, rider_idx: i16, sp: u32) -> u32 {
    if in_charge != 0 {
        let vb = o.wrapping_add(0x880);
        for off in COMPUTER_VBASE_SLOTS {
            m.w32(o.wrapping_add(off), vb);
        }
        e.call(m, BOARDER_CTOR, &[vb]);
    }
    e.call(m, BOARDER_HANDLER_CTOR, &[o, 0, rider_idx as i32 as u32]);
    let b = m.r32(o);
    m.w32(b, PLAYER_VBASE_VT);
    if in_charge == 0 {
        let copy = sp.wrapping_sub(0x120);
        for k in 0..0xb0 / 4 {
            let w = m.r32(PLAYER_VBASE_VT + 4 * k);
            m.w32(copy + 4 * k, w);
        }
        let delta = o.wrapping_sub(b.wrapping_sub(0x880)) as u16;
        for off in PLAYER_VBASE_ADJ {
            let adj = m.r16(PLAYER_VBASE_VT + off).wrapping_add(delta);
            m.w16(copy + off, adj);
        }
        m.w32(b, copy);
    }
    m.w32(o.wrapping_add(0x834), PLAYER_VT);
    m.w32(o.wrapping_add(0x878), 0);
    m.w32(o.wrapping_add(0x87c), 0);
    let replay = m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x40);
    let b_final = if replay != 0 {
        let inp = e.call(m, REPLAY_ADD_INPUT_STREAM, &[replay]);
        m.w32(o.wrapping_add(0x878), inp);
        let game = m.r32(m.r32(GAPP) + 0x730);
        let ai = m.r32(game + 0x1c);
        let rep2 = m.r32(game + 0x40);
        let nplayers = m.r32(ai + 0x7c);
        let pin = e.call(m, REPLAY_ADD_PIN, &[rep2, nplayers]);
        m.w32(o.wrapping_add(0x87c), pin);
        m.r32(o)
    } else {
        b
    };
    m.w32(b_final.wrapping_add(0x41c), 1);
    o
}

/// `cPlayer::~cPlayer` 0x151080(o, how; sp on entry): restores `PLAYER_VT` at `+0x834` and `PLAYER_VBASE_VT` at `*o`
/// (or its 10-entry adjusted 0xb0-byte stack copy at `sp - 0x120` when `how == 0`), removes replay input stream
/// (`0x191d30`) if `+0x878 != 0` and replay pin (`0x191ac0`) if `+0x87c != 0`, deletes `*(o+0x874)`, destroys
/// `cBoarderHandler` (`0x11f770`, `0`), destroys `cBoarder` (`0x116990`, `0`) on `how & 2`, and deletes `o` on
/// `how & 1`
pub fn player_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32, sp: u32) {
    m.w32(o.wrapping_add(0x834), PLAYER_VT);
    let b = m.r32(o);
    m.w32(b, PLAYER_VBASE_VT);
    let (destroy_base, delete_self) = if how == 0 {
        let copy = sp.wrapping_sub(0x120);
        for k in 0..0xb0 / 4 {
            let w = m.r32(PLAYER_VBASE_VT + 4 * k);
            m.w32(copy + 4 * k, w);
        }
        let delta = o.wrapping_sub(b.wrapping_sub(0x880)) as u16;
        for off in PLAYER_VBASE_ADJ {
            let adj = m.r16(PLAYER_VBASE_VT + off).wrapping_add(delta);
            m.w16(copy + off, adj);
        }
        m.w32(b, copy);
        (false, false)
    } else {
        (how & 2 != 0, how & 1 != 0)
    };
    let inp = m.r32(o.wrapping_add(0x878));
    if inp != 0 {
        let rep = m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x40);
        e.call(m, REPLAY_REMOVE_INPUT_STREAM, &[rep, inp]);
    }
    let pin = m.r32(o.wrapping_add(0x87c));
    if pin != 0 {
        let rep = m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x40);
        e.call(m, REPLAY_REMOVE_PIN, &[rep, pin]);
    }
    let extra = m.r32(o.wrapping_add(0x874));
    e.call(m, DELETE, &[extra]);
    e.call(m, BOARDER_HANDLER_DTOR, &[o, 0]);
    if destroy_base {
        let b = m.r32(o);
        e.call(m, BOARDER_DTOR, &[b, 0]);
    }
    if delete_self {
        e.call(m, DELETE, &[o]);
    }
}

/// `Circuit_Destruct` 0x16a978(o, how): calls `Mem_Delete(o)` (`0x114f80`) when `how` has bit 0
pub fn circuit_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32) {
    if how & 1 != 0 {
        e.call(m, DELETE, &[o]);
    }
}

/// `Career_Destruct` 0x161760(o, how): destroys embedded circuit at `o + 0x320c` (`2`), then tail-calls
/// [`circuit_destruct`] on `(o, how)`
pub fn career_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32) {
    circuit_destruct(m, e, o.wrapping_add(0x320c), 2);
    circuit_destruct(m, e, o, how);
}

/// `StaticDtor_Career` 0x167170(): tail-calls [`career_destruct`] on `(0x32b390, 2)`
pub fn static_dtor_career(m: &mut Mem, e: &mut dyn FeEnv) {
    career_destruct(m, e, G_CAREER, 2);
}

/// `Career_InitShortRecord` 0x165660(_, src, dst): copies `src[0], src[0x28], src[0x30]` to `dst[0..=8]` and
/// zeroes `dst[0xc]` and `dst[0x10]`
pub fn career_init_short_record(m: &mut Mem, src: u32, dst: u32) {
    let w0 = m.r32(src);
    let w1 = m.r32(src + 0x28);
    let w2 = m.r32(src + 0x30);
    m.w32(dst, w0);
    m.w32(dst + 4, w1);
    m.w32(dst + 8, w2);
    m.w32(dst + 0xc, 0);
    m.w32(dst + 0x10, 0);
}

/// `Career_SetupSoloRider` 0x165688(c): sets `c+0x2b0c = 3`, finds the first non-negative player slot among the
/// four 0x44-byte slots at `c+0x30fc` (copying `c+0x168c+4*ch` to `slot+4`, calling `Career_SetupPlayerRider`, and
/// setting `c+0x27dc = 1`), then runs `Rivals_InitPairs`, `PreRaceSel_Choose(..., 3)`, and `Career_FinalizeRoster`
pub fn career_setup_solo_rider(m: &mut Mem, e: &mut dyn FeEnv, c: u32) {
    m.w32(c.wrapping_add(0x2b0c), 3);
    for slot in 0..4u32 {
        let off = slot * 0x44;
        let ch = m.r8(c.wrapping_add(0x30fc + off)) as i8;
        if ch >= 0 {
            let v = m.r32(c.wrapping_add(0x168c + (ch as u32) * 4));
            m.w32(c.wrapping_add(0x3100 + off), v);
            e.call(m, CAREER_SETUP_PLAYER_RIDER, &[c, c.wrapping_add(0x30fc + off), c.wrapping_add(0x27e0)]);
            m.w32(c.wrapping_add(0x27dc), 1);
            break;
        }
    }
    let rel = m.r32(G_REL_TABLE_PTR);
    let n = m.r32(c.wrapping_add(0x27dc));
    e.call(m, RIVALS_INIT_PAIRS, &[rel, n, c.wrapping_add(0x27e0), c.wrapping_add(0x30fc)]);
    let r0 = m.r32(c.wrapping_add(0x2818));
    let course = m.r32(c.wrapping_add(0x2af8));
    let mode = m.r32(c.wrapping_add(0x2afc));
    e.call(m, PRERACE_SEL_CHOOSE, &[G_PRERACE_SEL, r0, course, mode, 3]);
    e.call(m, CAREER_FINALIZE_ROSTER, &[G_PRERACE_SEL, c.wrapping_add(0x27dc)]);
}

/// `PreRaceSel_Destruct` 0x168688(o, how): calls `Mem_Delete(o)` (`0x114f80`) when `how` has bit 0
pub fn pre_race_sel_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32) {
    if how & 1 != 0 {
        e.call(m, DELETE, &[o]);
    }
}

/// `PreRaceSel_GetPicks` 0x168b80(o): returns `o`
pub fn pre_race_sel_get_picks(o: u32) -> u32 {
    o
}

/// `RaceScene_Destruct` 0x169668(o, how): calls `Mem_Delete(o)` (`0x114f80`) when `how` has bit 0
pub fn race_scene_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32) {
    if how & 1 != 0 {
        e.call(m, DELETE, &[o]);
    }
}

/// `StaticDtor_RaceScene` 0x169d28(): destroys `gRaceScene` (`0x32edc0`, `2`), then `gPreRaceSel` (`0x32ecf0`, `2`)
pub fn static_dtor_race_scene(m: &mut Mem, e: &mut dyn FeEnv) {
    race_scene_destruct(m, e, G_RACE_SCENE, 2);
    pre_race_sel_destruct(m, e, G_PRERACE_SEL, 2);
}

/// `Circuit_ResetHeat` 0x16bca8(o): zeroes `o + 0x14`
pub fn circuit_reset_heat(m: &mut Mem, o: u32) {
    m.w32(o + 0x14, 0);
}

/// `Circuit_OnEventOver` 0x16d0a8(): no-op
pub fn circuit_on_event_over() {}

/// `Circuit_SetupRider` 0x16cfd8(o, slot, rider): if `rider` matches the player's character (`*(*(o+0x10)+0x14)`),
/// copies `career+0x1684+4*ch` to `player_slot+4`, calls `Career_SetupPlayerRider`, and writes `o+0x18` into
/// `roster[slot]+0x74`; otherwise tail-calls `Career_SetupAIRider` with `gCharTable+rider*0x38` and the player's
/// attitude slot at `*(player_slot+0x18)+0x164+8*rider`
pub fn circuit_setup_rider(m: &mut Mem, e: &mut dyn FeEnv, o: u32, slot: u32, rider: u32) {
    let ps = m.r32(o + 0x10);
    let p_ch = m.r32(m.r32(ps + 0x14));
    let entry_off = slot.wrapping_mul(0x84);
    if p_ch == rider {
        let ch = m.r8(ps) as i8 as i32 as u32;
        let career = m.r32(o + 4);
        let v = m.r32(career.wrapping_add(ch << 2).wrapping_add(0x1684));
        m.w32(ps + 4, v);
        let roster = m.r32(o + 8);
        e.call(m, CAREER_SETUP_PLAYER_RIDER, &[o, ps, roster.wrapping_add(entry_off).wrapping_add(4)]);
        let roster2 = m.r32(o + 8);
        let bonus = m.r32(o + 0x18);
        m.w32(roster2.wrapping_add(entry_off).wrapping_add(0x74), bonus);
    } else {
        let char_entry = G_CHAR_TABLE.wrapping_add(rider.wrapping_mul(0x38));
        let att_ptr = m.r32(ps + 0x18).wrapping_add(rider << 3).wrapping_add(0x164);
        let roster_entry = m.r32(o + 8).wrapping_add(entry_off).wrapping_add(4);
        e.call(m, CAREER_SETUP_AI_RIDER, &[o, char_entry, att_ptr, roster_entry]);
    }
}

/// `Circuit_SetupRace` 0x16d230(o; sp on entry): sets roster count `= 1`, sets up the player rider via
/// `Career_SetupPlayerRider`, sets heat `o+0x14 = 2`, looks up `Circuit_GetMedalThresholds(course, sp - 0x40)` to
/// write `heat + (idx << 16)` at `roster+0x330`, clears and populates `gPreRaceSel` (`0x32ecf0`), and calls
/// `Career_FinalizeRoster`
pub fn circuit_setup_race(m: &mut Mem, e: &mut dyn FeEnv, o: u32, sp: u32) {
    let out_idx = sp.wrapping_sub(0x40);
    let roster = m.r32(o + 8);
    let ps = m.r32(o + 0x10);
    m.w32(roster, 1);
    let ch = m.r8(ps) as i8 as i32 as u32;
    let career = m.r32(o + 4);
    let v = m.r32(career.wrapping_add(ch << 2).wrapping_add(0x1684));
    m.w32(ps + 4, v);
    e.call(m, CAREER_SETUP_PLAYER_RIDER, &[o, ps, roster.wrapping_add(4)]);
    m.w32(o + 0x14, 2);
    let course = m.r32(m.r32(o + 8) + 0x31c);
    e.call(m, CIRCUIT_GET_MEDAL_THRESHOLDS, &[course, out_idx]);
    let idx = m.r32(out_idx);
    let heat = m.r32(o + 0x14);
    let roster2 = m.r32(o + 8);
    m.w32(roster2 + 0x330, heat.wrapping_add(idx << 16));
    e.call(m, PRERACE_SEL_CLEAR, &[G_PRERACE_SEL]);
    let ps2 = m.r32(o + 0x10);
    let roster3 = m.r32(o + 8);
    let rider = m.r32(m.r32(ps2 + 0x14));
    let course2 = m.r32(roster3 + 0x31c);
    let mode = m.r32(roster3 + 0x320);
    e.call(m, PRERACE_SEL_CHOOSE, &[G_PRERACE_SEL, rider, course2, mode, 3]);
    let roster4 = m.r32(o + 8);
    e.call(m, CAREER_FINALIZE_ROSTER, &[G_PRERACE_SEL, roster4]);
}
