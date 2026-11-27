//! The camera's small functions (board row E11j): the camera node a trigger script spawns (`cCameraNode`), the
//! camera animation frame (`cCamAnimFrame`), the offline objects' destructor and its three users, `cBxCamera`'s
//! destructor and the cameras' static destructor, the two empty per-frame steps, the shake's parameters, the once-only
//! camera file load, a camera file handle's free, and the course's camera table (`Course_GetCamera` and the course
//! loader's two camera fix-ups). Everything they call outside this file goes through [`CamMiscEnv`].

use crate::pf::Mem;

/// The calls out: the function's address and its integer arguments; `f12` the float argument where there is one
pub trait CamMiscEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
    fn call_f(&mut self, m: &mut Mem, func: u32, args: &[u32], f12: f32) -> u32;
}

pub const MEM_DELETE: u32 = 0x114f80;
pub const ANIM_FRAME_CLEAR: u32 = 0x1cb150;
pub const ANIM_FRAME_INIT: u32 = 0x1cb078;
/// `cAnimFrame::vf1` 0x1cb300 (the curves evaluated at a time), `cAnimFrame::~cAnimFrame` 0x1cba40
pub const ANIM_FRAME_EVAL: u32 = 0x1cb300;
pub const ANIM_FRAME_DESTRUCT: u32 = 0x1cba40;
pub const TYPE_NODE_NEW: u32 = 0x17d048;
pub const TYPE_NODE_DESTRUCT: u32 = 0x17ddc0;
pub const CAM_SET_SHOT: u32 = 0x148350;
pub const CAM_SCENE_DESTRUCT: u32 = 0x172238;
pub const LOAD_CML_FILES: u32 = 0x1783e8;
pub const COURSE_DEBUG_PRINT: u32 = 0x255310;

/// The tables: `cOfflineObject` 0x37f350, `cAnimFrame` 0x391b10, `cCamAnimFrame` 0x391ae0, the camera node's own
/// (at +0xb4) 0x36c6b0, `cBxCamera`'s (at +0xeb8) 0x3830a8
pub const OFFLINE_VT: u32 = 0x37f350;
pub const ANIM_FRAME_VT: u32 = 0x391b10;
pub const CAM_ANIM_FRAME_VT: u32 = 0x391ae0;
pub const CAMERA_NODE_VT: u32 = 0x36c6b0;
pub const BX_CAMERA_VT: u32 = 0x3830a8;
/// The world (its +4 the course, whose +0x78 is the camera table)
pub const WORLD: u32 = 0x346488;
/// Set while the camera files are still to be loaded
pub const CML_PENDING: u32 = 0x33777c;
/// The two static cameras: a `cBxCamera` at 0x3368b0, and eight objects of 0x40 at 0x3366a0
pub const STATIC_CAMERA: u32 = 0x3368b0;
pub const STATIC_OBJECTS: u32 = 0x3366a0;
/// The course loader's messages (0x3a6f10, 0x3a6f30)
pub const RESOLVE_POINTERS_TEXT: u32 = 0x3a6f10;
pub const RESOLVE_CAMERAS_TEXT: u32 = 0x3a6f30;

/// A virtual call through the table at `obj`+0: the adjust halfword at `off`, the function at `off + 4`
fn vcall(m: &mut Mem, e: &mut dyn CamMiscEnv, obj: u32, off: u32, rest: &[u32]) -> u32 {
    let vt = m.r32(obj);
    let me = obj.wrapping_add(m.r16(vt + off) as i16 as u32);
    let mut a = vec![me];
    a.extend_from_slice(rest);
    e.call(m, m.r32(vt + off + 4), &a)
}

/// `cOfflineObject::~cOfflineObject` 0x16dcc0(o, how), and the three that are only a jump to it:
/// `CamDirector::~CamDirector` 0x16e7c0, `CamPath::~CamPath` 0x1713d8, `CamRegion::~CamRegion` 0x171fb0. The
/// table back, deleted when `how` has bit 0
pub fn offline_destruct(m: &mut Mem, e: &mut dyn CamMiscEnv, o: u32, how: u32) {
    m.w32(o, OFFLINE_VT);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[o]);
    }
}

/// `CamView_Update` 0x1703b8 and `CamLens_Update` 0x1730d8: nothing
pub fn view_update(_m: &mut Mem, _view: u32) {}
pub fn lens_update(_m: &mut Mem, _lens: u32) {}

/// `CamCamera_Update` 0x16ddd0(c): the view (`c`+4) and its lens (+0x120) stepped (both do nothing)
pub fn camcamera_update(m: &mut Mem, c: u32) {
    let v = m.r32(c + 4);
    view_update(m, v);
    let v = m.r32(c + 4);
    lens_update(m, v + 0x120);
}

/// `Cam_SetShakeParams` 0x16e8f0(c, a: f12, b: f13): the shake (the view at `c`+0x11c): +0xc4 = b, +0xb8 = a
pub fn set_shake(m: &mut Mem, c: u32, a: f32, b: f32) {
    let v = m.r32(c + 0x11c);
    m.w32(v + 0xc4, b.to_bits());
    m.w32(v + 0xb8, a.to_bits());
}

/// `Camera_LoadCmlOnce` 0x1782d0: the camera files loaded (`Camera_LoadCmlFiles` 0x1783e8) when 0x33777c is 1;
/// 0x33777c cleared either way
pub fn load_cml_once(m: &mut Mem, e: &mut dyn CamMiscEnv) {
    if m.r32(CML_PENDING) == 1 {
        e.call(m, LOAD_CML_FILES, &[]);
    }
    m.w32(CML_PENDING, 0);
}

/// `Cml_FreeHandle` 0x179068(h): the block `h` holds deleted
pub fn cml_free(m: &mut Mem, e: &mut dyn CamMiscEnv, h: u32) {
    let p = m.r32(h);
    e.call(m, MEM_DELETE, &[p]);
}

/// `cCamAnimFrame::vf1` 0x1cb768(f, t): a jump to `cAnimFrame::vf1` 0x1cb300 (the curves at time `t`)
pub fn anim_frame_eval(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32, t: f32) {
    e.call_f(m, ANIM_FRAME_EVAL, &[f], t);
}

/// `cCamAnimFrame::cCamAnimFrame` 0x1cb718(f, t): `AnimFrame_Init`, its table, the curves at `t`; `f`
pub fn anim_frame_new(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32, t: f32) -> u32 {
    e.call_f(m, ANIM_FRAME_INIT, &[f], t);
    m.w32(f + 0x8c, CAM_ANIM_FRAME_VT);
    anim_frame_eval(m, e, f, t);
    f
}

/// `cCamAnimFrame::~cCamAnimFrame` 0x1cb8b0(f, how): its table, then `cAnimFrame::~cAnimFrame` 0x1cba40
pub fn anim_frame_destruct(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32, how: u32) {
    m.w32(f + 0x8c, CAM_ANIM_FRAME_VT);
    e.call(m, ANIM_FRAME_DESTRUCT, &[f, how]);
}

/// `CameraNodeState_Destruct` 0x149448(n, how): the animation frame (+0x1c) destroyed, `n` deleted when `how` has
/// bit 0
pub fn node_state_destruct(m: &mut Mem, e: &mut dyn CamMiscEnv, n: u32, how: u32) {
    anim_frame_destruct(m, e, n + 0x1c, 2);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[n]);
    }
}

/// `cCameraNode::~cCameraNode` 0x148478(n, how): the node's table (+0xb4), its list node (+0xac) destroyed
/// (0x17ddc0), then [`node_state_destruct`]
pub fn node_destruct(m: &mut Mem, e: &mut dyn CamMiscEnv, n: u32, how: u32) {
    m.w32(n + 0xb4, CAMERA_NODE_VT);
    e.call(m, TYPE_NODE_DESTRUCT, &[n + 0xac, 0]);
    node_state_destruct(m, e, n, how);
}

/// `cCameraNode::cCameraNode` 0x148218(n, list, rec): the animation frame (+0x1c, its table at +0xa8) cleared, the
/// list node (+0xac, type 1000), the node's table (+0xb4), the course camera of `rec`+0x18 (+0xc4), the shot from
/// `rec`+8 (0 without a record); `n`
pub fn node_new(m: &mut Mem, e: &mut dyn CamMiscEnv, n: u32, list: u32, rec: u32) -> u32 {
    m.w32(n + 0xa8, ANIM_FRAME_VT);
    e.call(m, ANIM_FRAME_CLEAR, &[n + 0x1c]);
    m.w32(n + 0xa8, CAM_ANIM_FRAME_VT);
    e.call(m, TYPE_NODE_NEW, &[n + 0xac, list, 1000]);
    let i = m.r32(rec + 0x18);
    m.w32(n + 0xb4, CAMERA_NODE_VT);
    let cam = course_get_camera(m, WORLD, i);
    m.w32(n + 0xc4, cam);
    let shot = if rec == 0 { 0 } else { rec + 8 };
    e.call(m, CAM_SET_SHOT, &[n, shot]);
    n
}

/// The destructor of an object at `o` whose table (+0) has it at +8 / +0xc, called with 0
fn destruct_at(m: &mut Mem, e: &mut dyn CamMiscEnv, o: u32) {
    vcall(m, e, o, 8, &[0]);
}

/// `cBxCamera::~cBxCamera` 0x177dd8(c, how): its table (+0xeb8), the eight objects of 0x40 from +0x220 destroyed
/// last first, the scene (+0x214, 0x172238) and the path (+0x1e8, [`offline_destruct`]), deleted when `how` has
/// bit 0
pub fn bx_camera_destruct(m: &mut Mem, e: &mut dyn CamMiscEnv, c: u32, how: u32) {
    m.w32(c + 0xeb8, BX_CAMERA_VT);
    let first = c.wrapping_add(0x220);
    if first != 0 && first != c.wrapping_add(0x420) {
        let mut o = c.wrapping_add(0x3e0);
        loop {
            destruct_at(m, e, o);
            if o == first {
                break;
            }
            o = o.wrapping_sub(0x40);
        }
    }
    e.call(m, CAM_SCENE_DESTRUCT, &[c.wrapping_add(0x214), 2]);
    offline_destruct(m, e, c.wrapping_add(0x1e8), 2);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[c]);
    }
}

/// `Camera_StaticDtor` 0x178dc8: the static camera (0x3368b0) destroyed, then the eight objects at 0x3366a0 last
/// first
pub fn static_destruct(m: &mut Mem, e: &mut dyn CamMiscEnv) {
    bx_camera_destruct(m, e, STATIC_CAMERA, 2);
    let mut o = STATIC_OBJECTS + 0x1c0;
    loop {
        destruct_at(m, e, o);
        if o == STATIC_OBJECTS {
            break;
        }
        o -= 0x40;
    }
}

/// `Course_GetCamera` 0x254b78(w, i): camera `i` of the world's course (`w`+4, its table at +0x78); 0 without a
/// course or a table
pub fn course_get_camera(m: &Mem, w: u32, i: u32) -> u32 {
    let c = m.r32(w + 4);
    if c == 0 {
        return 0;
    }
    let t = m.r32(c + 0x78);
    if t == 0 {
        return 0;
    }
    m.r32((i << 2).wrapping_add(t))
}

/// `cCourseResolve_Common::ResolveCameraPointers` 0x2542a8(o, n, p, base): its message, then the `n` words at `p`
/// made pointers (`base` added); 1
pub fn resolve_camera_pointers(m: &mut Mem, e: &mut dyn CamMiscEnv, n: u32, p: u32, base: u32) -> u32 {
    e.call(m, COURSE_DEBUG_PRINT, &[RESOLVE_POINTERS_TEXT]);
    let (mut n, mut p) = (n, p);
    if (n as i32) > 0 {
        loop {
            n = n.wrapping_sub(1);
            let v = m.r32(p).wrapping_add(base);
            m.w32(p, v);
            p += 4;
            if n == 0 {
                break;
            }
        }
    }
    1
}

/// `cCourseResolve_Common::ResolveCameras` 0x254318(o, n, p): its message, then each of the `n` records at `p`
/// (each starting with its size) handed to the loader's virtual (`o`+0x10's table, +0x100 / +0x104); 1
pub fn resolve_cameras(m: &mut Mem, e: &mut dyn CamMiscEnv, o: u32, n: u32, p: u32) -> u32 {
    e.call(m, COURSE_DEBUG_PRINT, &[RESOLVE_CAMERAS_TEXT]);
    let (mut n, mut p) = (n, p);
    if (n as i32) > 0 {
        loop {
            let vt = m.r32(o + 0x10);
            n = n.wrapping_sub(1);
            let me = o.wrapping_add(m.r16(vt + 0x100) as i16 as u32);
            let f = m.r32(vt + 0x104);
            e.call(m, f, &[me, p]);
            let step = m.r32(p);
            p = p.wrapping_add(step);
            if n == 0 {
                break;
            }
        }
    }
    1
}

/// `cMeshAnimFrame`'s table (0x391af8), `gIdentityMat4` (0x346440), `gAnimData` (0x3211a8),
/// `AnimData_Destruct` 0x15a8c0, `AnimFrame_SetKey` 0x1cb0d8, `cMeshAnimFrame::vf1` 0x1cb498
pub const MESH_ANIM_FRAME_VT: u32 = 0x391af8;
pub const IDENTITY_MAT4: u32 = 0x346440;
pub const ANIM_DATA_GLOBAL: u32 = 0x3211a8;
pub const ANIM_DATA_DESTRUCT: u32 = 0x15a8c0;
pub const ANIM_FRAME_SET_KEY: u32 = 0x1cb0d8;
pub const MESH_ANIM_FRAME_EVAL: u32 = 0x1cb498;

/// `StaticDtor_AnimLayer` 0x15a980(): tail-calls `AnimData_Destruct(0x3211a8, 2)` (`0x15a8c0`)
pub fn static_dtor_anim_layer(m: &mut Mem, e: &mut dyn CamMiscEnv) {
    e.call(m, ANIM_DATA_DESTRUCT, &[ANIM_DATA_GLOBAL, 2]);
}

/// `cAnimFrame::cAnimFrame` 0x1cba68(f): installs `ANIM_FRAME_VT` at `+0x8c`, calls `AnimFrame_Clear` (`0x1cb150`),
/// and returns `f`
pub fn anim_frame_ctor(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32) -> u32 {
    m.w32(f + 0x8c, ANIM_FRAME_VT);
    e.call(m, ANIM_FRAME_CLEAR, &[f]);
    f
}

/// `cAnimFrame::~cAnimFrame` 0x1cba40(f, how): restores `ANIM_FRAME_VT` at `+0x8c`, and calls `Mem_Delete(f)` when
/// `how` has bit 0
pub fn anim_frame_dtor(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32, how: u32) {
    m.w32(f + 0x8c, ANIM_FRAME_VT);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[f]);
    }
}

/// `AnimFrame_Init` 0x1cb078(f, key; t): installs `ANIM_FRAME_VT` at `+0x8c`, calls `AnimFrame_Clear` (`0x1cb150`),
/// calls `AnimFrame_SetKey(f, key; t)` (`0x1cb0d8`), and returns `f`
pub fn anim_frame_init(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32, key: u32, t: f32) -> u32 {
    m.w32(f + 0x8c, ANIM_FRAME_VT);
    e.call(m, ANIM_FRAME_CLEAR, &[f]);
    e.call_f(m, ANIM_FRAME_SET_KEY, &[f, key], t);
    f
}

/// `cMeshAnimFrame::cMeshAnimFrame` 0x1cb278(f, key; t): calls [`anim_frame_init`], installs `MESH_ANIM_FRAME_VT`
/// at `+0x8c`, copies the 64-byte matrix at `0x346440` into `(f+0x90) & !15`, stores `key` at `+0x48`, calls
/// `cMeshAnimFrame::vf1(f; t)` (`0x1cb498`), and returns `f`
pub fn mesh_anim_frame_ctor(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32, key: u32, t: f32) -> u32 {
    anim_frame_init(m, e, f, key, t);
    m.w32(f + 0x8c, MESH_ANIM_FRAME_VT);
    let dst = (f.wrapping_add(0x90)) & !15;
    for k in 0..16u32 {
        let w = m.r32(IDENTITY_MAT4 + 4 * k);
        m.w32(dst + 4 * k, w);
    }
    m.w32(f + 0x48, key);
    e.call_f(m, MESH_ANIM_FRAME_EVAL, &[f], t);
    f
}

/// `cMeshAnimFrame::~cMeshAnimFrame` 0x1cb990(f, how): restores `MESH_ANIM_FRAME_VT` at `+0x8c` and tail-calls
/// [`anim_frame_dtor`]
pub fn mesh_anim_frame_dtor(m: &mut Mem, e: &mut dyn CamMiscEnv, f: u32, how: u32) {
    m.w32(f + 0x8c, MESH_ANIM_FRAME_VT);
    anim_frame_dtor(m, e, f, how);
}

/// `AnimFrame_MulMatrix` 0x1cb658(f, mat): multiplies each row of `(f+0x90) & !15` through the four rows of
/// `mat & !15` (VU0 `vmulax`/`vmadday`/`vmaddaz`/`vmaddw`) and stores the 4×4 product back at `(f+0x90) & !15`
pub fn anim_frame_mul_matrix(m: &mut Mem, f: u32, mat: u32) {
    use crate::ps2f::F;
    let a = mat & !15;
    let b = f.wrapping_add(0x90) & !15;
    let row = |m: &Mem, base: u32, i: u32| -> [F; 4] {
        [0, 1, 2, 3].map(|j| F(f32::from_bits(m.r32(base + 0x10 * i + 4 * j))))
    };
    let ra = [row(m, a, 0), row(m, a, 1), row(m, a, 2), row(m, a, 3)];
    let rb = [row(m, b, 0), row(m, b, 1), row(m, b, 2), row(m, b, 3)];
    let out: [[F; 4]; 4] = std::array::from_fn(|i| {
        let v = rb[i];
        std::array::from_fn(|c| ((ra[0][c] * v[0] + ra[1][c] * v[1]) + ra[2][c] * v[2]) + ra[3][c] * v[3])
    });
    for (i, r) in out.iter().enumerate() {
        for (j, x) in r.iter().enumerate() {
            m.w32(b + 0x10 * i as u32 + 4 * j as u32, x.0.to_bits());
        }
    }
}
