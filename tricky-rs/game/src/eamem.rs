//! The memory library's fill, copy and move (board row E2i), on the game's bytes ([`Mem`]): `EAMem_Set`
//! 0x2cc808, `EAMem_Copy` 0x2ce114, `Mem_Move` 0x2ce228. Their stores come in the original's sizes and order, so
//! overlapping copies and fills that start off a word boundary come out as the game's do. (The checksum the
//! same library has, `Crc_Compute16` 0x2ce04c, is `career::crc16`.) Checked in the function runner
//! (`tests/eamem.rs`).

use crate::pf::Mem;

/// `EAMem_Set` 0x2cc808(p, v, n): `n` bytes (none for `n` ≤ 0) from `p` filled from the word `v`: a byte (v's
/// low byte) to reach a 2-byte boundary, a half (v's low half) to reach 4, a word to reach 8, then 64 bytes at a
/// time of `v` twice, 16 at a time, words, a half, a byte. Each step only when enough is left (1, 2, 4, 64, 16, 4,
/// 2, 1 bytes), so a short fill at an odd place does not follow the word's byte pattern.
pub fn set(m: &mut Mem, p: u32, v: u32, n: u32) {
    let mut p = p;
    let mut n = n as i32;
    if n > 0 && p & 1 != 0 {
        m.w8(p, v as u8);
        n -= 1;
        p = p.wrapping_add(1);
    }
    if n > 1 && p & 2 != 0 {
        m.w16(p, v as u16);
        n -= 2;
        p = p.wrapping_add(2);
    }
    if n >= 4 && p & 4 != 0 {
        m.w32(p, v);
        n -= 4;
        p = p.wrapping_add(4);
    }
    if n >= 0x40 {
        loop {
            for k in 0..16 {
                m.w32(p.wrapping_add(4 * k), v);
            }
            n -= 0x40;
            p = p.wrapping_add(0x40);
            if n <= 0x3f {
                break;
            }
        }
    }
    while n > 0xf {
        for k in 0..4 {
            m.w32(p.wrapping_add(4 * k), v);
        }
        n -= 0x10;
        p = p.wrapping_add(0x10);
    }
    while n >= 4 {
        m.w32(p, v);
        n -= 4;
        p = p.wrapping_add(4);
    }
    if n > 1 {
        m.w16(p, v as u16);
        n -= 2;
        p = p.wrapping_add(2);
    }
    if n > 0 {
        m.w8(p, v as u8);
    }
}

/// `n` bytes from `s` read, then written to `d`
fn chunk(m: &mut Mem, d: u32, s: u32, n: u32) {
    let b: Vec<u8> = (0..n).map(|k| m.r8(s.wrapping_add(k))).collect();
    for (k, x) in b.into_iter().enumerate() {
        m.w8(d.wrapping_add(k as u32), x);
    }
}

/// `EAMem_Copy` 0x2ce114(d, s, n): `n` bytes from `s` to `d`, forward. Under 18 bytes, byte by byte. Else bytes
/// until `d` is on a 16-byte boundary (stopping, the byte read but not written, should the count reach 0); then,
/// when `s` is on one too, 128 bytes at a time (each 64 read, then written), 8 at a time (read, then written), then
/// bytes; else bytes for the rest.
pub fn copy(m: &mut Mem, d: u32, s: u32, n: u32) {
    let (mut d, mut s, mut n) = (d, s, n as i32);
    if n >= 0x12 {
        while d & 0xf != 0 {
            n -= 1;
            if n <= 0 {
                return;
            }
            let b = m.r8(s);
            s = s.wrapping_add(1);
            m.w8(d, b);
            d = d.wrapping_add(1);
        }
        if s & 0xf == 0 {
            n -= 0x80;
            while n >= 0 {
                chunk(m, d, s, 0x40);
                chunk(m, d.wrapping_add(0x40), s.wrapping_add(0x40), 0x40);
                n -= 0x80;
                s = s.wrapping_add(0x80);
                d = d.wrapping_add(0x80);
            }
            n += 0x78;
            while n >= 0 {
                chunk(m, d, s, 8);
                s = s.wrapping_add(8);
                n -= 8;
                d = d.wrapping_add(8);
            }
            n += 8;
        }
    }
    n -= 1;
    while n >= 0 {
        n -= 1;
        let b = m.r8(s);
        s = s.wrapping_add(1);
        m.w8(d, b);
        d = d.wrapping_add(1);
    }
}

/// `Mem_Move` 0x2ce228(d, s, n): when `d` lies inside `s`'s bytes (s < d < s + n, as unsigned addresses), copied
/// backward a byte at a time (`n` bytes; none for 0); else `EAMem_Copy`
pub fn mem_move(m: &mut Mem, d: u32, s: u32, n: u32) {
    let end = s.wrapping_add(n);
    if s < d && d < end {
        let (mut d, mut s, mut n) = (d.wrapping_add(n), end, n);
        loop {
            n = n.wrapping_sub(1);
            if n == u32::MAX {
                break;
            }
            s = s.wrapping_sub(1);
            d = d.wrapping_sub(1);
            let b = m.r8(s);
            m.w8(d, b);
        }
    } else {
        copy(m, d, s, n);
    }
}
