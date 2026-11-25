//! The course resolver's steps (`cCourseResolve_Common`, board row E3g): the resolver (`r`) keeps the course at +4,
//! the instances' and particle instances' index tables at +8 / +0xc and its table at +0x10. Each step prints its
//! name (`Course_DebugPrint`, which prints nothing), walks `n` records and turns their numbers and offsets into
//! pointers, through the course's getters or the resolver's own virtuals. Every call out goes through
//! [`ResolveEnv`]. Row E3j adds the header pass (`ResolveDataHeader`) and the course's collision free
//! (`Course_FreeCollision`).

use crate::pf::Mem;

/// The calls out, with the float argument (f12) where there is one
pub trait ResolveEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
    fn call_f(&mut self, m: &mut Mem, func: u32, args: &[u32], f12: f32) -> u32;
}

pub const MEM_DELETE: u32 = 0x114f80;
pub const LOOKUP_INSTANCE: u32 = 0x2522e0;
pub const LOOKUP_PARTICLE: u32 = 0x252318;
pub const GET_INSTANCE: u32 = 0x254a28;
pub const GET_MATERIAL: u32 = 0x254af0;
pub const GET_SPLINE_SEGMENT: u32 = 0x254b20;
pub const GET_SPLINE: u32 = 0x254b50;
pub const DEBUG_PRINT: u32 = 0x255310;
/// gcc's float to double (`__extendsfdf2`, the library's)
pub const EXTENDSFDF2: u32 = 0x2f7aa8;
/// `cCourseResolve_Common`'s table
pub const VT: u32 = 0x3a7058;
/// The empty hierarchy a particle model without one points at
pub const NO_HIERARCHY: u32 = 0x346440;

fn print(m: &mut Mem, e: &mut dyn ResolveEnv, args: &[u32]) {
    e.call(m, DEBUG_PRINT, args);
}

/// The resolver's virtual at `off` (adjust) / `off + 4` (function) of its table (+0x10)
fn vcall(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, off: u32, rest: &[u32]) -> u32 {
    let vt = m.r32(r + 0x10);
    let me = r.wrapping_add(m.r16(vt + off) as i16 as u32);
    let mut a = vec![me];
    a.extend_from_slice(rest);
    e.call(m, m.r32(vt + off + 4), &a)
}

/// `cCourseResolve_Common::cCourseResolve_Common` 0x2521e8(r): its table, then vf1; `r`
pub fn construct(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32) -> u32 {
    m.w32(r + 0x10, VT);
    vf1(m, e, r);
    r
}

/// `cCourseResolve_Common::~cCourseResolve_Common` 0x252220(r, how): its table, vf3, deleted on bit 0
pub fn destruct(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, how: u32) {
    m.w32(r + 0x10, VT);
    vf3(m, e, r);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[r]);
    }
}

/// `cCourseResolve_Common::vf1` 0x252278 and `vf3` 0x2522b8(r): the virtual at +0x18 (`Reset`)
pub fn vf1(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32) {
    vcall(m, e, r, 0x18, &[]);
}
pub fn vf3(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32) {
    vcall(m, e, r, 0x18, &[]);
}

/// `cCourseResolve_Common::Reset` 0x2522a0(r): +0xc, +0, +4, +8 cleared
pub fn reset(m: &mut Mem, r: u32) {
    for at in [0xc, 0, 4, 8] {
        m.w32(r + at, 0);
    }
}

/// `cCourseResolve_Common::ResolveDataOnly` 0x252408(r, course) (`sp` the stack pointer on entry): 4 without a
/// course, 6 with more than 4000 particle instances (+0x2c); else the index tables on the stack (+8 at `sp` −
/// 0x4e30, +0xc 16000 bytes above it), the course kept and the virtual at +0x58 run: 1 when it answers 0, else 0
pub fn resolve_data_only(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, course: u32, sp: u32) -> u32 {
    if course == 0 {
        return 4;
    }
    if (m.r32(course + 0x2c) as i32) >= 0xfa1 {
        return 6;
    }
    let frame = sp.wrapping_sub(0x4e30);
    m.w32(r + 0xc, frame + 0x3e80);
    m.w32(r + 4, course);
    m.w32(r + 8, frame);
    (vcall(m, e, r, 0x58, &[course]) == 0) as u32
}

/// The loops below: `n` times (none when `n` ≤ 0)
fn times(n: u32) -> u32 {
    if (n as i32) > 0 { n } else { 0 }
}

/// `cCourseResolve_Common::ResolveInstances` 0x253400(r, n, p): each instance (0x100 bytes): +0xc0 by id
/// (0x2522e0), +0xf4 cleared, +0xf0 from the index table (+8), +0xc4 / +0xc8 the instance of that number (0x254a28)
/// or 0 when negative; 1
pub fn resolve_instances(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, n: u32, p: u32) -> u32 {
    print(m, e, &[0x3a6b70]);
    let mut p = p;
    for _ in 0..times(n) {
        let id = m.r32(p + 0xc0);
        let v = e.call(m, LOOKUP_INSTANCE, &[r, id]);
        m.w32(p + 0xc0, v);
        let i = m.r32(p + 0xf0);
        let t = m.r32(r + 8);
        let a = m.r32(p + 0xc4);
        m.w32(p + 0xf4, 0);
        let v = m.r32((i << 2).wrapping_add(t));
        m.w32(p + 0xf0, v);
        for (at, k) in [(0xc4, a), (0xc8, 0)] {
            let k = if at == 0xc8 { m.r32(p + 0xc8) } else { k };
            if (k as i32) < 0 {
                m.w32(p + at, 0);
            } else {
                let v = e.call(m, GET_INSTANCE, &[r, k]);
                m.w32(p + at, v);
            }
        }
        p = p.wrapping_add(0x100);
    }
    1
}

/// `cCourseResolve_Common::ResolveParticleInstances` 0x2534c8(r, n, p): each (0x70 bytes): +0x40 by id (0x252318),
/// +0x64 cleared; 1
pub fn resolve_particle_instances(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, n: u32, p: u32) -> u32 {
    print(m, e, &[0x3a6b88]);
    let mut p = p;
    for _ in 0..times(n) {
        let id = m.r32(p + 0x40);
        let v = e.call(m, LOOKUP_PARTICLE, &[r, id]);
        m.w32(p + 0x40, v);
        m.w32(p + 0x64, 0);
        p = p.wrapping_add(0x70);
    }
    1
}

/// `ResolveModelPointers` 0x253540 (message 0x3a6bb0) and `ResolveParticleModelPointers` 0x253b60 (0x3a6d88)
/// (r, n, p, base): the `n` words at `p` made pointers; 1
pub fn resolve_pointers(m: &mut Mem, e: &mut dyn ResolveEnv, text: u32, n: u32, p: u32, base: u32) -> u32 {
    print(m, e, &[text]);
    let mut p = p;
    for _ in 0..times(n) {
        let v = m.r32(p).wrapping_add(base);
        m.w32(p, v);
        p = p.wrapping_add(4);
    }
    1
}

/// `ResolveModels` 0x2535b0 (message 0x3a6bd0, virtual +0x78) and `ResolveParticleModels` 0x253bd0 (0x3a6db0,
/// +0xc8) (r, n, p): each size-prefixed record handed to the virtual; 1
pub fn resolve_records(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, text: u32, off: u32, n: u32, p: u32) -> u32 {
    print(m, e, &[text]);
    let mut p = p;
    for _ in 0..times(n) {
        vcall(m, e, r, off, &[p]);
        let s = m.r32(p);
        p = p.wrapping_add(s);
    }
    1
}

/// `cCourseResolve_Common::ResolveMeshContext` 0x253b58: 1
pub fn resolve_mesh_context() -> u32 {
    1
}

/// `cCourseResolve_Common::ResolveParticleModelHierarchies` 0x253d30(r, n, p): each (0x10 bytes) printed (its three
/// words), its +4 and +8 offsets made pointers and handed to the virtuals +0xd8 / +0xa0 when set, +0xc the
/// hierarchy (0x346440 when -1); 1
pub fn resolve_particle_hierarchies(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, n: u32, p: u32) -> u32 {
    print(m, e, &[0x3a6df0]);
    let mut p = p;
    for _ in 0..times(n) {
        print(m, e, &[0x3a6e20]);
        for (k, t) in [(0, 0x3a6ce0), (4, 0x3a6e48), (8, 0x3a6d38)] {
            let w = m.r32(p + k);
            print(m, e, &[0x3a6610, t, w]);
        }
        let a = m.r32(p + 4);
        if a != 0 {
            let a = a.wrapping_add(p);
            m.w32(p + 4, a);
            vcall(m, e, r, 0xd8, &[a]);
        }
        let h = m.r32(p + 0xc);
        m.w32(p + 0xc, if h != u32::MAX { h.wrapping_add(p) } else { NO_HIERARCHY });
        let b = m.r32(p + 8);
        if b != 0 {
            let b = b.wrapping_add(p);
            m.w32(p + 8, b);
            vcall(m, e, r, 0xa0, &[b]);
        }
        p = p.wrapping_add(0x10);
    }
    1
}

/// `cCourseResolve_Common::ResolveParticleMesh` 0x253e80(r, mesh): its words printed (the two float triples as
/// doubles), +0x24 made a pointer; 1
pub fn resolve_particle_mesh(m: &mut Mem, e: &mut dyn ResolveEnv, mesh: u32) -> u32 {
    print(m, e, &[0x3a6e58]);
    let w = m.r32(mesh);
    print(m, e, &[0x3a6610, 0x3a6d68, w]);
    for (at, t) in [(4, 0x3a65c0), (0x10, 0x3a65d0)] {
        let mut d = [0u32; 3];
        for (k, d) in d.iter_mut().enumerate() {
            let f = f32::from_bits(m.r32(mesh + at + 4 * k as u32));
            *d = e.call_f(m, EXTENDSFDF2, &[], f);
        }
        print(m, e, &[0x3a65a0, t, d[0], d[1], d[2]]);
    }
    for (at, t) in [(0x1c, 0x3a6d70), (0x20, 0x3a6e78)] {
        let w = m.r32(mesh + at);
        print(m, e, &[0x3a6610, t, w]);
    }
    let v = m.r32(mesh + 0x24).wrapping_add(mesh);
    m.w32(mesh + 0x24, v);
    1
}

/// `cCourseResolve_Common::ResolveMaterialBlocks` 0x253fb0(r, n, p): each block (a count, then that many material
/// numbers) made pointers (0x254af0), the count read again after each; 1
pub fn resolve_material_blocks(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, n: u32, p: u32) -> u32 {
    print(m, e, &[0x3a6e88]);
    let mut p = p;
    let mut i = 0u32;
    if (n as i32) > 0 {
        loop {
            let mut c = m.r32(p);
            if (c as i32) > 0 {
                let mut k = 0u32;
                let mut q = p + 4;
                loop {
                    let id = m.r32(q);
                    k += 1;
                    let v = e.call(m, GET_MATERIAL, &[r, id]);
                    m.w32(q, v);
                    c = m.r32(p);
                    q += 4;
                    if (k as i32) >= (c as i32) {
                        break;
                    }
                }
            }
            i += 1;
            p = p.wrapping_add((c << 2).wrapping_add(4));
            if (i as i32) >= (n as i32) {
                break;
            }
        }
    }
    1
}

/// `cCourseResolve_Common::ResolveSplines` 0x254160(r, n): each spline segment's +0x20 the spline of that number; 1
pub fn resolve_splines(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, n: u32) -> u32 {
    print(m, e, &[0x3a6ed8]);
    let mut i = 0u32;
    if (n as i32) > 0 {
        loop {
            let s = e.call(m, GET_SPLINE_SEGMENT, &[r, i]);
            i += 1;
            let k = m.r32(s + 0x20);
            let v = e.call(m, GET_SPLINE, &[r, k]);
            m.w32(s + 0x20, v);
            if (i as i32) >= (n as i32) {
                break;
            }
        }
    }
    1
}

/// `cCourseResolve_Common::ResolveSplineSegments` 0x2541e8(r, n): each spline's +0x50 / +0x54 the splines of those
/// numbers (0 for -1), +0x58 its first segment; 1
pub fn resolve_spline_segments(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, n: u32) -> u32 {
    print(m, e, &[0x3a6ef0]);
    let mut i = 0u32;
    if (n as i32) > 0 {
        loop {
            let s = e.call(m, GET_SPLINE, &[r, i]);
            for at in [0x50, 0x54] {
                let k = m.r32(s + at);
                if k == u32::MAX {
                    m.w32(s + at, 0);
                } else {
                    let v = e.call(m, GET_SPLINE, &[r, k]);
                    m.w32(s + at, v);
                }
            }
            let k = m.r32(s + 0x58);
            i += 1;
            let v = e.call(m, GET_SPLINE_SEGMENT, &[r, k]);
            m.w32(s + 0x58, v);
            if (i as i32) >= (n as i32) {
                break;
            }
        }
    }
    1
}

/// `cCourseResolve_Common::ResolveMaterials` 0x254600(r, n, p): each material (0x48 bytes) handed to the virtual
/// +0x110; 1
pub fn resolve_materials(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, n: u32, p: u32) -> u32 {
    print(m, e, &[0x3a7010]);
    let mut p = p;
    for _ in 0..times(n) {
        vcall(m, e, r, 0x110, &[p]);
        p = p.wrapping_add(0x48);
    }
    1
}

/// `cCourseResolve_Common::vf33` 0x254678(r, x): `x`+0x44 the particle instance of that number from the index
/// table (+0xc), 0 for -1; 1
pub fn vf33(m: &mut Mem, r: u32, x: u32) -> u32 {
    let k = m.r32(x + 0x44);
    let v = if k == u32::MAX { 0 } else { m.r32((k << 2).wrapping_add(m.r32(r + 0xc))) };
    m.w32(x + 0x44, v);
    1
}

/// The header's words printed by `ResolveDataHeader`, each with its name's text (0x3a67d0 the format)
const HEADER_WORDS: [u32; 32] = [
    0x3a6930, 0x3a6738, 0x3a6948, 0x3a6958, 0x3a6970, 0x3a6980, 0x3a6998, 0x3a6770, 0x3a69a8, 0x3a69c0, 0x3a69d8,
    0x3a69e8, 0x3a6a00, 0x3a6a10, 0x3a6a20, 0x3a6a30, 0x3a6a48, 0x3a6a58, 0x3a6a68, 0x3a6a88, 0x3a6a98, 0x3a6ab0,
    0x3a6ac0, 0x3a6ad0, 0x3a6ae8, 0x3a6b00, 0x3a6870, 0x3a6b18, 0x3a68c0, 0x3a6b38, 0x3a6b50, 0x3a6b60,
];

/// `cCourseResolve_Common::ResolveDataHeader` 0x252e18(r, h): the header `h` printed (its three version bytes, the
/// kind byte +3, which must be 0 to 3 or the answer is 0, then the words +4..+0x80); its offsets made pointers
/// when set and handed with their counts to the virtuals: +0x50, +0x54 (+0x98 with the count +0x18), the index
/// tables (`CourseResolve_IndexTable08` 0x254e58 / `0C` 0x254e98, run after +0x64 is made a pointer), +0x50 again
/// (+0x108, count +0x14), +0x68/+0x6c (+0x68 then +0x70, count +0x2c), +0x70/+0x74 (+0xb8, +0xc0, count +0x30),
/// +0x40, +0x44, +0x48 (+0x60, count +0xc), +0x4c (+0xb0, count +0x10), +0x58, +0x5c/+0x60 (+0xe0 count +0x20,
/// +0xe8 count +0x24), +0x78/+0x7c (+0xf0, +0xf8, count +0x38), +0x80 (+0x118); 1
pub fn resolve_data_header(m: &mut Mem, e: &mut dyn ResolveEnv, r: u32, h: u32) -> u32 {
    print(m, e, &[0x3a68d8]);
    let b = |m: &Mem, k: u32| m.r8(h + k) as i8 as i32 as u32;
    let v = [b(m, 0), b(m, 1), b(m, 2)];
    print(m, e, &[0x3a6568, v[0], v[1], v[2]]);
    let text = match m.r8(h + 3) as i8 {
        0 => 0x3a68f0,
        1 => 0x3a6900,
        2 => 0x3a6910,
        3 => 0x3a6920,
        _ => return 0,
    };
    print(m, e, &[text]);
    for (k, t) in HEADER_WORDS.iter().enumerate() {
        let w = m.r32(h + 4 + 4 * k as u32);
        print(m, e, &[0x3a67d0, *t, w]);
    }
    // the offset at `at` made a pointer when set: `Some(pointer)`
    let fix = |m: &mut Mem, at: u32| {
        let o = m.r32(h + at);
        (o != 0).then(|| {
            let p = o.wrapping_add(h);
            m.w32(h + at, p);
            p
        })
    };
    fix(m, 0x50);
    if let Some(p) = fix(m, 0x54) {
        let n = m.r32(h + 0x18);
        vcall(m, e, r, 0x98, &[n, p]);
    }
    fix(m, 0x64);
    crate::course_lookup::index_table_08(m, r);
    crate::course_lookup::index_table_0c(m, r);
    let p = m.r32(h + 0x50);
    if p != 0 {
        let n = m.r32(h + 0x14);
        vcall(m, e, r, 0x108, &[n, p]);
    }
    // a pair: the second offset decides, the first is made a pointer with it (without a test of its own)
    let pair = |m: &mut Mem, e: &mut dyn ResolveEnv, a: u32, n_at: u32, v1: u32, v2: u32| {
        if let Some(q) = fix(m, a + 4) {
            let p = m.r32(h + a).wrapping_add(h);
            m.w32(h + a, p);
            let n = m.r32(h + n_at);
            vcall(m, e, r, v1, &[n, p, q]);
            let n = m.r32(h + n_at);
            let q = m.r32(h + a + 4);
            vcall(m, e, r, v2, &[n, q]);
        }
    };
    pair(m, e, 0x68, 0x2c, 0x68, 0x70);
    pair(m, e, 0x70, 0x30, 0xb8, 0xc0);
    fix(m, 0x40);
    fix(m, 0x44);
    if let Some(p) = fix(m, 0x48) {
        let n = m.r32(h + 0xc);
        vcall(m, e, r, 0x60, &[n, p]);
    }
    if let Some(p) = fix(m, 0x4c) {
        let n = m.r32(h + 0x10);
        vcall(m, e, r, 0xb0, &[n, p]);
    }
    fix(m, 0x58);
    if let Some(p) = fix(m, 0x5c) {
        let q = m.r32(h + 0x60).wrapping_add(h);
        m.w32(h + 0x60, q);
        let n = m.r32(h + 0x20);
        vcall(m, e, r, 0xe0, &[n, p, q]);
        let n = m.r32(h + 0x24);
        let (p, q) = (m.r32(h + 0x5c), m.r32(h + 0x60));
        vcall(m, e, r, 0xe8, &[n, p, q]);
    }
    pair(m, e, 0x78, 0x38, 0xf0, 0xf8);
    if let Some(p) = fix(m, 0x80) {
        vcall(m, e, r, 0x118, &[p]);
    }
    print(m, e, &[0x3a66e8]);
    1
}

/// `Mem_Free` 0x14a040
pub const MEM_FREE: u32 = 0x14a040;
/// `SphereTreeBufCache`'s table, `SphereTreeBufCache_FreeAll` 0x238dd0 and its destructor 0x23b520
pub const SPHERE_CACHE_VT: u32 = 0x3a40f8;
pub const SPHERE_CACHE_FREE_ALL: u32 = 0x238dd0;
pub const SPHERE_CACHE_DESTRUCT: u32 = 0x23b520;

/// `Course_FreeCollision` 0x25fd40(c) (`sp` the stack pointer on entry): +0x14 and +0x20 freed and cleared; the
/// array at +0x18 (8 bytes each, its count 0x10 bytes before it) emptied from the end (`Mem_DeleteIfFlag9`
/// 0x260fa0 with 0: nothing), freed from its count and cleared; +0x24 freed and cleared; the array at +0x1c (0x150
/// bytes each) destroyed from the end through each one's virtual +8 with 0, freed and cleared (both loops read the
/// array again each time); then a `SphereTreeBufCache` made on the stack (`sp` − 0x50), emptied
/// (`SphereTreeBufCache_FreeAll`) and destroyed (with 2)
pub fn free_collision(m: &mut Mem, e: &mut dyn ResolveEnv, c: u32, sp: u32) {
    let free = |m: &mut Mem, e: &mut dyn ResolveEnv, at: u32| {
        let p = m.r32(c + at);
        if p != 0 {
            e.call(m, MEM_FREE, &[p]);
            m.w32(c + at, 0);
        }
    };
    free(m, e, 0x14);
    free(m, e, 0x20);
    let a = m.r32(c + 0x18);
    if a != 0 {
        let mut s = a.wrapping_add(m.r32(a.wrapping_sub(0x10)) << 3);
        let mut a = a;
        while a != s {
            s = s.wrapping_sub(8);
            // `Mem_DeleteIfFlag9`(s, 0) (`copies::delete_if`): deletes nothing
            a = m.r32(c + 0x18);
        }
        e.call(m, MEM_FREE, &[a.wrapping_sub(0x10)]);
        m.w32(c + 0x18, 0);
    }
    free(m, e, 0x24);
    let a = m.r32(c + 0x1c);
    if a != 0 {
        let mut s = m.r32(a.wrapping_sub(0x10)).wrapping_mul(0x150).wrapping_add(a);
        let mut a = a;
        while a != s {
            s = s.wrapping_sub(0x150);
            let vt = m.r32(s);
            let me = s.wrapping_add(m.r16(vt + 8) as i16 as u32);
            e.call(m, m.r32(vt + 0xc), &[me, 0]);
            a = m.r32(c + 0x1c);
        }
        e.call(m, MEM_FREE, &[a.wrapping_sub(0x10)]);
        m.w32(c + 0x1c, 0);
    }
    let frame = sp.wrapping_sub(0x50);
    m.w32(frame, SPHERE_CACHE_VT);
    e.call(m, SPHERE_CACHE_FREE_ALL, &[frame]);
    e.call(m, SPHERE_CACHE_DESTRUCT, &[frame, 2]);
}
