//! The in-game HUD/screen layout mockup editor (board row E1m): all 25 `MockupEditor*` / `MockupItem*` /
//! `MockupEditorEntry_*` functions (`0x1d3b88..0x1d4de8`) on the game's bytes (`Mem`), checked bit-for-bit in
//! `tests/game_tail.rs`.
//!
//! Layout:
//! - `MockupEditor`: `+0` current item index, `+4` item count, `+8` array of 20 items (stride `0xd4`, ending at
//!   `+0x1098`), `+0x1098` open flag, `+0x109c` target x, `+0x10a0` target y.
//! - `MockupItem` (`0xd4` bytes): `+0` name (ASCII), `+0x14` kind (1 loc text, 2 literal text, 3 sprite), `+0x18` x,
//!   `+0x1c` y, `+0x20` scale, `+0x24` font, `+0x28` alignment (0 left, 1 right, 2 centre), `+0x2c..+0x3c` 16-byte
//!   colour, `+0x3c` loc id, `+0x40` text (ASCII), `+0xc0` sprite flags, `+0xc4` sprite index, `+0xc8` sx,
//!   `+0xcc` sy, `+0xd0` edit axis (0..=5).

use crate::fe3dstate::GAPP;
use crate::pf::Mem;
use crate::ps2f::F;

pub const FONT_COLOR: u32 = 0x337fe8;
pub const BLINK_COUNTER: u32 = 0x33f010;
pub const MEM_DELETE: u32 = 0x114f80;
pub const SPRITE_DRAW_RECT_SCALED: u32 = 0x14eed0;
pub const FONT_SET_COLOR_V: u32 = 0x14eee8;
pub const INPUT_GET_DEVICE: u32 = 0x17a830;
pub const WSTR_TO_ASCII: u32 = 0x17ccf8;
pub const FONT_PRINTF_SHADOWED: u32 = 0x19d3f0;
pub const FONT_MEASURE_ASCII: u32 = 0x19da30;
pub const SPRITE_SET_GET_RECT: u32 = 0x1f9e50;
pub const LOC_GET_STRING: u32 = 0x2c5160;
pub const STRCPY: u32 = 0x2fd14c;

pub const BANNER_TEXT: u32 = 0x392578;
pub const DEMO_NAME_0: u32 = 0x3925a0;
pub const DEMO_TEXT_0: u32 = 0x3925b0;
pub const DEMO_NAME_1: u32 = 0x3925b8;
pub const DEMO_TEXT_1: u32 = 0x3925c0;
pub const DEMO_NAME_2: u32 = 0x3925d0;
pub const DEMO_NAME_3: u32 = 0x3925e0;

pub const ITEM_STRIDE: u32 = 0xd4;

/// Calls out from `MockupEditor*` (renderer virtual `+0x1d4`, font/sprite/loc/input/strcpy/Mem_Delete)
pub trait MockupEnv {
    fn call(&mut self, m: &mut Mem, func: u32, ints: &[u32], floats: &[f32]) -> u32;
}

fn read16(m: &Mem, addr: u32) -> [u8; 16] {
    std::array::from_fn(|i| m.r8(addr.wrapping_add(i as u32)))
}

fn write16(m: &mut Mem, addr: u32, bytes: &[u8; 16]) {
    for (i, &b) in bytes.iter().enumerate() {
        m.w8(addr.wrapping_add(i as u32), b);
    }
}

/// `MockupEditorEntry_Construct` 0x1d3b88(o): returns `o`
pub fn entry_construct(o: u32) -> u32 {
    o
}

/// `MockupEditorEntry_Destruct` 0x1d3b90(o, how): calls `Mem_Delete(o)` (`0x114f80`) when `how` has bit 0
pub fn entry_destruct(m: &mut Mem, e: &mut dyn MockupEnv, o: u32, how: u32) {
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[o], &[]);
    }
}

/// `MockupItem_Init` 0x1d3bb0(item): resets name/text to empty, scale to 1.0, kind/x/y/font/align/sx/sy/axis to 0,
/// and loc id / sprite index to -1
pub fn item_init(m: &mut Mem, item: u32) {
    m.w32(item + 0x20, 0x3f80_0000);
    m.w8(item, 0);
    m.w32(item + 0x14, 0);
    m.w32(item + 0x18, 0);
    m.w32(item + 0x1c, 0);
    m.w32(item + 0x24, 0);
    m.w32(item + 0x28, 0);
    m.w32(item + 0x3c, 0xffff_ffff);
    m.w32(item + 0xc4, 0xffff_ffff);
    m.w8(item + 0x40, 0);
    m.w32(item + 0xd0, 0);
    m.w32(item + 0xc8, 0);
    m.w32(item + 0xcc, 0);
}

/// `MockupEditor_DrawItemInfo` 0x1d3c00(item, selected; sp on entry): sets renderer state `+0x1d4` (0x13), sets
/// font colour from `item+0x2c`, steps blink counter `0x33f010` (`0..=40`, scaling alpha by `0.4` when `selected`
/// and counter `>= 31`), then draws either the sprite (kind 3) or aligned shadowed text (kind 1 loc / kind 2 ASCII)
pub fn draw_item_info(m: &mut Mem, e: &mut dyn MockupEnv, item: u32, selected: u32, sp: u32) {
    let buf = sp.wrapping_sub(0xe0);
    let ren = m.r32(m.r32(GAPP) + 0x724);
    let vt = m.r32(ren + 0x10);
    let adj = m.r16(vt + 0x1d0) as i16 as u32;
    let f = m.r32(vt + 0x1d4);
    e.call(m, f, &[ren.wrapping_add(adj), 0x13], &[]);

    e.call(m, FONT_SET_COLOR_V, &[FONT_COLOR, item.wrapping_add(0x2c)], &[]);

    let next = (m.r32(BLINK_COUNTER) as i32).wrapping_add(1);
    let cnt = if next < 0x29 { next as u32 } else { 0 };
    m.w32(BLINK_COUNTER, cnt);

    let mut mult = F(1.0);
    if selected != 0 && (m.r32(BLINK_COUNTER) as i32) >= 0x1f {
        mult = F(f32::from_bits(0x3ecc_cccd));
    }
    let alpha = F(f32::from_bits(m.r32(FONT_COLOR))) * mult;
    m.w32(FONT_COLOR, alpha.0.to_bits());

    let kind = m.r32(item + 0x14);
    if kind == 3 {
        let sx = f32::from_bits(m.r32(item + 0xc8));
        let sy = f32::from_bits(m.r32(item + 0xcc));
        let sprites = m.r32(m.r32(GAPP) + 0x728);
        let idx = m.r32(item + 0xc4);
        let rect = e.call(m, SPRITE_SET_GET_RECT, &[sprites, idx], &[]);
        let (dsx, dsy) = if sx == -1.0 || sy == -1.0 { (1.0f32, 1.0f32) } else { (sx, sy) };
        let flags = m.r32(item + 0xc0);
        let x = f32::from_bits(m.r32(item + 0x18));
        let y = f32::from_bits(m.r32(item + 0x1c));
        e.call(m, SPRITE_DRAW_RECT_SCALED, &[rect, flags], &[x, y, dsx, dsy]);
        return;
    }

    m.w8(buf, 0);
    if kind == 1 {
        let loc = m.r32(m.r32(GAPP) + 0x50);
        let id = m.r32(item + 0x3c);
        let wstr = e.call(m, LOC_GET_STRING, &[loc, id], &[]);
        e.call(m, WSTR_TO_ASCII, &[buf, wstr], &[]);
    } else if kind == 2 {
        e.call(m, STRCPY, &[buf, item.wrapping_add(0x40)], &[]);
    }

    let font = m.r32(item + 0x24);
    let scale = f32::from_bits(m.r32(item + 0x20));
    let w_i = e.call(m, FONT_MEASURE_ASCII, &[font, buf, 0], &[scale]) as i32;
    let wf = F(w_i as f32);
    let xoff = match m.r32(item + 0x28) {
        1 => F(-wf.0),
        2 => F(-(wf * F(0.5)).0),
        _ => F(0.0),
    };
    let x = F(f32::from_bits(m.r32(item + 0x18))) + xoff;
    let y = f32::from_bits(m.r32(item + 0x1c));
    let font2 = m.r32(item + 0x24);
    let scale2 = f32::from_bits(m.r32(item + 0x20));
    e.call(m, FONT_PRINTF_SHADOWED, &[font2, buf], &[scale2, x.0, y]);
}

/// `MockupItem_Increase` 0x1d3e40(item): steps the active axis (`item+0xd0`, 0..=5) upward (`x + 1`, `y + 1`,
/// `scale + 0.01`, `sx + 0.02`, `sy + 0.02`, or both `sx, sy + 0.01`)
pub fn item_increase(m: &mut Mem, item: u32) {
    let add = |m: &mut Mem, off: u32, delta: u32| {
        let v = F(f32::from_bits(m.r32(item + off))) + F(f32::from_bits(delta));
        m.w32(item + off, v.0.to_bits());
    };
    match m.r32(item + 0xd0) {
        0 => add(m, 0x18, 0x3f80_0000),
        1 => add(m, 0x1c, 0x3f80_0000),
        2 => add(m, 0x20, 0x3c23_d70a),
        3 => add(m, 0xc8, 0x3ca3_d70a),
        4 => add(m, 0xcc, 0x3ca3_d70a),
        5 => {
            add(m, 0xc8, 0x3c23_d70a);
            add(m, 0xcc, 0x3c23_d70a);
        }
        _ => {}
    }
}

/// `MockupItem_Decrease` 0x1d3f30(item): steps the active axis (`item+0xd0`, 0..=5) downward (`x - 1`, `y - 1`,
/// `scale - 0.01`, `sx - 0.02`, `sy - 0.02`, or both `sx, sy - 0.01`)
pub fn item_decrease(m: &mut Mem, item: u32) {
    let sub = |m: &mut Mem, off: u32, delta: u32| {
        let v = F(f32::from_bits(m.r32(item + off))) - F(f32::from_bits(delta));
        m.w32(item + off, v.0.to_bits());
    };
    match m.r32(item + 0xd0) {
        0 => sub(m, 0x18, 0x3f80_0000),
        1 => sub(m, 0x1c, 0x3f80_0000),
        2 => sub(m, 0x20, 0x3c23_d70a),
        3 => sub(m, 0xc8, 0x3ca3_d70a),
        4 => sub(m, 0xcc, 0x3ca3_d70a),
        5 => {
            sub(m, 0xc8, 0x3c23_d70a);
            sub(m, 0xcc, 0x3c23_d70a);
        }
        _ => {}
    }
}

/// `MockupItem_SetAxis` 0x1d41c0(item, axis): stores `axis` at `item+0xd0`
pub fn item_set_axis(m: &mut Mem, item: u32, axis: u32) {
    m.w32(item + 0xd0, axis);
}

/// `MockupItem_StubA` 0x1d41c8 and `MockupItem_StubB` 0x1d41d0: empty callbacks
pub fn item_stub_a() {}
pub fn item_stub_b() {}

/// `MockupEditor_SetTarget` 0x1d4ce0(ed; x, y): stores `x` at `+0x109c` and `y` at `+0x10a0`
pub fn set_target(m: &mut Mem, ed: u32, x: f32, y: f32) {
    m.w32(ed + 0x10a0, y.to_bits());
    m.w32(ed + 0x109c, x.to_bits());
}

/// `MockupEditor_Open` 0x1d4cf0(ed): sets `+0x1098 = 1`
pub fn open(m: &mut Mem, ed: u32) {
    m.w32(ed + 0x1098, 1);
}

/// `MockupEditor_Close` 0x1d4d00(ed): sets `+0x1098 = 0`
pub fn close(m: &mut Mem, ed: u32) {
    m.w32(ed + 0x1098, 0);
}

/// `MockupEditor_Reset` 0x1d42f0(ed): zeroes current/count and calls [`item_init`] on all 20 embedded items
pub fn reset(m: &mut Mem, ed: u32) {
    m.w32(ed, 0);
    m.w32(ed + 4, 0);
    for i in 0..20u32 {
        item_init(m, ed.wrapping_add(8 + i * ITEM_STRIDE));
    }
}

/// `MockupEditor_Init` 0x1d41d8(ed): constructs the 20 items, sets target `(0, 0)`, closes, resets, and returns `ed`
pub fn init(m: &mut Mem, ed: u32) -> u32 {
    for i in 0..20u32 {
        let _ = entry_construct(ed.wrapping_add(8 + i * ITEM_STRIDE));
    }
    set_target(m, ed, 0.0, 0.0);
    close(m, ed);
    reset(m, ed);
    ed
}

/// `MockupEditor_Delete` 0x1d4258(ed, how): destroys the 20 embedded items in reverse (`how = 0`) and calls
/// `Mem_Delete(ed)` when `how` has bit 0
pub fn delete(m: &mut Mem, e: &mut dyn MockupEnv, ed: u32, how: u32) {
    let start = ed.wrapping_add(8);
    if start != 0 {
        let mut cur = ed.wrapping_add(0x1098);
        while cur != start {
            cur = cur.wrapping_sub(ITEM_STRIDE);
            entry_destruct(m, e, cur, 0);
        }
    }
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[ed], &[]);
    }
}

/// `MockupEditor_Draw` 0x1d4338(ed; sp on entry): when count `> 0` and open (`+0x1098 != 0`), draws the editor
/// banner (`0x392578` at `(18.0, 445.0)`, scale `0.82` with alpha `1.0`) and each item via [`draw_item_info`]
pub fn draw(m: &mut Mem, e: &mut dyn MockupEnv, ed: u32, sp: u32) {
    if (m.r32(ed + 4) as i32) <= 0 || m.r32(ed + 0x1098) == 0 {
        return;
    }
    m.w32(FONT_COLOR, 0x3f80_0000);
    let font = m.r32(m.r32(GAPP) + 0x60);
    e.call(m, FONT_PRINTF_SHADOWED, &[font, BANNER_TEXT], &[f32::from_bits(0x3f51_eb85), 18.0, 445.0]);
    let mut i = 0i32;
    let inner_sp = sp.wrapping_sub(0x40);
    while i < m.r32(ed + 4) as i32 {
        let sel = (m.r32(ed) as i32 == i) as u32;
        draw_item_info(m, e, ed.wrapping_add(8 + (i as u32) * ITEM_STRIDE), sel, inner_sp);
        i += 1;
    }
}

/// `MockupEditor_AddSprite` 0x1d4410(ed, name, sprite_idx, color_ptr, flags; x, y, sx, sy)
#[allow(clippy::too_many_arguments)]
pub fn add_sprite(
    m: &mut Mem,
    e: &mut dyn MockupEnv,
    ed: u32,
    name: u32,
    sprite_idx: u32,
    color_ptr: u32,
    flags: u32,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
) {
    let col = read16(m, color_ptr);
    let base = ed.wrapping_add(m.r32(ed + 4).wrapping_mul(ITEM_STRIDE));
    m.w32(base + 0x1c, 3);
    e.call(m, STRCPY, &[base.wrapping_add(8), name], &[]);
    let n = m.r32(ed + 4);
    let base = ed.wrapping_add(n.wrapping_mul(ITEM_STRIDE));
    m.w32(base + 0xcc, sprite_idx);
    m.w32(base + 0x20, x.to_bits());
    m.w32(base + 0x24, y.to_bits());
    m.w32(base + 0xd0, sx.to_bits());
    m.w32(base + 0xd4, sy.to_bits());
    write16(m, base.wrapping_add(0x34), &col);
    m.w32(base + 0xc8, flags);
    m.w32(ed + 4, n.wrapping_add(1));
}

/// `MockupEditor_AddLocText` 0x1d4538(ed, name, loc_id, font, color_ptr, align; x, y, scale)
#[allow(clippy::too_many_arguments)]
pub fn add_loc_text(
    m: &mut Mem,
    e: &mut dyn MockupEnv,
    ed: u32,
    name: u32,
    loc_id: u32,
    font: u32,
    color_ptr: u32,
    align: u32,
    x: f32,
    y: f32,
    scale: f32,
) {
    let col = read16(m, color_ptr);
    let base = ed.wrapping_add(m.r32(ed + 4).wrapping_mul(ITEM_STRIDE));
    m.w32(base + 0x1c, 1);
    e.call(m, STRCPY, &[base.wrapping_add(8), name], &[]);
    let n = m.r32(ed + 4);
    let base = ed.wrapping_add(n.wrapping_mul(ITEM_STRIDE));
    m.w32(base + 0x44, loc_id);
    m.w32(base + 0x20, x.to_bits());
    m.w32(base + 0x24, y.to_bits());
    m.w32(base + 0x28, scale.to_bits());
    write16(m, base.wrapping_add(0x34), &col);
    m.w32(ed + 4, n.wrapping_add(1));
    m.w32(base + 0x2c, font);
    m.w32(base + 0x30, align);
}

/// `MockupEditor_AddItem` 0x1d4660(ed, name, text, font, color_ptr, align; x, y, scale)
#[allow(clippy::too_many_arguments)]
pub fn add_item(
    m: &mut Mem,
    e: &mut dyn MockupEnv,
    ed: u32,
    name: u32,
    text: u32,
    font: u32,
    color_ptr: u32,
    align: u32,
    x: f32,
    y: f32,
    scale: f32,
) {
    let col = read16(m, color_ptr);
    let base = ed.wrapping_add(m.r32(ed + 4).wrapping_mul(ITEM_STRIDE));
    m.w32(base + 0x1c, 2);
    e.call(m, STRCPY, &[base.wrapping_add(8), name], &[]);
    let base2 = ed.wrapping_add(m.r32(ed + 4).wrapping_mul(ITEM_STRIDE));
    e.call(m, STRCPY, &[base2.wrapping_add(0x48), text], &[]);
    let n = m.r32(ed + 4);
    let base = ed.wrapping_add(n.wrapping_mul(ITEM_STRIDE));
    m.w32(base + 0x20, x.to_bits());
    m.w32(base + 0x24, y.to_bits());
    m.w32(base + 0x28, scale.to_bits());
    write16(m, base.wrapping_add(0x34), &col);
    m.w32(ed + 4, n.wrapping_add(1));
    m.w32(base + 0x2c, font);
    m.w32(base + 0x30, align);
}

/// `MockupEditor_NextItem` 0x1d4790(ed): advances `+0` modulo `+4` (no-op when `+4 == 0`)
pub fn next_item(m: &mut Mem, ed: u32) {
    let count = m.r32(ed + 4) as i32;
    if count == 0 {
        return;
    }
    let next = (m.r32(ed) as i32).wrapping_add(1);
    m.w32(ed, if next < count { next as u32 } else { 0 });
}

/// `MockupEditor_PrevItem` 0x1d47c0(ed): steps `+0` backward modulo `+4` (no-op when `+4 == 0`)
pub fn prev_item(m: &mut Mem, ed: u32) {
    let count = m.r32(ed + 4) as i32;
    if count == 0 {
        return;
    }
    let prev = (m.r32(ed) as i32).wrapping_sub(1);
    m.w32(ed, if prev >= 0 { prev as u32 } else { (count - 1) as u32 });
}

/// `MockupEditor_ForEachA` 0x1d4d08(ed): calls [`item_stub_a`] for each of `ed+4` items
pub fn for_each_a(m: &Mem, ed: u32) {
    let count = m.r32(ed + 4) as i32;
    for _ in 0..count {
        item_stub_a();
    }
}

/// `MockupEditor_ForEachB` 0x1d4d78(ed): calls [`item_stub_b`] for each of `ed+4` items
pub fn for_each_b(m: &Mem, ed: u32) {
    let count = m.r32(ed + 4) as i32;
    for _ in 0..count {
        item_stub_b();
    }
}

/// `MockupEditor_Update` 0x1d47f0(ed): reads pad 0 (`Input_GetDevice(gApp+0x24, 0)`), opens on held `0x400` +
/// newly pressed `0x100`, computes step multiplier (`1` or `3` with `0x200`, `+8` with `0x1000`), moves/scales the
/// selected item via `item_set_axis`/`item_increase`/`item_decrease`, switches items on `0x2000` + `4`/`8`, runs
/// `for_each_a`/`item_stub_a`/`for_each_b` on `0x10`/`0x40`/`0x20`, and closes on `0x80`
pub fn update(m: &mut Mem, e: &mut dyn MockupEnv, ed: u32) {
    let input = m.r32(m.r32(GAPP) + 0x24);
    let dev = e.call(m, INPUT_GET_DEVICE, &[input, 0], &[]);
    let prev = m.r32(dev + 0xc);
    let cur = m.r32(dev + 0x10);
    let pressed = cur & !prev;
    if m.r32(ed + 0x1098) == 0 {
        if (cur & 0x400) == 0 || (pressed & 0x100) == 0 {
            return;
        }
        open(m, ed);
    }
    let mut steps = if (cur & 0x200) != 0 { 3i32 } else { 1 };
    if (cur & 0x1000) != 0 {
        steps += 8;
    }
    let cur_item = |m: &Mem| ed.wrapping_add(8 + m.r32(ed).wrapping_mul(ITEM_STRIDE));
    if (cur & 0x2400) == 0 {
        if (pressed & 2) != 0 {
            item_set_axis(m, cur_item(m), 1);
            for _ in 0..steps {
                item_increase(m, cur_item(m));
            }
        }
        if (pressed & 1) != 0 {
            item_set_axis(m, cur_item(m), 1);
            for _ in 0..steps {
                item_decrease(m, cur_item(m));
            }
        }
        if (pressed & 4) != 0 {
            item_set_axis(m, cur_item(m), 0);
            for _ in 0..steps {
                item_increase(m, cur_item(m));
            }
        }
        if (pressed & 8) != 0 {
            item_set_axis(m, cur_item(m), 0);
            for _ in 0..steps {
                item_decrease(m, cur_item(m));
            }
        }
    }
    if (cur & 0x400) != 0 {
        if (pressed & 2) != 0 {
            item_set_axis(m, cur_item(m), 4);
            for _ in 0..steps {
                item_increase(m, cur_item(m));
            }
        }
        if (pressed & 1) != 0 {
            item_set_axis(m, cur_item(m), 4);
            for _ in 0..steps {
                item_decrease(m, cur_item(m));
            }
        }
        if (pressed & 4) != 0 {
            item_set_axis(m, cur_item(m), 3);
            for _ in 0..steps {
                item_increase(m, cur_item(m));
            }
        }
        if (pressed & 8) != 0 {
            item_set_axis(m, cur_item(m), 3);
            for _ in 0..steps {
                item_decrease(m, cur_item(m));
            }
        }
    }
    if (cur & 0x2000) != 0 {
        if (pressed & 4) != 0 {
            next_item(m, ed);
        }
        if (pressed & 8) != 0 {
            prev_item(m, ed);
        }
    }
    if (pressed & 0x10) != 0 {
        for_each_a(m, ed);
    }
    if (pressed & 0x40) != 0 {
        item_stub_a();
    }
    if (pressed & 0x20) != 0 {
        for_each_b(m, ed);
    }
    if (pressed & 0x80) != 0 {
        close(m, ed);
    }
}

/// `MockupEditor_DrawItems` 0x1d4de8(ed, mode; sp on entry): when `mode == 0`, populates the four demo items
/// (two literal text items, one loc text item `0x1e4`, and one sprite item `0x2e`) using `sp - 0x50` for colour
pub fn draw_items(m: &mut Mem, e: &mut dyn MockupEnv, ed: u32, mode: u32, sp: u32) {
    if mode != 0 {
        return;
    }
    let col = sp.wrapping_sub(0x50);
    let write_col = |m: &mut Mem, c: [u32; 4]| {
        for (i, w) in c.into_iter().enumerate() {
            m.w32(col + 4 * i as u32, w);
        }
    };
    let white = [0x3f80_0000; 4];
    write_col(m, white);
    let font60 = m.r32(m.r32(GAPP) + 0x60);
    add_item(m, e, ed, DEMO_NAME_0, DEMO_TEXT_0, font60, col, 0, 50.0, 50.0, 1.0);

    write_col(m, [0x3f80_0000, 0x3f73_f7cf, 0x3f3a_9fbe, 0x3dd9_1687]);
    let font5c = m.r32(m.r32(GAPP) + 0x5c);
    add_item(m, e, ed, DEMO_NAME_1, DEMO_TEXT_1, font5c, col, 0, 70.0, 150.0, f32::from_bits(0x3fa6_6666));

    write_col(m, white);
    let font60 = m.r32(m.r32(GAPP) + 0x60);
    add_loc_text(m, e, ed, DEMO_NAME_2, 0x1e4, font60, col, 0, 70.0, 200.0, 1.0);

    write_col(m, white);
    let s12 = f32::from_bits(0x3f99_999a);
    add_sprite(m, e, ed, DEMO_NAME_3, 0x2e, col, 0, 30.0, 300.0, s12, s12);
}
