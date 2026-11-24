//! The front end's manager (board row G9b2d1) on the game's bytes: `cFE3D`'s per-frame update (vf5) and draw (vf6),
//! its player count and its small helpers, every call out through [`FeEnv`]; checked in the function runner
//! (`tests/fe_manager.rs`). Its constructors, destructor and vf4 (row G9b2d2a) are here too, checked in
//! `tests/fe_manager_setup.rs`; vf1, the 19 objects' set-up (row G9b2d2b), in `tests/fe_manager_init.rs`.
//!
//! The manager (gApp+0x734): +4 the players (a signed byte), +5 a byte, +6 the state (0 the menus, 1 leaving for the
//! game, 2 left), +8 a word, +0x60 a vector, +0x74 / +0x78 the two players, +0x7c the players' picks (two bytes
//! each), +0x80 a list of things drawn and updated, +0xa4 the 3D scene (its vtable at +0), +0xa8 the screen stack
//! (the finished screens at +0xcc), +0xf0 the game type, +0xf4 the idle frames, +0xf8 the attract film (none while
//! the menus run), +0x124 the video manager (its vtable at +8).

use crate::fe3dstate::{vcall, FeEnv, ALLOC, FILL, GAPP};
use crate::fe_characcept::CAREER;
use crate::fe_charselect::SEEN;
use crate::fe_helpers::{add_player, last};
use crate::fe_riderprofile::{SET_FILM, VIDEO_VTABLE};
use crate::fe_title::BASE_CTOR;
use crate::fe_trickbook::CLEAR_SLOT;
use crate::pf::Mem;
use crate::ps2f::F;

/// The calls: the transition's update (0x1fece8) and the fade's draw (0x1ff000) on gApp+0x72c; a player's cheats
/// (0x26cf60); the drawn list's update (0x26ff88) and draws (0x26feb8, 0x26ff40); the sound bank's update (0x17d950
/// with 3 and 4 on 0x337c60) and draw-side calls (0x17d9d8); the pads' check (0x26cef8); the stack's close
/// (0x275ac8) and push (0x275b10); the music's tick (0x20ee58: the word at 0x343050, 0, f12 0.02); the game made on
/// leaving (by the career's +0x3710, its +0x2afc and the players: 0x24c748 (0x1e68 bytes, with 1), 0x24d7f8
/// (0x9f0), 0x247a60 (0xd64), 0x245840 (0xec4)) and handed to the app (0x17c6b8); the course's draw (0x1ce278 the
/// camera, 0x2008f8 and 0x200390 on the course at 0x346488); the character select made (0x276be8) and the pads'
/// rule (0x265af8)
pub const TRANSITION_UPDATE: u32 = 0x1fece8;
pub const FADE_DRAW: u32 = 0x1ff000;
pub const CHEATS: u32 = 0x26cf60;
pub const DRAWN_UPDATE: u32 = 0x26ff88;
pub const DRAWN_DRAW_COURSE: u32 = 0x26feb8;
pub const DRAWN_DRAW: u32 = 0x26ff40;
pub const SOUND_BANK: u32 = 0x337c60;
pub const SOUND_UPDATE: u32 = 0x17d950;
pub const SOUND_DRAW: u32 = 0x17d9d8;
pub const PAD_CONNECTED: u32 = 0x26cef8;
pub const STACK_CLOSE: u32 = 0x275ac8;
pub const STACK_PUSH: u32 = 0x275b10;
pub const MUSIC_TICK: u32 = 0x20ee58;
pub const MUSIC_WORD: u32 = 0x343050;
pub const GAME_HAND: u32 = 0x17c6b8;
pub const GAMES: [(u32, u32); 4] = [(0x24c748, 0x1e68), (0x24d7f8, 0x9f0), (0x247a60, 0xd64), (0x245840, 0xec4)];
pub const CAMERA: u32 = 0x1ce278;
pub const COURSE: u32 = 0x346488;
pub const COURSE_DRAW: u32 = 0x2008f8;
pub const COURSE_DRAW2: u32 = 0x200390;
pub const CHARSELECT_CTOR: u32 = 0x276be8;
pub const ATTRACT_FILM: u32 = 0x3a8d48;
pub const ORDER: u32 = 0x3470b8;

fn i8_at(m: &Mem, at: u32) -> i32 {
    m.r8(at) as i8 as i32
}

/// The renderer's virtual (gApp+0x724; its vtable at +0x10)
fn renderer_call(m: &mut Mem, e: &mut dyn FeEnv, off: u32, args: &[u32]) -> u32 {
    let r = m.r32(m.r32(GAPP).wrapping_add(0x724));
    let vt = m.r32(r.wrapping_add(0x10));
    let adj = m.r16(vt.wrapping_add(off)) as i16 as u32;
    let f = m.r32(vt.wrapping_add(off + 4));
    let mut a = vec![r.wrapping_add(adj)];
    a.extend_from_slice(args);
    e.call(m, f, &a)
}

/// A virtual of an object whose vtable is at `vt_at`
fn vcall_at(m: &mut Mem, e: &mut dyn FeEnv, obj: u32, vt_at: u32, off: u32, args: &[u32]) -> u32 {
    let vt = m.r32(obj.wrapping_add(vt_at));
    let adj = m.r16(vt.wrapping_add(off)) as i16 as u32;
    let f = m.r32(vt.wrapping_add(off + 4));
    let mut a = vec![obj.wrapping_add(adj)];
    a.extend_from_slice(args);
    e.call(m, f, &a)
}

fn alloc(m: &mut Mem, e: &mut dyn FeEnv, tag: u32, size: u32, align: u32) -> u32 {
    let at = e.call(m, ALLOC, &[tag, size, align]);
    if at != 0 {
        e.call(m, FILL, &[at, 0xdead_c0de, size]);
    }
    at
}

/// 0x263670 (the manager, the players): fewer players clear the slots from the new count up (0x1637d0); more seat
/// each new one (0x1635b8 with its own number twice and 0); the count kept (+4)
pub fn set_players(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, n: u32) {
    let n = n as u8 as i8 as i32;
    let cur = i8_at(m, fe + 4);
    if n < cur {
        let mut k = n;
        loop {
            e.call(m, CLEAR_SLOT, &[CAREER, k as u32]);
            k += 1;
            if k >= i8_at(m, fe + 4) {
                break;
            }
        }
    } else if cur < n {
        let more = (n - m.r8(fe + 4) as i32) as u8 as i8 as i32;
        for k in 0..more {
            let slot = (i8_at(m, fe + 4) + k) as u32;
            e.call(m, SEEN, &[CAREER, slot, slot, 0]);
        }
    }
    m.w8(fe + 4, n as u8);
}

/// `cFE3D` vf2 0x263750: when the 3D scene's +0x128 answers, +8 cleared and 1; else 0
pub fn vf2(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) -> u32 {
    let scene = m.r32(fe + 0xa4);
    if vcall_at(m, e, scene, 0, 0x128, &[]) != 0 {
        m.w32(fe + 8, 0);
        1
    } else {
        0
    }
}

/// `cFE3D` vf5 0x265398 (the frame's update): the transition and the video manager (+0x10) ticked; then by the
/// state: in the menus, each player's cheats, the drawn list, the sound bank's 3 and 4, and with no attract film any
/// player's input resets the idle count, else (unless the top screen answers its +0x70) after 0xe11 idle frames the
/// attract film (a video screen with the film 0x3a8d48) is pushed; the finished screens closed; one music tick.
/// Leaving (1): the game made by the career and players and handed to the app, state 2, then two music ticks;
/// afterwards (2 on) two music ticks.
pub fn update(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) {
    let rec = m.r32(m.r32(GAPP).wrapping_add(0x72c));
    e.call(m, TRANSITION_UPDATE, &[rec]);
    let video = m.r32(fe + 0x124);
    vcall_at(m, e, video, 8, 0x10, &[]);
    let st = i8_at(m, fe + 6);
    let tick = |m: &mut Mem, e: &mut dyn FeEnv| {
        let w = m.r32(MUSIC_WORD);
        e.call(m, MUSIC_TICK, &[w, 0, 0.02f32.to_bits()]);
    };
    if st != 0 {
        if st == 1 {
            let app = m.r32(GAPP);
            let k = if m.r32(CAREER + 0x3710) == 1 {
                0
            } else if m.r32(CAREER + 0x2afc) == 6 {
                1
            } else if i8_at(m, fe + 4) == 2 {
                2
            } else {
                3
            };
            let (ctor, size) = GAMES[k];
            let at = alloc(m, e, 0x3a9090, size, 0x10);
            let game = if k == 0 { e.call(m, ctor, &[at, 1]) } else { e.call(m, ctor, &[at]) };
            e.call(m, GAME_HAND, &[app, game]);
            m.w8(fe + 6, 2);
        }
        tick(m, e);
        tick(m, e);
        return;
    }
    for k in 0..2 {
        let p = m.r32(fe + 0x74 + 4 * k);
        e.call(m, CHEATS, &[p]);
    }
    e.call(m, DRAWN_UPDATE, &[fe + 0x80]);
    e.call(m, SOUND_UPDATE, &[SOUND_BANK, 3]);
    e.call(m, SOUND_UPDATE, &[SOUND_BANK, 4]);
    if m.r32(fe + 0xf8) == 0 {
        let mut any = false;
        for k in 0..2 {
            let p = m.r32(fe + 0x74 + 4 * k);
            if e.call(m, PAD_CONNECTED, &[p]) != 0 {
                let p = m.r32(fe + 0x74 + 4 * k);
                if m.r32(p) != 0 {
                    any = true;
                }
            }
        }
        if any {
            m.w32(fe + 0xf4, 0);
        } else {
            let top = last(m, fe + 0xa8);
            if vcall(m, e, top, 0x70, &[]) == 0 {
                let idle = m.r32(fe + 0xf4).wrapping_add(1);
                m.w32(fe + 0xf4, idle);
                if (idle as i32) >= 0xe11 {
                    let v = alloc(m, e, 0x3a8d38, 0x160, 0x10);
                    e.call(m, BASE_CTOR, &[v]);
                    m.w32(v.wrapping_add(0x15c), 0);
                    m.w32(v.wrapping_add(0x154), 0);
                    m.w32(fe + 0xf8, v);
                    m.w32(v.wrapping_add(0xc), VIDEO_VTABLE);
                    m.w32(v.wrapping_add(0x158), 1);
                    e.call(m, SET_FILM, &[v, ATTRACT_FILM]);
                    let v = m.r32(fe + 0xf8);
                    vcall(m, e, v, 0x60, &[]);
                    let v = m.r32(fe + 0xf8);
                    e.call(m, STACK_PUSH, &[fe + 0xa8, v]);
                }
            }
        }
    }
    e.call(m, STACK_CLOSE, &[fe + 0xa8]);
    tick(m, e);
}

/// `cFE3D` vf6 0x264fd8 (the frame's draw; answers 0 when the renderer isn't ready (+0x70), else 1; `buf` 0x80
/// bytes): in the menus the video manager's draw (+0x18); the attract film drawn alone (+0xe0); a film screen on top
/// (its +0x70 answering 0xc) or no screen drawn alone; otherwise the fade, and in the menus the scene: the renderer's
/// set-up (+0x1a0, +0x88, +0xd8, +0x1f8(0, 0), +0x1e0(0), the saved +0x210, +0x1e8(0, 0), +0x1c8(1, 2, 0.08)), with a
/// course loaded its camera and the drawn list over it, the 640 × 480 projection, +0x108, +0x1e0(3), +0x1c8(2, 2,
/// 0.08), the word at 0x337ff8 2.0, +0x1e8(1, 0), the drawn list, +0x1e8 back, and the frame's end (+0x80, +0x1a8,
/// +0x90, +0xe0); the others end with +0x80 alone
pub fn draw(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, buf: u32) -> u32 {
    if renderer_call(m, e, 0x70, &[]) == 0 {
        return 0;
    }
    if i8_at(m, fe + 6) == 0 {
        let video = m.r32(fe + 0x124);
        vcall_at(m, e, video, 8, 0x18, &[]);
    }
    let film = m.r32(fe + 0xf8);
    let alone = if film != 0 {
        true
    } else {
        let top = last(m, fe + 0xa8);
        top == 0 || {
            let top = last(m, fe + 0xa8);
            vcall(m, e, top, 0x70, &[]) == 0xc
        }
    };
    if alone {
        let film = m.r32(fe + 0xf8);
        if film != 0 {
            vcall(m, e, film, 0xe0, &[]);
        }
        renderer_call(m, e, 0x80, &[]);
        return 1;
    }
    let rec = m.r32(m.r32(GAPP).wrapping_add(0x72c));
    e.call(m, FADE_DRAW, &[rec]);
    if i8_at(m, fe + 6) != 0 {
        return 1;
    }
    renderer_call(m, e, 0x1a0, &[]);
    renderer_call(m, e, 0x88, &[]);
    renderer_call(m, e, 0xd8, &[]);
    renderer_call(m, e, 0x1f8, &[0, 0]);
    renderer_call(m, e, 0x1e0, &[0]);
    let saved = renderer_call(m, e, 0x210, &[]);
    renderer_call(m, e, 0x1e8, &[0, 0]);
    let fog = 0.08f32.to_bits();
    renderer_call(m, e, 0x1c8, &[1, 2, fog]);
    if m.r32(COURSE) != 0 {
        e.call(m, CAMERA, &[buf]);
        renderer_call(m, e, 0x218, &[buf]);
        renderer_call(m, e, 0x1d0, &[2]);
        e.call(m, DRAWN_DRAW_COURSE, &[fe + 0x80]);
        e.call(m, COURSE_DRAW, &[COURSE]);
        renderer_call(m, e, 0x2c8, &[]);
        e.call(m, COURSE_DRAW2, &[COURSE]);
        e.call(m, SOUND_DRAW, &[SOUND_BANK, 3]);
        e.call(m, SOUND_DRAW, &[SOUND_BANK, 4]);
    }
    renderer_call(m, e, 0xb0, &[0, 0, 640.0f32.to_bits(), 480.0f32.to_bits(), 0, 1000.0f32.to_bits()]);
    renderer_call(m, e, 0x108, &[]);
    renderer_call(m, e, 0x1e0, &[3]);
    renderer_call(m, e, 0x1c8, &[2, 2, fog]);
    m.w32(0x337ff8, 2.0f32.to_bits());
    renderer_call(m, e, 0x1e8, &[1, 0]);
    e.call(m, DRAWN_DRAW, &[fe + 0x80]);
    renderer_call(m, e, 0x1e8, &[saved, 0]);
    renderer_call(m, e, 0x80, &[]);
    renderer_call(m, e, 0x1a8, &[]);
    renderer_call(m, e, 0x90, &[]);
    renderer_call(m, e, 0xe0, &[]);
    1
}

/// 0x265758: the next of two alternating values: the byte at 0x3471f8 × 2 + the byte at 0x347200 (signed), the
/// latter flipped
pub fn alternate(m: &mut Mem) -> u32 {
    let a = m.r8(0x3471f8) as u32;
    let b = m.r8(0x347200) as u32;
    m.w8(0x347200, (b ^ 1) as u8);
    (b.wrapping_add(a << 1) as u8 as i8 as i32) as u32
}

/// 0x265788 (the manager, a place): the character at that place of the front end's order (bytes at 0x3470b8)
pub fn order(m: &Mem, k: u32) -> u32 {
    let k = k as u8 as i8 as i32;
    (m.r8(ORDER.wrapping_add(k as u32)) as i8 as i32) as u32
}

/// 0x2657a8 (the manager): the career set for the game (+0x2b08 cleared; by the mode at +0x27dc+0x320: 6 the
/// tutorial (+0x2af8 10, one player), 7 the trick book (its trick's +0x10 from 0x163468, the manager's players), 1
/// the manager's players, any other the career's +0x16b8), the career's 0x163980, leaving (+6 1)
pub fn set_game(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) {
    let c = CAREER;
    m.w32(c + 0x2b08, 0);
    let mode = m.r32(c + 0x27dc + 0x320);
    match mode {
        6 => {
            m.w32(c + 0x2af8, 10);
            m.w32(c + 0x27dc, 1);
        }
        7 => {
            let arg = m.r32(c + 0x27dc + 0x31c);
            let r = e.call(m, 0x163468, &[c, 2, arg]);
            m.w32(c + 0x2b08, m.r32(r.wrapping_add(0x10)));
            m.w32(c + 0x27dc, i8_at(m, fe + 4) as u32);
        }
        1 => {
            m.w32(c + 0x27dc, i8_at(m, fe + 4) as u32);
        }
        _ => {
            let v = m.r32(c + 0x16b8);
            m.w32(c + 0x27dc, v);
        }
    }
    e.call(m, 0x163980, &[c]);
    m.w8(fe + 6, 1);
}

/// 0x265870 (the manager): the players whose pads are in
pub fn pads_in(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) -> u32 {
    let mut n = 0u32;
    for k in 0..2 {
        let p = m.r32(fe + 0x74 + 4 * k);
        if e.call(m, PAD_CONNECTED, &[p]) != 0 {
            n += 1;
        }
    }
    n
}

/// 0x2658e8 (the manager, a character): its place in the order, or 0
pub fn place_of(m: &Mem, c: u32) -> u32 {
    let c = c as u8 as i8 as i32;
    for k in 0..12u32 {
        if order(m, k) as i32 == c {
            return k;
        }
    }
    0
}

/// 0x265970 (the manager; `_a1` unused): the character select made (0x154 bytes, 0x276be8); with the pads' rule
/// (0x265af8) each player not yet picked (its +0x44 0) added to it, or every player when none was; without it every
/// player
pub fn make_character_select(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, _a1: u32) -> u32 {
    let at = alloc(m, e, 0x3a90a0, 0x154, 0);
    let sel = e.call(m, CHARSELECT_CTOR, &[at]);
    if pads_rule(m) != 0 {
        m.w8(sel.wrapping_add(0x2d), 0);
        for k in 0..2u32 {
            let p = m.r32(fe + 0x74 + 4 * k);
            if m.r8(p.wrapping_add(0x44)) == 0 {
                add_player(m, sel, k);
            }
        }
        if m.r8(sel.wrapping_add(0x2d)) == 0 {
            add_player(m, sel, u32::MAX);
        }
    } else {
        add_player(m, sel, u32::MAX);
    }
    sel
}

/// 0x265af8: the pads' rule (the word at 0x32e5f0's low two bits clear)
pub fn pads_rule(m: &Mem) -> u32 {
    (m.r32(0x32e5f0) & 3 == 0) as u32
}

/// 0x265bb8 (a player): not picked (+0x44 -1)
pub fn player_reset(m: &mut Mem, p: u32) {
    m.w8(p + 0x44, 0xff);
}

/// 0x265be8 / 0x265bf0 / 0x265bf8: the idle frames cleared; the game type read and set
pub fn idle_reset(m: &mut Mem, fe: u32) {
    m.w32(fe + 0xf4, 0);
}
pub fn game_type(m: &Mem, fe: u32) -> u32 {
    m.r32(fe + 0xf0)
}
pub fn set_game_type(m: &mut Mem, fe: u32, v: u32) {
    m.w32(fe + 0xf0, v);
}

/// 0x265c00 / 0x265c10 (the manager, a player, (a pick)): the player's pick byte (+0x7c + 2 × player) read and set
pub fn pick(m: &Mem, fe: u32, p: u32) -> u32 {
    (m.r8(fe.wrapping_add(p.wrapping_mul(2)).wrapping_add(0x7c)) as i8 as i32) as u32
}
pub fn set_pick(m: &mut Mem, fe: u32, p: u32, v: u32) {
    m.w8(fe.wrapping_add(p.wrapping_mul(2)).wrapping_add(0x7c), v as u8);
}

/// 0x265c28: two or more pads in
pub fn two_pads(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) -> u32 {
    (pads_in(m, e, fe) as i32 >= 2) as u32
}

/// 0x265c50 / 0x265c58: the vector at +0x60 (its address; set from a quadword)
pub fn vector_at(fe: u32) -> u32 {
    fe + 0x60
}
pub fn set_vector(m: &mut Mem, fe: u32, v: u32) {
    for k in 0..4 {
        let x = m.r32((v & !0xf) + 4 * k);
        m.w32(((fe + 0x60) & !0xf) + 4 * k, x);
    }
}

/// `cFE3D` vf3 0x265c68: +8. 0x265c70 / 0x265c78: the state (+6) read; leaving (+6 1). 0x265c88 / 0x265c90: +5 set /
/// read. 0x265c98: the players (+4)
pub fn vf3(m: &Mem, fe: u32) -> u32 {
    m.r32(fe + 8)
}
pub fn state(m: &Mem, fe: u32) -> u32 {
    i8_at(m, fe + 6) as u32
}
pub fn leave(m: &mut Mem, fe: u32) {
    m.w8(fe + 6, 1);
}
pub fn set_byte5(m: &mut Mem, fe: u32, v: u32) {
    m.w8(fe + 5, v as u8);
}
pub fn byte5(m: &Mem, fe: u32) -> u32 {
    i8_at(m, fe + 5) as u32
}
pub fn players(m: &Mem, fe: u32) -> u32 {
    i8_at(m, fe + 4) as u32
}

/// 0x265a58 (the manager, a pass-on word): with two pads in, a 0x54-byte screen (the base constructor, vtable
/// 0x3b30b0); else one player and the character select
pub fn after_players(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, a1: u32) -> u32 {
    if pads_in(m, e, fe) as i32 >= 2 {
        let s = alloc(m, e, 0x3a90b0, 0x54, 0);
        e.call(m, BASE_CTOR, &[s]);
        m.w32(s.wrapping_add(0xc), 0x3b30b0);
        s
    } else {
        set_players(m, e, fe, 1);
        make_character_select(m, e, fe, a1)
    }
}

/// 0x265b10: the front end's light presets (the words at 0x3d2df8..0x3d2e40)
pub fn set_lights(m: &mut Mem) {
    for (at, v) in [
        (0x3d2e20u32, 0x3b03_126fu32),
        (0x3d2df8, 0x3f80_0000),
        (0x3d2e00, 0),
        (0x3d2e40, 0x3f80_0000),
        (0x3d2e18, 0x3f80_0000),
        (0x3d2e1c, 0x3d9b_3d08),
        (0x3d2e08, 0x3f80_0000),
        (0x3d2e0c, 0x3ef2_fec5),
        (0x3d2e10, 0),
        (0x3d2dfc, 0x3f76_e979),
        (0x3d2e28, u32::MAX),
        (0x3d2e2c, 0x3f80_0000),
        (0x3d2e30, 0x3f80_0000),
        (0x3d2e34, 0),
        (0x3d2e38, 0),
        (0x3d2e3c, 0x3f80_0000),
    ] {
        m.w32(at, v);
    }
}

/// The heap's free (0x114f80)
pub const FREE: u32 = 0x114f80;

/// 0x265bc8 (an object, the destructor's flags): freed when the flags ask (bit 0)
pub fn free_if(m: &mut Mem, e: &mut dyn FeEnv, obj: u32, flags: u32) {
    if flags & 1 != 0 {
        e.call(m, FREE, &[obj]);
    }
}

/// The set-up and tear-down's calls (board row G9b2d2a): a drawn list's constructor (0x2abfb0), the lesson's set up
/// (0x158d18 on 0x3211a8 with 0x7d3e8) and the career's two-player test (0x164288); the manager's lights' destructor
/// (0x261e98 on +0x70), the fades' reset (0x1ff6a0 on gApp+0x72c), a model's unload (0x2abf38), the stack's clear
/// (0x275c60), the drawn list's clear (0x26feb0), the course's unload (0x25f498 on 0x346488, 1), a player's
/// destructor (0x265bc8), the fonts' release (0x19c8d0), the lesson's stop (0x159070) and reset (0x158cc8), the world
/// lists' clear (0x17d440 on 0x337c60), the pool's (0x23d368 on 0x345e48), the stack's, the drawn list's and the
/// lights' own destructors (0x2759f0, 0x265dc0, 0x265da0); a model's load (0x266320), loaded test (0x2663e8) and
/// finish (0x2661e8), the frame wait (0x2ced00); a video screen's film (0x2abb58) and its first screen (0x2aba70)
pub mod setup {
    pub const VTABLE: u32 = 0x3a90c0;
    pub const LIST_VTABLE: u32 = 0x3a9100;
    pub const LIST_CTOR: u32 = 0x2abfb0;
    pub const LESSON_INIT: u32 = 0x158d18;
    pub const LESSON: u32 = 0x3211a8;
    pub const LESSON_FUNC: u32 = 0x7d3e8;
    pub const TWO_PLAYERS: u32 = 0x164288;
    pub const LIGHTS_DTOR: u32 = 0x261e98;
    pub const FADES_RESET: u32 = 0x1ff6a0;
    pub const MODEL_UNLOAD: u32 = 0x2abf38;
    pub const STACK_CLEAR: u32 = 0x275c60;
    pub const DRAWN_CLEAR: u32 = 0x26feb0;
    pub const COURSE_UNLOAD: u32 = 0x25f498;
    pub const PLAYER_DTOR: u32 = 0x265bc8;
    pub const FONT_RELEASE: u32 = 0x19c8d0;
    pub const LESSON_STOP: u32 = 0x159070;
    pub const LESSON_RESET: u32 = 0x158cc8;
    pub const LISTS_CLEAR: u32 = 0x17d440;
    pub const POOL_CLEAR: u32 = 0x23d368;
    pub const STACK_DTOR: u32 = 0x2759f0;
    pub const DRAWN_DTOR: u32 = 0x265dc0;
    pub const LIGHTS_DTOR2: u32 = 0x265da0;
    pub const MODEL_LOAD: u32 = 0x266320;
    pub const MODEL_LOADED: u32 = 0x2663e8;
    pub const MODEL_FINISH: u32 = 0x2661e8;
    pub const FRAME_WAIT: u32 = 0x2ced00;
    pub const FIRST_SCREEN: u32 = 0x2aba70;
    pub const FILM_NAME: u32 = 0x3a8d38;
    /// The boot films: the logo's and the intro's
    pub const FILMS: [u32; 2] = [0x3a8d48, 0x3a8d60];
    /// The first screen the menus push when there is no film (the word at 0x3471fc)
    pub const FIRST: u32 = 0x3471fc;
}

/// The part both constructors share: the table, +0x70 3 and +0x71 -1, the picks (+0x7c..+0x7f) cleared, the drawn
/// list (+0x80, its table at +0xa0) and the stack's two lists (+0xa8, +0xcc) made
fn fe_base(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) {
    m.w32(fe, setup::VTABLE);
    m.w8(fe + 0x70, 3);
    m.w8(fe + 0x71, 0xff);
    for k in 0..4 {
        m.w8(fe + 0x7c + k, 0);
    }
    e.call(m, setup::LIST_CTOR, &[fe + 0x80]);
    m.w32(fe + 0x80 + 0x20, setup::LIST_VTABLE);
    e.call(m, setup::LIST_CTOR, &[fe + 0xa8]);
    e.call(m, setup::LIST_CTOR, &[fe + 0xcc]);
}

/// `cFE3D::cFE3D` 0x263280(fe, from_game) -> fe: the base; +5, +6 and +0xc..+0x18, +0x50, +0xa4 cleared, the game
/// type 2 (+0xf0), the idle frames and the film cleared; the app's step cleared, the career's +0x3710 cleared and,
/// coming from the game, its +0x3260 1; the lesson set up, +8 cleared, the players by the career's two-player test
pub fn fe_ctor(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, from_game: u32) -> u32 {
    m.w32(fe, setup::VTABLE);
    m.w8(fe + 0x70, 3);
    m.w8(fe + 0x71, 0xff);
    m.w8(fe + 5, 0);
    m.w8(fe + 6, 0);
    for at in [0xcu32, 0x10, 0x14, 0x18, 0x50] {
        m.w32(fe + at, 0);
    }
    for k in 0..4 {
        m.w8(fe + 0x7c + k, 0);
    }
    e.call(m, setup::LIST_CTOR, &[fe + 0x80]);
    m.w32(fe + 0xa4, 0);
    m.w32(fe + 0x80 + 0x20, setup::LIST_VTABLE);
    e.call(m, setup::LIST_CTOR, &[fe + 0xa8]);
    e.call(m, setup::LIST_CTOR, &[fe + 0xcc]);
    m.w32(fe + 0xf0, 2);
    m.w32(fe + 0xf4, 0);
    let app = m.r32(GAPP);
    m.w32(app + 8, 0);
    m.w32(fe + 0xf8, 0);
    m.w32(CAREER + 0x3710, 0);
    if from_game != 0 {
        m.w32(CAREER + 0x3260, 1);
    }
    e.call(m, setup::LESSON_INIT, &[setup::LESSON, setup::LESSON_FUNC]);
    m.w32(fe + 8, 0);
    let n = e.call(m, setup::TWO_PLAYERS, &[CAREER]);
    m.w8(fe + 4, n as u8);
    set_players(m, e, fe, n as u8 as i8 as i32 as u32);
    fe
}

/// `cFE3D::cFE3D` 0x265ca0(fe) -> fe: the base only
pub fn fe_ctor_base(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) -> u32 {
    fe_base(m, e, fe);
    fe
}

/// An object destroyed through its table at `vt_at` (+8 with 3), when there is one
fn destroy_at(m: &mut Mem, e: &mut dyn FeEnv, o: u32, vt_at: u32) {
    if o != 0 {
        vcall_at(m, e, o, vt_at, 8, &[3]);
    }
}

/// `cFE3D::~cFE3D` 0x2633d0(fe, flags): its table; the lights, the fades reset, the twelve models (+0x20..)
/// unloaded and destroyed, the stack and drawn list cleared, the course unloaded; +0x18 destroyed and cleared; the
/// two scenes (+0x10, +0x14) closed (+0x18) and destroyed; both players; the 3D scene (+0xa4) closed (+0x120) and
/// destroyed; the video manager (+0x124); the effects loader freed, the fonts released, the renderer's +0x148, the
/// lesson stopped and reset, the world lists and the pool cleared, the app's front end (+0x734) cleared; the stack,
/// the drawn list and the lights destroyed (with 2), then the module's destructor
pub fn fe_dtor(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, flags: u32) {
    m.w32(fe, setup::VTABLE);
    e.call(m, setup::LIGHTS_DTOR, &[fe + 0x70]);
    let fades = m.r32(m.r32(GAPP) + 0x72c);
    e.call(m, setup::FADES_RESET, &[fades]);
    for k in 0..12u32 {
        let at = fe + 0x20 + 4 * k;
        let o = m.r32(at);
        e.call(m, setup::MODEL_UNLOAD, &[o]);
        let o = m.r32(at);
        destroy_at(m, e, o, 0xc);
    }
    e.call(m, setup::STACK_CLEAR, &[fe + 0xa8]);
    e.call(m, setup::DRAWN_CLEAR, &[fe + 0x80]);
    e.call(m, setup::COURSE_UNLOAD, &[COURSE, 1]);
    let o = m.r32(fe + 0x18);
    destroy_at(m, e, o, 4);
    m.w32(fe + 0x18, 0);
    for at in [0x10u32, 0x14] {
        let o = m.r32(fe + at);
        vcall_at(m, e, o, 0x10, 0x18, &[]);
        let o = m.r32(fe + at);
        destroy_at(m, e, o, 0x10);
    }
    for at in [0x74u32, 0x78] {
        let p = m.r32(fe + at);
        if p != 0 {
            e.call(m, setup::PLAYER_DTOR, &[p, 3]);
        }
    }
    let sc = m.r32(fe + 0xa4);
    vcall_at(m, e, sc, 0, 0x120, &[]);
    let sc = m.r32(fe + 0xa4);
    destroy_at(m, e, sc, 0);
    let vm = m.r32(fe + 0x124);
    destroy_at(m, e, vm, 8);
    let app = m.r32(GAPP);
    crate::app::free_effects_loader(m, e, app);
    for at in [0x5cu32, 0x60] {
        let f = m.r32(m.r32(GAPP) + at);
        e.call(m, setup::FONT_RELEASE, &[f]);
    }
    renderer_call(m, e, 0x148, &[]);
    e.call(m, setup::LESSON_STOP, &[setup::LESSON]);
    e.call(m, setup::LESSON_RESET, &[setup::LESSON]);
    e.call(m, setup::LISTS_CLEAR, &[SOUND_BANK]);
    e.call(m, setup::POOL_CLEAR, &[0x345e48]);
    let app = m.r32(GAPP);
    m.w32(app + 0x734, 0);
    e.call(m, setup::STACK_DTOR, &[fe + 0xa8, 2]);
    e.call(m, setup::DRAWN_DTOR, &[fe + 0x80, 2]);
    e.call(m, setup::LIGHTS_DTOR2, &[fe + 0x70, 2]);
    crate::game_main::module_dtor(m, e, fe, flags);
}

/// `cvt.s.w`: toward zero
fn int_float(v: u32) -> F {
    let v = v as i32;
    let mut f = v as f32;
    if (f as f64).abs() > (v as f64).abs() {
        f = f32::from_bits(f.to_bits() - 1);
    }
    F(f)
}

/// An unsigned word as a float (`cvt.s.w` of half with the low bit kept, doubled, past 2^31)
fn unsigned_float(v: u32) -> F {
    if (v as i32) >= 0 {
        int_float(v)
    } else {
        let h = int_float((v >> 1) | (v & 1));
        h + h
    }
}

/// `cFE3D` vf4 0x2637a0(fe), the start: the renderer's +0x120 with the career's +0x1684, the viewport over the
/// whole display, its +0x140; the twelve models (+0x20..) loaded, waited on frame by frame (each frame their +0x68)
/// and finished; the scene at +0xc closed (+0x18), destroyed and cleared; both players' +0x4c 10. After a career's
/// first start (+0x3260 1) the two boot films (0x160 bytes each, video screens) made, the intro's first screen the
/// logo's, the logo pushed and +0x3260 2; otherwise the first screen (0x3471fc) pushed. +8 1
pub fn fe_start(m: &mut Mem, e: &mut dyn FeEnv, fe: u32) {
    let v = m.r32(CAREER + 0x1684);
    renderer_call(m, e, 0x120, &[v]);
    let w = unsigned_float(renderer_call(m, e, 0x58, &[]));
    let h = unsigned_float(renderer_call(m, e, 0x60, &[]));
    renderer_call(m, e, 0x98, &[0, 0, w.0.to_bits(), h.0.to_bits()]);
    renderer_call(m, e, 0x140, &[]);
    for k in 0..12u32 {
        let at = fe + 0x20 + 4 * k;
        let o = m.r32(at);
        e.call(m, setup::MODEL_LOAD, &[o]);
        loop {
            let o = m.r32(at);
            // (the first test's a1 is whatever was left there; the rest 0)
            if e.call(m, setup::MODEL_LOADED, &[o]) != 0 {
                break;
            }
            e.call(m, setup::FRAME_WAIT, &[0]);
            let o = m.r32(at);
            vcall_at(m, e, o, 0xc, 0x68, &[]);
        }
        let o = m.r32(at);
        e.call(m, setup::MODEL_FINISH, &[o, 0]);
    }
    let sc = m.r32(fe + 0xc);
    vcall_at(m, e, sc, 0x10, 0x18, &[]);
    let sc = m.r32(fe + 0xc);
    destroy_at(m, e, sc, 0x10);
    m.w32(fe + 0xc, 0);
    for k in 0..2u32 {
        let p = m.r32(fe + 0x74 + 4 * k);
        m.w32(p + 0x4c, 10);
    }
    let first = m.r32(setup::FIRST);
    if m.r32(CAREER + 0x3260) == 1 {
        let film = |m: &mut Mem, e: &mut dyn FeEnv, name: u32| {
            let v = alloc(m, e, setup::FILM_NAME, 0x160, 0x10);
            e.call(m, BASE_CTOR, &[v]);
            m.w32(v + 0x154, 0);
            m.w32(v + 0x158, 1);
            m.w32(v + 0xc, VIDEO_VTABLE);
            m.w32(v + 0x15c, 0);
            e.call(m, SET_FILM, &[v, name]);
            v
        };
        let logo = film(m, e, setup::FILMS[0]);
        let first = m.r32(setup::FIRST);
        m.w32(logo + 0x15c, first);
        vcall_at(m, e, logo, 0xc, 0x60, &[]);
        let intro = film(m, e, setup::FILMS[1]);
        m.w32(fe + 0xf8, intro);
        let v = m.r32(fe + 0xf8);
        e.call(m, setup::FIRST_SCREEN, &[v, 0]);
        let v = m.r32(fe + 0xf8);
        m.w32(v + 0x15c, logo);
        let v = m.r32(fe + 0xf8);
        vcall_at(m, e, v, 0xc, 0x60, &[]);
        let v = m.r32(fe + 0xf8);
        e.call(m, STACK_PUSH, &[fe + 0xa8, v]);
        m.w32(CAREER + 0x3260, 2);
    } else {
        e.call(m, STACK_PUSH, &[fe + 0xa8, first]);
    }
    m.w32(fe + 8, 1);
}

/// The calls of the 19 objects' set-up (`cFE3D` vf1 0x263b30, board row G9b2d2b): the block pool's set-up
/// (`BlockMan_Init` 0x23d160 on 0x345e48 with 4 and 0x3a8d78), the world lists' (`BxList_Init` 0x17d2f0 on 0x337c60
/// with 0xe), a player's pad word (`FE3D_SetPadDeviceWord8` 0x26cf28), the animations' archive
/// (`AnimData_OpenBig` 0x158ea0 on 0x3211a8), the lens effects (`LensFX_Create` 0x1ea408), the video manager
/// (`cVideoStreamMan::cVideoStreamMan` 0x1ffd80) and its streams (`VideoPlayer_AddStream` 0x1ffe70), the fonts
/// (`Font_ParseSfn` 0x19c858), the front end's archive (`BigArchive_Init` / `_Open` / `_Close` / `_Delete` 0x23bb70 /
/// 0x23bc28 / 0x23bcc0 / 0x23bb98) and the course's render data from it (`Course_LoadRenderData` 0x25edf0 on
/// 0x346488), the start script (`World_RunScript` 0x13bad8), the fog (`cFogObjMan::cFogObjMan` 0x1c9b20), the
/// sprites (`SpriteSet_InitFrontEndRects` 0x1f1580, `HUD_LoadIconPack` 0x1f9970, `HUD_CreateDummyTextures` 0x1f9f38),
/// the riders' archives (`cMemoryBigFile::cMemoryBigFile` 0x23c9d0, `cAsyncBigFile::cAsyncBigFile` 0x23bd10,
/// `AsyncBdrLoad_Init` / `_SetMode` / `_SetManagers` 0x201b50 / 0x201788 / 0x201c10), the career's profiles
/// (`Career_CountProfiles` 0x164288, `Career_GetPlayerSlot` 0x164318, `Career_GetPlayerProfile` 0x164330), the
/// characters (`FE3DCharacter_Init` 0x265ed8, `FE3DCharSel_RestoreChoice` 0x267b18), the camera
/// (`cFE3DCameraCombined::cFE3DCameraCombined` 0x26b3f8), the drawn list's add (`List_PushBack` 0x2ac140), a group's
/// list (`List_Init` 0x2abfb0) and add (`FE3D_AddWidget` 0x2abc68), the name hash (`Name_Hash` 0x240ed8), the models
/// (`FE3DGeometry_BindInstance` 0x269a70, `FE3DGeometry_GetPos` 0x269ad0, `cFE3DGeometry::~cFE3DGeometry` 0x26cc40,
/// `FE3DNode_RefreshModel` 0x269be8), the 3D scene (`cFE3DSound::cFE3DSound` 0x2702e0), the fade in
/// (`FE3DTransition_Start` 0x1ff648) and the title screen (`cFE3DStateTitle::cFE3DStateTitle` 0x293390)
pub mod init {
    pub const BLOCKS_INIT: u32 = 0x23d160;
    pub const BLOCKS: u32 = 0x345e48;
    pub const BLOCKS_NAME: u32 = 0x3a8d78;
    pub const LISTS_INIT: u32 = 0x17d2f0;
    pub const PAD_WORD: u32 = 0x26cf28;
    pub const ANIM_OPEN: u32 = 0x158ea0;
    pub const ANIM_DATA: u32 = 0x3211a8;
    pub const LENS_FX: u32 = 0x1ea408;
    pub const VIDEO_CTOR: u32 = 0x1ffd80;
    pub const VIDEO_ADD: u32 = 0x1ffe70;
    pub const FONT_PARSE: u32 = 0x19c858;
    pub const BIG_INIT: u32 = 0x23bb70;
    pub const BIG_OPEN: u32 = 0x23bc28;
    pub const BIG_CLOSE: u32 = 0x23bcc0;
    pub const BIG_DELETE: u32 = 0x23bb98;
    pub const COURSE_LOAD: u32 = 0x25edf0;
    pub const RUN_SCRIPT: u32 = 0x13bad8;
    pub const FOG_CTOR: u32 = 0x1c9b20;
    pub const SPRITE_RECTS: u32 = 0x1f1580;
    pub const ICON_PACK: u32 = 0x1f9970;
    pub const DUMMY_TEXTURES: u32 = 0x1f9f38;
    pub const MEMBIG_CTOR: u32 = 0x23c9d0;
    pub const ASYNCBIG_CTOR: u32 = 0x23bd10;
    pub const BDR_INIT: u32 = 0x201b50;
    pub const BDR_MODE: u32 = 0x201788;
    pub const BDR_MANAGERS: u32 = 0x201c10;
    pub const COUNT_PROFILES: u32 = 0x164288;
    pub const PLAYER_SLOT: u32 = 0x164318;
    pub const PLAYER_PROFILE: u32 = 0x164330;
    pub const CHAR_INIT: u32 = 0x265ed8;
    pub const CHAR_RESTORE: u32 = 0x267b18;
    pub const CAMERA_CTOR: u32 = 0x26b3f8;
    pub const PUSH_BACK: u32 = 0x2ac140;
    pub const LIST_INIT: u32 = 0x2abfb0;
    pub const ADD_WIDGET: u32 = 0x2abc68;
    pub const NAME_HASH: u32 = 0x240ed8;
    pub const BIND: u32 = 0x269a70;
    pub const GET_POS: u32 = 0x269ad0;
    pub const GEOM_DTOR: u32 = 0x26cc40;
    pub const REFRESH: u32 = 0x269be8;
    pub const SOUND_CTOR: u32 = 0x2702e0;
    pub const TRANSITION: u32 = 0x1ff648;
    pub const TITLE_CTOR: u32 = 0x293390;
    /// The heap's word set to 0x10 for the set-up and put back after (0x3bb978)
    pub const HEAP_MODE: u32 = 0x3bb978;
    /// The tables: `cFE3DThing` (the base every object starts with), `cFE3DGroup`, `cFE3DAnimRotate`,
    /// `cFE3DGeometry`, `cFE3DMovieGeometry`
    pub const THING_VT: u32 = 0x3b5a00;
    pub const GROUP_VT: u32 = 0x3b58d0;
    pub const ANIMROT_VT: u32 = 0x3ab910;
    pub const GEOM_VT: u32 = 0x3abbc8;
    pub const MOVIE_VT: u32 = 0x3abae0;
    /// The model names (pointers): the 12 Mac and Marty screen models from 0x3470c8 (the 13th, 0x3470f8, Marty's),
    /// the 12 platforms from 0x347100, the 10 video screens from 0x347130, the 10 medals from 0x347158
    pub const MAC_NAMES: u32 = 0x3470c8;
    pub const MARTY_NAME: u32 = 0x3470f8;
    pub const PLATFORM_NAMES: u32 = 0x347100;
    pub const SCREEN_NAMES: u32 = 0x347130;
    pub const MEDAL_NAMES: u32 = 0x347158;
    /// The scene's flip byte (0x3471f8; also [`super::alternate`]'s)
    pub const SCENE_FLIP: u32 = 0x3471f8;
    /// The names: the pool, the players ("fe3dinput"), the video manager, the two fonts, the archive's ("f3trkbig")
    /// and its path, the start script, the fog's, the four riders' archives' and their three paths, the characters'
    /// ("f3char"), the camera's, a group's ("f3grp"), the characters' group ("char grp"), the turner's ("f3anrot"),
    /// "char", the screen models' ("f3MacMartyScrn"), the medals' and platforms' ("f3medal"), the video screens'
    /// ("f3venmv"), the scene's ("fe3dsound"), the title's ("f3stttl")
    pub const PLAYER_NAME: u32 = 0x3a8db8;
    pub const VIDEO_NAME: u32 = 0x3a8df8;
    pub const FONT_TITLE: u32 = 0x3a8e08;
    pub const FONT_MENU: u32 = 0x3a8e20;
    pub const BIG_NAME: u32 = 0x3a8e48;
    pub const BIG_PATH: u32 = 0x3a8e58;
    pub const START_SCRIPT: u32 = 0x3a8e80;
    pub const FOG_NAME: u32 = 0x3a8ea0;
    pub const MEMBIG_MDL_NAME: u32 = 0x3a8ee0;
    pub const MEMBIG_BRD_NAME: u32 = 0x3a8ef0;
    pub const ASYNCBIG_NAME: u32 = 0x3a8f00;
    pub const BDR_NAME: u32 = 0x3a8f10;
    pub const TEX_PATH: u32 = 0x3a8f20;
    pub const BRD_PATH: u32 = 0x3a8f38;
    pub const MDL_PATH: u32 = 0x3a8f50;
    pub const CHAR_NAME: u32 = 0x3a8f78;
    pub const CAMERA_NAME: u32 = 0x3a8f90;
    pub const GROUP_NAME: u32 = 0x3a8fa8;
    pub const CHARS_GROUP: u32 = 0x3a8fb0;
    pub const ANIMROT_NAME: u32 = 0x3a8fc0;
    pub const CHAR_WORD: u32 = 0x3a8fc8;
    pub const SCREEN_MODEL_NAME: u32 = 0x3a8fe0;
    pub const MEDAL_NAME: u32 = 0x3a8ff0;
    pub const MOVIE_NAME: u32 = 0x3a8ff8;
    pub const SOUND_NAME: u32 = 0x3a9010;
    pub const TITLE_NAME: u32 = 0x3a9030;
}

/// The constructor every front-end object starts with (`cFE3DThing`, written out inline by the compiler): +0 -1, +4
/// and +8 the object itself (an empty list), its table, +0x14 and +0x28 1, +0x60..+0x6c 1.0 (the scale), +0x8c 1.0,
/// the rest of +0x10..+0x98 cleared (+0x34..+0x3c, +0x78, +0x7c left)
fn thing_init(m: &mut Mem, o: u32) {
    let one = 1.0f32.to_bits();
    m.w32(o + 0x40, 0);
    m.w32(o, 0xffff_ffff);
    m.w32(o + 4, o);
    m.w32(o + 8, o);
    m.w32(o + 0xc, init::THING_VT);
    m.w32(o + 0x10, 0);
    m.w32(o + 0x14, 1);
    for at in [0x18u32, 0x1c, 0x20, 0x24] {
        m.w32(o + at, 0);
    }
    m.w32(o + 0x28, 1);
    m.w32(o + 0x2c, 0);
    m.w32(o + 0x30, 0);
    for at in (0x44u32..0x60).step_by(4) {
        m.w32(o + at, 0);
    }
    for at in (0x60u32..0x70).step_by(4) {
        m.w32(o + at, one);
    }
    m.w32(o + 0x70, 0);
    m.w32(o + 0x74, 0);
    for at in [0x80u32, 0x84, 0x88] {
        m.w32(o + at, 0);
    }
    m.w32(o + 0x8c, one);
    for at in [0x90u32, 0x94, 0x98] {
        m.w32(o + at, 0);
    }
}

/// A `cFE3DGeometry` (inline): the base, its table, +0xa0 cleared
fn geometry_init(m: &mut Mem, o: u32) {
    thing_init(m, o);
    m.w32(o + 0xc, init::GEOM_VT);
    m.w32(o + 0xa0, 0);
}

/// A `cFE3DGroup` (0xd0 bytes, inline): the base, its table, its list (+0xa0), its name's hash (+0x74) and its
/// virtual +0x20 with 1
fn group_init(m: &mut Mem, e: &mut dyn FeEnv, o: u32, name: u32) {
    thing_init(m, o);
    m.w32(o + 0xc, init::GROUP_VT);
    e.call(m, init::LIST_INIT, &[o + 0xa0]);
    let h = e.call(m, init::NAME_HASH, &[name]);
    m.w32(o + 0x74, h);
    vcall(m, e, o, 0x20, &[1]);
}

/// A model (`cFE3DGeometry`, 0xb0 bytes, allocated under `tag`) named by the pointer at `names` + 4 `k`: its name's
/// hash, bound to its instance, its virtual +0x28 with 0, refreshed by `how` (f12 0.0) and added to the drawn list
fn model(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, tag: u32, name: u32, how: u32) {
    let o = alloc(m, e, tag, 0xb0, 0);
    geometry_init(m, o);
    let h = e.call(m, init::NAME_HASH, &[name]);
    m.w32(o + 0x74, h);
    e.call(m, init::BIND, &[o, name]);
    vcall(m, e, o, 0x28, &[0]);
    e.call(m, init::REFRESH, &[o, how, 0]);
    e.call(m, init::PUSH_BACK, &[fe + 0x80, o]);
}

/// `abs.s`
fn fabs(v: F) -> F {
    F(f32::from_bits(v.0.to_bits() & 0x7fff_ffff))
}

/// `cFE3D` vf1 0x263b30(fe), the 19 objects' set-up (`buf` 0xd0 bytes of scratch for the stack's vector and its two
/// passing models): the heap's word 0x3bb978 0x10 for the while; the app's front end (+0x734) this one; the block
/// pool and the world lists set up; the two players (0x50 bytes: +0x40 its number, +0x38 / +0x3c 1, not picked, pad
/// word 0) at +0x74 / +0x78; the animations' archive opened, the renderer's +0x138 with 0, the effects loader, the lens
/// effects, the video manager (+0x124) with two streams 0x80 × 0x80; the title and menu fonts parsed and scaled
/// (+0x18 / +0x1c: 1.4 / 1.3 and 1.8 / 1.4); the front end's archive (+0x1c) opened for the course's render data,
/// closed and deleted; the start script run; the fog, the sprites and the icons; the riders' archives (+0xc, +0x10
/// two memory, +0x14 an async one, +0x18 their loader in mode 1) opened on the three paths. The career's profiles give
/// each seated player's character (its +0x44 the profile, the pick byte +0x7d + 2k its place); with none, the fifth
/// character in the order seated alone. The 12 characters (+0x20.., 0x170 bytes, from the order) made and restored;
/// the camera (+0x50) made and drawn; the characters' group with its turner and 12 groups, each holding its
/// character (+0xb8 its number), the platform model's ground distance (√(x² + y²) of its position) kept at +0x60;
/// the 12 screen models (refreshed 2), Marty's screen refreshed 4 on the stack, the 12 platforms (2), the 10 video
/// screens (+0xfc..) and the 10 medals (4) drawn. Without a 3D scene (+0xa4) the scene's flip byte toggled and one
/// made (`cFE3DSound`, its +0x118). By the career's +0x3260: 16 a fade in, then with 1, 2, 4, 8 or 16 the title
/// screen made (0x3471fc); the first screen's +0x60. The app's step (+8) goes 3 to 10 on the way
pub fn fe_init(m: &mut Mem, e: &mut dyn FeEnv, fe: u32, buf: u32) {
    use init::*;
    let one = F(1.0);
    let heap_mode = m.r32(HEAP_MODE);
    m.w32(HEAP_MODE, 0x10);
    let app = m.r32(GAPP);
    m.w32(app + 0x734, fe);
    e.call(m, BLOCKS_INIT, &[BLOCKS, 4, BLOCKS_NAME]);
    e.call(m, LISTS_INIT, &[SOUND_BANK, 0xe]);
    for k in 0..2u32 {
        let p = alloc(m, e, PLAYER_NAME, 0x50, 0);
        m.w32(p + 0x40, k);
        m.w32(p, 0);
        m.w8(p + 0x34, 0);
        m.w32(p + 0x38, 1);
        m.w32(p + 0x3c, 1);
        m.w32(p + 0x48, 0);
        m.w32(p + 0x4c, 0);
        player_reset(m, p);
        m.w32(fe + 0x74 + 4 * k, p);
        e.call(m, PAD_WORD, &[p, 0]);
    }
    e.call(m, ANIM_OPEN, &[ANIM_DATA, 0, 1, 0, 0]);
    let app = m.r32(GAPP);
    m.w32(app + 8, 3);
    renderer_call(m, e, 0x138, &[0]);
    let app = m.r32(GAPP);
    crate::app::create_effects_loader(m, e, app);
    e.call(m, LENS_FX, &[1, 0]);
    let v = alloc(m, e, VIDEO_NAME, 0x28, 0);
    let v = e.call(m, VIDEO_CTOR, &[v, 2]);
    m.w32(fe + 0x124, v);
    e.call(m, VIDEO_ADD, &[v, 0x80, 0x80]);
    let v = m.r32(fe + 0x124);
    e.call(m, VIDEO_ADD, &[v, 0x80, 0x80]);
    let font = m.r32(m.r32(GAPP) + 0x5c);
    e.call(m, FONT_PARSE, &[font, FONT_TITLE, 0]);
    let app = m.r32(GAPP);
    let (f5c, f60) = (m.r32(app + 0x5c), m.r32(app + 0x60));
    m.w32(f5c + 0x1c, 0x3fa6_6666);
    m.w32(f5c + 0x18, 0x3fb3_3333);
    e.call(m, FONT_PARSE, &[f60, FONT_MENU, 0]);
    let app = m.r32(GAPP);
    let f60 = m.r32(app + 0x60);
    m.w32(app + 8, 4);
    m.w32(f60 + 0x1c, 0x3fb3_3333);
    m.w32(f60 + 0x18, 0x3fe6_6666);
    let b = alloc(m, e, BIG_NAME, 0x8c, 0x10);
    let b = e.call(m, BIG_INIT, &[b]);
    m.w32(fe + 0x1c, b);
    e.call(m, BIG_OPEN, &[b, BIG_PATH, 0x10]);
    e.call(m, COURSE_LOAD, &[COURSE, 0xffff_ffff]);
    let b = m.r32(fe + 0x1c);
    e.call(m, BIG_CLOSE, &[b]);
    let b = m.r32(fe + 0x1c);
    if b != 0 {
        e.call(m, BIG_DELETE, &[b, 3]);
    }
    m.w32(fe + 0x1c, 0);
    e.call(m, RUN_SCRIPT, &[START_SCRIPT]);
    let app = m.r32(GAPP);
    m.w32(app + 8, 5);
    let f = alloc(m, e, FOG_NAME, 0x110, 0);
    e.call(m, FOG_CTOR, &[f, 4]);
    for (call, args) in [(SPRITE_RECTS, &[][..]), (ICON_PACK, &[0x2c9, 0][..]), (DUMMY_TEXTURES, &[][..])] {
        let sprites = m.r32(m.r32(GAPP) + 0x728);
        let mut a = vec![sprites];
        a.extend_from_slice(args);
        e.call(m, call, &a);
    }
    let o = alloc(m, e, MEMBIG_MDL_NAME, 0x18, 0x10);
    let o = e.call(m, MEMBIG_CTOR, &[o]);
    m.w32(fe + 0xc, o);
    let o = alloc(m, e, MEMBIG_BRD_NAME, 0x18, 0);
    let o = e.call(m, MEMBIG_CTOR, &[o]);
    m.w32(fe + 0x10, o);
    let o = alloc(m, e, ASYNCBIG_NAME, 0x14, 0);
    let o = e.call(m, ASYNCBIG_CTOR, &[o]);
    m.w32(fe + 0x14, o);
    let o = alloc(m, e, BDR_NAME, 0x3d4, 0);
    let o = e.call(m, BDR_INIT, &[o]);
    m.w32(fe + 0x18, o);
    e.call(m, BDR_MODE, &[o, 1]);
    let a = [m.r32(fe + 0x18), m.r32(fe + 0xc), m.r32(fe + 0x10), m.r32(fe + 0x14)];
    e.call(m, BDR_MANAGERS, &a);
    for (at, path, align) in [(0x14u32, TEX_PATH, 0u32), (0x10, BRD_PATH, 0), (0xc, MDL_PATH, 0x10)] {
        let o = m.r32(fe + at);
        vcall_at(m, e, o, 0x10, 0x10, &[path, align]);
    }
    let app = m.r32(GAPP);
    m.w32(app + 8, 6);
    let mut k = 0u32;
    while (k as i32) < e.call(m, COUNT_PROFILES, &[CAREER]) as i32 {
        let slot = e.call(m, PLAYER_SLOT, &[CAREER, k]);
        if slot != 0 {
            let who = m.r8(slot) as i8 as i32;
            if who >= 0 {
                let p = m.r32(fe + 0x74 + 4 * who as u32);
                m.w8(p + 0x44, k as u8);
                let who = m.r8(slot) as i8 as i32 as u32;
                let c = e.call(m, PLAYER_PROFILE, &[CAREER, who]);
                let place = place_of(m, c as u8 as i8 as i32 as u32);
                m.w8(fe + 0x7c + 2 * k + 1, place as u8);
            }
        }
        k += 1;
    }
    if e.call(m, COUNT_PROFILES, &[CAREER]) == 0 {
        let c = order(m, 4);
        e.call(m, crate::fe_charselect::SEEN, &[CAREER, 0, 0, c]);
        set_players(m, e, fe, 1);
    }
    for k in 0..12u32 {
        let o = alloc(m, e, CHAR_NAME, 0x170, 0);
        let c = order(m, k);
        let ch = e.call(m, CHAR_INIT, &[o, c]);
        m.w32(fe + 0x20 + 4 * k, ch);
        e.call(m, CHAR_RESTORE, &[ch, 0]);
    }
    let app = m.r32(GAPP);
    m.w32(app + 8, 7);
    let o = alloc(m, e, CAMERA_NAME, 0x190, 0);
    let cam = e.call(m, CAMERA_CTOR, &[o]);
    m.w32(fe + 0x50, cam);
    e.call(m, PUSH_BACK, &[fe + 0x80, cam]);
    let chars = alloc(m, e, GROUP_NAME, 0xd0, 0);
    group_init(m, e, chars, CHARS_GROUP);
    e.call(m, PUSH_BACK, &[fe + 0x80, chars]);
    let rot = alloc(m, e, ANIMROT_NAME, 0xd0, 0);
    thing_init(m, rot);
    for at in [0xa0u32, 0xa4, 0xa8, 0xac] {
        m.w32(rot + at, 0);
    }
    m.w32(rot + 0xc, ANIMROT_VT);
    let h = e.call(m, NAME_HASH, &[CHAR_WORD]);
    m.w32(rot + 0x74, h);
    for at in (0xb0u32..0xc8).step_by(4) {
        m.w32(rot + at, 0);
    }
    e.call(m, ADD_WIDGET, &[chars, rot]);
    let (pos, temp) = (buf, buf + 0x20);
    for k in 0..12u32 {
        let g = alloc(m, e, GROUP_NAME, 0xd0, 0);
        group_init(m, e, g, CHAR_WORD);
        e.call(m, ADD_WIDGET, &[chars, g]);
        // (a platform model on the stack, for its position)
        geometry_init(m, temp);
        let name = m.r32(PLATFORM_NAMES);
        e.call(m, BIND, &[temp, name]);
        e.call(m, GET_POS, &[temp, pos]);
        e.call(m, GEOM_DTOR, &[temp, 2]);
        let (x, y) = (fabs(F(f32::from_bits(m.r32(pos)))), fabs(F(f32::from_bits(m.r32(pos + 4)))));
        let d = (x * x + y * y).sqrt();
        let z = m.r32(pos + 8);
        m.w32(fe + 0x60, d.0.to_bits());
        m.w32(fe + 0x64, 0);
        m.w32(fe + 0x68, z);
        m.w32(fe + 0x6c, one.0.to_bits());
        let ch = m.r32(fe + 0x20 + 4 * k);
        m.w32(ch + 0xb8, k);
        e.call(m, ADD_WIDGET, &[g, ch]);
    }
    let app = m.r32(GAPP);
    m.w32(app + 8, 8);
    for k in 0..12u32 {
        let name = m.r32(MAC_NAMES + 4 * k);
        model(m, e, fe, SCREEN_MODEL_NAME, name, 2);
    }
    // (Marty's screen, on the stack: refreshed and gone)
    let marty = buf;
    geometry_init(m, marty);
    let name = m.r32(MARTY_NAME);
    e.call(m, BIND, &[marty, name]);
    e.call(m, REFRESH, &[marty, 4, 0]);
    m.w32(marty + 0x14, 0);
    e.call(m, GEOM_DTOR, &[marty, 2]);
    for k in 0..12u32 {
        let name = m.r32(PLATFORM_NAMES + 4 * k);
        model(m, e, fe, MEDAL_NAME, name, 2);
    }
    for k in 0..10u32 {
        let o = alloc(m, e, MOVIE_NAME, 0xc0, 0);
        thing_init(m, o);
        m.w32(fe + 0xfc + 4 * k, o);
        let name = m.r32(SCREEN_NAMES + 4 * k);
        m.w32(o + 0xa0, 0);
        m.w32(o + 0xb0, 0xffff_ffff);
        m.w32(o + 0xc, MOVIE_VT);
        e.call(m, BIND, &[o, name]);
        let o = m.r32(fe + 0xfc + 4 * k);
        e.call(m, PUSH_BACK, &[fe + 0x80, o]);
    }
    for k in 0..10u32 {
        let name = m.r32(MEDAL_NAMES + 4 * k);
        model(m, e, fe, MEDAL_NAME, name, 4);
    }
    if m.r32(fe + 0xa4) == 0 {
        m.w8(SCENE_FLIP, m.r8(SCENE_FLIP) ^ 1);
        let o = alloc(m, e, SOUND_NAME, 8, 0);
        let flip = m.r8(SCENE_FLIP) as i8 as i32 as u32;
        let s = e.call(m, SOUND_CTOR, &[o, flip]);
        m.w32(fe + 0xa4, s);
        vcall_at(m, e, s, 0, 0x118, &[]);
    }
    let app = m.r32(GAPP);
    m.w32(app + 8, 9);
    let mode = m.r32(CAREER + 0x3260);
    if mode == 16 {
        for k in 0..3 {
            m.w32(buf + 4 * k, 0);
        }
        let fades = m.r32(m.r32(GAPP) + 0x72c);
        e.call(m, TRANSITION, &[fades, buf, 3, 0, one.0.to_bits(), 0.5f32.to_bits()]);
    }
    if matches!(mode, 1 | 2 | 4 | 8 | 16) {
        let o = alloc(m, e, TITLE_NAME, 0x5c, 0);
        let t = e.call(m, TITLE_CTOR, &[o]);
        m.w32(setup::FIRST, t);
    }
    let first = m.r32(setup::FIRST);
    vcall(m, e, first, 0x60, &[]);
    m.w32(HEAP_MODE, heap_mode);
    let app = m.r32(GAPP);
    m.w32(app + 8, 10);
}
