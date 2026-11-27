//! EA's file loads that wait (board row E2u) on the game's bytes: the sync open / read / write / close / size
//! (each a file system operation queued, waited on, its result taken), reads in chunks of 0x8000 bytes, and the
//! loads (`File_LoadAlloc`, `File_LoadInto`, `File_OpenBig`, `File_Load` with unpacking, ...).
//!
//! Most loads are a callback run on the file thread with the heap (`FileSys_RunWithHeap` 0x2cb190: the callback, a
//! word from `FileSys_QueueUnsupported` 0x2cebb0 (always 0), the word at 0x3bb930, a block of arguments on the
//! caller's stack); the callback gets (a number, the block). The block: +0 the path, +4 the buffer, +8 the flags or
//! size, +0xc 1 (0 in the `…0` forms). Locals live on the game's stack, so these take `sp`, the stack pointer on
//! entry, and keep the game's frames.
//!
//! Calls out (the file system's queue and waits, the heap through the pointers at 0x3bb958 / 0x3bb95c, the thread
//! runner) go through [`LoadEnv`]. Proven from the disassembly of `SLUS_203.26` (read by hand: Ghidra was not
//! running). `Host_WriteFile` 0x2c8708 and `Host_WriteFileChecked` 0x2c87e8 (writes to the development host,
//! through the kernel's file calls) are row E2y.

use crate::eamem;
use crate::pf::Mem;
use crate::shapelib;
use tricky_data::{big, compress};

/// The calls out
pub trait LoadEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

pub const RUN_WITH_HEAP: u32 = 0x2cb190;
/// The word `FileSys_RunWithHeap` and `File_Exists` are handed (gp − 0x7fc0)
pub const HEAP_WORD: u32 = 0x3bb930;
pub const ALLOC_PTR: u32 = 0x3bb958;
pub const FREE_PTR: u32 = 0x3bb95c;

pub const FS_WAIT_OP: u32 = 0x2ca628;
pub const FS_GET_STATUS: u32 = 0x2ca368;
pub const FS_GET_RESULT: u32 = 0x2ca6f8;
pub const FS_SET_CALLBACK: u32 = 0x2ca410;
pub const FS_OPEN: u32 = 0x2ca7f8;
pub const FS_QUEUE_OPEN: u32 = 0x2ca8a8;
pub const FS_QUEUE_CLOSE: u32 = 0x2ca960;
pub const FS_QUEUE_READ: u32 = 0x2caa08;
pub const FS_QUEUE_WRITE: u32 = 0x2caae0;
pub const FS_GET_FILE_SIZE: u32 = 0x2caba0;
pub const FS_QUEUE_MOUNT_BIG: u32 = 0x2cae70;
pub const FS_QUEUE_UNMOUNT_BIG: u32 = 0x2cafd8;

/// The callbacks, by address
pub const READ_CHUNK_CB: u32 = 0x2c8be8;
pub const GET_SIZE_CB: u32 = 0x2c7e58;
pub const LOAD_ALLOC_CB: u32 = 0x2c7f60;
pub const LOAD_ALLOC_SIZE_CB: u32 = 0x2c80b8;
pub const LOAD_INTO_CB: u32 = 0x2c8258;
pub const LOAD_MANY_CB: u32 = 0x2c8398;
pub const OPEN_BIG_CB: u32 = 0x2c8508;

const CHUNK: i32 = 0x8000;
/// `OpenBig`'s first read: enough for most archives' directories
const BIG_FIRST: u32 = 0xa90;

fn alloc(m: &mut Mem, e: &mut dyn LoadEnv, name: u32, size: u32, flags: u32) -> u32 {
    let f = m.r32(ALLOC_PTR);
    e.call(m, f, &[name, size, flags])
}

fn free(m: &mut Mem, e: &mut dyn LoadEnv, p: u32) {
    let f = m.r32(FREE_PTR);
    e.call(m, f, &[p]);
}

/// `FileSys_QueueUnsupported` 0x2cebb0: 0
pub fn queue_unsupported() -> u32 {
    0
}

/// The sync form: queued (last argument 0), waited on, its result
fn sync(m: &mut Mem, e: &mut dyn LoadEnv, queue: u32, args: &[u32]) -> u32 {
    let mut a = args.to_vec();
    a.push(0);
    let op = e.call(m, queue, &a);
    if op == 0 {
        return 0;
    }
    e.call(m, FS_WAIT_OP, &[op]);
    e.call(m, FS_GET_RESULT, &[op])
}

/// The sync open form: 1 when the operation ended with status 1, its result at `out` (0 when not queued)
fn sync_open(m: &mut Mem, e: &mut dyn LoadEnv, queue: u32, a: u32, b: u32, c: u32, out: u32) -> u32 {
    let op = e.call(m, queue, &[a, b, c, 0]);
    if op == 0 {
        m.w32(out, 0);
        return 0;
    }
    e.call(m, FS_WAIT_OP, &[op]);
    let ok = (e.call(m, FS_GET_STATUS, &[op]) == 1) as u32;
    let r = e.call(m, FS_GET_RESULT, &[op]);
    m.w32(out, r);
    ok
}

/// `File_OpenSync` 0x2c8d98(path, mode, n, &handle)
pub fn open_sync(m: &mut Mem, e: &mut dyn LoadEnv, path: u32, mode: u32, n: u32, out: u32) -> u32 {
    sync_open(m, e, FS_QUEUE_OPEN, path, mode, n, out)
}

/// `FileSys_MountBig` 0x2c8f50(a, b, c, &out)
pub fn mount_big(m: &mut Mem, e: &mut dyn LoadEnv, a: u32, b: u32, c: u32, out: u32) -> u32 {
    sync_open(m, e, FS_QUEUE_MOUNT_BIG, a, b, c, out)
}

/// `File_CloseSync` 0x2c8e48(handle, n)
pub fn close_sync(m: &mut Mem, e: &mut dyn LoadEnv, h: u32, n: u32) -> u32 {
    sync(m, e, FS_QUEUE_CLOSE, &[h, n])
}

/// `File_SizeSync` 0x2c8ea0(handle, n)
pub fn size_sync(m: &mut Mem, e: &mut dyn LoadEnv, h: u32, n: u32) -> u32 {
    sync(m, e, FS_GET_FILE_SIZE, &[h, n])
}

/// `FileSys_OpenSync` 0x2c9018(path, n)
pub fn fs_open_sync(m: &mut Mem, e: &mut dyn LoadEnv, path: u32, n: u32) -> u32 {
    sync(m, e, FS_OPEN, &[path, n])
}

/// `FileSys_UnmountBig` 0x2c8fc0(a, n)
pub fn unmount_big(m: &mut Mem, e: &mut dyn LoadEnv, a: u32, n: u32) -> u32 {
    sync(m, e, FS_QUEUE_UNMOUNT_BIG, &[a, n])
}

/// `FileSys_SyncOp0` 0x2c8ef8 and `FileSys_SyncOp2` 0x2c9070: their queue is `FileSys_QueueUnsupported`, so 0
pub fn sync_unsupported() -> u32 {
    queue_unsupported()
}

/// `FileSys_Exists` 0x2c7e38(path): `FileSys_OpenSync`(path, the word at 0x3bb930)
pub fn exists(m: &mut Mem, e: &mut dyn LoadEnv, path: u32) -> u32 {
    let w = m.r32(HEAP_WORD);
    fs_open_sync(m, e, path, w)
}

/// `File_ReadChunkCb` 0x2c8be8(op, status, block): the read's bytes counted; status not 1, a short read or
/// nothing left ends it (+0xc left 0); else the next chunk queued
pub fn read_chunk_cb(m: &mut Mem, e: &mut dyn LoadEnv, op: u32, status: u32, b: u32) {
    let n = e.call(m, FS_GET_RESULT, &[op]);
    m.w32(b + 0x20, 0);
    if status != 1 {
        m.w32(b + 0xc, 0);
        return;
    }
    m.w32(b + 8, m.r32(b + 8).wrapping_add(n));
    m.w32(b + 0x18, m.r32(b + 0x18).wrapping_add(n));
    m.w32(b + 0x10, m.r32(b + 0x10).wrapping_add(n));
    if (n as i32) < m.r32(b + 0x14) as i32 {
        m.w32(b + 0xc, 0);
    } else {
        m.w32(b + 0xc, m.r32(b + 0xc).wrapping_sub(n));
    }
    let left = m.r32(b + 0xc);
    if left as i32 <= 0 {
        return;
    }
    m.w32(b + 0x14, if CHUNK < left as i32 { CHUNK as u32 } else { left });
    let q = m.r32(b + 0x1c);
    let args = [m.r32(b + 4), m.r32(b + 8), m.r32(b + 0x18), m.r32(b + 0x14), m.r32(b), b];
    let op = e.call(m, q, &args);
    m.w32(b + 0x20, op);
    if op != 0 {
        e.call(m, FS_SET_CALLBACK, &[op, READ_CHUNK_CB]);
    } else {
        m.w32(b + 0xc, 0);
    }
}

/// `File_ReadChunked` 0x2c8cf0(handle, offset, buffer, size, n, queue): reads of at most 0x8000 bytes through
/// `queue` (each next one queued by [`read_chunk_cb`]), waited on until nothing is left or pending; the bytes read.
/// Its block on the stack: +0 n, +4 handle, +8 offset, +0xc left, +0x10 read, +0x14 this chunk, +0x18 buffer, +0x1c
/// queue, +0x20 the operation.
#[allow(clippy::too_many_arguments)]
pub fn read_chunked(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, h: u32, off: u32, buf: u32, size: u32, n: u32, queue: u32) -> u32 {
    let b = sp - 0x40;
    m.w32(b, n);
    m.w32(b + 4, h);
    m.w32(b + 8, off);
    m.w32(b + 0xc, size);
    m.w32(b + 0x10, 0);
    m.w32(b + 0x14, size);
    m.w32(b + 0x18, buf);
    m.w32(b + 0x1c, queue);
    if CHUNK < m.r32(b + 0x14) as i32 {
        m.w32(b + 0x14, CHUNK as u32);
    }
    let op = e.call(m, queue, &[h, off, m.r32(b + 0x18), m.r32(b + 0x14), n, b]);
    m.w32(b + 0x20, op);
    if op != 0 {
        e.call(m, FS_SET_CALLBACK, &[op, READ_CHUNK_CB]);
        while m.r32(b + 0xc) != 0 || m.r32(b + 0x20) != 0 {
            e.call(m, FS_WAIT_OP, &[m.r32(b + 0x20)]);
        }
    }
    m.r32(b + 0x10)
}

/// `File_ReadSync` 0x2c8e08(handle, offset, buffer, size, n)
pub fn read_sync(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, h: u32, off: u32, buf: u32, size: u32, n: u32) -> u32 {
    read_chunked(m, e, sp - 0x10, h, off, buf, size, n, FS_QUEUE_READ)
}

/// `File_WriteSync` 0x2c8e28(handle, offset, buffer, size, n)
pub fn write_sync(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, h: u32, off: u32, buf: u32, size: u32, n: u32) -> u32 {
    read_chunked(m, e, sp - 0x10, h, off, buf, size, n, FS_QUEUE_WRITE)
}

/// A load run on the file thread: the block (path, buffer, flags, the fourth word) at the caller's `sp − frame`
fn run(m: &mut Mem, e: &mut dyn LoadEnv, cb: u32, b: u32) -> u32 {
    let t = queue_unsupported();
    let w = m.r32(HEAP_WORD);
    e.call(m, RUN_WITH_HEAP, &[cb, t, w, b])
}

/// `File_GetSizeCb` 0x2c7e58(n, block): opened, sized, closed (the size, 0 when it doesn't open)
pub fn get_size_cb(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, n: u32, b: u32) -> u32 {
    let h = sp - 0x40;
    if open_sync(m, e, m.r32(b), 1, n, h) == 0 {
        return 0;
    }
    let n = n.wrapping_sub(1);
    let size = size_sync(m, e, m.r32(h), n);
    close_sync(m, e, m.r32(h), n);
    size
}

/// `File_GetSize` 0x2c7ec8(path) (`File_GetSize0` 0x2c7f18 with `one` 0)
pub fn get_size(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, path: u32, one: u32) -> u32 {
    let b = sp - 0x30;
    m.w32(b + 0xc, one);
    m.w32(b, path);
    run(m, e, GET_SIZE_CB, b)
}

/// `File_LoadAllocCb` 0x2c7f60(n, block) and, with `keep_size`, `File_LoadAllocSizeCb` 0x2c80b8 (the size then
/// written to +8): opened, sized, a buffer of the size (path, size, flags +8), read whole, closed; the buffer or 0
fn load_alloc_cb_(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, frame: u32, n: u32, b: u32, keep_size: bool) -> u32 {
    let h = sp - frame;
    if open_sync(m, e, m.r32(b), 1, n, h) == 0 {
        return 0;
    }
    let n = n.wrapping_sub(1);
    let size = size_sync(m, e, m.r32(h), n);
    let buf = alloc(m, e, m.r32(b), size, m.r32(b + 8));
    if buf == 0 {
        close_sync(m, e, m.r32(h), n);
        return 0;
    }
    read_sync(m, e, h, m.r32(h), 0, buf, size, n);
    close_sync(m, e, m.r32(h), n);
    if keep_size {
        m.w32(b + 8, size);
    }
    buf
}

/// `File_LoadAllocCb` 0x2c7f60(n, block)
pub fn load_alloc_cb(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, n: u32, b: u32) -> u32 {
    load_alloc_cb_(m, e, sp, 0x50, n, b, false)
}

/// `File_LoadAllocSizeCb` 0x2c80b8(n, block)
pub fn load_alloc_size_cb(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, n: u32, b: u32) -> u32 {
    load_alloc_cb_(m, e, sp, 0x60, n, b, true)
}

/// `File_LoadAlloc` 0x2c8018(path, flags) (`File_LoadAlloc0` 0x2c8068 with `one` 0)
pub fn load_alloc(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, path: u32, flags: u32, one: u32) -> u32 {
    let b = sp - 0x30;
    m.w32(b + 8, flags);
    m.w32(b + 0xc, one);
    m.w32(b, path);
    run(m, e, LOAD_ALLOC_CB, b)
}

/// `File_LoadAllocSize` 0x2c8178(path, &size, flags) (`File_ReadWhole` 0x2c81e8 with `one` 0): the size given
/// back from +8 when asked
pub fn load_alloc_size(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, path: u32, size_out: u32, flags: u32, one: u32) -> u32 {
    let b = sp - 0x40;
    m.w32(b + 8, flags);
    m.w32(b, path);
    m.w32(b + 0xc, one);
    let r = run(m, e, LOAD_ALLOC_SIZE_CB, b);
    if size_out != 0 {
        m.w32(size_out, m.r32(b + 8));
    }
    r
}

/// `File_LoadIntoCb` 0x2c8258(n, block): opened, read into +4 (at most +8 bytes; none or negative: no limit),
/// closed; the bytes read
pub fn load_into_cb(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, n: u32, b: u32) -> u32 {
    let h = sp - 0x50;
    let mut size = m.r32(b + 8);
    if 0 >= size as i32 {
        size = 0x7fff_ffff;
    }
    if open_sync(m, e, m.r32(b), 1, n, h) == 0 {
        return 0;
    }
    let n = n.wrapping_sub(1);
    let r = read_sync(m, e, h, m.r32(h), 0, m.r32(b + 4), size, n);
    close_sync(m, e, m.r32(h), n);
    r
}

/// `File_LoadInto` 0x2c82f0(path, buffer, size) (`File_LoadInto0` 0x2c8348 with `one` 0)
pub fn load_into(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, path: u32, buf: u32, size: u32, one: u32) -> u32 {
    let b = sp - 0x30;
    m.w32(b + 4, buf);
    m.w32(b + 8, size);
    m.w32(b + 0xc, one);
    m.w32(b, path);
    run(m, e, LOAD_INTO_CB, b)
}

/// `File_LoadManyCb` 0x2c8398(n, block): each path of +4's list loaded (`File_LoadAlloc0`, flags +0xc) into +8's
/// list; 1 when all loaded
pub fn load_many_cb(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, b: u32) -> u32 {
    let mut all = 1;
    let mut i = 0u32;
    while (i as i32) < m.r32(b) as i32 {
        let path = m.r32(m.r32(b + 4).wrapping_add(4 * i));
        let r = load_alloc(m, e, sp - 0x50, path, m.r32(b + 0xc), 0);
        m.w32(m.r32(b + 8).wrapping_add(4 * i), r);
        if m.r32(m.r32(b + 8).wrapping_add(4 * i)) == 0 {
            all = 0;
        }
        i += 1;
    }
    all
}

/// `File_LoadMany` 0x2c8430(count, paths, buffers, flags) (`File_BatchLoad` 0x2c84a0 with `one` 0): 0 for no files
pub fn load_many(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, count: u32, paths: u32, bufs: u32, flags: u32, one: u32) -> u32 {
    if count as i32 <= 0 {
        return 0;
    }
    let b = sp - 0x40;
    m.w32(b + 8, bufs);
    m.w32(b + 0xc, flags);
    m.w32(b + 4, paths);
    m.w32(b, count);
    m.w32(b + 0x10, one);
    run(m, e, LOAD_MANY_CB, b)
}

/// `File_OpenBigCb` 0x2c8508(n, block): an archive's directory: its first 0xa90 bytes read (a buffer of path, flags
/// +8); not an archive, given back; a directory longer than that, a buffer of its size and the rest read into it;
/// the buffer or 0
pub fn open_big_cb(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, n: u32, b: u32) -> u32 {
    let h = sp - 0x70;
    if open_sync(m, e, m.r32(b), 1, n, h) == 0 {
        return 0;
    }
    let buf = alloc(m, e, m.r32(b), BIG_FIRST, m.r32(b + 8));
    let n = n.wrapping_sub(1);
    if buf == 0 {
        close_sync(m, e, m.r32(h), n);
        return 0;
    }
    read_sync(m, e, h, m.r32(h), 0, buf, BIG_FIRST, n);
    let head: Vec<u8> = (0..0x10).map(|k| m.r8(buf.wrapping_add(k))).collect();
    if big::identify(&head) == 0 {
        free(m, e, buf);
        close_sync(m, e, m.r32(h), n);
        return 0;
    }
    let size = big::header_size(&head) as u32;
    if size < BIG_FIRST + 1 {
        close_sync(m, e, m.r32(h), n);
        return buf;
    }
    let whole = alloc(m, e, m.r32(b), size, m.r32(b + 8));
    if whole == 0 {
        free(m, e, buf);
        close_sync(m, e, m.r32(h), n);
        return 0;
    }
    eamem::copy(m, whole, buf, BIG_FIRST);
    free(m, e, buf);
    read_sync(m, e, h, m.r32(h), BIG_FIRST, whole + BIG_FIRST, size - BIG_FIRST, n);
    close_sync(m, e, m.r32(h), n);
    whole
}

/// `File_OpenBig` 0x2c8668(path, flags) (`File_OpenBig0` 0x2c86b8 with `one` 0)
pub fn open_big(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, path: u32, flags: u32, one: u32) -> u32 {
    let b = sp - 0x30;
    m.w32(b + 8, flags);
    m.w32(b + 0xc, one);
    m.w32(b, path);
    run(m, e, OPEN_BIG_CB, b)
}

fn unpacked_size(m: &Mem, p: u32) -> u32 {
    let head: Vec<u8> = (0..6).map(|k| m.r8(p.wrapping_add(k))).collect();
    compress::unpacked_size(&head)
}

/// `File_PeekUnpackedSize` 0x2c88d8(path): the size its first 0x80 bytes (read onto the stack) say it unpacks
/// to, 0 when nothing is read; `File_PeekUnpackedSizeUnchecked` 0x2c8910 (`checked` false) asks even then
pub fn peek_unpacked_size(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, path: u32, checked: bool) -> u32 {
    let at = sp - 0x90;
    let r = load_into(m, e, at, path, at, 0x80, 0);
    if checked && r == 0 {
        return 0;
    }
    unpacked_size(m, at)
}

/// Unpacks `src` into a new buffer (path, its size, flags): the buffer, or 0 (none, or the codec refused it)
fn unpack(m: &mut Mem, e: &mut dyn LoadEnv, name: u32, src: u32, size: u32, flags: u32) -> u32 {
    let out = alloc(m, e, name, size, flags);
    if out != 0 && shapelib::decode(m, src, out) == 0 {
        free(m, e, out);
        return 0;
    }
    out
}

/// `File_LoadAndUnpack` 0x2c8938(path, flags) (`File_Load` 0x2c8a70, from `sp − 0x10`): read whole; a packed
/// file is copied to a buffer of flags ^ 0x10 and unpacked into one of `flags`; the buffer or 0
pub fn load_and_unpack(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, name: u32, flags: u32) -> u32 {
    let size_at = sp - 0x70;
    let buf = load_alloc_size(m, e, size_at, name, size_at, flags, 0);
    if buf == 0 {
        return 0;
    }
    let us = unpacked_size(m, buf);
    let size = m.r32(size_at);
    if size == 0 || us == 0 {
        return buf;
    }
    let tmp = alloc(m, e, name, size, flags ^ 0x10);
    if tmp == 0 {
        free(m, e, buf);
        return 0;
    }
    eamem::copy(m, tmp, buf, size);
    free(m, e, buf);
    let out = unpack(m, e, name, tmp, us, flags);
    free(m, e, tmp);
    out
}

/// `File_LoadUnpackInto` 0x2c8a90(path, dst, max) (`File_LoadUnpackInto2` 0x2c8bc8, from `sp − 0x10`): read whole
/// (flags 0x10), unpacked when packed, at most `max` bytes (none or negative: all) copied to `dst`; the bytes
/// copied, 0 when not read or not unpacked
pub fn load_unpack_into(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, name: u32, dst: u32, max: u32) -> u32 {
    let size_at = sp - 0x70;
    let mut buf = load_alloc_size(m, e, size_at, name, size_at, 0x10, 0);
    if buf == 0 {
        return 0;
    }
    let us = unpacked_size(m, buf);
    if m.r32(size_at) as i32 > 0 && us as i32 > 0 {
        let out = unpack(m, e, name, buf, us, 0x10);
        free(m, e, buf);
        if out == 0 {
            return 0;
        }
        m.w32(size_at, us);
        buf = out;
    }
    if max as i32 > 0 && (max as i32) < m.r32(size_at) as i32 {
        m.w32(size_at, max);
    }
    let n = m.r32(size_at);
    eamem::copy(m, dst, buf, n);
    free(m, e, buf);
    m.r32(size_at)
}

pub const SCE_OPEN: u32 = 0x30a7f0;
pub const SCE_CLOSE: u32 = 0x30aa78;
pub const SCE_WRITE: u32 = 0x30b0a0;
/// Points at "host0:" (gp − 0x7e98)
pub const HOST_PREFIX_PTR: u32 = 0x3bba58;

/// `Host_WriteFile` 0x2c8708 / `Host_WriteFileChecked` 0x2c87e8 (`checked`)(path, buffer, size): the path (with
/// "host0:" put first unless it has it) opened on the development host (`sceOpen`, flags 0x602: write, create,
/// truncate), written in pieces of at most 0x1000 bytes (one write even for no bytes), closed; 1 if opened. The
/// checked form answers 0 if a write fails (the file is then left open) or the close fails. `sp` is the stack
/// pointer on entry: the path is made on the game's frame.
pub fn host_write_file(m: &mut Mem, e: &mut dyn LoadEnv, sp: u32, path: u32, buf: u32, size: u32, checked: bool) -> u32 {
    let at = sp - 0x140;
    m.w8(at, 0);
    let host = m.r32(HOST_PREFIX_PTR);
    let n = crate::libc::strlen(m, host);
    if crate::libc::strncmp(m, path, host, n) != 0 {
        crate::libc::strcpy(m, at, host);
    }
    let len = crate::libc::strlen(m, at);
    crate::libc::strcpy(m, at + len, path);
    let fd = e.call(m, SCE_OPEN, &[at, 0x602]);
    if (fd as i32) < 0 {
        return 0;
    }
    let (mut buf, mut left) = (buf, size as i32);
    loop {
        let r = if left > 0x1000 {
            let r = e.call(m, SCE_WRITE, &[fd, buf, 0x1000]);
            left -= 0x1000;
            buf = buf.wrapping_add(0x1000);
            r
        } else {
            let r = e.call(m, SCE_WRITE, &[fd, buf, left as u32]);
            left = 0;
            r
        };
        if checked && (r as i32) < 0 {
            return 0;
        }
        if left <= 0 {
            break;
        }
    }
    let r = e.call(m, SCE_CLOSE, &[fd]);
    if checked && (r as i32) < 0 {
        return 0;
    }
    1
}
