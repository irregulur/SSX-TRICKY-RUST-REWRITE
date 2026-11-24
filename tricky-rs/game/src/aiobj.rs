//! The race object's (`cAI`) object glue (board row G4k) on the game's bytes: its reset (vf10 0x1108f0), the riders
//! made (vf11 0x110bf8), torn down (vf13 0x110fa0) and set out at the start of a race (vf16 0x111560). Allocation,
//! the career's race roster and virtual calls: every call out goes through [`FeEnv`]. Checked in the function runner
//! (`tests/aiobj.rs`). Row G4k2 adds its constructor 0x114c00, destructor 0x114d30 and the node's default virtuals,
//! G4k3 the race reset (vf14 0x111108) and the restart (vf15 0x1114d0).
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

/// `cPreAI::~cPreAI` 0x114b08 and `cPostAI::~cPostAI` 0x114a68(node, how): the class's table back at +8, then the
/// node's destructor with `how`
pub fn ai_node_destruct(m: &mut Mem, e: &mut dyn FeEnv, node: u32, vt: u32, how: u32) {
    m.w32(node + 8, vt);
    e.call(m, NODE_DTOR, &[node, how]);
}
