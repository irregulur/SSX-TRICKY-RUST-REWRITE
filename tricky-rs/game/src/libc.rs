//! The C library's string, memory and float functions (board rows E2l, E2m). The string and memory functions are
//! newlib's, rewritten for the Emotion Engine: when both pointers are 8- or 16-byte aligned they go a doubleword or
//! a quadword at a time while no 0 byte is in the block.
//!
//! Those blocks change nothing that can be seen, so they are byte loops here: the comparisons and `strlen` only
//! skip blocks that are equal or hold no 0; a copy reads a block whole and then writes it, and as both pointers
//! are aligned to the block they are a whole number of blocks apart, so even a copy onto its own source a block
//! ahead leaves what a byte loop leaves; `memset` stores the same byte everywhere. The float functions (`fmodf`
//! 0x2f2908, `logf` 0x2f2a48 and their `__ieee754_*` cores) are newlib fdlibm compiled for the EE's non-IEEE FPU
//! ([`crate::ps2f::F`]: no NaNs, infinities or denormals) with `_LIB_VERSION` 0x3b9f98 = 1 and default `matherr`
//! 0x2f40d0. Checked in the function runner (`tests/libc.rs`).

use crate::pf::Mem;
use crate::ps2f::F;

/// newlib's `_ctype_` + 1 (0x3ba520 + 1): bit 0 marks a capital letter. The case-blind comparisons look a
/// character up as a signed byte while they walk and as an unsigned one at the end, so a byte from 0x80 up reads
/// the bytes before the table (the names of the C++ type-info classes and the assert message) inside the loop.
pub const CTYPE: u32 = 0x3ba521;

/// `strlen` 0x2fd260: the bytes before the first 0.
pub fn strlen(m: &Mem, s: u32) -> u32 {
    let mut p = s;
    while m.r8(p) != 0 {
        p = p.wrapping_add(1);
    }
    p.wrapping_sub(s)
}

/// `strcpy` 0x2fd14c: bytes up to and with the 0. Returns `d`. (The game's goes a quadword or a doubleword at a
/// time while the next block holds no 0, when both pointers are 16- or 8-byte aligned; see the module's note.)
pub fn strcpy(m: &mut Mem, d: u32, s: u32) -> u32 {
    let mut k = 0u32;
    loop {
        let c = m.r8(s.wrapping_add(k));
        m.w8(d.wrapping_add(k), c);
        if c == 0 {
            return d;
        }
        k = k.wrapping_add(1);
    }
}

/// `strncpy` 0x2fd630(d, s, n): bytes up to and with the 0, at most `n`, and the rest of the `n` filled with 0.
/// Returns `d`. (Blocks as `strcpy`'s; its first quadword's test is not the 0-byte test, see the port notes.)
pub fn strncpy(m: &mut Mem, d: u32, s: u32, n: u32) -> u32 {
    let mut k = 0u32;
    while k < n {
        let c = m.r8(s.wrapping_add(k));
        m.w8(d.wrapping_add(k), c);
        k += 1;
        if c == 0 {
            break;
        }
    }
    while k < n {
        m.w8(d.wrapping_add(k), 0);
        k += 1;
    }
    d
}

/// `memcpy` 0x2fb2b4(d, s, n): `n` bytes, first to last. Returns `d`. (With `n` ≥ 32 and both pointers 16-byte
/// aligned the game's goes two quadwords at a time while 32 are left, then doublewords while 8 are.)
pub fn memcpy(m: &mut Mem, d: u32, s: u32, n: u32) -> u32 {
    for k in 0..n {
        let c = m.r8(s.wrapping_add(k));
        m.w8(d.wrapping_add(k), c);
    }
    d
}

/// `memset` 0x2fb468(d, c, n): `n` bytes of `c`'s low byte (16-byte stores when `d` is 16-byte aligned and
/// `n` ≥ 8, the same bytes). Returns `d`.
pub fn memset(m: &mut Mem, d: u32, c: u32, n: u32) -> u32 {
    for k in 0..n {
        m.w8(d.wrapping_add(k), c as u8);
    }
    d
}

/// `strcmp` 0x2fd008: the difference of the first unequal bytes, as unsigned, or 0.
pub fn strcmp(m: &Mem, a: u32, b: u32) -> i32 {
    let (mut a, mut b) = (a, b);
    loop {
        let (x, y) = (m.r8(a), m.r8(b));
        if x == 0 || x != y {
            return x as i32 - y as i32;
        }
        a = a.wrapping_add(1);
        b = b.wrapping_add(1);
    }
}

/// `strncmp` 0x2fd478: as `strcmp` over at most `n` bytes.
pub fn strncmp(m: &Mem, a: u32, b: u32, n: u32) -> i32 {
    let (mut a, mut b, mut n) = (a, b, n);
    if n == 0 {
        return 0;
    }
    loop {
        let (x, y) = (m.r8(a), m.r8(b));
        n -= 1;
        if x != y {
            return x as i32 - y as i32;
        }
        if n == 0 || x == 0 {
            return 0;
        }
        a = a.wrapping_add(1);
        b = b.wrapping_add(1);
    }
}

/// `memcmp` 0x2fb220: the difference of the first unequal bytes of `n`, as unsigned, or 0.
pub fn memcmp(m: &Mem, a: u32, b: u32, n: u32) -> i32 {
    for k in 0..n {
        let (x, y) = (m.r8(a.wrapping_add(k)), m.r8(b.wrapping_add(k)));
        if x != y {
            return x as i32 - y as i32;
        }
    }
    0
}

/// A character in lower case the way the walk sees it: as a signed byte, looked up at [`CTYPE`] + c.
fn lower_signed(m: &Mem, c: u8) -> i32 {
    let c = c as i8 as i32;
    if m.r8(CTYPE.wrapping_add(c as u32)) & 1 != 0 {
        c + 0x20
    } else {
        c
    }
}

/// The same at the end, as an unsigned byte.
fn lower_unsigned(m: &Mem, c: u8) -> i32 {
    let c = c as i32;
    if m.r8(CTYPE + c as u32) & 1 != 0 {
        c + 0x20
    } else {
        c
    }
}

/// `stricmp` 0x2fcc98: walks while the first string has not ended and the two characters are equal in lower
/// case; returns the difference of the two characters it stopped at, in lower case.
pub fn stricmp(m: &Mem, a: u32, b: u32) -> i32 {
    let (mut a, mut b) = (a, b);
    while m.r8(a) != 0 {
        if lower_signed(m, m.r8(a)) != lower_signed(m, m.r8(b)) {
            break;
        }
        a = a.wrapping_add(1);
        b = b.wrapping_add(1);
    }
    lower_unsigned(m, m.r8(a)) - lower_unsigned(m, m.r8(b))
}

/// `strnicmp` 0x2fd398(a, b, n): as `stricmp` over at most `n` characters, stopping after either string's end.
pub fn strnicmp(m: &Mem, a: u32, b: u32, n: u32) -> i32 {
    let (mut a, mut b) = (a, b);
    if n == 0 {
        return 0;
    }
    let mut left = n.wrapping_sub(1) as i32;
    while left != -1 {
        let (x, y) = (m.r8(a), m.r8(b));
        if lower_signed(m, x) != lower_signed(m, y) || left == 0 || x == 0 {
            break;
        }
        left = left.wrapping_sub(1);
        if y == 0 {
            break;
        }
        a = a.wrapping_add(1);
        b = b.wrapping_add(1);
    }
    lower_unsigned(m, m.r8(a)) - lower_unsigned(m, m.r8(b))
}

/// `isnanf` 0x2f4200: raw IEEE NaN bit-pattern test (`|x| > 0x7f80_0000`, integer `mfc1`, bypassing the EE FPU).
pub fn isnanf(x: f32) -> bool {
    (x.to_bits() & 0x7fff_ffff) > 0x7f80_0000
}

/// `__ieee754_fmodf` 0x2f2b90: newlib single-precision remainder. Integer-only for finite `x` and `y != 0`;
/// `y == 0`, `|x| >= inf` or `|y| > inf` fall into `(x * y) / (x * y)` on the EE FPU (`ps2f::F`: `+f32::MAX` when
/// `x * y` flushes to 0, `1.0` otherwise).
pub fn ieee754_fmodf(x: f32, y: f32) -> f32 {
    let bx = x.to_bits();
    let sx = bx & 0x8000_0000;
    let mut hx = (bx ^ sx) as i32;
    let mut hy = (y.to_bits() & 0x7fff_ffff) as i32;

    if hy == 0 || hx > 0x7f7f_ffff || hy > 0x7f80_0000 {
        let p = F(x) * F(y);
        return (p / p).0;
    }
    if hx < hy {
        return x;
    }
    if hx == hy {
        return f32::from_bits(sx);
    }

    let ix = if hx < 0x0080_0000 {
        let mut e = -126i32;
        let mut i = hx << 8;
        while i > 0 {
            e -= 1;
            i <<= 1;
        }
        e
    } else {
        (hx >> 23) - 127
    };

    let mut iy = if hy < 0x0080_0000 {
        let mut e = -126i32;
        let mut i = hy << 8;
        while i >= 0 {
            e -= 1;
            i <<= 1;
        }
        e
    } else {
        (hy >> 23) - 127
    };

    if ix >= -126 {
        hx = 0x0080_0000 | (0x007f_ffff & hx);
    } else {
        hx <<= (-126 - ix) & 31;
    }
    if iy >= -126 {
        hy = 0x0080_0000 | (0x007f_ffff & hy);
    } else {
        hy <<= (-126 - iy) & 31;
    }

    let mut n = ix - iy;
    while n != 0 {
        n -= 1;
        let hz = hx.wrapping_sub(hy);
        if hz < 0 {
            hx <<= 1;
        } else {
            if hz == 0 {
                return f32::from_bits(sx);
            }
            hx = hz << 1;
        }
    }
    let hz = hx.wrapping_sub(hy);
    if hz >= 0 {
        hx = hz;
    }
    if hx == 0 {
        return f32::from_bits(sx);
    }

    while hx < 0x0080_0000 {
        hx <<= 1;
        iy -= 1;
    }
    if iy >= -126 {
        hx = (hx - 0x0080_0000) | ((iy + 127) << 23);
    } else {
        hx >>= (-126 - iy) & 31;
    }
    f32::from_bits((hx as u32) | sx)
}

/// `fmodf` 0x2f2908: calls `__ieee754_fmodf` 0x2f2b90, then (with `_LIB_VERSION` 0x3b9f98 = 1 and `matherr`
/// 0x2f40d0 returning 0) when neither input is NaN (`isnanf` 0x2f4200) and `c.eq.s y, 0.0` sees `y == 0` on the EE
/// FPU (`±0.0` or any subnormal `y`), sets `errno = EDOM` (`0x21`) and returns `__truncdfsf2(0x7ff8_0000_0000_0000)`
/// (`0x7fb0_0000` via `__unpack_d` 0x2f5be8 / `__pack_f` 0x2f6f08).
pub fn fmodf(x: f32, y: f32) -> f32 {
    let z = ieee754_fmodf(x, y);
    if !isnanf(y) && !isnanf(x) && F(y).ee() == 0.0 {
        return f32::from_bits(0x7fb0_0000);
    }
    z
}

/// `__ieee754_logf` 0x2f2de0: newlib single-precision natural logarithm, evaluated in EE FPU arithmetic
/// (`ps2f::F`).
pub fn ieee754_logf(x: f32) -> f32 {
    const LN2_HI: F = F(f32::from_bits(0x3f31_7180));
    const LN2_LO: F = F(f32::from_bits(0x3717_f7d1));
    const TWO25: F = F(f32::from_bits(0x4c00_0000));
    const LG1: F = F(f32::from_bits(0x3f2a_aaab));
    const LG2: F = F(f32::from_bits(0x3ecc_cccd));
    const LG3: F = F(f32::from_bits(0x3e92_4925));
    const LG4: F = F(f32::from_bits(0x3e63_8e29));
    const LG5: F = F(f32::from_bits(0x3e3a_3325));
    const LG6: F = F(f32::from_bits(0x3e1c_d04f));
    const LG7: F = F(f32::from_bits(0x3e17_8897));
    const THIRD: F = F(f32::from_bits(0x3eaa_aaab));

    let mut ix = x.to_bits() as i32;
    let mut k = 0i32;
    if ix < 0x0080_0000 {
        if (ix & 0x7fff_ffff) == 0 {
            return f32::from_bits(0xff80_0000);
        }
        if ix < 0 {
            return ((F(x) - F(x)) / F(0.0)).0;
        }
        k -= 25;
        ix = (F(x) * TWO25).0.to_bits() as i32;
    }
    if ix >= 0x7f80_0000 {
        let xf = F(f32::from_bits(ix as u32));
        return (xf + xf).0;
    }

    k += (ix >> 23) - 127;
    ix &= 0x007f_ffff;
    let i = ix.wrapping_add(0x004a_fb20) & 0x0080_0000;
    let xf = f32::from_bits((ix | (i ^ 0x3f80_0000)) as u32);
    k += i >> 23;
    let f = F(xf) - F(1.0);

    if (ix.wrapping_add(0x0f) & 0x007f_ffff) < 0x10 {
        if f.ee() == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = F(k as f32);
            return (dk * LN2_HI + dk * LN2_LO).0;
        }
        let r = (f * f) * (F(0.5) - f * THIRD);
        if k == 0 {
            return (f - r).0;
        }
        let dk = F(k as f32);
        return (dk * LN2_HI - ((r - dk * LN2_LO) - f)).0;
    }

    let s = f / (f + F(2.0));
    let dk = F(k as f32);
    let z = s * s;
    let w = z * z;
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let r = t2 + t1;
    let v1 = ix.wrapping_sub(0x0030_a3d0) | 0x0035_c288i32.wrapping_sub(ix);
    if v1 > 0 {
        let hfsq = (f * F(0.5)) * f;
        if k == 0 {
            (f - (hfsq - s * (hfsq + r))).0
        } else {
            (dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)).0
        }
    } else if k == 0 {
        (f - s * (f - r)).0
    } else {
        (dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)).0
    }
}

/// `logf` 0x2f2a48: calls `__ieee754_logf` 0x2f2de0, then (with `_LIB_VERSION` 0x3b9f98 = 1 and `matherr`
/// 0x2f40d0 returning 0) when `x` is not NaN (`isnanf` 0x2f4200) and `c.lt.s 0.0, x` is false on the EE FPU
/// (`x <= 0.0` or any subnormal `x`), sets `errno = EDOM` (`0x21`) and returns `__truncdfsf2(__subdf3(0.0, +inf))`
/// (`0xff80_0000`, `-inf`).
pub fn logf(x: f32) -> f32 {
    let z = ieee754_logf(x);
    if !isnanf(x) && !(0.0 < F(x).ee()) {
        return f32::from_bits(0xff80_0000);
    }
    z
}
