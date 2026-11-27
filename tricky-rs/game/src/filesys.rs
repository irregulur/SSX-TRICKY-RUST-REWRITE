//! EA's file system queue (board row E2v) on the game's bytes: the pools of operations and handles, the queue
//! (sorted by priority, run one operation at a time), the opens, reads, writes, closes and sizes, the BIG archive
//! mounts and the root path.
//!
//! The state at 0x34eec0 (`S`): +0 operations, +4 handles, +8 the highest priority that may run (0xff; the heap
//! level of `FileSys_RunWithHeap`), +0xc operations queued, +0x10 callbacks running (no operation starts while one
//! does), +0x14 the lock, +0x20 the operation running, +0x24 the operations (0x30 bytes each), +0x28 the handles
//! (0x10c bytes each), +0x2c the queue, +0x30 the mounted archives, +0x38 the root path (0x100 bytes).
//!
//! An operation: +0 its id (bits 0..19 a number, 20..23 its kind, 24..31 its place in the pool), +4 cancelled,
//! +8 its status (0 pending, 1 done, −1 cancelled, −2 failed), +0xc its error, +0x10 its priority, +0x14 its
//! context, +0x18 / +0x1c / +0x20 its arguments (offset or mode, size, buffer), +0x24 its handle, +0x28 its
//! callback, +0x2c the next in the queue. Kinds: 1 allocated, 2 open, 3 close, 4 read, 5 write, 6 size, 7 a no-op,
//! 8 a test open (`FileSys_Open`), 9 mount an archive, 10 unmount it.
//!
//! A handle: +0 in use / the device's handle / the archive's record, +4 the size, +8 the offset in an archive,
//! +0xc the path (0x100 bytes). An archive's record (0x14 bytes, from the heap): +0 its directory, +4 the
//! directory's size, +8 its handle, +0xc the mount operation, +0x10 the next.
//!
//! Calls out (the lock, the devices, the heap through the pointers at 0x3bb958 / 0x3bb95c, the poll in
//! `FileSys_WaitOp`, callbacks other than the archive ones) go through [`FsEnv`]. Proven from the disassembly of
//! `SLUS_203.26` (read by hand: Ghidra was not running).

use crate::eamem;
use crate::pf::Mem;
use tricky_data::big;

/// The calls out
pub trait FsEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

pub const S: u32 = 0x34eec0;
const OPS: u32 = S + 0x24;
const HANDLES: u32 = S + 0x28;
const QUEUE: u32 = S + 0x2c;
const BIGS: u32 = S + 0x30;
pub const ROOT: u32 = S + 0x38;
const LOCK: u32 = S + 0x14;

pub const ALLOC_PTR: u32 = 0x3bb958;
pub const FREE_PTR: u32 = 0x3bb95c;
/// The next operation number (gp − 0x7f9c)
pub const OP_NUMBER: u32 = 0x3bb954;
/// The heap flags of an archive's directory (gp − 0x7f78)
pub const BIG_FLAGS: u32 = 0x3bb978;
/// Points at "cd:" (gp − 0x7fa4)
pub const CD_PREFIX_PTR: u32 = 0x3bb94c;
/// Points at "host0:" (gp − 0x7e98)
pub const HOST_PREFIX_PTR: u32 = 0x3bba58;
/// The device the root path names: 1 the disc, 2 the host (gp − 0x7e90)
pub const ROOT_DEVICE: u32 = 0x3bba60;
/// The devices there are (gp − 0x7e8c): bit 1 the host
pub const DEVICES: u32 = 0x3bba64;
/// Whether there is a disc (`CdFs_Mount` is called) and the size of a disc entry (0x349d48, 0x349d44)
pub const HAS_DISC: u32 = 0x349d48;
pub const DISC_ENTRY: u32 = 0x349d44;
/// "File Sys", "bigfile buf", "/"
pub const NAME_FILE_SYS: u32 = 0x3b9050;
pub const NAME_BIG_BUF: u32 = 0x3b9088;
pub const SLASH: u32 = 0x3bb950;

pub const MUTEX_INIT: u32 = 0x2cea98;
pub const MUTEX_LOCK: u32 = 0x2ceb00;
pub const MUTEX_UNLOCK: u32 = 0x2ceb60;
pub const POLL: u32 = 0x2ced00;
pub const CDFS_MOUNT: u32 = 0x2c9970;
pub const HOSTFS_INIT: u32 = 0x2cc2c0;
pub const OPEN_ON_DISC: u32 = 0x2cc4b8;
pub const HOSTFS_CLOSE: u32 = 0x2cc5a8;
pub const DEV_READ: u32 = 0x2cc600;
pub const HOSTFS_WRITE: u32 = 0x2cc730;
pub const DEV_GET_SIZE: u32 = 0x2cc7a0;

pub const COMPLETE_CURRENT: u32 = 0x2cb780;
pub const ON_BIG_HEADER_READ: u32 = 0x2caca8;
pub const ON_BIG_OPENED: u32 = 0x2cadc0;
pub const ON_BIG_CLOSED: u32 = 0x2cafa8;

const OP_SIZE: u32 = 0x30;
const HANDLE_SIZE: u32 = 0x10c;
const ID: u32 = 0xfffff;

fn kind(word: u32) -> u32 {
    word >> 20 & 0xf
}

/// The operation an id names (by its place only)
fn op_of(m: &Mem, id: u32) -> u32 {
    m.r32(OPS) + (id >> 24) * OP_SIZE
}

fn lock(m: &mut Mem, e: &mut dyn FsEnv) {
    e.call(m, MUTEX_LOCK, &[LOCK]);
}

fn unlock(m: &mut Mem, e: &mut dyn FsEnv) {
    e.call(m, MUTEX_UNLOCK, &[LOCK]);
}

fn alloc(m: &mut Mem, e: &mut dyn FsEnv, name: u32, size: u32, flags: u32) -> u32 {
    let f = m.r32(ALLOC_PTR);
    e.call(m, f, &[name, size, flags])
}

fn free(m: &mut Mem, e: &mut dyn FsEnv, p: u32) {
    let f = m.r32(FREE_PTR);
    e.call(m, f, &[p]);
}

/// A callback (id, status, context): the archive ones are ours
fn callback(m: &mut Mem, e: &mut dyn FsEnv, f: u32, id: u32, status: u32, ctx: u32) {
    match f {
        ON_BIG_HEADER_READ => on_big_header_read(m, e, id, status, ctx),
        ON_BIG_OPENED => on_big_opened(m, e, id, status, ctx),
        ON_BIG_CLOSED => on_big_closed(m, e, id, status, ctx),
        _ => {
            e.call(m, f, &[id, status, ctx]);
        }
    }
}

/// A zero-ended string's bytes (without the zero)
fn cstr(m: &Mem, mut a: u32) -> Vec<u8> {
    let mut v = vec![];
    loop {
        let c = m.r8(a);
        if c == 0 {
            return v;
        }
        v.push(c);
        a += 1;
    }
}

/// `FileSys_CalcMemSize` 0x2ca320(handles, disc entries, operations): 0 means the default (0x10, 0x3e8, 0x20)
pub fn calc_mem_size(m: &Mem, handles: u32, entries: u32, ops: u32) -> u32 {
    let h = if handles != 0 { handles } else { 0x10 };
    let o = if ops != 0 { ops } else { 0x20 };
    let n = if entries != 0 { entries } else { 0x3e8 };
    let entry = m.r32(DISC_ENTRY).wrapping_add(0xc);
    o.wrapping_mul(OP_SIZE).wrapping_add(h.wrapping_mul(HANDLE_SIZE)).wrapping_add(n.wrapping_mul(entry)).wrapping_add(0x40)
}

/// `FileSys_Init` 0x2ca170: 0 if already made; else its memory from the heap ("File Sys", flags 0x10) and
/// `FileSys_InitWithBuffer`
pub fn init(m: &mut Mem, e: &mut dyn FsEnv, handles: u32, entries: u32, ops: u32) -> u32 {
    if m.r32(S) != 0 {
        return 0;
    }
    let size = calc_mem_size(m, handles, entries, ops);
    let buf = alloc(m, e, NAME_FILE_SYS, size, 0x10);
    init_with_buffer(m, e, handles, entries, ops, buf)
}

/// `FileSys_InitWithBuffer` 0x2ca208: the pools in `buf` (zeroed), the disc's table after them (`CdFs_Mount`,
/// only with a disc), the host's files; 1 (0 if already made)
pub fn init_with_buffer(m: &mut Mem, e: &mut dyn FsEnv, handles: u32, entries: u32, ops: u32, buf: u32) -> u32 {
    let h = if handles != 0 { handles } else { 0x10 };
    let n = if entries != 0 { entries } else { 0x3e8 };
    let o = if ops != 0 { ops } else { 0x20 };
    if m.r32(S) != 0 {
        return 0;
    }
    m.w32(S + 8, 0xff);
    m.w32(S, o);
    e.call(m, MUTEX_INIT, &[LOCK]);
    m.w32(S + 4, h);
    m.w32(OPS, buf);
    let size = calc_mem_size(m, h, n, o);
    eamem::set(m, m.r32(OPS), 0, size);
    let handles_at = m.r32(OPS).wrapping_add(m.r32(S).wrapping_mul(OP_SIZE));
    m.w32(HANDLES, handles_at);
    let disc = if m.r32(HAS_DISC) != 0 {
        let at = m.r32(S + 4).wrapping_mul(HANDLE_SIZE).wrapping_add(handles_at);
        e.call(m, CDFS_MOUNT, &[n, at, COMPLETE_CURRENT])
    } else {
        0
    };
    e.call(m, HOSTFS_INIT, &[h, disc]);
    1
}

/// `FileSys_GetStatus` 0x2ca368: the operation's status, −3 if the id is stale
pub fn get_status(m: &Mem, id: u32) -> u32 {
    let op = op_of(m, id);
    if id & ID != m.r32(op) & ID {
        return -3i32 as u32;
    }
    m.r32(op + 8)
}

/// `FileSys_GetOpState` 0x2ca3b0: the operation's error
pub fn get_op_state(m: &Mem, id: u32) -> u32 {
    m.r32(op_of(m, id) + 0xc)
}

/// `FileSys_GetOpContext` 0x2ca3e0: the operation's context
pub fn get_op_context(m: &Mem, id: u32) -> u32 {
    m.r32(op_of(m, id) + 0x14)
}

/// `FileSys_SetCallback` 0x2ca410: called at once if the operation has ended
pub fn set_callback(m: &mut Mem, e: &mut dyn FsEnv, id: u32, cb: u32) {
    let op = op_of(m, id);
    m.w32(op + 0x28, cb);
    let status = m.r32(op + 8);
    if status != 0 {
        let ctx = m.r32(op + 0x14);
        callback(m, e, cb, id, status, ctx);
    }
}

/// Puts `op` in the queue after every operation of its priority or less; the operation it went after (0: first)
fn insert(m: &mut Mem, op: u32) {
    let mut prev = 0;
    let head = m.r32(QUEUE);
    m.w32(op + 0x2c, head);
    if head != 0 && (m.r32(op + 0x10) as i32) >= (m.r32(head + 0x10) as i32) {
        let p = m.r32(op + 0x10) as i32;
        loop {
            prev = m.r32(op + 0x2c);
            let next = m.r32(prev + 0x2c);
            m.w32(op + 0x2c, next);
            if next == 0 || p < m.r32(next + 0x10) as i32 {
                break;
            }
        }
    }
    if prev != 0 {
        m.w32(prev + 0x2c, op);
    } else {
        m.w32(QUEUE, op);
    }
}

/// `FileSys_SetPriority` 0x2ca460: a queued operation is moved to its new place
pub fn set_priority(m: &mut Mem, e: &mut dyn FsEnv, id: u32, prio: u32) {
    let op = op_of(m, id);
    if id & ID != m.r32(op) & ID {
        return;
    }
    lock(m, e);
    let old = m.r32(op + 0x10);
    m.w32(op + 0x10, prio);
    if (m.r32(S + 0xc) as i32) >= 2 && op != m.r32(S + 0x20) && m.r32(op + 8) == 0 && old != prio {
        // out of the queue (the last one stands in if it is not there)
        let mut prev = 0;
        let head = m.r32(QUEUE);
        let next = m.r32(op + 0x2c);
        if head != 0 && head != op {
            prev = head;
            loop {
                let v = m.r32(prev + 0x2c);
                if v == 0 || v == op {
                    break;
                }
                prev = v;
            }
        }
        if prev != 0 {
            m.w32(prev + 0x2c, next);
        } else {
            m.w32(QUEUE, next);
        }
        insert(m, op);
    }
    unlock(m, e);
}

/// `FileSys_CancelOp` 0x2ca5c0: marks the operation cancelled (not a close or an unmount: the id's kind)
pub fn cancel_op(m: &mut Mem, id: u32) {
    let op = op_of(m, id);
    if id & ID == m.r32(op) & ID {
        let k = kind(id);
        if k != 3 && k != 10 {
            m.w32(op + 4, 1);
        }
    }
}

/// `FileSys_WaitOp` 0x2ca628: polls (`Sys_RunPollTasks`(0)) until the operation ends; its status, −3 if the id
/// is or becomes stale
pub fn wait_op(m: &mut Mem, e: &mut dyn FsEnv, id: u32) -> u32 {
    let op = op_of(m, id);
    if id & ID != m.r32(op) & ID {
        return -3i32 as u32;
    }
    while m.r32(op + 8) == 0 {
        e.call(m, POLL, &[0]);
        let now = op_of(m, id);
        if id & ID != m.r32(now) & ID {
            return -3i32 as u32;
        }
    }
    m.r32(op + 8)
}

/// `FileSys_GetResult` 0x2ca6f8: by kind, done: the handle (open, mount), the status (close, 7, unmount), the
/// bytes (read, write), the size or the answer (size, test open); not done: 0. A failed or cancelled open or
/// mount gives its handle back. The operation is freed. (The id is not checked.)
pub fn get_result(m: &mut Mem, e: &mut dyn FsEnv, id: u32) -> u32 {
    let op = op_of(m, id);
    let status = m.r32(op + 8) as i32;
    let mut r = 0;
    if status == 1 {
        r = match kind(m.r32(op)) {
            2 | 9 => m.r32(op + 0x24),
            3 | 7 | 10 => m.r32(op + 8),
            4 | 5 => m.r32(op + 0x1c),
            6 | 8 => m.r32(op + 0x18),
            _ => 0,
        };
    } else if status < 0 && status >= -2 {
        let k = kind(m.r32(op));
        if k == 2 || k == 9 {
            free_handle(m, e, m.r32(op + 0x24));
        }
    }
    free_op(m, e, op);
    r
}

/// An operation of `kind` from the pool, with its priority and context
fn new_op(m: &mut Mem, e: &mut dyn FsEnv, kind: u32, prio: u32, ctx: u32) -> u32 {
    let op = alloc_op(m, e);
    m.w32(op + 0x14, ctx);
    m.w32(op + 0x10, prio);
    let w = m.r32(op);
    m.w32(op, w & 0xff0f_ffff | kind << 20);
    op
}

/// `FileSys_Open` 0x2ca7f8(path, priority, context): queues a test open (kind 8: the file is there; its handle
/// given back at once); the id
pub fn open(m: &mut Mem, e: &mut dyn FsEnv, path: u32, prio: u32, ctx: u32) -> u32 {
    let op = alloc_op(m, e);
    m.w32(op + 0x14, ctx);
    m.w32(op + 0x10, prio);
    let w = m.r32(op);
    m.w32(op, w & 0xff0f_ffff | 0x80_0000);
    m.w32(op + 0x18, 1);
    open_rest(m, e, op, path)
}

fn open_rest(m: &mut Mem, e: &mut dyn FsEnv, op: u32, path: u32) -> u32 {
    let h = alloc_handle(m, e);
    m.w32(op + 0x24, h);
    if h == 0 {
        m.w32(op + 0xc, 2);
    }
    let h = m.r32(op + 0x24);
    crate::libc::strncpy(m, h + 0xc, path, 0x100);
    run_queue(m, e, op);
    m.r32(op)
}

/// `FileSys_QueueOpen` 0x2ca8a8(path, mode, priority, context): the id
pub fn queue_open(m: &mut Mem, e: &mut dyn FsEnv, path: u32, mode: u32, prio: u32, ctx: u32) -> u32 {
    let op = alloc_op(m, e);
    m.w32(op + 0x14, ctx);
    m.w32(op + 0x18, mode);
    let w = m.r32(op);
    m.w32(op, w & 0xff0f_ffff | 0x20_0000);
    m.w32(op + 0x10, prio);
    open_rest(m, e, op, path)
}

/// `FileSys_QueueClose` 0x2ca960(handle, priority, context): error 3 for an archive's handle (cleared when it
/// runs); the id
pub fn queue_close(m: &mut Mem, e: &mut dyn FsEnv, handle: u32, prio: u32, ctx: u32) -> u32 {
    let op = new_op(m, e, 3, prio, ctx);
    m.w32(op + 0x24, handle);
    let mut b = m.r32(BIGS);
    while b != 0 {
        if m.r32(b + 8) == handle {
            m.w32(op + 0xc, 3);
            break;
        }
        b = m.r32(b + 0x10);
    }
    run_queue(m, e, op);
    m.r32(op)
}

/// `FileSys_QueueRead` 0x2caa08(handle, offset, buffer, size, priority, context): the size cut at the file's end
/// (error 6 without a handle, which the game then reads at 4); the id
#[allow(clippy::too_many_arguments)]
pub fn queue_read(m: &mut Mem, e: &mut dyn FsEnv, handle: u32, pos: u32, buf: u32, size: u32, prio: u32, ctx: u32) -> u32 {
    let op = new_op(m, e, 4, prio, ctx);
    if handle == 0 {
        m.w32(op + 0xc, 6);
    }
    m.w32(op + 0x24, handle);
    let end = m.r32(handle + 4);
    m.w32(op + 0x20, buf);
    m.w32(op + 0x18, pos);
    let n = if (end as i32) < pos.wrapping_add(size) as i32 { end.wrapping_sub(pos) } else { size };
    m.w32(op + 0x1c, n);
    run_queue(m, e, op);
    m.r32(op)
}

/// `FileSys_QueueWrite` 0x2caae0(handle, offset, buffer, size, priority, context): the id
#[allow(clippy::too_many_arguments)]
pub fn queue_write(m: &mut Mem, e: &mut dyn FsEnv, handle: u32, pos: u32, buf: u32, size: u32, prio: u32, ctx: u32) -> u32 {
    let op = new_op(m, e, 5, prio, ctx);
    if handle == 0 {
        m.w32(op + 0xc, 6);
    }
    m.w32(op + 0x24, handle);
    m.w32(op + 0x10, prio);
    m.w32(op + 0x1c, size);
    m.w32(op + 0x18, pos);
    m.w32(op + 0x20, buf);
    run_queue(m, e, op);
    m.r32(op)
}

/// `FileSys_GetFileSize` 0x2caba0(handle, priority, context): done at once (not queued) with the handle's size;
/// without a handle, error 6 and never done. The id
pub fn get_file_size(m: &mut Mem, e: &mut dyn FsEnv, handle: u32, prio: u32, ctx: u32) -> u32 {
    let op = new_op(m, e, 6, prio, ctx);
    if handle != 0 {
        m.w32(op + 8, 1);
        m.w32(op + 0x18, m.r32(handle + 4));
    } else {
        m.w32(op + 0xc, 6);
    }
    m.r32(op)
}

/// `FileSys_QueueOp7` 0x2cac38(_, priority, context): an operation that does nothing, queued; the id
pub fn queue_op7(m: &mut Mem, e: &mut dyn FsEnv, prio: u32, ctx: u32) -> u32 {
    let op = new_op(m, e, 7, prio, ctx);
    run_queue(m, e, op);
    m.r32(op)
}

/// `FileSys_OnBigHeaderRead` 0x2caca8(id, status, record): the directory read (the status is not looked at); if
/// it is longer than read, read again whole; else the archive goes first in the list and its mount runs. (Not an
/// archive: its buffer is freed and still looked at.)
pub fn on_big_header_read(m: &mut Mem, e: &mut dyn FsEnv, id: u32, _status: u32, rec: u32) {
    let op = op_of(m, id);
    let handle = m.r32(op + 0x24);
    let prio = m.r32(op + 0x10);
    m.w32(rec + 8, handle);
    let mount = m.r32(rec + 0xc);
    m.w32(mount + 0x24, handle);
    get_result(m, e, id);
    let head: Vec<u8> = (0..0x10).map(|k| m.r8(m.r32(rec) + k)).collect();
    if big::identify(&head) == 0 {
        free(m, e, m.r32(rec));
    }
    let size = big::header_size(&head) as u32;
    if (m.r32(rec + 4) as i32) < size as i32 {
        free(m, e, m.r32(rec));
        m.w32(rec + 4, size);
        let flags = m.r32(BIG_FLAGS);
        let buf = alloc(m, e, NAME_BIG_BUF, size, flags);
        m.w32(rec, buf);
        let id = queue_read(m, e, m.r32(rec + 8), 0, buf, size, prio, rec);
        set_callback(m, e, id, ON_BIG_HEADER_READ);
    } else {
        m.w32(rec + 0x10, m.r32(BIGS));
        m.w32(BIGS, rec);
        run_queue(m, e, m.r32(rec + 0xc));
    }
}

/// `FileSys_OnBigOpened` 0x2cadc0(id, status, record): opened, its first 0x800 bytes read; else the mount fails
/// (error 4)
pub fn on_big_opened(m: &mut Mem, e: &mut dyn FsEnv, id: u32, status: u32, rec: u32) {
    let prio = m.r32(op_of(m, id) + 0x10);
    let handle = get_result(m, e, id);
    if status != 1 {
        let mount = m.r32(rec + 0xc);
        m.w32(mount + 0xc, 4);
        run_queue(m, e, mount);
    } else {
        let id = queue_read(m, e, handle, 0, m.r32(rec), 0x800, prio, rec);
        set_callback(m, e, id, ON_BIG_HEADER_READ);
    }
}

/// `FileSys_QueueMountBig` 0x2cae70(path, heap flags, priority, context): an archive mounted already gives its
/// handle (the game answers the operation's address, not its id); else a record and a 0x800-byte buffer from the
/// heap, the archive opened (mode 1), `FileSys_OnBigOpened` next; the id
pub fn queue_mount_big(m: &mut Mem, e: &mut dyn FsEnv, path: u32, flags: u32, prio: u32, ctx: u32) -> u32 {
    let mut b = m.r32(BIGS);
    let op = new_op(m, e, 9, prio, ctx);
    while b != 0 {
        if crate::libc::strncmp(m, path, m.r32(b + 8) + 0xc, 0x100) == 0 {
            m.w32(op + 0x24, m.r32(b + 8));
            run_queue(m, e, op);
            return op;
        }
        b = m.r32(b + 0x10);
    }
    let rec = alloc(m, e, path, 0x14, flags);
    m.w32(rec + 0xc, op);
    m.w32(rec + 4, 0x800);
    let f = m.r32(BIG_FLAGS);
    let buf = alloc(m, e, NAME_BIG_BUF, 0x800, f);
    m.w32(rec, buf);
    let id = queue_open(m, e, path, 1, prio, rec);
    set_callback(m, e, id, ON_BIG_OPENED);
    m.r32(op)
}

/// `FileSys_OnBigClosed` 0x2cafa8(id, status, unmount): the close taken, the unmount runs
pub fn on_big_closed(m: &mut Mem, e: &mut dyn FsEnv, id: u32, _status: u32, unmount: u32) {
    get_result(m, e, id);
    run_queue(m, e, unmount);
}

/// `FileSys_QueueUnmountBig` 0x2cafd8(handle, priority, context): error 1 for a file in an archive, and fails
/// (still unmounting) while a file of the archive is open; the archive leaves the list (the game reads near 0 if
/// it is not there), its buffers are freed and its handle closed, `FileSys_OnBigClosed` next; the id
pub fn queue_unmount_big(m: &mut Mem, e: &mut dyn FsEnv, handle: u32, prio: u32, ctx: u32) -> u32 {
    let mut b = m.r32(BIGS);
    let op = new_op(m, e, 10, prio, ctx);
    let handles = m.r32(HANDLES);
    if m.r32(handle + 8) != 0 {
        m.w32(op + 0xc, 1);
    }
    let mut i = 0;
    let mut h = handles;
    while (i as i32) < m.r32(S + 4) as i32 {
        if h != 0 && m.r32(h + 8) != 0 && m.r32(m.r32(h) + 8) == handle {
            m.w32(op + 8, -2i32 as u32);
            m.w32(op + 0xc, 1);
            break;
        }
        i += 1;
        h = h.wrapping_add(HANDLE_SIZE);
    }
    let mut prev = 0;
    while b != 0 && m.r32(b + 8) != handle {
        prev = b;
        b = m.r32(b + 0x10);
    }
    if b == 0 {
        m.w32(op + 0xc, 1);
    }
    if prev != 0 {
        m.w32(prev + 0x10, m.r32(b + 0x10));
    } else {
        let first = m.r32(BIGS);
        m.w32(BIGS, m.r32(first + 0x10));
    }
    free(m, e, m.r32(b));
    free(m, e, b);
    let id = queue_close(m, e, handle, prio, op);
    set_callback(m, e, id, ON_BIG_CLOSED);
    m.r32(op)
}

/// `FileSys_RunWithHeap` 0x2cb190(func, _, level, arg): `func`(level, arg) with only operations up to `level`
/// running, then the queue run; its answer
pub fn run_with_heap(m: &mut Mem, e: &mut dyn FsEnv, func: u32, level: u32, arg: u32) -> u32 {
    let saved = m.r32(S + 8);
    m.w32(S + 8, level);
    let r = e.call(m, func, &[level, arg]);
    m.w32(S + 8, saved);
    run_queue(m, e, 0);
    r
}

/// `FileSys_SetHeap` 0x2cb1f0: the highest priority that may run, then the queue run
pub fn set_heap(m: &mut Mem, e: &mut dyn FsEnv, level: u32) {
    m.w32(S + 8, level);
    run_queue(m, e, 0);
}

/// `FileSys_SetRootPath` 0x2cb218 (`FileSys_SetRootThunk` 0x2cc328): a "cd:" path (refused, answering 1, when
/// only the host is there) or a "host0:" one (refused, 2, without the host) sets the root device; the path after
/// the prefix (or all of it) becomes the root, ending in '/' (unless it fills all 0x100 bytes). The answer is
/// whatever the last call left (the root, or its end).
pub fn set_root_path(m: &mut Mem, path: u32) -> u32 {
    let host = m.r32(HOST_PREFIX_PTR);
    let n = crate::libc::strlen(m, host);
    let mut path = path;
    let cd = m.r32(CD_PREFIX_PTR);
    if crate::libc::strncmp(m, path, cd, 3) == 0 {
        if m.r32(DEVICES) & 3 == 2 {
            return 1;
        }
        path += 3;
        m.w32(ROOT_DEVICE, 1);
    } else if crate::libc::strncmp(m, path, host, n) == 0 {
        if m.r32(DEVICES) & 2 == 0 {
            return 2;
        }
        path = path.wrapping_add(n);
        m.w32(ROOT_DEVICE, 2);
    }
    let r = crate::libc::strcpy(m, ROOT, path);
    if m.r8(ROOT + 0xff) != 0 {
        return r;
    }
    let len = crate::libc::strlen(m, ROOT);
    if (len as i32) <= 0 {
        return ROOT.wrapping_add(len);
    }
    if m.r8(ROOT + len - 1) == b'/' {
        return ROOT + len;
    }
    // strcat
    crate::libc::strcpy(m, ROOT + len, SLASH);
    ROOT
}

/// `FileSys_GetRootPath` 0x2cb300 (`FileSys_GetRootThunk` 0x2cc348): the root copied out (0x100 bytes)
pub fn get_root_path(m: &mut Mem, dst: u32) -> u32 {
    crate::libc::strncpy(m, dst, ROOT, 0x100)
}

/// The rules of a path: "archive|name" looks in that archive (4), "|name" in every archive (2), a plain name
/// on the device (1) and, with mode bit 0, also in every archive (3). The archive's name and the file's name.
fn split(name: &[u8], mode: u32) -> (u32, Vec<u8>, Vec<u8>) {
    if let Some(bar) = name.iter().position(|&c| c == b'|') {
        let rule = if name[0] == b'|' { 2 } else { 4 };
        let archive = if rule == 4 { name[..bar].to_vec() } else { vec![] };
        (rule, archive, name[bar + 1..].to_vec())
    } else if mode & 1 != 0 {
        (3, vec![], name.to_vec())
    } else {
        (1, vec![], vec![])
    }
}

/// `BIG_Lookup` on a mounted archive's directory: (offset, size). The directory is read to its end and up to
/// 0x100 bytes on, for a name that runs past it.
fn lookup(m: &Mem, dir: u32, name: &[u8]) -> Option<(u32, u32)> {
    let head: Vec<u8> = (0..0x10).map(|k| m.r8(dir + k)).collect();
    let size = big::header_size(&head) as u32;
    let mut b = vec![];
    for k in 0..size + 0x100 {
        let c = m.r8(dir + k);
        b.push(c);
        if k >= size && c == 0 {
            break;
        }
    }
    big::lookup_bytes(&b, Some(name), 0).map(|x| (x.offset, x.size))
}

/// The archives searched for `name` (in `archive` alone if it is named): the record, offset and size
fn find_in_bigs(m: &Mem, rule: u32, archive: &[u8], name: &[u8]) -> Option<(u32, u32, u32)> {
    let mut b = m.r32(BIGS);
    while b != 0 {
        if rule & 4 == 0 || cstr(m, m.r32(b + 8) + 0xc) == archive {
            if let Some((off, size)) = lookup(m, m.r32(b), name) {
                return Some((b, off, size));
            }
        }
        b = m.r32(b + 0x10);
    }
    None
}

/// `FileSys_RunQueue` 0x2cb328(op): queues `op` (if any) by priority; if nothing runs and no callback is running,
/// the first operation (if its priority may run) starts: cancelled ones end at once; opens look on the device
/// and in the archives (`split`), reads go to the device (an archive's file through the archive's handle), writes
/// to the host, sizes and the rest end at once.
pub fn run_queue(m: &mut Mem, e: &mut dyn FsEnv, op: u32) {
    lock(m, e);
    if op != 0 {
        insert(m, op);
        m.w32(S + 0xc, m.r32(S + 0xc).wrapping_add(1));
    }
    if m.r32(S + 0x20) != 0 || m.r32(S + 0x10) != 0 {
        unlock(m, e);
        return;
    }
    let mut cur = m.r32(QUEUE);
    if cur != 0 {
        if (m.r32(S + 8) as i32) < m.r32(cur + 0x10) as i32 {
            cur = 0;
        } else {
            m.w32(S + 0xc, m.r32(S + 0xc).wrapping_sub(1));
            m.w32(QUEUE, m.r32(cur + 0x2c));
        }
    }
    m.w32(S + 0x20, cur);
    unlock(m, e);
    if cur == 0 {
        return;
    }
    if m.r32(cur + 4) != 0 {
        complete_current(m, e, 0);
        return;
    }
    let k = kind(m.r32(cur));
    match k {
        2 | 8 => {
            let h = m.r32(cur + 0x24);
            let name = cstr(m, h + 0xc);
            let (rule, archive, file) = split(&name, m.r32(cur + 0x18));
            let mut found = 0;
            if rule & 1 != 0 {
                let mode = m.r32(cur + 0x18);
                if e.call(m, OPEN_ON_DISC, &[h + 0xc, mode, h]) != 0 {
                    found = 1;
                    if kind(m.r32(cur)) == 8 {
                        let d = m.r32(h);
                        e.call(m, HOSTFS_CLOSE, &[d]);
                    } else {
                        let d = m.r32(h);
                        let size = e.call(m, DEV_GET_SIZE, &[d]);
                        m.w32(h + 4, size);
                    }
                }
            }
            if found == 0 && rule & 6 != 0 {
                if let Some((b, off, size)) = find_in_bigs(m, rule, &archive, &file) {
                    found = 1;
                    m.w32(h, b);
                    m.w32(h + 4, size);
                    m.w32(h + 8, off);
                }
            }
            if kind(m.r32(cur)) == 8 {
                free_handle(m, e, m.r32(cur + 0x24));
                m.w32(cur + 0x18, found);
            }
            complete_current(m, e, found);
        }
        3 => {
            let h = m.r32(cur + 0x24);
            m.w32(cur + 0xc, 0);
            if h != 0 {
                if m.r32(h + 8) == 0 {
                    let d = m.r32(h);
                    let r = e.call(m, HOSTFS_CLOSE, &[d]);
                    m.w32(cur + 0xc, r);
                }
                free_handle(m, e, m.r32(cur + 0x24));
                m.w32(cur + 0x24, 0);
            }
            let ok = (m.r32(cur + 0xc) == 0) as u32;
            complete_current(m, e, ok);
        }
        4 => {
            let n = m.r32(cur + 0x1c);
            if n as i32 <= 0 {
                complete_current(m, e, 1);
                return;
            }
            let h = m.r32(cur + 0x24);
            let off = m.r32(h + 8);
            let (buf, pos) = (m.r32(cur + 0x20), m.r32(cur + 0x18));
            if off != 0 {
                let d = m.r32(m.r32(m.r32(h) + 8));
                e.call(m, DEV_READ, &[d, buf, off.wrapping_add(pos), n]);
            } else {
                let d = m.r32(h);
                e.call(m, DEV_READ, &[d, buf, pos, n]);
            }
        }
        5 => {
            let h = m.r32(cur + 0x24);
            let d = m.r32(h);
            let (buf, pos, n) = (m.r32(cur + 0x20), m.r32(cur + 0x18), m.r32(cur + 0x1c));
            e.call(m, HOSTFS_WRITE, &[d, buf, pos, n]);
        }
        6 => {
            let h = m.r32(cur + 0x24);
            m.w32(cur + 0x18, m.r32(h + 4));
            complete_current(m, e, 1);
        }
        7 | 9 | 10 => {
            let ok = (m.r32(cur + 0xc) == 0) as u32;
            complete_current(m, e, ok);
        }
        _ => {}
    }
}

/// `FileSys_CompleteCurrent` 0x2cb780(ok): the running operation ends (cancelled −1, else 1 or −2) and its
/// callback is called; then, with no callback running, the queue runs. (With none running, the count of
/// callbacks running stays one up.)
pub fn complete_current(m: &mut Mem, e: &mut dyn FsEnv, ok: u32) {
    m.w32(S + 0x10, m.r32(S + 0x10).wrapping_add(1));
    let cur = m.r32(S + 0x20);
    if cur == 0 {
        return;
    }
    let status = if m.r32(cur + 4) != 0 {
        -1i32 as u32
    } else if ok != 0 {
        1
    } else {
        -2i32 as u32
    };
    m.w32(cur + 8, status);
    m.w32(S + 0x20, 0);
    let cb = m.r32(cur + 0x28);
    if cb != 0 {
        let (ctx, id, st) = (m.r32(cur + 0x14), m.r32(cur), m.r32(cur + 8));
        callback(m, e, cb, id, st, ctx);
    }
    m.w32(S + 0x10, m.r32(S + 0x10).wrapping_sub(1));
    if m.r32(S + 0x10) == 0 {
        run_queue(m, e, 0);
    }
}

/// `FileSys_ParsePath` 0x2cb830(path, mode, out size): whether the file is there (on the device or in an
/// archive, by `split`'s rules) and its size (0 if not). `sp` is the stack pointer on entry: the device's handle
/// goes to the game's frame.
pub fn parse_path(m: &mut Mem, e: &mut dyn FsEnv, sp: u32, path: u32, mode: u32, out: u32) -> u32 {
    m.w32(out, 0);
    let name = cstr(m, path);
    let (rule, archive, file) = split(&name, mode);
    if rule & 1 != 0 {
        let at = sp - 0x280 + 0x200;
        if e.call(m, OPEN_ON_DISC, &[path, mode, at]) != 0 {
            let d = m.r32(at);
            let size = e.call(m, DEV_GET_SIZE, &[d]);
            let d = m.r32(at);
            m.w32(out, size);
            e.call(m, HOSTFS_CLOSE, &[d]);
            return 1;
        }
    }
    if rule & 6 != 0 {
        if let Some((_, _, size)) = find_in_bigs(m, rule, &archive, &file) {
            m.w32(out, size);
            return 1;
        }
    }
    0
}

/// `FileSys_AllocOp` 0x2cb9d0: the first free operation, kind 1, numbered (1 after 0xfffff). With none free the
/// game spins forever.
pub fn alloc_op(m: &mut Mem, e: &mut dyn FsEnv) -> u32 {
    lock(m, e);
    let mut i = 0;
    while (i as i32) < m.r32(S) as i32 {
        let op = m.r32(OPS) + i * OP_SIZE;
        let w = m.r32(op);
        if kind(w) == 0 {
            m.w32(op, w & 0xff0f_ffff | 0x10_0000);
            m.w8(op + 3, i as u8);
            let n = m.r32(OP_NUMBER);
            let w = m.r32(op);
            m.w32(op, w & 0xfff0_0000 | n & ID);
            let next = n.wrapping_add(1);
            m.w32(OP_NUMBER, if (ID as i32) < next as i32 { 1 } else { next });
            break;
        }
        i += 1;
    }
    unlock(m, e);
    if i == m.r32(S) {
        panic!("FileSys_AllocOp: no free operation (the game spins here forever)");
    }
    m.r32(OPS) + i * OP_SIZE
}

/// `FileSys_FreeOp` 0x2cbb18: zeroed
pub fn free_op(m: &mut Mem, e: &mut dyn FsEnv, op: u32) {
    lock(m, e);
    eamem::set(m, op, 0, OP_SIZE);
    unlock(m, e);
}

/// `FileSys_AllocHandle` 0x2cbb68: the first free handle, its first word 1. With none free the game spins
/// forever.
pub fn alloc_handle(m: &mut Mem, e: &mut dyn FsEnv) -> u32 {
    lock(m, e);
    let mut i = 0;
    while (i as i32) < m.r32(S + 4) as i32 {
        let h = m.r32(HANDLES) + i * HANDLE_SIZE;
        if m.r32(h) == 0 {
            m.w32(h, 1);
            break;
        }
        i += 1;
    }
    unlock(m, e);
    if i == m.r32(S + 4) {
        panic!("FileSys_AllocHandle: no free handle (the game spins here forever)");
    }
    m.r32(HANDLES) + i * HANDLE_SIZE
}

/// `FileSys_FreeHandle` 0x2cbc28: zeroed
pub fn free_handle(m: &mut Mem, e: &mut dyn FsEnv, h: u32) {
    lock(m, e);
    eamem::set(m, h, 0, HANDLE_SIZE);
    unlock(m, e);
}
