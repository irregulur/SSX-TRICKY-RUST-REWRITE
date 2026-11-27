//! The sprite sets' loaders (board row G8z) on the game's bytes: a sprite set's `.ssh` file loaded (at once, or
//! started and finished by `SpriteSet_PollLoad`), its shapes made textures by the renderer and the set's records
//! pointed at them.
//!
//! A set: +0 the kind of binder the pending load ends with, +4 the async file (0: none), +8 its records (0x1c
//! bytes: +0 the texture, then six floats), +0xeb0 a texture, +0x15e8.. up to seven textures, +0x1610 a pointer to
//! another run of records. The files: 8-byte entries (path, flags) at 0x33f3b8, picked by number.
//!
//! The binders find shapes by name (`Shape_FindByName`, run here) and call the renderer's virtuals: +0x150 makes a
//! texture of a shape (shape, name, 0), +0x158 an empty one, +0x160 loads a shape into an existing texture. Calls
//! out go through [`SpriteEnv`]. Proven from the disassembly of `SLUS_203.26` (read by hand: Ghidra was not
//! running).

use crate::pf::Mem;
use crate::{eamem, hudrects, shapelib};

/// The calls out
pub trait SpriteEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
    /// `AsyncFile_Finish` 0x2c6160(handle, &file, 0): the loaded file
    fn async_finish(&mut self, m: &mut Mem, handle: u32) -> u32;
}

pub const FILE_LOAD: u32 = 0x2c8a70;
pub const ASYNC_LOAD: u32 = 0x2c5e20;
pub const ASYNC_IS_DONE: u32 = 0x2c6108;
pub const ASYNC_FINISH: u32 = 0x2c6160;
pub const EAMEM_ALLOC: u32 = 0x2ccf70;
pub const EAMEM_FREE: u32 = 0x2ccfc0;
pub const MEM_FREE: u32 = 0x14a040;
/// `gApp`, whose +0x724 is the renderer
pub const GAPP: u32 = 0x337c58;
/// The files: (path, flags) by number
pub const FILES: u32 = 0x33f3b8;
/// `HUD_LoadIconShapes`' seven shapes ("gene", "trik", "cons", "fe_1", "fe_2", "ps2c", "dvd1")
pub const ICON_NAMES: u32 = 0x39a4c8;
/// `SpriteSet_BindHudGameTex`'s three ("map1", "map4", "hud1")
pub const HUD_NAMES: u32 = 0x39a500;
/// `HUD_LoadLoadingIcons`' three ("loa1", "loa2", "lcon")
pub const LOADING_NAMES: u32 = 0x39a518;
/// `SpriteSet_RegisterNamedShapes`' 30 (record, name) pairs: "tb00".."tb29" on records 0..4 in turn
pub const NAMED: u32 = 0x340a20;
/// `SpriteSet_RegisterRiderProfileShapes`' 4 pairs ("rpb1".."rpb4" on 124..127)
pub const RIDER_PROFILE: u32 = 0x39a550;
/// `SpriteSet_RegisterLoadScreenShapes`' 19 pairs ("dved".."ddem" on 104..122)
pub const LOAD_SCREEN: u32 = 0x39a608;
/// `HUD_CreateDummyTextures`' names: the buffer's ("dummy") and the textures' ("plr")
pub const DUMMY_NAME: u32 = 0x39a6a0;
pub const DUMMY_TEXTURE: u32 = 0x39a6a8;

/// The texture fields
const FIELDS: u32 = 0x15e8;

/// `HUD_LoadIconShapes`' records: each one's texture field (0..6 from +0x15e8), records 0..103
#[rustfmt::skip]
pub const ICON_RECTS: [u8; 104] = [
    0, 1, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    4, 4, 3, 3, 3, 4, 3, 4, 4, 6, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 5, 5, 5, 5, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
];

/// `HUD_LoadLoadingIcons`' records 0..27
#[rustfmt::skip]
pub const LOADING_RECTS: [u8; 28] = [0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0];

/// The renderer's virtual at `off` (adjust) / `off + 4` (function), on its table at +0x10
fn renderer(m: &mut Mem, e: &mut dyn SpriteEnv, off: u32, rest: &[u32]) -> u32 {
    let r = m.r32(m.r32(GAPP) + 0x724);
    let vt = m.r32(r + 0x10);
    let me = r.wrapping_add(m.r16(vt + off) as i16 as i32 as u32);
    let mut a = vec![me];
    a.extend_from_slice(rest);
    e.call(m, m.r32(vt + off + 4), &a)
}

fn record(set: u32, k: u32) -> u32 {
    set.wrapping_add(8).wrapping_add(k.wrapping_mul(0x1c))
}

/// Shapes `names[k]` (8 bytes apart) made textures into the fields from +0x15e8 (a missing shape keeps its word),
/// then each record's texture from its field
fn bind(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32, names: u32, n: u32, rects: &[u8]) {
    for k in 0..n {
        let name = names + 8 * k;
        let shape = shapelib::find_by_name(m, file, name);
        if shape != 0 {
            let t = renderer(m, e, 0x150, &[shape, name, 0]);
            m.w32(set + FIELDS + 4 * k, t);
        }
    }
    for (k, f) in rects.iter().enumerate() {
        let t = m.r32(set + FIELDS + 4 * *f as u32);
        m.w32(record(set, k as u32), t);
    }
}

/// `HUD_LoadIconShapes` 0x1f8c98(set, file)
pub fn icon_shapes(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32) {
    bind(m, e, set, file, ICON_NAMES, 7, &ICON_RECTS);
}

/// `HUD_LoadLoadingIcons` 0x1f93c8(set, file)
pub fn loading_icons(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32) {
    bind(m, e, set, file, LOADING_NAMES, 3, &LOADING_RECTS);
}

/// `SpriteSet_BindHudGameTex` 0x1f9080(set, file) ([`hudrects::bind_hud_game_tex`])
pub fn hud_game(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32) {
    let mut tex = [None; 3];
    for (k, t) in tex.iter_mut().enumerate() {
        let name = HUD_NAMES + 8 * k as u32;
        let shape = shapelib::find_by_name(m, file, name);
        if shape != 0 {
            *t = Some(renderer(m, e, 0x150, &[shape, name, 0]));
        }
    }
    hudrects::bind_hud_game_tex(m, set, &mut |k| tex[k]);
}

/// `SpriteSet_RegisterNamedShapes` 0x1f9550(set, file): each of the 30 (record, name) pairs' shape loaded into
/// that record's texture of the run at +0x1610
pub fn named_shapes(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32) {
    for i in 0..30 {
        let (k, name) = (m.r32(NAMED + 8 * i), m.r32(NAMED + 8 * i + 4));
        let shape = shapelib::find_by_name(m, file, name);
        if shape != 0 {
            let t = m.r32(k.wrapping_mul(0x1c).wrapping_add(m.r32(set + 0x1610)));
            renderer(m, e, 0x160, &[t, shape]);
        }
    }
}

/// `SpriteSet_RegisterFirstShape` 0x1f9608(set, file): the file's first shape loaded into the texture at +0xeb0
pub fn first_shape(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32) {
    let shape = shapelib::get_entry(m, file, 0);
    if shape != 0 {
        renderer(m, e, 0x160, &[m.r32(set + 0xeb0), shape]);
    }
}

/// The (record, name) pairs' binder: the shapes found are loaded in turn into the textures of records 124, 125,
/// ... (one more each shape found), and each pair's record takes that texture
fn pairs(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32, table: u32, n: u32) {
    let mut at = 0x7c;
    let mut found = 1;
    for i in 0..n {
        let rec = record(set, at);
        let (k, name) = (m.r32(table + 8 * i), m.r32(table + 8 * i + 4));
        let shape = shapelib::find_by_name(m, file, name);
        if shape != 0 {
            renderer(m, e, 0x160, &[m.r32(rec), shape]);
            at = found + 0x7c;
            found += 1;
            let t = m.r32(rec);
            m.w32(record(set, k), t);
        }
    }
}

/// `SpriteSet_RegisterRiderProfileShapes` 0x1f9660(set, file)
pub fn rider_profile_shapes(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32) {
    pairs(m, e, set, file, RIDER_PROFILE, 4);
}

/// `SpriteSet_RegisterLoadScreenShapes` 0x1f9798(set, file)
pub fn load_screen_shapes(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32, file: u32) {
    pairs(m, e, set, file, LOAD_SCREEN, 19);
}

/// The binder a pending load of `kind` ends with (`SpriteSet_PollLoad`'s choice): 0 icons, 1 the HUD's game
/// textures, 2 loading icons, 3 named shapes, 4 the first shape, 5 rider profile, 6 load screen
fn binder(m: &mut Mem, e: &mut dyn SpriteEnv, kind: u32, set: u32, file: u32) {
    match kind {
        0 => icon_shapes(m, e, set, file),
        1 => hud_game(m, e, set, file),
        2 => loading_icons(m, e, set, file),
        3 => named_shapes(m, e, set, file),
        4 => first_shape(m, e, set, file),
        5 => rider_profile_shapes(m, e, set, file),
        6 => load_screen_shapes(m, e, set, file),
        _ => {}
    }
}

/// The loaders: file `n` of [`FILES`] loaded at once (`File_Load`), bound and given back; or with `wait` set, its
/// load started (`AsyncFile_Load`) and the set marked with `kind` for [`poll`]. `HUD_LoadIconPack` 0x1f9970 (kind
/// 0, its at-once load asks flags 0x10 instead of the table's), `OverlayMan_LoadTextures` 0x1f99f8 (1),
/// `LoadScreen_LoadIcons` 0x1f9a80 (2), `SpriteSet_LoadFromTable` 0x1f9b08 (3), `TrickBook_LoadTextures`
/// 0x1f9b90 (4), `RiderProfile_LoadTextures` 0x1f9c18 (5), `LoadScreen_LoadTexturePack` 0x1f9ca0 (6).
pub fn load(m: &mut Mem, e: &mut dyn SpriteEnv, kind: u32, set: u32, n: u32, wait: u32) {
    let entry = FILES.wrapping_add(n << 3);
    if wait != 0 {
        let h = e.call(m, ASYNC_LOAD, &[m.r32(entry), m.r32(entry + 4)]);
        m.w32(set + 4, h);
        m.w32(set, kind);
        return;
    }
    let flags = if kind == 0 { 0x10 } else { m.r32(entry + 4) };
    let file = e.call(m, FILE_LOAD, &[m.r32(entry), flags]);
    binder(m, e, kind, set, file);
    e.call(m, EAMEM_FREE, &[file]);
}

/// `SpriteSet_PollLoad` 0x1f9d28(set): 1 with no load pending; 0 while it is (`AsyncFile_IsDone` not 1); once
/// done, the file finished, bound by the set's kind, given back, nothing pending, 1
pub fn poll(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32) -> u32 {
    let h = m.r32(set + 4);
    if h == 0 {
        return 1;
    }
    if e.call(m, ASYNC_IS_DONE, &[h]) != 1 {
        return 0;
    }
    let file = e.async_finish(m, m.r32(set + 4));
    let kind = m.r32(set);
    binder(m, e, kind, set, file);
    e.call(m, EAMEM_FREE, &[file]);
    m.w32(set + 4, 0);
    1
}

/// `SpriteSet_SetShape` 0x1f9ee8(set, k, rect): record k set from the 0x1c bytes at `rect`
pub fn set_shape(m: &mut Mem, set: u32, k: u32, rect: u32) {
    let to = record(set, k);
    let bytes: Vec<u8> = (0..0x1c).map(|i| m.r8(rect.wrapping_add(i))).collect();
    for (i, b) in bytes.iter().enumerate() {
        m.w8(to.wrapping_add(i as u32), *b);
    }
}

/// `HUD_CreateDummyTextures` 0x1f9f38(set): a 0x4000-byte buffer ("dummy", flags 0x10; filled with 0xdeadc0ed when
/// allocated) made into eleven empty 128 × 128 textures (renderer +0x158: buffer, "plr", 0x80, 0x80, 0, 4, 1, and
/// on the stack the buffer and 0): ten into the first ten records of the run at +0x1610, one into +0xeb0; the
/// buffer given back (`Mem_Free`) when there
pub fn dummy_textures(m: &mut Mem, e: &mut dyn SpriteEnv, set: u32) {
    let buf = e.call(m, EAMEM_ALLOC, &[DUMMY_NAME, 0x4000, 0x10]);
    if buf != 0 {
        eamem::set(m, buf, 0xdead_c0ed, 0x4000);
    }
    let args = [buf, DUMMY_TEXTURE, 0x80, 0x80, 0, 4, 1, buf, 0];
    for i in 0..10 {
        let t = renderer(m, e, 0x158, &args);
        let at = m.r32(set + 0x1610).wrapping_add(0x1c * i);
        m.w32(at, t);
    }
    let t = renderer(m, e, 0x158, &args);
    m.w32(set + 0xeb0, t);
    if buf != 0 {
        e.call(m, MEM_FREE, &[buf]);
    }
}
