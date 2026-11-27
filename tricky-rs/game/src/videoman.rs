//! The small films' stream manager (board row G11a3) on the game's bytes: `cVideoStreamMan` and its streams, which
//! play the `.SSS` films (trick book, course previews) a frame at a time into a texture. The frames themselves are
//! `tricky_data::video` (row G11a1).
//!
//! The manager: +8 its table (0x39af08), +0xc the stream count (at most 3), +0x10 the three streams, +0x1c their
//! users. A stream (0x118 bytes, "VideoStream" 0x39aef8): +0 the film's name, +0x80 EA's stream (`Strm_Create`),
//! +0x84 the queued file, +0x88 the stream's memory, +0x8c the texture, +0x90 a frame is waiting, +0x94 the frame's
//! shape, +0x98 its pixel bytes, +0x9c / +0xa0 width and height, +0xa4 the texture's description (4, four
//! pointers to +0xb8, +0xb8 the texture's half, three −1, +0xf8 1), +0x100 the tick of the last frame, +0x104 the
//! tick, +0x108 frames a second (1..30), +0x10c loop, +0x110 finished, +0x114 playing.
//!
//! Calls out (EA's stream library, the heaps, the renderer's virtuals, the base node) go through [`VideoEnv`]; the
//! fill and copy (`EAMem_Set` / `EAMem_Copy`), `strcpy` / `strcmp` and `Shape_PixelFormatIndex` are run here.
//! Proven from the disassembly of `SLUS_203.26` (read by hand: Ghidra was not running).

use crate::pf::Mem;
use crate::{eamem, libc, shapelib};

/// The calls out
pub trait VideoEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

pub const OBJNODE_CTOR: u32 = 0x17cf50;
pub const OBJNODE_DTOR: u32 = 0x17cfe0;
pub const MEM_DELETE: u32 = 0x114f80;
pub const SHAPE_PALETTE_FORMAT: u32 = 0x2c2040;
pub const SHAPE_UNPACK: u32 = 0x2c2318;
pub const STRM_MEM_SIZE: u32 = 0x2c6e48;
pub const STRM_CREATE: u32 = 0x2c6e70;
pub const STRM_CLOSE: u32 = 0x2c7178;
pub const STRM_QUEUE_FILE: u32 = 0x2c73d0;
pub const STRM_PURGE: u32 = 0x2c75d8;
pub const STRM_GET_PACKET: u32 = 0x2c7a28;
pub const STRM_RELEASE_PACKET: u32 = 0x2c7b30;
pub const STRM_IS_FINISHED: u32 = 0x2c7c70;
pub const EAMEM_ALLOC: u32 = 0x2ccf70;
pub const EAMEM_FREE: u32 = 0x2ccfc0;
/// `gApp`, whose +0x724 is the renderer
pub const GAPP: u32 = 0x337c58;
/// The manager's table
pub const VT: u32 = 0x39af08;
/// The allocations' names: the stream's memory, the texture, the shape, the stream
pub const NAME_STREAM_MEM: u32 = 0x39aed8;
pub const NAME_TEXTURE: u32 = 0x39aee0;
pub const NAME_SHAPE: u32 = 0x39aee8;
pub const NAME_STREAM: u32 = 0x39aef8;
/// The stream size EA's stream gets (whatever was allocated)
pub const STREAM_BYTES: u32 = 0x19000;

/// The renderer's virtual at `off` (adjust) / `off + 4` (function), on its table at +0x10
fn renderer_call(m: &mut Mem, e: &mut dyn VideoEnv, off: u32, rest: &[u32]) -> u32 {
    let r = m.r32(m.r32(GAPP) + 0x724);
    let vt = m.r32(r + 0x10);
    let me = r.wrapping_add(m.r16(vt + off) as i16 as i32 as u32);
    let mut a = vec![me];
    a.extend_from_slice(rest);
    e.call(m, m.r32(vt + off + 4), &a)
}

/// `VideoStream_AllocShape` 0x1ffbd8(_, w, h, kind): a shape for a frame of w × h, filled with 0xdeadc0ed
/// (when allocated) and its 16-byte header cleared, then type, width and height. Kind 1: 16-bit pixels (type
/// `Shape_PixelFormatIndex(16)`); kind 2: 8-bit pixels and a 256-colour palette block after them (type
/// `Shape_PixelFormatIndex(8)`, the next-block offset in the first word; the palette block's type
/// `Shape_PaletteFormatIndex(32)`, 256 × 1, a grey ramp (colour k: k, k, k, 0xff); an odd w · h puts its halves at odd
/// addresses, an address error on the PS2). (The game only asks for kind 1; kind 0 and others read registers the
/// function never set, and are not ported.)
pub fn alloc_shape(m: &mut Mem, e: &mut dyn VideoEnv, w: u32, h: u32, kind: u32) -> u32 {
    let n = w.wrapping_mul(h);
    let (pixels, size, ty) = match kind {
        1 => {
            let s = n.wrapping_mul(2).wrapping_add(0x10);
            (s, s, shapelib::pixel_format_index(0x10))
        }
        2 => (n.wrapping_add(0x10), n.wrapping_add(0x420), shapelib::pixel_format_index(8)),
        _ => panic!("VideoStream_AllocShape: kind {kind} reads registers it never set"),
    };
    let p = e.call(m, EAMEM_ALLOC, &[NAME_SHAPE, size, 0]);
    if p != 0 {
        eamem::set(m, p, 0xdead_c0ed, size);
    }
    eamem::set(m, p, 0, 0x10);
    let pal = p.wrapping_add(pixels);
    m.w8(p, ty as u8);
    m.w16(p + 4, w as u16);
    m.w16(p + 6, h as u16);
    if kind == 2 {
        let pt = e.call(m, SHAPE_PALETTE_FORMAT, &[0x20]);
        m.w32(p, m.r8(p) as u32 | pal.wrapping_sub(p) << 8);
        eamem::set(m, pal, 0, 0x10);
        m.w16(pal + 6, 1);
        m.w8(pal, pt as u8);
        m.w16(pal + 4, 0x100);
        for k in 0..0x100 {
            let c = pal + 0x10 + 4 * k;
            m.w8(c + 2, k as u8);
            m.w8(c + 1, k as u8);
            m.w8(c, k as u8);
            m.w8(c + 3, 0xff);
        }
    }
    p
}

/// `VideoStream_CreateFrameTexture` 0x1ff890(s): the frame's shape (kind 1) at +0x94, its pixel bytes (w · h · 2)
/// at +0x98 and cleared, and a texture made of it by the renderer (virtual +0x150 with the name 0x39aee0, 0) at
/// +0x8c; returns the texture
pub fn create_frame_texture(m: &mut Mem, e: &mut dyn VideoEnv, s: u32) -> u32 {
    let (w, h) = (m.r32(s + 0x9c), m.r32(s + 0xa0));
    let shape = alloc_shape(m, e, w, h, 1);
    m.w32(s + 0x94, shape);
    let bytes = m.r32(s + 0x9c).wrapping_mul(m.r32(s + 0xa0)) << 1;
    m.w32(s + 0x98, bytes);
    eamem::set(m, shape.wrapping_add(0x10), 0, bytes);
    let t = renderer_call(m, e, 0x150, &[m.r32(s + 0x94), NAME_TEXTURE, 0]);
    m.w32(s + 0x8c, t);
    t
}

/// `VideoStream_Init` 0x1ff6f0(s, w, h): a stream of w × h frames, stopped, 10 frames a second; its texture
/// ([`create_frame_texture`]) and description; EA's stream on memory of `Strm_CalcMemSize(2, 1, 1)` + 0x18000 +
/// w · h / 2 bytes (at least 0x19000, signed), though the stream is told 0x19000. Returns `s`.
pub fn stream_init(m: &mut Mem, e: &mut dyn VideoEnv, s: u32, w: u32, h: u32) -> u32 {
    for o in [0x100, 0x104] {
        m.w32(s + o, 0);
    }
    m.w32(s + 0x9c, w);
    m.w32(s + 0xa0, h);
    for o in [0x110, 0x10c, 0x114] {
        m.w32(s + o, 0);
    }
    m.w32(s + 0x108, 10);
    create_frame_texture(m, e, s);
    m.w32(s + 0xa4, 4);
    for k in (0..4).rev() {
        m.w32(s + 0xa8 + 4 * k, s + 0xb8);
    }
    let t = m.r16(s + 0x8c);
    m.w16(s + 0xbe, 0xffff);
    m.w16(s + 0xb8, t);
    m.w32(s + 0xf8, 1);
    m.w16(s + 0xba, 0xffff);
    let calc = e.call(m, STRM_MEM_SIZE, &[2, 1, 1]);
    m.w16(s + 0xbc, 0xffff);
    let half = (w.wrapping_mul(h) as i32 / 2) as u32;
    let want = calc.wrapping_add(0x18000).wrapping_add(half);
    let size = if (want as i32) < STREAM_BYTES as i32 { STREAM_BYTES } else { want };
    let mem = e.call(m, EAMEM_ALLOC, &[NAME_STREAM_MEM, size, 0]);
    m.w32(s + 0x88, mem);
    let st = e.call(m, STRM_CREATE, &[2, 1, 1, mem, STREAM_BYTES]);
    m.w32(s + 0x80, st);
    m.w32(s + 0x84, 0);
    s
}

/// `VideoStreamMan_FreeFrameBuffer` 0x1ff988(s): the frame's shape given back
pub fn free_frame_buffer(m: &mut Mem, e: &mut dyn VideoEnv, s: u32) {
    e.call(m, EAMEM_FREE, &[m.r32(s + 0x94)]);
}

/// `VideoStreamMan_Destroy` 0x1ff820(s, flags): stopped if playing, EA's stream closed, its memory and the frame's
/// shape given back; with flag 1 the stream itself (`Mem_Delete`)
pub fn stream_destroy(m: &mut Mem, e: &mut dyn VideoEnv, s: u32, flags: u32) {
    if m.r32(s + 0x114) != 0 {
        stream_stop(m, e, s);
    }
    e.call(m, STRM_CLOSE, &[m.r32(s + 0x80)]);
    e.call(m, EAMEM_FREE, &[m.r32(s + 0x88)]);
    free_frame_buffer(m, e, s);
    if flags & 1 != 0 {
        e.call(m, MEM_DELETE, &[s]);
    }
}

/// `VideoStream_Open` 0x1ff990(s, name, loop, fps): unless already playing (then it answers +0x114): playing,
/// `loop`, fps clamped to 1..30 (anything ≤ 0 is 1), the name copied, the file queued on EA's stream. Returns the
/// queue's answer.
pub fn stream_open(m: &mut Mem, e: &mut dyn VideoEnv, s: u32, name: u32, looped: u32, fps: i32) -> u32 {
    let playing = m.r32(s + 0x114);
    if playing != 0 {
        return playing;
    }
    m.w32(s + 0x10c, looped);
    m.w32(s + 0x114, 1);
    let fps = if fps > 0 { if fps < 0x1f { fps } else { 30 } } else { 1 };
    m.w32(s + 0x108, fps as u32);
    libc::strcpy(m, s, name);
    m.w32(s + 0x110, 0);
    let q = e.call(m, STRM_QUEUE_FILE, &[m.r32(s + 0x80), s, 0, 0]);
    m.w32(s + 0x84, q);
    m.w32(s + 0x104, 0);
    m.w32(s + 0x100, 0);
    q
}

/// `VideoStream_Stop` 0x1ffa10(s): when playing, stopped and the queued file purged (`Strm_Purge`, whose answer
/// it returns); else 0
pub fn stream_stop(m: &mut Mem, e: &mut dyn VideoEnv, s: u32) -> u32 {
    if m.r32(s + 0x114) == 0 {
        return 0;
    }
    m.w32(s + 0x114, 0);
    e.call(m, STRM_PURGE, &[m.r32(s + 0x80), m.r32(s + 0x84)])
}

/// `VideoStreamMan_Reopen` 0x1ffa38(s): stopped, EA's stream closed and made again on the same memory
pub fn stream_reopen(m: &mut Mem, e: &mut dyn VideoEnv, s: u32) {
    stream_stop(m, e, s);
    e.call(m, STRM_CLOSE, &[m.r32(s + 0x80)]);
    let st = e.call(m, STRM_CREATE, &[2, 1, 1, m.r32(s + 0x88), STREAM_BYTES]);
    m.w32(s + 0x80, st);
}

/// `VideoStreamMan_DecodeFrame` 0x1ff928(s, frame): the frame's shape typed 0x60 and unpacked (`Shape_Unpack`),
/// its pixels copied into the stream's shape, the unpacked copy given back; a frame is waiting
pub fn decode_frame(m: &mut Mem, e: &mut dyn VideoEnv, s: u32, frame: u32) {
    m.w8(frame, 0x60);
    let u = e.call(m, SHAPE_UNPACK, &[frame]);
    eamem::copy(m, m.r32(s + 0x94).wrapping_add(0x10), u.wrapping_add(0x10), m.r32(s + 0x98));
    e.call(m, EAMEM_FREE, &[u]);
    m.w32(s + 0x90, 1);
}

/// `VideoStreamMan_UpdateFrame` 0x1ffa88(s): a tick of a playing, unfinished stream: once 60 / fps ticks have
/// passed since the last frame, EA's stream's next packet (when there is one) decoded from its +8 and released;
/// when the stream has finished, the file queued again if looping, else finished and stopped
pub fn update_frame(m: &mut Mem, e: &mut dyn VideoEnv, s: u32) {
    if m.r32(s + 0x114) == 0 || m.r32(s + 0x110) != 0 {
        return;
    }
    let fps = m.r32(s + 0x108) as i32;
    let tick = m.r32(s + 0x104).wrapping_add(1);
    m.w32(s + 0x104, tick);
    if (tick.wrapping_sub(m.r32(s + 0x100)) as i32) < 60 / fps {
        return;
    }
    let pkt = e.call(m, STRM_GET_PACKET, &[m.r32(s + 0x80)]);
    if pkt != 0 {
        m.w32(s + 0x100, m.r32(s + 0x104));
        decode_frame(m, e, s, pkt + 8);
        e.call(m, STRM_RELEASE_PACKET, &[m.r32(s + 0x80), pkt]);
    }
    if e.call(m, STRM_IS_FINISHED, &[m.r32(s + 0x80)]) == 0 {
        return;
    }
    if m.r32(s + 0x10c) != 0 {
        let q = e.call(m, STRM_QUEUE_FILE, &[m.r32(s + 0x80), s, 0, 0]);
        m.w32(s + 0x84, q);
    } else {
        m.w32(s + 0x110, 1);
        stream_stop(m, e, s);
    }
}

/// `VideoStreamMan_UploadFrame` 0x1ffb80(s): a waiting frame of a playing stream handed to the renderer
/// (virtual +0x160: the texture, the shape)
pub fn upload_frame(m: &mut Mem, e: &mut dyn VideoEnv, s: u32) {
    if m.r32(s + 0x90) == 0 || m.r32(s + 0x114) == 0 {
        return;
    }
    m.w32(s + 0x90, 0);
    renderer_call(m, e, 0x160, &[m.r32(s + 0x8c), m.r32(s + 0x94)]);
}

/// `cVideoStreamMan::cVideoStreamMan` 0x1ffd80(v): the base node made, no streams, no users
pub fn new(m: &mut Mem, e: &mut dyn VideoEnv, v: u32) -> u32 {
    e.call(m, OBJNODE_CTOR, &[v]);
    m.w32(v + 0xc, 0);
    m.w32(v + 8, VT);
    for k in (0..3).rev() {
        m.w32(v + 0x1c + 4 * k, 0);
    }
    v
}

/// `cVideoStreamMan::~cVideoStreamMan` 0x1ffde0(v, flags): every stream destroyed (flags 3), then the base
/// node's destructor with `flags`
pub fn delete(m: &mut Mem, e: &mut dyn VideoEnv, v: u32, flags: u32) {
    m.w32(v + 8, VT);
    let mut i = 0;
    while (i as i32) < m.r32(v + 0xc) as i32 {
        let s = m.r32(v + 0x10 + 4 * i);
        if s != 0 {
            stream_destroy(m, e, s, 3);
        }
        i += 1;
    }
    e.call(m, OBJNODE_DTOR, &[v, flags]);
}

/// `VideoPlayer_AddStream` 0x1ffe70(v, w, h): a new stream of w × h (0x118 bytes filled with 0xdeadc0de when
/// allocated), unless there are 3; returns 1 or 0
pub fn add_stream(m: &mut Mem, e: &mut dyn VideoEnv, v: u32, w: u32, h: u32) -> u32 {
    let n = m.r32(v + 0xc);
    if n as i32 >= 3 {
        return 0;
    }
    let slot = v + 0x10 + 4 * n;
    let s = e.call(m, EAMEM_ALLOC, &[NAME_STREAM, 0x118, 0]);
    if s != 0 {
        eamem::set(m, s, 0xdead_c0de, 0x118);
    }
    let s = stream_init(m, e, s, w, h);
    m.w32(slot, s);
    m.w32(v + 0xc, m.r32(v + 0xc).wrapping_add(1));
    1
}

/// `VideoPlayer_Open` 0x1fff30(v, name, w, h, loop, fps): a playing stream of that film, `loop` and fps gets one
/// more user; else the first stopped stream of w × h opens it (fps as asked, clamped by the open) and gets one;
/// returns its index, or −1
pub fn player_open(m: &mut Mem, e: &mut dyn VideoEnv, v: u32, name: u32, w: u32, h: u32, looped: u32, fps: u32) -> i32 {
    let n = m.r32(v + 0xc) as i32;
    for i in 0..n.max(0) as u32 {
        let s = m.r32(v + 0x10 + 4 * i);
        if m.r32(s + 0x114) == 0 {
            continue;
        }
        if libc::strcmp(m, s, name) != 0 || m.r32(s + 0x10c) != looped || m.r32(s + 0x108) != fps {
            continue;
        }
        let u = v + 0x1c + 4 * i;
        m.w32(u, m.r32(u).wrapping_add(1));
        return i as i32;
    }
    for i in 0..n.max(0) as u32 {
        let s = m.r32(v + 0x10 + 4 * i);
        if m.r32(s + 0x114) != 0 || m.r32(s + 0x9c) != w || m.r32(s + 0xa0) != h {
            continue;
        }
        stream_open(m, e, s, name, looped, fps as i32);
        let u = v + 0x1c + 4 * i;
        m.w32(u, m.r32(u).wrapping_add(1));
        return i as i32;
    }
    -1
}

/// `VideoPlayer_Release` 0x200098(v, i): one user fewer (none: 0 back); the last one stops the stream. Returns
/// the users left.
pub fn player_release(m: &mut Mem, e: &mut dyn VideoEnv, v: u32, i: u32) -> u32 {
    let u = (v + 0x1c).wrapping_add(i << 2);
    let n = m.r32(u);
    if n == 0 {
        return 0;
    }
    m.w32(u, n.wrapping_sub(1));
    if n.wrapping_sub(1) == 0 {
        stream_stop(m, e, m.r32(v.wrapping_add(i << 2) + 0x10));
    }
    m.r32(u)
}

/// `VideoStreamMan_Release` 0x2000f0(v, i): one user fewer when there are any (signed); with none left the stream
/// is made again ([`stream_reopen`]). Returns the users left.
pub fn man_release(m: &mut Mem, e: &mut dyn VideoEnv, v: u32, i: u32) -> u32 {
    let u = (v + 0x1c).wrapping_add(i << 2);
    let mut n = m.r32(u);
    if n as i32 > 0 {
        n = n.wrapping_sub(1);
        m.w32(u, n);
    }
    if n != 0 {
        return n;
    }
    stream_reopen(m, e, m.r32(v.wrapping_add(i << 2) + 0x10));
    m.r32(u)
}

/// `VideoStreamMan_ReleaseAll` 0x200148(v): [`man_release`] on every stream
pub fn release_all(m: &mut Mem, e: &mut dyn VideoEnv, v: u32) {
    let mut i = 0u32;
    while (i as i32) < m.r32(v + 0xc) as i32 {
        man_release(m, e, v, i);
        i += 1;
    }
}

/// `VideoStreamMan_GetTexture` 0x2001a0(v, i): the stream's texture while it has users (signed), else −1
pub fn get_texture(m: &Mem, v: u32, i: u32) -> u32 {
    let a = v.wrapping_add(i << 2);
    if m.r32(a + 0x1c) as i32 > 0 {
        m.r32(m.r32(a + 0x10) + 0x8c)
    } else {
        u32::MAX
    }
}

/// `Movie_GetFrameTexture` 0x2001c8(v, i): the stream's texture description (+0xa4) while it has users, else 0
pub fn frame_texture(m: &Mem, v: u32, i: u32) -> u32 {
    let a = v.wrapping_add(i << 2);
    if m.r32(a + 0x1c) as i32 > 0 {
        m.r32(a + 0x10).wrapping_add(0xa4)
    } else {
        0
    }
}

/// `cVideoStreamMan::vf1` 0x2001f8(v): every stream with users ticked ([`update_frame`])
pub fn update(m: &mut Mem, e: &mut dyn VideoEnv, v: u32) {
    let mut i = 0u32;
    while (i as i32) < m.r32(v + 0xc) as i32 {
        if m.r32(v + 0x1c + 4 * i) as i32 > 0 {
            update_frame(m, e, m.r32(v + 0x10 + 4 * i));
        }
        i += 1;
    }
}

/// `cVideoStreamMan::vf2` 0x200270(v): every stream with users uploaded ([`upload_frame`])
pub fn upload(m: &mut Mem, e: &mut dyn VideoEnv, v: u32) {
    let mut i = 0u32;
    while (i as i32) < m.r32(v + 0xc) as i32 {
        if m.r32(v + 0x1c + 4 * i) as i32 > 0 {
            upload_frame(m, e, m.r32(v + 0x10 + 4 * i));
        }
        i += 1;
    }
}

/// `VideoStreamMan_IsFinished` 0x2002e8(v, i): the stream's +0x110
pub fn is_finished(m: &Mem, v: u32, i: u32) -> u32 {
    m.r32(m.r32(v.wrapping_add(i << 2) + 0x10) + 0x110)
}
