//! Runner checks for board row E11k (`tricky_game::camera_rest`, the camera functions left unchecked) against the
//! original (TRICKY_ELF; skipped without it). Every call out is a stub on both sides that logs its arguments (float
//! ones as bits) and answers from one hash of the case's seed, the callee and the call's place in the log (pointers
//! into the test's block, floats as floats); the calls, the results, the block, the globals and the frames the
//! functions write are compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::ps2float::Rules;
use r5900::Runner;
use tricky_game::camera_rest::*;
use tricky_game::fe3dstate::FeEnv;
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
}

const SIZE: u32 = 0x4000;
/// Where pointer answers land (0x100 slots of 0x10 bytes, each with room for a matrix or a block's +0x11c)
const PTRS: u32 = 0x2000;
const VT_FN: u32 = 0x7e4000;

fn hash(seed: u64, func: u32, n: usize) -> u64 {
    let mut h = seed ^ (func as u64) << 32 ^ n as u64;
    h = (h ^ h >> 30).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ h >> 27).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ h >> 31
}

/// The answer of a call out (as the 32 bits of v0, or of f0 for `Math_Atan`).
fn answer(seed: u64, base: u32, func: u32, n: usize) -> u32 {
    let h = hash(seed, func, n);
    match func {
        CML_FIND => base + PTRS + 16 * (h % 0x100) as u32,
        FIND_CAM_ANIM | FIND_INSTANCE if h % 3 != 0 => base + PTRS + 16 * (h % 0x100) as u32,
        MATH_ATAN => ((h % 4001) as f32 / 1000.0 - 2.0).to_bits(),
        _ if h % 3 == 0 => 0,
        _ => (h >> 8) as u32,
    }
}

type Log = Rc<RefCell<(u64, u32, Vec<(u32, Vec<u32>)>)>>;

struct RecFe(Log);
impl FeEnv for RecFe {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut l = self.0.borrow_mut();
        let v = answer(l.0, l.1, func, l.2.len());
        l.2.push((func, args.to_vec()));
        v
    }
}

/// A float as the tests' data: mostly ordinary, some special.
fn float(g: &mut Rng) -> u32 {
    match g.below(8) {
        0 => [0u32, 0x8000_0000, 0x3f80_0000, 0xbf80_0000, 0x0000_0001, 0x7f80_0000][g.below(6) as usize],
        1 => g.next(),
        2 => g.below(5),
        _ => ((g.below(200_001) as f32 - 100_000.0) / 100.0).to_bits(),
    }
}

#[test]
fn camera_rest_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    // natively, inside the functions that call them (each also checked on its own)
    for a in [0x16dd68u32, 0x16dda8, 0x1777d0, 0x16e4c0, 0x178330, 0x178310, 0x178320, 0x148350] {
        r.allow(a);
    }
    // `CamAnimFrame_Create`'s own vtable's entry +0x60 (its `CamAnim_SetShot` calls it)
    let frame_fn = r.mem.read_u32(CAM_ANIM_FRAME_VT + 0x64);
    let log: Log = Rc::new(RefCell::new((0, 0, vec![])));
    let outs = [
        CML_FIND, CML_FIND_IN_SCOPE, OFFLINE_DTOR, SET_SHAKE, GAME_GET_CAMERA, MEMPOOL_ALLOC, EAMEM_SET, CAMERA_NODE_NEW, ANIM_FRAME_CLEAR, TYPE_OBJ_NODE_NEW, NAME_HASH,
        FIND_CAM_ANIM, FIND_INSTANCE, MATH_ATAN, SPRINTF, FILE_EXISTS, FILE_LOAD, CAMERA_CUT, REGION_CONTAINS, frame_fn,
    ];
    for f in outs.into_iter().chain((0..0x20).map(|k| VT_FN + 8 * k)) {
        let l = log.clone();
        r.stub(f, move |c, _| {
            let mut l = l.borrow_mut();
            let v = answer(l.0, l.1, f, l.2.len());
            let args: Vec<u32> = match f {
                SET_SHAKE => vec![c.gpr(4) as u32, c.f(12).to_bits(), c.f(13).to_bits()],
                MATH_ATAN => vec![c.f(12).to_bits()],
                _ => [4, 5, 6, 7, 8, 9].iter().map(|&k| c.gpr(k) as u32).collect(),
            };
            l.2.push((f, args));
            if f == MATH_ATAN {
                c.set_f(0, f32::from_bits(v));
            } else {
                c.set_gpr(2, v as i32 as i64 as u64);
            }
        });
    }
    let base = r.mem.alloc(SIZE);
    r.cpu.set_gpr(4, 0);
    r.call(0x178310).unwrap();
    let sp = r.cpu.gpr(29) as u32;
    let globals: Vec<(u32, u32)> = vec![(CUT_FADE_FORCE, 12), (CML_COURSE, 0x18), (REPLAY_ON, 4), (COURSE_ID, 4)];
    let statics: Vec<(u32, Vec<u8>)> = [(CML_DIR_FMT, 16u32), (CAM_ANIM_FRAME_VT + 0x60, 8)]
        .iter()
        .map(|&(a, n)| {
            let mut b = vec![0u8; n as usize];
            r.mem.read(a, &mut b).unwrap();
            (a, b)
        })
        .collect();
    let saved: Vec<(u32, Vec<u8>)> = globals
        .iter()
        .map(|&(a, n)| {
            let mut b = vec![0u8; n as usize];
            r.mem.read(a, &mut b).unwrap();
            (a, b)
        })
        .collect();

    let mut g = Rng(0x0e11_6b00_1777_d001);
    const N: u32 = 25;
    let mut seen = [0u32; N as usize];
    for case in 0..60_000u32 {
        let which = if case < N * 40 { case % N } else { g.below(N) };
        seen[which as usize] += 1;
        let mut words: Vec<u32> = (0..SIZE / 4).map(|_| float(&mut g)).collect();
        // a quarter of the cases: the matrices and blocks of zeros, negative zeros and ±1 (the yaw's edge cases)
        if g.below(4) == 0 {
            for w in &mut words[(PTRS / 4) as usize..(PTRS / 4 + 0x500) as usize] {
                *w = [0u32, 0x8000_0000, 0x8000_0001, 0x3f80_0000, 0xbf80_0000][g.below(5) as usize];
            }
        }
        let img: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        r.mem.write(base, &img).unwrap();
        let o = base;
        let shot = base + 0x1800;
        let scene = base + 0x1900;
        let out = base + 0x1a00;
        let yaw = base + 0x1a40;
        let vt = base + 0x1b00;
        // the object's vtable (`CamAnim_SetShot`'s virtual at +0x60)
        r.mem.write_u32(o + 0xb4, vt);
        r.mem.write_u16(vt + 0x60, (g.below(11) as u16).wrapping_sub(5));
        r.mem.write_u32(vt + 0x64, VT_FN + 0x60);
        r.mem.write_u32(shot + 0xc, g.below(3));
        // the scene's actor records (0x30 bytes): their areas 0..2 (another area reads v1 left by the caller)
        r.mem.write_u32(scene + 8, base + 0x1c00);
        for k in 0..6 {
            r.mem.write_u32(base + 0x1c00 + 0x30 * k + 0x1c, g.below(3));
        }
        r.mem.write_u32(base + 0x100 + 0x38, if g.below(3) == 0 { u32::MAX } else { g.below(4) });
        r.mem.write_u32(CUT_FADE_FORCE, g.next());
        r.mem.write_u32(CUT_FADE_ON, g.next());
        r.mem.write_u32(CUT_FADE_T, g.next());
        r.mem.write_u32(REPLAY_ON, g.below(2));
        r.mem.write_u32(COURSE_ID, g.below(14));
        for k in 0..6 {
            r.mem.write_u32(CML_COURSE + 4 * k, g.next());
        }
        let (a1, a2, a3) = (g.next(), g.next(), g.next());
        let (p3, p2) = (f32::from_bits(float(&mut g)), f32::from_bits(float(&mut g)));
        let seed = g.next() as u64 | (g.next() as u64) << 32;

        // the stack below the call, random (the frames the functions write over)
        let junk: Vec<u8> = (0..0x200).map(|_| g.next() as u8).collect();
        r.mem.write(sp - 0x200, &junk).unwrap();
        let mut m = Mem::default();
        let mut snap = vec![0u8; SIZE as usize];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut stack = vec![0u8; 0x200];
        r.mem.read(sp - 0x200, &mut stack).unwrap();
        m.regions.push((sp - 0x200, stack));
        for &(a, n) in &globals {
            let mut b = vec![0u8; n as usize];
            r.mem.read(a, &mut b).unwrap();
            m.regions.push((a, b));
        }
        m.regions.extend(statics.iter().cloned());

        *log.borrow_mut() = (seed, base, vec![]);
        let i = g.below(6);
        let (addr, args): (u32, Vec<u32>) = match which {
            0 => (0x11ca88, vec![base + 0x100]),
            1 => (0x13d0b8, vec![a1, a2]),
            2 => (0x1482c0, vec![o, a1, shot]),
            3 => (0x148350, vec![o, shot]),
            4 => (0x16d990, vec![o, a1]),
            5 => (0x16d9d0, vec![o, a1, a2]),
            6 => (0x16dd68, vec![o, a1]),
            7 => (0x16e6b0, vec![o, a1]),
            8 => (0x170470, vec![o, a1]),
            9 => (0x1720c0, vec![o, a1]),
            10 => (0x16dcb8, vec![o, a1]),
            11 => (0x16ded0, vec![o, a1]),
            12 => (0x172238, vec![o, a1]),
            13 => (0x16e4c0, vec![base + 0x40, a1]),
            14 => (0x171e28, vec![a1, a2, a3, g.below(4)]),
            15 => (0x172100, vec![scene, i, out, yaw]),
            16 => (0x173a60, vec![a1]),
            17 => (0x174c30, vec![]),
            18 => (0x174c48, vec![]),
            19 => (0x177758, vec![base + 0x300]),
            20 => (0x1777d0, vec![g.below(3), out, yaw]),
            21 => (0x178310, vec![]),
            22 => (0x178320, vec![]),
            23 => (0x178330, vec![]),
            _ => (0x1783e8, vec![]),
        };
        for k in 4..8 {
            r.cpu.set_gpr(k, args.get(k - 4).copied().unwrap_or(0xbad0_0000 + k as u32) as i32 as i64 as u64);
        }
        r.cpu.set_f(12, p3);
        r.cpu.set_f(13, p2);
        r.cpu.set_gpr(2, 0xdead_beef);
        r.call(addr).unwrap_or_else(|e| panic!("case {case} {addr:#x}: {e}"));
        let theirs_ret = r.cpu.gpr(2) as u32;
        let theirs_log = std::mem::take(&mut log.borrow_mut().2);

        let e = &mut RecFe(log.clone());
        let a = &args;
        let mine_ret: Option<u32> = match which {
            0 => Some(boarder_get_camera(&mut m, e, a[0])),
            1 => Some(trigger_op_spawn_camera_node(&mut m, e, a[1])),
            2 => Some(cam_anim_frame_create(&mut m, e, o, a[1], shot)),
            3 => {
                cam_anim_set_shot(&mut m, e, o, shot);
                None
            }
            4 => Some(cam_animation_new(&mut m, e, o, a[1])),
            5 => Some(cam_animation_init_scoped(&mut m, e, o, a[1], a[2])),
            6 => Some(cam_camera_new(&mut m, e, o, a[1])),
            7 => Some(cam_director_new(&mut m, e, o, a[1])),
            8 => Some(cam_path_new(&mut m, e, o, a[1])),
            9 => Some(cam_scene_new(&mut m, e, o, a[1])),
            10..=12 => {
                cml_object_drop(&mut m, e, o, a[1]);
                None
            }
            13 => {
                cml_load_if_exists(&mut m, e, a[0], a[1], sp);
                None
            }
            14 => Some(cam_region_check_trigger(&mut m, e, a[0], a[1], a[2], a[3])),
            15 => Some(cam_scene_get_actor_position(&mut m, e, scene, i, out, yaw)),
            16 => {
                camera_cut_for_reset(&mut m, e, a[0]);
                None
            }
            17 => {
                camera_start_cut_fade(&mut m);
                None
            }
            18 => {
                camera_force_cut_fade(&mut m);
                None
            }
            19 => {
                cam_shake(&mut m, e, a[0], p3, p2, sp);
                None
            }
            20 => Some(cam_stage_area_to_world(&mut m, e, a[0], out, yaw)),
            21 => Some(camera_get_common_cml_name()),
            22 => Some(camera_get_scripts_cml_name()),
            23 => {
                let v = camera_get_course_cml_name(&m);
                // the app's names (`tricky_data::intro`) are the same strings
                let id = m.r32(COURSE_ID);
                let want = tricky_data::intro::course_cml_name(id);
                let got = (v != 0).then(|| {
                    let mut b = vec![0u8; 16];
                    r.mem.read(v, &mut b).unwrap();
                    String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap()]).into_owned()
                });
                assert_eq!(want.map(str::to_owned), got, "course {id}");
                Some(v)
            }
            _ => {
                camera_load_cml_files(&mut m, e, sp);
                None
            }
        };
        let mine_log = std::mem::take(&mut log.borrow_mut().2);
        let ctx = format!("case {case} which {which} ({addr:#x})");
        assert_eq!(theirs_log.len(), mine_log.len(), "{ctx}: calls out {theirs_log:x?} vs {mine_log:x?}");
        for (k, (t, p)) in theirs_log.iter().zip(&mine_log).enumerate() {
            assert_eq!(t.0, p.0, "{ctx}: call {k}");
            let need = if t.0 == MEMPOOL_ALLOC { 6 } else { 1 };
            assert!(p.1.len() >= need, "{ctx}: call {k} ({:#x}) with {} arguments", t.0, p.1.len());
            assert_eq!(&t.1[..p.1.len()], &p.1[..], "{ctx}: arguments of call {k} ({:#x})", t.0);
        }
        if let Some(v) = mine_ret {
            assert_eq!(theirs_ret, v, "{ctx}: result");
        }
        let mut theirs = vec![0u8; SIZE as usize];
        r.mem.read(base, &mut theirs).unwrap();
        if theirs != m.regions[0].1 {
            let at = theirs.iter().zip(&m.regions[0].1).position(|(x, y)| x != y).unwrap();
            panic!("{ctx}: memory differs at +{at:#x}: game {:#x} port {:#x}", theirs[at], m.regions[0].1[at]);
        }
        for (k, &(a, n)) in globals.iter().enumerate() {
            let mut b = vec![0u8; n as usize];
            r.mem.read(a, &mut b).unwrap();
            assert_eq!(b, m.regions[2 + k].1, "{ctx}: globals at {a:#x}");
        }
        // the frames written: `Cam_Shake`'s local camera, `Camera_LoadCmlFiles`' copy of its format
        let frame = match which {
            19 => Some((sp - 0x40, 8u32)),
            24 => Some((sp - 0xc0, 15)),
            _ => None,
        };
        if let Some((at, n)) = frame {
            let mut b = vec![0u8; n as usize];
            r.mem.read(at, &mut b).unwrap();
            let o = (at - (sp - 0x200)) as usize;
            assert_eq!(b, m.regions[1].1[o..o + n as usize], "{ctx}: frame");
        }
    }
    for (a, b) in saved {
        r.mem.write(a, &b).unwrap();
    }
    eprintln!("cases per function: {seen:?}");
}
