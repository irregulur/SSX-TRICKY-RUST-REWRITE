//! The async BIG file class (`cAsyncBigFile`, board row E2d) and the BIG archive's init, reset, open, close and
//! delete, the request table's clear and the read buffers' unpacking (row E2g). Every `cAsyncBigFile`
//! shares one table of 100 requests (0x5c bytes each at 0x343a58) and one lock (0x343a50, made by the first file and
//! destroyed by the last, counted at 0x343a54). A request:
//!
//! | at | what |
//! |---|---|
//! | +0 | the file that made it |
//! | +4 / +8 | the callback when its read ends, and the callback's first argument |
//! | +0xc | its state: 0 free, 1 waiting, 2 reading, 3 read |
//! | +0x10 | the word handed to the read (vf8) |
//! | +0x14 / +0x1c | the read's second answer and its buffer (both from `AsyncFile_Finish`) |
//! | +0x18 | the read's handle (`AsyncFile_Load`) |
//! | +0x20 | the unpacked data (vf12) |
//! | +0x24 | the file's name, as vf7 writes it (`|` first) |
//! | +0x58 | how many hold it |
//!
//! A file: +0 set while it exists (the destructor ends its reads only then), +8 set while a read runs when +0xc says
//! one at a time, +0x10 its table. Every call out goes through [`BigEnv`]; the C library's `strcmp`, `strcpy` and
//! `strcat` are run here.

use crate::pf::Mem;

/// The calls out
pub trait BigEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

pub const MEM_DELETE: u32 = 0x114f80;
pub const MEM_FREE: u32 = 0x14a040;
pub const MEMPOOL_ALLOC: u32 = 0x23d3b0;
pub const COMPRESS_DECODE: u32 = 0x2c2f90;
pub const COMPRESS_UNPACKED_SIZE: u32 = 0x2c3190;
pub const ASYNC_LOAD: u32 = 0x2c5e20;
pub const ASYNC_SET_CALLBACK: u32 = 0x2c6070;
pub const ASYNC_FINISH: u32 = 0x2c6160;
pub const ASYNC_CANCEL: u32 = 0x2c6278;
pub const EAMEM_SET: u32 = 0x2cc808;
pub const EAMEM_ALLOC: u32 = 0x2ccf70;
pub const EAMEM_FREE: u32 = 0x2ccfc0;
pub const EAMEM_GET_SIZE: u32 = 0x2cd9b0;
pub const EAMEM_COPY: u32 = 0x2ce114;
pub const MUTEX_CREATE: u32 = 0x2cf468;
pub const MUTEX_DESTROY: u32 = 0x2cf4b0;
pub const MUTEX_LOCK: u32 = 0x2cf4d0;
pub const MUTEX_UNLOCK: u32 = 0x2cf550;
/// `AsyncBigFile_FinishRead`, the read's callback
pub const FINISH_READ: u32 = 0x23beb0;
/// `cAsyncBigFile`'s table
pub const VT: u32 = 0x3a41a8;
/// The lock, how many files share it, the requests
pub const LOCK: u32 = 0x343a50;
pub const FILES: u32 = 0x343a54;
pub const REQUESTS: u32 = 0x343a58;
pub const REQUEST: u32 = 0x5c;
pub const COUNT: u32 = 100;
/// The memory pool flag 0x40000000 picks, and the allocations' name
pub const POOL: u32 = 0x345e48;
pub const NAME: u32 = 0x3a4110;
/// What fresh allocations are filled with when the flags' top bit is clear
pub const FILL: u32 = 0xdead_c0ed;

/// Request `i`'s address
pub fn req(i: u32) -> u32 {
    REQUESTS.wrapping_add(i.wrapping_mul(REQUEST))
}

/// The file's virtual at `off` (adjust) / `off + 4` (function) of its table (+0x10)
fn vcall(m: &mut Mem, e: &mut dyn BigEnv, f: u32, off: u32, rest: &[u32]) -> u32 {
    let vt = m.r32(f + 0x10);
    let me = f.wrapping_add(m.r16(vt + off) as i16 as u32);
    let mut a = vec![me];
    a.extend_from_slice(rest);
    e.call(m, m.r32(vt + off + 4), &a)
}

/// `strcpy` 0x2fd14c ([`crate::libc::strcpy`])
pub fn strcpy(m: &mut Mem, to: u32, from: u32) {
    crate::libc::strcpy(m, to, from);
}

/// `strcmp` 0x2fd008, as equal or not
fn str_eq(m: &Mem, a: u32, b: u32) -> bool {
    crate::libc::strcmp(m, a, b) == 0
}

/// `BigArchive_Init` 0x23bb70(a): reset; `a`
pub fn archive_init(m: &mut Mem, e: &mut dyn BigEnv, a: u32) -> u32 {
    archive_reset(m, e, a);
    a
}

/// `BigArchive_Reset` 0x23bbe8(a): +0x88 = −1, the 0x81 bytes from +4 cleared (`EAMem_Set`), +0 = 0
pub fn archive_reset(m: &mut Mem, e: &mut dyn BigEnv, a: u32) {
    m.w32(a + 0x88, u32::MAX);
    e.call(m, EAMEM_SET, &[a + 4, 0, 0x81]);
    m.w32(a, 0);
}

/// `FileSys_OpenSync`, `FileSys_MountBig`, `FileSys_UnmountBig`, `memset`
pub const FS_OPEN_SYNC: u32 = 0x2c9018;
pub const FS_MOUNT_BIG: u32 = 0x2c8f50;
pub const FS_UNMOUNT_BIG: u32 = 0x2c8fc0;
pub const MEMSET: u32 = 0x2fb468;
/// The memory flags word (0x3bb978) an archive's open sets to its flags for the while (what the word does is
/// inferred: `game_helpers::QUIET`)
pub const MEM_FLAGS: u32 = 0x3bb978;

/// `BigArchive_Delete` 0x23bb98(a, flags): closed; deleted with flag 1 (`Mem_Delete`)
pub fn archive_delete(m: &mut Mem, e: &mut dyn BigEnv, a: u32, flags: u32) {
    archive_close(m, e, a);
    if flags & 1 != 0 {
        e.call(m, MEM_DELETE, &[a]);
    }
}

/// `BigArchive_Open` 0x23bc28(a, name, flags): the name copied to +4, then with the memory flags word set to
/// `flags`: opened (`FileSys_OpenSync`(+4, 100)) and, when it is, mounted (`FileSys_MountBig`(+4, flags, 100,
/// +0x88)); +0 whether both went (0 or 1), the word put back; +0
pub fn archive_open(m: &mut Mem, e: &mut dyn BigEnv, a: u32, name: u32, flags: u32) -> u32 {
    strcpy(m, a + 4, name);
    let old = m.r32(MEM_FLAGS);
    m.w32(MEM_FLAGS, flags);
    let ok = e.call(m, FS_OPEN_SYNC, &[a + 4, 0x64]) != 0;
    m.w32(a, ok as u32);
    if ok {
        let ok = e.call(m, FS_MOUNT_BIG, &[a + 4, flags, 0x64, a + 0x88]) != 0;
        m.w32(a, ok as u32);
    }
    m.w32(MEM_FLAGS, old);
    m.r32(a)
}

/// `BigArchive_Close` 0x23bcc0(a): when open (+0), unmounted (`FileSys_UnmountBig`(+0x88, 100)); reset; whether
/// it unmounted (0 when not open)
pub fn archive_close(m: &mut Mem, e: &mut dyn BigEnv, a: u32) -> u32 {
    let mut ok = 0;
    if m.r32(a) != 0 {
        ok = (e.call(m, FS_UNMOUNT_BIG, &[m.r32(a + 0x88), 0x64]) != 0) as u32;
    }
    archive_reset(m, e, a);
    ok
}

/// `BigFile_ClearTable` 0x23be28: the request table zeroed (`memset`, 0x23f0 bytes: 100 requests of 0x5c)
pub fn clear_table(m: &mut Mem, e: &mut dyn BigEnv) {
    e.call(m, MEMSET, &[REQUESTS, 0, 0x23f0]);
}

/// `cAsyncBigFile::vf2` 0x23bea0 and `vf3` 0x23be98: nothing
pub fn vf2() {}
pub fn vf3() {}

/// `File_GetUnpackedSize` 0x23c7c8(f, i): request `i`'s read buffer's (+0x1c) unpacked size
/// (`Compress_UnpackedSizeApi`), or when it is not packed its size (`EAMem_GetSize`)
pub fn unpacked_size(m: &mut Mem, e: &mut dyn BigEnv, i: u32) -> u32 {
    let at = req(i).wrapping_add(0x1c);
    let n = e.call(m, COMPRESS_UNPACKED_SIZE, &[m.r32(at)]);
    if n != 0 {
        return n;
    }
    e.call(m, EAMEM_GET_SIZE, &[m.r32(at)])
}

/// `File_DecompressBuffer` 0x23c818(f, i, dst): request `i`'s read buffer (+0x1c) unpacked into `dst`
/// (`Compress_DecodeApi`) or, not packed, copied whole (`EAMem_GetSize`, `EAMem_Copy`); then freed (`EAMem_Free`)
/// and +0x1c, +0x20 cleared
pub fn decompress_buffer(m: &mut Mem, e: &mut dyn BigEnv, i: u32, dst: u32) {
    let at = req(i).wrapping_add(0x1c);
    if e.call(m, COMPRESS_UNPACKED_SIZE, &[m.r32(at)]) != 0 {
        e.call(m, COMPRESS_DECODE, &[m.r32(at), dst]);
    } else {
        let n = e.call(m, EAMEM_GET_SIZE, &[m.r32(at)]);
        e.call(m, EAMEM_COPY, &[dst, m.r32(at), n]);
    }
    e.call(m, EAMEM_FREE, &[m.r32(at)]);
    m.w32(at, 0);
    m.w32(req(i).wrapping_add(0x20), 0);
}

/// `cAsyncBigFile::cAsyncBigFile` 0x23bd10(f): its table, +0 = 1, +4 = 0, one more file on the lock (made by the
/// first), +0xc = 1, +8 = 0; `f`
pub fn construct(m: &mut Mem, e: &mut dyn BigEnv, f: u32) -> u32 {
    let n = m.r32(FILES);
    m.w32(f + 0x10, VT);
    m.w32(f, 1);
    m.w32(f + 4, 0);
    m.w32(f + 0xc, 0);
    let n = if n == 0 {
        let l = e.call(m, MUTEX_CREATE, &[]);
        m.w32(LOCK, l);
        m.r32(FILES)
    } else {
        n
    };
    m.w32(FILES, n.wrapping_add(1));
    m.w32(f + 0xc, 1);
    m.w32(f + 8, 0);
    f
}

/// `cAsyncBigFile::~cAsyncBigFile` 0x23bd90(f, how): its table, vf2 0x23bea0 (empty) when +0 is set, one file fewer
/// on the lock (destroyed and cleared with the last), deleted on bit 0
pub fn destruct(m: &mut Mem, e: &mut dyn BigEnv, f: u32, how: u32) {
    m.w32(f + 0x10, VT);
    let n = m.r32(FILES).wrapping_sub(1);
    m.w32(FILES, n);
    if n == 0 {
        let l = m.r32(LOCK);
        e.call(m, MUTEX_DESTROY, &[l]);
        m.w32(LOCK, 0);
    }
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[f]);
    }
}

/// `cAsyncBigFile::vf1` 0x23be90 and `vf4` 0x23bea8: nothing
pub fn vf1() {}
pub fn vf4() {}

/// `cAsyncBigFile::vf5` 0x23d080(f): +0
pub fn vf5(m: &Mem, f: u32) -> u32 {
    m.r32(f)
}

/// `AsyncBigFile_FinishRead` 0x23beb0(h) (`sp` the stack pointer on entry), the reads' callback: under the lock,
/// `AsyncFile_Finish`(h, two words on the stack, cleared first) and the request completed with them
pub fn finish_read(m: &mut Mem, e: &mut dyn BigEnv, h: u32, sp: u32) {
    let l = m.r32(LOCK);
    e.call(m, MUTEX_LOCK, &[l]);
    let at = sp.wrapping_sub(0x40);
    m.w32(at, 0);
    m.w32(at + 4, 0);
    e.call(m, ASYNC_FINISH, &[h, at, at | 4]);
    let (buf, n) = (m.r32(at), m.r32(at + 4));
    complete_request(m, e, h, buf, n);
    let l = m.r32(LOCK);
    e.call(m, MUTEX_UNLOCK, &[l]);
}

/// `AsyncFile_CompleteRequest` 0x23bf18(h, buf, n): each reading request (state 2) with the handle `h`: read
/// (state 3, handle cleared), `buf` at +0x1c and `n` at +0x14, its callback run with (+8, `buf`), then its file's
/// next read started (vf6, +8 cleared) when the file reads one at a time
pub fn complete_request(m: &mut Mem, e: &mut dyn BigEnv, h: u32, buf: u32, n: u32) {
    for i in 0..COUNT {
        let r = req(i);
        if m.r32(r + 0xc) != 2 || m.r32(r + 0x18) != h {
            continue;
        }
        m.w32(r + 0x18, 0);
        m.w32(r + 0xc, 3);
        m.w32(r + 0x14, n);
        let cb = m.r32(r + 4);
        m.w32(r + 0x1c, buf);
        m.w32(r + 0x18, 0);
        if cb != 0 {
            let a = m.r32(r + 8);
            e.call(m, cb, &[a, buf]);
        }
        let f = m.r32(r);
        if m.r32(f + 0xc) != 0 {
            m.w32(f + 8, 0);
            vcall(m, e, f, 0x38, &[]);
        }
    }
}

/// `cAsyncBigFile::vf8` 0x23c010(f, name, w): `AsyncFile_Load`(name, w) with `AsyncBigFile_FinishRead` as its
/// callback; the handle
pub fn vf8(m: &mut Mem, e: &mut dyn BigEnv, name: u32, w: u32) -> u32 {
    let h = e.call(m, ASYNC_LOAD, &[name, w]);
    e.call(m, ASYNC_SET_CALLBACK, &[h, FINISH_READ]);
    h
}

/// `cAsyncBigFile::vf9` 0x23c050(f, r): the request's read cancelled and finished with nothing
pub fn vf9(m: &mut Mem, e: &mut dyn BigEnv, r: u32) {
    let h = m.r32(r + 0x18);
    e.call(m, ASYNC_CANCEL, &[h]);
    let h = m.r32(r + 0x18);
    e.call(m, ASYNC_FINISH, &[h, 0, 0]);
}

/// `cAsyncBigFile::vf6` 0x23c088(f): starts the file's waiting requests (nothing while one runs when it reads one
/// at a time, or when +0 is clear): each becomes reading (handle cleared) and gets its handle from the virtual +0x48
/// (vf8) with its name and +0x10; it stops after a request that is read at once, or after the first when the file
/// reads one at a time (+8 set)
pub fn vf6(m: &mut Mem, e: &mut dyn BigEnv, f: u32) {
    if m.r32(f + 0xc) != 0 && m.r32(f + 8) != 0 {
        return;
    }
    if m.r32(f) == 0 {
        return;
    }
    for i in 0..COUNT {
        let r = req(i);
        let s = m.r32(r + 0xc);
        if s != 1 || m.r32(r) != f {
            continue;
        }
        m.w32(r + 0x18, 0);
        m.w32(r + 0xc, 2);
        let w = m.r32(r + 0x10);
        let h = vcall(m, e, f, 0x48, &[r + 0x24, w]);
        m.w32(r + 0x18, h);
        if m.r32(r + 0xc) == 3 {
            return;
        }
        if m.r32(f + 0xc) != 0 {
            m.w32(f + 8, s);
            return;
        }
    }
}

/// `cAsyncBigFile::vf7` 0x23c190(f, out, name): `name` into `out` with a `|` in front unless it starts with one
pub fn vf7(m: &mut Mem, out: u32, name: u32) {
    if m.r8(name) != b'|' {
        m.w8(out, b'|');
        m.w8(out + 1, 0);
        // `strcat` 0x2fcd4c onto the one-character string
        strcpy(m, out + 1, name);
        return;
    }
    strcpy(m, out, name);
}

/// `cAsyncBigFile::vf14` 0x23c1d0(f): how many requests are waiting or reading
pub fn vf14(m: &Mem) -> u32 {
    let mut n = 0u32;
    for i in 0..COUNT {
        let s = m.r32(req(i) + 0xc);
        if s != 0 && s != 3 {
            n += 1;
        }
    }
    n
}

/// `cAsyncBigFile::vf15` 0x23c210(f, name, w, cb, arg) (`sp` the stack pointer on entry): the name made by the
/// virtual +0x40 (vf7) into a buffer on the stack (`sp` − 0x100); a request in use with that name is held once more
/// and its number answered; else the first free one takes the file, `w`, the callback and its argument, one holder
/// and the name, waits (state 1), and the file's reads are started (virtual +0x38, vf6, also when none is free);
/// its number, or −1
#[allow(clippy::too_many_arguments)]
pub fn vf15(m: &mut Mem, e: &mut dyn BigEnv, f: u32, name: u32, w: u32, cb: u32, arg: u32, sp: u32) -> u32 {
    let buf = sp.wrapping_sub(0x100);
    vcall(m, e, f, 0x40, &[buf, name]);
    for i in 0..COUNT {
        let r = req(i);
        if m.r32(r + 0xc) != 0 && str_eq(m, buf, r + 0x24) {
            let n = m.r32(r + 0x58).wrapping_add(1);
            m.w32(r + 0x58, n);
            return i;
        }
    }
    let mut got = u32::MAX;
    for i in 0..COUNT {
        let r = req(i);
        if m.r32(r + 0xc) == 0 {
            m.w32(r + 0x10, w);
            m.w32(r + 0xc, 1);
            m.w32(r + 0x14, 0);
            m.w32(r + 8, arg);
            m.w32(r + 0x58, 1);
            m.w32(r + 4, cb);
            m.w32(r, f);
            m.w32(r + 0x18, 0);
            m.w32(r + 0x20, 0);
            strcpy(m, r + 0x24, buf);
            got = i;
            break;
        }
    }
    vcall(m, e, f, 0x38, &[]);
    got
}

/// `cAsyncBigFile::vf11` 0x23c370(f, i): request `i` read
pub fn vf11(m: &Mem, i: u32) -> u32 {
    (m.r32(req(i) + 0xc) == 3) as u32
}

/// The read buffer of request `r` (+0x1c) into memory of its own: unpacked (`Compress_DecodeApi`) when
/// `Compress_UnpackedSizeApi` gives a size, else copied whole (`EAMem_GetSize`, `EAMem_Copy`); the memory from the
/// pool (0x345e48) when `flags` has 0x40000000, else `EAMem_Alloc`(name, size, `flags` & 0x3ffffff), and when it
/// came, filled with 0 (`flags` negative) or 0xdeadc0ed; the buffer freed and cleared; the memory
fn take(m: &mut Mem, e: &mut dyn BigEnv, r: u32, flags: u32, name: u32) -> u32 {
    let at = r + 0x1c;
    let size = e.call(m, COMPRESS_UNPACKED_SIZE, &[m.r32(at)]);
    let packed = size != 0;
    let size = if packed { size } else { e.call(m, EAMEM_GET_SIZE, &[m.r32(at)]) };
    let p = if flags & 0x4000_0000 != 0 {
        e.call(m, MEMPOOL_ALLOC, &[POOL, size, flags, name, 0, 0])
    } else {
        e.call(m, EAMEM_ALLOC, &[name, size, flags & 0x3ff_ffff])
    };
    if p != 0 {
        let v = if (flags as i32) < 0 { 0 } else { FILL };
        e.call(m, EAMEM_SET, &[p, v, size]);
    }
    if packed {
        e.call(m, COMPRESS_DECODE, &[m.r32(at), p]);
    } else {
        e.call(m, EAMEM_COPY, &[p, m.r32(at), size]);
    }
    e.call(m, EAMEM_FREE, &[m.r32(at)]);
    m.w32(at, 0);
    p
}

/// `cAsyncBigFile::vf12` 0x23c398(f, i, flags): request `i`'s data (+0x20), taken from its read buffer the first
/// time (named 0x3a4110)
pub fn vf12(m: &mut Mem, e: &mut dyn BigEnv, i: u32, flags: u32) -> u32 {
    let r = req(i);
    if m.r32(r + 0x20) == 0 {
        let p = take(m, e, r, flags, NAME);
        m.w32(r + 0x20, p);
    }
    m.r32(r + 0x20)
}

/// `cAsyncBigFile::vf13` 0x23c5c0(f, i, flags, name): request `i`'s read buffer taken into new memory with that
/// name, handed over (+0x20 cleared, not kept)
pub fn vf13(m: &mut Mem, e: &mut dyn BigEnv, i: u32, flags: u32, name: u32) -> u32 {
    let r = req(i);
    let p = take(m, e, r, flags, name);
    m.w32(r + 0x20, 0);
    p
}

/// `cAsyncBigFile::vf10` 0x23c8d0(f, i): one holder fewer on request `i`; with none left, under the lock: its read
/// stopped (virtual +0x50, vf9) when reading, its buffer (`EAMem_Free`) and data (`Mem_Free`) freed when set, the
/// request cleared (`EAMem_Set`, 0x5c bytes) and the file's +8 cleared
pub fn vf10(m: &mut Mem, e: &mut dyn BigEnv, f: u32, i: u32) {
    let r = req(i);
    let n = m.r32(r + 0x58).wrapping_sub(1);
    m.w32(r + 0x58, n);
    if (n as i32) > 0 {
        return;
    }
    let l = m.r32(LOCK);
    e.call(m, MUTEX_LOCK, &[l]);
    if m.r32(r + 0xc) == 2 {
        vcall(m, e, f, 0x50, &[r]);
    }
    let b = m.r32(r + 0x1c);
    if b != 0 {
        e.call(m, EAMEM_FREE, &[b]);
    }
    let d = m.r32(r + 0x20);
    if d != 0 {
        e.call(m, MEM_FREE, &[d]);
    }
    e.call(m, EAMEM_SET, &[r, 0, REQUEST]);
    m.w32(f + 8, 0);
    let l = m.r32(LOCK);
    e.call(m, MUTEX_UNLOCK, &[l]);
}
