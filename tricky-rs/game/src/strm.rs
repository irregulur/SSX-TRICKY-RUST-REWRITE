//! EA's stream library (board row E2z: its small calls) on the game's bytes. A stream handle's first word points
//! at the stream's header (magic "STRM"); the handle's +4 is a word callers set, +8 the bytes buffered. The header:
//! +0x20 the chunk table (16 bytes each) and +0x24 its count, +0x28 / +0x30 the buffer's start and end, +0x34 the
//! state, +0x38 / +0x3c the priorities (the second the file system's), +0x40 the low-water mark, +0x44 urgent,
//! +0x48 the bytes filled, +0x170 the file system operation reading.
//!
//! Calls out (`FileSys_SetPriority`, ported as `filesys::set_priority`) go through [`StrmEnv`]. Proven from the
//! disassembly of `SLUS_203.26` (read by hand: Ghidra was not running).

use crate::pf::Mem;

/// The calls out
pub trait StrmEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

pub const MAGIC: u32 = 0x4d52_5453;
pub const FS_SET_PRIORITY: u32 = 0x2ca460;

/// `Strm_GetHeader` 0x2c6358(handle, header out, handle out): 0 and both out for a handle whose header has the
/// magic; 1 (nothing out) for none or another
pub fn get_header(m: &mut Mem, h: u32, header_out: u32, handle_out: u32) -> u32 {
    match header(m, h) {
        Some(head) => {
            m.w32(handle_out, h);
            m.w32(header_out, head);
            0
        }
        None => 1,
    }
}

/// The header of a good handle
fn header(m: &Mem, h: u32) -> Option<u32> {
    if h == 0 {
        return None;
    }
    let head = m.r32(h);
    (m.r32(head) == MAGIC).then_some(head)
}

/// `RingBuf_InRange` 0x2c6390(start, end, at): whether `at` is in [start, end) of a ring (end below start wraps)
pub fn ring_in_range(start: u32, end: u32, at: u32) -> u32 {
    if end < start {
        (at >= start || at < end) as u32
    } else {
        (at >= start && at < end) as u32
    }
}

/// `Strm_CalcMemSize` 0x2c6e48(a, b, c): a × 0x124 + b × 0xc + c × 16 + 0x1b8
pub fn calc_mem_size(a: u32, b: u32, c: u32) -> u32 {
    a.wrapping_mul(0x124).wrapping_add(b.wrapping_mul(0xc).wrapping_add(0x178)).wrapping_add(c << 4).wrapping_add(0x40)
}

/// `Strm_SetPriorities` 0x2c7220(handle, priority, file priority)
pub fn set_priorities(m: &mut Mem, h: u32, prio: u32, file_prio: u32) {
    if let Some(head) = header(m, h) {
        m.w32(head + 0x38, prio);
        m.w32(head + 0x3c, file_prio);
    }
}

/// `Strm_SetLowWater` 0x2c7270(handle, mark): set; if the bytes filled cross it (below the new mark and not the old,
/// or the other way), `Strm_SetUrgent`(below the new mark)
pub fn set_low_water(m: &mut Mem, e: &mut dyn StrmEnv, h: u32, mark: u32) {
    if let Some(head) = header(m, h) {
        let old = m.r32(head + 0x40);
        m.w32(head + 0x40, mark);
        let fill = m.r32(head + 0x48) as i32;
        let below_new = fill < mark as i32;
        if (fill < old as i32) != below_new {
            set_urgent(m, e, h, below_new as u32);
        }
    }
}

/// `Strm_SetUrgent` 0x2c72e0(handle, urgent): set; becoming urgent while reading (state 1) moves its file system
/// operation up to the stream's file priority
pub fn set_urgent(m: &mut Mem, e: &mut dyn StrmEnv, h: u32, urgent: u32) {
    if let Some(head) = header(m, h) {
        m.w32(head + 0x44, urgent);
        if urgent != 0 && m.r32(head + 0x34) == 1 {
            let (op, prio) = (m.r32(head + 0x170), m.r32(head + 0x3c));
            e.call(m, FS_SET_PRIORITY, &[op, prio]);
        }
    }
}

/// `Strm_GetChunk` 0x2c7340(handle, n): the n-th (from 1) entry of the chunk table, 0 if none
pub fn get_chunk(m: &Mem, h: u32, n: u32) -> u32 {
    match header(m, h) {
        Some(head) if n as i32 > 0 && (m.r32(head + 0x24) as i32) >= n as i32 => m.r32(head + 0x20).wrapping_add((n << 4).wrapping_sub(0x10)),
        _ => 0,
    }
}

/// `Strm_GetWord4` 0x2c73a0: the handle's +4 (0 for a bad handle)
pub fn get_word4(m: &Mem, h: u32) -> u32 {
    header(m, h).map_or(0, |_| m.r32(h + 4))
}

/// `Strm_GetBufferedBytes` 0x2c7c10: the handle's +8
pub fn get_buffered_bytes(m: &Mem, h: u32) -> u32 {
    header(m, h).map_or(0, |_| m.r32(h + 8))
}

/// `Strm_GetState` 0x2c7c40
pub fn get_state(m: &Mem, h: u32) -> u32 {
    header(m, h).map_or(0, |head| m.r32(head + 0x34))
}

/// `Strm_IsFinished` 0x2c7c70: state 0 and nothing buffered
pub fn is_finished(m: &Mem, h: u32) -> u32 {
    header(m, h).map_or(0, |head| (m.r32(head + 0x34) == 0 && m.r32(h + 8) == 0) as u32)
}

/// `Strm_GetBufferSize` 0x2c7cb8: the buffer's end − start
pub fn get_buffer_size(m: &Mem, h: u32) -> u32 {
    header(m, h).map_or(0, |head| m.r32(head + 0x30).wrapping_sub(m.r32(head + 0x28)))
}

/// `Strm_GetFillBytes` 0x2c7cf0: the bytes filled
pub fn get_fill_bytes(m: &Mem, h: u32) -> u32 {
    header(m, h).map_or(0, |head| m.r32(head + 0x48))
}
