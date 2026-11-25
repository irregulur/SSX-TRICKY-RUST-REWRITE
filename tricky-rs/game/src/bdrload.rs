//! The rider skin's banks and shape lists (board row G7r1): `SkinBank_*`, `MdfMemBank_Init`, `MemBank_AlignSize`
//! 0x203e88..0x203fcc, the outfit's shape list `BdrLoad_*` 0x203fd0..0x204594, `SkinTexList_Find` 0x204598 and
//! `BdrLoad_BindTextures` 0x2021f8. On the bytes at their addresses ([`Mem`]); the heaps, files, shapes, the
//! renderer's and texture managers' virtuals and the skin build are calls out ([`TextBoxEnv`]). Checked in the
//! function runner (`tests/bdrload.rs`).
//!
//! And the async rider loader (board row E2f): `cAsyncBdrLoad` 0x201788..0x201e68 and `cBdrLoad::~cBdrLoad`
//! 0x202478, its slots, requests and managers (below `set_mode`); checked in `tests/bdrload_async.rs`.
//!
//! **A bank** (`MdfMemBank`): +0 its memory, +4 the next free place (on a 128-byte boundary), +8 how much is
//! taken, +0xc its size.
//!
//! **A shape list** (an outfit's textures by part): +0 how many names; +4 the names, 5 bytes each (4 characters
//! and a 0); +0x130 each name's texture (60 words, −1 for none).

use crate::pf::Mem;
use crate::textbox::TextBoxEnv;

pub const ALLOC: u32 = 0x2ccf70;
pub const MEM_SET: u32 = 0x2cc808;
pub const MEM_COPY: u32 = 0x2ce114;
pub const FREE: u32 = 0x14a040;
pub const DELETE: u32 = 0x114f80;
pub const STRNCMP: u32 = 0x2fd478;
pub const FILE_LOAD: u32 = 0x2c8a70;
pub const EAMEM_FREE: u32 = 0x2ccfc0;
pub const SHAPE_FIND: u32 = 0x2c2528;
pub const RESET_SKIN: u32 = 0x116a18;
pub const CLEAR_PARTS: u32 = 0x2070e8;
pub const BUILD_SKIN: u32 = 0x201138;
/// The names: the bank's heap name (`MdfMemBank`), the five parts (`head`, `helm`, `boot`, `suit`, `bord`), the
/// textures' buffer (`dummy`) and the textures' name (`plr`)
pub const BANK_NAME: u32 = 0x39e4f0;
pub const PARTS: [u32; 5] = [0x39e500, 0x39e508, 0x39e510, 0x39e518, 0x39e520];
pub const DUMMY_NAME: u32 = 0x39e528;
pub const TEXTURE_NAME: u32 = 0x39e530;
const APP: u32 = 0x337c58;

/// A virtual on the object at `obj` (its vtable at +0x10: the this-shift at `off`, the function at `off` + 4)
fn vcall(m: &mut Mem, env: &mut dyn TextBoxEnv, obj: u32, off: u32, args: &[u32]) -> u32 {
    let vt = m.r32(obj + 0x10);
    let this = obj.wrapping_add(m.r16(vt + off) as i16 as i32 as u32);
    let func = m.r32(vt + off + 4);
    let mut w = vec![this];
    w.extend_from_slice(args);
    env.call(m, func, &w, &[])
}
fn renderer(m: &Mem) -> u32 {
    m.r32(m.r32(APP) + 0x724)
}

/// `SkinBank_Init` 0x203e88(b): no memory; `b`
pub fn bank_init(m: &mut Mem, b: u32) -> u32 {
    m.w32(b, 0);
    b
}

/// `MdfMemBank_Init` 0x203e98(b, size): `size` + 0x80 taken (`MdfMemBank`, filled with 0xdeadc0ed), then aligned
pub fn mem_bank_init(m: &mut Mem, env: &mut dyn TextBoxEnv, b: u32, size: u32) {
    let total = size.wrapping_add(0x80);
    m.w32(b + 0xc, total);
    let p = env.call(m, ALLOC, &[BANK_NAME, total, 0], &[]);
    if p != 0 {
        env.call(m, MEM_SET, &[p, 0xdead_c0ed, total], &[]);
    }
    m.w32(b, p);
    align(m, b);
}

/// `SkinBank_Delete` 0x203f08(b, flags): its memory freed; deleted with flag 1
pub fn bank_delete(m: &mut Mem, env: &mut dyn TextBoxEnv, b: u32, flags: u32) {
    let p = m.r32(b);
    if p != 0 {
        env.call(m, FREE, &[p], &[]);
    }
    if flags & 1 != 0 {
        env.call(m, DELETE, &[b], &[]);
    }
}

/// `SkinBank_Alloc` 0x203f60(b, n): the next free place, moved on (and the taken grown) by `n` rounded down to
/// 128 bytes plus 128 (toward zero for a negative `n`)
pub fn bank_alloc(m: &mut Mem, b: u32, n: u32) -> u32 {
    let s = n as i32;
    let t = if s > -1 { s } else { s.wrapping_add(0x7f) };
    let step = ((t >> 7) << 7).wrapping_add(0x80) as u32;
    let at = m.r32(b + 4);
    m.w32(b + 4, at.wrapping_add(step));
    m.w32(b + 8, m.r32(b + 8).wrapping_add(step));
    at
}

/// `MemBank_AlignSize` 0x203fa0(b): the next free place the memory rounded up to 128 bytes, the taken what that
/// skips
pub fn align(m: &mut Mem, b: u32) {
    let p = m.r32(b) as i32;
    let t = if p.wrapping_add(0x7f) > -1 { p.wrapping_add(0x7f) } else { p.wrapping_add(0xfe) };
    let at = ((t >> 7) << 7) as u32;
    m.w32(b + 4, at);
    m.w32(b + 8, at.wrapping_sub(p as u32));
}

/// `BdrLoad_ConstructShapeList` 0x203fd0(l): cleared; `l`
pub fn list_construct(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32) -> u32 {
    list_clear(m, env, l);
    l
}
/// `BdrLoad_DestructShapeList` 0x203ff8(l, flags): cleared; deleted with flag 1
pub fn list_destruct(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32, flags: u32) {
    list_clear(m, env, l);
    if flags & 1 != 0 {
        env.call(m, DELETE, &[l], &[]);
    }
}
/// `BdrLoad_ClearShapeList` 0x204048(l): no names, every texture −1
pub fn list_clear(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32) {
    m.w32(l, 0);
    env.call(m, MEM_SET, &[l + 0x130, 0xffff_ffff, 0xf0], &[]);
}

/// `BdrLoad_LoadOutfitParts` 0x204068(l): the five parts named (`head`, `helm`, `boot`, `suit`, `bord`) and each
/// given a texture to draw into (the renderer's +0x158: a 64 KB `dummy` buffer, `plr`, 128 × 128 (the suit's
/// 256 × 256), 3, 4, 1; the buffer and 0 on the stack), the buffer freed after
pub fn load_outfit_parts(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32) {
    for name in PARTS {
        find_part_name(m, env, l, name);
    }
    let buf = env.call(m, ALLOC, &[DUMMY_NAME, 0x10000, 0x10], &[]);
    if buf != 0 {
        env.call(m, MEM_SET, &[buf, 0xdead_c0ed, 0x10000], &[]);
    }
    for (k, size) in [0x80u32, 0x80, 0x80, 0x100, 0x80].into_iter().enumerate() {
        let r = renderer(m);
        let t = vcall(m, env, r, 0x158, &[buf, TEXTURE_NAME, size, size, 3, 4, 1, buf, 0]);
        m.w32(l + 0x130 + 4 * k as u32, t);
    }
    if buf != 0 {
        env.call(m, FREE, &[buf], &[]);
    }
}

/// `BdrLoad_RegisterShapeFile` 0x2042a0(l, file): the file loaded (`File_Load`, 0x10), its shapes registered, it
/// freed
pub fn register_shape_file(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32, file: u32) {
    let data = env.call(m, FILE_LOAD, &[file, 0x10], &[]);
    register_shapes(m, env, l, data);
    env.call(m, EAMEM_FREE, &[data], &[]);
}

/// `BdrLoad_RegisterShapes` 0x2042e8(l, shapes): each name's shape in the file (`Shape_FindByName`), when there is
/// one, made a texture (the renderer's +0x150: the shape, `plr`, 0)
pub fn register_shapes(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32, shapes: u32) {
    let mut i = 0u32;
    while (i as i32) < m.r32(l) as i32 {
        let s = env.call(m, SHAPE_FIND, &[shapes, l + 4 + 5 * i], &[]);
        if s != 0 {
            let r = renderer(m);
            let t = vcall(m, env, r, 0x150, &[s, TEXTURE_NAME, 0]);
            m.w32(l + 0x130 + 4 * i, t);
        }
        i += 1;
    }
}

/// `BdrLoad_RebindShapeFile` 0x2043b0(l, file): as `register_shape_file`, rebinding
pub fn rebind_shape_file(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32, file: u32) {
    let data = env.call(m, FILE_LOAD, &[file, 0x10], &[]);
    rebind_shapes(m, env, l, data);
    env.call(m, EAMEM_FREE, &[data], &[]);
}

/// `BdrLoad_RebindShapes` 0x2043f8(l, shapes): each name's texture (if any) given the file's shape of that name
/// (if any; the renderer's +0x160: the texture, the shape). The count is read once.
pub fn rebind_shapes(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32, shapes: u32) {
    let n = m.r32(l) as i32;
    let mut i = 0i32;
    while i < n {
        let t = shape_slot(m, l, i as u32);
        let s = env.call(m, SHAPE_FIND, &[shapes, l + 4 + 5 * i as u32], &[]);
        if s != 0 && (t as i32) >= 0 {
            let r = renderer(m);
            vcall(m, env, r, 0x160, &[t, s]);
        }
        i += 1;
    }
}

/// `BdrLoad_FindPartName` 0x2044c0(l, name): the name's place (its first 4 characters, `strncmp`); a new one
/// added at the end (4 copied and a 0); −1 for an empty name
pub fn find_part_name(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32, name: u32) -> u32 {
    if m.r8(name) == 0 {
        return 0xffff_ffff;
    }
    let mut i = 0i32;
    while i < m.r32(l) as i32 {
        if env.call(m, STRNCMP, &[l + 4 + 5 * i as u32, name, 4], &[]) == 0 {
            return i as u32;
        }
        i += 1;
    }
    let n = m.r32(l);
    env.call(m, MEM_COPY, &[l.wrapping_add(n.wrapping_mul(5)).wrapping_add(4), name, 4], &[]);
    let n = m.r32(l);
    m.w8(l.wrapping_add(n.wrapping_mul(5)).wrapping_add(8), 0);
    let n = m.r32(l);
    m.w32(l, n.wrapping_add(1));
    n
}

/// `BdrLoad_GetShapeSlot` 0x204588(l, i): name `i`'s texture
pub fn shape_slot(m: &Mem, l: u32, i: u32) -> u32 {
    m.r32(l.wrapping_add(i << 2) + 0x130)
}

/// `SkinTexList_Find` 0x204598(l, name): the texture of the name (its first 4 characters); −1 for none or an
/// empty name
pub fn find_texture(m: &mut Mem, env: &mut dyn TextBoxEnv, l: u32, name: u32) -> u32 {
    if m.r8(name) == 0 {
        return 0xffff_ffff;
    }
    let mut i = 0i32;
    while i < m.r32(l) as i32 {
        if env.call(m, STRNCMP, &[l + 4 + 5 * i as u32, name, 4], &[]) == 0 {
            return shape_slot(m, l, i as u32);
        }
        i += 1;
    }
    0xffff_ffff
}

/// `BdrLoad_BindTextures` 0x2021f8(load, n): rider `n`'s textures (its record, 0x50 bytes from +0x14) bound:
/// - its three ids (+0x10..+0x18) through the loader's texture manager +8 (the third through +0xc when there
///   is one), the manager's +0x68 (the id, 0x10), into +0x1c..+0x24 (0 for an id below 0);
/// - its five more (+0x28..+0x38) through +0x10 into +0x3c..+0x4c;
/// - with the rider's skin (its +4's +0x79c) wanting it (+0x46b0): the skin reset (`Boarder_ResetSkin`, flag
///   bit 13 off) for flag bit 8, else its parts cleared (`RiderSkin_ClearParts`) for bit 9;
///
/// then built (`BdrLoad_BuildSkin` 0x201138: the loader, the record).
pub fn bind_textures(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, n: u32) {
    let rec = load.wrapping_add(n.wrapping_mul(0x50)).wrapping_add(0x14);
    let rider = m.r32(rec + 4);
    let skin = rider.wrapping_add(0x79c);
    for k in 0..3u32 {
        let id = m.r32(rec + 0x10 + 4 * k);
        if (id as i32) < 0 {
            m.w32(rec + 0x1c + 4 * k, 0);
            continue;
        }
        let third = m.r32(load + 0xc);
        if third != 0 && k == 2 {
            let t = vcall(m, env, third, 0x68, &[m.r32(rec + 0x18), 0x10]);
            m.w32(rec + 0x24, t);
        } else {
            let tm = m.r32(load + 8);
            let t = vcall(m, env, tm, 0x68, &[m.r32(rec + 0x10 + 4 * k), 0x10]);
            m.w32(rec + 0x1c + 4 * k, t);
        }
    }
    for k in 0..5u32 {
        let id = m.r32(rec + 0x28 + 4 * k);
        let t = if (id as i32) < 0 {
            0
        } else {
            let tm = m.r32(load + 0x10);
            vcall(m, env, tm, 0x68, &[id, 0x10])
        };
        m.w32(rec + 0x3c + 4 * k, t);
    }
    if m.r32(skin + 0x3f14) != 0 {
        let flags = m.r32(rec + 0xc);
        if (flags >> 8) & 1 != 0 {
            env.call(m, RESET_SKIN, &[rider, ((flags as i32 >> 13) as u32 ^ 1) & 1], &[]);
        } else if (flags >> 9) & 1 != 0 {
            env.call(m, CLEAR_PARTS, &[skin], &[]);
        }
    }
    env.call(m, BUILD_SKIN, &[load, rec], &[]);
}

// The async rider loader (board row E2f): `cAsyncBdrLoad` 0x201788..0x202478. A loader: +0 its mode, +4 its
// vtable, +8 / +0xc / +0x10 the managers (the rider meshes', the board mesh's (none: the rider meshes'), the
// textures'), +0x14 twelve slots of 0x50 bytes. A slot: +0 its state (0 free, 1 loading, 2 loaded), +4 the rider,
// +8 its owner, +0xc the request word (byte 0 and the part bits below), +0x10..+0x18 the body, head and board
// meshes' ids, +0x28..+0x38 the head, helm, suit, boot and board textures' ids (−1 for none); `bind_textures`
// turns the ids into the data at +0x1c.. and +0x3c...

/// The request word's bits: rider meshes (body, head), board mesh, board texture, outfit textures (suit, boot),
/// head textures (head, helm)
pub const WANT_MESHES: u32 = 1 << 8;
pub const WANT_BOARD: u32 = 1 << 9;
pub const WANT_BOARD_TEXTURE: u32 = 1 << 10;
pub const WANT_OUTFIT: u32 = 1 << 11;
pub const WANT_HEAD: u32 = 1 << 12;
/// The file names' tables (by the rider's choices, its +0x464 record): body .mpf (+0x50), head .mpf (+0x58),
/// the board .mpf (one name), head and helm .ssh in pairs (+0x54), suit and boot .ssh in pairs (+0x4c), board
/// .ssh (+0x48)
pub const BODY_MPF: u32 = 0x3419c8;
pub const HEAD_MPF: u32 = 0x341a00;
pub const BOARD_MPF: u32 = 0x341f98;
pub const HEAD_SSH: u32 = 0x341ce8;
pub const OUTFIT_SSH: u32 = 0x341a38;
pub const BORD_SSH: u32 = 0x341d50;
/// `cAsyncBdrLoad`'s and `cBdrLoad`'s vtables
pub const VTABLE: u32 = 0x39de40;
pub const BASE_VTABLE: u32 = 0x39e280;
pub const SLOTS: u32 = 12;
pub const SLOT: u32 = 0x50;

fn slot(load: u32, n: u32) -> u32 {
    load.wrapping_add(n.wrapping_mul(SLOT)).wrapping_add(0x14)
}
/// The name in `table` at the rider's choice `+at` (`stride` 4, or 8 for the pairs)
fn file_name(m: &Mem, r: u32, at: u32, table: u32, stride: u32) -> u32 {
    m.r32(table.wrapping_add(m.r32(r + at).wrapping_mul(stride)))
}

/// `AsyncBdrLoad_SetMode` 0x201788(p, v): *p = v
pub fn set_mode(m: &mut Mem, p: u32, v: u32) {
    m.w32(p, v);
}

/// `BdrLoad_RequestRider` 0x201790(load, rider, owner, word): the rider loaded at once (`sp` the stack pointer
/// it runs on: the caller's less 0xf0). A slot is built on the stack (state 2, the rider, the owner, the word,
/// copied in by `EAMem_Copy` first), the files the word asks for loaded into it (`File_Load`, 0x10; 0 for the
/// rest) in the order body, head, board .mpf, head, helm, suit, boot, board .ssh, the skin built from it
/// (`BdrLoad_BuildSkin`: the loader, the slot) and every file it has freed after.
pub fn request_rider(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, rider: u32, owner: u32, word: u32, sp: u32) {
    let r = m.r32(rider + 0x464);
    m.w32(sp + 0x50, word);
    env.call(m, MEM_COPY, &[sp | 0xc, sp + 0x50, 4], &[]);
    for k in (0..3).rev() {
        m.w32(sp + 0x1c + 4 * k, 0);
    }
    for k in (0..5).rev() {
        m.w32(sp + 0x3c + 4 * k, 0);
    }
    let load_file = |m: &mut Mem, env: &mut dyn TextBoxEnv, name: u32| env.call(m, FILE_LOAD, &[name, 0x10], &[]);
    if m.r32(sp + 0x50) & WANT_MESHES != 0 {
        let f = load_file(m, env, file_name(m, r, 0x50, BODY_MPF, 4));
        m.w32(sp + 0x1c, f);
        let f = load_file(m, env, file_name(m, r, 0x58, HEAD_MPF, 4));
        m.w32(sp + 0x20, f);
    }
    if m.r32(sp + 0x50) & WANT_BOARD != 0 {
        let f = load_file(m, env, m.r32(BOARD_MPF));
        m.w32(sp + 0x24, f);
    }
    if m.r32(sp + 0x50) & WANT_HEAD != 0 {
        let f = load_file(m, env, file_name(m, r, 0x54, HEAD_SSH, 8));
        m.w32(sp + 0x3c, f);
        let f = load_file(m, env, file_name(m, r, 0x54, HEAD_SSH + 4, 8));
        m.w32(sp + 0x40, f);
    }
    if m.r32(sp + 0x50) & WANT_OUTFIT != 0 {
        let f = load_file(m, env, file_name(m, r, 0x4c, OUTFIT_SSH, 8));
        m.w32(sp + 0x44, f);
        let f = load_file(m, env, file_name(m, r, 0x4c, OUTFIT_SSH + 4, 8));
        m.w32(sp + 0x48, f);
    }
    if m.r32(sp + 0x50) & WANT_BOARD_TEXTURE != 0 {
        let f = load_file(m, env, file_name(m, r, 0x48, BORD_SSH, 4));
        m.w32(sp + 0x4c, f);
    }
    m.w32(sp + 4, rider);
    m.w32(sp, 2);
    m.w32(sp + 8, owner);
    m.w32(sp + 0xc, m.r32(sp + 0x50));
    env.call(m, BUILD_SKIN, &[load, sp], &[]);
    for k in 0..3 {
        let f = m.r32(sp + 0x1c + 4 * k);
        if f != 0 {
            env.call(m, EAMEM_FREE, &[f], &[]);
        }
    }
    for k in 0..5 {
        let f = m.r32(sp + 0x3c + 4 * k);
        if f != 0 {
            env.call(m, EAMEM_FREE, &[f], &[]);
        }
    }
}

/// `BdrLoad_FERequest` 0x201a38(p): byte 0 = 1, every part asked for (bits 8..12), bit 13 off
pub fn fe_request(m: &mut Mem, p: u32) {
    m.w8(p, 1);
    m.w32(p, (m.r32(p) | 0x1f00) & !0x2000);
}
/// `BdrLoad_DefaultRequest` 0x201a98(p): as `fe_request` with byte 0 = 0xf
pub fn default_request(m: &mut Mem, p: u32) {
    m.w8(p, 0xf);
    m.w32(p, (m.r32(p) | 0x1f00) & !0x2000);
}
/// `BdrLoad_FEBoardRequest` 0x201ad8(p): byte 0 = 1, the board's mesh and texture asked for (bits 9, 10), the
/// rider's not (8, 11, 12), bit 13 on
pub fn fe_board_request(m: &mut Mem, p: u32) {
    m.w8(p, 1);
    let v = m.r32(p) & !0x100;
    m.w32(p, ((v | 0x600) & !0x800 & !0x1000) | 0x2000);
}

/// `AsyncBdrLoad_Init` 0x201b50(load): mode 0, the vtable, every slot free (the last first); `load`
pub fn init(m: &mut Mem, load: u32) -> u32 {
    m.w32(load, 0);
    m.w32(load + 4, VTABLE);
    for n in (0..SLOTS).rev() {
        m.w32(slot(load, n), 0);
    }
    load
}

/// `cAsyncBdrLoad::~cAsyncBdrLoad` 0x201b90(load, flags): the vtable put back, every slot in use freed
/// (`free_slot`), then `cBdrLoad`'s
pub fn destruct(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, flags: u32) {
    m.w32(load + 4, VTABLE);
    for n in 0..SLOTS {
        if m.r32(slot(load, n)) != 0 {
            free_slot(m, env, load, n);
        }
    }
    base_destruct(m, env, load, flags);
}

/// `cBdrLoad::~cBdrLoad` 0x202478(load, flags): its vtable put back; deleted with flag 1 (`Mem_Delete`)
pub fn base_destruct(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, flags: u32) {
    m.w32(load + 4, BASE_VTABLE);
    if flags & 1 != 0 {
        env.call(m, DELETE, &[load], &[]);
    }
}

/// `AsyncBdrLoad_SetManagers` 0x201c10(load, meshes, board, textures): +8, +0xc, +0x10 (in the order +0x10,
/// +8, +0xc)
pub fn set_managers(m: &mut Mem, load: u32, meshes: u32, board: u32, textures: u32) {
    m.w32(load + 0x10, textures);
    m.w32(load + 8, meshes);
    m.w32(load + 0xc, board);
}

/// The manager a slot's mesh `k` (0 body, 1 head, 2 board) is loaded by and its id: the board's through the
/// board manager when there is one (its id read again at +0x18)
fn mesh_call(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, rec: u32, k: u32, off: u32) -> u32 {
    let board = m.r32(load + 0xc);
    if board != 0 && k == 2 {
        vcall(m, env, board, off, &[m.r32(rec + 0x18)])
    } else {
        let mgr = m.r32(load + 8);
        vcall(m, env, mgr, off, &[m.r32(rec + 0x10 + 4 * k)])
    }
}

/// `AsyncBdrLoad_FreeSlot` 0x201c20(load, n): slot `n`'s ids (those not below 0) let go of through their
/// managers' +0x58 (meshes, then textures), the slot free
pub fn free_slot(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, n: u32) {
    let rec = slot(load, n);
    for k in 0..3 {
        if (m.r32(rec + 0x10 + 4 * k) as i32) >= 0 {
            mesh_call(m, env, load, rec, k, 0x58);
        }
    }
    for k in 0..5 {
        let id = m.r32(rec + 0x28 + 4 * k);
        if (id as i32) >= 0 {
            let tm = m.r32(load + 0x10);
            vcall(m, env, tm, 0x58, &[id]);
        }
    }
    m.w32(rec, 0);
}

/// `AsyncBdrLoad_IsSlotReady` 0x201d30(load, n): 1 for a slot not loading; a loading one asks its managers'
/// +0x60 for each id not below 0 (meshes, then textures) and answers 0 at the first not ready, else it is
/// loaded (state 2) and answers 1
pub fn is_slot_ready(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, n: u32) -> u32 {
    let rec = slot(load, n);
    if m.r32(rec) != 1 {
        return 1;
    }
    for k in 0..3 {
        if (m.r32(rec + 0x10 + 4 * k) as i32) >= 0 && mesh_call(m, env, load, rec, k, 0x60) == 0 {
            return 0;
        }
    }
    for k in 0..5 {
        let id = m.r32(rec + 0x28 + 4 * k);
        if (id as i32) >= 0 {
            let tm = m.r32(load + 0x10);
            if vcall(m, env, tm, 0x60, &[id]) == 0 {
                return 0;
            }
        }
    }
    m.w32(rec, 2);
    1
}

/// `AsyncBdrLoad_Request` 0x201e68(load, rider, owner, word): the first free slot (−1 for none) made loading:
/// the word (kept on the stack, `sp` the stack pointer it runs on: the caller's less 0xb0, and copied into the
/// slot by `EAMem_Copy` first), the rider, the owner, every id −1; then the files the word asks for started
/// through the managers' +0x80 (the name, 0x10, 0, 0) in the order body, head, board .mpf (the board manager's
/// when there is one), head, helm, suit, boot, board .ssh (the textures'). The slot's number.
pub fn request(m: &mut Mem, env: &mut dyn TextBoxEnv, load: u32, rider: u32, owner: u32, word: u32, sp: u32) -> u32 {
    let r = m.r32(rider + 0x464);
    m.w32(sp, word);
    let mut n = 0u32;
    let rec = loop {
        let rec = slot(load, n);
        if m.r32(rec) == 0 {
            break rec;
        }
        n += 1;
        if n >= SLOTS {
            return 0xffff_ffff;
        }
    };
    env.call(m, MEM_COPY, &[rec + 0xc, sp, 4], &[]);
    let w = m.r32(sp);
    m.w32(rec, 1);
    m.w32(rec + 4, rider);
    m.w32(rec + 8, owner);
    m.w32(rec + 0xc, w);
    for k in (0..3).rev() {
        m.w32(rec + 0x10 + 4 * k, 0xffff_ffff);
    }
    for k in (0..5).rev() {
        m.w32(rec + 0x28 + 4 * k, 0xffff_ffff);
    }
    let start = |m: &mut Mem, env: &mut dyn TextBoxEnv, mgr: u32, name: u32| vcall(m, env, mgr, 0x80, &[name, 0x10, 0, 0]);
    if w & WANT_MESHES != 0 {
        let id = start(m, env, m.r32(load + 8), file_name(m, r, 0x50, BODY_MPF, 4));
        m.w32(rec + 0x10, id);
        let id = start(m, env, m.r32(load + 8), file_name(m, r, 0x58, HEAD_MPF, 4));
        m.w32(rec + 0x14, id);
    }
    if m.r32(sp) & WANT_BOARD != 0 {
        let board = m.r32(load + 0xc);
        let mgr = if board != 0 { board } else { m.r32(load + 8) };
        let id = start(m, env, mgr, m.r32(BOARD_MPF));
        m.w32(rec + 0x18, id);
    }
    if m.r32(sp) & WANT_HEAD != 0 {
        let id = start(m, env, m.r32(load + 0x10), file_name(m, r, 0x54, HEAD_SSH, 8));
        m.w32(rec + 0x28, id);
        let id = start(m, env, m.r32(load + 0x10), file_name(m, r, 0x54, HEAD_SSH + 4, 8));
        m.w32(rec + 0x2c, id);
    }
    if m.r32(sp) & WANT_OUTFIT != 0 {
        let id = start(m, env, m.r32(load + 0x10), file_name(m, r, 0x4c, OUTFIT_SSH, 8));
        m.w32(rec + 0x30, id);
        let id = start(m, env, m.r32(load + 0x10), file_name(m, r, 0x4c, OUTFIT_SSH + 4, 8));
        m.w32(rec + 0x34, id);
    }
    if m.r32(sp) & WANT_BOARD_TEXTURE != 0 {
        let id = start(m, env, m.r32(load + 0x10), file_name(m, r, 0x48, BORD_SSH, 4));
        m.w32(rec + 0x38, id);
    }
    n
}
