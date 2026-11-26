//! The memory library on the game's bytes ([`Mem`]). Its fill, copy and move (board row E2i): `EAMem_Set`
//! 0x2cc808, `EAMem_Copy` 0x2ce114, `Mem_Move` 0x2ce228. Their stores come in the original's sizes and order, so
//! overlapping copies and fills that start off a word boundary come out as the game's do. (The checksum the
//! same library has, `Crc_Compute16` 0x2ce04c, is `career::crc16`.) Checked in the function runner
//! (`tests/eamem.rs`).
//!
//! Its heaps (row E2j): `EAMem_InitHeap`, `EAMem_AllocEx` and its wrappers, `EAMem_Free`, `EAMem_Resize` and the
//! helpers they share, below [`HEAPS`]; checked in the runner (`tests/eamem_heap.rs`).
//!
//! The rest (row E2n): making the main heap, destroying heaps, freeing a whole heap, the heap checks, and the
//! compressor's two wrappers, below [`MAIN_HEAP_BASE`]; checked in the runner (`tests/eamem_rest.rs`).

use crate::libc::strlen;
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

// ---- The heaps (row E2j) ----
//
// A heap: a header block at its base whose data is the heap's record (0x44 bytes), then blocks to a last header
// block at its top. Every block starts with a 16-byte header: +0 a tag (0x424d when made, 0x4246 free, 0x424f
// taken out of the free list, 0x4253 the two ends), +2 flags (the heap's number in the low 4 bits, 0x4000 free,
// 0x8000 an end, 0x100 the block carries its name, 0x200 "BEND" after the data, 0x800 four zero bytes 12 past the
// data), +4 the data's size (a free block: its whole span, header included), +8 the next block, +0xc the one before.
// A free block also has +0x10 / +0x14, the next and previous free block, in a list kept in address order through a
// sentinel in the heap's record. The record (at base + 0x10): +0 the name, +8 the base, +0xc the top block, +0x10
// the sentinel (tag 0x4253, size 0x7fffffff, +0x20 / +0x24 the first and last free block), +0x28 the alignment,
// +0x2c the large alignment, +0x30 the bytes kept after each block's data, +0x34 flags, +0x38 the lock, +0x3c a
// word the caller gives.

/// `gEAMem_Heaps` 0x34eff8: the 16 heaps' records, by number
pub const HEAPS: u32 = 0x34eff8;
/// `gEAMem_OutOfMemoryFn` 0x3bb990 (gp − 0x7f60): the function `EAMem_AllocEx` calls when nothing fits, or 0
pub const OUT_OF_MEMORY: u32 = 0x3bb990;

const TAG_MADE: u16 = 0x424d;
const TAG_FREE: u16 = 0x4246;
const TAG_TAKEN: u16 = 0x424f;
const TAG_END: u16 = 0x4253;
const FREE: u16 = 0x4000;

fn heap_of(m: &Mem, flags: u32) -> u32 {
    m.r32(HEAPS + 4 * (flags & 0xf))
}


fn cstr(m: &Mem, s: u32) -> Vec<u8> {
    (0..strlen(m, s)).map(|k| m.r8(s.wrapping_add(k))).collect()
}

/// A string's bytes and its 0 written from `d`
fn put_str(m: &mut Mem, d: u32, s: &[u8]) {
    for (k, b) in s.iter().chain(&[0]).enumerate() {
        m.w8(d.wrapping_add(k as u32), *b);
    }
}

/// `EAMem_InitBlock` 0x2cd168(b, name, size, after, flags, before, next): a block header (tag 0x424d), "BEND"
/// after the data with flag 0x200, four zero bytes 12 past the data with 0x800, and the name (when given and with
/// 0x100) `after` bytes past the data. Returns the bytes from the header to past the name.
#[allow(clippy::too_many_arguments)]
pub fn init_block(m: &mut Mem, b: u32, name: Option<&[u8]>, size: u32, after: u32, flags: u32, before: u32, next: u32) -> u32 {
    let end = b.wrapping_add(size).wrapping_add(0x10);
    m.w16(b, TAG_MADE);
    m.w32(b + 8, next);
    m.w32(b + 0xc, before);
    m.w16(b + 2, flags as u16);
    m.w32(b + 4, size);
    if flags & 0x200 != 0 {
        for (k, c) in b"BEND".iter().enumerate() {
            m.w8(end.wrapping_add(k as u32), *c);
        }
    }
    let mut q = end.wrapping_add(after);
    if flags & 0x800 != 0 {
        for k in 0..4 {
            m.w8(end.wrapping_add(0xc + k), 0);
        }
    }
    if let Some(name) = name {
        if flags & 0x100 != 0 {
            put_str(m, q, name);
            q = q.wrapping_add(strlen(m, q) + 1);
        }
    }
    q.wrapping_sub(b)
}

/// The free block `b` put into its heap's free list (record `h`) in address order, its size made its whole span
/// (to the next block), tagged free. Found from whichever end is nearer the middle of the first and last free
/// block. (Written out in `EAMem_InitHeap`, `EAMem_AllocEx`, `EAMem_Free` and `EAMem_Resize`.)
fn free_insert(m: &mut Mem, h: u32, b: u32) {
    let s = h + 0x10;
    let size = m.r32(b + 8).wrapping_sub(b);
    let (first, last) = (m.r32(s + 0x10), m.r32(s + 0x14));
    let mid = first.wrapping_add(last.wrapping_sub(first) >> 1);
    let (before, after);
    if mid < b {
        let mut a = m.r32(s + 0x14);
        while b < a {
            a = m.r32(a + 0x14);
        }
        before = a;
        after = m.r32(a + 0x10);
    } else {
        let mut a = m.r32(s + 0x10);
        while a < b {
            a = m.r32(a + 0x10);
        }
        after = a;
        before = m.r32(a + 0x14);
    }
    m.w32(b + 4, size);
    m.w32(b + 0x10, after);
    m.w32(b + 0x14, before);
    m.w32(before + 0x10, b);
    m.w32(after + 0x14, b);
    m.w16(b, TAG_FREE);
    m.w16(b + 2, m.r16(b + 2) | FREE);
}

/// The free block `b` taken out of the free list, tagged 0x424f, its free flag cleared
fn unlink(m: &mut Mem, b: u32) {
    let (next, before) = (m.r32(b + 0x10), m.r32(b + 0x14));
    m.w32(before + 0x10, next);
    m.w32(next + 0x14, before);
    m.w16(b, TAG_TAKEN);
    m.w16(b + 2, m.r16(b + 2) & !FREE);
}

/// `EAMem_InitHeap` 0x2cd278: heap number `id` (its low 4 bits) made in `size` bytes at `base`, named `name` (an
/// address). `align`: where blocks start (a power of two); `big_align`: the first free block's alignment, and the
/// one `EAMem_AllocEx` uses with flag 0x40; `after`: bytes kept after each block's data; `bend`: "BEND" after every
/// block (flag 0x200); `names`: every block carries its name (0x100); `lock`: the lock, when the caller asked for
/// one (`RecMutex_Create` 0x2cf468 is our host's, so its handle is passed in); `word`: kept at +0x3c. The first
/// free block spans from past the record (aligned) to the top block (`after` + 0x30 short of the end). Its ends
/// are named "<name> LOW" and "<name> HIGH" with `names` (the game's `sprintf` 0x2fca28 with "%s LOW" / "%s HIGH",
/// 0x3bb998 / 0x3bb9a0). Returns the free block's size.
#[allow(clippy::too_many_arguments)]
pub fn init_heap(
    m: &mut Mem,
    id: u32,
    name: u32,
    base: u32,
    size: u32,
    align: u32,
    big_align: u32,
    after: u32,
    bend: bool,
    names: bool,
    lock: Option<u32>,
    word: u32,
) -> u32 {
    let mut flags = if bend { id | 0x200 } else { id };
    if names {
        flags |= 0x100;
    }
    let free = (base.wrapping_add(after + 0x54).wrapping_add(big_align + 0x2f) & big_align.wrapping_neg()).wrapping_sub(0x10);
    let top = base.wrapping_add(size).wrapping_sub(after + 0x30);
    let h = base + 0x10;
    let nm = cstr(m, name);
    let low = [&nm[..], b" LOW"].concat();
    init_block(m, base, Some(&low), 0x44, after, flags | 0x8000, 0, free);
    init_block(m, free, None, top.wrapping_sub(free).wrapping_sub(0x10), after, flags, base, top);
    let high = [&nm[..], b" HIGH"].concat();
    init_block(m, top, Some(&high), 0, after, flags | 0x8010, free, 0);
    m.w32(HEAPS + 4 * (id & 0xf), h);
    set(m, h, 0, 0x44);
    let nm = cstr(m, name);
    put_str(m, h, &nm);
    m.w32(h + 0xc, top);
    m.w32(h + 8, base);
    m.w16(base, TAG_END);
    let t = m.r32(h + 0xc);
    m.w16(t, TAG_END);
    m.w16(h + 0x10, TAG_END);
    m.w32(h + 0x14, 0x7fff_ffff);
    m.w32(h + 0x2c, big_align);
    m.w32(h + 0x30, after);
    m.w32(h + 0x34, flags);
    m.w32(h + 0x28, align);
    m.w32(h + 0x20, h + 0x10);
    m.w32(h + 0x3c, word);
    m.w32(h + 0x24, h + 0x10);
    m.w32(h + 0x38, 0);
    free_insert(m, h, free);
    if let Some(l) = lock {
        m.w32(h + 0x38, l);
    }
    m.r32(free + 4)
}

/// `EAMem_BlockHeaderSize` 0x2cc9a0(name, flags): the bytes the heap keeps after a block's data: its `after`, and
/// the name with its 0 when the heap names its blocks
pub fn block_header_size(m: &Mem, name: u32, flags: u32) -> u32 {
    let h = heap_of(m, flags);
    let n = if name != 0 && m.r32(h + 0x34) & 0x100 != 0 { strlen(m, name) + 1 } else { 0 };
    m.r32(h + 0x30).wrapping_add(n)
}

/// `EAMem_AllocEx` 0x2cca00(name, size, align, offset, flags): `size` bytes from heap `flags & 0xf`, placed so
/// that address + `offset` is a multiple of `align` (0: the heap's alignment, or its large one with flag 0x40);
/// 0 for a negative size. The free list is searched from its start (from its end with flag 0x10, the block then
/// taken from the free block's end), for the first block that fits (the largest with flag 0x20). What is left over
/// becomes a free block when more than the alignment + 0x2f. Nothing found: the function at [`OUT_OF_MEMORY`],
/// when there is one, is called (`out_of_memory(m, function, name, size, flags)`; the caller runs it) and the
/// search is tried again while it answers 1. Returns the data's address or 0. (The lock is our host's.)
/// `EAMem_AllocAligned` 0x2ccf50 is this (the word it adds in t1 is not read).
pub fn alloc_ex(
    m: &mut Mem,
    name: u32,
    size: u32,
    align: u32,
    offset: u32,
    flags: u32,
    out_of_memory: &mut dyn FnMut(&mut Mem, u32, u32, u32, u32) -> u32,
) -> u32 {
    let heap = heap_of(m, flags);
    if (size as i32) < 0 {
        return 0;
    }
    let s = heap + 0x10;
    loop {
        let al = m.r32(heap + 0x28).wrapping_sub(1);
        let mask = if align != 0 {
            align - 1
        } else if flags & 0x40 != 0 {
            m.r32(heap + 0x2c).wrapping_sub(1)
        } else {
            al
        };
        let mut need = size.wrapping_add(block_header_size(m, name, flags)) as i32;
        if need <= 7 {
            need = 8;
        }
        let lead = mask.wrapping_add(0x10);
        let span = need.wrapping_add(lead as i32);
        let back = flags & 0x10 != 0;
        let link = if back { 0x14 } else { 0x10 };
        let found = if flags & 0x20 != 0 {
            let mut least = span.wrapping_sub(1).max(0);
            let mut best = 0;
            let mut b = m.r32(s + link);
            loop {
                while least >= (m.r32(b + 4) as i32).wrapping_sub(0x10) {
                    b = m.r32(b + link);
                }
                if m.r16(b) == TAG_END {
                    break;
                }
                best = b;
                least = (m.r32(b + 4) as i32).wrapping_sub(0x10);
                b = m.r32(b + link);
            }
            best
        } else {
            let mut b = m.r32(s + link);
            while (m.r32(b + 4) as i32) < span {
                b = m.r32(b + link);
            }
            if m.r16(b) != TAG_END { b } else { 0 }
        };
        if found == 0 {
            let f = m.r32(OUT_OF_MEMORY);
            if f != 0 && out_of_memory(m, f, name, size, flags) == 1 {
                continue;
            }
            return 0;
        }
        let free_size = m.r32(found + 4);
        unlink(m, found);
        let next = m.r32(found + 8);
        let before = m.r32(found + 0xc);
        let at = if back { found.wrapping_add(free_size).wrapping_sub(need as u32) } else { found.wrapping_add(lead) };
        let data = (at.wrapping_add(offset) & !mask).wrapping_sub(offset);
        let left = (free_size as i32).wrapping_sub(span);
        let b = data.wrapping_sub(0x10);
        m.w32(b + 8, next);
        m.w32(b + 0xc, before);
        if (al.wrapping_add(0x30) as i32) < left {
            if back {
                let n = m.r32(found + 8);
                m.w32(n + 0xc, b);
                let n = m.r32(found + 8);
                m.w32(b + 0xc, found);
                m.w32(b + 8, n);
                let p = m.r32(found + 0xc);
                init_block(m, found, None, 0, 0, 0, p, b);
                free_insert(m, heap, found);
            } else {
                let f = data.wrapping_add(need as u32).wrapping_add(al) & !al;
                m.w32(next + 0xc, f);
                let p = m.r32(b + 0xc);
                m.w32(p + 8, b);
                let n = m.r32(b + 8);
                init_block(m, f, None, 0, 0, 0, b, n);
                free_insert(m, heap, f);
                m.w32(b + 8, f);
            }
        } else {
            m.w32(next + 0xc, b);
            let p = m.r32(b + 0xc);
            m.w32(p + 8, b);
        }
        let mut fl = flags | (m.r32(heap + 0x34) & 0x700);
        if name == 0 {
            fl &= !0x100;
        }
        let nm = (name != 0).then(|| cstr(m, name));
        let (p, n) = (m.r32(b + 0xc), m.r32(b + 8));
        init_block(m, b, nm.as_deref(), size, m.r32(heap + 0x30), fl, p, n);
        return data;
    }
}

/// `EAMem_Alloc` 0x2ccf70(name, size, flags) and `EAMem_Alloc0` 0x2ccf98 (the same; the word it passes differently
/// is not read): `EAMem_AllocEx` at the heap's own alignment
pub fn alloc(m: &mut Mem, name: u32, size: u32, flags: u32, out_of_memory: &mut dyn FnMut(&mut Mem, u32, u32, u32, u32) -> u32) -> u32 {
    alloc_ex(m, name, size, 0, 0, flags, out_of_memory)
}

/// `EAMem_Free` 0x2ccfc0(p): the block at `p` (none for 0) made free, joined with a free block before it and one
/// after it, and put into the free list. Returns 1.
pub fn free(m: &mut Mem, p: u32) -> u32 {
    if p != 0 {
        let heap = heap_of(m, m.r16(p - 0xe) as u32);
        let mut b = p - 0x10;
        let mut before = m.r32(b + 0xc);
        let mut next = m.r32(b + 8);
        if m.r16(before + 2) & FREE != 0 {
            unlink(m, before);
            b = before;
            before = m.r32(b + 0xc);
            m.w32(b + 8, next);
            m.w32(before + 8, b);
            m.w32(next + 0xc, b);
        }
        if m.r16(next + 2) & FREE != 0 {
            unlink(m, next);
            next = m.r32(next + 8);
            m.w32(b + 0xc, before);
            m.w32(b + 8, next);
            m.w32(next + 0xc, b);
        }
        free_insert(m, heap, b);
    }
    1
}

/// `EAMem_HeapHeaderSize` 0x2cd990(flags): the bytes heap `flags & 0xf` keeps after each block's data
pub fn heap_header_size(m: &Mem, flags: u32) -> u32 {
    m.r32(heap_of(m, flags) + 0x30)
}

/// `EAMem_GetSize` 0x2cd9b0(p): the block's data size
pub fn get_size(m: &Mem, p: u32) -> u32 {
    m.r32(p.wrapping_sub(0xc))
}

/// `EAMem_GetName` 0x2cd9b8(p): where the block's name is (past its data and the heap's kept bytes), or 0 when it
/// carries none
pub fn get_name(m: &Mem, p: u32) -> u32 {
    let flags = m.r16(p - 0xe) as u32;
    if flags & 0x100 == 0 {
        return 0;
    }
    let n = heap_header_size(m, flags);
    p.wrapping_add(m.r32(p - 0xc)).wrapping_add(n)
}

/// `EAMem_Resize` 0x2cda18(p, size): the block at `p` grown or shrunk in place to `size` bytes (−1: as far as it
/// can; other sizes under 8 but not negative: 8), joining a free block after it and, with more than 0x40 bytes
/// left over, giving them back as a free block. A size that does not fit is cut to the room there is. The bytes
/// after the data (the kept bytes and the name) move with its end. Returns `p`.
pub fn resize(m: &mut Mem, p: u32, size: u32) -> u32 {
    let b = p - 0x10;
    let flags = m.r16(b + 2) as u32;
    let heap = heap_of(m, flags);
    let mut next = m.r32(b + 8);
    if m.r16(next + 2) & FREE != 0 {
        unlink(m, next);
        next = m.r32(next + 8);
        m.w32(b + 8, next);
        m.w32(next + 0xc, b);
    }
    let mut size = size as i32;
    let mut want = size;
    if want < 8 {
        if want == -1 {
            want = 0x4000_0000;
        } else if want >= 0 {
            want = 8;
        }
    }
    let name = get_name(m, p);
    let tail = block_header_size(m, name, flags);
    let room = (next.wrapping_sub(b) as i32).wrapping_sub(0x10);
    let al = m.r32(heap + 0x28);
    let mut used = ((want.wrapping_add(tail as i32).wrapping_add(al as i32).wrapping_add(0xf) as u32 & al.wrapping_neg()) as i32).wrapping_sub(0x10);
    if room < used {
        used = room;
        size = room.wrapping_sub(tail as i32);
    }
    let old = m.r32(b + 4);
    mem_move(m, p.wrapping_add(size as u32), p.wrapping_add(old), tail);
    m.w32(b + 4, size as u32);
    if room.wrapping_sub(used) > 0x40 {
        let f = p.wrapping_add(used as u32).wrapping_add(0x10);
        init_block(m, f, None, 0, 0, 0, b, next);
        free_insert(m, heap, f);
        m.w32(next + 0xc, f);
        m.w32(b + 8, f);
    }
    p
}

// ---- The rest of the library (row E2n) ----

/// `gEAMem_MainHeapBase` 0x3bb97c (gp − 0x7f74): where the main heap starts (set by the first
/// `EAMem_InitMainHeap`, or by `EAMem_InitDefaultHeap` to what it allocated; 0 after `EAMem_DestroyAll`)
pub const MAIN_HEAP_BASE: u32 = 0x3bb97c;
/// `gEAMem_AllocFlags` 0x3bb978 (gp − 0x7f78): the flags `Compress_Alloc` allocates with (also set for a while by
/// the archives and the front end: `asyncbig::MEM_FLAGS`)
pub const ALLOC_FLAGS: u32 = 0x3bb978;
/// `gEAMem_AllocName` 0x3bb98c (gp − 0x7f64): the name `Compress_Alloc` gives its blocks
pub const ALLOC_NAME: u32 = 0x3bb98c;
/// `gEAMem_CheckError` 0x3bb9ac (gp − 0x7f44): why the last heap check failed (1 not a list block, 2 an end block
/// without its end flag, 3 "BEND" missing, 4 an unknown tag, 5 a link not pointing back, 6 the free list not
/// closing on the sentinel)
pub const CHECK_ERROR: u32 = 0x3bb9ac;
/// "RAM" (0x3bb9a8), the main heap's name
pub const MAIN_HEAP_NAME: u32 = 0x3bb9a8;
/// `EAMem_DestroyAll` 0x2cd8b0, the exit handler `EAMem_InitDefaultHeap` registers
pub const DESTROY_ALL: u32 = 0x2cd8b0;
/// The out-of-memory word the main heap keeps at +0x3c (0x2cd790, a function that answers 0)
pub const MAIN_HEAP_WORD: u32 = 0x2cd790;

/// `EAMem_Zero` 0x2cc938(p, n): `EAMem_Set` with 0
pub fn zero(m: &mut Mem, p: u32, n: u32) {
    set(m, p, 0, n);
}

/// `Compress_Alloc` 0x2cc958(size): `EAMem_Alloc` with the name at [`ALLOC_NAME`] and the flags at
/// [`ALLOC_FLAGS`]
pub fn compress_alloc(m: &mut Mem, size: u32, out_of_memory: &mut dyn FnMut(&mut Mem, u32, u32, u32, u32) -> u32) -> u32 {
    let (name, flags) = (m.r32(ALLOC_NAME), m.r32(ALLOC_FLAGS));
    alloc(m, name, size, flags, out_of_memory)
}

/// `Compress_Free` 0x2cc980(p): `EAMem_Free`
pub fn compress_free(m: &mut Mem, p: u32) -> u32 {
    free(m, p)
}

/// `EAMem_ClearHeapTable` 0x2cd250: the 16 heaps forgotten (their records left as they are)
pub fn clear_heap_table(m: &mut Mem) {
    set(m, HEAPS, 0, 0x40);
}

/// `EAMem_DestroyHeap` 0x2cd500(id): heap `id & 0xf`'s lock destroyed (`RecMutex_Destroy` 0x2cf4b0, our host's:
/// `destroy_lock` gets its handle), its record zeroed, its table entry cleared. 0 when there was no heap, else 1.
/// The blocks are left as they are.
pub fn destroy_heap(m: &mut Mem, id: u32, destroy_lock: &mut dyn FnMut(u32)) -> u32 {
    let slot = HEAPS + 4 * (id & 0xf);
    let h = m.r32(slot);
    if h == 0 {
        return 0;
    }
    let l = m.r32(h + 0x38);
    if l != 0 {
        destroy_lock(l);
    }
    set(m, h, 0, 0x44);
    m.w32(slot, 0);
    1
}

/// `EAMem_FreeAll` 0x2cd570(id): every block of heap `id & 0xf` but the two ends passed to `EAMem_Free`, walking
/// from the base by each block's next link, read after the free. As in the game, this is only sound when no
/// block is free to begin with: a free block is freed again (put in the list a second time), and the next link
/// read after a block was joined to the free block after it leads into that free block's old header. Never
/// called by the game.
pub fn free_all(m: &mut Mem, id: u32) {
    let mut b = m.r32(heap_of(m, id) + 8);
    while b != 0 {
        if m.r16(b + 2) & 0x8000 == 0 {
            free(m, b + 0x10);
        }
        b = m.r32(b + 8);
    }
}

/// `EAMem_InitMainHeap` 0x2cd798(base, size): heap 0, "RAM", in `size` bytes at `base` (alignment 0x40, large
/// alignment 0x80, nothing kept, no "BEND", no names, a lock: `lock` is the handle our host made, the word
/// 0x2cd790), copied to slot 1; [`MAIN_HEAP_BASE`] set to `base` when it was 0. Returns `EAMem_InitHeap`'s answer.
pub fn init_main_heap(m: &mut Mem, base: u32, size: u32, lock: u32) -> u32 {
    let n = init_heap(m, 0, MAIN_HEAP_NAME, base, size, 0x40, 0x80, 0, false, false, Some(lock), MAIN_HEAP_WORD);
    m.w32(HEAPS + 4, m.r32(HEAPS));
    if m.r32(MAIN_HEAP_BASE) == 0 {
        m.w32(MAIN_HEAP_BASE, base);
    }
    n
}

/// `EAMem_InitDefaultHeap` 0x2cd820(size): [`DESTROY_ALL`] registered to run at exit (`Sys_AddExitHandler`
/// 0x2ce9d0: `add_exit_handler`), `size` bytes from the C library (`malloc` 0x2fa818: `malloc`, both our host's)
/// kept in [`MAIN_HEAP_BASE`], made the main heap, and kept at the main heap's +0x40 (for `EAMem_DestroyAll` to
/// give back). 0 when `malloc` gave nothing, else `EAMem_InitMainHeap`'s answer. Never called by the game (only
/// by `EAMem_InitDefaultHeapMax`, which nothing calls).
pub fn init_default_heap(
    m: &mut Mem,
    size: u32,
    add_exit_handler: &mut dyn FnMut(u32),
    malloc: &mut dyn FnMut(u32) -> u32,
    lock: u32,
) -> u32 {
    add_exit_handler(DESTROY_ALL);
    let p = malloc(size);
    m.w32(MAIN_HEAP_BASE, p);
    if p == 0 {
        return 0;
    }
    let n = init_main_heap(m, p, size, lock);
    let h = m.r32(HEAPS);
    m.w32(h + 0x40, m.r32(MAIN_HEAP_BASE));
    n
}

/// `EAMem_InitDefaultHeapMax` 0x2cd890: `EAMem_InitDefaultHeap` with 0x6000000 bytes
pub fn init_default_heap_max(
    m: &mut Mem,
    add_exit_handler: &mut dyn FnMut(u32),
    malloc: &mut dyn FnMut(u32) -> u32,
    lock: u32,
) -> u32 {
    init_default_heap(m, 0x600_0000, add_exit_handler, malloc, lock)
}

/// `EAMem_DestroyAll` 0x2cd8b0: every table entry that repeats an earlier one's heap cleared (for each slot, the
/// later and earlier slots holding the same record), then every heap left destroyed (`EAMem_DestroyHeap`) and the
/// memory at its +0x40 (read before) given back to the C library (`free` 0x2fa840, our host's: `free_fn`), and
/// [`MAIN_HEAP_BASE`] cleared.
pub fn destroy_all(m: &mut Mem, destroy_lock: &mut dyn FnMut(u32), free_fn: &mut dyn FnMut(u32)) {
    for i in 0..16 {
        for j in 0..16 {
            if i != j {
                let a = m.r32(HEAPS + 4 * i);
                if a == m.r32(HEAPS + 4 * j) && a != 0 {
                    m.w32(HEAPS + 4 * j, 0);
                }
            }
        }
    }
    for i in 0..16 {
        let h = m.r32(HEAPS + 4 * i);
        if h != 0 {
            let p = m.r32(h + 0x40);
            destroy_heap(m, i, destroy_lock);
            if p != 0 {
                free_fn(p);
            }
        }
    }
    m.w32(MAIN_HEAP_BASE, 0);
}

/// `EAMem_IsBlockHeader` 0x2cdc60(p): 1 when `p` is not 0 (`Mem_IsReadable` 0x2ce2c0, which only checks that)
/// and is a free block (0x4246) or an end block (0x4253); else [`CHECK_ERROR`] 1 and 0
pub fn is_block_header(m: &mut Mem, p: u32) -> u32 {
    if p != 0 && matches!(m.r16(p), TAG_FREE | TAG_END) {
        return 1;
    }
    m.w32(CHECK_ERROR, 1);
    0
}

/// `EAMem_CheckNeighbours` 0x2cdcc8(b): an end block: 2 with its end flag (0x8000), else error 2 and 0. A free
/// block: 0 without its free flag (no error), else whether its next and previous free blocks are list blocks
/// (`EAMem_IsBlockHeader`). A made block: 2 without "BEND" (0x200), else 1 when "BEND" follows its data (read
/// big-endian), error 3 and 0 when not. Any other tag (a block taken from the free list and not made): error 4, 0.
pub fn check_neighbours(m: &mut Mem, b: u32) -> u32 {
    match m.r16(b) {
        TAG_END => {
            if m.r16(b + 2) & 0x8000 != 0 {
                return 2;
            }
            m.w32(CHECK_ERROR, 2);
            0
        }
        TAG_FREE => {
            if m.r16(b + 2) & FREE == 0 {
                return 0;
            }
            if is_block_header(m, m.r32(b + 0x10)) == 0 {
                return 0;
            }
            is_block_header(m, m.r32(b + 0x14))
        }
        TAG_MADE => {
            if m.r16(b + 2) & 0x200 == 0 {
                return 2;
            }
            let at = m.r32(b + 4).wrapping_add(b).wrapping_add(0x10);
            let mut v = 0u32;
            for k in 0..4 {
                v = (v << 8).wrapping_add(m.r8(at.wrapping_add(k)) as u32);
            }
            if v == 0x4245_4e44 {
                return 1;
            }
            m.w32(CHECK_ERROR, 3);
            0
        }
        _ => {
            m.w32(CHECK_ERROR, 4);
            0
        }
    }
}

/// `EAMem_CheckBackLinks` 0x2cddc0(b): an end block: 2 with its end flag, else error 2 and 0. A free block: 0
/// without its free flag (no error), else 1 when its next free block's previous and its previous one's next are
/// `b`, error 5 and 0 when not. A made block: the same with the block chain (+8 / +0xc). Any other tag: error 4, 0.
pub fn check_back_links(m: &mut Mem, b: u32) -> u32 {
    let links = |m: &Mem, next: u32, prev: u32| m.r32(m.r32(b + next) + prev) == b && m.r32(m.r32(b + prev) + next) == b;
    match m.r16(b) {
        TAG_END => {
            if m.r16(b + 2) & 0x8000 != 0 {
                return 2;
            }
            m.w32(CHECK_ERROR, 2);
            0
        }
        TAG_FREE => {
            if m.r16(b + 2) & FREE == 0 {
                return 0;
            }
            if links(m, 0x10, 0x14) {
                return 1;
            }
            m.w32(CHECK_ERROR, 5);
            0
        }
        TAG_MADE => {
            if links(m, 8, 0xc) {
                return 1;
            }
            m.w32(CHECK_ERROR, 5);
            0
        }
        _ => {
            m.w32(CHECK_ERROR, 4);
            0
        }
    }
}

/// `EAMem_CheckInHeap` 0x2cde70(s): from the sentinel `s`, along the free list's next links past every free block
/// (tag 0x4246 with the free flag) to an end block, then back along the previous links past every free block:
/// 1 when that ends at `s`, else error 6 and 0. (A list that never reaches a block of another kind loops, as in
/// the game.)
pub fn check_in_heap(m: &mut Mem, s: u32) -> u32 {
    let free = |m: &Mem, b: u32| m.r32(b) & 0x4000_ffff == 0x4000_4246;
    let mut b = m.r32(s + 0x10);
    while free(m, b) {
        b = m.r32(b + 0x10);
    }
    let mut ok = false;
    if m.r16(b) == TAG_END {
        b = m.r32(b + 0x14);
        while free(m, b) {
            b = m.r32(b + 0x14);
        }
        ok = b == s;
    }
    if !ok {
        m.w32(CHECK_ERROR, 6);
    }
    ok as u32
}

/// `EAMem_ValidateHeap` 0x2cdf08(id): heap `id & 0xf`'s blocks from its base along the next links, each through
/// `EAMem_CheckNeighbours` and `EAMem_CheckBackLinks` until one fails, then its free list (`EAMem_CheckInHeap`).
/// 1 when all pass, else 0 ([`CHECK_ERROR`] says why). (The lock around it is our host's.)
pub fn validate_heap(m: &mut Mem, id: u32) -> u32 {
    let h = heap_of(m, id);
    let mut b = m.r32(h + 8);
    let mut ok;
    loop {
        ok = check_neighbours(m, b) != 0 && check_back_links(m, b) != 0;
        if ok {
            b = m.r32(b + 8);
        }
        if b == 0 || !ok {
            break;
        }
    }
    if ok {
        check_in_heap(m, h + 0x10)
    } else {
        0
    }
}

/// `EAMem_ValidateAll` 0x2cdfd8: every heap in the table, in order, through `EAMem_ValidateHeap` until one
/// fails. 1 when none did.
pub fn validate_all(m: &mut Mem) -> u32 {
    let mut r = 1;
    for i in 0..16 {
        if m.r32(HEAPS + 4 * i) != 0 {
            r = validate_heap(m, i);
        }
        if r == 0 {
            break;
        }
    }
    r
}
