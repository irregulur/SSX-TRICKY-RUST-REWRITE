//! The final tail of `world` functions (board row E9o): stage-area markers, spline-path draw/update/dtor,
//! crowd box/system teardown, world-instance sound-key query, sphere-tree helpers and plane intersection,
//! ground probing, and `cWorldSphere` / `cWorldBoarder` / `cWorldSphTree` / `cWorldLine` virtuals on engine
//! memory (`pf::Mem`). Every function is ported 1:1 from `SLUS_203.26` and checked bit-for-bit in the function
//! runner (`tests/world_final_tail.rs`).

use crate::pf::Mem;
use crate::ps2f::F;

type V4 = [f32; 4];
type M4 = [V4; 4];

// Global addresses in SLUS_203.26.
pub const G_APP: u32 = 0x0033_7c58;
pub const G_IDENTITY_MAT4: u32 = 0x0034_6440;
pub const G_WORLD: u32 = 0x0034_6488;
pub const G_WORLD_SOUND: u32 = 0x0034_6f58;
pub const STR_STAGE_AREA_START: u32 = 0x0038_3068;
pub const STR_STAGE_AREA_FINISH: u32 = 0x0038_3080;
pub const G_CROWD_STOP_CALLBACK: u32 = 0x0039_fbd8;

// Vtables in SLUS_203.26.
pub const VT_SPLINE_PATH_NODE: u32 = 0x0039_a710;
pub const VT_BX_SPHERE_TREE: u32 = 0x003a_4018;
pub const VT_SPHERE_TREE: u32 = 0x003a_40b8;
pub const VT_WORLD_BOARDER: u32 = 0x003a_77d8;
pub const VT_WORLD_SPH_TREE: u32 = 0x003a_85f8;

// Callee addresses in SLUS_203.26.
pub const VEC4_SCALE: u32 = 0x0010_2f50;
pub const MEM_DELETE: u32 = 0x0011_4f80;
pub const MEM_FREE: u32 = 0x0014_a040;
pub const MAT4_TRANSFORM_VEC4: u32 = 0x0017_8078;
pub const BX_OBJ_NODE_FROM_LINK: u32 = 0x0017_d1c8;
pub const BX_SORT_OBJ_NODE_DTOR: u32 = 0x0017_dd00;
pub const LIGHTS_CLEAR: u32 = 0x001c_e278;
pub const SPLINE_PATH_NODE_PLACE_ALL: u32 = 0x001f_a838;
pub const SPLINE_PATH_NODE_1FAAD0: u32 = 0x001f_aad0;
pub const SPLINE_PATH_NODE_FREE_MEM: u32 = 0x001f_b3c8;
pub const PTR_ARRAY_FREE_ALL: u32 = 0x0021_edf0;
pub const CROWD_BOX_DESTRUCT: u32 = 0x0021_ef58;
pub const CROWD_BOX_STOP_ALL: u32 = 0x0022_0338;
pub const CROWD_STOP_CHEER: u32 = 0x0022_0720;
pub const CROWD_SYS_STOP_ALL: u32 = 0x0022_1330;
pub const WORLD_INSTANCE_GET_SOUND_KEYS: u32 = 0x0022_baf8;
pub const WORLD_EMITTER_IS_HIT_ONLY_KEY: u32 = 0x0022_e240;
pub const COLL_BODY_DESTRUCT: u32 = 0x0023_6190;
pub const SPHERE_TREE_CACHE_GET: u32 = 0x0023_8f50;
pub const BX_SPHERE_TREE_DTOR: u32 = 0x0023_91d0;
pub const SPHERE_TREE_DTOR: u32 = 0x0023_9c38;
pub const COLLIDE_SPHERE_SPH_TREE: u32 = 0x0023_a160;
pub const SPH_TREE_SPHERE_RECURSE: u32 = 0x0023_a728;
pub const SPH_TREE_SORT_OCTANTS: u32 = 0x0023_b398;
pub const SPHERE_TREE_BUF_CACHE_DTOR: u32 = 0x0023_b520;
pub const TRI_CONTAINS_POINT_VU0: u32 = 0x0023_b6b0;
pub const COLLIDE_SPHERE_BOX: u32 = 0x0023_b6e8;
pub const NAME_HASH: u32 = 0x0024_0ed8;
pub const MATH_SIN_COS: u32 = 0x0025_0c10;
pub const MATH_ATAN: u32 = 0x0025_10f8;
pub const SPLINE_PATH_NODE_DRAW_PATH: u32 = 0x0025_1748;
pub const COURSE_FIND_INSTANCE_BY_HASH: u32 = 0x0025_50c8;
pub const WORLD_RAY_CAST: u32 = 0x0025_aff8;
pub const WORLD_LINE_CTOR: u32 = 0x0025_d7f0;

/// External calls and stack-like scratch space needed by the `world` tail functions.
pub trait WorldTailEnv {
    /// Allocate `size` bytes of 16-byte-aligned scratch memory inside `Mem`.
    fn scratch(&mut self, size: u32) -> u32;
    /// Call a function or virtual returning a 32-bit integer in `$v0`.
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32;
    /// Call a function or virtual with a float argument in `$f12`, returning `$v0`.
    fn call_f(&mut self, m: &mut Mem, target: u32, args: &[u32], f12: f32) -> u32 {
        let _ = f12;
        self.call(m, target, args)
    }
    /// Call a function taking `$a0..` (and optional `$f12`) that returns an `f32` in `$f0`.
    fn call_ret_f(&mut self, m: &mut Mem, target: u32, args: &[u32], f12: f32) -> f32 {
        f32::from_bits(self.call_f(m, target, args, f12))
    }
    /// Call a virtual taking `(a0: u32, a1: u64)` (used by `CrowdSys_StopAll` for the 8-byte member pointer).
    fn call_u64(&mut self, m: &mut Mem, target: u32, a0: u32, a1: u64) -> u32 {
        self.call(m, target, &[a0, a1 as u32, (a1 >> 32) as u32])
    }
}

#[inline]
fn rf32(m: &Mem, a: u32) -> f32 {
    f32::from_bits(m.r32(a))
}

#[inline]
fn wf32(m: &mut Mem, a: u32, v: f32) {
    m.w32(a, v.to_bits());
}

#[inline]
fn rvec4(m: &Mem, a: u32) -> V4 {
    [rf32(m, a), rf32(m, a + 4), rf32(m, a + 8), rf32(m, a + 12)]
}

#[inline]
fn wvec4(m: &mut Mem, a: u32, v: V4) {
    wf32(m, a, v[0]);
    wf32(m, a + 4, v[1]);
    wf32(m, a + 8, v[2]);
    wf32(m, a + 12, v[3]);
}

#[inline]
fn rmat4(m: &Mem, a: u32) -> M4 {
    [rvec4(m, a), rvec4(m, a + 0x10), rvec4(m, a + 0x20), rvec4(m, a + 0x30)]
}

#[inline]
fn wmat4(m: &mut Mem, a: u32, mat: M4) {
    wvec4(m, a, mat[0]);
    wvec4(m, a + 0x10, mat[1]);
    wvec4(m, a + 0x20, mat[2]);
    wvec4(m, a + 0x30, mat[3]);
}

#[inline]
fn vu_add(a: V4, b: V4) -> V4 {
    [(F(a[0]) + F(b[0])).0, (F(a[1]) + F(b[1])).0, (F(a[2]) + F(b[2])).0, (F(a[3]) + F(b[3])).0]
}

#[inline]
fn vu_sub(a: V4, b: V4) -> V4 {
    [(F(a[0]) - F(b[0])).0, (F(a[1]) - F(b[1])).0, (F(a[2]) - F(b[2])).0, (F(a[3]) - F(b[3])).0]
}

#[inline]
fn vu_scale(a: V4, s: f32) -> V4 {
    let s = F(s);
    [(F(a[0]) * s).0, (F(a[1]) * s).0, (F(a[2]) * s).0, (F(a[3]) * s).0]
}

/// VU0 4-lane dot product: `vmul.xyzw`, `vadday.x`, `vmaddaz.x` (vf6.x = 1.0), `vmaddw.x` (vf6.x = 1.0).
#[inline]
fn vu_dot4(a: V4, b: V4) -> f32 {
    let p0 = F(a[0]) * F(b[0]);
    let p1 = F(a[1]) * F(b[1]);
    let p2 = F(a[2]) * F(b[2]);
    let p3 = F(a[3]) * F(b[3]);
    (((p0 + p1) + F(1.0) * p2) + F(1.0) * p3).0
}

/// VU0 row-vector transform `v * m`: `vmulax(m[0], v.x) + vmadday(m[1], v.y) + vmaddaz(m[2], v.z) + vmaddw(m[3], v.w)`.
#[inline]
fn vu_mat_mul_vec(m: M4, v: V4) -> V4 {
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        let s0 = F(m[0][c]) * F(v[0]);
        let s1 = s0 + F(m[1][c]) * F(v[1]);
        let s2 = s1 + F(m[2][c]) * F(v[2]);
        let s3 = s2 + F(m[3][c]) * F(v[3]);
        out[c] = s3.0;
    }
    out
}

/// VU0 4x4 matrix multiply where each row `r` of `rhs` is transformed through `lhs`.
#[inline]
fn vu_mat_mul(lhs: M4, rhs: M4) -> M4 {
    [
        vu_mat_mul_vec(lhs, rhs[0]),
        vu_mat_mul_vec(lhs, rhs[1]),
        vu_mat_mul_vec(lhs, rhs[2]),
        vu_mat_mul_vec(lhs, rhs[3]),
    ]
}

/// EE `cvt.w.s`: truncate toward zero with EE float loading and 32-bit signed clamp.
#[inline]
fn ee_cvt_w_s(x: F) -> i32 {
    let v = x.ee();
    if v >= i32::MAX as f32 {
        i32::MAX
    } else if v <= i32::MIN as f32 {
        i32::MIN
    } else {
        v.trunc() as i32
    }
}

/// Build the axis-angle 4x4 rotation matrix in the exact FPU instruction order of `cSplinePathNode::Draw`.
fn axis_angle_mat(ax: f32, ay: f32, az: f32, s: f32, c: f32) -> M4 {
    let one = F(1.0);
    let f11 = F(c);
    let f0 = F(ax);
    let f1 = one - f11;
    let f3 = F(az);
    let f6 = F(ay);
    let f8 = F(s);

    let f2 = f1 * f3;
    let f5 = f1 * f0;
    let f1 = f1 * f6;
    let f13 = f8 * f3;
    let f12 = f8 * f0;

    let f9 = f2 * f3;
    let f10 = f5 * f3;
    let f4 = f2 * f0;
    let f7 = f5 * f0;
    let f3 = f1 * f3;
    let f0 = f1 * f0;
    let f8 = f8 * f6;
    let f2 = f2 * f6;
    let f5 = f5 * f6;
    let f1 = f1 * f6;

    let m02 = (f4 + f8).0;
    let m01 = (f0 - f13).0;
    let m12 = (f2 - f12).0;
    let m22 = (f9 + f11).0;
    let m00 = (f7 + f11).0;
    let m10 = (f5 + f13).0;
    let m20 = (f10 - f8).0;
    let m11 = (f1 + f11).0;
    let m21 = (f3 + f12).0;

    [
        [m00, m01, m02, 0.0],
        [m10, m11, m12, 0.0],
        [m20, m21, m22, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

// ---------------------------------------------------------------------------
// 1. Stage area marker
// ---------------------------------------------------------------------------

/// `World_GetStageAreaMarker` 0x1779b8
pub fn world_get_stage_area_marker(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    kind: i32,
    out_pos: u32,
    out_yaw: u32,
) -> u32 {
    let hash = match kind {
        1 => env.call(m, NAME_HASH, &[STR_STAGE_AREA_START]),
        2 => env.call(m, NAME_HASH, &[STR_STAGE_AREA_FINISH]),
        _ => 0,
    };
    if hash == 0 {
        return 0;
    }
    let inst = env.call(m, COURSE_FIND_INSTANCE_BY_HASH, &[G_WORLD, hash]);
    if inst == 0 {
        return 0;
    }

    let sp = env.scratch(0x70);
    let mut inv = rmat4(m, inst);
    let r01 = inv[0][1];
    let r10 = inv[1][0];
    let r02 = inv[0][2];
    let r20 = inv[2][0];
    let r12 = inv[1][2];
    let r21 = inv[2][1];
    inv[0][1] = r10;
    inv[1][0] = r01;
    inv[0][2] = r20;
    inv[2][0] = r02;
    inv[1][2] = r21;
    inv[2][1] = r12;

    wmat4(m, sp, inv);
    wvec4(m, sp + 0x50, inv[3]);
    wvec4(m, sp + 0x60, inv[3]);

    env.call_f(m, VEC4_SCALE, &[sp + 0x40, sp + 0x50], -1.0);
    m.w32(sp + 0x4c, 0);
    env.call(m, MAT4_TRANSFORM_VEC4, &[sp + 0x50, sp, sp + 0x40]);

    let mut t = rvec4(m, sp + 0x50);
    t[3] = 1.0;
    wvec4(m, sp + 0x40, t);
    wvec4(m, sp + 0x30, t);
    inv[3] = t;

    let pos_in = rvec4(m, out_pos);
    let pos_out = vu_mat_mul_vec(inv, pos_in);
    wvec4(m, out_pos, pos_out);
    m.w32(out_pos + 0xc, 0);

    let inst_mat = rmat4(m, inst);
    let dir = vu_mat_mul_vec(inst_mat, [1.0, 0.0, 0.0, 0.0]);
    let x = dir[0];
    let y = dir[1];
    let zero = 0.0f32;

    let yaw = if F(x).ee() == zero {
        if F(y).ee() == zero {
            -y
        } else if zero <= F(y).ee() {
            -f32::from_bits(0x3fc9_0fdb)
        } else {
            -f32::from_bits(0xbfc9_0fdb)
        }
    } else {
        let ratio = (F(y) / F(x)).0;
        let mut ang = env.call_ret_f(m, MATH_ATAN, &[], ratio);
        if F(x).ee() < zero {
            let pi = f32::from_bits(0x4049_0fdb);
            if zero < F(y).ee() {
                ang = (F(ang) + F(pi)).0;
            } else {
                ang = (F(ang) - F(pi)).0;
            }
        }
        -ang
    };
    wf32(m, out_yaw, yaw);
    1
}

// ---------------------------------------------------------------------------
// 2. cSplinePathNode
// ---------------------------------------------------------------------------

/// `cSplinePathNode::~cSplinePathNode` 0x1fa640
pub fn spline_path_node_dtor(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32, how: u32) {
    m.w32(o + 8, VT_SPLINE_PATH_NODE);
    let pts = m.r32(o + 0x7c);
    if pts != 0 {
        env.call(m, MEM_FREE, &[pts]);
    }
    env.call(m, SPLINE_PATH_NODE_FREE_MEM, &[o + 0x64, 2]);
    env.call(m, BX_SORT_OBJ_NODE_DTOR, &[o, how]);
}

/// `cSplinePathNode::Update` 0x1fa6a0
pub fn spline_path_node_update(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32) {
    if m.r32(o + 0x40) != 0 {
        return;
    }
    let inst = m.r32(o + 0x78);
    if m.r32(inst + 0xe4) == 0 || (m.r32(inst + 0xe8) & 0x800) != 0 {
        if o != 0 {
            let vt = m.r32(o + 8);
            let adj = m.r16(vt + 8) as i16 as i32;
            let fn_addr = m.r32(vt + 0xc);
            env.call(m, fn_addr, &[o.wrapping_add(adj as u32), 3]);
        }
        return;
    }

    let sp = env.scratch(0x10);
    m.w32(sp, 0);
    m.w32(sp + 4, 0);
    m.w32(sp + 8, 0);
    m.w32(sp + 12, 0);
    env.call(m, SPLINE_PATH_NODE_PLACE_ALL, &[o, sp]);

    let dt = rf32(m, o + 0x4c);
    let t = (F(rf32(m, o + 0x50)) + F(dt)).0;
    wf32(m, o + 0x50, t);
    if F(t).ee() < 0.0 {
        m.w32(o + 0x50, 0);
        if m.r32(o + 0x34) == 2 {
            wf32(m, o + 0x4c, -dt);
        }
    }

    let len = rf32(m, o + 0x74);
    let t = rf32(m, o + 0x50);
    if F(len).ee() <= F(t).ee() {
        let mode = m.r32(o + 0x34);
        if mode == 0 {
            m.w32(o + 0x40, 1);
        } else if mode == 3 {
            m.w32(o + 0x4c, 0);
        } else if mode == 2 {
            let cur_dt = rf32(m, o + 0x4c);
            wf32(m, o + 0x4c, -cur_dt);
        } else {
            wf32(m, o + 0x50, (F(t) - F(len)).0);
        }
    } else {
        let inst2 = m.r32(o + 0x78);
        if (m.r32(inst2 + 0xe8) & 0x80) != 0 && m.r32(o + 0x34) == 3 {
            env.call(m, SPLINE_PATH_NODE_1FAAD0, &[o, sp]);
        }
    }

    if m.r32(o + 0x30) == 1 {
        let inst3 = m.r32(o + 0x78);
        let ctl = m.r32(inst3 + 0xe4);
        if ctl != 0 && (m.r32(inst3 + 0xe8) & 0x80) != 0 {
            let vt = m.r32(ctl + 8);
            let adj = m.r16(vt + 0xc8) as i16 as i32;
            let fn_addr = m.r32(vt + 0xcc);
            let pts = m.r32(o + 0x7c);
            env.call(m, fn_addr, &[ctl.wrapping_add(adj as u32), pts, pts.wrapping_add(0x10)]);
        }
    }
}

/// `cSplinePathNode::Draw` 0x1fabc8
pub fn spline_path_node_draw(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32) {
    if m.r32(o + 0x3c) != 0 {
        env.call(m, SPLINE_PATH_NODE_DRAW_PATH, &[o + 0x64, o + 0x54]);
    }
    let app = m.r32(G_APP);
    let gfx = m.r32(app + 0x724);
    let inst0 = m.r32(o + 0x78);
    let model = m.r32(inst0 + 0xc0);
    if (m.r32(o + 0x30) as i32) <= 0 {
        return;
    }

    let sp = env.scratch(0x150);
    let b0_ptr = sp;
    let b1_ptr = sp + 0x10;
    let mat_ptr = sp + 0x30;
    let lights_ptr = sp + 0xb0;
    let sc_z = sp + 0x130;
    let sc_y = sp + 0x138;
    let sc_x = sp + 0x140;

    let mut i = 0i32;
    loop {
        let off = (i as u32) << 5;
        let entry = m.r32(o + 0x7c).wrapping_add(off);
        let pos = rvec4(m, entry);
        let b0 = vu_add(rvec4(m, o + 0x80), pos);
        let b1 = vu_add(rvec4(m, o + 0x90), pos);
        wvec4(m, b0_ptr, b0);
        wvec4(m, b1_ptr, b1);

        let vt = m.r32(gfx + 0x10);
        let adj = m.r16(vt + 0x280) as i16 as i32;
        let cull_fn = m.r32(vt + 0x284);
        let culled = env.call(m, cull_fn, &[gfx.wrapping_add(adj as u32), b0_ptr, b1_ptr]);

        if culled != 1 {
            let mut mat = rmat4(m, G_IDENTITY_MAT4);

            // Z rotation (0, 0, 1) by entry+0x18
            let ang_z = rf32(m, m.r32(o + 0x7c).wrapping_add(off) + 0x18);
            env.call_f(m, MATH_SIN_COS, &[sc_z, sc_z + 4], ang_z);
            let rot_z = axis_angle_mat(0.0, 0.0, 1.0, rf32(m, sc_z), rf32(m, sc_z + 4));
            mat = vu_mat_mul(mat, rot_z);

            // Y rotation (0, 1, 0) by entry+0x14
            let ang_y = rf32(m, m.r32(o + 0x7c).wrapping_add(off) + 0x14);
            env.call_f(m, MATH_SIN_COS, &[sc_y, sc_y + 4], ang_y);
            let rot_y = axis_angle_mat(0.0, 1.0, 0.0, rf32(m, sc_y), rf32(m, sc_y + 4));
            mat = vu_mat_mul(mat, rot_y);

            // X rotation (1, 0, 0) by entry+0x10
            let ang_x = rf32(m, m.r32(o + 0x7c).wrapping_add(off) + 0x10);
            env.call_f(m, MATH_SIN_COS, &[sc_x, sc_x + 4], ang_x);
            let rot_x = axis_angle_mat(1.0, 0.0, 0.0, rf32(m, sc_x), rf32(m, sc_x + 4));
            mat = vu_mat_mul(mat, rot_x);

            mat[3] = rvec4(m, m.r32(o + 0x7c).wrapping_add(off));
            wmat4(m, mat_ptr, mat);

            let inst = m.r32(o + 0x78);
            let ctl = m.r32(inst + 0xe4);
            if ctl != 0 && ((m.r32(ctl + 0x14) >> 8) & 1) != 0 {
                let obj = env.call(m, BX_OBJ_NODE_FROM_LINK, &[ctl]);
                let obj_vt = m.r32(obj + 0x38);
                let obj_adj = m.r16(obj_vt + 0xe8) as i16 as i32;
                let obj_fn = m.r32(obj_vt + 0xec);
                env.call(m, obj_fn, &[obj.wrapping_add(obj_adj as u32), mat_ptr, 0]);
            } else {
                env.call(m, LIGHTS_CLEAR, &[lights_ptr]);
                let gvt = m.r32(gfx + 0x10);
                let gadj = m.r16(gvt + 0x2a8) as i16 as i32;
                let draw_fn = m.r32(gvt + 0x2ac);
                let inst2 = m.r32(o + 0x78);
                let flags_f0 = m.r32(inst2 + 0xf0);
                env.call(
                    m,
                    draw_fn,
                    &[gfx.wrapping_add(gadj as u32), model, flags_f0, mat_ptr, lights_ptr, 0, 0],
                );
            }
        }

        i += 1;
        if i >= (m.r32(o + 0x30) as i32) {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Crowd & WorldInstance sound key
// ---------------------------------------------------------------------------

/// `CrowdBox_Destruct` 0x21ef58
pub fn crowd_box_destruct(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32, how: u32) {
    env.call(m, CROWD_STOP_CHEER, &[o]);
    env.call_f(m, CROWD_BOX_STOP_ALL, &[o], 0.0);
    if (how & 1) != 0 {
        env.call(m, MEM_DELETE, &[o]);
    }
}

/// `CrowdSys_Free` 0x220bb8
pub fn crowd_sys_free(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32) {
    env.call(m, CROWD_SYS_STOP_ALL, &[o]);
    let arr = m.r32(o + 0x372c);
    if arr != 0 {
        let count = m.r32(arr.wrapping_sub(0x10));
        let mut ptr = arr.wrapping_add(count.wrapping_mul(0x190));
        if arr != ptr {
            loop {
                ptr = ptr.wrapping_sub(0x190);
                env.call(m, CROWD_BOX_DESTRUCT, &[ptr, 0]);
                if m.r32(o + 0x372c) == ptr {
                    break;
                }
            }
        }
        let cur_arr = m.r32(o + 0x372c);
        env.call(m, MEM_FREE, &[cur_arr.wrapping_sub(0x10)]);
        m.w32(o + 0x372c, 0);
        m.w32(o + 0x3728, 0);
    }

    for i in 0..3u32 {
        let base = o + 0x3730 + i * 0x54;
        m.w32(base + 0x48, 0);
        m.w32(base + 0x4c, 0);
        m.w32(base + 0x50, 0);
        env.call(m, PTR_ARRAY_FREE_ALL, &[base + 0x30]);
        env.call(m, PTR_ARRAY_FREE_ALL, &[base + 0x18]);
        env.call(m, PTR_ARRAY_FREE_ALL, &[base]);
    }
}

/// `CrowdSys_StopAll` 0x221330
pub fn crowd_sys_stop_all(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32) {
    let mut i = 0i32;
    while i < (m.r32(o + 0x3728) as i32) {
        let elem = m.r32(o + 0x372c).wrapping_add((i as u32).wrapping_mul(0x190));
        env.call_f(m, CROWD_BOX_STOP_ALL, &[elem], 0.0);
        i += 1;
    }
    let vt = m.r32(o + 4);
    let adj = m.r16(vt + 0x38) as i16 as i32;
    let fn_addr = m.r32(vt + 0x3c);
    let cb = (m.r32(G_CROWD_STOP_CALLBACK) as u64) | ((m.r32(G_CROWD_STOP_CALLBACK + 4) as u64) << 32);
    env.call_u64(m, fn_addr, o.wrapping_add(adj as u32), cb);
}

/// `WorldInstance_HasHitOnlyKey` 0x22e1b8
pub fn world_instance_has_hit_only_key(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    emitter: u32,
    inst: u32,
) -> u32 {
    let sp = env.scratch(0x10);
    let mgr = m.r32(G_WORLD_SOUND);
    env.call(m, WORLD_INSTANCE_GET_SOUND_KEYS, &[mgr, inst, sp, sp + 4, sp + 8, sp + 12]);
    for k in 0..4u32 {
        let key = m.r32(sp + k * 4);
        if env.call(m, WORLD_EMITTER_IS_HIT_ONLY_KEY, &[emitter, key]) != 0 {
            return 1;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// 4. SphereTree / bxSphereTree (10 functions)
// ---------------------------------------------------------------------------

/// `SphTree_Assert` 0x2391c8
#[inline]
pub fn sph_tree_assert(_cond: u32) {}

/// `bxSphereTree::~bxSphereTree` 0x2391d0
pub fn bx_sphere_tree_dtor(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32, how: u32) {
    m.w32(o, VT_BX_SPHERE_TREE);
    env.call(m, SPHERE_TREE_DTOR, &[o, how]);
}

/// `SphTree_ForEachLeaf` 0x2396f8
pub fn sph_tree_for_each_leaf(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32) {
    let mut hdr = m.r32(o + 0xa0);
    if m.r32(hdr + 8) != 0 {
        let ptr = env.call(m, SPHERE_TREE_CACHE_GET, &[o]);
        hdr = m.r32(o + 0xa0);
        m.w32(hdr + 0x28, ptr);
    }
    let max_depth = m.r32(hdr + 0xc) as i32;
    sph_tree_for_each_leaf_recurse(m, env, o, max_depth, o + 0x90, 0, 0);
}

/// `SphTree_ForEachLeafRecurse` 0x239750
pub fn sph_tree_for_each_leaf_recurse(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    o: u32,
    max_depth: i32,
    node_pos: u32,
    depth: i32,
    idx: i32,
) {
    if depth < max_depth {
        let lv_off = (depth as u32).wrapping_mul(12);
        let hdr = m.r32(o + 0xa0);
        let half = rf32(m, m.r32(hdr + 0x20).wrapping_add(lv_off).wrapping_add(0x10));
        let child_ptr = env.scratch(0x10);
        for k in 0..8u32 {
            let hdr_k = m.r32(o + 0xa0);
            let occ = m.r8(m.r32(hdr_k + 0x28).wrapping_add(idx as u32));
            if (((occ as i32) >> k) & 1) != 0 {
                let dir = rvec4(m, o + 0x10 + (k << 4));
                let child = vu_add(rvec4(m, node_pos), vu_scale(dir, half));
                wvec4(m, child_ptr, child);
                let stride = m.r32(m.r32(hdr_k + 0x20).wrapping_add(lv_off).wrapping_add(8)) as i32;
                let next_idx = idx.wrapping_add(((k as i32) + 1).wrapping_mul(stride));
                sph_tree_for_each_leaf_recurse(m, env, o, max_depth, child_ptr, depth + 1, next_idx);
            }
        }
    } else {
        let app = m.r32(G_APP);
        let gfx = m.r32(app + 0x724);
        let hdr = m.r32(o + 0xa0);
        let rad = rf32(m, m.r32(hdr + 0x20).wrapping_add((depth as u32).wrapping_mul(12)));
        let color_ptr = env.scratch(0x10);
        wvec4(m, color_ptr, [1.0, 0.0, 1.0, 0.0]);
        let vt = m.r32(gfx + 0x10);
        let adj = m.r16(vt + 0x268) as i16 as i32;
        let draw_fn = m.r32(vt + 0x26c);
        env.call_f(m, draw_fn, &[gfx.wrapping_add(adj as u32), node_pos, color_ptr, 4], rad);
    }
}

/// `SphereTree::~SphereTree` 0x239c38
pub fn sphere_tree_dtor(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32, how: u32) {
    m.w32(o, VT_SPHERE_TREE);
    env.call(m, SPHERE_TREE_BUF_CACHE_DTOR, &[o, how]);
}

/// `SphTree_SetPosition` 0x239c98
pub fn sph_tree_set_position(m: &mut Mem, o: u32, pos_ptr: u32) {
    sph_tree_assert((m.r32(o + 0xa0) != 0) as u32);
    let hdr = m.r32(o + 0xa0);
    let home = [rf32(m, hdr + 0x14), rf32(m, hdr + 0x18), rf32(m, hdr + 0x1c), 1.0];
    let res = vu_add(home, rvec4(m, pos_ptr));
    wvec4(m, o + 0x90, res);
}

/// `SphTree_Translate` 0x239d18
pub fn sph_tree_translate(m: &mut Mem, o: u32, delta_ptr: u32) {
    sph_tree_assert((m.r32(o + 0xa0) != 0) as u32);
    let res = vu_add(rvec4(m, o + 0x90), rvec4(m, delta_ptr));
    wvec4(m, o + 0x90, res);
}

/// `Collide_SphereSphTree` 0x23a160
pub fn collide_sphere_sph_tree(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    o: u32,
    center_ptr: u32,
    out_pen: u32,
    out_hit: u32,
    radius: f32,
) -> u32 {
    sph_tree_assert((m.r32(o + 0xa0) != 0) as u32);
    let mut hdr = m.r32(o + 0xa0);
    if m.r32(hdr + 8) != 0 {
        let ptr = env.call(m, SPHERE_TREE_CACHE_GET, &[o]);
        hdr = m.r32(o + 0xa0);
        m.w32(hdr + 0x28, ptr);
    }
    sph_tree_assert((m.r32(hdr + 0x28) != 0) as u32);
    env.call_f(m, SPH_TREE_SPHERE_RECURSE, &[o, center_ptr, o + 0x90, 0, 0, out_pen, out_hit], radius)
}

/// `SphTree_IntersectPlane` 0x23a2c0
pub fn sph_tree_intersect_plane(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    o: u32,
    plane_ptr: u32,
    out_pen: u32,
    out_hit: u32,
) -> u32 {
    sph_tree_assert((m.r32(o + 0xa0) != 0) as u32);
    let mut hdr = m.r32(o + 0xa0);
    if m.r32(hdr + 8) != 0 {
        let ptr = env.call(m, SPHERE_TREE_CACHE_GET, &[o]);
        hdr = m.r32(o + 0xa0);
        m.w32(hdr + 0x28, ptr);
    }
    sph_tree_assert((m.r32(hdr + 0x28) != 0) as u32);
    env.call(m, SPH_TREE_SORT_OCTANTS, &[o, plane_ptr]);
    m.w32(o + 0xa4, 0);
    let root = rvec4(m, o + 0x90);
    sph_tree_plane_recurse_vec(m, o, plane_ptr, root, 0, 0, out_pen, out_hit);
    (F(rf32(m, o + 0xa4)).ee() < 0.0) as u32
}

/// `SphTree_PlaneRecurse` 0x23ab78
pub fn sph_tree_plane_recurse(
    m: &mut Mem,
    o: u32,
    plane_ptr: u32,
    node_pos: u32,
    depth: i32,
    idx: i32,
    out_pen: u32,
    out_hit: u32,
) {
    let node = rvec4(m, node_pos);
    sph_tree_plane_recurse_vec(m, o, plane_ptr, node, depth, idx, out_pen, out_hit);
}

fn sph_tree_plane_recurse_vec(
    m: &mut Mem,
    o: u32,
    plane_ptr: u32,
    node: V4,
    depth: i32,
    idx: i32,
    out_pen: u32,
    out_hit: u32,
) {
    let pn = rvec4(m, plane_ptr);
    let pd = rf32(m, plane_ptr + 0x10);
    let hdr = m.r32(o + 0xa0);
    let best = rf32(m, o + 0xa4);
    let d = (F(vu_dot4(pn, node)) - F(pd)).0;
    let lv_off = (depth as u32).wrapping_mul(12);
    let lv_base = m.r32(hdr + 0x20).wrapping_add(lv_off);
    let rad = rf32(m, lv_base);
    if !(F(d).ee() < (F(rad) + F(best)).ee()) {
        return;
    }
    let occ = m.r8(m.r32(hdr + 0x28).wrapping_add(idx as u32));
    let max_depth = m.r32(hdr + 0xc) as i32;
    if depth < max_depth && occ != 0 {
        let half = rf32(m, lv_base + 0x10);
        for i in 0..8u32 {
            let k = m.r32(o + 0xa8 + i * 4);
            if (((occ as i32) >> (k & 31)) & 1) != 0 {
                let dir = rvec4(m, o + 0x10 + (k << 4));
                let child = vu_add(node, vu_scale(dir, half));
                let hdr2 = m.r32(o + 0xa0);
                let stride = m.r32(m.r32(hdr2 + 0x20).wrapping_add(lv_off).wrapping_add(8)) as i32;
                let next_idx = idx.wrapping_add(((k as i32).wrapping_add(1)).wrapping_mul(stride));
                sph_tree_plane_recurse_vec(m, o, plane_ptr, child, depth + 1, next_idx, out_pen, out_hit);
            }
        }
    } else {
        let pen = (F(d) - F(rad)).0;
        if F(pen).ee() < F(best).ee() {
            wvec4(m, out_pen, vu_scale(pn, pen));
            wvec4(m, out_hit, vu_sub(node, vu_scale(pn, d)));
            wf32(m, o + 0xa4, pen);
        }
    }
}

// ---------------------------------------------------------------------------
// 5. cWorldBoarder, World_ProbeGround, stubs, CellList_Destruct, cWorldSphere, cWorldSphTree
// ---------------------------------------------------------------------------

/// `cWorldBoarder::vf10_257ca0` 0x257ca0
pub fn world_boarder_dtor(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32, how: u32) {
    m.w32(o + 0x40, VT_WORLD_BOARDER);
    env.call(m, COLL_BODY_DESTRUCT, &[o + 0x60, 2]);
    if (how & 1) != 0 {
        env.call(m, MEM_DELETE, &[o]);
    }
}

/// `World_ProbeGround` 0x259030
pub fn world_probe_ground(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    world: u32,
    pos_ptr: u32,
    out_z: u32,
    out_surf: u32,
    out_normal: u32,
) -> u32 {
    let grid = m.r32(G_WORLD);
    let p = rvec4(m, pos_ptr);
    let mut from = p;
    let mut to = p;

    let x0 = rf32(m, grid + 4);
    let y0 = rf32(m, grid + 8);
    let step = rf32(m, grid + 0x28);
    let cx = ee_cvt_w_s(((F(p[0]) - F(x0)) + F(step)) / F(step));
    let cy = ee_cvt_w_s(((F(p[1]) - F(y0)) + F(step)) / F(step));
    let w = m.r32(grid + 0x2c) as i32;
    let h = m.r32(grid + 0x30) as i32;

    let mut x_max = if 0 < cx { cx } else { 0 };
    let mut x_min = cx.wrapping_sub(2);
    if !(x_min < i32::MAX) {
        x_min = i32::MAX;
    }
    let mut y_max = if 0 < cy { cy } else { 0 };
    let mut y_min = cy.wrapping_sub(2);
    if !(y_min < i32::MAX) {
        y_min = i32::MAX;
    }
    if w - 1 < x_max {
        x_max = w - 1;
    }
    if !(-1 < x_min) {
        x_min = 0;
    }
    if h - 1 < y_max {
        y_max = h - 1;
    }
    if !(-1 < y_min) {
        y_min = 0;
    }

    if x_min <= x_max {
        let mut gx = x_min;
        while gx <= x_max {
            let mut gy = y_min;
            while gy <= y_max {
                let g2 = m.r32(world);
                let gw = m.r32(g2 + 0x2c) as i32;
                let gh = m.r32(g2 + 0x30) as i32;
                let cx_clamped = if gx < 0 {
                    0
                } else if gw - 1 < gx {
                    gw - 1
                } else {
                    gx
                };
                let cy_clamped = if gy < 0 {
                    0
                } else if gh - 1 < gy {
                    gh - 1
                } else {
                    gy
                };
                let cells = m.r32(g2 + 0x4c);
                let idx = cy_clamped.wrapping_mul(gw).wrapping_add(cx_clamped) as u32;
                let cell = m.r32(cells.wrapping_add(idx << 2));
                if cell != 0 {
                    let inside = F(rf32(m, cell)).ee() <= F(p[0]).ee()
                        && F(p[0]).ee() <= F(rf32(m, cell + 0xc)).ee()
                        && F(rf32(m, cell + 4)).ee() <= F(p[1]).ee()
                        && F(p[1]).ee() <= F(rf32(m, cell + 0x10)).ee();
                    if inside {
                        let cz_min = rf32(m, cell + 8);
                        if F(cz_min).ee() < F(from[2]).ee() {
                            from[2] = cz_min;
                        }
                        let cz_max = rf32(m, cell + 0x14);
                        if F(to[2]).ee() < F(cz_max).ee() {
                            to[2] = cz_max;
                        }
                    }
                }
                gy += 1;
            }
            gx += 1;
        }
    }

    if F(to[2]).ee() <= F(from[2]).ee() {
        return 0;
    }

    let sp = env.scratch(0x100);
    let from_ptr = sp;
    let to_ptr = sp + 0x10;
    let hit_ptr = sp + 0x20;
    let line_ptr = sp + 0x60;
    wvec4(m, from_ptr, from);
    wvec4(m, to_ptr, to);

    let target = ((F(p[2]) - F(from[2])) / (F(to[2]) - F(from[2]))).0;
    env.call_f(m, WORLD_LINE_CTOR, &[line_ptr, from_ptr, to_ptr, 2], target);
    let t = env.call_ret_f(m, WORLD_RAY_CAST, &[world, line_ptr, hit_ptr, 0], 0.0);
    if !(0.0 <= F(t).ee()) {
        return 0;
    }

    let hit_kind = m.r32(hit_ptr);
    let surf = if hit_kind == 0 { m.r32(hit_ptr + 0x34) } else { 1 };
    wf32(m, out_z, rf32(m, hit_ptr + 0x18));
    m.w32(out_surf, surf);
    if out_normal != 0 {
        wvec4(m, out_normal, rvec4(m, hit_ptr + 0x20));
    }
    1
}

/// `cWorldIntersectType::vf9_25d7a0` 0x25d7a0
#[inline]
pub fn world_intersect_type_vf9(_o: u32) {}

/// `cWorldIntersectType::vf3_25d7a8` 0x25d7a8
#[inline]
pub fn world_intersect_type_vf3(_o: u32) -> u32 {
    0
}

/// `cWorldLine::vf6_25dc40` 0x25dc40
#[inline]
pub fn world_line_vf6(_o: u32) -> u32 {
    0
}

/// `cWorldLine::vf2_25ebd8` 0x25ebd8
#[inline]
pub fn world_line_vf2(m: &Mem, o: u32) -> f32 {
    rf32(m, o + 0x90)
}

/// `CellList_Destruct` 0x260fc0
pub fn cell_list_destruct(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32, how: u32) {
    if (how & 1) != 0 {
        env.call(m, MEM_DELETE, &[o]);
    }
}

/// `cWorldSphere::vf0_2611b0` 0x2611b0
pub fn world_sphere_push(m: &mut Mem, o: u32, mat_ptr: u32) {
    let mat = rmat4(m, mat_ptr);
    let c = rvec4(m, o + 0x70);
    wvec4(m, o + 0x50, vu_mat_mul_vec(mat, c));
    m.w32(o + 4, 1);
}

/// `cWorldSphere::vf1_2611f8` 0x2611f8
pub fn world_sphere_pop(m: &mut Mem, o: u32) {
    let c = rvec4(m, o + 0x70);
    wvec4(m, o + 0x50, c);
    m.w32(o + 4, 0);
}

/// `cWorldSphere::vf4_261208` 0x261208
#[allow(clippy::too_many_arguments)]
pub fn world_sphere_tri_front(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    o: u32,
    a: u32,
    b: u32,
    c: u32,
    n: u32,
    out_pt: u32,
    out_n: u32,
    out_pen: u32,
) -> u32 {
    let center = rvec4(m, o + 0x50);
    let radius = rf32(m, o + 0x60);
    let nv = rvec4(m, n);
    let d = vu_dot4(nv, vu_sub(center, rvec4(m, a)));
    if !(F(d.abs()).ee() < F(radius).ee()) {
        return 0;
    }
    let proj = vu_sub(center, vu_scale(nv, d));
    wvec4(m, out_pt, proj);
    let hit = env.call_f(m, TRI_CONTAINS_POINT_VU0, &[a, b, c, out_pt], f32::from_bits(0x38d1_b717));
    if hit != 0 {
        wvec4(m, out_n, nv);
        wf32(m, out_pen, (F(radius) - F(d)).0);
        1
    } else {
        0
    }
}

/// `cWorldSphere::vf5_261328` 0x261328
#[allow(clippy::too_many_arguments)]
pub fn world_sphere_tri_both(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    o: u32,
    a: u32,
    b: u32,
    c: u32,
    n: u32,
    out_pt: u32,
    out_n: u32,
    out_pen: u32,
) -> u32 {
    let center = rvec4(m, o + 0x50);
    let radius = rf32(m, o + 0x60);
    let mut nv = rvec4(m, n);
    let mut d = vu_dot4(nv, vu_sub(center, rvec4(m, a)));
    if F(radius).ee() < F(d.abs()).ee() {
        return 0;
    }
    if F(d).ee() < 0.0 {
        let sp = env.scratch(0x20);
        wvec4(m, sp, nv);
        env.call_f(m, VEC4_SCALE, &[sp + 0x10, sp], -1.0);
        nv = rvec4(m, sp + 0x10);
        d = -d;
    }
    let proj = vu_sub(center, vu_scale(nv, d));
    wvec4(m, out_pt, proj);
    let hit = env.call_f(m, TRI_CONTAINS_POINT_VU0, &[a, b, c, out_pt], f32::from_bits(0x38d1_b717));
    if hit != 0 {
        wvec4(m, out_n, nv);
        wf32(m, out_pen, (F(radius) - F(d)).0);
        1
    } else {
        0
    }
}

/// `cWorldSphere::vf6_2614a8` 0x2614a8
pub fn world_sphere_sphtree(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    o: u32,
    sphtree: u32,
    out_pt: u32,
    out_n: u32,
    out_pen: u32,
) -> u32 {
    let sp = env.scratch(0x10);
    let hit = env.call_f(m, COLLIDE_SPHERE_SPH_TREE, &[sphtree, o + 0x50, sp, out_pt], rf32(m, o + 0x60));
    if hit != 0 {
        let pen = rvec4(m, sp);
        let len = F(vu_dot4(pen, pen)).sqrt();
        let inv = (F(1.0) / len).0;
        wf32(m, out_pen, len.0);
        wvec4(m, out_n, vu_scale(pen, inv));
    }
    hit
}

/// `cWorldSphere::vf7_261560` 0x261560
pub fn world_sphere_box(
    m: &mut Mem,
    env: &mut impl WorldTailEnv,
    o: u32,
    lo: u32,
    hi: u32,
) -> u32 {
    env.call_f(m, COLLIDE_SPHERE_BOX, &[lo, hi, o + 0x50], rf32(m, o + 0x60))
}

/// `cWorldSphere::vf2_261590` 0x261590
pub fn world_sphere_target(m: &Mem, o: u32) -> f32 {
    if m.r32(o) == 0 {
        -1.0
    } else {
        let r = F(rf32(m, o + 0x60));
        (r + r).0
    }
}

/// `cWorldSphTree::vf10_261a48` 0x261a48
pub fn world_sph_tree_dtor(m: &mut Mem, env: &mut impl WorldTailEnv, o: u32, how: u32) {
    m.w32(o + 0x40, VT_WORLD_SPH_TREE);
    env.call(m, BX_SPHERE_TREE_DTOR, &[o + 0x60, 2]);
    if (how & 1) != 0 {
        env.call(m, MEM_DELETE, &[o]);
    }
}
