//! The board's frame exactly as the original builds it (board row G16): `Boarder_UpdatePose` 0x129628 up to its
//! call of `Boarder_UpdateSkeleton` (the rest poses the skeleton: animation, row G5), and after it the frame's
//! position moved to the pivot bone ([`frame_where`], F4b3t). The frame (rider+0x4aa0.., four rows: the board's x, y
//! and z axes and the position) is what the air tick's down ray, the rail code, the shove and the drawing read. Checked bit for bit in the function runner (`tests/pose.rs`).

type V4 = [f32; 4];

/// What `Boarder_UpdatePose` reads to build the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoseIn {
    /// +0x170: the attitude quaternion (w, x, y, z)
    pub quat: V4,
    /// +0x214 the steer, +0x424 the motion (3: the rail, no steering tilt), +0x420 the board class
    pub steer: f32,
    pub motion: u32,
    pub class: u32,
    /// +0x140 the position, +0x2c0 the rider's up, +0x308 the body's height above the snow
    pub pos: V4,
    pub up: V4,
    pub height: f32,
    /// +0x330 the board's side on the ground, +0x244 the lean value, stats+0x60 its scale
    pub side: V4,
    pub lean: f32,
    pub lean_scale: f32,
    /// +0x1d0, +0x1d4: the trick's spin and flip; the pivot bone's height (rider + +0x44d8 × 0x394 + 0x8d0)
    pub spin: f32,
    pub flip: f32,
    pub pivot_z: f32,
}

/// The frame times a matrix given by rows (vmulabc / vmaddabc / vmaddbc: each new row is the frame's rows weighted by
/// the matrix row's four lanes, summed in order).
fn mul(f: [V4; 4], m: [V4; 4]) -> [V4; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for (o, r) in out.iter_mut().zip(m.iter()) {
        *o = apply(f, *r);
    }
    out
}

/// The frame applied to one vector (x, y, z, w).
fn apply(f: [V4; 4], v: V4) -> V4 {
    let mut o = [0.0f32; 4];
    for i in 0..4 {
        o[i] = ((f[0][i] * v[0] + f[1][i] * v[1]) + f[2][i] * v[2]) + f[3][i] * v[3];
    }
    o
}

/// `Boarder_UpdatePose` 0x129628 to its `Boarder_UpdateSkeleton` call: the frame from the quaternion; tilted by the
/// steer about the board's y axis (angle steer × 1.134464 on an alpine board, else × 0.87266463; not on a rail, nor
/// with no steer); its position the rider's position + height · up + (lean scale · lean) · side; with a spin or
/// flip, turned by them (spin about z, then flip about x) about the pivot bone's height.
pub fn board_frame(p: &PoseIn) -> [V4; 4] {
    let (w, x, y, z) = (p.quat[0], p.quat[1], p.quat[2], p.quat[3]);
    let y2 = y + y;
    let z2 = z + z;
    let wx2 = w * (x + x);
    let xx = 1.0 - x * (x + x);
    let mut f = [
        [(1.0 - y * y2) - z * z2, x * y2 + w * z2, x * z2 - w * y2, 0.0],
        [x * y2 - w * z2, xx - z * z2, y * z2 + wx2, 0.0],
        [x * z2 + w * y2, y * z2 - wx2, xx - y * y2, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let z0 = f[3][0];
    if p.steer != z0 && p.motion != 3 {
        let k = if p.class == 2 { f32::from_bits(0x3f91_361e) } else { f32::from_bits(0x3f5f_66f3) };
        let (s, c) = crate::math::sin_cos(p.steer * k);
        let t = 1.0 - c;
        let (f23, f31, f14) = (t * z0, t * z0, t * 1.0);
        let r = [
            [f31 * z0 + c, f14 * z0 - s * z0, f23 * z0 + s * 1.0, z0],
            [f31 * 1.0 + s * z0, f14 * 1.0 + c, f23 * 1.0 - s * z0, z0],
            [f31 * z0 - s * 1.0, f14 * z0 + s * z0, f23 * z0 + c, z0],
            [z0, z0, z0, 1.0],
        ];
        f = mul(f, r);
    }
    let a = [p.up[0] * p.height, p.up[1] * p.height, p.up[2] * p.height, p.up[3] * p.height];
    let pos = [p.pos[0] + a[0], p.pos[1] + a[1], p.pos[2] + a[2], p.pos[3] + a[3]];
    let k = p.lean_scale * p.lean;
    let b = [p.side[0] * k, p.side[1] * k, p.side[2] * k, p.side[3] * k];
    f[3] = [pos[0] + b[0], pos[1] + b[1], pos[2] + b[2], pos[3] + b[3]];
    if p.spin != 0.0 || p.flip != 0.0 {
        let bz = p.pivot_z;
        f[3] = apply(f, [0.0, 0.0, bz, 1.0]);
        let (s, c) = crate::math::sin_cos(p.spin);
        let t = 1.0 - c;
        let (f16, f23, f14) = (t * 1.0, t * 0.0, t * 0.0);
        let r = [
            [f23 * 0.0 + c, f14 * 0.0 - s * 1.0, f16 * 0.0 + s * 0.0, 0.0],
            [f23 * 0.0 + s * 1.0, f14 * 0.0 + c, f16 * 0.0 - s * 0.0, 0.0],
            [f23 * 1.0 - s * 0.0, f14 * 1.0 + s * 0.0, f16 * 1.0 + c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        f = mul(f, r);
        let (s, c) = crate::math::sin_cos(p.flip);
        let t = 1.0 - c;
        let (f16, f23, f14) = (t * 0.0, t * 1.0, t * 0.0);
        let r = [
            [f23 * 1.0 + c, f14 * 1.0 - s * 0.0, f16 * 1.0 + s * 0.0, 0.0],
            [f23 * 0.0 + s * 0.0, f14 * 0.0 + c, f16 * 0.0 - s * 1.0, 0.0],
            [f23 * 0.0 - s * 0.0, f14 * 0.0 + s * 1.0, f16 * 0.0 + c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        f = mul(f, r);
        f[3] = apply(f, [0.0, 0.0, -bz, 1.0]);
    }
    f
}

/// What `Boarder_UpdatePose` 0x129628 does to the frame's position (+0x4ad0) after its `Boarder_UpdateSkeleton`
/// call, for a rider whose stats+0x78 is 0 (every rider of every recording; the other path turns the frame by
/// +0x168 and is not ported). `pivot` is the pivot bone's position (rider + +0x44d8 × 0x394 + 0x8c8, x y z), read
/// before the skeleton is posed, so the last frame's. Unless the layer-0 clip has the skin's children follow the
/// frame (`skin_child`: state +0x428 = 1 and the clip not 0x1fc, or the clip 0x291 or 0x2ab, or of type 0x10), the
/// position first moves by the frame's rows times ((pivot + up·lift) − pivot, then w + 1), where `lift` is the
/// rider's up (+0x2c0) × +0x1c0 × stats+0x60 in motions 2 and 4 and 0 otherwise (each lane, in that order). Then
/// always: the position is the pivot through the frame (the hips, ~67 cm above the board). The rows are left as
/// [`board_frame`] built them. The skin calls between (`Boarder_GetBoneWorldPos` 0x208398,
/// `Boarder_BuildBoneMatrices` 0x2083f8, `Boarder_SetSkinChildMatrices` 0x2083e8) only read the frame.
pub fn frame_where(f: [V4; 4], pivot: [f32; 3], skin_child: bool, motion: u32, up: V4, x1c0: f32, scale: f32) -> V4 {
    let p1 = [pivot[0], pivot[1], pivot[2], 1.0];
    let mut f = f;
    if !skin_child {
        let mut b = p1;
        if motion == 2 || motion == 4 {
            for k in 0..4 {
                b[k] = b[k] + (up[k] * x1c0) * scale;
            }
        }
        let mut c = [0.0f32; 4];
        for k in 0..4 {
            c[k] = (b[k] - p1[k]) + [0.0, 0.0, 0.0, 1.0][k];
        }
        f[3] = apply(f, c);
    }
    apply(f, p1)
}

/// The body's height above the snow one tick on (rider+0x308; `cBoarder::vf7` 0x117198): toward the surface table's
/// word 10 in ground motion (rider+0x424 = 2), toward 0 otherwise, at most 1.6666666 cm a tick.
pub fn ease_body_height(height: f32, motion: u32, surface_word10: f32) -> f32 {
    let target = if motion == 2 { surface_word10 } else { 0.0 };
    let step = f32::from_bits(0x3fd5_5555);
    if target + step < height {
        height - step
    } else if height < target - step {
        height + step
    } else {
        target
    }
}

/// The board's four corners (rider+0x5760 front left, +0x5770 front right, +0x5780 back left, +0x5790 back right, as
/// the board track reads them) as `Boarder_UpdatePose` 0x129628 makes them after `Boarder_UpdateSkeleton`: from the
/// board bone's position (`bone[3]`, +0x4a90), its x row (+0x4a60, along the board) and z row (+0x4a80, across), the
/// board class (+0x420: 0 96.09368 / 98.02858 cm to the ends and 16.22651 half wide, 1 84.922966 / 84.07263 /
/// 16.232784, else 114.15547 / 124.23839 / 15.998993; the two lengths swapped for the other
/// natural stance, stats+0x40) times stats+0x60 (`Vec4_Scale` 0x102f50: all four lanes)
pub fn board_corners(bone: [V4; 4], class: u32, natural_stance: u32, scale: f32) -> [V4; 4] {
    let (a, b, w) = match class {
        0 => (f32::from_bits(0x42c0_2ff7), f32::from_bits(0x42c4_0ea2), f32::from_bits(0x4181_cfe4)),
        1 => (f32::from_bits(0x42a9_d88f), f32::from_bits(0x42a8_2530), f32::from_bits(0x4181_dcbe)),
        _ => (f32::from_bits(0x42e4_4f9a), f32::from_bits(0x42f8_7a0e), f32::from_bits(0x417f_fbe0)),
    };
    let (front, back) = if natural_stance != 0 { (b, a) } else { (a, b) };
    let mul = |v: V4, k: f32| [v[0] * k, v[1] * k, v[2] * k, v[3] * k];
    let add = |p: V4, q: V4| [p[0] + q[0], p[1] + q[1], p[2] + q[2], p[3] + q[3]];
    let sub = |p: V4, q: V4| [p[0] - q[0], p[1] - q[1], p[2] - q[2], p[3] - q[3]];
    let pos = bone[3];
    let fwd = add(pos, mul(bone[0], front * scale));
    let rear = sub(pos, mul(bone[0], back * scale));
    let half = mul(bone[2], w * scale);
    [add(fwd, half), sub(fwd, half), add(rear, half), sub(rear, half)]
}

use crate::animcurve::sin_cos as sin_cos_ee;
use crate::animseq::{div as div_ee, sqrt as sqrt_ee};
use crate::pf::Mem;
use crate::ps2f::F;

type VF4 = [F; 4];
type MF4 = [VF4; 4];

fn rf(m: &Mem, a: u32) -> F {
    F(f32::from_bits(m.r32(a)))
}
fn wf(m: &mut Mem, a: u32, v: F) {
    m.w32(a, v.0.to_bits());
}
fn rv(m: &Mem, a: u32) -> VF4 {
    [0, 1, 2, 3].map(|k| rf(m, a + 4 * k))
}
fn wv(m: &mut Mem, a: u32, v: VF4) {
    for (k, x) in v.into_iter().enumerate() {
        wf(m, a + 4 * k as u32, x);
    }
}
fn rm(m: &Mem, a: u32) -> MF4 {
    [0, 1, 2, 3].map(|r| rv(m, a + 0x10 * r))
}
fn wm(m: &mut Mem, a: u32, v: &MF4) {
    for (r, row) in v.iter().enumerate() {
        wv(m, a + 0x10 * r as u32, *row);
    }
}
fn apply_f(a: &MF4, v: VF4) -> VF4 {
    [0, 1, 2, 3].map(|i| ((a[0][i] * v[0] + a[1][i] * v[1]) + a[2][i] * v[2]) + a[3][i] * v[3])
}
fn mul_f(a: &MF4, b: &MF4) -> MF4 {
    b.map(|row| apply_f(a, row))
}
fn dot4_f(a: VF4, b: VF4) -> F {
    let p = [0, 1, 2, 3].map(|k| a[k] * b[k]);
    ((p[0] + p[1]) + F(1.0) * p[2]) + F(1.0) * p[3]
}
fn cross_f(a: VF4, b: VF4) -> VF4 {
    [a[1] * b[2] - b[1] * a[2], a[2] * b[0] - b[2] * a[0], a[0] * b[1] - b[0] * a[1], F(0.0)]
}
fn mul3x3_rows(a: [[F; 3]; 3], b: [[F; 3]; 3]) -> [[F; 3]; 3] {
    // Matches 0x11aa80..0x11ae5c and 0x12a788..0x12ab60: out[r][c] = (b[0][c]*a[r][0] + b[1][c]*a[r][1]) + b[2][c]*a[r][2]
    let mut out = [[F(0.0); 3]; 3];
    for c in 0..3 {
        for r in 0..3 {
            out[r][c] = (b[0][c] * a[r][0] + b[1][c] * a[r][1]) + b[2][c] * a[r][2];
        }
    }
    out
}

/// `Boarder_UpdateSkeleton` 0x12ae18(rider, on_snow): sets the board part's world matrix from `rider+0x4aa0` via
/// `RiderSkin_SetRootMatrix` 0x2083d8; in ground motion (`rider+0x424 == 2`), tilts the board so its x row is
/// orthogonal to `rider+0x2c0` (up) and, if `rider+0x1cc > 0`, shifts its position along `rider+0x2a0` (normal) ×
/// `+0x1cc`; finally copies the 4×4 board bone world matrix to `rider+0x4a60` (`board_bone_frame`).
pub fn update_skeleton(m: &mut Mem, rider: u32) {
    crate::riderskin::set_root_matrix(m, rider + 0x79c, rider + 0x4aa0);
    let board_part = m.r32(rider + 0x4498);
    let board_mat = if board_part != 0 { m.r32(board_part + 4) } else { 0 };
    if board_mat == 0 {
        return;
    }
    if m.r32(rider + 0x424) == 2 {
        let up = rv(m, rider + 0x2c0);
        let x = rv(m, board_mat);
        let axis = cross_f(up, x);
        let alen = sqrt_ee(dot4_f(axis, axis));
        if F(f32::from_bits(0x322b_cc77)) < alen {
            let inv = div_ee(F(1.0), alen);
            let n = axis.map(|v| v * inv);
            let lim = F(f32::from_bits(0x3f7f_ff58));
            let c_alen = if lim < alen { lim } else { alen };
            let d = dot4_f(up, x);
            let asin_v = crate::math::asin_ee(c_alen);
            let half_pi = F(f32::from_bits(0x3fc9_0fdb));
            let theta = if F(0.0) < d { -(half_pi - asin_v) } else { half_pi - asin_v };
            let (s, c) = sin_cos_ee(theta);
            let b = rm(m, board_mat);
            let saved = [b[0], b[1], b[2], [F(0.0), F(0.0), F(0.0), F(1.0)]];
            let (ax, ay, az) = (n[0], n[1], n[2]);
            let one_c = F(1.0) - c;
            let (f2, f3, f4) = (one_c * ax, one_c * ay, one_c * az);
            let (f8, f5, f7) = (s * ax, s * ay, s * az);
            let rot = [
                [f2 * ax + c, f3 * ax - f7, f4 * ax + f5, F(0.0)],
                [f2 * ay + f7, f3 * ay + c, f4 * ay - f8, F(0.0)],
                [f2 * az - f5, f3 * az + f8, f4 * az + c, F(0.0)],
                b[3],
            ];
            let res = mul_f(&rot, &saved);
            wm(m, board_mat, &res);
        }
        let lift = rf(m, rider + 0x1cc);
        if F(0.0) < lift {
            let norm = rv(m, rider + 0x2a0);
            let base = [F(0.0), F(0.0), F(0.0), F(1.0)];
            let v = [0, 1, 2, 3].map(|k| base[k] + norm[k] * lift);
            let b = rm(m, board_mat);
            let p = apply_f(&b, v);
            wv(m, board_mat + 0x30, p);
        }
    }
    let b = rm(m, board_mat);
    wm(m, rider + 0x4a60, &b);
}

/// Captures the rider's bind-pose foot-to-board 3×3 rotation matrices at `rider+0x3d0` (right foot, bone 15) and
/// `rider+0x3f4` (left foot, bone 18) as `cBoarder::Init` 0x11a8b8..0x11ae5c computes them after posing clip
/// `0x1ff` at identity: `foot_local = foot_rot * board_rot^T`.
pub fn init_foot_bindings(m: &mut Mem, rider: u32, ident_addr: u32) {
    crate::riderskin::set_root_matrix(m, rider + 0x79c, ident_addr);
    crate::riderskin::set_skin_child_matrices(m, rider + 0x79c, ident_addr);
    let body_part = m.r32(rider + 0x4494);
    let board_part = m.r32(rider + 0x4498);
    if body_part == 0 || board_part == 0 {
        return;
    }
    let body_mats = m.r32(body_part + 4);
    let board_mat = m.r32(board_part + 4);
    let b = rm(m, board_mat);
    let bt = [
        [b[0][0], b[1][0], b[2][0]],
        [b[0][1], b[1][1], b[2][1]],
        [b[0][2], b[1][2], b[2][2]],
    ];
    for &(bone_off, dst_off) in &[(0x3c0u32, 0x3d0u32), (0x480u32, 0x3f4u32)] {
        let f = rm(m, body_mats + bone_off);
        let f3 = [[f[0][0], f[0][1], f[0][2]], [f[1][0], f[1][1], f[1][2]], [f[2][0], f[2][1], f[2][2]]];
        let rel = mul3x3_rows(f3, bt);
        for r in 0..3u32 {
            for c in 0..3u32 {
                wf(m, rider + dst_off + (r * 3 + c) * 4, rel[r as usize][c as usize]);
            }
        }
    }
}

/// Applies the stored foot-to-board binding 3×3 matrices (`rider+0x3d0` -> bone 15 `r_foot` at `+0x3c0`,
/// `rider+0x3f4` -> bone 18 `l_foot` at `+0x480`) against the current board world matrix (`Boarder_UpdatePose`
/// 0x12a6c4..0x12abf4), keeping both boots locked flat to the snowboard bindings after 2-bone leg IK.
pub fn apply_foot_bindings(m: &mut Mem, rider: u32) {
    let body_part = m.r32(rider + 0x4494);
    let board_part = m.r32(rider + 0x4498);
    if body_part == 0 || board_part == 0 {
        return;
    }
    let body_mats = m.r32(body_part + 4);
    let board_mat = m.r32(board_part + 4);
    let b = rm(m, board_mat);
    let b3 = [[b[0][0], b[0][1], b[0][2]], [b[1][0], b[1][1], b[1][2]], [b[2][0], b[2][1], b[2][2]]];
    for &(src_off, bone_off) in &[(0x3d0u32, 0x3c0u32), (0x3f4u32, 0x480u32)] {
        let mut l = [[F(0.0); 3]; 3];
        for r in 0..3u32 {
            for c in 0..3u32 {
                l[r as usize][c as usize] = rf(m, rider + src_off + (r * 3 + c) * 4);
            }
        }
        let rot = mul3x3_rows(l, b3);
        let dst = body_mats + bone_off;
        for r in 0..3u32 {
            for c in 0..3u32 {
                wf(m, dst + r * 0x10 + c * 4, rot[r as usize][c as usize]);
            }
        }
    }
}

/// Full `Boarder_UpdatePose` 0x129628 for normal riders (`stats+0x78 == 0`):
/// 1. Builds `rider+0x4aa0` via [`board_frame`].
/// 2. Runs [`update_skeleton`] (`Boarder_UpdateSkeleton` 0x12ae18) -> board world matrix & `rider+0x4a60`.
/// 3. If `skin_child`, poses all body bones by FK (`Boarder_SetSkinChildMatrices` 0x2083e8); else shifts
///    `rider+0x4ad0` by `x1c0 * scale * up` in motions 2/4, resolves left/right board binding world targets via
///    `Boarder_GetBoneWorldPos` 0x208398 (part 1, points 1 and 0), runs `Boarder_BuildBoneMatrices` 0x2083f8
///    (FK bones 0..12 + 2-bone leg IK on 16..18 and 13..15), and locks both feet via [`apply_foot_bindings`].
/// 4. Moves `rider+0x4ad0` to world pivot (`apply(f, pivot)`) and writes `board_corners` to `rider+0x5760`.
/// Returns `(board_frame_before_pivot, board_frame_final, board_bone_frame)`.
pub fn update_pose(
    m: &mut Mem,
    rider: u32,
    p: &PoseIn,
    skin_child: bool,
    x1c0: f32,
    natural_stance: u32,
    scratch: u32,
) -> ([[f32; 4]; 4], [[f32; 4]; 4], [[f32; 4]; 4]) {
    let f0 = board_frame(p);
    for (r, row) in f0.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            m.w32(rider + 0x4aa0 + 0x10 * r as u32 + 4 * c as u32, v.to_bits());
        }
    }
    let bone_ix = m.r32(rider + 0x44d8);
    let piv_at = rider + bone_ix.wrapping_mul(0x394) + 0x8c8;
    let pivot = [
        f32::from_bits(m.r32(piv_at)),
        f32::from_bits(m.r32(piv_at + 4)),
        f32::from_bits(m.r32(piv_at + 8)),
    ];
    update_skeleton(m, rider);
    let mut f = f0;
    if skin_child {
        crate::riderskin::set_skin_child_matrices(m, rider + 0x79c, rider + 0x4aa0);
    } else {
        let p1 = [pivot[0], pivot[1], pivot[2], 1.0];
        let mut b = p1;
        if p.motion == 2 || p.motion == 4 {
            for k in 0..4 {
                b[k] = b[k] + (p.up[k] * x1c0) * p.lean_scale;
            }
        }
        let mut c = [0.0f32; 4];
        for k in 0..4 {
            c[k] = (b[k] - p1[k]) + [0.0, 0.0, 0.0, 1.0][k];
        }
        f[3] = apply(f, c);
        for (k, &v) in f[3].iter().enumerate() {
            m.w32(rider + 0x4ad0 + 4 * k as u32, v.to_bits());
        }
        crate::limbik::boarder_bone_world_pos(m, scratch, rider + 0x79c, 1, 1, rider + 0x4a60);
        crate::limbik::boarder_bone_world_pos(m, scratch + 0x10, rider + 0x79c, 1, 0, rider + 0x4a60);
        crate::limbik::build_bone_matrices(m, rider + 0x79c, rider + 0x4aa0, scratch, scratch + 0x10);
        apply_foot_bindings(m, rider);
    }
    f[3] = apply(f, [pivot[0], pivot[1], pivot[2], 1.0]);
    for (k, &v) in f[3].iter().enumerate() {
        m.w32(rider + 0x4ad0 + 4 * k as u32, v.to_bits());
    }
    let bone_frame: [[f32; 4]; 4] = std::array::from_fn(|r| {
        std::array::from_fn(|c| f32::from_bits(m.r32(rider + 0x4a60 + 0x10 * r as u32 + 4 * c as u32)))
    });
    let corners = board_corners(bone_frame, p.class, natural_stance, p.lean_scale);
    for (r, row) in corners.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            m.w32(rider + 0x5760 + 0x10 * r as u32 + 4 * c as u32, v.to_bits());
        }
    }
    (f0, f, bone_frame)
}
