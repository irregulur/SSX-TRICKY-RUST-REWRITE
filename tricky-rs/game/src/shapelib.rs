//! EA's shape library (`lib-shape`, board row E2r) on the game's bytes, and the codecs' front door it calls.
//!
//! A shape file (`SHPS`): +8 the shape count, from +0x10 one (4-character name, offset from the file) pair per
//! shape. A shape is a chain of blocks; each block's first word holds the type in its low byte and, in the other
//! 24 bits (signed), the offset to the next block of the shape (0: the last). A block of type 0x70 ('p') holds the
//! shape's long name from +4. The first block's header is 16 bytes: +4 width and +6 height (16-bit), +0xc the mip
//! level count in its top 4 bits; the pixels follow from +0x10. A first block with type bit 0x80 holds a packed
//! stream from +0x10 (the compression codecs); types 0x60..0x62 hold 8-byte colour blocks (see
//! `tricky_data::video`).

use crate::eamem;
use crate::pf::Mem;
use tricky_data::compress;

/// The names `Shape_Unpack` allocates under: 0x3bb918 (packed) and 0x3bb920 (colour blocks).
pub const NAME_PACKED: u32 = 0x3bb918;
pub const NAME_BLOCKS: u32 = 0x3bb920;

/// The bytes from `a` to the end of the memory given around it.
fn window(m: &Mem, a: u32) -> &[u8] {
    for (base, v) in &m.regions {
        if *base <= a && a < base + v.len() as u32 {
            return &v[(a - base) as usize..];
        }
    }
    panic!("shapelib: {a:#x} is outside the memory given");
}

fn read(m: &Mem, a: u32, n: u32) -> Vec<u8> {
    (0..n).map(|k| m.r8(a.wrapping_add(k))).collect()
}

fn write(m: &mut Mem, a: u32, b: &[u8]) {
    for (k, &x) in b.iter().enumerate() {
        m.w8(a.wrapping_add(k as u32), x);
    }
}

/// `Compress_UnpackedSizeApi` 0x2c3190 (a plain call of `Compress_UnpackedSize` 0x2c2fb0) on the game's bytes.
pub fn unpacked_size(m: &Mem, src: u32) -> u32 {
    compress::unpacked_size(window(m, src))
}

/// `Compress_DecodeApi` 0x2c2f90 (a plain call of `Compress_Decode` 0x2c2de0) on the game's bytes: the stream at
/// `src` unpacked to `dst`; returns the codec's answer (0 for a type it doesn't take).
pub fn decode(m: &mut Mem, src: u32, dst: u32) -> u32 {
    let s = window(m, src).to_vec();
    if s.len() < 2 || s[1] != 0xfb {
        return 0;
    }
    let mut out = vec![];
    let r = match s[0] & 0xfe {
        0x10 | 0x90 => compress::refpack_decode(&s, &mut out),
        0x30 | 0x32 | 0x34 | 0xb0 | 0xb2 | 0xb4 => compress::huff_decode(&s, &mut out),
        0x46 | 0xc6 => compress::btree_decode(&s, &mut out),
        // every byte up to the last block's end is written (a row of blocks overruns a width not a multiple of
        // 4 into the next pixel rows, never leaving a gap)
        0x18 | 0x1a | 0x1c | 0x98 | 0x9a | 0x9c => compress::decode18(&s, &mut out),
        // 0x1e / 0x9e: the registered codec (gp − 0x7fc8), never set
        _ => return 0,
    };
    write(m, dst, &out);
    r
}

/// `Shape_GetCount` 0x2c2660
pub fn get_count(m: &Mem, f: u32) -> u32 {
    m.r32(f + 8)
}

/// `Shape_GetEntry` 0x2c266c(file, i): shape `i`'s first block, 0 past the count (unsigned)
pub fn get_entry(m: &Mem, f: u32, i: u32) -> u32 {
    if i < m.r32(f + 8) {
        f.wrapping_add(m.r32(f.wrapping_add(0x14).wrapping_add(i.wrapping_mul(8))))
    } else {
        0
    }
}

/// `Shape_GetEntryName` 0x2c2694(file, i, out): shape `i`'s 4-character name (0 past the count) stored at `out`
/// (no 0 after it); returns `out`
pub fn get_entry_name(m: &mut Mem, f: u32, i: u32, out: u32) -> u32 {
    let v = if i < m.r32(f + 8) { m.r32(f.wrapping_add(0x10).wrapping_add(i.wrapping_mul(8))) } else { 0 };
    m.w32(out, v);
    out
}

/// `Shape_GetLongName` 0x2c2610(block): the long name of the shape whose chain `block` is in (the first 'p'
/// block from it on, its +4), or 0
pub fn get_long_name(m: &Mem, mut p: u32) -> u32 {
    if p == 0 {
        return 0;
    }
    loop {
        if m.r8(p) == b'p' {
            return p + 4;
        }
        let w = m.r32(p);
        p = if w & 0xffff_ff00 == 0 { 0 } else { p.wrapping_add((w as i32 >> 8) as u32) };
        if p == 0 {
            return 0;
        }
    }
}

fn cstr(m: &Mem, mut a: u32) -> Vec<u8> {
    let mut s = vec![];
    loop {
        let c = m.r8(a);
        if c == 0 {
            return s;
        }
        s.push(c);
        a += 1;
    }
}

/// `Shape_FindByName` 0x2c2528(file, name) (and `Shape_Find` 0x2c25f0, a plain call of it): the first shape whose
/// long name, or 4-character name when it has none, equals `name` (`strcmp`); its first block, or 0
pub fn find_by_name(m: &Mem, f: u32, name: u32) -> u32 {
    let n = get_count(m, f) as i32;
    let want = cstr(m, name);
    for i in 0..n.max(0) as u32 {
        let e = get_entry(m, f, i);
        let long = get_long_name(m, e);
        let have = if long == 0 {
            // the 4 characters on the stack with a 0 after them
            let w = m.r32(f.wrapping_add(0x10).wrapping_add(i * 8)).to_le_bytes();
            w.iter().take_while(|&&c| c != 0).copied().collect()
        } else {
            cstr(m, long)
        };
        if have == want {
            return f.wrapping_add(m.r32(f.wrapping_add(0x14).wrapping_add(i * 8)));
        }
    }
    0
}

/// `Shape_PixelFormatIndex` 0x2c26d8: a pixel format (bits per pixel, or the 555 / 565 / 1555 / 888 / 8888
/// spellings) to the library's index: 4 → 1, 8 → 2, 15 / 16 / 555 / 565 / 1555 → 3, 24 / 888 → 4,
/// 32 / 8888 → 5, else 0
pub fn pixel_format_index(f: i32) -> u32 {
    match f {
        4 => 1,
        8 => 2,
        0xf | 0x10 | 0x22b | 0x235 | 0x613 => 3,
        0x18 | 0x378 => 4,
        0x20 | 0x22b8 => 5,
        _ => 0,
    }
}

/// `Shape_Unpack` 0x2c2318(block): a shape's first block made plain in new memory, or the block itself when it
/// already is. Types 0x60 / 0x61 / 0x62: 8-byte colour blocks (16 bytes apart for 0x61 and 0x62) to 16-bit
/// pixels by `Compress_Decode18Rows` 0x2c2bf8, in width × (2 bytes, 4 for 0x61 / 0x62) × height bytes (the
/// second half of a 0x61 / 0x62 buffer left as allocated), the type byte then `Shape_PixelFormatIndex(15)`.
/// Packed (type bit 0x80): mip levels of 0x18fb / 0x28fb streams go to [`unpack_blocks`]; any other stream is
/// unpacked after a copy of the 16-byte header (a 0x1afb stream's type byte set to 3). The type byte loses bit
/// 0x80, and the block's next offset is cleared when it had one.
pub fn unpack(m: &mut Mem, p: u32, out_of_memory: &mut dyn FnMut(&mut Mem, u32, u32, u32, u32) -> u32) -> u32 {
    let ty = m.r8(p);
    let flags = m.r32(eamem::ALLOC_FLAGS);
    let out;
    if ty & 0x80 == 0 {
        let (mode, bits) = match ty {
            0x60 => (1, 0x10),
            0x61 => (3, 0x20),
            0x62 => (5, 0x20),
            _ => return p,
        };
        let width = m.r16(p + 4) as i16 as i32;
        let size = (width.wrapping_mul(bits) >> 3).wrapping_mul(m.r16(p + 6) as i16 as i32);
        out = eamem::alloc(m, NAME_BLOCKS, size.wrapping_add(0x10) as u32, flags, out_of_memory);
        eamem::copy(m, out, p, 0x10);
        let src = window(m, p + 0x10).to_vec();
        let mut px = vec![];
        compress::decode18_rows(&src, &mut px, width, size, mode);
        write(m, out + 0x10, &px);
        m.w8(out, pixel_format_index(0xf) as u8);
        return out;
    }
    let src = p + 0x10;
    let kind = (m.r8(src) as u32) << 8 | m.r8(src + 1) as u32;
    if (kind == 0x18fb || kind == 0x28fb) && m.r32(p + 0xc) >> 28 != 0 {
        out = unpack_blocks(m, p, out_of_memory);
    } else {
        let n = unpacked_size(m, src);
        out = eamem::alloc(m, NAME_PACKED, n.wrapping_add(0x10), flags, out_of_memory);
        eamem::copy(m, out, p, 0x10);
        decode(m, src, out + 0x10);
        if kind == 0x1afb {
            m.w8(out, pixel_format_index(0xf) as u8);
        }
    }
    let b = m.r8(out) & 0x7f;
    m.w8(out, b);
    if m.r32(p) & 0xffff_ff00 != 0 {
        m.w32(out, b as u32);
    }
    out
}

/// `Shape_UnpackBlocks` 0x2c2088(block): a packed shape with mip levels (+0xc's top 4 bits, k = 1..=count). The
/// first level's stream (8-byte header, then a quarter of its size: colour blocks pack 4 to 1) unpacks after the
/// copied 16-byte header, into (size + 0x10) × 2 bytes. Each next level's stream has no header of its own: the
/// first's is written over the 8 bytes before its data (the end of the level before), with the width shifted
/// down by k and the size by 2k, unpacked after the levels before, and then only the first 4 of those 8 bytes
/// put back (the source keeps the header's last 4).
pub fn unpack_blocks(m: &mut Mem, p: u32, out_of_memory: &mut dyn FnMut(&mut Mem, u32, u32, u32, u32) -> u32) -> u32 {
    let src = p + 0x10;
    let n = unpacked_size(m, src) as i32;
    let levels = m.r32(p + 0xc) >> 28;
    let size = if levels == 0 { n.wrapping_add(0x10) } else { n.wrapping_add(0x10).wrapping_mul(2) };
    let flags = m.r32(eamem::ALLOC_FLAGS);
    let out = eamem::alloc(m, NAME_PACKED, size as u32, flags, out_of_memory);
    eamem::copy(m, out, p, 0x10);
    let head = read(m, src, 8);
    decode(m, src, out + 0x10);
    let mut dst = out.wrapping_add(unpacked_size(m, src)).wrapping_add(0x10);
    let q = p.wrapping_add((unpacked_size(m, src) as i32 / 4) as u32);
    let mut next = q.wrapping_add(0x18);
    if levels != 0 {
        let mut at = q.wrapping_add(0x10);
        let width = (head[6] as i32) << 8 | head[7] as i32;
        let bytes = (head[2] as i32) << 16 | (head[3] as i32) << 8 | head[4] as i32;
        for k in 1..=levels {
            let kept = read(m, at, 8);
            write(m, at, &head);
            let w = width / (1 << k);
            write(m, at + 6, &[(w >> 8) as u8, w as u8]);
            let s = bytes / (1 << ((k & 0xf) << 1));
            write(m, at + 2, &[(s >> 16) as u8, (s >> 8) as u8, s as u8]);
            decode(m, at, dst);
            dst = dst.wrapping_add(unpacked_size(m, at));
            next = next.wrapping_add((unpacked_size(m, at) as i32 / 4) as u32);
            write(m, at, &kept[..4]);
            at = next.wrapping_sub(8);
        }
    }
    out
}
