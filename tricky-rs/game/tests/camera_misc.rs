//! `tricky_game::camera_misc` (board row E11j) against the original in the function runner (TRICKY_ELF; skipped
//! without it): each case fills an arena with random words, builds what the function walks (tables, the eight
//! objects of a camera, the course's camera table, the loader's records), runs one function on both sides and
//! compares the calls out (their arguments, and f12 where it is passed), the answer and the memory.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::camera_misc::{self as cm, CamMiscEnv};
use tricky_game::pf::Mem;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
}

const LOW: (u32, u32) = (0, 0x100);
const STATICS: (u32, u32) = (0x336000, 0x2000);
const WORLDS: (u32, u32) = (0x346000, 0x1000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x4000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const OBJ: u32 = ARENA + 0x100;
const VTS: u32 = ARENA + 0x3000;
const COURSE: u32 = ARENA + 0x3400;
const TABLE: u32 = ARENA + 0x3500;
const REC: u32 = ARENA + 0x3600;
/// The stand-ins for the objects' destructors and the loader's virtual
const DTOR: u32 = 0x01f0_0000;
const RESOLVE: u32 = 0x01f0_0100;

const FUNCS: [u32; 21] = [
    0x16dcc0, 0x16e7c0, 0x1713d8, 0x171fb0, 0x1703b8, 0x1730d8, 0x16ddd0, 0x16e8f0, 0x1782d0, 0x179068, 0x1cb768,
    0x1cb718, 0x1cb8b0, 0x149448, 0x148478, 0x148218, 0x177dd8, 0x178dc8, 0x254b78, 0x2542a8, 0x254318,
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32) -> usize {
    match f {
        cm::LOAD_CML_FILES => 0,
        cm::TYPE_NODE_NEW => 3,
        cm::ANIM_FRAME_DESTRUCT | cm::TYPE_NODE_DESTRUCT | cm::CAM_SET_SHOT | cm::CAM_SCENE_DESTRUCT | DTOR
        | RESOLVE => 2,
        _ => 1,
    }
}
fn floats(f: u32) -> bool {
    f == cm::ANIM_FRAME_INIT || f == cm::ANIM_FRAME_EVAL
}

struct Env {
    log: Vec<Call>,
}
impl CamMiscEnv for Env {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func), 0);
        if func == cm::ANIM_FRAME_CLEAR {
            // the frame's table as the clear finds it
            w.push(m.r32(w[0] + 0x8c));
        }
        self.log.push(Call(func, w));
        0x5eed
    }
    fn call_f(&mut self, _m: &mut Mem, func: u32, args: &[u32], f12: f32) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func), 0);
        w.push(f12.to_bits());
        self.log.push(Call(func, w));
        0x5eed
    }
}

#[test]
fn camera_small_functions_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for func in [
        cm::MEM_DELETE, cm::ANIM_FRAME_CLEAR, cm::ANIM_FRAME_INIT, cm::ANIM_FRAME_EVAL, cm::ANIM_FRAME_DESTRUCT,
        cm::TYPE_NODE_NEW, cm::TYPE_NODE_DESTRUCT, cm::CAM_SET_SHOT, cm::CAM_SCENE_DESTRUCT, cm::LOAD_CML_FILES,
        cm::COURSE_DEBUG_PRINT, DTOR, RESOLVE,
    ] {
        let l = log.clone();
        r.stub(func, move |c, mem| {
            let mut w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            if func == cm::ANIM_FRAME_CLEAR {
                let mut b = [0u8; 4];
                mem.read(w[0] + 0x8c, &mut b).unwrap();
                w.push(u32::from_le_bytes(b));
            }
            if floats(func) {
                w.push(c.f(12).to_bits());
            }
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, 0x5eed);
        });
    }
    let mut g = Rng(0xca3e_0000_0000_0001);
    let mut calls = 0usize;
    for case in 0..FUNCS.len() as u32 * 400 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (LOW.0, vec![0; LOW.1 as usize]),
                (STATICS.0, vec![0; STATICS.1 as usize]),
                (WORLDS.0, vec![0; WORLDS.1 as usize]),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        for k in 0..LOW.1 / 4 {
            m.w32(4 * k, g.next());
        }
        // four tables: the destructor at +8 / +0xc, the loader's virtual at +0x100 / +0x104
        for k in 0..4 {
            let vt = VTS + 0x110 * k;
            m.w16(vt + 8, g.pick(&[0u16, 0, 4, 0xfff0]));
            m.w32(vt + 0xc, DTOR);
            m.w16(vt + 0x100, g.pick(&[0u16, 0, 8]));
            m.w32(vt + 0x104, RESOLVE);
        }
        let vt = |g: &mut Rng| VTS + 0x110 * g.below(4);
        for k in 0..8 {
            let t = vt(&mut g);
            m.w32(OBJ + 0x220 + 0x40 * k, t);
            let t = vt(&mut g);
            m.w32(cm::STATIC_OBJECTS + 0x40 * k, t);
            let t = vt(&mut g);
            m.w32(cm::STATIC_CAMERA + 0x220 + 0x40 * k, t);
        }
        m.w32(cm::CML_PENDING, g.pick(&[0, 1, 1, 2]));
        m.w32(OBJ + 0x11c, ARENA + 0x2000);
        let (c, t) = (g.pick(&[0, COURSE, COURSE]), g.pick(&[0, TABLE, TABLE]));
        m.w32(cm::WORLD + 4, c);
        m.w32(COURSE + 0x78, t);
        m.w32(REC + 0x18, g.below(16));
        let n = g.pick(&[0, 1, 2, 5, 8, u32::MAX]);
        // the loader's records, each starting with its size
        let mut p = REC;
        for _ in 0..8 {
            let s = 4 * (g.below(8) + 1);
            m.w32(p, s);
            p += s;
        }
        m.w32(OBJ + 0x10, vt(&mut g));
        let (fa, fb) = (f32::from_bits(g.next() & 0xbfff_ffff), g.next() as i32 as f32 / 65536.0);
        let how = g.pick(&[0, 1, 2, 3]);
        let base = g.next();
        let (a1, a2, a3) = match func {
            0x148218 => (ARENA + 0x2800, g.pick(&[REC, REC, REC, 0]), 0),
            0x179068 => (OBJ, 0, 0),
            0x254b78 => (cm::WORLD, g.below(16), 0),
            0x2542a8 => (n, REC, base),
            0x254318 => (n, REC, 0),
            _ => (how, 0, 0),
        };
        if func == 0x148218 && a2 == 0 {
            m.w32(0x18, g.below(16));
        }
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let none = 0x5eed_5eed;
        let (mm, e) = (&mut m, &mut env);
        let ours = match func {
            0x16dcc0 | 0x16e7c0 | 0x1713d8 | 0x171fb0 => { cm::offline_destruct(mm, e, OBJ, a1); none }
            0x1703b8 => { cm::view_update(mm, OBJ); none }
            0x1730d8 => { cm::lens_update(mm, OBJ); none }
            0x16ddd0 => { cm::camcamera_update(mm, OBJ); none }
            0x16e8f0 => { cm::set_shake(mm, OBJ, fa, fb); none }
            0x1782d0 => { cm::load_cml_once(mm, e); none }
            0x179068 => { cm::cml_free(mm, e, a1); none }
            0x1cb768 => { cm::anim_frame_eval(mm, e, OBJ, fa); none }
            0x1cb718 => cm::anim_frame_new(mm, e, OBJ, fa),
            0x1cb8b0 => { cm::anim_frame_destruct(mm, e, OBJ, a1); none }
            0x149448 => { cm::node_state_destruct(mm, e, OBJ, a1); none }
            0x148478 => { cm::node_destruct(mm, e, OBJ, a1); none }
            0x148218 => cm::node_new(mm, e, OBJ, a1, a2),
            0x177dd8 => { cm::bx_camera_destruct(mm, e, OBJ, a1); none }
            0x178dc8 => { cm::static_destruct(mm, e); none }
            0x254b78 => cm::course_get_camera(mm, a1, a2),
            0x2542a8 => cm::resolve_camera_pointers(mm, e, a1, a2, a3),
            0x254318 => cm::resolve_cameras(mm, e, OBJ, a1, a2),
            _ => unreachable!(),
        };
        calls += env.log.len();
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        let regs = if func == 0x254b78 { [a1, a2, 0, 0] } else { [OBJ, a1, a2, a3] };
        for (k, v) in regs.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *v as i32 as i64 as u64);
        }
        r.cpu.set_f(12, fa);
        r.cpu.set_f(13, fb);
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if ours != none {
            assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x}: the answer");
        }
        assert_eq!(*log.borrow(), env.log, "case {case} {func:#x}: the calls");
        for (at, b) in &m.regions {
            if *at == STACK.0 {
                continue;
            }
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    println!("{calls} calls");
    assert!(calls > 10000);
}
