//! The block manager (board row E2s) on the game's bytes: pools of fixed-size blocks carved out of one
//! allocation, the game's own at 0x345e48 (`BlockMan_Init` with 4 classes from the table at 0x3a8d78).
//!
//! The manager (0x10 bytes): +0 the allocation, +4 its size, +8 the class table (the allocation's start), +0xc
//! the class count. A class (0x10 bytes): +0 its block count, +4 its block size (rounded up to 16), +8 the first
//! free block, +0xc the first block. A block: a 0x10-byte header, then the data. Free: +4 the next free block (0
//! for the last). Taken: +0 the class, +4 / +8 / +0xc three words from the caller (a name and two more).
//!
//! The game's classes (0x3a8d78): 10 of 0x80 bytes, 10 of 0x200, 200 of 0x400, 32 of 0x800. A class of 0
//! blocks would still point its free list at its start (the next class's first block), so asking it would hand
//! out a block of the next class (no table has one).
//!
//! Proven from the disassembly of `SLUS_203.26` (no decompiler: read by hand).

use crate::eamem;
use crate::pf::Mem;

/// The game's manager
pub const BLOCKS: u32 = 0x345e48;
/// The allocation's name ("BlockManMem", 0x3a4258)
pub const NAME: u32 = 0x3a4258;
/// What a new allocation is filled with
pub const FILL: u32 = 0xdead_c0ed;

/// The block size rounding: `(n + 15) / 16 * 16`, the division rounding toward zero (signed)
fn round16(n: u32) -> u32 {
    ((n.wrapping_add(15) as i32 / 16) as u32).wrapping_mul(16)
}

/// `BlockMan_Init` 0x23d160(man, n, classes): `classes` holds n (count, size) word pairs. Allocates (`EAMem_Alloc`
/// 0x2ccf70 under [`NAME`], flags 0) the class table (16 bytes a class) and every class's blocks (count × (size
/// rounded + 16)), fills it with [`FILL`] (`EAMem_Set`) when it got memory, then lays the classes out in order
/// after the table, each class's blocks chained into its free list in address order. (With no memory it still
/// writes the table, at 0.)
pub fn init(m: &mut Mem, man: u32, n: i32, classes: u32, out_of_memory: &mut dyn FnMut(&mut Mem, u32, u32, u32, u32) -> u32) {
    m.w32(man + 4, 0);
    for i in 0..n.max(0) as u32 {
        let (count, size) = (m.r32(classes + 8 * i), m.r32(classes + 8 * i + 4));
        let bytes = count.wrapping_mul(round16(size).wrapping_add(0x10));
        m.w32(man + 4, m.r32(man + 4).wrapping_add(bytes));
    }
    let table = round16((n as u32).wrapping_mul(16));
    let total = m.r32(man + 4).wrapping_add(table);
    m.w32(man + 4, total);
    let mem = eamem::alloc(m, NAME, total, 0, out_of_memory);
    if mem != 0 {
        eamem::set(m, mem, FILL, total);
    }
    m.w32(man + 0xc, n as u32);
    m.w32(man, mem);
    m.w32(man + 8, mem);
    let mut at = mem.wrapping_add(table);
    for i in 0..n.max(0) as u32 {
        let c = m.r32(man + 8).wrapping_add(16 * i);
        m.w32(c + 8, at);
        m.w32(c + 0xc, at);
        let size = round16(m.r32(classes + 8 * i + 4));
        m.w32(c + 4, size);
        let stride = size.wrapping_add(0x10);
        let count = m.r32(classes + 8 * i);
        m.w32(c, count);
        at = count.wrapping_mul(stride).wrapping_add(at);
        let mut k = 0i32;
        while k < count as i32 {
            let first = m.r32(c + 0xc);
            let next = if k == count as i32 - 1 { 0 } else { first.wrapping_add((k as u32 + 1).wrapping_mul(stride)) };
            m.w32(first.wrapping_add((k as u32).wrapping_mul(stride)) + 4, next);
            k += 1;
        }
    }
}

/// `Pool_Destroy` 0x23d368(man): the allocation given back (`Mem_Free` 0x14a040, so `EAMem_Free`) when there,
/// and the four words cleared
pub fn destroy(m: &mut Mem, man: u32) {
    let p = m.r32(man);
    if p != 0 {
        mem_free(m, p);
    }
    for o in [0xc, 0, 4, 8] {
        m.w32(man + o, 0);
    }
}

/// `MemPool_Alloc` 0x23d3b0(man, size, flags, name, w2, w3): a block from the first class (in order) whose block
/// size is at least `size` (unsigned) and that has one free; its header takes the class and the three words.
/// Returns the data (header + 0x10), or 0. (`flags` is not read.)
pub fn alloc(m: &mut Mem, man: u32, size: u32, name: u32, w2: u32, w3: u32) -> u32 {
    let n = m.r32(man + 0xc) as i32;
    for i in 0..n.max(0) as u32 {
        let c = m.r32(man + 8).wrapping_add(16 * i);
        if m.r32(c + 4) < size {
            continue;
        }
        let b = m.r32(c + 8);
        if b == 0 {
            continue;
        }
        m.w32(b, i);
        m.w32(c + 8, m.r32(b + 4));
        m.w32(b + 4, name);
        m.w32(b + 8, w2);
        m.w32(b + 0xc, w3);
        return b + 0x10;
    }
    0
}

/// `MemPool_Free` 0x23d420(man, p): the block put back at the head of its class's free list. Returns the class ×
/// 16 (what is left in v0).
pub fn free(m: &mut Mem, man: u32, p: u32) -> u32 {
    let b = p.wrapping_sub(0x10);
    let k = m.r32(b) << 4;
    let c = m.r32(man + 8).wrapping_add(k);
    m.w32(b + 4, m.r32(c + 8));
    m.w32(c + 8, b);
    k
}

/// `Mem_Free` 0x14a040 and `Mem_Delete` 0x114f80 (the same code): `p` given back to the game's block manager when
/// it lies past the start of its allocation and within its size (signed), else to the heaps (`EAMem_Free`
/// 0x2ccfc0, which takes 0 too). Returns what the callee leaves.
pub fn mem_free(m: &mut Mem, p: u32) -> u32 {
    let d = p.wrapping_sub(m.r32(BLOCKS)) as i32;
    if d > 0 && d < m.r32(BLOCKS + 4) as i32 {
        free(m, BLOCKS, p)
    } else {
        eamem::free(m, p)
    }
}

/// `Object_Delete` 0x23d738(obj): an object's deleting destructor, through its vtable (`obj`+0's entry 1: the
/// this-adjust half +8, the function +0xc) with 3 (destroy and free); nothing for 0. `call(m, f, this, 3)` runs
/// the function.
pub fn object_delete(m: &mut Mem, obj: u32, call: &mut dyn FnMut(&mut Mem, u32, u32, u32)) {
    if obj == 0 {
        return;
    }
    let vt = m.r32(obj);
    let adjust = m.r16(vt + 8) as i16 as i32 as u32;
    let f = m.r32(vt + 0xc);
    call(m, f, obj.wrapping_add(adjust), 3);
}

/// `Debug_Printf` 0x23d5d0: the release build's printf, which spills its arguments and does nothing
pub fn debug_printf() {}
