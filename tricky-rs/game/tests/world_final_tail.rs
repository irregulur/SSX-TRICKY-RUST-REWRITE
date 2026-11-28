//! Function-runner check for row E9o: the final 33 `not started` `world` functions,
//! verified bit-for-bit against `SLUS_203.26` (`md5 162580fd65611e72a90deed640a70aef`).

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::worldtail::{self as wt, WorldTailEnv};

const BASE: u32 = 0x0140_0000;
const SCRATCH_BASE: u32 = 0x0150_0000;

// Synthetic virtual target addresses (mapped via runner stubs, never in real ELF).
const FAKE_NODE_DEL: u32 = 0x002f_f600;
const FAKE_CTL_C8: u32 = 0x002f_f610;
const FAKE_GFX_284: u32 = 0x002f_f620;
const FAKE_GFX_2AC: u32 = 0x002f_f630;
const FAKE_OBJ_EC: u32 = 0x002f_f640;
const FAKE_CROWD_CB: u32 = 0x002f_f650;
const FAKE_GFX_26C: u32 = 0x002f_f660;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        let u = (self.next() >> 8) as f32 / (1u32 << 24) as f32;
        lo + (hi - lo) * u
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    r.allow(0x0023_91c8); // SphTree_Assert
    r.allow(0x0023_9750); // SphTree_ForEachLeafRecurse
    r.allow(0x0023_ab78); // SphTree_PlaneRecurse
    Some(r)
}

fn call_u32(r: &mut Runner, addr: u32, args: &[u32]) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn call_f_ret_u32(r: &mut Runner, addr: u32, args: &[u32], f12: f32) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    r.cpu.fpr[12] = f12.to_bits();
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn call_ret_f32(r: &mut Runner, addr: u32, args: &[u32]) -> f32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    r.call(addr).unwrap();
    f32::from_bits(r.cpu.fpr[0])
}

fn fresh_mem(r: &mut Runner) -> Mem {
    let z = vec![0u8; 0x20000];
    r.mem.write(BASE, &z);
    let zg = vec![0u8; 0x1000];
    r.mem.write(0x0033_7000, &zg);
    r.mem.write(0x0034_6000, &zg);
    // Keep real ELF bytes at 0x0039_fbd8 (8-byte crowd stop callback) mirrored in Mem!
    let mut cb_bytes = vec![0u8; 0x100];
    r.mem.read(0x0039_fb00, &mut cb_bytes).unwrap();
    Mem {
        regions: vec![
            (0x0033_7000, vec![0u8; 0x1000]),
            (0x0034_6000, vec![0u8; 0x1000]),
            (0x0039_fb00, cb_bytes),
            (BASE, vec![0u8; 0x20000]),
            (SCRATCH_BASE, vec![0u8; 0x10000]),
        ],
    }
}

fn write_both_u32(r: &mut Runner, m: &mut Mem, a: u32, v: u32) {
    r.mem.write_u32(a, v);
    m.w32(a, v);
}

fn write_both_f32(r: &mut Runner, m: &mut Mem, a: u32, v: f32) {
    write_both_u32(r, m, a, v.to_bits());
}

fn write_both_u16(r: &mut Runner, m: &mut Mem, a: u32, v: u16) {
    r.mem.write_u16(a, v);
    m.w16(a, v);
}

fn write_both_u8(r: &mut Runner, m: &mut Mem, a: u32, v: u8) {
    r.mem.write(a, &[v]);
    m.w8(a, v);
}

fn write_both_vec4(r: &mut Runner, m: &mut Mem, a: u32, v: [f32; 4]) {
    for k in 0..4 {
        write_both_f32(r, m, a + (k as u32) * 4, v[k]);
    }
}

fn copy_out(r: &mut Runner, at: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    r.mem.read(at, &mut b).unwrap();
    b
}

fn mem_slice(m: &Mem, at: u32, len: usize) -> Vec<u8> {
    (0..len).map(|i| m.r8(at + i as u32)).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CallRec {
    target: u32,
    args: Vec<u32>,
    f12_bits: u32,
    a1_64: Option<u64>,
    snap: Vec<u8>,
}

#[derive(Default, Clone)]
struct EnvState {
    calls: Vec<CallRec>,
    scratch_cur: u32,
    // Programmable responses
    name_hash_ret: u32,
    find_inst_ret: u32,
    cull_seq: Vec<u32>,
    cull_idx: usize,
    bx_obj_ret: u32,
    sound_keys: [u32; 4],
    hit_only_key: u32,
    cache_get_ret: u32,
    sph_recurse_ret: u32,
    sph_pen_out: [f32; 4],
    sph_hit_out: [f32; 4],
    ray_ret: f32,
    ray_hit_kind: u32,
    ray_hit_z: f32,
    ray_hit_surf: u32,
    ray_hit_normal: [f32; 4],
    tri_ret: u32,
    box_ret: u32,
}

impl EnvState {
    fn new() -> Self {
        Self { scratch_cur: SCRATCH_BASE, ..Self::default() }
    }
}

struct TestEnv(Rc<RefCell<EnvState>>);

fn stub_vec4_scale(m: &mut Mem, dst: u32, src: u32, s: f32) {
    for k in 0..4u32 {
        let v = f32::from_bits(m.r32(src + k * 4)) * s;
        m.w32(dst + k * 4, v.to_bits());
    }
}

fn stub_mat4_transform_vec4(m: &mut Mem, dst: u32, mat: u32, v: u32) {
    let mut rows = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            rows[r][c] = f32::from_bits(m.r32(mat + (r as u32) * 16 + (c as u32) * 4));
        }
    }
    let vx = f32::from_bits(m.r32(v));
    let vy = f32::from_bits(m.r32(v + 4));
    let vz = f32::from_bits(m.r32(v + 8));
    let vw = f32::from_bits(m.r32(v + 12));
    for c in 0..4 {
        let o = rows[0][c] * vx + rows[1][c] * vy + rows[2][c] * vz + rows[3][c] * vw;
        m.w32(dst + (c as u32) * 4, o.to_bits());
    }
}

fn stub_sincos(ang: f32) -> (f32, f32) {
    (ang.sin(), ang.cos())
}

fn stub_atan(x: f32) -> f32 {
    x.atan()
}

impl WorldTailEnv for TestEnv {
    fn scratch(&mut self, size: u32) -> u32 {
        let mut s = self.0.borrow_mut();
        let aligned = (size + 15) & !15;
        let ptr = s.scratch_cur;
        s.scratch_cur += aligned;
        ptr
    }

    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        self.call_f(m, target, args, 0.0)
    }

    fn call_f(&mut self, m: &mut Mem, target: u32, args: &[u32], f12: f32) -> u32 {
        let mut s = self.0.borrow_mut();
        let snap = match target {
            FAKE_GFX_284 => {
                let mut v = mem_slice(m, args[1], 16);
                v.extend_from_slice(&mem_slice(m, args[2], 16));
                v
            }
            FAKE_GFX_2AC => mem_slice(m, args[3], 0x40),
            FAKE_OBJ_EC => mem_slice(m, args[1], 0x40),
            FAKE_GFX_26C => {
                let mut v = mem_slice(m, args[1], 16);
                v.extend_from_slice(&mem_slice(m, args[2], 16));
                v
            }
            wt::WORLD_LINE_CTOR => {
                let mut v = mem_slice(m, args[1], 16);
                v.extend_from_slice(&mem_slice(m, args[2], 16));
                v
            }
            wt::TRI_CONTAINS_POINT_VU0 => mem_slice(m, args[3], 16),
            _ => Vec::new(),
        };
        // Strip stack/scratch pointer values from recorded args when they are local stack slots,
        // keeping semantic arguments & snapped contents identical to the runner.
        let norm_args: Vec<u32> = match target {
            wt::VEC4_SCALE | wt::MAT4_TRANSFORM_VEC4 | wt::LIGHTS_CLEAR => vec![],
            wt::MATH_SIN_COS => vec![],
            wt::SPLINE_PATH_NODE_PLACE_ALL | wt::SPLINE_PATH_NODE_1FAAD0 => vec![args[0]],
            FAKE_GFX_284 => vec![args[0]],
            FAKE_GFX_2AC => vec![args[0], args[1], args[2], args[5], args[6]],
            FAKE_OBJ_EC => vec![args[0], args[2]],
            FAKE_GFX_26C => vec![args[0], args[3]],
            wt::WORLD_INSTANCE_GET_SOUND_KEYS => vec![args[0], args[1]],
            wt::WORLD_LINE_CTOR => vec![args[3]],
            wt::WORLD_RAY_CAST => vec![args[0], args[3]],
            wt::COLLIDE_SPHERE_SPH_TREE => vec![args[0], args[1], args[3]],
            _ => args.to_vec(),
        };
        s.calls.push(CallRec { target, args: norm_args, f12_bits: f12.to_bits(), a1_64: None, snap });

        match target {
            wt::NAME_HASH => s.name_hash_ret,
            wt::COURSE_FIND_INSTANCE_BY_HASH => s.find_inst_ret,
            wt::VEC4_SCALE => {
                stub_vec4_scale(m, args[0], args[1], f12);
                0
            }
            wt::MAT4_TRANSFORM_VEC4 => {
                stub_mat4_transform_vec4(m, args[0], args[1], args[2]);
                0
            }
            wt::MATH_SIN_COS => {
                let (sn, cs) = stub_sincos(f12);
                m.w32(args[0], sn.to_bits());
                m.w32(args[1], cs.to_bits());
                0
            }
            wt::MATH_ATAN => stub_atan(f12).to_bits(),
            FAKE_GFX_284 => {
                let idx = s.cull_idx;
                s.cull_idx += 1;
                s.cull_seq.get(idx).copied().unwrap_or(0)
            }
            wt::BX_OBJ_NODE_FROM_LINK => s.bx_obj_ret,
            wt::WORLD_INSTANCE_GET_SOUND_KEYS => {
                for k in 0..4u32 {
                    m.w32(args[2 + k as usize], s.sound_keys[k as usize]);
                }
                0
            }
            wt::WORLD_EMITTER_IS_HIT_ONLY_KEY => (args[1] == s.hit_only_key) as u32,
            wt::SPHERE_TREE_CACHE_GET => s.cache_get_ret,
            wt::SPH_TREE_SPHERE_RECURSE => {
                if s.sph_recurse_ret != 0 {
                    for k in 0..4u32 {
                        m.w32(args[5] + k * 4, s.sph_pen_out[k as usize].to_bits());
                        m.w32(args[6] + k * 4, s.sph_hit_out[k as usize].to_bits());
                    }
                }
                s.sph_recurse_ret
            }
            wt::COLLIDE_SPHERE_SPH_TREE => {
                if s.sph_recurse_ret != 0 {
                    for k in 0..4u32 {
                        m.w32(args[2] + k * 4, s.sph_pen_out[k as usize].to_bits());
                        m.w32(args[3] + k * 4, s.sph_hit_out[k as usize].to_bits());
                    }
                }
                s.sph_recurse_ret
            }
            wt::WORLD_RAY_CAST => {
                let hit = args[2];
                m.w32(hit, s.ray_hit_kind);
                m.w32(hit + 0x18, s.ray_hit_z.to_bits());
                m.w32(hit + 0x34, s.ray_hit_surf);
                for k in 0..4u32 {
                    m.w32(hit + 0x20 + k * 4, s.ray_hit_normal[k as usize].to_bits());
                }
                s.ray_ret.to_bits()
            }
            wt::TRI_CONTAINS_POINT_VU0 => s.tri_ret,
            wt::COLLIDE_SPHERE_BOX => s.box_ret,
            _ => 0,
        }
    }

    fn call_u64(&mut self, _m: &mut Mem, target: u32, a0: u32, a1: u64) -> u32 {
        let mut s = self.0.borrow_mut();
        s.calls.push(CallRec { target, args: vec![a0], f12_bits: 0, a1_64: Some(a1), snap: Vec::new() });
        0
    }
}

fn install_stubs(r: &mut Runner, log: Rc<RefCell<EnvState>>) {
    let simple = [
        (wt::MEM_DELETE, 1usize, false),
        (wt::MEM_FREE, 1, false),
        (wt::BX_SORT_OBJ_NODE_DTOR, 2, false),
        (wt::SPLINE_PATH_NODE_FREE_MEM, 2, false),
        (wt::SPLINE_PATH_NODE_DRAW_PATH, 2, false),
        (wt::PTR_ARRAY_FREE_ALL, 1, false),
        (wt::CROWD_BOX_DESTRUCT, 2, false),
        (wt::CROWD_BOX_STOP_ALL, 1, true),
        (wt::CROWD_STOP_CHEER, 1, false),
        (wt::CROWD_SYS_STOP_ALL, 1, false),
        (wt::COLL_BODY_DESTRUCT, 2, false),
        (wt::BX_SPHERE_TREE_DTOR, 2, false),
        (wt::SPHERE_TREE_DTOR, 2, false),
        (wt::SPHERE_TREE_BUF_CACHE_DTOR, 2, false),
        (wt::SPH_TREE_SORT_OCTANTS, 2, false),
        (FAKE_NODE_DEL, 2, false),
        (FAKE_CTL_C8, 3, false),
    ];
    for (addr, nargs, has_f12) in simple {
        let st = log.clone();
        r.stub(addr, move |cpu, _mem| {
            let args: Vec<u32> = (0..nargs).map(|k| cpu.gpr(4 + k) as u32).collect();
            let f12_bits = if has_f12 { cpu.fpr[12] } else { 0 };
            st.borrow_mut().calls.push(CallRec { target: addr, args, f12_bits, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, 0);
        });
    }

    // NAME_HASH
    {
        let st = log.clone();
        r.stub(wt::NAME_HASH, move |cpu, _mem| {
            let a0 = cpu.gpr(4) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::NAME_HASH, args: vec![a0], f12_bits: 0, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, s.name_hash_ret as u64);
        });
    }
    // COURSE_FIND_INSTANCE_BY_HASH
    {
        let st = log.clone();
        r.stub(wt::COURSE_FIND_INSTANCE_BY_HASH, move |cpu, _mem| {
            let a0 = cpu.gpr(4) as u32;
            let a1 = cpu.gpr(5) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::COURSE_FIND_INSTANCE_BY_HASH, args: vec![a0, a1], f12_bits: 0, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, s.find_inst_ret as u64);
        });
    }
    // VEC4_SCALE
    {
        let st = log.clone();
        r.stub(wt::VEC4_SCALE, move |cpu, mem| {
            let dst = cpu.gpr(4) as u32;
            let src = cpu.gpr(5) as u32;
            let f12 = f32::from_bits(cpu.fpr[12]);
            st.borrow_mut().calls.push(CallRec { target: wt::VEC4_SCALE, args: vec![], f12_bits: cpu.fpr[12], a1_64: None, snap: Vec::new() });
            for k in 0..4u32 {
                let v = f32::from_bits(mem.read_u32(src + k * 4)) * f12;
                mem.write_u32(dst + k * 4, v.to_bits());
            }
            cpu.set_gpr(2, 0);
        });
    }
    // MAT4_TRANSFORM_VEC4
    {
        let st = log.clone();
        r.stub(wt::MAT4_TRANSFORM_VEC4, move |cpu, mem| {
            let dst = cpu.gpr(4) as u32;
            let mat = cpu.gpr(5) as u32;
            let v = cpu.gpr(6) as u32;
            st.borrow_mut().calls.push(CallRec { target: wt::MAT4_TRANSFORM_VEC4, args: vec![], f12_bits: 0, a1_64: None, snap: Vec::new() });
            let mut rows = [[0.0f32; 4]; 4];
            for row in 0..4 {
                for col in 0..4 {
                    rows[row][col] = f32::from_bits(mem.read_u32(mat + (row as u32) * 16 + (col as u32) * 4));
                }
            }
            let vx = f32::from_bits(mem.read_u32(v));
            let vy = f32::from_bits(mem.read_u32(v + 4));
            let vz = f32::from_bits(mem.read_u32(v + 8));
            let vw = f32::from_bits(mem.read_u32(v + 12));
            for col in 0..4 {
                let o = rows[0][col] * vx + rows[1][col] * vy + rows[2][col] * vz + rows[3][col] * vw;
                mem.write_u32(dst + (col as u32) * 4, o.to_bits());
            }
            cpu.set_gpr(2, 0);
        });
    }
    // MATH_ATAN
    {
        let st = log.clone();
        r.stub(wt::MATH_ATAN, move |cpu, _mem| {
            let f12 = f32::from_bits(cpu.fpr[12]);
            st.borrow_mut().calls.push(CallRec { target: wt::MATH_ATAN, args: vec![], f12_bits: cpu.fpr[12], a1_64: None, snap: Vec::new() });
            cpu.fpr[0] = stub_atan(f12).to_bits();
        });
    }
    // MATH_SIN_COS
    {
        let st = log.clone();
        r.stub(wt::MATH_SIN_COS, move |cpu, mem| {
            let sptr = cpu.gpr(4) as u32;
            let cptr = cpu.gpr(5) as u32;
            let f12 = f32::from_bits(cpu.fpr[12]);
            st.borrow_mut().calls.push(CallRec { target: wt::MATH_SIN_COS, args: vec![], f12_bits: cpu.fpr[12], a1_64: None, snap: Vec::new() });
            let (sn, cs) = stub_sincos(f12);
            mem.write_u32(sptr, sn.to_bits());
            mem.write_u32(cptr, cs.to_bits());
            cpu.set_gpr(2, 0);
        });
    }
    // SPLINE_PATH_NODE_PLACE_ALL & 1FAAD0
    for addr in [wt::SPLINE_PATH_NODE_PLACE_ALL, wt::SPLINE_PATH_NODE_1FAAD0] {
        let st = log.clone();
        r.stub(addr, move |cpu, _mem| {
            let a0 = cpu.gpr(4) as u32;
            st.borrow_mut().calls.push(CallRec { target: addr, args: vec![a0], f12_bits: 0, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, 0);
        });
    }
    // LIGHTS_CLEAR
    {
        let st = log.clone();
        r.stub(wt::LIGHTS_CLEAR, move |cpu, _mem| {
            st.borrow_mut().calls.push(CallRec { target: wt::LIGHTS_CLEAR, args: vec![], f12_bits: 0, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, 0);
        });
    }
    // BX_OBJ_NODE_FROM_LINK
    {
        let st = log.clone();
        r.stub(wt::BX_OBJ_NODE_FROM_LINK, move |cpu, _mem| {
            let a0 = cpu.gpr(4) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::BX_OBJ_NODE_FROM_LINK, args: vec![a0], f12_bits: 0, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, s.bx_obj_ret as u64);
        });
    }
    // FAKE_GFX_284 (cull)
    {
        let st = log.clone();
        r.stub(FAKE_GFX_284, move |cpu, mem| {
            let a0 = cpu.gpr(4) as u32;
            let b0 = cpu.gpr(5) as u32;
            let b1 = cpu.gpr(6) as u32;
            let mut snap = vec![0u8; 32];
            mem.read(b0, &mut snap[0..16]).unwrap();
            mem.read(b1, &mut snap[16..32]).unwrap();
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: FAKE_GFX_284, args: vec![a0], f12_bits: 0, a1_64: None, snap });
            let idx = s.cull_idx;
            s.cull_idx += 1;
            let ret = s.cull_seq.get(idx).copied().unwrap_or(0);
            cpu.set_gpr(2, ret as u64);
        });
    }
    // FAKE_GFX_2AC (draw model)
    {
        let st = log.clone();
        r.stub(FAKE_GFX_2AC, move |cpu, mem| {
            let a0 = cpu.gpr(4) as u32;
            let a1 = cpu.gpr(5) as u32;
            let a2 = cpu.gpr(6) as u32;
            let mat = cpu.gpr(7) as u32;
            let t1 = cpu.gpr(9) as u32;
            let t2 = cpu.gpr(10) as u32;
            let mut snap = vec![0u8; 0x40];
            mem.read(mat, &mut snap).unwrap();
            st.borrow_mut().calls.push(CallRec {
                target: FAKE_GFX_2AC,
                args: vec![a0, a1, a2, t1, t2],
                f12_bits: 0,
                a1_64: None,
                snap,
            });
            cpu.set_gpr(2, 0);
        });
    }
    // FAKE_OBJ_EC (draw anim obj)
    {
        let st = log.clone();
        r.stub(FAKE_OBJ_EC, move |cpu, mem| {
            let a0 = cpu.gpr(4) as u32;
            let mat = cpu.gpr(5) as u32;
            let a2 = cpu.gpr(6) as u32;
            let mut snap = vec![0u8; 0x40];
            mem.read(mat, &mut snap).unwrap();
            st.borrow_mut().calls.push(CallRec { target: FAKE_OBJ_EC, args: vec![a0, a2], f12_bits: 0, a1_64: None, snap });
            cpu.set_gpr(2, 0);
        });
    }
    // FAKE_CROWD_CB (64-bit a1)
    {
        let st = log.clone();
        r.stub(FAKE_CROWD_CB, move |cpu, _mem| {
            let a0 = cpu.gpr(4) as u32;
            let a1 = cpu.gpr(5);
            st.borrow_mut().calls.push(CallRec { target: FAKE_CROWD_CB, args: vec![a0], f12_bits: 0, a1_64: Some(a1), snap: Vec::new() });
            cpu.set_gpr(2, 0);
        });
    }
    // WORLD_INSTANCE_GET_SOUND_KEYS
    {
        let st = log.clone();
        r.stub(wt::WORLD_INSTANCE_GET_SOUND_KEYS, move |cpu, mem| {
            let a0 = cpu.gpr(4) as u32;
            let a1 = cpu.gpr(5) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::WORLD_INSTANCE_GET_SOUND_KEYS, args: vec![a0, a1], f12_bits: 0, a1_64: None, snap: Vec::new() });
            for k in 0..4usize {
                let dst = cpu.gpr(6 + k) as u32;
                mem.write_u32(dst, s.sound_keys[k]);
            }
            cpu.set_gpr(2, 0);
        });
    }
    // WORLD_EMITTER_IS_HIT_ONLY_KEY
    {
        let st = log.clone();
        r.stub(wt::WORLD_EMITTER_IS_HIT_ONLY_KEY, move |cpu, _mem| {
            let a0 = cpu.gpr(4) as u32;
            let a1 = cpu.gpr(5) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::WORLD_EMITTER_IS_HIT_ONLY_KEY, args: vec![a0, a1], f12_bits: 0, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, (a1 == s.hit_only_key) as u64);
        });
    }
    // SPHERE_TREE_CACHE_GET
    {
        let st = log.clone();
        r.stub(wt::SPHERE_TREE_CACHE_GET, move |cpu, _mem| {
            let a0 = cpu.gpr(4) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::SPHERE_TREE_CACHE_GET, args: vec![a0], f12_bits: 0, a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, s.cache_get_ret as u64);
        });
    }
    // FAKE_GFX_26C (draw sphere leaf)
    {
        let st = log.clone();
        r.stub(FAKE_GFX_26C, move |cpu, mem| {
            let a0 = cpu.gpr(4) as u32;
            let pos = cpu.gpr(5) as u32;
            let col = cpu.gpr(6) as u32;
            let a3 = cpu.gpr(7) as u32;
            let mut snap = vec![0u8; 32];
            mem.read(pos, &mut snap[0..16]).unwrap();
            mem.read(col, &mut snap[16..32]).unwrap();
            st.borrow_mut().calls.push(CallRec { target: FAKE_GFX_26C, args: vec![a0, a3], f12_bits: cpu.fpr[12], a1_64: None, snap });
            cpu.set_gpr(2, 0);
        });
    }
    // SPH_TREE_SPHERE_RECURSE
    {
        let st = log.clone();
        r.stub(wt::SPH_TREE_SPHERE_RECURSE, move |cpu, mem| {
            let args: Vec<u32> = (0..7).map(|k| cpu.gpr(4 + k) as u32).collect();
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::SPH_TREE_SPHERE_RECURSE, args: args.clone(), f12_bits: cpu.fpr[12], a1_64: None, snap: Vec::new() });
            if s.sph_recurse_ret != 0 {
                for k in 0..4u32 {
                    mem.write_u32(args[5] + k * 4, s.sph_pen_out[k as usize].to_bits());
                    mem.write_u32(args[6] + k * 4, s.sph_hit_out[k as usize].to_bits());
                }
            }
            cpu.set_gpr(2, s.sph_recurse_ret as u64);
        });
    }
    // WORLD_LINE_CTOR
    {
        let st = log.clone();
        r.stub(wt::WORLD_LINE_CTOR, move |cpu, mem| {
            let from = cpu.gpr(5) as u32;
            let to = cpu.gpr(6) as u32;
            let kind = cpu.gpr(7) as u32;
            let mut snap = vec![0u8; 32];
            mem.read(from, &mut snap[0..16]).unwrap();
            mem.read(to, &mut snap[16..32]).unwrap();
            st.borrow_mut().calls.push(CallRec {
                target: wt::WORLD_LINE_CTOR,
                args: vec![kind],
                f12_bits: cpu.fpr[12],
                a1_64: None,
                snap,
            });
            cpu.set_gpr(2, cpu.gpr(4));
        });
    }
    // WORLD_RAY_CAST
    {
        let st = log.clone();
        r.stub(wt::WORLD_RAY_CAST, move |cpu, mem| {
            let world = cpu.gpr(4) as u32;
            let hit = cpu.gpr(6) as u32;
            let flags = cpu.gpr(7) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::WORLD_RAY_CAST, args: vec![world, flags], f12_bits: 0, a1_64: None, snap: Vec::new() });
            mem.write_u32(hit, s.ray_hit_kind);
            mem.write_u32(hit + 0x18, s.ray_hit_z.to_bits());
            mem.write_u32(hit + 0x34, s.ray_hit_surf);
            for k in 0..4u32 {
                mem.write_u32(hit + 0x20 + k * 4, s.ray_hit_normal[k as usize].to_bits());
            }
            cpu.fpr[0] = s.ray_ret.to_bits();
        });
    }
    // TRI_CONTAINS_POINT_VU0
    {
        let st = log.clone();
        r.stub(wt::TRI_CONTAINS_POINT_VU0, move |cpu, mem| {
            let args: Vec<u32> = (0..4).map(|k| cpu.gpr(4 + k) as u32).collect();
            let mut snap = vec![0u8; 16];
            mem.read(args[3], &mut snap).unwrap();
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::TRI_CONTAINS_POINT_VU0, args, f12_bits: cpu.fpr[12], a1_64: None, snap });
            cpu.set_gpr(2, s.tri_ret as u64);
        });
    }
    // COLLIDE_SPHERE_BOX
    {
        let st = log.clone();
        r.stub(wt::COLLIDE_SPHERE_BOX, move |cpu, _mem| {
            let args: Vec<u32> = (0..3).map(|k| cpu.gpr(4 + k) as u32).collect();
            let mut s = st.borrow_mut();
            s.calls.push(CallRec { target: wt::COLLIDE_SPHERE_BOX, args, f12_bits: cpu.fpr[12], a1_64: None, snap: Vec::new() });
            cpu.set_gpr(2, s.box_ret as u64);
        });
    }
}

#[test]
fn stage_area_marker_matches() {
    let Some(mut r) = runner() else { return };
    let r_st = Rc::new(RefCell::new(EnvState::new()));
    install_stubs(&mut r, r_st.clone());

    let mut rng = Rng(0x1779_b801);
    let inst = BASE + 0x100;
    let out_pos = BASE + 0x200;
    let out_yaw = BASE + 0x220;

    // Special cases for yaw quadrants + kind 0/1/2 + missing hash/instance + random matrices.
    let mut cases: Vec<(i32, u32, u32, [f32; 4])> = vec![
        (0, 0x1234, inst, [1.0, 0.0, 0.0, 0.0]),
        (1, 0, inst, [1.0, 0.0, 0.0, 0.0]),
        (2, 0x5678, 0, [1.0, 0.0, 0.0, 0.0]),
        (1, 0x1111, inst, [0.0, 0.0, 1.0, 0.0]),
        (1, 0x1111, inst, [0.0, 1.0, 0.0, 0.0]),
        (2, 0x2222, inst, [0.0, -1.0, 0.0, 0.0]),
        (1, 0x1111, inst, [-1.0, 0.5, 0.0, 0.0]),
        (2, 0x2222, inst, [-1.0, -0.5, 0.0, 0.0]),
        (2, 0x2222, inst, [-1.0, 0.0, 0.0, 0.0]),
    ];
    for i in 0..40 {
        let kind = if i % 2 == 0 { 1 } else { 2 };
        cases.push((kind, 0x9999 + i, inst, [rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), 0.0]));
    }

    for (ci, (kind, hash, found, r0)) in cases.into_iter().enumerate() {
        let mut m = fresh_mem(&mut r);
        write_both_vec4(&mut r, &mut m, inst, r0);
        write_both_vec4(&mut r, &mut m, inst + 0x10, [rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), 0.0]);
        write_both_vec4(&mut r, &mut m, inst + 0x20, [rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), 0.0]);
        write_both_vec4(&mut r, &mut m, inst + 0x30, [rng.f(-500.0, 500.0), rng.f(-500.0, 500.0), rng.f(-500.0, 500.0), 1.0]);
        write_both_vec4(&mut r, &mut m, out_pos, [rng.f(-100.0, 100.0), rng.f(-100.0, 100.0), rng.f(-100.0, 100.0), 1.0]);
        write_both_f32(&mut r, &mut m, out_yaw, -999.0);

        let setup = |st: &Rc<RefCell<EnvState>>| {
            let mut s = st.borrow_mut();
            *s = EnvState::new();
            s.name_hash_ret = hash;
            s.find_inst_ret = found;
        };
        setup(&r_st);
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        setup(&p_st);
        let mut env = TestEnv(p_st.clone());

        let r_ret = call_u32(&mut r, 0x0017_79b8, &[kind as u32, out_pos, out_yaw]);
        let p_ret = wt::world_get_stage_area_marker(&mut m, &mut env, kind, out_pos, out_yaw);

        assert_eq!(p_ret, r_ret, "case {ci} ret");
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "case {ci} calls");
        assert_eq!(mem_slice(&m, out_pos, 16), copy_out(&mut r, out_pos, 16), "case {ci} out_pos");
        assert_eq!(mem_slice(&m, out_yaw, 4), copy_out(&mut r, out_yaw, 4), "case {ci} out_yaw");
    }
}

#[test]
fn spline_path_node_matches() {
    let Some(mut r) = runner() else { return };
    let r_st = Rc::new(RefCell::new(EnvState::new()));
    install_stubs(&mut r, r_st.clone());

    let node = BASE + 0x100;
    let vt_node = BASE + 0x300;
    let inst = BASE + 0x400;
    let ctl = BASE + 0x600;
    let vt_ctl = BASE + 0x700;
    let pts = BASE + 0x800;
    let app = BASE + 0x1000;
    let gfx = BASE + 0x1800;
    let vt_gfx = BASE + 0x1900;
    let anim_obj = BASE + 0x2000;
    let vt_obj = BASE + 0x2100;

    // 1. ~cSplinePathNode (0x1fa640)
    for pts_val in [0u32, pts] {
        for how in 0u32..4 {
            let mut m = fresh_mem(&mut r);
            write_both_u32(&mut r, &mut m, node + 0x7c, pts_val);
            *r_st.borrow_mut() = EnvState::new();
            let p_st = Rc::new(RefCell::new(EnvState::new()));
            let mut env = TestEnv(p_st.clone());

            call_u32(&mut r, 0x001f_a640, &[node, how]);
            wt::spline_path_node_dtor(&mut m, &mut env, node, how);

            assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "dtor pts={pts_val:#x} how={how}");
            assert_eq!(mem_slice(&m, node, 0x80), copy_out(&mut r, node, 0x80));
        }
    }

    // 2. cSplinePathNode::Update (0x1fa6a0)
    let update_cases: &[(u32, u32, u32, u32, u32, f32, f32, f32)] = &[
        // paused, ctl, flags, count, mode, t, dt, len
        (1, ctl, 0x80, 1, 0, 0.5, 0.1, 2.0),
        (0, 0, 0x80, 1, 0, 0.5, 0.1, 2.0),
        (0, ctl, 0x880, 1, 0, 0.5, 0.1, 2.0),
        (0, ctl, 0x80, 1, 2, 0.05, -0.2, 2.0),
        (0, ctl, 0x80, 2, 1, 0.05, -0.2, 2.0),
        (0, ctl, 0x80, 1, 0, 1.9, 0.2, 2.0),
        (0, ctl, 0x80, 1, 1, 1.9, 0.2, 2.0),
        (0, ctl, 0x80, 1, 2, 1.9, 0.2, 2.0),
        (0, ctl, 0x80, 1, 3, 1.9, 0.2, 2.0),
        (0, ctl, 0x80, 1, 3, 0.5, 0.1, 2.0),
        (0, ctl, 0x00, 1, 3, 0.5, 0.1, 2.0),
        (0, ctl, 0x80, 3, 1, 0.5, 0.1, 2.0),
    ];
    for (ci, &(paused, ctl_p, flags, count, mode, t, dt, len)) in update_cases.iter().enumerate() {
        let mut m = fresh_mem(&mut r);
        write_both_u32(&mut r, &mut m, node + 8, vt_node);
        write_both_u16(&mut r, &mut m, vt_node + 8, 4);
        write_both_u32(&mut r, &mut m, vt_node + 0xc, FAKE_NODE_DEL);
        write_both_u32(&mut r, &mut m, node + 0x30, count);
        write_both_u32(&mut r, &mut m, node + 0x34, mode);
        write_both_u32(&mut r, &mut m, node + 0x40, paused);
        write_both_f32(&mut r, &mut m, node + 0x4c, dt);
        write_both_f32(&mut r, &mut m, node + 0x50, t);
        write_both_f32(&mut r, &mut m, node + 0x74, len);
        write_both_u32(&mut r, &mut m, node + 0x78, inst);
        write_both_u32(&mut r, &mut m, node + 0x7c, pts);
        write_both_u32(&mut r, &mut m, inst + 0xe4, ctl_p);
        write_both_u32(&mut r, &mut m, inst + 0xe8, flags);
        write_both_u32(&mut r, &mut m, ctl + 8, vt_ctl);
        write_both_u16(&mut r, &mut m, vt_ctl + 0xc8, 8);
        write_both_u32(&mut r, &mut m, vt_ctl + 0xcc, FAKE_CTL_C8);

        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());

        call_u32(&mut r, 0x001f_a6a0, &[node]);
        wt::spline_path_node_update(&mut m, &mut env, node);

        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "update case {ci} calls");
        assert_eq!(mem_slice(&m, node, 0x80), copy_out(&mut r, node, 0x80), "update case {ci} mem");
    }

    // 3. cSplinePathNode::Draw (0x1fabc8)
    let mut rng = Rng(0x1fab_c801);
    for (ci, &(draw_path, count, ctl_p, ctl_type, cull0, cull1)) in [
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32),
        (1, 0, 0, 0, 0, 0),
        (1, 2, 0, 0, 0, 1),
        (0, 3, ctl, 0x000, 1, 0),
        (1, 2, ctl, 0x100, 0, 0),
    ]
    .iter()
    .enumerate()
    {
        let mut m = fresh_mem(&mut r);
        // Set identity matrix at G_IDENTITY_MAT4 (0x346440)
        for row in 0..4u32 {
            for col in 0..4u32 {
                write_both_f32(&mut r, &mut m, wt::G_IDENTITY_MAT4 + row * 16 + col * 4, if row == col { 1.0 } else { 0.0 });
            }
        }
        write_both_u32(&mut r, &mut m, wt::G_APP, app);
        write_both_u32(&mut r, &mut m, app + 0x724, gfx);
        write_both_u32(&mut r, &mut m, gfx + 0x10, vt_gfx);
        write_both_u16(&mut r, &mut m, vt_gfx + 0x280, 0);
        write_both_u32(&mut r, &mut m, vt_gfx + 0x284, FAKE_GFX_284);
        write_both_u16(&mut r, &mut m, vt_gfx + 0x2a8, 4);
        write_both_u32(&mut r, &mut m, vt_gfx + 0x2ac, FAKE_GFX_2AC);

        write_both_u32(&mut r, &mut m, node + 0x30, count);
        write_both_u32(&mut r, &mut m, node + 0x3c, draw_path);
        write_both_u32(&mut r, &mut m, node + 0x78, inst);
        write_both_u32(&mut r, &mut m, node + 0x7c, pts);
        write_both_vec4(&mut r, &mut m, node + 0x80, [-10.0, -20.0, -30.0, 0.0]);
        write_both_vec4(&mut r, &mut m, node + 0x90, [10.0, 20.0, 30.0, 0.0]);

        write_both_u32(&mut r, &mut m, inst + 0xc0, 0xdead_beef);
        write_both_u32(&mut r, &mut m, inst + 0xe4, ctl_p);
        write_both_u32(&mut r, &mut m, inst + 0xf0, 0x0000_0042);
        write_both_u32(&mut r, &mut m, ctl + 0x14, ctl_type);
        write_both_u32(&mut r, &mut m, anim_obj + 0x38, vt_obj);
        write_both_u16(&mut r, &mut m, vt_obj + 0xe8, 0);
        write_both_u32(&mut r, &mut m, vt_obj + 0xec, FAKE_OBJ_EC);

        for k in 0..count {
            let e = pts + k * 0x20;
            write_both_vec4(&mut r, &mut m, e, [rng.f(-100.0, 100.0), rng.f(-100.0, 100.0), rng.f(-100.0, 100.0), 1.0]);
            write_both_vec4(&mut r, &mut m, e + 0x10, [rng.f(-3.1, 3.1), rng.f(-3.1, 3.1), rng.f(-3.1, 3.1), 0.0]);
        }

        let setup = |st: &Rc<RefCell<EnvState>>| {
            let mut s = st.borrow_mut();
            *s = EnvState::new();
            s.cull_seq = vec![cull0, cull1, 0];
            s.bx_obj_ret = anim_obj;
        };
        setup(&r_st);
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        setup(&p_st);
        let mut env = TestEnv(p_st.clone());

        call_u32(&mut r, 0x001f_abc8, &[node]);
        wt::spline_path_node_draw(&mut m, &mut env, node);

        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "draw case {ci}");
    }
}

#[test]
fn crowd_and_sound_keys_match() {
    let Some(mut r) = runner() else { return };
    let r_st = Rc::new(RefCell::new(EnvState::new()));
    install_stubs(&mut r, r_st.clone());

    let box_p = BASE + 0x100;
    let sys = BASE + 0x1000;
    let vt_sys = BASE + 0x500;
    let arr_hdr = BASE + 0x6000;
    let arr = arr_hdr + 0x10;

    // 1. CrowdBox_Destruct (0x21ef58)
    for how in 0u32..4 {
        let mut m = fresh_mem(&mut r);
        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());
        call_u32(&mut r, 0x0021_ef58, &[box_p, how]);
        wt::crowd_box_destruct(&mut m, &mut env, box_p, how);
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "CrowdBox_Destruct how={how}");
    }

    // 2. CrowdSys_Free (0x220bb8)
    for &(arr_ptr, count) in &[(0u32, 0u32), (arr, 0), (arr, 3)] {
        let mut m = fresh_mem(&mut r);
        write_both_u32(&mut r, &mut m, sys + 0x3728, count);
        write_both_u32(&mut r, &mut m, sys + 0x372c, arr_ptr);
        write_both_u32(&mut r, &mut m, arr_hdr, count);
        for i in 0..3u32 {
            let b = sys + 0x3730 + i * 0x54;
            write_both_u32(&mut r, &mut m, b + 0x48, 77);
            write_both_u32(&mut r, &mut m, b + 0x4c, 88);
            write_both_u32(&mut r, &mut m, b + 0x50, 99);
        }
        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());

        call_u32(&mut r, 0x0022_0bb8, &[sys]);
        wt::crowd_sys_free(&mut m, &mut env, sys);

        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "CrowdSys_Free count={count}");
        assert_eq!(mem_slice(&m, sys + 0x3720, 0x180), copy_out(&mut r, sys + 0x3720, 0x180));
    }

    // 3. CrowdSys_StopAll (0x221330)
    for count in [0u32, 1, 4] {
        let mut m = fresh_mem(&mut r);
        write_both_u32(&mut r, &mut m, sys + 4, vt_sys);
        write_both_u16(&mut r, &mut m, vt_sys + 0x38, 0);
        write_both_u32(&mut r, &mut m, vt_sys + 0x3c, FAKE_CROWD_CB);
        write_both_u32(&mut r, &mut m, sys + 0x3728, count);
        write_both_u32(&mut r, &mut m, sys + 0x372c, arr);

        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());

        call_u32(&mut r, 0x0022_1330, &[sys]);
        wt::crowd_sys_stop_all(&mut m, &mut env, sys);

        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "CrowdSys_StopAll count={count}");
    }

    // 4. WorldInstance_HasHitOnlyKey (0x22e1b8)
    for hit_key in [10u32, 20, 30, 40, 99] {
        let mut m = fresh_mem(&mut r);
        write_both_u32(&mut r, &mut m, wt::G_WORLD_SOUND, BASE + 0x40);
        let setup = |st: &Rc<RefCell<EnvState>>| {
            let mut s = st.borrow_mut();
            *s = EnvState::new();
            s.sound_keys = [10, 20, 30, 40];
            s.hit_only_key = hit_key;
        };
        setup(&r_st);
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        setup(&p_st);
        let mut env = TestEnv(p_st.clone());

        let r_ret = call_u32(&mut r, 0x0022_e1b8, &[BASE + 0x80, BASE + 0xc0]);
        let p_ret = wt::world_instance_has_hit_only_key(&mut m, &mut env, BASE + 0x80, BASE + 0xc0);

        assert_eq!(p_ret, r_ret, "HasHitOnlyKey hit={hit_key}");
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "HasHitOnlyKey calls hit={hit_key}");
    }
}

#[test]
fn sphtree_ten_functions_match() {
    let Some(mut r) = runner() else { return };
    let r_st = Rc::new(RefCell::new(EnvState::new()));
    install_stubs(&mut r, r_st.clone());

    let tree = BASE + 0x100;
    let hdr = BASE + 0x400;
    let levels = BASE + 0x500;
    let masks = BASE + 0x600;
    let pos = BASE + 0x700;
    let plane = BASE + 0x720;
    let out_pen = BASE + 0x740;
    let out_hit = BASE + 0x760;
    let app = BASE + 0x1000;
    let gfx = BASE + 0x1800;
    let vt_gfx = BASE + 0x1900;

    // 1. SphTree_Assert (0x2391c8)
    for c in [0u32, 1] {
        call_u32(&mut r, 0x0023_91c8, &[c]);
        wt::sph_tree_assert(c);
    }

    // 2. bxSphereTree::~bxSphereTree (0x2391d0) & SphereTree::~SphereTree (0x239c38)
    for how in 0u32..4 {
        let mut m = fresh_mem(&mut r);
        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());
        call_u32(&mut r, 0x0023_91d0, &[tree, how]);
        wt::bx_sphere_tree_dtor(&mut m, &mut env, tree, how);
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls);
        assert_eq!(m.r32(tree), r.mem.read_u32(tree));

        let mut m = fresh_mem(&mut r);
        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());
        call_u32(&mut r, 0x0023_9c38, &[tree, how]);
        wt::sphere_tree_dtor(&mut m, &mut env, tree, how);
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls);
        assert_eq!(m.r32(tree), r.mem.read_u32(tree));
    }

    // 3. SphTree_SetPosition (0x239c98) & SphTree_Translate (0x239d18)
    let mut rng = Rng(0x239c_9801);
    for _ in 0..30 {
        let mut m = fresh_mem(&mut r);
        write_both_u32(&mut r, &mut m, tree + 0xa0, hdr);
        write_both_f32(&mut r, &mut m, hdr + 0x14, rng.f(-50.0, 50.0));
        write_both_f32(&mut r, &mut m, hdr + 0x18, rng.f(-50.0, 50.0));
        write_both_f32(&mut r, &mut m, hdr + 0x1c, rng.f(-50.0, 50.0));
        write_both_vec4(&mut r, &mut m, pos, [rng.f(-100.0, 100.0), rng.f(-100.0, 100.0), rng.f(-100.0, 100.0), 0.0]);
        call_u32(&mut r, 0x0023_9c98, &[tree, pos]);
        wt::sph_tree_set_position(&mut m, tree, pos);
        assert_eq!(mem_slice(&m, tree + 0x90, 16), copy_out(&mut r, tree + 0x90, 16));

        write_both_vec4(&mut r, &mut m, pos, [rng.f(-20.0, 20.0), rng.f(-20.0, 20.0), rng.f(-20.0, 20.0), 0.0]);
        call_u32(&mut r, 0x0023_9d18, &[tree, pos]);
        wt::sph_tree_translate(&mut m, tree, pos);
        assert_eq!(mem_slice(&m, tree + 0x90, 16), copy_out(&mut r, tree + 0x90, 16));
    }

    // Helper to build a valid 2-level sphere tree in both Runner and Mem
    let setup_tree = |r: &mut Runner, m: &mut Mem, packed: u32, depth: u32, rng: &mut Rng| {
        write_both_u32(r, m, wt::G_APP, app);
        write_both_u32(r, m, app + 0x724, gfx);
        write_both_u32(r, m, gfx + 0x10, vt_gfx);
        write_both_u16(r, m, vt_gfx + 0x268, 0);
        write_both_u32(r, m, vt_gfx + 0x26c, FAKE_GFX_26C);

        for (k, d) in tricky_game::sphtree::DIRS.iter().enumerate() {
            write_both_vec4(r, m, tree + 0x10 + (k as u32) * 16, *d);
        }
        write_both_vec4(r, m, tree + 0x90, [rng.f(-10.0, 10.0), rng.f(-10.0, 10.0), rng.f(-10.0, 10.0), 1.0]);
        write_both_u32(r, m, tree + 0xa0, hdr);
        write_both_u32(r, m, hdr + 8, packed);
        write_both_u32(r, m, hdr + 0xc, depth);
        write_both_u32(r, m, hdr + 0x20, levels);
        write_both_u32(r, m, hdr + 0x28, if packed == 0 { masks } else { 0 });

        // Level 0: rad=20, half=10, stride=9
        write_both_f32(r, m, levels, 20.0);
        write_both_f32(r, m, levels + 4, 10.0);
        write_both_u32(r, m, levels + 8, 9);
        // Level 1: rad=10, half=5, stride=1
        write_both_f32(r, m, levels + 12, 10.0);
        write_both_f32(r, m, levels + 16, 5.0);
        write_both_u32(r, m, levels + 20, 1);
        // Level 2: rad=5, half=2.5, stride=0
        write_both_f32(r, m, levels + 24, 5.0);
        write_both_f32(r, m, levels + 28, 2.5);
        write_both_u32(r, m, levels + 32, 0);

        for i in 0..100u32 {
            write_both_u8(r, m, masks + i, (rng.next() & 0xff) as u8);
        }
        // Octant permutation 0..8
        for i in 0..8u32 {
            write_both_u32(r, m, tree + 0xa8 + i * 4, (7 - i) as u32);
        }
    };

    // 4. SphTree_ForEachLeaf (0x2396f8) & SphTree_ForEachLeafRecurse (0x239750)
    for packed in [0u32, 1] {
        for depth in 0u32..=2 {
            let mut m = fresh_mem(&mut r);
            setup_tree(&mut r, &mut m, packed, depth, &mut rng);
            let setup = |st: &Rc<RefCell<EnvState>>| {
                let mut s = st.borrow_mut();
                *s = EnvState::new();
                s.cache_get_ret = masks;
            };
            setup(&r_st);
            let p_st = Rc::new(RefCell::new(EnvState::new()));
            setup(&p_st);
            let mut env = TestEnv(p_st.clone());

            call_u32(&mut r, 0x0023_96f8, &[tree]);
            wt::sph_tree_for_each_leaf(&mut m, &mut env, tree);

            assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "ForEachLeaf packed={packed} depth={depth}");
            assert_eq!(m.r32(hdr + 0x28), r.mem.read_u32(hdr + 0x28));
        }
    }

    // 5. Collide_SphereSphTree (0x23a160)
    for packed in [0u32, 1] {
        for ret in [0u32, 1] {
            let mut m = fresh_mem(&mut r);
            setup_tree(&mut r, &mut m, packed, 2, &mut rng);
            let setup = |st: &Rc<RefCell<EnvState>>| {
                let mut s = st.borrow_mut();
                *s = EnvState::new();
                s.cache_get_ret = masks;
                s.sph_recurse_ret = ret;
                s.sph_pen_out = [1.5, -2.5, 3.0, 0.0];
                s.sph_hit_out = [10.0, 20.0, 30.0, 1.0];
            };
            setup(&r_st);
            let p_st = Rc::new(RefCell::new(EnvState::new()));
            setup(&p_st);
            let mut env = TestEnv(p_st.clone());

            let r_ret = call_f_ret_u32(&mut r, 0x0023_a160, &[tree, pos, out_pen, out_hit], 12.5);
            let p_ret = wt::collide_sphere_sph_tree(&mut m, &mut env, tree, pos, out_pen, out_hit, 12.5);

            assert_eq!(p_ret, r_ret);
            assert_eq!(p_st.borrow().calls, r_st.borrow().calls);
            assert_eq!(mem_slice(&m, out_pen, 16), copy_out(&mut r, out_pen, 16));
            assert_eq!(mem_slice(&m, out_hit, 16), copy_out(&mut r, out_hit, 16));
        }
    }

    // 6. SphTree_IntersectPlane (0x23a2c0) & SphTree_PlaneRecurse (0x23ab78) — 120 random trees/planes!
    for ci in 0..120 {
        let mut m = fresh_mem(&mut r);
        let packed = (ci & 1) as u32;
        let depth = (ci % 3) as u32;
        setup_tree(&mut r, &mut m, packed, depth, &mut rng);
        let nx = rng.f(-1.0, 1.0);
        let ny = rng.f(-1.0, 1.0);
        let nz = rng.f(-1.0, 1.0);
        let inv = 1.0 / (nx * nx + ny * ny + nz * nz + 1e-6).sqrt();
        write_both_vec4(&mut r, &mut m, plane, [nx * inv, ny * inv, nz * inv, 0.0]);
        write_both_f32(&mut r, &mut m, plane + 0x10, rng.f(-25.0, 25.0));

        let setup = |st: &Rc<RefCell<EnvState>>| {
            let mut s = st.borrow_mut();
            *s = EnvState::new();
            s.cache_get_ret = masks;
        };
        setup(&r_st);
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        setup(&p_st);
        let mut env = TestEnv(p_st.clone());

        let r_ret = call_u32(&mut r, 0x0023_a2c0, &[tree, plane, out_pen, out_hit]);
        let p_ret = wt::sph_tree_intersect_plane(&mut m, &mut env, tree, plane, out_pen, out_hit);

        assert_eq!(p_ret, r_ret, "IntersectPlane case {ci} ret");
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "IntersectPlane case {ci} calls");
        assert_eq!(m.r32(tree + 0xa4), r.mem.read_u32(tree + 0xa4), "IntersectPlane case {ci} best");
        assert_eq!(mem_slice(&m, out_pen, 16), copy_out(&mut r, out_pen, 16), "IntersectPlane case {ci} out_pen");
        assert_eq!(mem_slice(&m, out_hit, 16), copy_out(&mut r, out_hit, 16), "IntersectPlane case {ci} out_hit");
    }

    // Direct SphTree_PlaneRecurse (0x23ab78) check
    {
        let mut m = fresh_mem(&mut r);
        setup_tree(&mut r, &mut m, 0, 2, &mut rng);
        write_both_vec4(&mut r, &mut m, plane, [0.0, 0.0, 1.0, 0.0]);
        write_both_f32(&mut r, &mut m, plane + 0x10, 0.0);
        write_both_f32(&mut r, &mut m, tree + 0xa4, 0.0);
        call_u32(&mut r, 0x0023_ab78, &[tree, plane, tree + 0x90, 0, 0, out_pen, out_hit]);
        wt::sph_tree_plane_recurse(&mut m, tree, plane, tree + 0x90, 0, 0, out_pen, out_hit);
        assert_eq!(m.r32(tree + 0xa4), r.mem.read_u32(tree + 0xa4));
        assert_eq!(mem_slice(&m, out_pen, 16), copy_out(&mut r, out_pen, 16));
        assert_eq!(mem_slice(&m, out_hit, 16), copy_out(&mut r, out_hit, 16));
    }
}

#[test]
fn probe_ground_and_world_shapes_match() {
    let Some(mut r) = runner() else { return };
    let r_st = Rc::new(RefCell::new(EnvState::new()));
    install_stubs(&mut r, r_st.clone());

    let obj = BASE + 0x100;
    let world = BASE + 0x200;
    let grid = BASE + 0x300;
    let cells = BASE + 0x400;
    let cell0 = BASE + 0x500;
    let cell1 = BASE + 0x540;
    let pos = BASE + 0x600;
    let out_z = BASE + 0x620;
    let out_surf = BASE + 0x624;
    let out_norm = BASE + 0x630;

    // 1. Destructors: cWorldBoarder::vf10_257ca0, CellList_Destruct, cWorldSphTree::vf10_261a48
    for how in 0u32..4 {
        let mut m = fresh_mem(&mut r);
        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());
        call_u32(&mut r, 0x0025_7ca0, &[obj, how]);
        wt::world_boarder_dtor(&mut m, &mut env, obj, how);
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls);
        assert_eq!(m.r32(obj + 0x40), r.mem.read_u32(obj + 0x40));

        let mut m = fresh_mem(&mut r);
        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());
        call_u32(&mut r, 0x0026_0fc0, &[obj, how]);
        wt::cell_list_destruct(&mut m, &mut env, obj, how);
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls);

        let mut m = fresh_mem(&mut r);
        *r_st.borrow_mut() = EnvState::new();
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        let mut env = TestEnv(p_st.clone());
        call_u32(&mut r, 0x0026_1a48, &[obj, how]);
        wt::world_sph_tree_dtor(&mut m, &mut env, obj, how);
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls);
        assert_eq!(m.r32(obj + 0x40), r.mem.read_u32(obj + 0x40));
    }

    // 2. Four tiny stubs
    {
        let mut m = fresh_mem(&mut r);
        call_u32(&mut r, 0x0025_d7a0, &[obj]);
        wt::world_intersect_type_vf9(obj);
        assert_eq!(wt::world_intersect_type_vf3(obj), call_u32(&mut r, 0x0025_d7a8, &[obj]));
        assert_eq!(wt::world_line_vf6(obj), call_u32(&mut r, 0x0025_dc40, &[obj]));
        write_both_f32(&mut r, &mut m, obj + 0x90, 0.725);
        assert_eq!(wt::world_line_vf2(&m, obj).to_bits(), call_ret_f32(&mut r, 0x0025_ebd8, &[obj]).to_bits());
    }

    // 3. World_ProbeGround (0x259030)
    let probe_cases: &[([f32; 4], f32, u32, u32)] = &[
        // point, ray_ret, hit_kind, out_normal_ptr
        ([5000.0, 5000.0, 10.0, 1.0], 0.5, 0, out_norm), // outside all cells -> z_max <= z_min -> 0
        ([15.0, 15.0, 5.0, 1.0], -1.0, 0, out_norm),      // ray miss -> 0
        ([15.0, 15.0, 5.0, 1.0], 0.35, 0, out_norm),      // hit_kind 0 with normal
        ([15.0, 15.0, 5.0, 1.0], 0.0, 2, out_norm),       // hit_kind != 0 -> surf = 1
        ([15.0, 15.0, 5.0, 1.0], 0.75, 0, 0),             // out_normal == null
    ];
    for (ci, &(pt, ray_ret, hit_kind, norm_ptr)) in probe_cases.iter().enumerate() {
        let mut m = fresh_mem(&mut r);
        write_both_u32(&mut r, &mut m, wt::G_WORLD, grid);
        write_both_u32(&mut r, &mut m, world, grid);
        write_both_f32(&mut r, &mut m, grid + 4, 0.0);   // x0
        write_both_f32(&mut r, &mut m, grid + 8, 0.0);   // y0
        write_both_f32(&mut r, &mut m, grid + 0x28, 10.0); // step
        write_both_u32(&mut r, &mut m, grid + 0x2c, 4);  // w
        write_both_u32(&mut r, &mut m, grid + 0x30, 4);  // h
        write_both_u32(&mut r, &mut m, grid + 0x4c, cells);

        // Cell 0 covers [0..30, 0..30, -50..20]
        write_both_f32(&mut r, &mut m, cell0, 0.0);
        write_both_f32(&mut r, &mut m, cell0 + 4, 0.0);
        write_both_f32(&mut r, &mut m, cell0 + 8, -50.0);
        write_both_f32(&mut r, &mut m, cell0 + 0xc, 30.0);
        write_both_f32(&mut r, &mut m, cell0 + 0x10, 30.0);
        write_both_f32(&mut r, &mut m, cell0 + 0x14, 20.0);
        // Cell 1 covers [10..40, 10..40, -10..80]
        write_both_f32(&mut r, &mut m, cell1, 10.0);
        write_both_f32(&mut r, &mut m, cell1 + 4, 10.0);
        write_both_f32(&mut r, &mut m, cell1 + 8, -10.0);
        write_both_f32(&mut r, &mut m, cell1 + 0xc, 40.0);
        write_both_f32(&mut r, &mut m, cell1 + 0x10, 40.0);
        write_both_f32(&mut r, &mut m, cell1 + 0x14, 80.0);

        write_both_u32(&mut r, &mut m, cells + (1 * 4 + 1) * 4, cell0);
        write_both_u32(&mut r, &mut m, cells + (2 * 4 + 2) * 4, cell1);
        write_both_vec4(&mut r, &mut m, pos, pt);

        let setup = |st: &Rc<RefCell<EnvState>>| {
            let mut s = st.borrow_mut();
            *s = EnvState::new();
            s.ray_ret = ray_ret;
            s.ray_hit_kind = hit_kind;
            s.ray_hit_z = -12.25;
            s.ray_hit_surf = 7;
            s.ray_hit_normal = [0.0, 0.6, 0.8, 0.0];
        };
        setup(&r_st);
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        setup(&p_st);
        let mut env = TestEnv(p_st.clone());

        let r_ret = call_u32(&mut r, 0x0025_9030, &[world, pos, out_z, out_surf, norm_ptr]);
        let p_ret = wt::world_probe_ground(&mut m, &mut env, world, pos, out_z, out_surf, norm_ptr);

        assert_eq!(p_ret, r_ret, "ProbeGround case {ci} ret");
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "ProbeGround case {ci} calls");
        assert_eq!(mem_slice(&m, out_z, 0x20), copy_out(&mut r, out_z, 0x20), "ProbeGround case {ci} out");
    }

    // 4. All 7 cWorldSphere virtuals (0x2611b0..0x261590)
    let mut rng = Rng(0x2611_b001);
    let sph = BASE + 0x1000;
    let mat = BASE + 0x1100;
    let va = BASE + 0x1200;
    let vb = BASE + 0x1210;
    let vc = BASE + 0x1220;
    let vn = BASE + 0x1230;
    let out_pt = BASE + 0x1240;
    let out_n = BASE + 0x1250;
    let out_pen = BASE + 0x1260;

    // vf0 (push) & vf1 (pop) & vf2 (target) & vf7 (box)
    for kind in [0u32, 1, 2] {
        let mut m = fresh_mem(&mut r);
        write_both_u32(&mut r, &mut m, sph, kind);
        write_both_f32(&mut r, &mut m, sph + 0x60, 12.75);
        write_both_vec4(&mut r, &mut m, sph + 0x70, [rng.f(-50.0, 50.0), rng.f(-50.0, 50.0), rng.f(-50.0, 50.0), 1.0]);
        for row in 0..4u32 {
            write_both_vec4(&mut r, &mut m, mat + row * 16, [rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), rng.f(-2.0, 2.0), if row == 3 { 1.0 } else { 0.0 }]);
        }

        call_u32(&mut r, 0x0026_11b0, &[sph, mat]);
        wt::world_sphere_push(&mut m, sph, mat);
        assert_eq!(mem_slice(&m, sph, 0x80), copy_out(&mut r, sph, 0x80));

        call_u32(&mut r, 0x0026_11f8, &[sph]);
        wt::world_sphere_pop(&mut m, sph);
        assert_eq!(mem_slice(&m, sph, 0x80), copy_out(&mut r, sph, 0x80));

        assert_eq!(wt::world_sphere_target(&m, sph).to_bits(), call_ret_f32(&mut r, 0x0026_1590, &[sph]).to_bits());

        let setup = |st: &Rc<RefCell<EnvState>>| {
            let mut s = st.borrow_mut();
            *s = EnvState::new();
            s.box_ret = kind & 1;
        };
        setup(&r_st);
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        setup(&p_st);
        let mut env = TestEnv(p_st.clone());
        let r_ret = call_u32(&mut r, 0x0026_1560, &[sph, va, vb]);
        let p_ret = wt::world_sphere_box(&mut m, &mut env, sph, va, vb);
        assert_eq!(p_ret, r_ret);
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls);
    }

    // vf4 (0x261208) & vf5 (0x261328)
    for (ci, &(c_z, tri_ret)) in [
        (0.5f32, 1u32),
        (0.5, 0),
        (-0.5, 1),
        (-0.5, 0),
        (2.0, 1),   // |d| == r (vf4 misses, vf5 hits!)
        (3.0, 1),   // |d| > r
        (-3.0, 1),
    ]
    .iter()
    .enumerate()
    {
        let mut m = fresh_mem(&mut r);
        write_both_vec4(&mut r, &mut m, sph + 0x50, [1.0, 2.0, c_z, 1.0]);
        write_both_f32(&mut r, &mut m, sph + 0x60, 2.0);
        write_both_vec4(&mut r, &mut m, va, [0.0, 0.0, 0.0, 1.0]);
        write_both_vec4(&mut r, &mut m, vb, [5.0, 0.0, 0.0, 1.0]);
        write_both_vec4(&mut r, &mut m, vc, [0.0, 5.0, 0.0, 1.0]);
        write_both_vec4(&mut r, &mut m, vn, [0.0, 0.0, 1.0, 0.0]);

        for &(addr, is_both) in &[(0x0026_1208u32, false), (0x0026_1328u32, true)] {
            let setup = |st: &Rc<RefCell<EnvState>>| {
                let mut s = st.borrow_mut();
                *s = EnvState::new();
                s.tri_ret = tri_ret;
            };
            setup(&r_st);
            let p_st = Rc::new(RefCell::new(EnvState::new()));
            setup(&p_st);
            let mut env = TestEnv(p_st.clone());

            let r_ret = call_u32(&mut r, addr, &[sph, va, vb, vc, vn, out_pt, out_n, out_pen]);
            let p_ret = if is_both {
                wt::world_sphere_tri_both(&mut m, &mut env, sph, va, vb, vc, vn, out_pt, out_n, out_pen)
            } else {
                wt::world_sphere_tri_front(&mut m, &mut env, sph, va, vb, vc, vn, out_pt, out_n, out_pen)
            };

            assert_eq!(p_ret, r_ret, "vf4/vf5 case {ci} both={is_both} ret");
            assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "vf4/vf5 case {ci} both={is_both} calls");
            assert_eq!(mem_slice(&m, out_pt, 0x24), copy_out(&mut r, out_pt, 0x24), "vf4/vf5 case {ci} both={is_both} out");
        }
    }

    // vf6 (0x2614a8): stub Collide_SphereSphTree
    {
        let st = r_st.clone();
        r.stub(wt::COLLIDE_SPHERE_SPH_TREE, move |cpu, mem| {
            let a0 = cpu.gpr(4) as u32;
            let a1 = cpu.gpr(5) as u32;
            let a2 = cpu.gpr(6) as u32;
            let a3 = cpu.gpr(7) as u32;
            let mut s = st.borrow_mut();
            s.calls.push(CallRec {
                target: wt::COLLIDE_SPHERE_SPH_TREE,
                args: vec![a0, a1, a3],
                f12_bits: cpu.fpr[12],
                a1_64: None,
                snap: Vec::new(),
            });
            if s.sph_recurse_ret != 0 {
                for k in 0..4u32 {
                    mem.write_u32(a2 + k * 4, s.sph_pen_out[k as usize].to_bits());
                    mem.write_u32(a3 + k * 4, s.sph_hit_out[k as usize].to_bits());
                }
            }
            cpu.set_gpr(2, s.sph_recurse_ret as u64);
        });
    }
    for (ci, &(hit, pen)) in [
        (0u32, [0.0f32, 0.0, 0.0, 0.0]),
        (1, [3.0, 4.0, 0.0, 0.0]),
        (1, [-1.5, 2.25, -4.75, 0.0]),
        (1, [0.0, 0.0, 0.0, 0.0]), // zero pen -> vsqrt(0), vdiv 1/0 = f32::MAX!
    ]
    .iter()
    .enumerate()
    {
        let mut m = fresh_mem(&mut r);
        write_both_f32(&mut r, &mut m, sph + 0x60, 8.5);
        let setup = |st: &Rc<RefCell<EnvState>>| {
            let mut s = st.borrow_mut();
            *s = EnvState::new();
            s.sph_recurse_ret = hit;
            s.sph_pen_out = pen;
            s.sph_hit_out = [7.0, 8.0, 9.0, 1.0];
        };
        setup(&r_st);
        let p_st = Rc::new(RefCell::new(EnvState::new()));
        setup(&p_st);
        let mut env = TestEnv(p_st.clone());

        let r_ret = call_u32(&mut r, 0x0026_14a8, &[sph, BASE + 0x900, out_pt, out_n, out_pen]);
        let p_ret = wt::world_sphere_sphtree(&mut m, &mut env, sph, BASE + 0x900, out_pt, out_n, out_pen);

        assert_eq!(p_ret, r_ret, "vf6 case {ci} ret");
        assert_eq!(p_st.borrow().calls, r_st.borrow().calls, "vf6 case {ci} calls");
        assert_eq!(mem_slice(&m, out_pt, 0x24), copy_out(&mut r, out_pt, 0x24), "vf6 case {ci} out");
    }
}
