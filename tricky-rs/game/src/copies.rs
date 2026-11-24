//! The small helpers the compiler copied into many files (board row X2c): the 28 copies of `Vec4_ScaleCopy`, the
//! empty constructors, `Vec4_AddTo`, the `memcpy` wrappers, `Rand_Init`, `Vec3_PackShort`,
//! `Mat4_TransformVec4Copy1`, `Mem_DeleteIfFlag9` and `PtrArray_FreeAll`, on the game's bytes. Calls out go
//! through [`FeEnv`]; `memcpy` (a library function the app hosts) is done here.

use crate::fe3dstate::FeEnv;
use crate::pf::Mem;
use crate::ps2f::F;
use crate::sndlib::cvt_w;

/// The copies of `Vec4_ScaleCopy` not ported elsewhere (`Vec4_ScaleCopy1`..`31`; 0x1ce058, 0x26cd98 and 0x2adcb8
/// are in `lensfx`, `fe_misc` and `fe_wbar`), each the same eleven instructions as `Vec4_Scale` 0x102f50
pub const SCALE_COPIES: [u32; 28] = [
    0x105b90, 0x1097f8, 0x10afb0, 0x10c138, 0x110868, 0x1231d8, 0x126918, 0x12c088, 0x1370d8, 0x1381e0, 0x149fd8,
    0x154548, 0x1780b8, 0x197d70, 0x1cae48, 0x1d8dc0, 0x1d9b38, 0x1ea358, 0x201098, 0x208628, 0x238a80, 0x239930,
    0x23b588, 0x257d50, 0x25c798, 0x25ec30, 0x261608, 0x2b6a70,
];
/// `Rand_Init` 0x2401a0's 24 bytes (the random state's seed words)
pub const RAND_SEED: u32 = 0x3a4a00;
/// `Mem_Free` 0x14a040, `Mem_Delete` 0x114f80
pub const MEM_FREE: u32 = 0x14a040;
pub const MEM_DELETE: u32 = 0x114f80;

fn rq(m: &Mem, a: u32) -> [F; 4] {
    [0, 1, 2, 3].map(|k| F(f32::from_bits(m.r32(a + 4 * k))))
}
fn wq(m: &mut Mem, a: u32, v: [F; 4]) {
    for (k, x) in v.into_iter().enumerate() {
        m.w32(a + 4 * k as u32, x.0.to_bits());
    }
}
fn copy(m: &mut Mem, dst: u32, src: u32, n: u32) {
    let b: Vec<u8> = (0..n).map(|k| m.r8(src + k)).collect();
    for (k, x) in b.into_iter().enumerate() {
        m.w8(dst + k as u32, x);
    }
}

/// `Vec4_ScaleCopy*` ([`SCALE_COPIES`])(out, v; s): every lane of `v` times `s` (VU0 `vmulx`); `out`
pub fn vec4_scale(m: &mut Mem, out: u32, v: u32, s: F) -> u32 {
    let x = rq(m, v).map(|a| a * s);
    wq(m, out, x);
    out
}

/// `Quat_Construct` 0x110860, `Vec4_Construct` 0x110898(p): nothing; `p`
pub fn construct(p: u32) -> u32 {
    p
}

/// `Vec4_AddTo` 0x1108d0(a, b): a += b, all four lanes; `a`
pub fn vec4_add_to(m: &mut Mem, a: u32, b: u32) -> u32 {
    let (x, y) = (rq(m, a), rq(m, b));
    wq(m, a, [0, 1, 2, 3].map(|k| x[k] + y[k]));
    a
}

/// `Vec3_Assign` 0x198720 (12 bytes), `Mem_Copy4` 0x198738 and `Float_Assign` 0x198750 (4)(dst, src): `memcpy`
/// unless they are the same place
pub fn assign(m: &mut Mem, dst: u32, src: u32, n: u32) {
    if dst != src {
        copy(m, dst, src, n);
    }
}

/// `Rand_Init` 0x2401a0(state): the 24 seed bytes at [`RAND_SEED`] copied in
pub fn rand_init(m: &mut Mem, state: u32) {
    copy(m, state, RAND_SEED, 0x18);
}

/// `Vec3_PackShort` 0x1e7da0(_, base, i, v): x, y, z times 32767 (truncated, as `cvt.w.s`) as three halfwords
/// at `base + 6i`
pub fn vec3_pack_short(m: &mut Mem, base: u32, i: u32, v: u32) {
    let at = base.wrapping_add(i.wrapping_mul(6));
    let k = F(f32::from_bits(0x46fffe00));
    for c in 0..3 {
        let x = F(f32::from_bits(m.r32(v + 4 * c))) * k;
        m.w16(at + 2 * c, cvt_w(x.0) as u16);
    }
}

/// `Mat4_TransformVec4Copy1` 0x2085e8(out, mat, v): Σ mat rows × v's lanes (VU0 `vmulax`, `vmadday`, `vmaddaz`,
/// `vmaddw`); `out`
pub fn mat4_transform(m: &mut Mem, out: u32, mat: u32, v: u32) -> u32 {
    let r = [0, 1, 2, 3].map(|k| rq(m, mat + 0x10 * k));
    let x = rq(m, v);
    let o = [0, 1, 2, 3].map(|i| ((r[0][i] * x[0] + r[1][i] * x[1]) + r[2][i] * x[2]) + r[3][i] * x[3]);
    wq(m, out, o);
    out
}

/// `Mem_DeleteIfFlag9` 0x260fa0(p, how): `Mem_Delete(p)` when `how` has bit 0
pub fn delete_if(m: &mut Mem, e: &mut dyn FeEnv, p: u32, how: u32) {
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[p]);
    }
}

/// `PtrArray_FreeAll` 0x21edf0(arr): each of the `arr[0]` pointers after it (the count read again every time)
/// freed if set and cleared; the count zeroed
pub fn ptr_array_free_all(m: &mut Mem, e: &mut dyn FeEnv, arr: u32) {
    let mut i = 0u32;
    if (m.r32(arr) as i32) > 0 {
        loop {
            let at = arr + 4 + 4 * i;
            let p = m.r32(at);
            i += 1;
            if p != 0 {
                e.call(m, MEM_FREE, &[p]);
            }
            m.w32(at, 0);
            if (i as i32) >= (m.r32(arr) as i32) {
                break;
            }
        }
    }
    m.w32(arr, 0);
}
