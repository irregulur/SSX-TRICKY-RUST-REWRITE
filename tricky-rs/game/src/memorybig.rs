//! The in-memory BIG file class (`cMemoryBigFile`, table 0x3a4120, board row E2e) and the BIG library's wrappers
//! over `BIG_Lookup` 0x2cbdc0. A `cMemoryBigFile` is a `cAsyncBigFile` ([`crate::asyncbig`]) whose whole archive is
//! loaded at once (+0x14, `File_LoadAlloc`): its requests share the same table, but their data comes out of the
//! archive by name with no reads. Every call out goes through [`BigEnv`].

use crate::asyncbig::{self as ab, req, strcpy, BigEnv};
use crate::pf::Mem;

/// `cMemoryBigFile`'s table
pub const VT: u32 = 0x3a4120;
pub const BIG_LOOKUP: u32 = 0x2cbdc0;
pub const FILE_LOAD_ALLOC: u32 = 0x2c8018;
/// The word the offset lookups hand `BIG_Lookup` for the size (gp − 0x7f90), which they never read
pub const SIZE_SINK: u32 = 0x3bb960;

/// `BIG_Find` 0x2cc010(big, name, index, off, size): `BIG_Lookup` with the same arguments; its answer (the entry's
/// name, 0 when there is none)
pub fn find(m: &mut Mem, e: &mut dyn BigEnv, big: u32, name: u32, index: u32, off: u32, size: u32) -> u32 {
    e.call(m, BIG_LOOKUP, &[big, name, index, off, size])
}

/// `BIG_GetFileOffset` 0x2cc030(big, name) (`sp` the stack pointer on entry): the entry's offset (a word on the
/// stack, 0 first), looked up only when there is a name
pub fn file_offset(m: &mut Mem, e: &mut dyn BigEnv, big: u32, name: u32, sp: u32) -> u32 {
    let at = sp.wrapping_sub(0x20);
    m.w32(at, 0);
    if name != 0 {
        find(m, e, big, name, 0, at, SIZE_SINK);
    }
    m.r32(at)
}

/// `BIG_GetFileDataByIndex` 0x2cc060(big, index) (`sp`): the entry's bytes in the archive (`big` + its offset), 0
/// when it is not there
pub fn file_data_by_index(m: &mut Mem, e: &mut dyn BigEnv, big: u32, index: u32, sp: u32) -> u32 {
    let at = sp.wrapping_sub(0x30);
    if find(m, e, big, 0, index, at, SIZE_SINK) != 0 {
        big.wrapping_add(m.r32(at))
    } else {
        0
    }
}

/// `BIG_GetFileData` 0x2cc0a8(big, name) (`sp`): `big` + the offset, 0 when the offset is 0
pub fn file_data(m: &mut Mem, e: &mut dyn BigEnv, big: u32, name: u32, sp: u32) -> u32 {
    let o = file_offset(m, e, big, name, sp.wrapping_sub(0x20));
    if o == 0 { 0 } else { big.wrapping_add(o) }
}

/// `strchr` 0x2fce78
fn strchr(m: &Mem, s: u32, c: u8) -> u32 {
    let mut p = s;
    loop {
        let b = m.r8(p);
        if b == c {
            return p;
        }
        if b == 0 {
            return 0;
        }
        p = p.wrapping_add(1);
    }
}

/// `BIG_GetFileDataList` 0x2cc120(big, names, out) (`sp`): each name of the comma-separated list (each comma made
/// 0 for the lookup and put back) to its data (`BIG_GetFileData`), into the words from `out`
pub fn file_data_list(m: &mut Mem, e: &mut dyn BigEnv, big: u32, names: u32, out: u32, sp: u32) {
    let (mut s, mut out) = (names, out);
    loop {
        let c = strchr(m, s, b',');
        if c != 0 {
            m.w8(c, 0);
        }
        let v = file_data(m, e, big, s, sp.wrapping_sub(0x60));
        m.w32(out, v);
        out = out.wrapping_add(4);
        if c == 0 {
            return;
        }
        m.w8(c, b',');
        s = c.wrapping_add(1);
        if s == 0 {
            return;
        }
    }
}

/// `BIG_GetFileName` 0x2cc268(big, index, out): the entry's name copied to `out` and `out`; else `out` emptied and 0
pub fn file_name(m: &mut Mem, e: &mut dyn BigEnv, big: u32, index: u32, out: u32) -> u32 {
    let n = find(m, e, big, 0, index, 0, 0);
    if n == 0 {
        m.w8(out, 0);
        return 0;
    }
    strcpy(m, out, n);
    out
}

/// `cMemoryBigFile::cMemoryBigFile` 0x23c9d0(f): a `cAsyncBigFile`, no archive, its own table; `f`
pub fn construct(m: &mut Mem, e: &mut dyn BigEnv, f: u32) -> u32 {
    ab::construct(m, e, f);
    m.w32(f + 0x14, 0);
    m.w32(f + 0x10, VT);
    f
}

/// `cMemoryBigFile::~cMemoryBigFile` 0x23ca08(f, how): +0 = 0, its table, the archive freed (vf2), then
/// `cAsyncBigFile`'s destructor
pub fn destruct(m: &mut Mem, e: &mut dyn BigEnv, f: u32, how: u32) {
    m.w32(f, 0);
    m.w32(f + 0x10, VT);
    vf2(m, e, f);
    ab::destruct(m, e, f, how);
}

/// `cMemoryBigFile::vf1` 0x23ca70(f, name, w), open: the archive loaded whole (`File_LoadAlloc`) into +0x14
pub fn vf1(m: &mut Mem, e: &mut dyn BigEnv, f: u32, name: u32, w: u32) {
    let a = e.call(m, FILE_LOAD_ALLOC, &[name, w]);
    m.w32(f + 0x14, a);
}

/// `cMemoryBigFile::vf2` 0x23caa0(f), close: the archive freed (`EAMem_Free`) and cleared when there is one
pub fn vf2(m: &mut Mem, e: &mut dyn BigEnv, f: u32) {
    let a = m.r32(f + 0x14);
    if a != 0 {
        e.call(m, ab::EAMEM_FREE, &[a]);
        m.w32(f + 0x14, 0);
    }
}

/// `cMemoryBigFile::vf6` 0x23cb70: nothing (no reads to start)
pub fn vf6() {}

/// `cMemoryBigFile::vf7` 0x23ca50(f, out, name): the name without a leading `|`
pub fn vf7(m: &mut Mem, out: u32, name: u32) {
    if m.r8(name) == b'|' {
        strcpy(m, out, name + 1);
    } else {
        strcpy(m, out, name);
    }
}

/// `cMemoryBigFile::vf10` 0x23cad8(f, i): one holder fewer on request `i`; with none left its data freed
/// (`Mem_Free`) and cleared when set, and the request cleared (`EAMem_Set`)
pub fn vf10(m: &mut Mem, e: &mut dyn BigEnv, i: u32) {
    let r = req(i);
    let n = m.r32(r + 0x58).wrapping_sub(1);
    m.w32(r + 0x58, n);
    if (n as i32) > 0 {
        return;
    }
    let d = m.r32(r + 0x20);
    if d != 0 {
        e.call(m, ab::MEM_FREE, &[d]);
        m.w32(r + 0x20, 0);
    }
    e.call(m, ab::EAMEM_SET, &[r, 0, ab::REQUEST]);
}

/// `cMemoryBigFile::vf11` 0x23cb78: 1 (always read)
pub fn vf11() -> u32 {
    1
}

/// Request `r`'s entry found in the archive by its name (`BIG_Find` into two words at `at`, 0 first): +0x1c the
/// entry's bytes, then unpacked into new memory (`Compress_DecodeApi`) when packed, else copied (the entry's size);
/// the memory as in `cAsyncBigFile::vf12` (pool on 0x40000000, else `EAMem_Alloc`, filled with 0 or 0xdeadc0ed);
/// +0x1c cleared; the memory
fn take(m: &mut Mem, e: &mut dyn BigEnv, f: u32, r: u32, flags: u32, at: u32) -> u32 {
    m.w32(at + 4, 0);
    m.w32(at, 0);
    let big = m.r32(f + 0x14);
    find(m, e, big, r + 0x24, 0, at, at | 4);
    let src = m.r32(f + 0x14).wrapping_add(m.r32(at));
    m.w32(r + 0x1c, src);
    let size = e.call(m, ab::COMPRESS_UNPACKED_SIZE, &[src]);
    let packed = size != 0;
    let size = if packed { size } else { m.r32(at + 4) };
    let p = if flags & 0x4000_0000 != 0 {
        e.call(m, ab::MEMPOOL_ALLOC, &[ab::POOL, size, flags, ab::NAME, 0, 0])
    } else {
        e.call(m, ab::EAMEM_ALLOC, &[ab::NAME, size, flags & 0x3ff_ffff])
    };
    if p != 0 {
        let v = if (flags as i32) < 0 { 0 } else { ab::FILL };
        e.call(m, ab::EAMEM_SET, &[p, v, size]);
    }
    if packed {
        e.call(m, ab::COMPRESS_DECODE, &[m.r32(r + 0x1c), p]);
    } else {
        let n = m.r32(at + 4);
        e.call(m, ab::EAMEM_COPY, &[p, m.r32(r + 0x1c), n]);
    }
    m.w32(r + 0x1c, 0);
    p
}

/// `cMemoryBigFile::vf12` 0x23cb80(f, i, flags) (`sp`): request `i`'s data (+0x20), taken from the archive the
/// first time
pub fn vf12(m: &mut Mem, e: &mut dyn BigEnv, f: u32, i: u32, flags: u32, sp: u32) -> u32 {
    let r = req(i);
    if m.r32(r + 0x20) == 0 {
        let p = take(m, e, f, r, flags, sp.wrapping_sub(0x80));
        m.w32(r + 0x20, p);
    }
    m.r32(r + 0x20)
}

/// `cMemoryBigFile::vf13` 0x23cdb8(f, i, flags, name) (`sp`): 0 when request `i` already has data; else its data
/// taken from the archive (named 0x3a4110 whatever `name` says) and handed over, +0x20 left clear
pub fn vf13(m: &mut Mem, e: &mut dyn BigEnv, f: u32, i: u32, flags: u32, sp: u32) -> u32 {
    let r = req(i);
    if m.r32(r + 0x20) != 0 {
        return 0;
    }
    let p = take(m, e, f, r, flags, sp.wrapping_sub(0x90));
    m.w32(r + 0x20, 0);
    p
}
