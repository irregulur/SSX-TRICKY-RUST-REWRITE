//! EA's stream library (board rows E2z, E2aa, E2ab) on the game's bytes. A stream handle is one of its chunks
//! (16 bytes): +0 points at the stream's state (the "header", magic "STRM"), +4 the chunk's number, +8 the bytes
//! buffered in it, +0xc its first packet. The header:
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

/// `Strm_GetWord4` 0x2c73a0: the handle's +4, its chunk number (0 for a bad handle)
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

// ---- The request list (row E2aa) ----
//
// The stream's state (`st`): +4 its lock, +0x10 / +0x14 the requests (0x124 bytes each) and their count, +0x18 /
// +0x1c the chunk types (12 bytes each: mask, value, type) and their count, +0x58 the first request queued, +0x5c
// the one being read, +0x60 the last, +0x64 the free ones. A request: +0 its id (the low byte its place, the rest
// a number), +4 queued, +8 the one before, +0xc the one after (or the next free).

pub const MUTEX_LOCK: u32 = 0x2ceb00;
pub const MUTEX_UNLOCK: u32 = 0x2ceb60;
/// The next request number, in steps of 0x100 (gp − 0x7dc4)
pub const REQUEST_NUMBER: u32 = 0x3bbb2c;

/// `Strm_AllocRequest` 0x2c6460(st): a free request, numbered (0 skipped); 0 if none
pub fn alloc_request(m: &mut Mem, e: &mut dyn StrmEnv, st: u32) -> u32 {
    e.call(m, MUTEX_LOCK, &[st + 4]);
    let r = m.r32(st + 0x64);
    if r != 0 {
        let mut n = m.r32(REQUEST_NUMBER).wrapping_add(0x100);
        m.w32(st + 0x64, m.r32(r + 0xc));
        if n == 0 {
            n = 0x100;
        }
        m.w32(REQUEST_NUMBER, n);
        m.w32(r, m.r8(r) as u32 | n);
    }
    e.call(m, MUTEX_UNLOCK, &[st + 4]);
    r
}

/// `Strm_AppendRequest` 0x2c64e0(st, request): queued last (first, and being read, if the queue is empty)
pub fn append_request(m: &mut Mem, e: &mut dyn StrmEnv, st: u32, r: u32) {
    m.w32(r + 0xc, 0);
    m.w32(r + 4, 1);
    e.call(m, MUTEX_LOCK, &[st + 4]);
    let last = m.r32(st + 0x60);
    if last != 0 {
        m.w32(r + 8, last);
        m.w32(last + 0xc, r);
        m.w32(st + 0x60, r);
    } else {
        m.w32(r + 8, 0);
        m.w32(st + 0x60, r);
        m.w32(st + 0x58, r);
        m.w32(st + 0x5c, r);
    }
    e.call(m, MUTEX_UNLOCK, &[st + 4]);
}

/// `Strm_LookupRequest` 0x2c6558(st, id): the queued request with that id, else 0
pub fn lookup_request(m: &Mem, st: u32, id: u32) -> u32 {
    let i = id & 0xff;
    if (i as i32) >= m.r32(st + 0x14) as i32 {
        return 0;
    }
    let r = m.r32(st + 0x10).wrapping_add(i * 0x124);
    if m.r32(r) != id || m.r32(r + 4) == 0 {
        return 0;
    }
    r
}

/// `Strm_UnlinkRequest` 0x2c65a0(st, request): out of the queue (the one being read moves on to the next, or back
/// to the one before), into the free list
pub fn unlink_request(m: &mut Mem, st: u32, r: u32) {
    let (prev, next) = (m.r32(r + 8), m.r32(r + 0xc));
    if r == m.r32(st + 0x58) {
        m.w32(st + 0x58, next);
    } else {
        m.w32(prev + 0xc, next);
    }
    let (prev, next) = (m.r32(r + 8), m.r32(r + 0xc));
    if r == m.r32(st + 0x60) {
        m.w32(st + 0x60, prev);
    } else {
        m.w32(next + 8, prev);
    }
    if r == m.r32(st + 0x5c) {
        let v = if m.r32(r + 0xc) != 0 { m.r32(r + 0xc) } else { m.r32(r + 8) };
        m.w32(st + 0x5c, v);
    }
    m.w32(r + 4, 0);
    m.w32(r + 0xc, m.r32(st + 0x64));
    m.w32(st + 0x64, r);
}

/// `Strm_MatchChunkType` 0x2c6610(st, packet): the type of the first chunk type whose mask of the packet's first
/// word gives its value; −2 if none
pub fn match_chunk_type(m: &Mem, st: u32, packet: u32) -> u32 {
    let w = m.r32(packet);
    let n = m.r32(st + 0x1c) as i32;
    for i in 0..n.max(0) as u32 {
        let t = m.r32(st + 0x18).wrapping_add(i * 0xc);
        if w & m.r32(t) == m.r32(t + 4) {
            return m.r32(t + 8);
        }
    }
    -2i32 as u32
}

// ---- The rest of the stream library (row E2ab) ----
//
// More of the state: +0x28 the buffer's start, +0x2c where the data wraps to (a packet word −1 at the old place
// sends the reader there), +0x30 the buffer's end, +0x34 the state (0 idle, 1 reading, 2 full), +0x4c where the
// reader is, +0x50 how far the packets are sorted out, +0x54 how far the data came, +0x68 the file open (0x100
// bytes), +0x168 its handle, +0x16c where in it, +0x170 the file system operation, +0x174 the bytes asked. A packet:
// +0 its id word (−1 wrap, −2 used up), +4 its size (low 24 bits; the chunk number in the top 8 once sorted). A
// request also has: +0x10 1 for one from memory, +0x14 the file's name, +0x114 the memory it comes from, +0x118
// the file's start (or the memory's size), +0x11c the id word of its last packet, +0x120 where its data starts in
// the buffer. A request's state: 1 queued, 2 reading, 3 read, 4 purged. A chunk handle: +0 the state, +4 its
// number, +8 the bytes buffered, +0xc its first packet.

pub const MUTEX_INIT: u32 = 0x2cea98;
pub const MUTEX_DESTROY: u32 = 0x2ceae0;
pub const FS_GET_RESULT: u32 = 0x2ca6f8;
pub const FS_QUEUE_OPEN: u32 = 0x2ca8a8;
pub const FS_QUEUE_CLOSE: u32 = 0x2ca960;
pub const FS_QUEUE_READ: u32 = 0x2caa08;
pub const FS_SET_CALLBACK: u32 = 0x2ca410;
pub const FILE_CLOSE_SYNC: u32 = 0x2c8e48;
pub const THREAD_IS_CURRENT: u32 = 0x2cf200;
pub const THREAD_SLEEP_MS: u32 = 0x2cf1a8;
pub const POLL: u32 = 0x2ced00;
pub const ON_READ_DONE: u32 = 0x2c6890;
pub const OPEN_AFTER_CLOSE: u32 = 0x2c68d0;
pub const ON_DATA_READ: u32 = 0x2c6928;

const WRAP: u32 = -1i32 as u32;
const USED: u32 = -2i32 as u32;

fn lock(m: &mut Mem, e: &mut dyn StrmEnv, st: u32) {
    e.call(m, MUTEX_LOCK, &[st + 4]);
}

fn unlock(m: &mut Mem, e: &mut dyn StrmEnv, st: u32) {
    e.call(m, MUTEX_UNLOCK, &[st + 4]);
}

fn lt(a: u32, b: u32) -> bool {
    (a as i32) < b as i32
}

/// The priority to read with: the file one when urgent
fn read_prio(m: &Mem, st: u32) -> u32 {
    if m.r32(st + 0x44) != 0 {
        m.r32(st + 0x3c)
    } else {
        m.r32(st + 0x38)
    }
}

/// `Strm_ConsumeBytes` 0x2c63d0(st, n): the fill goes down; dropping below the low-water mark makes the stream
/// urgent (and, reading, its file operation moves up)
pub fn consume_bytes(m: &mut Mem, e: &mut dyn StrmEnv, st: u32, n: u32) {
    lock(m, e, st);
    let old = m.r32(st + 0x48);
    let new = old.wrapping_sub(n);
    m.w32(st + 0x48, new);
    unlock(m, e, st);
    let low = m.r32(st + 0x40);
    if lt(old, low) || !lt(new, low) {
        return;
    }
    m.w32(st + 0x44, 1);
    if m.r32(st + 0x34) == 1 {
        let (op, prio) = (m.r32(st + 0x170), m.r32(st + 0x3c));
        e.call(m, FS_SET_PRIORITY, &[op, prio]);
    }
}

/// `Strm_ClassifyPackets` 0x2c6668(st): the whole packets come in are sorted out: each gets its chunk's number (a
/// type of none: used up), counts in its chunk and the fill (no longer urgent above the low-water mark); the
/// request's last packet is padded to 64 bytes. 1 when the request's last packet was sorted out (not purged).
pub fn classify_packets(m: &mut Mem, e: &mut dyn StrmEnv, st: u32) -> u32 {
    let req = m.r32(st + 0x5c);
    let whole = |m: &Mem| -> Option<(u32, u32)> {
        let (end, p) = (m.r32(st + 0x54), m.r32(st + 0x50));
        if (end.wrapping_sub(p) as i32) < 8 {
            return None;
        }
        let size = m.r32(p + 4);
        if size & 0xff00_0000 != 0 || end < p.wrapping_add(size) {
            return None;
        }
        Some((p, size))
    };
    let Some((mut p, mut size)) = whole(m) else { return 0 };
    let pad = |size: u32, at: u32| {
        let r = at & 0x3f;
        (size.wrapping_add(r).wrapping_add(0x3f) & !0x3f).wrapping_sub(r)
    };
    loop {
        if m.r32(p) == m.r32(req + 0x11c) {
            size = pad(size, p);
        }
        let kind = match_chunk_type(m, st, p);
        let purged;
        if (kind as i32) < 0 {
            lock(m, e, st);
            purged = m.r32(req + 4) == 4;
            if !purged {
                m.w32(p, USED);
                m.w32(st + 0x50, m.r32(st + 0x50).wrapping_add(size));
            }
            unlock(m, e, st);
        } else {
            m.w32(p + 4, size | kind << 24);
            lock(m, e, st);
            purged = m.r32(req + 4) == 4;
            if !purged {
                let c = m.r32(st + 0x20).wrapping_add((kind << 4).wrapping_sub(0x10));
                let n = m.r32(c + 8).wrapping_add(size);
                m.w32(c + 8, n);
                if n == size {
                    m.w32(c + 0xc, p);
                }
                let (fill, low) = (m.r32(st + 0x48), m.r32(st + 0x40));
                let new = fill.wrapping_add(size);
                m.w32(st + 0x48, new);
                m.w32(st + 0x50, m.r32(st + 0x50).wrapping_add(size));
                if lt(fill, low) && !lt(new, low) {
                    m.w32(st + 0x44, 0);
                }
            }
            unlock(m, e, st);
        }
        if purged {
            if m.r32(p) == m.r32(req + 0x11c) {
                return 0;
            }
            let size = pad(size, m.r32(st + 0x50));
            m.w32(p + 4, size | kind << 24);
            return 0;
        }
        if m.r32(p) == m.r32(req + 0x11c) {
            return 1;
        }
        match whole(m) {
            Some((q, s)) => {
                p = q;
                size = s;
            }
            None => return 0,
        }
    }
}

/// `Strm_OnReadDone` 0x2c6890(_, _, st): the file opened; reading starts
pub fn on_read_done(m: &mut Mem, e: &mut dyn StrmEnv, st: u32) {
    let op = m.r32(st + 0x170);
    let h = e.call(m, FS_GET_RESULT, &[op]);
    m.w32(st + 0x168, h);
    if h != 0 {
        let prio = m.r32(st + 0x3c);
        scan_packets(m, e, st, prio);
    }
}

/// `Strm_OpenAfterClose` 0x2c68d0(_, _, st): the last file closed; the new one opened
pub fn open_after_close(m: &mut Mem, e: &mut dyn StrmEnv, st: u32) {
    let op = m.r32(st + 0x170);
    e.call(m, FS_GET_RESULT, &[op]);
    open_file(m, e, st, m.r32(st + 0x3c));
}

fn open_file(m: &mut Mem, e: &mut dyn StrmEnv, st: u32, prio: u32) {
    let op = e.call(m, FS_QUEUE_OPEN, &[st + 0x68, 1, prio, st]);
    m.w32(st + 0x170, op);
    if op != 0 {
        e.call(m, FS_SET_CALLBACK, &[op, ON_READ_DONE]);
    }
}

/// `Strm_OnDataRead` 0x2c6928(_, _, st): data came (from the file: the result; from memory: what was copied): the
/// packets sorted out; a request read to its end (a short read, or its last packet) or purged goes on to the next,
/// else reading goes on
pub fn on_data_read(m: &mut Mem, e: &mut dyn StrmEnv, st: u32) {
    let req = m.r32(st + 0x5c);
    let (got, done) = if m.r32(req + 0x10) == 1 {
        let got = m.r32(st + 0x174);
        (got, !lt(m.r32(st + 0x16c).wrapping_add(got), m.r32(req + 0x118)))
    } else {
        let op = m.r32(st + 0x170);
        let got = e.call(m, FS_GET_RESULT, &[op]);
        (got, lt(got, m.r32(st + 0x174)))
    };
    m.w32(st + 0x16c, m.r32(st + 0x16c).wrapping_add(got));
    m.w32(st + 0x54, m.r32(st + 0x54).wrapping_add(got));
    let last = classify_packets(m, e, st);
    let prio = m.r32(st + 0x3c);
    if m.r32(req + 4) == 4 {
        kick_next_request(m, e, st, prio);
    } else if done || last != 0 {
        lock(m, e, st);
        if m.r32(req + 4) != 4 {
            m.w32(req + 4, 3);
        }
        unlock(m, e, st);
        let prio = m.r32(st + 0x3c);
        kick_next_request(m, e, st, prio);
    } else {
        scan_packets(m, e, st, prio);
    }
}

/// `Strm_KickNextRequest` 0x2c6a28(st, priority): the next request starts reading (where the sorted packets end):
/// from memory at once; from a file: the same file reads on, another is opened (the last closed first); with none
/// left the stream is idle
pub fn kick_next_request(m: &mut Mem, e: &mut dyn StrmEnv, st: u32, prio: u32) {
    lock(m, e, st);
    let cur = m.r32(st + 0x5c);
    let none = if cur == 0 {
        true
    } else if m.r32(cur + 4) == 1 {
        false
    } else if m.r32(cur + 0xc) != 0 {
        m.w32(st + 0x5c, m.r32(cur + 0xc));
        false
    } else {
        true
    };
    let mut r = 0;
    if none {
        m.w32(st + 0x34, 0);
    } else {
        r = m.r32(st + 0x5c);
        m.w32(r + 4, 2);
        m.w32(r + 0x120, m.r32(st + 0x50));
    }
    unlock(m, e, st);
    if none {
        return;
    }
    m.w32(st + 0x54, m.r32(st + 0x50));
    if m.r32(r + 0x10) == 1 {
        m.w32(st + 0x16c, 0);
        scan_packets(m, e, st, prio);
        return;
    }
    m.w32(st + 0x16c, m.r32(r + 0x118));
    if crate::libc::strcmp(m, r + 0x14, st + 0x68) == 0 {
        scan_packets(m, e, st, prio);
        return;
    }
    crate::libc::strcpy(m, st + 0x68, r + 0x14);
    let h = m.r32(st + 0x168);
    if h == 0 {
        open_file(m, e, st, prio);
    } else {
        let op = e.call(m, FS_QUEUE_CLOSE, &[h, prio, st]);
        m.w32(st + 0x170, op);
        if op != 0 {
            e.call(m, FS_SET_CALLBACK, &[op, OPEN_AFTER_CLOSE]);
        }
    }
}

/// `Strm_ScanPackets` 0x2c6b88(st, priority): the reader skips used-up packets and wraps; requests wholly read
/// past are let go; then, with more than 0x8000 bytes of room (the part-packet moved to the buffer's start when
/// the end is near), the next 0x8000 bytes are read (from memory at once), else the stream is full (state 2)
pub fn scan_packets(m: &mut Mem, e: &mut dyn StrmEnv, st: u32, prio: u32) {
    loop {
        let rd = m.r32(st + 0x4c);
        if rd == m.r32(st + 0x50) {
            break;
        }
        match m.r32(rd) {
            WRAP => m.w32(st + 0x4c, m.r32(st + 0x2c)),
            USED => m.w32(st + 0x4c, rd.wrapping_add(m.r32(rd + 4))),
            _ => break,
        }
    }
    lock(m, e, st);
    loop {
        let first = m.r32(st + 0x58);
        let next = m.r32(first + 0xc);
        if next == 0 || m.r32(next + 4) == 1 {
            break;
        }
        let (rd, wr) = (m.r32(st + 0x4c), m.r32(st + 0x54));
        if ring_in_range(rd, wr, m.r32(next + 0x120).wrapping_sub(1)) != 0 {
            break;
        }
        unlink_request(m, st, first);
    }
    unlock(m, e, st);
    let (rd, wr) = (m.r32(st + 0x4c), m.r32(st + 0x54));
    let room = if wr < rd {
        rd.wrapping_sub(wr).wrapping_sub(0x41)
    } else {
        let room = m.r32(st + 0x30).wrapping_sub(wr).wrapping_sub(0x40);
        if 0x7fff < room as i32 {
            room
        } else {
            let cur = m.r32(st + 0x5c);
            let part = wr.wrapping_sub(m.r32(st + 0x50));
            let mem = m.r32(cur + 0x10) == 1;
            let space = rd.wrapping_sub(m.r32(st + 0x2c)).wrapping_sub(if mem { 0 } else { 0x40 });
            if lt(space, part.wrapping_add(1)) {
                m.w32(st + 0x34, 2);
                return;
            }
            if part & 0x3f == 0 || m.r32(cur + 0x10) == 1 {
                m.w32(st + 0x2c, m.r32(st + 0x28));
            } else {
                let p = part as i32;
                let down = (if p > -1 { p } else { p + 0x3f }) >> 6 << 6;
                let v = p.wrapping_sub(down).wrapping_sub(0x40) as u32;
                m.w32(st + 0x2c, m.r32(st + 0x28).wrapping_sub(v));
            }
            let (to, from) = (m.r32(st + 0x2c), m.r32(st + 0x50));
            crate::eamem::copy(m, to, from, part);
            let old = m.r32(st + 0x50);
            m.w32(old, WRAP);
            m.w32(old + 4, 8);
            let start = m.r32(st + 0x2c);
            m.w32(st + 0x50, start);
            m.w32(st + 0x54, start.wrapping_add(part));
            m.r32(st + 0x4c).wrapping_sub(start.wrapping_add(part)).wrapping_sub(1)
        }
    };
    if !(0x7fff < room as i32) {
        m.w32(st + 0x34, 2);
        return;
    }
    let cur = m.r32(st + 0x5c);
    if m.r32(cur + 0x10) == 1 {
        let (at, end) = (m.r32(st + 0x16c), m.r32(cur + 0x118));
        let n = if lt(end, at.wrapping_add(room)) { end.wrapping_sub(at) } else { room };
        m.w32(st + 0x174, n);
        let (to, from) = (m.r32(st + 0x54), m.r32(cur + 0x114));
        crate::eamem::copy(m, to, from, m.r32(st + 0x174));
        m.w32(cur + 0x114, m.r32(cur + 0x114).wrapping_add(m.r32(st + 0x174)));
        on_data_read(m, e, st);
    } else {
        m.w32(st + 0x174, 0x8000);
        let (h, at, to) = (m.r32(st + 0x168), m.r32(st + 0x16c), m.r32(st + 0x54));
        let op = e.call(m, FS_QUEUE_READ, &[h, at, to, 0x8000, prio, st]);
        m.w32(st + 0x170, op);
        if op != 0 {
            e.call(m, FS_SET_CALLBACK, &[op, ON_DATA_READ]);
        }
    }
}

/// `Strm_Create` 0x2c6e70(requests, chunk types, chunks, memory, size): the stream made in `memory` (requests 2 to
/// 0x100, types 1 to 16, chunks 1 to the types; at least 0x18000 bytes over `Strm_CalcMemSize` for the buffer):
/// the requests free, the types catch-all (type 1), the chunks idle, priorities 0x96 / 0x32. The first chunk's
/// handle, or 0.
pub fn create(m: &mut Mem, e: &mut dyn StrmEnv, nreq: u32, ntypes: u32, nchunks: u32, buf: u32, size: u32) -> u32 {
    let need = calc_mem_size(nreq, ntypes, nchunks);
    if !(0x17fff < size.wrapping_sub(need) as i32) {
        return 0;
    }
    let (r, t, c) = (nreq as i32, ntypes as i32, nchunks as i32);
    if r < 2 || r > 0x100 || ntypes.wrapping_sub(1) >= 0x10 || c <= 0 || t < c {
        return 0;
    }
    m.w32(buf, MAGIC);
    e.call(m, MUTEX_INIT, &[buf + 4]);
    let reqs = buf + 0x178;
    let types = nreq.wrapping_mul(0x124).wrapping_add(reqs);
    let chunks = ntypes.wrapping_mul(0xc).wrapping_add(types);
    let data = (chunks.wrapping_add(nchunks << 4) & !0x3f).wrapping_add(0x40);
    for (o, v) in [
        (0x14, nreq),
        (0x1c, ntypes),
        (0x24, nchunks),
        (0x34, 0),
        (0x40, 0),
        (0x44, 0),
        (0x48, 0),
        (0x58, 0),
        (0x5c, 0),
        (0x60, 0),
        (0x30, buf.wrapping_add(size)),
        (0x38, 0x96),
        (0x3c, 0x32),
        (0x54, data),
        (0x64, reqs),
        (0x10, reqs),
        (0x18, types),
        (0x20, chunks),
        (0x28, data),
        (0x2c, data),
        (0x4c, data),
        (0x50, data),
    ] {
        m.w32(buf + o, v);
    }
    crate::libc::memset(m, buf + 0x68, 0, 0x100);
    m.w32(buf + 0x168, 0);
    for i in 0..nreq {
        let q = m.r32(buf + 0x10) + i * 0x124;
        m.w32(q, i);
        m.w32(q + 4, 0);
        m.w32(q + 0xc, m.r32(buf + 0x10) + (i + 1) * 0x124);
    }
    let last = m.r32(buf + 0x10).wrapping_add(nreq * 0x124);
    m.w32(last.wrapping_sub(0x118), 0);
    for i in 0..ntypes {
        let q = m.r32(buf + 0x18) + i * 0xc;
        m.w32(q + 8, 1);
        m.w32(q, 0);
        m.w32(q + 4, 0);
    }
    for i in 0..nchunks {
        let q = m.r32(buf + 0x20) + (i << 4);
        m.w32(q + 8, 0);
        m.w32(q, buf);
        m.w32(q + 4, i + 1);
    }
    m.r32(buf + 0x20)
}

/// `Strm_SetPlaylistEntry` 0x2c7098(handle, type number, mask, value, chunk): a chunk type set (only while idle;
/// the last type stays catch-all; the chunk a number up to the chunks, or −1 / −2)
pub fn set_playlist_entry(m: &mut Mem, h: u32, n: u32, mask: u32, value: u32, chunk: u32) {
    let Some(st) = header(m, h) else { return };
    let types = m.r32(st + 0x1c);
    if n as i32 <= 0 || lt(types, n) {
        return;
    }
    if n == types && mask | value != 0 {
        return;
    }
    if chunk as i32 <= 0 && chunk != WRAP && chunk != USED {
        return;
    }
    if lt(m.r32(st + 0x24), chunk) || m.r32(st + 0x34) != 0 {
        return;
    }
    let t = m.r32(st + 0x18).wrapping_add((n * 0xc).wrapping_sub(0xc));
    m.w32(t + 8, chunk);
    m.w32(t, mask);
    m.w32(t + 4, value);
}

/// `VideoStream_Close` 0x2c7178(handle): stopped, waited on while reading (polling when on the file thread), the
/// magic cleared, the lock destroyed, the file closed
pub fn close(m: &mut Mem, e: &mut dyn StrmEnv, h: u32) {
    let Some(st) = header(m, h) else { return };
    stop(m, e, h);
    while m.r32(st + 0x34) == 1 {
        if e.call(m, THREAD_IS_CURRENT, &[0]) != 0 {
            e.call(m, POLL, &[0]);
        }
        e.call(m, THREAD_SLEEP_MS, &[0]);
    }
    m.w32(st, 0);
    e.call(m, MUTEX_DESTROY, &[st + 4]);
    let f = m.r32(st + 0x168);
    e.call(m, FILE_CLOSE_SYNC, &[f, 0x64]);
}

/// Queued; an idle stream starts reading
fn queue_request(m: &mut Mem, e: &mut dyn StrmEnv, st: u32, r: u32, prio: Option<u32>) -> u32 {
    append_request(m, e, st, r);
    lock(m, e, st);
    let state = m.r32(st + 0x34);
    if state == 0 {
        m.w32(st + 0x34, 1);
    }
    unlock(m, e, st);
    if state == 0 {
        let prio = prio.unwrap_or_else(|| read_prio(m, st));
        kick_next_request(m, e, st, prio);
    }
    m.r32(r)
}

/// `VideoStream_QueueFile` 0x2c73d0(handle, name, start, last packet's id word): a file request (its id, 0 if none
/// free)
pub fn queue_file(m: &mut Mem, e: &mut dyn StrmEnv, h: u32, name: u32, start: u32, last: u32) -> u32 {
    let Some(st) = header(m, h) else { return 0 };
    let r = alloc_request(m, e, st);
    if r == 0 {
        return 0;
    }
    m.w32(r + 0x10, 0);
    crate::libc::strncpy(m, r + 0x14, name, 0xff);
    m.w32(r + 0x118, start);
    m.w32(r + 0x11c, last);
    queue_request(m, e, st, r, None)
}

/// `Strm_Queue` 0x2c74c8(handle, memory, size, last packet's id word): a request from memory (size 0: up to and
/// with the last packet); its id, 0 if none free
pub fn queue(m: &mut Mem, e: &mut dyn StrmEnv, h: u32, src: u32, size: u32, last: u32) -> u32 {
    let Some(st) = header(m, h) else { return 0 };
    let r = alloc_request(m, e, st);
    if r == 0 {
        return 0;
    }
    let mut size = size;
    if size == 0 {
        let mut p = src;
        while m.r32(p) != last {
            let n = m.r32(p + 4);
            p = p.wrapping_add(n);
            size = size.wrapping_add(n);
        }
        size = size.wrapping_add(m.r32(p + 4));
    }
    m.w32(r + 0x118, size);
    m.w32(r + 0x114, src);
    m.w32(r + 0x11c, last);
    m.w32(r + 0x10, 1);
    queue_request(m, e, st, r, Some(0))
}

/// `Strm_Purge` 0x2c75d8(handle, request id): a queued request let go; a reading or read one purged: its packets
/// still buffered are used up, chunk by chunk (those before it in the buffer only theirs, those starting inside it
/// taken and given back)
pub fn purge(m: &mut Mem, e: &mut dyn StrmEnv, h: u32, id: u32) {
    let Some(st) = header(m, h) else { return };
    lock(m, e, st);
    let r = lookup_request(m, st, id);
    if r == 0 || m.r32(r + 4) == 4 {
        unlock(m, e, st);
        return;
    }
    if m.r32(r + 4) == 1 {
        unlink_request(m, st, r);
        unlock(m, e, st);
        return;
    }
    m.w32(r + 4, 4);
    let rd = m.r32(st + 0x4c);
    let from = if r == m.r32(st + 0x58) { rd } else { m.r32(r + 0x120) };
    let next = m.r32(r + 0xc);
    let to = if next != 0 && m.r32(next + 4) != 1 { m.r32(next + 0x120) } else { m.r32(st + 0x50) };
    unlock(m, e, st);
    let mut i = 0;
    while lt(i, m.r32(st + 0x24)) {
        let c = m.r32(st + 0x20).wrapping_add(i << 4);
        i += 1;
        if m.r32(c + 8) as i32 <= 0 {
            continue;
        }
        if ring_in_range(rd, from, m.r32(c + 0xc)) != 0 {
            let mut p = from;
            if from == to {
                continue;
            }
            let mine = m.r32(c + 4) << 24;
            loop {
                if m.r32(p) == WRAP {
                    p = m.r32(st + 0x2c);
                } else {
                    let w = m.r32(p + 4);
                    let size = w & 0xff_ffff;
                    if w & 0xff00_0000 == mine {
                        lock(m, e, st);
                        m.w32(c + 8, m.r32(c + 8).wrapping_sub(size));
                        unlock(m, e, st);
                        consume_bytes(m, e, st, size);
                        m.w32(p, USED);
                        m.w32(p + 4, size);
                    }
                    p = p.wrapping_add(size);
                }
                if p == to {
                    break;
                }
            }
        } else if ring_in_range(from, to, m.r32(c + 0xc)) != 0 {
            loop {
                let p = get_packet(m, e, c);
                release_packet(m, e, c, p);
                if m.r32(c + 8) as i32 <= 0 || ring_in_range(from, to, m.r32(c + 0xc)) == 0 {
                    break;
                }
            }
        }
    }
}

/// `Strm_Stop` 0x2c7858(handle): the requests queued or reading from the last back purged, those before the one
/// being read let go and it purged, every chunk emptied, the fill and the sorted packets used up; a full stream's
/// last packet padded to 64 bytes (or the buffer back to its start); idle
pub fn stop(m: &mut Mem, e: &mut dyn StrmEnv, h: u32) {
    let Some(st) = header(m, h) else { return };
    let mut last = m.r32(st + 0x60);
    if last == 0 {
        return;
    }
    while m.r32(last + 4).wrapping_sub(1) < 2 {
        let id = m.r32(last);
        purge(m, e, h, id);
        last = m.r32(st + 0x60);
    }
    while m.r32(st + 0x58) != m.r32(st + 0x5c) {
        let first = m.r32(st + 0x58);
        unlink_request(m, st, first);
    }
    let cur = m.r32(st + 0x5c);
    m.w32(cur + 4, 4);
    let mut i = 0;
    while lt(i, m.r32(st + 0x24)) {
        let c = m.r32(st + 0x20).wrapping_add(i << 4);
        m.w32(c + 8, 0);
        i += 1;
    }
    let fill = m.r32(st + 0x48);
    consume_bytes(m, e, st, fill);
    let mut p = m.r32(st + 0x4c);
    let mut size = 0u32;
    while p != m.r32(st + 0x50) {
        if m.r32(p) == WRAP {
            p = m.r32(st + 0x2c);
        } else {
            m.w32(p, USED);
            size = m.r32(p + 4) & 0xff_ffff;
            m.w32(p + 4, size);
            p = p.wrapping_add(size);
        }
    }
    if m.r32(st + 0x34) == 2 {
        if p == m.r32(st + 0x2c) {
            let start = m.r32(st + 0x28);
            m.w32(st + 0x2c, start);
            m.w32(st + 0x50, start);
        } else {
            let end = m.r32(st + 0x50);
            let mut pad = 0x40 - (end & 0x3f);
            if pad == 0x40 {
                pad = 0;
            }
            let lastp = end.wrapping_sub(size);
            m.w32(lastp + 4, size.wrapping_add(pad));
            m.w32(st + 0x50, end.wrapping_add(pad));
        }
        m.w32(st + 0x34, 0);
    }
}

/// `Strm_GetPacket` 0x2c7a28(handle): the chunk's first packet (its size word left without the chunk number), and
/// the chunk's next found; 0 if nothing buffered
pub fn get_packet(m: &mut Mem, e: &mut dyn StrmEnv, h: u32) -> u32 {
    let Some(st) = header(m, h) else { return 0 };
    if m.r32(h + 8) == 0 {
        return 0;
    }
    let p = m.r32(h + 0xc);
    let size = m.r32(p + 4) & 0xff_ffff;
    m.w32(p + 4, size);
    lock(m, e, st);
    let left = m.r32(h + 8).wrapping_sub(size);
    m.w32(h + 8, left);
    unlock(m, e, st);
    if left as i32 > 0 {
        let mine = m.r32(h + 4) << 24;
        let mut q = p.wrapping_add(size);
        while m.r32(q + 4) & 0xff00_0000 != mine {
            if m.r32(q) == WRAP {
                q = m.r32(st + 0x2c);
            } else {
                q = q.wrapping_add(m.r32(q + 4) & 0xff_ffff);
            }
        }
        m.w32(h + 0xc, q);
    }
    p
}

/// `Strm_ReleasePacket` 0x2c7b30(handle, packet): a packet in the buffer, not used up already, used up; a full
/// stream reads on
pub fn release_packet(m: &mut Mem, e: &mut dyn StrmEnv, h: u32, p: u32) {
    let Some(st) = header(m, h) else { return };
    if p < m.r32(st + 0x2c) || m.r32(st + 0x30).wrapping_sub(8) < p || m.r32(p) == USED {
        return;
    }
    let size = m.r32(p + 4);
    m.w32(p, USED);
    consume_bytes(m, e, st, size);
    lock(m, e, st);
    let state = m.r32(st + 0x34);
    if state == 2 {
        m.w32(st + 0x34, 1);
    }
    unlock(m, e, st);
    if state == 2 {
        let prio = read_prio(m, st);
        scan_packets(m, e, st, prio);
    }
}

/// `Strm_FindBufferedRequest` 0x2c7d20(handle): the id of the request the chunk's first packet came from (0 if
/// nothing buffered)
pub fn find_buffered_request(m: &mut Mem, e: &mut dyn StrmEnv, h: u32) -> u32 {
    let Some(st) = header(m, h) else { return 0 };
    if m.r32(h + 8) == 0 {
        return 0;
    }
    lock(m, e, st);
    let mut from = m.r32(st + 0x4c);
    let at = m.r32(h + 0xc);
    let mut r = m.r32(st + 0x58);
    loop {
        let next = m.r32(r + 0xc);
        if next == 0 || m.r32(next + 4) == 1 {
            break;
        }
        let start = m.r32(next + 0x120);
        if ring_in_range(from, start, at) != 0 {
            break;
        }
        r = next;
        from = start;
    }
    unlock(m, e, st);
    m.r32(r)
}

/// `Strm_GetRequestWord4` 0x2c7de8(handle, id): the request's state (0 if not queued)
pub fn get_request_state(m: &Mem, h: u32, id: u32) -> u32 {
    let Some(st) = header(m, h) else { return 0 };
    let r = lookup_request(m, st, id);
    if r == 0 {
        0
    } else {
        m.r32(r + 4)
    }
}
