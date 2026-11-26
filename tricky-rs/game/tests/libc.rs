//! `tricky_game::libc` (board row E2l) against the original in the function runner (TRICKY_ELF; skipped without
//! it): the C library's string and memory functions on random bytes (text, bytes from 0x80 up, long runs with no
//! 0), at every alignment but mostly 8- and 16-byte aligned so the block paths run, with copies that overlap and
//! second strings that are the first one changed in one place, in case, or cut short. Every byte of the area and
//! the answer must be equal.

use r5900::Runner;
use tricky_game::libc;
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
}

const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x400;
/// Around newlib's `_ctype_` (0x3ba520), the bytes before it included.
const CTYPE_AT: u32 = 0x3ba400;
const CTYPE_LEN: u32 = 0x300;

const STRLEN: u32 = 0x2fd260;
const STRCPY: u32 = 0x2fd14c;
const STRNCPY: u32 = 0x2fd630;
const MEMCPY: u32 = 0x2fb2b4;
const MEMSET: u32 = 0x2fb468;
const STRCMP: u32 = 0x2fd008;
const STRNCMP: u32 = 0x2fd478;
const MEMCMP: u32 = 0x2fb220;
const STRICMP: u32 = 0x2fcc98;
const STRNICMP: u32 = 0x2fd398;
const FUNCS: [u32; 10] = [STRLEN, STRCPY, STRNCPY, MEMCPY, MEMSET, STRCMP, STRNCMP, MEMCMP, STRICMP, STRNICMP];

/// A byte of one of the fills.
fn byte(g: &mut Rng, kind: u32) -> u8 {
    const TEXT: &[u8] = b"AaBbCcXxYyZz019_./\\ @[`{";
    match kind {
        // text, now and then a byte from 0x80 up or a 0
        0 => match g.below(40) {
            0 => 0,
            1..=3 => 0x80 | g.below(0x80) as u8,
            _ => TEXT[g.below(TEXT.len() as u32) as usize],
        },
        // long runs, no 0 but rarely
        1 => match g.below(200) {
            0 => 0,
            _ => 1 + g.below(255) as u8,
        },
        // 0x7f and up: `strncpy`'s first quadword goes through whole
        2 => 0x7f + g.below(0x81) as u8,
        _ => g.next() as u8,
    }
}

/// An offset in the first half of the area: mostly 16- or 8-byte aligned.
fn place(g: &mut Rng) -> u32 {
    let p = g.below(0x200);
    match g.below(10) {
        0..=4 => p & !0xf,
        5..=6 => p & !7,
        _ => p,
    }
}

fn count(g: &mut Rng) -> u32 {
    match g.below(3) {
        0 => [0, 1, 2, 7, 8, 9, 15, 16, 17, 23, 24, 31, 32, 33, 48, 64][g.below(16) as usize],
        1 => g.below(40),
        _ => g.below(0x120),
    }
}

fn case_swap(c: u8) -> u8 {
    if c.is_ascii_alphabetic() {
        c ^ 0x20
    } else {
        c
    }
}

#[test]
fn the_c_library_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let mut ctype = vec![0u8; CTYPE_LEN as usize];
    r.mem.read(CTYPE_AT, &mut ctype).unwrap();
    let mut g = Rng(0xe2d0_0000_0000_0001);
    let (mut equal, mut overlaps, mut nonzero) = (0u32, 0u32, 0u32);
    for case in 0..FUNCS.len() as u32 * 8000 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let kind = g.below(4);
        let mut area: Vec<u8> = (0..SIZE).map(|_| byte(&mut g, kind)).collect();
        // every string ends within 0x100 bytes, so a copy stays inside the area
        for k in (0xff..SIZE as usize).step_by(0x100) {
            area[k] = 0;
        }
        let (a, mut b) = (place(&mut g), place(&mut g));
        // the second string: the first one, changed a little
        if g.below(3) != 0 && func != MEMSET {
            let len = g.below(0x100) as usize;
            let (a, b) = (a as usize, b as usize);
            let copy: Vec<u8> = area[a..a + len].to_vec();
            for (k, &c) in copy.iter().enumerate() {
                if (b + k) % 0x100 != 0xff {
                    area[b + k] = if g.below(6) == 0 { case_swap(c) } else { c };
                }
            }
            let k = g.below(len as u32 + 1) as usize;
            match g.below(5) {
                0 => area[b + k] = byte(&mut g, 3),
                1 => area[b + k] = 0,
                // the first string ends where the second has 0xe0, which `strnicmp`'s walk lowers to 0
                2 if (b + k) % 0x100 != 0xff => {
                    area[a + k] = 0;
                    area[b + k] = 0xe0;
                }
                _ => {}
            }
        }
        // copies now and then onto or from a few bytes away, often a whole number of doublewords away
        if g.below(3) == 0 {
            b = match g.below(2) {
                0 => (a + g.below(0x20)).saturating_sub(0x10),
                _ => (a + 8 * g.below(5)).saturating_sub(0x10),
            };
        }
        let (mut a, mut b) = (ARENA + a, ARENA + b);
        let n = count(&mut g);
        let c = g.next();

        let mut m = Mem { regions: vec![(ARENA, area.clone()), (CTYPE_AT, ctype.clone())] };
        let (args, ours): (Vec<u32>, u32) = match func {
            STRLEN => (vec![b], libc::strlen(&m, b)),
            STRCPY => {
                // onto its own string ahead it would copy on for ever: only from ahead onto behind
                let len = libc::strlen(&m, b);
                if b < a && a <= b + len {
                    (a, b) = (b, a);
                }
                overlaps += (a < b && b <= a + libc::strlen(&m, b)) as u32;
                (vec![a, b], libc::strcpy(&mut m, a, b))
            }
            STRNCPY => {
                overlaps += (b < a && a < b + n) as u32;
                (vec![a, b, n], libc::strncpy(&mut m, a, b, n))
            }
            MEMCPY => {
                overlaps += (b < a && a < b + n) as u32;
                (vec![a, b, n], libc::memcpy(&mut m, a, b, n))
            }
            MEMSET => (vec![a, c, n], libc::memset(&mut m, a, c, n)),
            STRCMP => (vec![a, b], libc::strcmp(&m, a, b) as u32),
            STRNCMP => (vec![a, b, n], libc::strncmp(&m, a, b, n) as u32),
            MEMCMP => (vec![a, b, n], libc::memcmp(&m, a, b, n) as u32),
            STRICMP => (vec![a, b], libc::stricmp(&m, a, b) as u32),
            _ => (vec![a, b, n], libc::strnicmp(&m, a, b, n) as u32),
        };
        equal += (ours == 0) as u32;
        nonzero += (ours != 0 && func != STRLEN) as u32;

        r.mem.write(ARENA, &area).unwrap();
        for (k, x) in args.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *x as i32 as i64 as u64);
        }
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x} {args:x?}: {e}"));
        assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x} {args:x?}: the answer");
        let mut th = vec![0u8; SIZE as usize];
        r.mem.read(ARENA, &mut th).unwrap();
        let ourm = &m.regions[0].1;
        if &th != ourm {
            let i = th.iter().zip(ourm).position(|(x, y)| x != y).unwrap();
            panic!("case {case} {func:#x} {args:x?}: byte at {:#x}: theirs {:#x} ours {:#x}", ARENA + i as u32, th[i], ourm[i]);
        }
    }
    println!("{equal} answers 0, {nonzero} not, {overlaps} copies onto their own source");
    assert!(equal > 5000 && nonzero > 5000 && overlaps > 300);
}

#[test]
fn the_c_library_floats_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    const FMODF: u32 = 0x2f2908;
    const LOGF: u32 = 0x2f2a48;
    const IEEE754_FMODF: u32 = 0x2f2b90;
    const IEEE754_LOGF: u32 = 0x2f2de0;
    const ISNANF: u32 = 0x2f4200;
    const ERRNO_PTR: u32 = 0x35e9ec;
    for f in [
        FMODF, LOGF, IEEE754_FMODF, IEEE754_LOGF, ISNANF, 0x2f40d0, 0x2f64b0, 0x2f6398, 0x2f7aa8, 0x2f7018, 0x2f6728,
        0x2f5ab8, 0x2f5f20, 0x2f5c88, 0x2f6758, 0x2f5be8, 0x2f7a78, 0x2f6f08, 0x2fa4e0,
    ] {
        r.allow(f);
    }
    let reent = r.mem.read_u32(ERRNO_PTR);
    assert_ne!(reent, 0, "_impure_ptr must point to _reent");

    // Exhaustive special values: +/-0, subnormals, min/max normals, powers of two, +/-inf, quiet & signaling NaNs.
    const EDGES: &[u32] = &[
        0x0000_0000, 0x8000_0000, // +0, -0
        0x0000_0001, 0x0000_0002, 0x0040_0000, 0x007f_ffff, // +subnormals
        0x8000_0001, 0x8040_0000, 0x807f_ffff, // -subnormals
        0x0080_0000, 0x8080_0000, // +/-MIN_POSITIVE
        0x3e80_0000, 0x3f00_0000, 0x3f35_04f3, 0x3f80_0000, 0x3f80_0001, 0x3f7f_ffff, // near 0.25, 0.5, sqrt(0.5), 1
        0x3fb5_04f3, 0x4000_0000, 0x4049_0fdb, 0x42c8_0000, 0x4f00_0000, // sqrt(2), 2, pi, 100, 2^31
        0xbf80_0000, 0xc000_0000, // -1, -2
        0x7f7f_ffff, 0xff7f_ffff, // +/-MAX
        0x7f80_0000, 0xff80_0000, // +/-inf
        0x7f80_0001, 0x7fb0_0000, 0x7fc0_0000, 0x7fff_ffff, // +NaNs
        0xff80_0001, 0xffc0_0000, 0xffff_ffff, // -NaNs
    ];

    let mut g = Rng(0xe2e0_0000_0000_0001);

    // 1. isnanf 0x2f4200
    for &b in EDGES {
        r.cpu.fpr[12] = b;
        r.call(ISNANF).unwrap();
        assert_eq!(r.cpu.gpr(2) != 0, libc::isnanf(f32::from_bits(b)), "isnanf({b:#010x})");
    }

    // 2. __ieee754_fmodf 0x2f2b90 and fmodf 0x2f2908: all edge pairs + 20,000 random pairs
    let mut fmod_pairs: Vec<(u32, u32)> = Vec::with_capacity(EDGES.len() * EDGES.len() + 20_000);
    for &x in EDGES {
        for &y in EDGES {
            fmod_pairs.push((x, y));
        }
    }
    for _ in 0..20_000 {
        let pair = match g.below(8) {
            // arbitrary bit patterns
            0 => (g.next(), g.next()),
            // finite normals of similar scale
            1 | 2 => {
                let ey = 1 + g.below(254);
                let ex = (ey + g.below(20)).min(254);
                let x = ((g.next() & 0x807f_ffff) | (ex << 23)) as u32;
                let y = ((g.next() & 0x807f_ffff) | (ey << 23)) as u32;
                (x, y)
            }
            // exact integer multiple of y (hits hz == 0 branch)
            3 => {
                let ey = 1 + g.below(200);
                let y = f32::from_bits((g.next() & 0x807f_ffff) | (ey << 23));
                let mult = (1 + g.below(64)) as f32 * if g.below(2) == 0 { 1.0 } else { -1.0 };
                ((y * mult).to_bits(), y.to_bits())
            }
            // subnormal x or y, or remainder underflowing into subnormal
            4 => {
                let x = (g.next() & 0x80ff_ffff) as u32;
                let y = (g.next() & 0x80ff_ffff) as u32;
                (x, y)
            }
            // huge exponent gap (exercises long-division loop heavily)
            5 => {
                let x = (g.next() & 0x807f_ffff) | ((200 + g.below(55)) << 23);
                let y = (g.next() & 0x807f_ffff) | ((1 + g.below(30)) << 23);
                (x, y)
            }
            // |x| == |y| or |x| slightly above |y|
            6 => {
                let y = (g.next() & 0x807f_ffff) | ((1 + g.below(254)) << 23);
                let delta = g.below(4);
                let x = (y & 0x7fff_ffff).saturating_add(delta) | (g.next() & 0x8000_0000);
                (x, y)
            }
            // realistic song ms / beat ms values
            _ => {
                let beat = (100.0 + (g.below(10_000) as f32) * 0.1).to_bits();
                let pos = ((g.below(600_000) as f32) * 0.5).to_bits();
                (pos, beat)
            }
        };
        fmod_pairs.push(pair);
    }

    let (mut fmod_dom, mut fmod_sub_rem) = (0u32, 0u32);
    for (idx, &(xb, yb)) in fmod_pairs.iter().enumerate() {
        let (x, y) = (f32::from_bits(xb), f32::from_bits(yb));
        // __ieee754_fmodf
        r.cpu.fpr[12] = xb;
        r.cpu.fpr[13] = yb;
        r.call(IEEE754_FMODF).unwrap_or_else(|e| panic!("ieee754_fmodf[{idx}] ({xb:#010x}, {yb:#010x}): {e}"));
        let ours_raw = libc::ieee754_fmodf(x, y).to_bits();
        assert_eq!(r.cpu.fpr[0], ours_raw, "ieee754_fmodf[{idx}] ({xb:#010x}, {yb:#010x}): theirs {:#010x} ours {ours_raw:#010x}", r.cpu.fpr[0]);

        // fmodf wrapper + errno check
        r.mem.write_u32(reent, 0);
        r.cpu.fpr[12] = xb;
        r.cpu.fpr[13] = yb;
        r.call(FMODF).unwrap_or_else(|e| panic!("fmodf[{idx}] ({xb:#010x}, {yb:#010x}): {e}"));
        let ours = libc::fmodf(x, y).to_bits();
        assert_eq!(r.cpu.fpr[0], ours, "fmodf[{idx}] ({xb:#010x}, {yb:#010x}): theirs {:#010x} ours {ours:#010x}", r.cpu.fpr[0]);
        let expect_dom = !libc::isnanf(x) && !libc::isnanf(y) && tricky_game::ps2f::F(y).ee() == 0.0;
        let got_errno = r.mem.read_u32(reent);
        assert_eq!(got_errno, if expect_dom { 0x21 } else { 0 }, "fmodf[{idx}] ({xb:#010x}, {yb:#010x}) errno");
        fmod_dom += expect_dom as u32;
        fmod_sub_rem += ((ours & 0x7fff_ffff) != 0 && (ours & 0x7f80_0000) == 0) as u32;
    }

    // 3. __ieee754_logf 0x2f2de0 and logf 0x2f2a48: edges + power-of-two / threshold boundaries + 20,000 random
    let mut log_vals: Vec<u32> = EDGES.to_vec();
    for exp in 1u32..255 {
        let base = exp << 23;
        // exact powers of two and +/- 1..20 ulps (hits the (ix + 0xf) & 0x7fffff < 0x10 near-1 fast path)
        for d in 0u32..=20 {
            log_vals.push(base.wrapping_add(d));
            if base > d {
                log_vals.push(base.wrapping_sub(d));
            }
        }
        // threshold boundaries for i wrap (0x3504e0) and v1 (0x30a3d0, 0x35c288)
        for center in [0x0030_a3d0u32, 0x0035_04e0, 0x0035_c288] {
            for d in -2i32..=2 {
                log_vals.push(base | center.wrapping_add_signed(d));
            }
        }
    }
    for _ in 0..20_000 {
        let b = match g.below(5) {
            0 => g.next(),
            1 | 2 => (1 + g.below(254)) << 23 | (g.next() & 0x007f_ffff),
            3 => 0x3f80_0000u32.wrapping_add_signed((g.below(65) as i32) - 32),
            _ => ((1 + g.below(254)) << 23) | (0x0030_a300 + g.below(0x0005_2100)),
        };
        log_vals.push(b);
    }

    let (mut log_dom, mut log_fast_zero, mut log_fast_nz, mut log_hfsq, mut log_nohfsq) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for (idx, &xb) in log_vals.iter().enumerate() {
        let x = f32::from_bits(xb);
        // __ieee754_logf
        r.cpu.fpr[12] = xb;
        r.call(IEEE754_LOGF).unwrap_or_else(|e| panic!("ieee754_logf[{idx}] ({xb:#010x}): {e}"));
        let ours_raw = libc::ieee754_logf(x).to_bits();
        assert_eq!(r.cpu.fpr[0], ours_raw, "ieee754_logf[{idx}] ({xb:#010x}): theirs {:#010x} ours {ours_raw:#010x}", r.cpu.fpr[0]);

        // logf wrapper + errno check
        r.mem.write_u32(reent, 0);
        r.cpu.fpr[12] = xb;
        r.call(LOGF).unwrap_or_else(|e| panic!("logf[{idx}] ({xb:#010x}): {e}"));
        let ours = libc::logf(x).to_bits();
        assert_eq!(r.cpu.fpr[0], ours, "logf[{idx}] ({xb:#010x}): theirs {:#010x} ours {ours:#010x}", r.cpu.fpr[0]);
        let expect_dom = !libc::isnanf(x) && !(0.0 < tricky_game::ps2f::F(x).ee());
        let got_errno = r.mem.read_u32(reent);
        assert_eq!(got_errno, if expect_dom { 0x21 } else { 0 }, "logf[{idx}] ({xb:#010x}) errno");
        log_dom += expect_dom as u32;
        if (xb as i32) >= 0x0080_0000 && (xb as i32) < 0x7f80_0000 {
            let ix = (xb & 0x007f_ffff) as i32;
            if (ix.wrapping_add(0x0f) & 0x007f_ffff) < 0x10 {
                if ix == 0 { log_fast_zero += 1; } else { log_fast_nz += 1; }
            } else if (ix.wrapping_sub(0x0030_a3d0) | 0x0035_c288i32.wrapping_sub(ix)) > 0 {
                log_hfsq += 1;
            } else {
                log_nohfsq += 1;
            }
        }
    }

    println!(
        "fmodf: {} pairs ({fmod_dom} domain, {fmod_sub_rem} subnormal rem); logf: {} values ({log_dom} domain/sing, {log_fast_zero} exact pow2, {log_fast_nz} near pow2, {log_hfsq} hfsq, {log_nohfsq} non-hfsq)",
        fmod_pairs.len(),
        log_vals.len(),
    );
    assert!(fmod_dom > 50 && fmod_sub_rem > 50);
    assert!(log_dom > 500 && log_fast_zero > 200 && log_fast_nz > 500 && log_hfsq > 500 && log_nohfsq > 500);
}
