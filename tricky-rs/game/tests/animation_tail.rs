//! Runner checks for board row G5e (last 14 `not started` `animation` functions):
//! - `camera_misc`: `StaticDtor_AnimLayer` 0x15a980, `AnimFrame_Init` 0x1cb078,
//!   `cMeshAnimFrame::cMeshAnimFrame` 0x1cb278, `AnimFrame_MulMatrix` 0x1cb658,
//!   `cMeshAnimFrame::~cMeshAnimFrame` 0x1cb990, `cAnimFrame::~cAnimFrame` 0x1cba40,
//!   `cAnimFrame::cAnimFrame` 0x1cba68
//! - `objdraw`: `cMeshAnimNode::~cMeshAnimNode` 0x146ef0, `cAnimObjectNode::~cAnimObjectNode` 0x198da8,
//!   `cAnimComboNode::Draw` 0x1992e0, `cAnimComboNode::~cAnimComboNode` 0x19a1a8,
//!   `cAnimTexFlipNode::Draw` 0x19a8b0, `cAnimTexFlipNode::vf28_19a8f0` 0x19a8f0,
//!   `cAnimTexFlipNode::~cAnimTexFlipNode` 0x19ac48
//! (TRICKY_ELF; skipped without it).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::camera_misc::{
    anim_frame_ctor, anim_frame_dtor, anim_frame_init, anim_frame_mul_matrix, mesh_anim_frame_ctor,
    mesh_anim_frame_dtor, static_dtor_anim_layer, CamMiscEnv, ANIM_DATA_DESTRUCT, ANIM_FRAME_CLEAR,
    ANIM_FRAME_SET_KEY, IDENTITY_MAT4, MEM_DELETE as CAM_MEM_DELETE, MESH_ANIM_FRAME_EVAL,
};
use tricky_game::objdraw::{
    anim_combo_destruct, anim_combo_draw, anim_object_destruct, anim_tex_flip_destruct, anim_tex_flip_draw,
    anim_tex_flip_vf28, mesh_anim_node_destruct, ObjEnv, ANIM_OBJECT_VF28, CONTACT_REMOVE_FOR_NODE,
    CONTROLLER_NODE_DTOR, GAPP, MEM_DELETE, MEM_FREE, TEX_FLIP_APPLY, TEX_FLIP_DTOR,
};
use tricky_game::pf::Mem;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        let u = (self.next() as f32) / (u32::MAX as f32);
        lo + u * (hi - lo)
    }
}

type Log = Rc<RefCell<Vec<(u32, Vec<u32>)>>>;

struct RecCam(Log);
impl CamMiscEnv for RecCam {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.0.borrow_mut().push((func, args.to_vec()));
        0
    }
    fn call_f(&mut self, _m: &mut Mem, func: u32, args: &[u32], f12: f32) -> u32 {
        let mut v = args.to_vec();
        v.push(f12.to_bits());
        self.0.borrow_mut().push((func, v));
        0
    }
}

#[test]
fn anim_frames_and_static_dtor_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    r.allow(0x1cb078);
    r.allow(0x1cba40);
    let base = r.mem.alloc(0x400);
    let log: Log = Rc::new(RefCell::new(vec![]));

    for (f, arity, has_f12) in [
        (ANIM_DATA_DESTRUCT, 2usize, false),
        (ANIM_FRAME_CLEAR, 1, false),
        (ANIM_FRAME_SET_KEY, 2, true),
        (MESH_ANIM_FRAME_EVAL, 1, true),
        (CAM_MEM_DELETE, 1, false),
    ] {
        let l2 = log.clone();
        r.stub(f, move |c, _| {
            let mut v: Vec<u32> = (0..arity).map(|i| c.gpr(4 + i) as u32).collect();
            if has_f12 {
                v.push(c.f(12).to_bits());
            }
            l2.borrow_mut().push((f, v));
        });
    }

    let mut id_bytes = vec![0u8; 0x40];
    r.mem.read(IDENTITY_MAT4, &mut id_bytes).unwrap();

    let mut g = Rng(0x1cb0_78ee_feed_0001);
    for case in 0..2_000u32 {
        let mut init = vec![0u8; 0x400];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(base, &init).unwrap();

        // Test both 16-byte aligned and misaligned frame/matrix pointers
        let f_ptr = base + ((g.below(4)) << 2);
        let mat_ptr = base + 0x200 + ((g.below(4)) << 2);
        for k in 0..16u32 {
            r.mem.write_u32((f_ptr + 0x90 & !15) + 4 * k, g.f(-20.0, 20.0).to_bits());
            r.mem.write_u32((mat_ptr & !15) + 4 * k, g.f(-20.0, 20.0).to_bits());
        }

        let key = g.next();
        let t = g.f(-10.0, 100.0);
        let how = g.below(4);
        let which = case % 7;

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x400];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        m.regions.push((IDENTITY_MAT4, id_bytes.clone()));

        log.borrow_mut().clear();
        let ret_theirs = match which {
            0 => {
                r.call(0x15a980).unwrap_or_else(|e| panic!("StaticDtor_AnimLayer {case}: {e}"));
                0
            }
            1 => {
                r.cpu.set_gpr(4, f_ptr as u64);
                r.call(0x1cba68).unwrap_or_else(|e| panic!("cAnimFrame ctor {case}: {e}"));
                r.cpu.gpr(2) as u32
            }
            2 => {
                r.cpu.set_gpr(4, f_ptr as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x1cba40).unwrap_or_else(|e| panic!("cAnimFrame dtor {case}: {e}"));
                0
            }
            3 => {
                r.cpu.set_gpr(4, f_ptr as u64);
                r.cpu.set_gpr(5, key as u64);
                r.cpu.set_f(12, t);
                r.call(0x1cb078).unwrap_or_else(|e| panic!("AnimFrame_Init {case}: {e}"));
                r.cpu.gpr(2) as u32
            }
            4 => {
                r.cpu.set_gpr(4, f_ptr as u64);
                r.cpu.set_gpr(5, key as u64);
                r.cpu.set_f(12, t);
                r.call(0x1cb278).unwrap_or_else(|e| panic!("cMeshAnimFrame ctor {case}: {e}"));
                r.cpu.gpr(2) as u32
            }
            5 => {
                r.cpu.set_gpr(4, f_ptr as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x1cb990).unwrap_or_else(|e| panic!("cMeshAnimFrame dtor {case}: {e}"));
                0
            }
            _ => {
                r.cpu.set_gpr(4, f_ptr as u64);
                r.cpu.set_gpr(5, mat_ptr as u64);
                r.call(0x1cb658).unwrap_or_else(|e| panic!("AnimFrame_MulMatrix {case}: {e}"));
                0
            }
        };
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x400];
        r.mem.read(base, &mut theirs_mem).unwrap();

        let env = &mut RecCam(log.clone());
        let ret_mine = match which {
            0 => {
                static_dtor_anim_layer(&mut m, env);
                0
            }
            1 => anim_frame_ctor(&mut m, env, f_ptr),
            2 => {
                anim_frame_dtor(&mut m, env, f_ptr, how);
                0
            }
            3 => anim_frame_init(&mut m, env, f_ptr, key, t),
            4 => mesh_anim_frame_ctor(&mut m, env, f_ptr, key, t),
            5 => {
                mesh_anim_frame_dtor(&mut m, env, f_ptr, how);
                0
            }
            _ => {
                anim_frame_mul_matrix(&mut m, f_ptr, mat_ptr);
                0
            }
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
    }
}

struct RecObj(Log);
impl ObjEnv for RecObj {
    fn cull(&mut self, _m: &mut Mem, _func: u32, _this: u32, _min: [u32; 4], _max: [u32; 4]) -> u32 {
        unreachable!()
    }
    fn vcall(&mut self, _m: &mut Mem, func: u32, this: u32, args: &[u32]) -> u32 {
        let mut v = vec![this];
        v.extend_from_slice(args);
        self.0.borrow_mut().push((func, v));
        0
    }
    fn draw_model(
        &mut self,
        _m: &mut Mem,
        _func: u32,
        _this: u32,
        _model: u32,
        _part: u32,
        _matrix: u32,
        _lights: u32,
        _bones: u32,
    ) {
        unreachable!()
    }
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.0.borrow_mut().push((func, args.to_vec()));
        0
    }
}

#[test]
fn anim_nodes_draw_and_dtors_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.allow(0x198da8);
    let base = r.mem.alloc(0x2000);
    let (app, mgr, node, inst, vt_draw, vt_frame, frames_blk) = (
        base,
        base + 0x800,
        base + 0x900,
        base + 0x1100,
        base + 0x1300,
        base + 0x1500,
        base + 0x1600,
    );
    let log: Log = Rc::new(RefCell::new(vec![]));

    const FRAME_DTOR_FN: u32 = 0x7e1000;
    const DRAW_VF_FN: u32 = 0x7e1010;

    for (f, arity) in [
        (MEM_DELETE, 1usize),
        (MEM_FREE, 1),
        (CONTROLLER_NODE_DTOR, 2),
        (CONTACT_REMOVE_FOR_NODE, 2),
        (TEX_FLIP_APPLY, 1),
        (TEX_FLIP_DTOR, 2),
        (ANIM_OBJECT_VF28, 3),
        (FRAME_DTOR_FN, 2),
        (DRAW_VF_FN, 3),
    ] {
        let l2 = log.clone();
        r.stub(f, move |c, _| {
            let v: Vec<u32> = (0..arity).map(|i| c.gpr(4 + i) as u32).collect();
            l2.borrow_mut().push((f, v));
        });
    }

    let mut g = Rng(0x198d_a8cc_a11e_0002);
    for case in 0..2_100u32 {
        let mut init = vec![0u8; 0x2000];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(base, &init).unwrap();

        r.mem.write_u32(GAPP, app);
        r.mem.write_u32(app + 0x730, mgr);
        r.mem.write_u32(mgr + 0x30, g.next());

        // Setup vtable for frame virtual destructor (+8 adj, +0xc fn) and Draw virtual (+0xe8 adj, +0xec fn)
        r.mem.write_u16(vt_frame + 8, ((g.below(9) as i16) - 4) as u16);
        r.mem.write_u32(vt_frame + 0xc, FRAME_DTOR_FN);
        r.mem.write_u16(vt_draw + 0xe8, ((g.below(9) as i16) - 4) as u16);
        r.mem.write_u32(vt_draw + 0xec, DRAW_VF_FN);

        let which = case % 7;
        let how = g.below(4);
        let matrix = g.next();
        let test = g.below(2);

        // Configure fields for the chosen function
        match which {
            0 => {
                // cMeshAnimNode::~cMeshAnimNode
                let p = if g.below(2) == 0 { 0 } else { g.next() | 4 };
                r.mem.write_u32(node + 0x6c0, p);
            }
            1 | 3 | 6 => {
                // cAnimObjectNode / cAnimComboNode / cAnimTexFlipNode dtor
                let anim_off = match which {
                    1 => 0u32,
                    3 => 0x1c,
                    _ => 0x18,
                };
                if which == 3 {
                    let p84 = if g.below(2) == 0 { 0 } else { g.next() | 4 };
                    r.mem.write_u32(node + 0x84, p84);
                }
                let contact = if g.below(2) == 0 { 0 } else { g.next() | 4 };
                r.mem.write_u32(node + anim_off + 0x64, contact);
                if g.below(3) == 0 {
                    r.mem.write_u32(node + anim_off + 0x60, 0);
                } else {
                    let cnt = g.below(5); // 0..=4 elements of 0xd0 bytes
                    r.mem.write_u32(frames_blk, cnt);
                    let arr = frames_blk + 0x10;
                    for k in 0..cnt {
                        r.mem.write_u32(arr + k * 0xd0 + 0x8c, vt_frame);
                    }
                    r.mem.write_u32(node + anim_off + 0x60, arr);
                }
            }
            2 => {
                // cAnimComboNode::Draw
                r.mem.write_u32(node + 0x38, vt_draw);
                r.mem.write_u32(node + 0x58, inst);
                r.mem.write_u32(inst + 0xe8, g.next());
            }
            4 => {
                // cAnimTexFlipNode::Draw
                r.mem.write_u32(node + 0x50, vt_draw);
                r.mem.write_u32(node + 0x70, inst);
                r.mem.write_u32(inst + 0xe8, g.next());
            }
            _ => {}
        }

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x2000];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut gb = vec![0u8; 4];
        r.mem.read(GAPP, &mut gb).unwrap();
        m.regions.push((GAPP, gb));

        log.borrow_mut().clear();
        match which {
            0 => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x146ef0).unwrap_or_else(|e| panic!("cMeshAnimNode dtor {case}: {e}"));
            }
            1 => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x198da8).unwrap_or_else(|e| panic!("cAnimObjectNode dtor {case}: {e}"));
            }
            2 => {
                r.cpu.set_gpr(4, node as u64);
                r.call(0x1992e0).unwrap_or_else(|e| panic!("cAnimComboNode Draw {case}: {e}"));
            }
            3 => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x19a1a8).unwrap_or_else(|e| panic!("cAnimComboNode dtor {case}: {e}"));
            }
            4 => {
                r.cpu.set_gpr(4, node as u64);
                r.call(0x19a8b0).unwrap_or_else(|e| panic!("cAnimTexFlipNode Draw {case}: {e}"));
            }
            5 => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, matrix as u64);
                r.cpu.set_gpr(6, test as u64);
                r.call(0x19a8f0).unwrap_or_else(|e| panic!("cAnimTexFlipNode vf28 {case}: {e}"));
            }
            _ => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x19ac48).unwrap_or_else(|e| panic!("cAnimTexFlipNode dtor {case}: {e}"));
            }
        }
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x2000];
        r.mem.read(base, &mut theirs_mem).unwrap();

        let env = &mut RecObj(log.clone());
        match which {
            0 => mesh_anim_node_destruct(&mut m, env, node, how),
            1 => anim_object_destruct(&mut m, env, node, how),
            2 => anim_combo_draw(&mut m, env, node),
            3 => anim_combo_destruct(&mut m, env, node, how),
            4 => anim_tex_flip_draw(&mut m, env, node),
            5 => anim_tex_flip_vf28(&mut m, env, node, matrix, test),
            _ => anim_tex_flip_destruct(&mut m, env, node, how),
        }
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
    }
}
