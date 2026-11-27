//! EA's text library (`lib-text`, board row E5f) on the game's bytes: 16-bit strings and the wide `printf`.
//!
//! The formatter `WStr_VFormat` 0x2c48e8(dst, fmt, args) reads `%` [flags `-` `+` ` ` `#` `0`] [width: `*` or
//! digits from 1-9] [`.` precision: `*` or digits from 1-9] conversion, arguments in 8-byte slots (`WStr_Sprintf`
//! spills its registers so). Every number goes through a double (`__floatsidf` 0x2f6500, then `__fixdfsi` /
//! `__fixunsdfsi` back: so `%u` `%x` `%o` `%p` print 0 for 0x80000000 and up, read as negative), and only `%f` formats a fraction: `%e` / `%E` / `%g` / `%G` print the double's integer
//! part as `%d` does. The precision only reaches `%f` (default 6); a precision starting with `0` is not one: the
//! `0` is then taken as the conversion and printed as it is, so `%.0f` prints `0f`. Conversions: `c` (16-bit),
//! `C` (8-bit), `d` `i` `o` `u` `x` `X`, `p` / `P` (hexadecimal, 8 wide, zero-padded, whatever the width), `e` `E`
//! `f` `g` `G`, `n` (the count so far stored), `s` (16-bit string) and `S` (8-bit), `(null)` for a null one;
//! anything else is printed as it is (`%%` so prints `%`). The number is built backwards in a 127-character
//! buffer: zeros up to the width before the sign with `0` (so the sign comes on top), then the sign (`-`; `+`
//! with `+`, and a space for 0 with `+`; a space with ` ` alone), `0` / `0x` with `#` in octal / hexadecimal, then
//! padding up to the width (spaces, or zeros with `0`), or spaces after it with `-`.

use crate::pf::Mem;

/// The digits the formatter writes with: lower case 0x3b8ed0, upper case 0x3b8ee8 (copied to the stack).
const DIGITS_LOWER: &[u8; 16] = b"0123456789abcdef";
const DIGITS_UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// `WStr_Len` 0x2c52f8: the characters before the 0
pub fn len(m: &Mem, s: u32) -> u32 {
    let mut n = 0u32;
    while m.r16(s.wrapping_add(2 * n)) != 0 {
        n += 1;
    }
    n
}

/// `WStr_NCmp` 0x2c5438(a, b, n): the difference of the first differing characters within `n`, stopping at
/// `a`'s end
pub fn ncmp(m: &Mem, a: u32, b: u32, n: i32) -> i32 {
    let at = |s: u32, i: i32| m.r16(s.wrapping_add(2 * i as u32)) as i32;
    let mut r = 0;
    if n > 0 {
        r = at(a, 0) - at(b, 0);
        if at(a, 0) == 0 {
            return r;
        }
        let mut i = 0;
        loop {
            i += 1;
            if i >= n || r != 0 {
                return r;
            }
            r = at(a, i) - at(b, i);
            if at(a, i) == 0 {
                return r;
            }
        }
    }
    r
}

/// `WStr_Cmp` 0x2c5418: [`ncmp`] with no limit (0x7fffffff)
pub fn cmp(m: &Mem, a: u32, b: u32) -> i32 {
    ncmp(m, a, b, 0x7fff_ffff)
}

/// `WStr_Find` 0x2c5380(s, sub): the first index where `sub` starts in `s`, or −1 (always −1 in an empty `s`)
pub fn find(m: &Mem, s: u32, sub: u32) -> i32 {
    let n = len(m, sub) as i32;
    if m.r16(s) == 0 {
        return -1;
    }
    let mut i = 0u32;
    loop {
        if ncmp(m, s.wrapping_add(2 * i), sub, n) == 0 {
            return i as i32;
        }
        i += 1;
        if m.r16(s.wrapping_add(2 * i)) == 0 {
            return -1;
        }
    }
}

/// `WStr_NCopy` 0x2c54b8(d, s, n): at most `n` characters, then a 0
pub fn ncopy(m: &mut Mem, d: u32, s: u32, n: i32) {
    let mut i = 0i32;
    if n > 0 && m.r16(s) != 0 {
        loop {
            let c = m.r16(s.wrapping_add(2 * i as u32));
            m.w16(d.wrapping_add(2 * i as u32), c);
            i += 1;
            if n <= i || m.r16(s.wrapping_add(2 * i as u32)) == 0 {
                break;
            }
        }
    }
    m.w16(d.wrapping_add(2 * i as u32), 0);
}

/// `WStr_CopyAll` 0x2c5498: [`ncopy`] with no limit
pub fn copy_all(m: &mut Mem, d: u32, s: u32) {
    ncopy(m, d, s, 0x7fff_ffff)
}

/// `WStr_FromAscii` 0x2c5510(d, s): each byte widened, then a 0
pub fn from_ascii(m: &mut Mem, d: u32, s: u32) {
    let mut i = 0u32;
    loop {
        let c = m.r8(s.wrapping_add(i));
        if c == 0 {
            break;
        }
        m.w16(d.wrapping_add(2 * i), c as u16);
        i += 1;
    }
    m.w16(d.wrapping_add(2 * i), 0);
}

/// `WStr_FormatArgs` 0x2c5578(d, fmt, a, b): `fmt` with `%1` replaced by `a` and `%2` by `b`; any other `%` kept
/// (`WStr_FormatThunk` 0x2c5558 passes no `b`: 0, which a `%2` would read as a string at 0)
pub fn format_args(m: &mut Mem, mut d: u32, mut f: u32, a: u32, b: u32) {
    loop {
        let c = m.r16(f);
        if c == 0 {
            m.w16(d, 0);
            return;
        }
        let next = m.r16(f + 2);
        if c == b'%' as u16 && (next == b'1' as u16 || next == b'2' as u16) {
            let s = if next == b'1' as u16 { a } else { b };
            copy_all(m, d, s);
            f += 4;
            d = d.wrapping_add(2 * len(m, s));
        } else {
            m.w16(d, c);
            d = d.wrapping_add(2);
            f += 2;
        }
    }
}

/// The C library's `fcvt` 0x2fa330 (`fcvtbuf` 0x303658 with its own buffer): between −1 and 1 (exclusive)
/// `_dtoa_r` mode 2, `ndigit` significant digits; otherwise mode 3, `ndigit` digits after the point; trailing
/// zeros dropped, then zeros added until the string is `ndigit` long. Rounding is to nearest, ties to even, on
/// the exact value. Returns the digits, the point's position and the sign. (0 gives "0" with the point at 1.)
pub fn fcvt(x: f64, ndigit: i32) -> (Vec<u8>, i32, bool) {
    let neg = x.is_sign_negative();
    let a = x.abs();
    let (mut d, decpt) = if a == 0.0 {
        (vec![b'0'], 1)
    } else if a < 1.0 {
        let n = ndigit.max(1) as usize;
        let s = format!("{a:.prec$e}", prec = n - 1);
        let (mant, exp) = s.split_once('e').unwrap();
        let d: Vec<u8> = mant.bytes().filter(|&c| c != b'.').collect();
        (d, exp.parse::<i32>().unwrap() + 1)
    } else {
        let s = format!("{a:.p$}", p = ndigit.max(0) as usize);
        let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
        (int.bytes().chain(frac.bytes()).collect(), int.len() as i32)
    };
    while d.len() > 1 && d.last() == Some(&b'0') {
        d.pop();
    }
    while (d.len() as i32) < ndigit {
        d.push(b'0');
    }
    (d, decpt, neg)
}

/// A number being built backwards from a 0, as on the formatter's stack.
struct Back {
    buf: Vec<u16>,
}

impl Back {
    fn push(&mut self, c: u16) {
        self.buf.push(c);
    }
    /// `WStr_Len` from the front (the last pushed) to the first 0 character
    fn len(&self) -> i32 {
        self.buf.iter().rev().take_while(|&&c| c != 0).count() as i32
    }
}

/// `Fmt_WriteDigits` 0x2c4478(p, v, base, digits): `v`'s digits written backwards (one for 0)
fn write_digits(b: &mut Back, mut v: u32, base: u32, digits: &[u8; 16]) {
    if v == 0 {
        b.push(digits[0] as u16);
        return;
    }
    while v != 0 {
        b.push(digits[(v % base) as usize] as u16);
        v /= base;
    }
}

/// `Fmt_WriteAbsDigits` 0x2c44d8: the same for |v|
fn write_abs_digits(b: &mut Back, v: i32, base: u32, digits: &[u8; 16]) {
    write_digits(b, if v > -1 { v as u32 } else { v.wrapping_neg() as u32 }, base, digits)
}

/// `Fmt_WriteFixed` 0x2c4508(p, x, prec): `fcvt`'s digits and the point backwards, `prec` digits after it (the
/// sign left to the caller). It writes the point + decpt + prec characters, so with no leading 0: 0.5 prints
/// `.500000`, and below 0.1 the point never comes and the first character is a 0 (the byte before the digits),
/// which ends the number for the padding and the copy: 0.05 prints as nothing (or the width's padding). Smaller
/// still (point + prec < −1: 0.005 at 0 places, under 1e-7 at 6) it never ends.
fn write_fixed(b: &mut Back, x: f64, prec: i32) {
    let (d, decpt, _) = fcvt(x, prec);
    let n = decpt + prec;
    // the count it writes to is never reached: it writes on down the stack until it faults
    assert!(n >= -1, "Fmt_WriteFixed({x}, {prec}): the game never returns");
    let (mut i, mut j) = (0i32, 0i32);
    if n != -1 {
        loop {
            if i == prec {
                b.push(b'.' as u16);
            } else {
                let at = n - j;
                // below 0.1 the point is never reached and the last digit read is the byte before `fcvt`'s
                // buffer: the top byte of its newlib heap chunk's size word, 0 (inferred from the chunk layout)
                let c = if (d.len() as i32) < at {
                    b'0'
                } else if at >= 1 {
                    d[(at - 1) as usize]
                } else {
                    0
                };
                b.push(c as u16);
                j += 1;
            }
            i += 1;
            if i == n + 1 {
                break;
            }
        }
    }
}

/// `__fixdfsi` 0x2f65b8: toward zero, saturating
fn fixdfsi(x: f64) -> i32 {
    x as i32
}

/// `__fixunsdfsi` 0x2f6650: toward zero, 0 for anything negative, saturating at 0xffffffff. The formatter's
/// unsigned conversions get their int through `__floatsidf`, so 0x80000000 and up print as 0.
fn fixunsdfsi(x: f64) -> u32 {
    x as u32
}

/// `Fmt_WriteNumber` 0x2c45e0(&dst, x, base, flags, width, prec, digits): the number at `*dst`, which moves past
/// it; returns the characters written. Flags: 1 `+`, 2 `#`, 4 ` `, 8 `0`, 0x10 `-`, 0x20 unsigned, 0x40 fixed
/// (0x80 / 0x100 within it the same).
#[allow(clippy::too_many_arguments)]
fn write_number(m: &mut Mem, dst: &mut u32, x: f64, base: u32, flags: u32, width: i32, prec: i32, digits: &[u8; 16]) -> i32 {
    let mut b = Back { buf: vec![] };
    if flags & 0x40 != 0 {
        write_fixed(&mut b, x, prec);
    } else if flags & 0x20 != 0 {
        write_digits(&mut b, fixunsdfsi(x), base, digits);
    } else {
        write_abs_digits(&mut b, fixdfsi(x), base, digits);
    }
    if flags & 0x18 == 8 {
        while b.len() < width {
            b.push(b'0' as u16);
        }
    }
    if base == 10 && flags & 0x20 == 0 {
        if x < 0.0 {
            b.push(b'-' as u16);
        }
        if x > 0.0 && flags & 1 != 0 {
            b.push(b'+' as u16);
        }
        if x == 0.0 && flags & 1 != 0 {
            b.push(b' ' as u16);
        }
        if x >= 0.0 && flags & 5 == 4 {
            b.push(b' ' as u16);
        }
    }
    if base == 8 && flags & 2 != 0 {
        b.push(b'0' as u16);
    }
    if base == 0x10 && flags & 2 != 0 {
        b.push(b'x' as u16);
        b.push(b'0' as u16);
    }
    if flags & 0x10 == 0 {
        let pad = if flags & 8 != 0 { b'0' } else { b' ' } as u16;
        while b.len() < width {
            b.push(pad);
        }
    }
    // `WStr_CopyAll`: up to the first 0 character
    let mut n = b.len();
    for (k, &c) in b.buf.iter().rev().take(n as usize).enumerate() {
        m.w16(dst.wrapping_add(2 * k as u32), c);
    }
    let mut p = dst.wrapping_add(2 * n as u32);
    m.w16(p, 0);
    if flags & 0x10 != 0 {
        while n < width {
            m.w16(p, b' ' as u16);
            p = p.wrapping_add(2);
            n += 1;
        }
    }
    *dst = p;
    n
}

/// `WStr_VFormat` 0x2c48e8(dst, fmt, args): see the module. Returns the address past the ending 0.
pub fn vformat(m: &mut Mem, dst: u32, fmt: u32, args: u32) -> u32 {
    let mut d = dst;
    let mut f = fmt;
    let mut va = args;
    let mut count = 0i32;
    let mut arg = |m: &Mem| {
        let a = va;
        va = va.wrapping_add(8);
        (m.r32(a), m.r32(a.wrapping_add(4)))
    };
    let mut c = m.r16(f);
    f = f.wrapping_add(2);
    loop {
        if c == 0 {
            m.w16(d, 0);
            return d.wrapping_add(2);
        }
        if c != b'%' as u16 {
            count += 1;
            m.w16(d, c);
            d = d.wrapping_add(2);
            c = m.r16(f);
            f = f.wrapping_add(2);
            continue;
        }
        let rd = |m: &Mem, f: &mut u32| {
            let c = m.r16(*f) as u32;
            *f = f.wrapping_add(2);
            c
        };
        let mut s = rd(m, &mut f);
        let (mut flags, mut width, mut prec) = (0u32, 0i32, 6i32);
        loop {
            let bit = match s {
                0x20 => 4,
                0x23 => 2,
                0x2b => 1,
                0x2d => 0x10,
                0x30 => 8,
                _ => break,
            };
            flags |= bit;
            s = rd(m, &mut f);
        }
        if s == b'*' as u32 {
            width = arg(m).0 as i32;
            s = rd(m, &mut f);
        } else if (0x31..0x3a).contains(&s) {
            width = 0;
            while s.wrapping_sub(0x30) < 10 {
                width = width.wrapping_mul(10).wrapping_add(s as i32 - 0x30);
                s = rd(m, &mut f);
            }
        }
        if s == b'.' as u32 {
            s = rd(m, &mut f);
            if s == b'*' as u32 {
                prec = arg(m).0 as i32;
                s = rd(m, &mut f);
            } else if (0x31..0x3a).contains(&s) {
                prec = 0;
                while s.wrapping_sub(0x30) < 10 {
                    prec = prec.wrapping_mul(10).wrapping_add(s as i32 - 0x30);
                    s = rd(m, &mut f);
                }
            }
        }
        let int = |v: u32| v as i32 as f64;
        let ch = s as u8;
        match ch {
            b'c' | b'C' if s < 0x100 => {
                let v = arg(m).0;
                m.w16(d, if ch == b'c' { v as u16 } else { v as u8 as u16 });
                d = d.wrapping_add(2);
                count += 1;
            }
            b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'p' | b'P' if s < 0x100 => {
                let v = int(arg(m).0);
                let (base, fl, digits, w) = match ch {
                    b'd' | b'i' => (10, flags, DIGITS_LOWER, width),
                    b'o' => (8, flags | 0x20, DIGITS_LOWER, width),
                    b'u' => (10, flags | 0x20, DIGITS_LOWER, width),
                    b'x' => (16, flags | 0x20, DIGITS_LOWER, width),
                    b'X' => (16, flags | 0x20, DIGITS_UPPER, width),
                    b'p' => (16, flags | 0x28, DIGITS_LOWER, 8),
                    _ => (16, flags | 0x28, DIGITS_UPPER, 8),
                };
                count += write_number(m, &mut d, v, base, fl, w, prec, digits);
            }
            b'e' | b'E' | b'g' | b'G' | b'f' if s < 0x100 => {
                let (lo, hi) = arg(m);
                let v = f64::from_bits((hi as u64) << 32 | lo as u64);
                let bit = match ch {
                    b'e' | b'E' => 0x80,
                    b'f' => 0x40,
                    _ => 0x100,
                };
                count += write_number(m, &mut d, v, 10, flags | bit, width, prec, DIGITS_LOWER);
            }
            b'n' if s < 0x100 => {
                let p = arg(m).0;
                m.w32(p, count as u32);
            }
            b's' | b'S' if s < 0x100 => {
                let p = arg(m).0;
                if p == 0 {
                    for &k in b"(null)" {
                        m.w16(d, k as u16);
                        d = d.wrapping_add(2);
                    }
                    count += 6;
                } else {
                    let mut q = p;
                    loop {
                        let k = if ch == b's' { m.r16(q) } else { m.r8(q) as u16 };
                        if k == 0 {
                            break;
                        }
                        m.w16(d, k);
                        d = d.wrapping_add(2);
                        count += 1;
                        q = q.wrapping_add(if ch == b's' { 2 } else { 1 });
                    }
                }
            }
            _ => {
                m.w16(d, s as u16);
                d = d.wrapping_add(2);
                count += 1;
            }
        }
        c = rd(m, &mut f) as u16;
    }
}

/// `WStr_Sprintf` 0x2c4e90(dst, fmt, ...): [`vformat`] over the arguments, spilled to 8-byte slots at `slots`
/// (the six register arguments `a2`..`t3`; the caller's stack after them)
pub fn sprintf(m: &mut Mem, dst: u32, fmt: u32, slots: u32) -> u32 {
    vformat(m, dst, fmt, slots)
}
