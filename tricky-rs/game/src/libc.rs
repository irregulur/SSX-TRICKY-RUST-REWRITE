//! The C library's string and memory functions on the game's bytes ([`Mem`]), board row E2l. The game's are
//! newlib's, rewritten for the Emotion Engine: when both pointers are 8- or 16-byte aligned they go a doubleword or
//! a quadword at a time while no 0 byte is in the block.
//!
//! Those blocks change nothing that can be seen, so they are byte loops here: the comparisons and `strlen` only
//! skip blocks that are equal or hold no 0; a copy reads a block whole and then writes it, and as both pointers
//! are aligned to the block they are a whole number of blocks apart, so even a copy onto its own source a block
//! ahead leaves what a byte loop leaves; `memset` stores the same byte everywhere. Checked in the function runner
//! (`tests/libc.rs`).

use crate::pf::Mem;

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
