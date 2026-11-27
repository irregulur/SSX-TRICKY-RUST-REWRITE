//! The lesson manager's draw (board row C11d1): `cLessonMan` vf2 0x14bbe8, the pad picture
//! (`LessonOverlay_DrawButtons` 0x14bd98), the page draws (`LessonOverlay_DrawPage` 0x14c098 and the five it picks),
//! `Lesson_DrawMenu` 0x14d5e0, `Lesson_GetString` 0x14ed88 and the trick info's small virtuals 0x1550c8..0x1550f8.
//!
//! `lm` is the `cLessonMan` (its fields: tutorial.md), `fonts` the overlay's font set (+4 the small font, +8 the
//! big one). Every call out goes through [`TextBoxEnv`] (the renderer's virtuals as the function its table holds,
//! `this` adjusted, first). Where the game builds on its stack (strings, colours, the text box), the port takes
//! `sp`, the stack pointer on entry, and lays its buffers out in the original's frame below it (so a caller's `sp`
//! for a page is the page's caller's less that caller's frame). Floats are the PS2's ([`F`]).

use crate::animseq::cvt_s_w;
use crate::pf::Mem;
use crate::ps2f::F;
use crate::textbox::TextBoxEnv;

pub const APP: u32 = 0x337c58;
/// The font colour (`0x337fe8`, four floats) and the scale after it (+0x10)
pub const COLOUR: u32 = 0x337fe8;
/// The menu manager `cMenuManager::IsActive` is asked about
pub const MENU_MANAGER: u32 = 0x337e08;
/// The trick book's career and the rider names (0x38 a rider, +4 the name)
pub const CAREER: u32 = 0x32b390;
pub const RIDER_NAMES: u32 = 0x331ea8;
/// The page arrows' pulse (a scale) and its step
pub const PULSE: u32 = 0x31e068;
pub const PULSE_STEP: u32 = 0x31e06c;
/// The empty string `LessonOverlay_DrawTipPage` compares the trick name with
pub const EMPTY: u32 = 0x36e3e0;

pub const SPRITE_DRAW: u32 = 0x14eed0;
pub const SET_COLOUR: u32 = 0x14eee8;
pub const TEXTBOX_DESTRUCT: u32 = 0x14f390;
pub const TRICK_GET_INFO: u32 = 0x154638;
pub const TRICK_INFO_VF2: u32 = 0x154688;
pub const GAME_TRICK_INFO_VF5: u32 = 0x154b18;
pub const FORMAT_TRICK_NAME: u32 = 0x1551c0;
pub const GET_TRICK_NAME: u32 = 0x157978;
pub const GET_TRICK_BOOK: u32 = 0x163f18;
pub const COUNT_COMPLETE_PAGES: u32 = 0x164060;
pub const TRICKS_PER_PAGE: u32 = 0x164108;
pub const COUNT_DONE_IN_PAGE: u32 = 0x164178;
pub const IS_TRICK_DONE: u32 = 0x164258;
pub const GET_PLAYER_ID: u32 = 0x166580;
pub const WSTR_FORMAT: u32 = 0x17ce38;
pub const MENU_IS_ACTIVE: u32 = 0x1895c8;
pub const TEXTBOX_INIT: u32 = 0x1957b0;
pub const TEXTBOX_DRAW: u32 = 0x1957d8;
pub const TEXTBOX_RESET: u32 = 0x195ee8;
pub const TEXTBOX_SET_FONT: u32 = 0x196028;
pub const TEXTBOX_SET_RECT: u32 = 0x196070;
pub const TEXTBOX_APPEND: u32 = 0x196088;
pub const TEXTBOX_LINE_HEIGHT: u32 = 0x1961e8;
pub const TEXTBOX_SET_SCALE: u32 = 0x1961f0;
pub const TEXTBOX_WRAP: u32 = 0x196218;
pub const FILLED_RECT: u32 = 0x19b108;
pub const PRINTF_SHADOWED: u32 = 0x19d568;
pub const PRINTF_CENTRED: u32 = 0x19d6e0;
pub const PRINT_CENTRED: u32 = 0x19d770;
pub const MEASURE_WIDE: u32 = 0x19dc78;
pub const PRINT_WRAPPED: u32 = 0x1a6d38;
pub const GET_LESSON: u32 = 0x1a7c48;
pub const PAUSE_MENU_VF2: u32 = 0x1a7c58;
pub const BACKDROP: u32 = 0x1a9bb8;
pub const DIALOG_DRAW: u32 = 0x1aa268;
pub const MENU_LIST: u32 = 0x1aa630;
pub const HUD_SPRITE: u32 = 0x1c0068;
pub const SPRITE_RECT: u32 = 0x1f9e50;
pub const FONT_PRINTF_SHADOWED: u32 = 0x19d3f0;
pub const FONT_GLYPH_WIDTH: u32 = 0x19d9e0;
pub const MEM_SET: u32 = 0x2cc808;
pub const LOC_STRING: u32 = 0x2c5160;
pub const SPRINTF: u32 = 0x2fca28;
pub const STRCMP: u32 = 0x2fd008;
pub const STRCPY: u32 = 0x2fd14c;
pub const STRLEN: u32 = 0x2fd260;
pub const MEM_DELETE: u32 = 0x114f80;

/// `cGameTrickInfo`'s table (0x36fa98), written by its constructor and destructor at +0x24
pub const GAME_TRICK_INFO_VT: u32 = 0x36fa98;
/// `cTrickInfo`'s table (0x36fad0), set back by its destructor at +0x24
pub const TRICK_INFO_VT: u32 = 0x36fad0;
/// The trick book table (`DAT_0031e210`): 12 riders × 30 tricks × `0x1c` bytes
pub const TRICK_TABLE: u32 = 0x31e210;
/// `TrickInfo_BuildText`'s 12 button-mask → glyph-char entries (`0x31e090`, 8 bytes each)
pub const TRICK_BUTTONS: u32 = 0x31e090;
/// `cGameTrickInfo`'s 12 icon-glyph records (`0x31e0f0`, `0x18` bytes each: char, 1.0, dx, dy, advance, sprite)
pub const TRICK_ICONS: u32 = 0x31e0f0;
/// Format strings used by `TrickInfo_BuildText`
pub const FMT_INT: u32 = 0x36fa50;
pub const FMT_RAIL: u32 = 0x36fa70;
pub const FMT_SPACE_INT: u32 = 0x36fa78;
pub const FMT_PLUS: u32 = 0x36fa80;
pub const FMT_OR: u32 = 0x36fa88;
pub const FMT_TO: u32 = 0x36fa90;

/// The pad picture's buttons (0x31df88, 16 bytes each): x and y from the picture's corner, the sprite, the
/// buttons' bits that light it
pub const BUTTONS: [(u32, u32, u32, u32); 14] = [
    (0x41e80000, 0x41100000, 0x7f, 0x200),
    (0x43400000, 0x41100000, 0x81, 0x400),
    (0x42080000, 0x00000000, 0x80, 0x1000),
    (0x43390000, 0x00000000, 0x82, 0x2000),
    (0x41b00000, 0x42840000, 0x77, 0x8),
    (0x424c0000, 0x42840000, 0x78, 0x4),
    (0x42180000, 0x42980000, 0x7a, 0x2),
    (0x42100000, 0x42480000, 0x79, 0x1),
    (0x43310000, 0x42820000, 0x86, 0x10),
    (0x43460000, 0x42380000, 0x84, 0x20),
    (0x43580000, 0x42800000, 0x83, 0x40),
    (0x43420000, 0x42a40000, 0x85, 0x800),
    (0x42bc0000, 0x42780000, 0x7b, 0x100),
    (0x430e0000, 0x42780000, 0x7c, 0x80),
];

/// The trick book page's five boxes (0x36e508, 0x36e520, 0x36e538): centre as a share of 640, width, height
const BOX_X: [u32; 5] = [0x3e29fbe7, 0x3eaa7efa, 0x3f000000, 0x3f2a7efa, 0x3f553f7d];
const BOX_W: u32 = 0x42b40000; // 90
const BOX_H: u32 = 0x428c0000; // 70

const GOLD: [u32; 4] = [0x3f800000, 0x3f73f7cf, 0x3f3a9fbe, 0x3dd91687];
const WHITE: [u32; 4] = [0x3f800000; 4];
const GREY: [u32; 4] = [0x3f800000, 0x3f000000, 0x3f000000, 0x3f000000];
const DONE_GOLD: [u32; 4] = [0x3f800000, 0x3f7afb7f, 0x3f48c7e3, 0x3e2ca57a];

fn k(bits: u32) -> F {
    F(f32::from_bits(bits))
}
fn rf(m: &Mem, a: u32) -> F {
    F(f32::from_bits(m.r32(a)))
}
fn wf(m: &mut Mem, a: u32, v: F) {
    m.w32(a, v.0.to_bits());
}
fn call(m: &mut Mem, e: &mut dyn TextBoxEnv, func: u32, args: &[u32], f: &[F]) -> u32 {
    e.call(m, func, args, f)
}
/// A virtual of the renderer (the app's +0x724; its table +0x10: the adjust halfword at `off`, the function at
/// `off + 4`), its table read again for every call as the original does
fn rcall(m: &mut Mem, e: &mut dyn TextBoxEnv, r: u32, off: u32, args: &[u32], f: &[F]) -> u32 {
    let vt = m.r32(r + 0x10);
    let me = r.wrapping_add(m.r16(vt + off) as i16 as u32);
    let func = m.r32(vt + off + 4);
    let mut a = vec![me];
    a.extend_from_slice(args);
    e.call(m, func, &a, f)
}
fn renderer(m: &Mem) -> u32 {
    m.r32(m.r32(APP) + 0x724)
}
fn loc(m: &mut Mem, e: &mut dyn TextBoxEnv, id: u32) -> u32 {
    let l = m.r32(m.r32(APP) + 0x50);
    call(m, e, LOC_STRING, &[l, id], &[])
}
fn colour(m: &mut Mem, e: &mut dyn TextBoxEnv, at: u32, c: [u32; 4]) {
    for (i, v) in c.into_iter().enumerate() {
        m.w32(at + 4 * i as u32, v);
    }
    call(m, e, SET_COLOUR, &[COLOUR, at], &[]);
}
fn set4(m: &mut Mem, at: u32, c: [u32; 4]) {
    for (i, v) in c.into_iter().enumerate() {
        m.w32(at + 4 * i as u32, v);
    }
}
/// The player's pad config: the race's (app+0x730) +0x1c, its +0x8c, that one's +0x870
fn pad_config_owner(m: &Mem) -> u32 {
    m.r32(m.r32(m.r32(m.r32(APP) + 0x730) + 0x1c) + 0x8c)
}
fn lesson_overlay(m: &mut Mem, e: &mut dyn TextBoxEnv) -> u32 {
    let om = m.r32(m.r32(m.r32(APP) + 0x730) + 0x38);
    call(m, e, GET_LESSON, &[om], &[])
}

/// `cLessonMan::vf2` 0x14bbe8(lm): the lesson's 2D pass: the renderer's state kept (+0x1d8), the 2D setup
/// (+0x88, the view +0xb0 over 640×480 and depth 0..1000, +0x108, +0x1e0 with 0), the colour's scale 0.15, +0x1c0 and
/// +0x1c8 (0.08, 2, 2); the pad picture in states 5 and 7; +0x90, +0x1c8 (0.08, 1, 2), +0x1c0, and the state back
/// (+0x1d0)
pub fn draw(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32) {
    let r = renderer(m);
    let kept = rcall(m, e, r, 0x1d8, &[], &[]);
    rcall(m, e, r, 0x88, &[], &[]);
    let z = F(0.0);
    rcall(m, e, r, 0xb0, &[], &[z, z, k(0x44200000), k(0x43f00000), z, k(0x447a0000)]);
    rcall(m, e, r, 0x108, &[], &[]);
    rcall(m, e, r, 0x1e0, &[0], &[]);
    m.w32(COLOUR + 0x10, 0x3e19999a);
    rcall(m, e, r, 0x1c0, &[0], &[]);
    rcall(m, e, r, 0x1c8, &[2, 2], &[k(0x3da3d70a)]);
    let state = m.r32(lm + 0xc);
    if state == 5 || state == 7 {
        draw_buttons(m, e, lm);
    }
    rcall(m, e, r, 0x90, &[], &[]);
    rcall(m, e, r, 0x1c8, &[1, 2], &[k(0x3da3d70a)]);
    rcall(m, e, r, 0x1c0, &[0], &[]);
    rcall(m, e, r, 0x1d0, &[kept], &[]);
}

/// `LessonOverlay_DrawButtons` 0x14bd98(lm): the pad picture (sprite 0x60 at lm+0xa0/+0xa4) and, once the recording
/// (+0x46c) has more than 120 records, the buttons its last record holds lit: the record's +4 bits through the
/// pad config's +0x70 words (14), its +8 bits through the +4 words (26, -1 skipped), its +0x10 low nibble; of the
/// bits in the config's +0x84 mask only the lowest stays when more than one is set
pub fn draw_buttons(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32) {
    let app = m.r32(APP);
    let r = renderer(m);
    rcall(m, e, r, 0x1d0, &[8], &[]);
    set4(m, COLOUR, WHITE);
    let set = m.r32(app + 0x728);
    let rect = call(m, e, SPRITE_RECT, &[set, 0x60], &[]);
    let one = k(0x3f800000);
    let (x, y) = (rf(m, lm + 0xa0), rf(m, lm + 0xa4));
    call(m, e, SPRITE_DRAW, &[rect, 0], &[x, y, one, one]);
    let buf = m.r32(lm + 0x46c);
    if (m.r32(buf) as i32) < 0x79 {
        return;
    }
    let owner = pad_config_owner(m);
    rcall(m, e, r, 0x1d0, &[9], &[]);
    set4(m, COLOUR, WHITE);
    let n = m.r32(m.r32(lm + 0x46c));
    let rec = m.r32(lm + 0x46c).wrapping_add(n.wrapping_mul(0x2c)).wrapping_sub(0x14);
    let (w1, w2, w4) = (m.r32(rec + 4), m.r32(rec + 8), m.r32(rec + 0x10));
    let cfg = m.r32(owner + 0x870);
    let mut lit = 0u32;
    for i in 0..14 {
        if (w1 as i32 >> i) & 1 != 0 {
            lit |= m.r32(cfg + 0x70 + 4 * i);
        }
    }
    for i in 0..26 {
        if (w2 as i32 >> i) & 1 != 0 {
            let v = m.r32(cfg + 4 + 4 * i);
            if v != u32::MAX {
                lit |= v;
            }
        }
    }
    lit |= w4 & 0xf;
    let mask = lit & m.r32(cfg + 0x84);
    let (mut first, mut count) = (-1i32, 0);
    for i in 0..32 {
        if (mask as i32 >> i) & 1 != 0 {
            count += 1;
            if first == -1 {
                first = i;
            }
        }
    }
    if count >= 2 && first != -1 {
        lit = (lit & !mask) | 1 << first;
    }
    for (bx, by, sprite, bits) in BUTTONS {
        let x = rf(m, lm + 0xa0) + k(bx);
        let y = rf(m, lm + 0xa4) + k(by);
        if lit & bits != 0 {
            let set = m.r32(m.r32(APP) + 0x728);
            let rect = call(m, e, SPRITE_RECT, &[set, sprite], &[]);
            call(m, e, SPRITE_DRAW, &[rect, 0], &[x, y, one, one]);
        }
    }
}

/// `LessonOverlay_DrawPage` 0x14c098(lm, fonts): nothing while the pause menu's +0x42c has its +0xc or the menu
/// manager is active; else the state's page (2 and 3 nothing, 4 the trick book, 5 and 7 the pad picture, 6 the
/// trick, 9 the tip, 10 the instructions) and, while the lesson overlay's dialog is up (+0x328), the screen
/// darkened (0.8, 0.1, 0.1, 0.1 over 640×480 in layer 8) and the dialog
pub fn draw_page(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32, fonts: u32, sp: u32) {
    let sp = sp - 0x50;
    let om = m.r32(m.r32(m.r32(APP) + 0x730) + 0x38);
    let pm = call(m, e, PAUSE_MENU_VF2, &[om], &[]);
    if m.r32(m.r32(pm + 0x42c) + 0xc) != 0 {
        return;
    }
    if call(m, e, MENU_IS_ACTIVE, &[MENU_MANAGER], &[]) != 0 {
        return;
    }
    match m.r32(lm + 0xc) {
        2 | 3 => {} // `LessonOverlay_DrawPage2` 0x14c2c0, `DrawPage3` 0x14c2c8: nothing
        4 => draw_trick_book_page(m, e, lm, fonts, sp),
        5 | 7 => draw_buttons(m, e, lm),
        6 => draw_trick_page(m, e, lm, fonts, sp),
        9 => draw_tip_page(m, e, lm, fonts, sp),
        10 => draw_instructions(m, e, lm, fonts, sp),
        _ => {}
    }
    let lo = lesson_overlay(m, e);
    if m.r32(lo + 0x328) == 0 {
        return;
    }
    let r = renderer(m);
    rcall(m, e, r, 0x1d0, &[8], &[]);
    rcall(m, e, r, 0x1e0, &[3], &[]);
    rcall(m, e, r, 0x1f0, &[u32::MAX, 0], &[]);
    set4(m, COLOUR, [0x3f4ccccd, 0x3dcccccd, 0x3dcccccd, 0x3dcccccd]);
    let z = F(0.0);
    call(m, e, FILLED_RECT, &[COLOUR], &[z, z, k(0x44200000), k(0x43f00000)]);
    rcall(m, e, r, 0x1d0, &[9], &[]);
    let lo = lesson_overlay(m, e);
    call(m, e, DIALOG_DRAW, &[lo], &[]);
}

/// `LessonOverlay_DrawTrickBookPage` 0x14c2d0(lm, fonts): the rider's trick book page: the title, the page number
/// (grey past the last complete page), the trick's number, five boxes of the page's tricks (the picked one
/// pulsing when +0xb0 is 1) and the frame round the picked one, the arrows, the trick's name, "done", its info, the
/// menu and the hints
pub fn draw_trick_book_page(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32, fonts: u32, sp: u32) {
    let sp = sp - 0x380;
    let one = k(0x3f800000);
    call(m, e, BACKDROP, &[fonts, 4], &[rf(m, lm + 0x53c)]);
    let r = renderer(m);
    rcall(m, e, r, 0x1d0, &[6], &[]);
    let s = loc(m, e, 0xea8);
    let name = m.r32(RIDER_NAMES + m.r32(lm + 0x510).wrapping_mul(0x38) + 4);
    call(m, e, WSTR_FORMAT, &[sp, s, name], &[]);
    let col = sp + 0x70;
    colour(m, e, col, WHITE);
    let big = m.r32(fonts + 8);
    call(m, e, PRINT_CENTRED, &[big, sp], &[k(0x3f851eb8), k(0x43a00000), k(0x42820000)]);
    let id = call(m, e, GET_PLAYER_ID, &[CAREER, 0], &[]);
    let book = call(m, e, GET_TRICK_BOOK, &[CAREER, id], &[]);
    let complete = call(m, e, COUNT_COMPLETE_PAGES, &[book], &[]);
    if (complete as i32) < (m.r32(lm + 0x50c) as i32) {
        colour(m, e, col, GREY);
    } else {
        colour(m, e, col, GOLD);
    }
    let s = loc(m, e, 0xe6a);
    let page = m.r32(lm + 0x50c).wrapping_add(1);
    call(m, e, WSTR_FORMAT, &[sp + 0x80, s, page], &[]);
    let big = m.r32(fonts + 8);
    let size = k(0x3f8f5c29);
    let w = call(m, e, MEASURE_WIDE, &[big, sp + 0x80, 0], &[size]);
    let width = crate::animseq::cvt_s_w(w as i32);
    let big = m.r32(fonts + 8);
    call(m, e, PRINTF_SHADOWED, &[big, sp + 0x80], &[size, k(0x43540000), k(0x42dc0000)]);
    colour(m, e, col, GOLD);
    let s = loc(m, e, 0xea7);
    let trick = m.r32(lm + 0x4e8).wrapping_add(1);
    call(m, e, WSTR_FORMAT, &[sp + 0xa0, s, trick], &[]);
    let big = m.r32(fonts + 8);
    call(m, e, PRINT_CENTRED, &[big, sp + 0xa0], &[k(0x3f1c28f6), k(0x439f0000), k(0x431b0000)]);
    let s = loc(m, e, 0xe6b);
    let done = call(m, e, COUNT_DONE_IN_PAGE, &[book, m.r32(lm + 0x50c)], &[]);
    let per = call(m, e, TRICKS_PER_PAGE, &[m.r32(lm + 0x50c)], &[]);
    call(m, e, WSTR_FORMAT, &[sp + 0xe0, s, done, per], &[]);

    // the arrows' pulse between 0.95 and 1.05
    let p = rf(m, PULSE);
    if k(0x3f866666) < p || p < k(0x3f733333) {
        let d = rf(m, PULSE_STEP);
        wf(m, PULSE_STEP, -d);
    }
    let p = rf(m, PULSE) + rf(m, PULSE_STEP);
    wf(m, PULSE, p);
    let bx: [F; 5] = BOX_X.map(k);
    let mut bw = [k(BOX_W); 5];
    let mut bh = [k(BOX_H); 5];
    let mut y = k(0x43570000); // 215
    if m.r32(lm + 0xb0) == 1 {
        let i = m.r32(lm + 0xb4) as usize;
        let s = p * k(0x3f7851ec);
        bw[i] = bw[i] * s;
        y = (one - s) * k(0x42340000) + y;
        bh[i] = bh[i] * s;
    }
    let half = k(0x3f000000);
    let full = k(0x44200000);
    for i in 0..5u32 {
        let first = m.r32(lm + 0xb8);
        if call(m, e, IS_TRICK_DONE, &[book, first.wrapping_add(i)], &[]) != 0 {
            colour(m, e, col, WHITE);
        } else {
            colour(m, e, col, GREY);
        }
        rcall(m, e, r, 0x1d0, &[6], &[]);
        let at = lm + 0x470 + 4 * m.r32(lm + 0xb8).wrapping_add(i);
        let tex = m.r32(at);
        rcall(m, e, r, 0x1f0, &[tex, 0], &[]);
        let (w, h) = (bw[i as usize], bh[i as usize]);
        let x = bx[i as usize] * full - w * half;
        let top = if i == m.r32(lm + 0xb4) { y } else { k(0x43570000) };
        call(m, e, FILLED_RECT, &[COLOUR], &[x, top, w, h]);
    }
    rcall(m, e, r, 0x1f0, &[u32::MAX, 0], &[]);
    colour(m, e, col, GOLD);
    let i = m.r32(lm + 0xb4) as usize;
    let (w, h) = (bw[i], bh[i]);
    let hw = w * half;
    let cx = bx[i] * full;
    let bottom = y + h;
    let right = cx + hw;
    let left = cx - hw;
    let (two, four) = (k(0x40000000), k(0x40800000));
    if m.r32(lm + 0xb0) != 0 {
        let l = left - two;
        let t = y - two;
        call(m, e, FILLED_RECT, &[COLOUR], &[l, t, w + four, four]);
        let i = m.r32(lm + 0xb4) as usize;
        call(m, e, FILLED_RECT, &[COLOUR], &[l, t, four, bh[i] + four]);
        let i = m.r32(lm + 0xb4) as usize;
        call(m, e, FILLED_RECT, &[COLOUR], &[right - two, t, four, bh[i] + four]);
        let i = m.r32(lm + 0xb4) as usize;
        call(m, e, FILLED_RECT, &[COLOUR], &[l, bottom - two, bw[i] + four, four]);
    }
    colour(m, e, col, WHITE);
    let s = rf(m, PULSE) * k(0x3f99999a);
    match m.r32(lm + 0xb0) {
        0 => {
            if (m.r32(lm + 0x50c) as i32) > 0 {
                call(m, e, HUD_SPRITE, &[fonts, 0x2b, 0x60], &[k(0x43400000), k(0x42fc0000), s, s]);
            }
            if (m.r32(lm + 0x50c) as i32) < 5 {
                let x = width + k(0x43540000) + k(0x41c80000);
                call(m, e, HUD_SPRITE, &[fonts, 0x2c, 0x60], &[x, k(0x42fc0000), s, s]);
            }
        }
        1 => {
            if (m.r32(lm + 0xb4) as i32) > 0 {
                call(m, e, HUD_SPRITE, &[fonts, 0x2b, 0x60], &[k(0x41f80000), k(0x43790000), s, s]);
            }
            if (m.r32(lm + 0xb4) as i32) < 4 {
                call(m, e, HUD_SPRITE, &[fonts, 0x2c, 0x60], &[k(0x44170000), k(0x43790000), s, s]);
            }
        }
        _ => {}
    }
    let info = call(m, e, TRICK_GET_INFO, &[m.r32(lm + 0x510), m.r32(lm + 0x4e8)], &[]);
    let name = sp + 0x180;
    call(m, e, FORMAT_TRICK_NAME, &[name, info], &[]);
    colour(m, e, col, WHITE);
    let len = call(m, e, STRLEN, &[name], &[]);
    let small = m.r32(fonts + 4);
    if len < 0x35 {
        call(m, e, PRINTF_CENTRED, &[small, name], &[k(0x3f666666), k(0x439d0000), k(0x433b0000)]);
    } else {
        call(m, e, PRINTF_CENTRED, &[small, name], &[k(0x3f43d70a), k(0x439d0000), k(0x433d0000)]);
    }
    colour(m, e, col, DONE_GOLD);
    if call(m, e, IS_TRICK_DONE, &[book, m.r32(lm + 0x4e8)], &[]) != 0 {
        let s = loc(m, e, 0xc93);
        let small = m.r32(fonts + 4);
        call(m, e, PRINT_CENTRED, &[small, s], &[k(0x3f4ccccd), k(0x43a00000), k(0x43ac8000)]);
    }
    colour(m, e, col, WHITE);
    draw_trick_info(m, e, lm, k(0x43a00000), k(0x43999999));
    let flag = (m.r32(lm + 0xb0) == 2) as u32;
    draw_menu(m, e, lm, flag);
    if m.r32(lm + 0xb0) != 0 {
        let y = (rf(m, lm + 0x53c) + k(0x43f00000)) * half - k(0x42200000) - k(0x41b80000);
        colour(m, e, col, WHITE);
        call(m, e, HUD_SPRITE, &[fonts, 0x73, 0], &[k(0x42200000), y, one, one]);
        colour(m, e, col, GOLD);
        let y = y + four;
        let s = loc(m, e, 0x1d7);
        let small = m.r32(fonts + 4);
        call(m, e, PRINTF_SHADOWED, &[small, s], &[k(0x3f3ae148), k(0x42800000), y]);
    }
    colour(m, e, col, WHITE);
    let hint = match m.r32(lm + 0xb0) {
        0 => 0xdd9,
        1 => 0xdda,
        _ => return,
    };
    let s = loc(m, e, hint);
    let small = m.r32(fonts + 4);
    call(m, e, PRINT_CENTRED, &[small, s], &[k(0x3f3ae148), k(0x43a00000), k(0x43c78000)]);
}

/// `LessonOverlay_DrawTrickInfo` 0x14cf38(lm; x, y): the trick info (lm+0x514) filled for the rider (+0x510) and
/// trick (+0x4e8) on the player's pad config, then drawn at x, y
pub fn draw_trick_info(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32, x: F, y: F) {
    let owner = pad_config_owner(m);
    let cfg = m.r32(owner + 0x870);
    call(m, e, TRICK_INFO_VF2, &[lm + 0x514, cfg, m.r32(lm + 0x510), m.r32(lm + 0x4e8)], &[]);
    let cfg = m.r32(owner + 0x870);
    call(m, e, GAME_TRICK_INFO_VF5, &[lm + 0x514, cfg], &[x, y]);
}

/// `LessonOverlay_DrawTrickPage` 0x14cfb0(lm, fonts): the trick's number and name over the backdrop, and the menu
pub fn draw_trick_page(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32, fonts: u32, sp: u32) {
    let sp = sp - 0x1a0;
    let (one, mid) = (k(0x3f800000), k(0x43a00000));
    call(m, e, BACKDROP, &[fonts, 4], &[rf(m, lm + 0x53c)]);
    let info = call(m, e, TRICK_GET_INFO, &[m.r32(lm + 0x510), m.r32(lm + 0x4e8)], &[]);
    call(m, e, FORMAT_TRICK_NAME, &[sp, info], &[]);
    let s = loc(m, e, 0xea7);
    let trick = m.r32(lm + 0x4e8).wrapping_add(1);
    call(m, e, WSTR_FORMAT, &[sp + 0x100, s, trick], &[]);
    let col = sp + 0x120;
    colour(m, e, col, GOLD);
    let big = m.r32(fonts + 8);
    call(m, e, PRINT_CENTRED, &[big, sp + 0x100], &[one, mid, k(0x431b0000)]);
    colour(m, e, col, WHITE);
    set4(m, col, WHITE);
    call(m, e, PRINT_WRAPPED, &[sp, col, 1], &[mid, k(0x43480000), k(0x43f00000), k(0x3f59999a)]);
    draw_menu(m, e, lm, 1);
}

/// `LessonOverlay_DrawTipPage` 0x14d138(lm, fonts): the tip's heading and either the line the lesson names
/// (+0xac) when the score has no trick name, or the trick name wrapped; then the menu
pub fn draw_tip_page(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32, fonts: u32, sp: u32) {
    let sp = sp - 0x190;
    let mid = k(0x43a00000);
    call(m, e, BACKDROP, &[fonts, 4], &[rf(m, lm + 0x53c)]);
    let score = m.r32(m.r32(m.r32(APP) + 0x730) + 0xa4).wrapping_add(0x5820);
    let name = call(m, e, GET_TRICK_NAME, &[score], &[]);
    call(m, e, STRCPY, &[sp, name], &[]);
    let col = sp + 0x100;
    colour(m, e, col, GOLD);
    let s = loc(m, e, 0xebb);
    let big = m.r32(fonts + 8);
    call(m, e, PRINT_CENTRED, &[big, s], &[k(0x3f4ccccd), mid, k(0x43250000)]);
    if call(m, e, STRCMP, &[sp, EMPTY], &[]) == 0 {
        colour(m, e, col, WHITE);
        let s = loc(m, e, m.r32(lm + 0xac));
        let small = m.r32(fonts + 4);
        call(m, e, PRINT_CENTRED, &[small, s], &[k(0x3f666666), mid, k(0x43570000)]);
    } else {
        let name = call(m, e, GET_TRICK_NAME, &[score], &[]);
        set4(m, col, WHITE);
        call(m, e, PRINT_WRAPPED, &[name, col, 1], &[mid, k(0x43570000), k(0x43f00000), k(0x3f59999a)]);
    }
    draw_menu(m, e, lm, 1);
}

/// `LessonOverlay_DrawInstructions` 0x14d320(lm, fonts): a text box (small font, scale 0.82, the box 130, 235,
/// 360×150, line height +3) with the instructions (0x22e, or 0x22f once +0x64 is set), the trick's number and
/// name, then the menu
pub fn draw_instructions(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32, fonts: u32, sp: u32) {
    let sp = sp - 0x1620;
    let (one, mid) = (k(0x3f800000), k(0x43a00000));
    call(m, e, BACKDROP, &[fonts, 4], &[rf(m, lm + 0x53c)]);
    let tb = sp;
    call(m, e, TEXTBOX_INIT, &[tb], &[]);
    call(m, e, TEXTBOX_SET_FONT, &[tb, m.r32(fonts + 4)], &[]);
    call(m, e, TEXTBOX_RESET, &[tb], &[]);
    let col = sp + 0x1470;
    set4(m, col, WHITE);
    for i in 0..4 {
        let v = m.r32(col + 4 * i);
        m.w32(tb + 0x3c + 4 * i, v);
    }
    let s = k(0x3f51eb85);
    call(m, e, TEXTBOX_SET_SCALE, &[tb], &[s, s]);
    m.w32(tb + 0x20, 1);
    m.w32(tb + 0x24, 0);
    call(m, e, TEXTBOX_SET_RECT, &[tb], &[k(0x43020000), k(0x436b0000), k(0x43b40000), k(0x43160000)]);
    let h = rf(m, tb + 0x14) + k(0x40400000);
    call(m, e, TEXTBOX_LINE_HEIGHT, &[tb], &[h]);
    let name = sp + 0x1480;
    let info = call(m, e, TRICK_GET_INFO, &[m.r32(lm + 0x510), m.r32(lm + 0x4e8)], &[]);
    call(m, e, FORMAT_TRICK_NAME, &[name, info], &[]);
    let id = if m.r32(lm + 0x64) != 0 { 0x22f } else { 0x22e };
    let s = loc(m, e, id);
    call(m, e, TEXTBOX_APPEND, &[tb, s], &[]);
    let s = loc(m, e, 0xea7);
    let trick = m.r32(lm + 0x4e8).wrapping_add(1);
    call(m, e, WSTR_FORMAT, &[sp + 0x1580, s, trick], &[]);
    colour(m, e, col, GOLD);
    let big = m.r32(fonts + 8);
    call(m, e, PRINT_CENTRED, &[big, sp + 0x1580], &[one, mid, k(0x43070000)]);
    colour(m, e, col, WHITE);
    set4(m, col, WHITE);
    call(m, e, PRINT_WRAPPED, &[name, col, 1], &[mid, k(0x43340000), k(0x43f00000), k(0x3f59999a)]);
    call(m, e, TEXTBOX_WRAP, &[tb, 0], &[]);
    call(m, e, TEXTBOX_DRAW, &[tb, u32::MAX, 0], &[]);
    draw_menu(m, e, lm, 1);
    call(m, e, TEXTBOX_DESTRUCT, &[tb, 2], &[]);
}

/// `Lesson_DrawMenu` 0x14d5e0(lm, flag): unless the lesson overlay's dialog is up (+0x328), the lesson's menu
/// (`Overlay_DrawMenuList`: items lm+0x68, count +0x5c, picked +0x58, at 600 and half way down the backdrop
/// (+0x53c) less 40, scale 0.73, `flag` and lm+0x84 on the stack: the call's ninth and tenth arguments) in layer
/// 0x10
pub fn draw_menu(m: &mut Mem, e: &mut dyn TextBoxEnv, lm: u32, flag: u32) {
    let lo = lesson_overlay(m, e);
    if m.r32(lo + 0x328) != 0 {
        return;
    }
    let r = renderer(m);
    rcall(m, e, r, 0x1d0, &[0x10], &[]);
    let y = (rf(m, lm + 0x53c) + k(0x43f00000)) * k(0x3f000000);
    let lo = lesson_overlay(m, e);
    let y = y - k(0x42200000);
    let (picked, count) = (m.r32(lm + 0x58), m.r32(lm + 0x5c));
    call(
        m,
        e,
        MENU_LIST,
        &[lo, 3, 0, lm + 0x68, count, picked, 0x74, 1, flag, lm + 0x84],
        &[k(0x44160000), y, k(0x3f3ae148)],
    );
}

/// `Lesson_GetString` 0x14ed88(lm, id): the localiser's string `id` (`lm` unused)
pub fn get_string(m: &mut Mem, e: &mut dyn TextBoxEnv, id: u32) -> u32 {
    loc(m, e, id)
}

/// `Trick_GetInfo` 0x154638(rider, trick): pointer to the trick's `0x1c`-byte record in `DAT_0031e210`
/// (`rider * 0x348 + trick * 0x1c`)
pub fn trick_get_info(rider: u32, trick: u32) -> u32 {
    TRICK_TABLE
        .wrapping_add(rider.wrapping_mul(0x348))
        .wrapping_add(trick.wrapping_mul(0x1c))
}

/// `cTrickInfo::vf2_154688` 0x154688(info, pad_cfg, rider, trick): records `rider` (+0x8) and `trick` (+0xc),
/// then calls virtual `vf4` (+0x28/+0x2c, measuring width on `pad_cfg`) and stores the returned float bits at +0x10
pub fn trick_info_set_trick(
    m: &mut Mem,
    e: &mut dyn TextBoxEnv,
    info: u32,
    pad_cfg: u32,
    rider: u32,
    trick: u32,
) {
    m.w32(info + 0x8, rider);
    m.w32(info + 0xc, trick);
    let vt = m.r32(info + 0x24);
    let adj = m.r16(vt + 0x28) as i16 as u32;
    let func = m.r32(vt + 0x2c);
    let w = call(m, e, func, &[info.wrapping_add(adj), pad_cfg], &[]);
    m.w32(info + 0x10, w);
}

fn append_button_mask(m: &mut Mem, out: u32, pos: &mut u32, mask: u32) {
    for k in 0..12u32 {
        let rec = TRICK_BUTTONS + 8 * k;
        if mask & m.r32(rec) != 0 {
            m.w8(out.wrapping_add(*pos), m.r8(rec + 4));
            *pos = pos.wrapping_add(1);
        }
    }
}

fn append_tweak(m: &mut Mem, e: &mut dyn TextBoxEnv, pad_cfg: u32, out: u32, pos: &mut u32) {
    let n = call(m, e, SPRINTF, &[out.wrapping_add(*pos), FMT_PLUS], &[]);
    *pos = pos.wrapping_add(n);
    let mask = m.r32(pad_cfg + 0x24c);
    let mut count = 0i32;
    for k in 0..12u32 {
        let rec = TRICK_BUTTONS + 8 * k;
        if mask & m.r32(rec) != 0 {
            let was_one = count == 1;
            count += 1;
            if was_one {
                let p = out.wrapping_add(*pos);
                let prev = m.r8(p.wrapping_sub(1));
                m.w8(p.wrapping_sub(1), b'(');
                m.w8(p, prev);
                *pos = pos.wrapping_add(1);
            }
            if count != 1 {
                let n = call(m, e, SPRINTF, &[out.wrapping_add(*pos), FMT_OR], &[]);
                *pos = pos.wrapping_add(n);
            }
            m.w8(out.wrapping_add(*pos), m.r8(rec + 4));
            *pos = pos.wrapping_add(1);
        }
    }
    if count >= 2 {
        m.w8(out.wrapping_add(*pos), b')');
        *pos = pos.wrapping_add(1);
    }
}

/// `TrickInfo_BuildText` 0x1546c8(pad_cfg, out, entry): builds the 48-byte glyph/text prompt for a trick
/// record (`0x1c` bytes from `DAT_0031e210`): clears `out[..0x30]`, flips spin direction if the player matching
/// `pad_cfg` in `race+0x8c` is goofy (`boarder+0x464` -> `+0x40 != 0`), appends spin (`a`/`d` + `180*n`), flip
/// (`+` if spin, `x`/`w` + `360*n`), rail (`" RAIL"` and `" %d"` for `(flags >> 4) & 0xf`), `' '`, grab 1
/// buttons, tweak 1 (`+b` or `+(b1ORb2)`), grab 2 (`" TO "` + buttons), and tweak 2
pub fn trick_info_build_text(m: &mut Mem, e: &mut dyn TextBoxEnv, pad_cfg: u32, out: u32, entry: u32) {
    call(m, e, MEM_SET, &[out, 0, 0x30], &[]);
    let mut pos = 0u32;
    let mut spin = m.r16(entry + 0xa) as i16 as i32;
    let race = m.r32(m.r32(m.r32(APP) + 0x730) + 0x1c);
    let n_players = m.r32(race + 0x7c) as i32;
    if n_players > 0 {
        for i in 0..n_players as u32 {
            let player = m.r32(race + 0x8c + 4 * i);
            if m.r32(player + 0x870) == pad_cfg {
                let boarder = m.r32(player);
                let goofy = m.r32(m.r32(boarder + 0x464) + 0x40);
                if goofy != 0 {
                    spin = spin.wrapping_neg();
                }
                break;
            }
        }
    }
    if spin != 0 {
        let deg = if spin < 0 {
            m.w8(out, b'a');
            (spin.wrapping_neg() as u32).wrapping_mul(0xb4)
        } else {
            m.w8(out, b'd');
            (spin as u32).wrapping_mul(0xb4)
        };
        let n = call(m, e, SPRINTF, &[out.wrapping_add(1), FMT_INT, deg], &[]);
        pos = n.wrapping_add(1);
    }
    if m.r16(entry + 0x8) as i16 != 0 {
        if m.r16(entry + 0xa) as i16 != 0 {
            m.w8(out.wrapping_add(pos), b'+');
            pos = pos.wrapping_add(1);
        }
        let flip = m.r16(entry + 0x8) as i16 as i32;
        let deg = if flip < 0 {
            m.w8(out.wrapping_add(pos), b'x');
            pos = pos.wrapping_add(1);
            (flip.wrapping_neg() as u32).wrapping_mul(0x168)
        } else {
            m.w8(out.wrapping_add(pos), b'w');
            pos = pos.wrapping_add(1);
            (flip as u32).wrapping_mul(0x168)
        };
        let n = call(m, e, SPRINTF, &[out.wrapping_add(pos), FMT_INT, deg], &[]);
        pos = pos.wrapping_add(n);
    }
    if m.r16(entry + 0xc) as i16 != 0 {
        let n = call(m, e, SPRINTF, &[out.wrapping_add(pos), FMT_RAIL], &[]);
        pos = pos.wrapping_add(n);
        let flags = m.r32(entry + 0x4);
        if flags & 0xf0 != 0 {
            let deg = ((flags >> 4) & 0xf).wrapping_mul(0xb4);
            let n = call(m, e, SPRINTF, &[out.wrapping_add(pos), FMT_SPACE_INT, deg], &[]);
            pos = pos.wrapping_add(n);
        }
    }
    m.w8(out.wrapping_add(pos), b' ');
    pos = pos.wrapping_add(1);
    let grab1 = m.r32(entry + 0x14);
    if grab1 != 0x1a {
        let mask = m.r32(pad_cfg.wrapping_add(grab1.wrapping_mul(4)).wrapping_add(0x1cc));
        if mask != u32::MAX {
            append_button_mask(m, out, &mut pos, mask);
        }
    }
    if m.r16(entry + 0x10) as i16 != 0 {
        append_tweak(m, e, pad_cfg, out, &mut pos);
    }
    let grab2 = m.r32(entry + 0x18);
    if grab2 != 0x1a {
        let n = call(m, e, SPRINTF, &[out.wrapping_add(pos), FMT_TO], &[]);
        pos = pos.wrapping_add(n);
        let grab2 = m.r32(entry + 0x18);
        let mask = m.r32(pad_cfg.wrapping_add(grab2.wrapping_mul(4)).wrapping_add(0x1cc));
        if mask != u32::MAX {
            append_button_mask(m, out, &mut pos, mask);
        }
    }
    if m.r16(entry + 0x12) as i16 != 0 {
        append_tweak(m, e, pad_cfg, out, &mut pos);
    }
}

/// `cGameTrickInfo::cGameTrickInfo` 0x154d20(info): writes vtable `0x36fa98` (+0x24), defaults `+0x14 = 240.0`,
/// `+0x18 = 20.0`, `+0x20 = 1.0`, zeroes `+0x0` and `+0x4`, returns `info`
pub fn game_trick_info_construct(m: &mut Mem, info: u32) -> u32 {
    m.w32(info + 0x24, GAME_TRICK_INFO_VT);
    m.w32(info + 0x14, 0x4370_0000);
    m.w32(info + 0x18, 0x41a0_0000);
    m.w32(info + 0x20, 0x3f80_0000);
    m.w32(info, 0);
    m.w32(info + 0x4, 0);
    info
}

/// `cGameTrickInfo::~cGameTrickInfo` 0x155058(info, how): sets vtable `0x36fa98` at +0x24 and tail-calls
/// `cTrickInfo::~cTrickInfo` 0x1550f8
pub fn game_trick_info_destruct(m: &mut Mem, e: &mut dyn TextBoxEnv, info: u32, how: u32) {
    m.w32(info + 0x24, GAME_TRICK_INFO_VT);
    trick_info_destruct(m, e, info, how);
}

/// `cGameTrickInfo::vf3_154d60` 0x154d60(info, x_ptr, idx; y): draws icon `idx` from `0x31e0f0` (`0x18` bytes
/// each: +8 dx, +0xc dy, +0x10 advance, +0x14 sprite id) at `(*x_ptr + dx, y + dy)` and adds `advance` to `*x_ptr`
pub fn game_trick_info_draw_icon(m: &mut Mem, e: &mut dyn TextBoxEnv, _info: u32, x_ptr: u32, idx: u32, y: F) {
    let rec = TRICK_ICONS.wrapping_add(idx.wrapping_mul(0x18));
    let dy = cvt_s_w(m.r32(rec + 0xc) as i32);
    let dx = cvt_s_w(m.r32(rec + 0x8) as i32);
    let set = m.r32(m.r32(APP) + 0x728);
    let sprite = m.r32(rec + 0x14);
    let py = y + dy;
    let rect = call(m, e, SPRITE_RECT, &[set, sprite], &[]);
    let px = rf(m, x_ptr) + dx;
    let one = k(0x3f800000);
    call(m, e, SPRITE_DRAW, &[rect, 0], &[px, py, one, one]);
    let adv = cvt_s_w(m.r32(rec + 0x10) as i32);
    wf(m, x_ptr, rf(m, x_ptr) + adv);
}

/// `cGameTrickInfo::vf4_154e20` 0x154e20(info, pad_cfg): builds the prompt text into its `0x80`-byte stack
/// frame (`sp - 0x80`) and sums widths: icon advance from `0x31e0f0`, `8.0` for `' '`, or
/// `font+0x18 * Font_GetGlyphWidth(font, ch) * info+0x20 + 1.0`
pub fn game_trick_info_measure(m: &mut Mem, e: &mut dyn TextBoxEnv, info: u32, pad_cfg: u32, sp: u32) -> F {
    let frame = sp - 0x80;
    let mut total = F(0.0);
    let entry = trick_get_info(m.r32(info + 0x8), m.r32(info + 0xc));
    trick_info_build_text(m, e, pad_cfg, frame, entry);
    let mut p = frame;
    loop {
        let ch = m.r8(p);
        if ch == 0 {
            return total;
        }
        let ch_i = ch as i8;
        let mut matched = false;
        for idx in 0..12u32 {
            let rec = TRICK_ICONS + 0x18 * idx;
            if ch_i == m.r8(rec) as i8 {
                total = total + cvt_s_w(m.r32(rec + 0x10) as i32);
                matched = true;
                break;
            }
        }
        if !matched {
            if ch_i == b' ' as i8 {
                total = total + k(0x4100_0000);
            } else {
                let font = m.r32(info);
                let spacing = rf(m, font + 0x18);
                let w = call(m, e, FONT_GLYPH_WIDTH, &[font, ch_i as i32 as u32], &[]) as i32;
                let scale = rf(m, info + 0x20);
                total = total + (spacing * cvt_s_w(w) * scale + k(0x3f80_0000));
            }
        }
        p = p.wrapping_add(1);
    }
}

/// `cGameTrickInfo::vf5_154b18` 0x154b18(info, pad_cfg; x, y): calls `vf1`, centres `x` by half of `info+0x10`,
/// builds the prompt into `sp - 0xb0`, and draws each glyph either via `vf3` (icons in `0x31e0f0`), `+8.0` for
/// `' '`, or `Font_PrintfShadowed` + `Font_GetGlyphWidth`
pub fn game_trick_info_draw(
    m: &mut Mem,
    e: &mut dyn TextBoxEnv,
    info: u32,
    pad_cfg: u32,
    x: F,
    y: F,
    sp: u32,
) {
    let frame = sp - 0xb0;
    let vt = m.r32(info + 0x24);
    let adj = m.r16(vt + 0x10) as i16 as u32;
    let vf1 = m.r32(vt + 0x14);
    call(m, e, vf1, &[info.wrapping_add(adj)], &[]);

    let cur_x = frame + 0x40;
    let start_x = x - rf(m, info + 0x10) * k(0x3f00_0000);
    wf(m, cur_x, start_x);
    let entry = trick_get_info(m.r32(info + 0x8), m.r32(info + 0xc));
    trick_info_build_text(m, e, pad_cfg, frame, entry);

    let mut p = frame;
    loop {
        let ch = m.r8(p);
        if ch == 0 {
            return;
        }
        let ch_i = ch as i8;
        let mut icon = None;
        for idx in 0..12u32 {
            let rec = TRICK_ICONS + 0x18 * idx;
            if ch_i == m.r8(rec) as i8 {
                icon = Some(idx);
                break;
            }
        }
        if let Some(idx) = icon {
            let vt = m.r32(info + 0x24);
            let adj = m.r16(vt + 0x20) as i16 as u32;
            let vf3 = m.r32(vt + 0x24);
            call(m, e, vf3, &[info.wrapping_add(adj), cur_x, idx], &[y]);
        } else {
            let y_off = if ch.wrapping_sub(0x28) < 2 {
                k(0xbf80_0000)
            } else if ch == b'+' {
                k(0xc000_0000)
            } else {
                F(0.0)
            };
            m.w8(frame + 0x30, ch);
            m.w8(frame + 0x31, 0);
            if m.r8(p) as i8 == b' ' as i8 {
                wf(m, cur_x, rf(m, cur_x) + k(0x4100_0000));
            } else {
                let scale = rf(m, info + 0x20);
                let cx = rf(m, cur_x);
                let py = y / scale + y_off;
                let px = cx / scale + F(0.0);
                let one = k(0x3f80_0000);
                let font = m.r32(info);
                call(m, e, FONT_PRINTF_SHADOWED, &[font, frame + 0x30], &[one, px, py]);
                let font = m.r32(info);
                let ch_s = m.r8(p) as i8 as i32 as u32;
                let spacing = rf(m, font + 0x18);
                let w = call(m, e, FONT_GLYPH_WIDTH, &[font, ch_s], &[]) as i32;
                let scale = rf(m, info + 0x20);
                let cx = rf(m, cur_x);
                wf(m, cur_x, cx + (spacing * cvt_s_w(w) * scale + one));
            }
        }
        p = p.wrapping_add(1);
    }
}

/// `cTrickInfo::~cTrickInfo` 0x1550f8(info, how): the table back at +0x24; deleted when `how` has bit 0. Its
/// other small virtuals: vf1 0x1550e8, vf3 0x1550e0, vf5 0x1550c8 do nothing ([`nothing`]), vf4 0x1550d0 answers
/// 0.0
pub fn trick_info_destruct(m: &mut Mem, e: &mut dyn TextBoxEnv, info: u32, how: u32) {
    m.w32(info + 0x24, TRICK_INFO_VT);
    if how & 1 != 0 {
        call(m, e, MEM_DELETE, &[info], &[]);
    }
}

/// The empty virtuals: `cTrickInfo` vf1 0x1550e8, vf3 0x1550e0, vf5 0x1550c8, `cLessonMan` vf7 0x17ded8
pub fn nothing() {}

/// `cTrickInfo::vf4` 0x1550d0: 0.0
pub fn trick_info_vf4() -> F {
    F(0.0)
}

/// `cLessonOverlay::Draw` 0x1b8548(ov): the race's lesson manager (+0x34) draws its page with the overlay as its
/// font set (a tail call: the same `sp`). `cLessonControllerOverlay::Draw` 0x1b8560 does nothing ([`nothing`]).
pub fn overlay_draw(m: &mut Mem, e: &mut dyn TextBoxEnv, ov: u32, sp: u32) {
    let lm = m.r32(m.r32(m.r32(APP) + 0x730) + 0x34);
    draw_page(m, e, lm, ov, sp);
}
