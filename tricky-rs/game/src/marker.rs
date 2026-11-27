//! The player marker over a rider in a race of two or more players (board row G7g5): `PlayerMarker_Draw`
//! 0x133c20. On the bytes at their addresses ([`Mem`]); the renderer's virtuals are calls out
//! (`particledraw::ParticleDrawEnv`). Checked in the function runner (`tests/marker.rs`).
//!
//! **A marker**: +0..+0x3f its four points, +0x20 also where it is (the third point), +0xc0 its eight vertices
//! (0x30 each, the places at +0x20: the corners are written there), +0x24c its height.

use crate::animseq::div;
use crate::particledraw::{vcall, ParticleDrawEnv, APP, BIAS};
use crate::pf::Mem;
use crate::ps2f::F;

type V = [F; 4];

fn rf(m: &Mem, a: u32) -> F {
    F(f32::from_bits(m.r32(a)))
}
fn rq(m: &Mem, a: u32) -> V {
    [0, 1, 2, 3].map(|k| rf(m, a.wrapping_add(4 * k)))
}
fn wq(m: &mut Mem, a: u32, v: V) {
    for k in 0..4 {
        m.w32(a + 4 * k, v[k as usize].0.to_bits());
    }
}
fn f(bits: u32) -> F {
    F(f32::from_bits(bits))
}

/// `PlayerMarker_Draw` 0x133c20(marker): the marker's texture (the app's +0x30 → +0x84) and the renderer's state
/// for it, then the marker as a strip of eight corners facing the camera:
/// - its size by its depth (the third point, +0x20, through the view's third row): 76.49 at 0 or nearer, then
///   76.49 + 63.37 × z / 31128.5, 139.86 from 31128.5 on;
/// - the camera's first column times the size, and its third column times the height (+0x24c), each w 0;
/// - each point less then plus the first, plus the second, written at the vertices' places (+0xe0, +0x110, …);
/// - the renderer's +0x220 (8, the vertices, 0), then the state back.
pub fn draw(m: &mut Mem, env: &mut dyn ParticleDrawEnv, marker: u32) {
    let app = m.r32(APP);
    let r = m.r32(app + 0x724);
    let tex = m.r32(m.r32(app + 0x30) + 0x84);
    vcall(m, env, r, 0x1f0, &[tex, 0]);
    vcall(m, env, r, 0x1c8, &[0, 0, BIAS]);
    vcall(m, env, r, 0x1c0, &[1]);
    vcall(m, env, r, 0x1e8, &[2, 0]);
    vcall(m, env, r, 0x1e0, &[3]);
    let view = vcall(m, env, r, 0xf8, &[]);
    let v = [0, 1, 2, 3].map(|i| rq(m, view.wrapping_add(0x10 * i)));
    let p = rq(m, marker + 0x20);
    let z = ((p[0] * v[2][0] + p[1] * v[2][1]) + p[2] * v[2][2]) + p[3] * v[2][3];
    let zero = F(0.0);
    let size = if z <= zero {
        f(0x4298_f9a5)
    } else if f(0x46f3_3115) <= z {
        f(0x430b_db37)
    } else {
        div(z, f(0x46f3_3115)) * f(0x427d_7992) + f(0x4298_f9a5)
    };
    // the first column ((1, 0, 0, 0) through the view, the 1 folded) and the third ((0, 0, 1, 0))
    let side: V = v.map(|row| ((row[0] + row[1] * zero) + row[2] * zero) + row[3] * zero);
    let up: V = v.map(|row| ((zero * row[0] + zero * row[1]) + F(1.0) * row[2]) + row[3] * zero);
    let mut side = side.map(|x| x * size);
    side[3] = zero;
    let height = rf(m, marker + 0x24c);
    let mut up = up.map(|x| x * height);
    up[3] = zero;
    for k in 0..4u32 {
        let pt = rq(m, marker + 0x10 * k);
        let a: V = [0, 1, 2, 3].map(|j| (pt[j] - side[j]) + up[j]);
        wq(m, marker + 0xe0 + 0x60 * k, a);
        let pt = rq(m, marker + 0x10 * k);
        let b: V = [0, 1, 2, 3].map(|j| (pt[j] + side[j]) + up[j]);
        wq(m, marker + 0x110 + 0x60 * k, b);
    }
    vcall(m, env, r, 0x220, &[8, marker + 0xc0, 0]);
    vcall(m, env, r, 0x1e0, &[3]);
    vcall(m, env, r, 0x1c8, &[0, 2, BIAS]);
    vcall(m, env, r, 0x1c0, &[0]);
    vcall(m, env, r, 0x1e8, &[0, 0]);
}

/// `PlayerMarker_Construct` 0x133808(marker): initializes the ease record at `+0x254` to 0 (`EaseRecord_Init`
/// 0x1230e0), calls [`reset`], and returns `marker`
pub fn construct(m: &mut Mem, marker: u32) -> u32 {
    m.w32(marker + 0x254, 0);
    m.w32(marker + 0x258, 0);
    m.w32(marker + 0x25c, 0);
    reset(m, marker);
    marker
}

/// `PlayerMarker_Reset` 0x133890(marker): sets the 8 quad slots at `+0x40` to `(0, 0, 0, 1)`, sets `+0xc8 + 0x30*k`
/// to `1.0` for `k in 0..8`, writes the 5 geometry constants at `+0x240..+0x250`, and zeroes `+0x254..+0x25c`
pub fn reset(m: &mut Mem, marker: u32) {
    let one = F(1.0);
    let zero = F(0.0);
    for k in 0..8u32 {
        wq(m, marker + 0x40 + 0x10 * k, [zero, zero, zero, one]);
        m.w32(marker + 0xc8 + 0x30 * k, 0x3f80_0000);
    }
    m.w32(marker + 0x240, 0x469c_4000);
    m.w32(marker + 0x244, 0xc302_e88f);
    m.w32(marker + 0x248, 0x4304_8afc);
    m.w32(marker + 0x24c, 0x424a_0000);
    m.w32(marker + 0x250, 0x4396_10b0);
    m.w32(marker + 0x25c, 0);
    m.w32(marker + 0x254, 0);
    m.w32(marker + 0x258, 0);
}

/// `PlayerMarker_Update` 0x133940(marker, rider): positions the 4 marker control points above bone 4 (`mats + 0x130`),
/// computes strip UVs from `+0x240/+0x248/+0x250`, tints the 8 vertices red (`0x80, 0, 0`) for player 0 or gold
/// (`0x80, 0x6e, 0`) otherwise (`*(rider+0x464)+0x65`), steps the `+0x254` pulse ease record between 20 and 40 at
/// rate `45.192516`, and writes the truncated integer alpha to the middle 4 vertices (0 to the outer 4)
pub fn update(m: &mut Mem, marker: u32, rider: u32) {
    let skel = m.r32(rider + 0x4494);
    let mats = if skel == 0 { 0 } else { m.r32(skel + 4) };
    let head = rq(m, mats.wrapping_add(0x130));
    let zero = F(0.0);
    let one = F(1.0);
    let neg_244 = F(-rf(m, marker + 0x244).0);
    let f2 = rf(m, marker + 0x248);
    let neg_248 = F(-f2.0);
    let p1: V = [head[0] + zero, head[1] + zero, head[2] + neg_244, head[3] + zero];
    wq(m, marker + 0x10, p1);
    let f6 = rf(m, marker + 0x240);
    let f1 = f6 * F(0.5);
    let f5 = f6 + f2;
    let p0: V = [p1[0] + zero, p1[1] + zero, p1[2] + neg_248, p1[3] + zero];
    wq(m, marker, p0);
    let p2: V = [p1[0] + zero, p1[1] + zero, p1[2] + f1, p1[3] + zero];
    wq(m, marker + 0x20, p2);
    let denom = rf(m, marker + 0x250);
    let v5 = div(f5, denom);
    let v2 = div(f2, denom);
    let v1 = div(f1, denom);
    let p3: V = [p1[0] + zero, p1[1] + zero, p1[2] + f6, p1[3] + zero];
    wq(m, marker + 0x30, p3);
    let u2 = v5 - v2;
    let u1 = u2 - v1;
    let wf = |m: &mut Mem, a: u32, v: F| m.w32(a, v.0.to_bits());
    wf(m, marker + 0xf4, v5);
    wf(m, marker + 0x210, one);
    wf(m, marker + 0x214, zero);
    wf(m, marker + 0x154, u2);
    wf(m, marker + 0x1b4, u1);
    wf(m, marker + 0xc0, zero);
    wf(m, marker + 0xc4, v5);
    wf(m, marker + 0xf0, one);
    wf(m, marker + 0x120, zero);
    wf(m, marker + 0x124, u2);
    wf(m, marker + 0x150, one);
    wf(m, marker + 0x180, zero);
    wf(m, marker + 0x184, u1);
    wf(m, marker + 0x1b0, one);
    wf(m, marker + 0x1e0, zero);
    wf(m, marker + 0x1e4, zero);
    let part = m.r32(rider + 0x464);
    let pid = m.r8(part + 0x65) as i8;
    let g = if pid == 0 { 0u32 } else { 0x6e };
    for k in 0..8u32 {
        let base = marker + 0x30 * k;
        m.w32(base + 0xd0, 0x80);
        m.w32(base + 0xd4, g);
        m.w32(base + 0xd8, 0);
    }
    let cur = rf(m, marker + 0x254);
    let lo = F(20.0);
    let hi = F(40.0);
    let rate_val = f(0x4234_c523);
    if cur <= lo {
        wf(m, marker + 0x25c, hi);
        wf(m, marker + 0x258, rate_val);
    } else if hi <= cur {
        wf(m, marker + 0x25c, lo);
        wf(m, marker + 0x258, rate_val);
    }
    let cur = rf(m, marker + 0x254);
    let rate = rf(m, marker + 0x258);
    let target = rf(m, marker + 0x25c);
    let step = rate * f(0x3c88_8889);
    let next = if target + step < cur {
        cur - step
    } else if cur < target - step {
        cur + step
    } else {
        target
    };
    wf(m, marker + 0x254, next);
    let alpha = rf(m, marker + 0x254).0 as i32 as u32;
    for k in 0..4u32 {
        m.w32(marker + 0x13c + 0x30 * k, alpha);
    }
    m.w32(marker + 0x22c, 0);
    m.w32(marker + 0xdc, 0);
    m.w32(marker + 0x10c, 0);
    m.w32(marker + 0x1fc, 0);
}
