//! The camera's small functions left over from E11 (board row E11k): the camera-file objects' constructors and
//! destructors, the camera node a trigger script spawns, the shot of a camera animation, the stage-area transform,
//! the shake, the region trigger, the cut fade and the camera files' names and loading. On the game's bytes (`Mem`)
//! with every call out through [`FeEnv`] (float arguments and results as their bits). Checked in the function
//! runner (`tests/camera_rest.rs`).

use crate::fe3dstate::FeEnv;
use crate::pf::Mem;
use crate::ps2f::F;

pub const CML_FIND: u32 = 0x16e518;
pub const CML_FIND_IN_SCOPE: u32 = 0x16e588;
pub const OFFLINE_DTOR: u32 = 0x16dcc0;
pub const SET_SHAKE: u32 = 0x16e8f0;
pub const GAME_GET_CAMERA: u32 = 0x181470;
pub const MEMPOOL_ALLOC: u32 = 0x23d3b0;
pub const EAMEM_SET: u32 = 0x2cc808;
pub const CAMERA_NODE_NEW: u32 = 0x148218;
pub const ANIM_FRAME_CLEAR: u32 = 0x1cb150;
pub const TYPE_OBJ_NODE_NEW: u32 = 0x17d048;
pub const NAME_HASH: u32 = 0x240ed8;
pub const FIND_CAM_ANIM: u32 = 0x255150;
pub const FIND_INSTANCE: u32 = 0x2550c8;
pub const MATH_ATAN: u32 = 0x2510f8;
pub const SPRINTF: u32 = 0x2fca28;
pub const FILE_EXISTS: u32 = 0x2c7e38;
pub const FILE_LOAD: u32 = 0x2c8018;
pub const CAMERA_CUT: u32 = 0x177db0;
pub const REGION_CONTAINS: u32 = 0x171610;

/// The vtables the constructors write: `CamAnimation` (0x37ee70), `CamCamera` (0x37f558), `CamDirector` (0x380200),
/// `CamPath` (0x3814c0), `CamScene` (0x382180), `CamAnimFrame`'s parts (0x391b10, 0x391ae0, 0x36c6b0)
pub const CAM_ANIMATION_VT: u32 = 0x37ee70;
pub const CAM_CAMERA_VT: u32 = 0x37f558;
pub const CAM_DIRECTOR_VT: u32 = 0x380200;
pub const CAM_PATH_VT: u32 = 0x3814c0;
pub const CAM_SCENE_VT: u32 = 0x382180;
pub const ANIM_FRAME_VT: u32 = 0x391b10;
pub const ANIM_FRAME_VT2: u32 = 0x391ae0;
pub const CAM_ANIM_FRAME_VT: u32 = 0x36c6b0;
/// The course's world (0x346488), the camera node's pool (0x345e48) and its name (0x36bbf0)
pub const COURSE_WORLD: u32 = 0x346488;
pub const NODE_POOL: u32 = 0x345e48;
pub const NODE_NAME: u32 = 0x36bbf0;
/// The stage areas' instance names (`Cam_StageAreaToWorld`: area 1 0x383068, area 2 0x383080)
pub const AREA1_NAME: u32 = 0x383068;
pub const AREA2_NAME: u32 = 0x383080;
/// The cut fade's words (0x3365f4 the forced count, 0x3365f8 / 0x3365fc started and its time)
pub const CUT_FADE_FORCE: u32 = 0x3365f4;
pub const CUT_FADE_ON: u32 = 0x3365f8;
pub const CUT_FADE_T: u32 = 0x3365fc;
/// `Camera_CutForReset`: the replay flag (0x337774) and the replay's camera (0x3368b0)
pub const REPLAY_ON: u32 = 0x337774;
pub const REPLAY_CAMERA: u32 = 0x3368b0;
/// The game's course id (0x32de88)
pub const COURSE_ID: u32 = 0x32de88;
/// The camera files' names: `"commonob"` (0x383830), `"scripts"` (0x383840), the courses' (0x383848..0x3838b8),
/// the directory format `"data/camera/%s"` (0x383900, 15 bytes), the file format `"%s.cml"` (0x37fba8)
pub const COMMON_CML: u32 = 0x383830;
pub const SCRIPTS_CML: u32 = 0x383840;
pub const CML_DIR_FMT: u32 = 0x383900;
pub const CML_FILE_FMT: u32 = 0x37fba8;
/// Where `Camera_LoadCmlFiles` keeps the three files: the course's (0x336688), common (0x336690), scripts (0x336698)
pub const CML_COURSE: u32 = 0x336688;
pub const CML_COMMON: u32 = 0x336690;
pub const CML_SCRIPTS: u32 = 0x336698;

fn f(m: &Mem, a: u32) -> F {
    F(f32::from_bits(m.r32(a)))
}
fn wf(m: &mut Mem, a: u32, v: F) {
    m.w32(a, v.0.to_bits());
}

/// `Boarder_GetCamera` 0x11ca88(b): the rider's camera (`Game_GetCamera` 0x181470 on its viewport +0x38), 0 for none
/// (-1)
pub fn boarder_get_camera(m: &mut Mem, e: &mut dyn FeEnv, b: u32) -> u32 {
    let v = m.r32(b + 0x38);
    if v == u32::MAX {
        return 0;
    }
    e.call(m, GAME_GET_CAMERA, &[v])
}

/// `TriggerOp_SpawnCameraNode` 0x13d0b8(thread, op): a camera node from the pool (`MemPool_Alloc(0x345e48, 0xc8,
/// 0x40000000, "...", 0, 0)`), filled with 0xdeadc0de when given, then `cCameraNode::cCameraNode(node, 3, op)` (also
/// on a null one); returns what that returns
pub fn trigger_op_spawn_camera_node(m: &mut Mem, e: &mut dyn FeEnv, op: u32) -> u32 {
    let p = e.call(m, MEMPOOL_ALLOC, &[NODE_POOL, 0xc8, 0x4000_0000, NODE_NAME, 0, 0]);
    if p != 0 {
        e.call(m, EAMEM_SET, &[p, 0xdead_c0de, 0xc8]);
    }
    e.call(m, CAMERA_NODE_NEW, &[p, 3, op])
}

/// `CamAnimFrame_Create` 0x1482c0(o, a1, shot): the frame at +0x1c (`AnimFrame_Clear`, its vtable at +0xa8 0x391b10
/// then 0x391ae0), the type node at +0xac (`cBxTypeObjNode::cBxTypeObjNode(o + 0xac, a1, 1000)`), its own vtable at
/// +0xb4 (0x36c6b0), then [`cam_anim_set_shot`]; returns `o`
pub fn cam_anim_frame_create(m: &mut Mem, e: &mut dyn FeEnv, o: u32, a1: u32, shot: u32) -> u32 {
    m.w32(o + 0xa8, ANIM_FRAME_VT);
    e.call(m, ANIM_FRAME_CLEAR, &[o + 0x1c, a1, shot]);
    m.w32(o + 0xa8, ANIM_FRAME_VT2);
    e.call(m, TYPE_OBJ_NODE_NEW, &[o + 0xac, a1, 1000]);
    m.w32(o + 0xb4, CAM_ANIM_FRAME_VT);
    cam_anim_set_shot(m, e, o, shot);
    o
}

/// `CamAnim_SetShot` 0x148350(o, shot): the camera animation of the shot's name (+0x10, `Name_Hash`, then
/// `Course_FindCamAnimByHash` in the course's world) at +0xc4; with one: the start (+8) and end (+0xc) frames from the
/// shot's +0 and +4 (a negative one: the animation's length +0x50) over 30, the rate (+0x10) the shot's +8 over 30 over
/// 60 (negated, and the end at +0x14, when the shot's +0xc is 1; else the start at +0x14), +0x18 cleared, the length
/// at +0x64, and its own virtual at +0x60
pub fn cam_anim_set_shot(m: &mut Mem, e: &mut dyn FeEnv, o: u32, shot: u32) {
    let name = m.r32(shot + 0x10);
    let h = e.call(m, NAME_HASH, &[name]);
    let a = e.call(m, FIND_CAM_ANIM, &[COURSE_WORLD, h]);
    m.w32(o + 0xc4, a);
    if a == 0 {
        return;
    }
    let thirty = F(30.0);
    let zero = F(0.0);
    let mut s = f(m, shot);
    if s.ee() < zero.ee() {
        s = f(m, a + 0x50);
    }
    let start = s / thirty;
    wf(m, o + 8, start);
    let e1 = f(m, shot + 4);
    let end = if e1.ee() < zero.ee() { f(m, a + 0x50) / thirty } else { e1 / thirty };
    wf(m, o + 0xc, end);
    let rate = (f(m, shot + 8) / thirty) * F(f32::from_bits(0x3c88_8889));
    wf(m, o + 0x10, rate);
    if m.r32(shot + 0xc) == 1 {
        wf(m, o + 0x14, end);
        wf(m, o + 0x10, -rate);
    } else {
        let v = m.r32(o + 8);
        m.w32(o + 0x14, v);
    }
    m.w32(o + 0x18, 0);
    let vt = m.r32(o + 0xb4);
    let len = m.r32(a + 0x54);
    m.w32(o + 0x64, len);
    let adj = m.r16(vt + 0x60) as i16 as u32;
    let fun = m.r32(vt + 0x64);
    e.call(m, fun, &[o.wrapping_add(adj)]);
}

/// A camera-file object: its vtable at +0, the named block (`Cml_FindObject(o, name, kind)`) at `at`
fn cml_object(m: &mut Mem, e: &mut dyn FeEnv, o: u32, vt: u32, at: u32, name: u32, kind: u32) -> u32 {
    m.w32(o, vt);
    let b = e.call(m, CML_FIND, &[o, name, kind]);
    m.w32(o + at, b);
    o
}

/// `CamAnimation::CamAnimation` 0x16d990(o, name): kind 1, the block at +4
pub fn cam_animation_new(m: &mut Mem, e: &mut dyn FeEnv, o: u32, name: u32) -> u32 {
    cml_object(m, e, o, CAM_ANIMATION_VT, 4, name, 1)
}

/// `CamAnimation_InitScoped` 0x16d9d0(o, scope, name): the same, by `Cml_FindObjectInScope(o, scope, name, 1)`
pub fn cam_animation_init_scoped(m: &mut Mem, e: &mut dyn FeEnv, o: u32, scope: u32, name: u32) -> u32 {
    m.w32(o, CAM_ANIMATION_VT);
    let b = e.call(m, CML_FIND_IN_SCOPE, &[o, scope, name, 1]);
    m.w32(o + 4, b);
    o
}

/// `CamCamera::CamCamera` 0x16dd68(o, name): kind 0, the block at +4
pub fn cam_camera_new(m: &mut Mem, e: &mut dyn FeEnv, o: u32, name: u32) -> u32 {
    cml_object(m, e, o, CAM_CAMERA_VT, 4, name, 0)
}

/// `CamDirector::CamDirector` 0x16e6b0(o, name): kind 2, the block at +4
pub fn cam_director_new(m: &mut Mem, e: &mut dyn FeEnv, o: u32, name: u32) -> u32 {
    cml_object(m, e, o, CAM_DIRECTOR_VT, 4, name, 2)
}

/// `CamPath::CamPath` 0x170470(o, name): kind 3, the block at +0x24
pub fn cam_path_new(m: &mut Mem, e: &mut dyn FeEnv, o: u32, name: u32) -> u32 {
    cml_object(m, e, o, CAM_PATH_VT, 0x24, name, 3)
}

/// `CamScene::CamScene` 0x1720c0(o, name): kind 5, the block at +8
pub fn cam_scene_new(m: &mut Mem, e: &mut dyn FeEnv, o: u32, name: u32) -> u32 {
    cml_object(m, e, o, CAM_SCENE_VT, 8, name, 5)
}

/// `CamAnimation::~CamAnimation` 0x16dcb8, `CamCamera::~CamCamera` 0x16ded0 and `CamScene::~CamScene` 0x172238
/// (o, flags): each jumps to `cOfflineObject::~cOfflineObject` 0x16dcc0
pub fn cml_object_drop(m: &mut Mem, e: &mut dyn FeEnv, o: u32, flags: u32) {
    e.call(m, OFFLINE_DTOR, &[o, flags]);
}

/// `Cml_LoadIfExists` 0x16e4c0(slot, name; sp on entry): `sprintf(buf, "%s.cml", name)` in its frame (sp − 0x120),
/// the slot cleared, and when `FileSys_Exists(buf)` the file (`File_LoadAlloc(buf, 0)`) in it
pub fn cml_load_if_exists(m: &mut Mem, e: &mut dyn FeEnv, slot: u32, name: u32, sp: u32) {
    let buf = sp.wrapping_sub(0x120);
    e.call(m, SPRINTF, &[buf, CML_FILE_FMT, name]);
    m.w32(slot, 0);
    if e.call(m, FILE_EXISTS, &[buf]) != 0 {
        let v = e.call(m, FILE_LOAD, &[buf, 0]);
        m.w32(slot, v);
    }
}

/// `CamRegion_CheckTrigger` 0x171e28(region, now, last, kind): kind 0 entering (`CamRegion_Contains` 0x171610 on
/// `now` and not on `last`), 1 leaving (on `last` and not on `now`), 2 inside (on `now`), else 0
pub fn cam_region_check_trigger(m: &mut Mem, e: &mut dyn FeEnv, region: u32, now: u32, last: u32, kind: u32) -> u32 {
    let mut inside = |m: &mut Mem, p: u32| e.call(m, REGION_CONTAINS, &[region, p]);
    match kind {
        0 => (inside(m, now) != 0 && inside(m, last) == 0) as u32,
        1 => (inside(m, last) != 0 && inside(m, now) == 0) as u32,
        2 => inside(m, now),
        _ => 0,
    }
}

/// `CamScene_GetActorPosition` 0x172100(scene, i, out, yaw): actor `i`'s record (0x30 bytes from the block's +8):
/// its position (+0x20..+0x28, w 1.0) into `out`, then [`cam_stage_area_to_world`] with its area (+0x1c)
pub fn cam_scene_get_actor_position(m: &mut Mem, e: &mut dyn FeEnv, scene: u32, i: u32, out: u32, yaw: u32) -> u32 {
    let rec = m.r32(scene + 8).wrapping_add(i.wrapping_mul(0x30));
    for k in 0..3 {
        let v = m.r32(rec + 0x20 + 4 * k);
        m.w32(out + 4 * k, v);
    }
    m.w32(out + 0xc, 0x3f80_0000);
    let area = m.r32(rec + 0x1c);
    cam_stage_area_to_world(m, e, area, out, yaw)
}

/// `Camera_CutForReset` 0x173a60(cam): `Camera_Cut` on the replay's camera (0x3368b0) while a replay runs (0x337774),
/// else on `cam`
pub fn camera_cut_for_reset(m: &mut Mem, e: &mut dyn FeEnv, cam: u32) {
    let c = if m.r32(REPLAY_ON) != 0 { REPLAY_CAMERA } else { cam };
    e.call(m, CAMERA_CUT, &[c]);
}

/// `Camera_StartCutFade` 0x174c30(): the fade's time cleared, the fade on
pub fn camera_start_cut_fade(m: &mut Mem) {
    m.w32(CUT_FADE_T, 0);
    m.w32(CUT_FADE_ON, 1);
}

/// `Camera_ForceCutFade` 0x174c48(): the forced fade count at 10
pub fn camera_force_cut_fade(m: &mut Mem) {
    m.w32(CUT_FADE_FORCE, 10);
}

/// `Cam_Shake` 0x177758(cam, p3, p2; sp on entry): a `CamCamera` on the camera's name (+0x24) in its frame (sp −
/// 0x40), its view (`CamCamera::GetView` 0x16dda8, which points the block's +0x11c at itself, asked twice), then
/// `Cam_SetShakeParams(view, p3, p2 × view's +0xcc)` and the local's destructor (flags 2)
pub fn cam_shake(m: &mut Mem, e: &mut dyn FeEnv, cam: u32, p3: f32, p2: f32, sp: u32) {
    let local = sp.wrapping_sub(0x40);
    cam_camera_new(m, e, local, cam + 0x24);
    let get_view = |m: &mut Mem| {
        let v = m.r32(local + 4);
        m.w32(v + 0x11c, v);
        v
    };
    let view = get_view(m);
    let v2 = get_view(m);
    let k = f(m, m.r32(v2 + 0x11c) + 0xcc);
    let amp = F(p2) * k;
    e.call(m, SET_SHAKE, &[view, p3.to_bits(), amp.0.to_bits()]);
    cml_object_drop(m, e, local, 2);
}

/// VU0's `vmulax` / `vmadday` / `vmaddaz` / `vmaddw`: the four rows at `mat` weighted by `v`'s lanes
fn vu0_transform(m: &Mem, mat: u32, v: [F; 4]) -> [F; 4] {
    let r = |k: u32, i: u32| f(m, mat + 0x10 * k + 4 * i);
    [0, 1, 2, 3].map(|i| ((r(0, i) * v[0] + r(1, i) * v[1]) + r(2, i) * v[2]) + r(3, i) * v[3])
}

/// `Cam_StageAreaToWorld` 0x1777d0(area, point, yaw): areas 1 and 2 are the course's instances named at 0x383068 /
/// 0x383080 (`Name_Hash`, `Course_FindInstanceByHash`); none (area 0, or no instance) returns 0. With one the point
/// (w set to 1.0) goes through its matrix (VU0), and `*yaw` is the angle of its x row: `atan(y / x)` (`Math_Atan`)
/// moved by ±π when x < 0, ±π/2 when x is 0, and y itself when both are 0; returns 1. (Any other area leaves the
/// name hash in v1 from the caller: not reached by the game's callers, here as area 0.)
pub fn cam_stage_area_to_world(m: &mut Mem, e: &mut dyn FeEnv, area: u32, point: u32, yaw: u32) -> u32 {
    let name = match area {
        1 => AREA1_NAME,
        2 => AREA2_NAME,
        _ => return 0,
    };
    let h = e.call(m, NAME_HASH, &[name]);
    if h == 0 {
        return 0;
    }
    let mat = e.call(m, FIND_INSTANCE, &[COURSE_WORLD, h]);
    if mat == 0 {
        return 0;
    }
    let one = F(1.0);
    let zero = F(0.0);
    wf(m, point + 0xc, one);
    let p = [0, 1, 2, 3].map(|k| f(m, point + 4 * k));
    let t = vu0_transform(m, mat, p);
    let w = vu0_transform(m, mat, [one, zero, zero, zero]);
    for (k, v) in t.iter().enumerate() {
        wf(m, point + 4 * k as u32, *v);
    }
    let (x, y) = (w[0], w[1]);
    let angle = if x.ee() == 0.0 {
        if y.ee() == 0.0 {
            y
        } else if 0.0 <= y.ee() {
            F(f32::from_bits(0x3fc9_0fdb))
        } else {
            F(f32::from_bits(0xbfc9_0fdb))
        }
    } else {
        let q = y / x;
        let mut a = F(f32::from_bits(e.call(m, MATH_ATAN, &[q.0.to_bits()])));
        if x.ee() < 0.0 {
            let pi = F(f32::from_bits(0x4049_0fdb));
            a = if 0.0 < y.ee() { a + pi } else { a - pi };
        }
        a
    };
    wf(m, yaw, angle);
    1
}

/// `Camera_GetCommonCmlName` 0x178310(): `"commonob"` (0x383830)
pub fn camera_get_common_cml_name() -> u32 {
    COMMON_CML
}

/// `Camera_GetScriptsCmlName` 0x178320(): `"scripts"` (0x383840)
pub fn camera_get_scripts_cml_name() -> u32 {
    SCRIPTS_CML
}

/// `Camera_GetCourseCmlName` 0x178330(): the course's camera file name (jump table 0x3838d0 on the course id
/// 0x32de88), 0 past 11; the same names as `tricky_data::intro::course_cml_name`
pub fn camera_get_course_cml_name(m: &Mem) -> u32 {
    const NAMES: [u32; 12] = [0x3838b8, 0x383850, 0x383870, 0x383860, 0x383898, 0x383880, 0x3838a0, 0x383888, 0x383878, 0x3838a0, 0x3838b0, 0x383848];
    NAMES.get(m.r32(COURSE_ID) as usize).copied().unwrap_or(0)
}

/// `Camera_LoadCmlFiles` 0x1783e8(; sp on entry): a copy of `"data/camera/%s"` (15 bytes) at the bottom of its frame
/// (sp − 0xc0, unused), then the course's file (when it has a name), common and scripts, each by `sprintf(sp − 0xb0,
/// "data/camera/%s", name)` and [`cml_load_if_exists`] into 0x336688 / 0x336690 / 0x336698
pub fn camera_load_cml_files(m: &mut Mem, e: &mut dyn FeEnv, sp: u32) {
    let frame = sp.wrapping_sub(0xc0);
    for k in 0..15 {
        let b = m.r8(CML_DIR_FMT + k);
        m.w8(frame + k, b);
    }
    let buf = frame + 0x10;
    let course = camera_get_course_cml_name(m);
    if course != 0 {
        e.call(m, SPRINTF, &[buf, CML_DIR_FMT, course]);
        cml_load_if_exists(m, e, CML_COURSE, buf, frame);
    }
    for (name, slot) in [(camera_get_common_cml_name(), CML_COMMON), (camera_get_scripts_cml_name(), CML_SCRIPTS)] {
        e.call(m, SPRINTF, &[buf, CML_DIR_FMT, name]);
        cml_load_if_exists(m, e, slot, buf, frame);
    }
}
