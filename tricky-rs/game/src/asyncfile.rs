//! EA's async file layer (board row E2t) on the game's bytes: a table of 0x30-byte slots, a free list, and loads
//! driven by the file system's callbacks (open, size, 0x8000-byte reads, close).
//!
//! A slot: +0 its id (index in the low byte, a generation above it), +4 the next free slot, +8 bytes read, +0xc
//! being finished, +0x10 cancelled, +0x14 the buffer it allocated (1: to be allocated, 0: the caller's), +0x18 the
//! caller's callback, +0x1c the file system operation pending (0: none), +0x20 the file handle, +0x24 the offset,
//! +0x28 the bytes left (or the allocation's flags until the size is known), +0x2c where the next read goes.
//!
//! Calls out (the file system's queue, the heap through the pointers at 0x3bb958 / 0x3bb95c, the threads, the
//! callbacks) go through [`FileEnv`]. The mutex at 0x4039d8 (`Mutex_Lock` 0x2ceb00, `Mutex_Unlock` 0x2ceb60,
//! `Mutex_Init` 0x2cea98, `Mutex_Destroy` 0x2ceae0) is our host's: the port is single-threaded. Proven from the
//! disassembly of `SLUS_203.26` (read by hand: Ghidra was not running).

use crate::pf::Mem;

/// The calls out
pub trait FileEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

/// The slots (gp − 0x7fc4)
pub const TABLE: u32 = 0x3bb92c;
/// The heap's allocate and free, as pointers (gp − 0x7f98, gp − 0x7f94)
pub const ALLOC_PTR: u32 = 0x3bb958;
pub const FREE_PTR: u32 = 0x3bb95c;
/// The number of slots (gp − 0x7dd8)
pub const COUNT: u32 = 0x3bbb18;
/// The free list: first and last (gp − 0x7dd0, gp − 0x7dcc)
pub const FREE_LIST: u32 = 0x3bbb20;
/// The generation (gp − 0x7dc8)
pub const GENERATION: u32 = 0x3bbb28;
/// "ASYNCFILE", "ASYNCFILEBUF"
pub const TABLE_NAME: u32 = 0x3b9038;
pub const BUFFER_NAME: u32 = 0x3b9028;

pub const FS_WAIT_OP: u32 = 0x2ca628;
pub const FS_CANCEL_OP: u32 = 0x2ca5c0;
pub const FS_GET_RESULT: u32 = 0x2ca6f8;
pub const FS_SET_CALLBACK: u32 = 0x2ca410;
pub const FS_QUEUE_OPEN: u32 = 0x2ca8a8;
pub const FS_QUEUE_CLOSE: u32 = 0x2ca960;
pub const FS_QUEUE_READ: u32 = 0x2caa08;
pub const FS_GET_FILE_SIZE: u32 = 0x2caba0;
pub const THREAD_IS_CURRENT: u32 = 0x2cf200;
pub const RUN_POLL_TASKS: u32 = 0x2ced00;
pub const THREAD_SLEEP_MS: u32 = 0x2cf1a8;

/// The file system's callbacks, by address
pub const ON_CLOSED: u32 = 0x2c5898;
pub const ON_CHUNK_THEN_CLOSE: u32 = 0x2c58d0;
pub const ON_SIZED: u32 = 0x2c59c0;
pub const ON_OPENED: u32 = 0x2c5a98;
pub const ON_CHUNK: u32 = 0x2c5b98;

const SLOT: u32 = 0x30;
const CHUNK: i32 = 0x8000;

/// `AsyncFile_ListAppend` 0x2c5660(list, slot): at the end of the list (first, last)
pub fn list_append(m: &mut Mem, list: u32, slot: u32) {
    if m.r32(list) == 0 {
        m.w32(list, slot);
    } else {
        let last = m.r32(list + 4);
        m.w32(last + 4, slot);
    }
    m.w32(list + 4, slot);
    m.w32(slot + 4, 0);
}

/// `AsyncFile_AllocSlot` 0x2c56c8(list): the first slot taken off the list, or 0
pub fn alloc_slot(m: &mut Mem, list: u32) -> u32 {
    let s = m.r32(list);
    if s != 0 {
        m.w32(list, m.r32(s + 4));
    }
    s
}

/// `AsyncFile_ResetSlot` 0x2c5730(slot): the next generation (0x100 more, never 0) above the slot's index
pub fn reset_slot(m: &mut Mem, slot: u32) {
    let mut g = m.r32(GENERATION).wrapping_add(0x100);
    if g == 0 {
        g = 0x100;
    }
    m.w32(GENERATION, g);
    m.w32(slot, m.r8(slot) as u32 | g);
}

/// `AsyncFile_FindSlot` 0x2c5760(id): the slot with this id, or 0
pub fn find_slot(m: &Mem, id: u32) -> u32 {
    let i = id & 0xff;
    if (id as i32) < 0x100 || i as i32 >= m.r32(COUNT) as i32 {
        return 0;
    }
    let s = m.r32(TABLE).wrapping_add(i * SLOT);
    if m.r32(s) == id {
        s
    } else {
        0
    }
}

/// `AsyncFile_FreeSlot` 0x2c57a8(slot): a cancelled load's own buffer given back, the id cut to its index, the
/// slot put back on the free list
pub fn free_slot(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    if m.r32(slot + 0x10) != 0 && m.r32(slot + 0x14) >= 2 {
        let f = m.r32(FREE_PTR);
        e.call(m, f, &[m.r32(slot + 0x14)]);
    }
    m.w32(slot + 0x1c, 0);
    m.w32(slot, m.r8(slot) as u32);
    list_append(m, FREE_LIST, slot);
}

/// `AsyncFile_Complete` 0x2c5808(slot): nothing pending; a cancelled slot is freed unless being finished, else the
/// caller's callback (if any) gets the id
pub fn complete(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    m.w32(slot + 0x1c, 0);
    let (cancelled, cb, finishing) = (m.r32(slot + 0x10), m.r32(slot + 0x18), m.r32(slot + 0xc));
    if cancelled != 0 {
        if finishing == 0 {
            free_slot(m, e, slot);
        }
    } else if cb != 0 {
        e.call(m, cb, &[m.r32(slot)]);
    }
}

/// The operation just queued kept at +0x1c, and its callback set when there is one
fn queued(m: &mut Mem, e: &mut dyn FileEnv, slot: u32, op: u32, cb: u32) {
    m.w32(slot + 0x1c, op);
    if m.r32(slot + 0x1c) != 0 {
        e.call(m, FS_SET_CALLBACK, &[m.r32(slot + 0x1c), cb]);
    }
}

fn close(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    let op = e.call(m, FS_QUEUE_CLOSE, &[m.r32(slot + 0x20), 0x63, slot]);
    queued(m, e, slot, op, ON_CLOSED);
}

/// The next read: `n` more bytes in, at most 0x8000 of what is left
fn read_on(m: &mut Mem, e: &mut dyn FileEnv, slot: u32, n: u32, cb: u32) {
    let left = m.r32(slot + 0x28).wrapping_sub(n);
    let off = m.r32(slot + 0x24).wrapping_add(n);
    m.w32(slot + 0x28, left);
    m.w32(slot + 0x24, off);
    let size = if CHUNK < left as i32 { CHUNK as u32 } else { left };
    let op = e.call(m, FS_QUEUE_READ, &[m.r32(slot + 0x20), off, m.r32(slot + 0x2c), size, 0x63, slot]);
    queued(m, e, slot, op, cb);
}

/// The bytes a read brought: the buffer moves on, the count goes up; answers whether a whole chunk came
fn took(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) -> u32 {
    let n = e.call(m, FS_GET_RESULT, &[m.r32(slot + 0x1c)]);
    m.w32(slot + 0x2c, m.r32(slot + 0x2c).wrapping_add(n));
    m.w32(slot + 8, m.r32(slot + 8).wrapping_add(n));
    n
}

/// `AsyncFile_OnClosed` 0x2c5898(_, _, slot)
pub fn on_closed(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    e.call(m, FS_GET_RESULT, &[m.r32(slot + 0x1c)]);
    complete(m, e, slot);
}

/// `AsyncFile_OnChunkThenClose` 0x2c58d0(_, _, slot): a whole chunk and not cancelled reads on, else the file
/// is closed
pub fn on_chunk_then_close(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    let n = took(m, e, slot);
    if 0x7fff < n as i32 && m.r32(slot + 0x10) == 0 {
        read_on(m, e, slot, n, ON_CHUNK_THEN_CLOSE);
    } else {
        close(m, e, slot);
    }
}

/// `AsyncFile_OnSized` 0x2c59c0(_, _, slot): cancelled, the file closed; else a buffer of the size
/// ("ASYNCFILEBUF", the flags kept at +0x28) and the first read
pub fn on_sized(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    let size = e.call(m, FS_GET_RESULT, &[m.r32(slot + 0x1c)]);
    if m.r32(slot + 0x10) != 0 {
        close(m, e, slot);
        return;
    }
    let flags = m.r32(slot + 0x28);
    m.w32(slot + 0x28, 0x7fff_ffff);
    let f = m.r32(ALLOC_PTR);
    let buf = e.call(m, f, &[BUFFER_NAME, size, flags]);
    m.w32(slot + 0x14, buf);
    m.w32(slot + 0x2c, buf);
    let op = e.call(m, FS_QUEUE_READ, &[m.r32(slot + 0x20), m.r32(slot + 0x24), buf, CHUNK as u32, 0x63, slot]);
    queued(m, e, slot, op, ON_CHUNK_THEN_CLOSE);
}

/// `AsyncFile_OnOpened` 0x2c5a98(_, _, slot): no handle completes; cancelled closes; a buffer to allocate asks
/// the size; else the first read into the caller's buffer
pub fn on_opened(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    let h = e.call(m, FS_GET_RESULT, &[m.r32(slot + 0x1c)]);
    m.w32(slot + 0x20, h);
    if h == 0 {
        complete(m, e, slot);
    } else if m.r32(slot + 0x10) != 0 {
        close(m, e, slot);
    } else if m.r32(slot + 0x14) != 0 {
        let op = e.call(m, FS_GET_FILE_SIZE, &[h, 0x63, slot]);
        queued(m, e, slot, op, ON_SIZED);
    } else {
        let left = m.r32(slot + 0x28);
        let size = if CHUNK < left as i32 { CHUNK as u32 } else { left };
        let op = e.call(m, FS_QUEUE_READ, &[h, m.r32(slot + 0x24), m.r32(slot + 0x2c), size, 0x63, slot]);
        queued(m, e, slot, op, ON_CHUNK_THEN_CLOSE);
    }
}

/// `AsyncFile_OnChunk` 0x2c5b98(_, _, slot): `StartRead`'s reads: a whole chunk and not cancelled reads on, else
/// complete (the file stays open)
pub fn on_chunk(m: &mut Mem, e: &mut dyn FileEnv, slot: u32) {
    let n = took(m, e, slot);
    if 0x7fff < n as i32 && m.r32(slot + 0x10) == 0 {
        read_on(m, e, slot, n, ON_CHUNK);
    } else {
        complete(m, e, slot);
    }
}

/// `AsyncFile_Init` 0x2c5c60(count, flags): the table ("ASYNCFILE", `count` × 0x30 bytes) and every slot on the
/// free list, in order; nothing when there is a table or more than 0x100 slots
pub fn init(m: &mut Mem, e: &mut dyn FileEnv, count: u32, flags: u32) {
    if m.r32(TABLE) != 0 || 0x101 <= count as i32 {
        return;
    }
    let bytes = count.wrapping_mul(SLOT);
    m.w32(COUNT, count);
    let f = m.r32(ALLOC_PTR);
    let t = e.call(m, f, &[TABLE_NAME, bytes, flags]);
    m.w32(FREE_LIST + 4, t.wrapping_add(bytes).wrapping_sub(SLOT));
    m.w32(TABLE, t);
    m.w32(FREE_LIST, t);
    for i in 0..count as i32 {
        let s = t.wrapping_add(i as u32 * SLOT);
        m.w32(s, i as u32);
        m.w32(s + 4, s + SLOT);
        m.w32(s + 0x1c, 0);
    }
    m.w32(m.r32(TABLE).wrapping_add(bytes).wrapping_sub(0x2c), 0);
}

/// `AsyncFile_CancelAll` 0x2c5d30: every slot cancelled, then waited on (the poll tasks run when on their thread,
/// a sleep) until none has an operation pending; the table given back
pub fn cancel_all(m: &mut Mem, e: &mut dyn FileEnv) {
    if m.r32(TABLE) == 0 {
        return;
    }
    let mut i = 0;
    while (i as i32) < m.r32(COUNT) as i32 {
        let id = m.r32(m.r32(TABLE).wrapping_add(i * SLOT));
        cancel(m, e, id);
        i += 1;
    }
    loop {
        let count = m.r32(COUNT) as i32;
        let t = m.r32(TABLE);
        let busy = (0..count.max(0) as u32).any(|i| m.r32(t.wrapping_add(i * SLOT) + 0x1c) != 0);
        if !busy {
            break;
        }
        if e.call(m, THREAD_IS_CURRENT, &[0]) != 0 {
            e.call(m, RUN_POLL_TASKS, &[0]);
        }
        e.call(m, THREAD_SLEEP_MS, &[0]);
    }
    let f = m.r32(FREE_PTR);
    e.call(m, f, &[m.r32(TABLE)]);
    m.w32(TABLE, 0);
}

/// A new slot for a load: its id, fields cleared
fn take(m: &mut Mem) -> Option<u32> {
    let s = alloc_slot(m, FREE_LIST);
    if s == 0 {
        return None;
    }
    reset_slot(m, s);
    Some(s)
}

/// `AsyncFile_Load` 0x2c5e20(path, flags): opened, its size asked, a buffer allocated with `flags`, read whole;
/// the id, or 0 (no slot, or the open not queued: that slot is not given back)
pub fn load(m: &mut Mem, e: &mut dyn FileEnv, path: u32, flags: u32) -> u32 {
    let Some(s) = take(m) else { return 0 };
    m.w32(s + 0x28, flags);
    for o in [8, 0xc, 0x10, 0x18, 0x24] {
        m.w32(s + o, 0);
    }
    m.w32(s + 0x14, 1);
    let op = e.call(m, FS_QUEUE_OPEN, &[path, 1, 0x64, s]);
    queued(m, e, s, op, ON_OPENED);
    if m.r32(s + 0x1c) != 0 {
        m.r32(s)
    } else {
        0
    }
}

/// `AsyncFile_LoadInto` 0x2c5ed0(path, buffer, size): read into the caller's buffer, at most `size` bytes (none
/// or negative: no limit)
pub fn load_into(m: &mut Mem, e: &mut dyn FileEnv, path: u32, buf: u32, size: u32) -> u32 {
    let Some(s) = take(m) else { return 0 };
    for o in [8, 0xc, 0x10, 0x14, 0x18, 0x24] {
        m.w32(s + o, 0);
    }
    m.w32(s + 0x28, if (size as i32) > 0 { size } else { 0x7fff_ffff });
    m.w32(s + 0x2c, buf);
    let op = e.call(m, FS_QUEUE_OPEN, &[path, 1, 0x64, s]);
    queued(m, e, s, op, ON_OPENED);
    if m.r32(s + 0x1c) != 0 {
        m.r32(s)
    } else {
        0
    }
}

/// `AsyncFile_StartRead` 0x2c5f98(handle, offset, buffer, size): a read on an open file
pub fn start_read(m: &mut Mem, e: &mut dyn FileEnv, h: u32, off: u32, buf: u32, size: u32) -> u32 {
    let Some(s) = take(m) else { return 0 };
    for o in [8, 0xc, 0x10, 0x14, 0x18] {
        m.w32(s + o, 0);
    }
    m.w32(s + 0x20, h);
    m.w32(s + 0x24, off);
    m.w32(s + 0x28, size);
    m.w32(s + 0x2c, buf);
    let n = if CHUNK < size as i32 { CHUNK as u32 } else { size };
    let op = e.call(m, FS_QUEUE_READ, &[h, off, buf, n, 0x64, s]);
    queued(m, e, s, op, ON_CHUNK);
    if m.r32(s + 0x1c) != 0 {
        m.r32(s)
    } else {
        0
    }
}

/// `AsyncFile_SetCallback` 0x2c6070(id, fn): kept unless there is one already; called at once when the load has
/// nothing pending
pub fn set_callback(m: &mut Mem, e: &mut dyn FileEnv, id: u32, f: u32) {
    let s = find_slot(m, id);
    if s == 0 || m.r32(s + 0x18) != 0 {
        return;
    }
    let done = m.r32(s + 0x1c) == 0;
    m.w32(s + 0x18, f);
    if done {
        e.call(m, f, &[id]);
    }
}

/// `AsyncFile_IsDone` 0x2c6108(id): 1 with nothing pending, 0 while something is, −1 for no such load
pub fn is_done(m: &Mem, id: u32) -> u32 {
    match find_slot(m, id) {
        0 => u32::MAX,
        s => (m.r32(s + 0x1c) == 0) as u32,
    }
}

/// `AsyncFile_Finish` 0x2c6160(id, &buffer, &size): waits on what is pending, gives the buffer and the bytes read
/// (0, 0 when cancelled meanwhile) and frees the slot; 1, or −1 for no such load or a cancelled one
pub fn finish(m: &mut Mem, e: &mut dyn FileEnv, id: u32, buf_out: u32, size_out: u32) -> u32 {
    let s = find_slot(m, id);
    let mut cancelled = 0;
    if s != 0 {
        cancelled = m.r32(s + 0x10);
        if cancelled == 0 {
            m.w32(s + 0xc, 1);
        }
    }
    if s == 0 || cancelled != 0 {
        return u32::MAX;
    }
    while m.r32(s + 0x1c) != 0 {
        e.call(m, FS_WAIT_OP, &[m.r32(s + 0x1c)]);
    }
    if m.r32(s + 0x10) != 0 {
        if buf_out != 0 {
            m.w32(buf_out, 0);
        }
        if size_out != 0 {
            m.w32(size_out, 0);
        }
        free_slot(m, e, s);
        return u32::MAX;
    }
    if buf_out != 0 {
        m.w32(buf_out, m.r32(s + 0x2c).wrapping_sub(m.r32(s + 8)));
    }
    if size_out != 0 {
        m.w32(size_out, m.r32(s + 8));
    }
    free_slot(m, e, s);
    1
}

/// `AsyncFile_Cancel` 0x2c6278(id): marks the load cancelled (not one being finished with nothing pending); the
/// pending operation cancelled, or the slot freed when none; 1, or −1 (no such load, cancelled already, or not
/// marked)
pub fn cancel(m: &mut Mem, e: &mut dyn FileEnv, id: u32) -> u32 {
    let s = find_slot(m, id);
    let (mut op, mut was, mut finishing) = (0, 0, 0);
    if s != 0 {
        op = m.r32(s + 0x1c);
        was = m.r32(s + 0x10);
        finishing = m.r32(s + 0xc);
        if op != 0 || finishing == 0 {
            m.w32(s + 0x10, 1);
        }
    }
    if s == 0 || was != 0 || m.r32(s + 0x10) == 0 {
        return u32::MAX;
    }
    if op != 0 {
        e.call(m, FS_CANCEL_OP, &[op]);
    } else if finishing == 0 {
        free_slot(m, e, s);
    }
    1
}
