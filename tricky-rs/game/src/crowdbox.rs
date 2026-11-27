//! The crowd boxes' sound bookkeeping (board row G10m): `CrowdSoundList_Construct` 0x21eca8 / `_Add` 0x21ed30,
//! `List_PickRandom` 0x21ed98, `CrowdBox_Init` 0x21ee70, `CrowdBox_StopSlot` 0x2201f8, `_StopAll` 0x220338, `_Arm`
//! 0x2206c8, `_Stop` 0x2206e0, `Crowd_StopCheer` 0x220720 and `CrowdSys_UpdateBoxes` 0x220968. The calls out are
//! [`LoadEnv`] calls by address; a call that takes a float in `f12` gets its bits as the argument after the ints.
//!
//! A box is 0x190 bytes: +0 the box's sound (-1 none), +0x10 .. +0x1c a list of cheer files ([`list_construct`]), +0xd4 the
//! slots in use (of fifteen from +0x20, three words each: in use, a value, the sound entry), +0xd8, three sound
//! entries of 0x30 bytes from +0xe0 (id, voice, use count), +0x170 the voices playing, +0x174 the cheer's voice, +0x178,
//! +0x17c, +0x180 the cheer state (0 off, 1 armed, 2 playing), +0x184, +0x188 the cheer level, +0x18c.

use crate::speechlib::Mem;
use crate::speechload::LoadEnv;

pub const OPEN_FILE: u32 = 0x2c8068;
pub const RAND: u32 = 0x23d4a8;
pub const VOICE_STOP: u32 = 0x20c2a0;
pub const BOX_UPDATE: u32 = 0x21efb0;
const AUDIO: u32 = 0x343120;
const QUARTER: u32 = 0x3e80_0000;

/// 0x21eca8: an empty list (the count and five files).
pub fn list_construct(m: &mut dyn Mem, l: u32) {
    for k in 0..6 {
        m.w32(l + k * 4, 0);
    }
}

/// 0x21ed30: add the file `name` (opened by `OPEN_FILE`) when there is room; 1 when it was added.
pub fn list_add(m: &mut dyn Mem, e: &mut dyn LoadEnv, l: u32, name: u32, mode: u32) -> u32 {
    if (m.r32(l) as i32) < 5 {
        let h = e.call(m, OPEN_FILE, &[name, mode]);
        let n = m.r32(l);
        m.w32(l.wrapping_add(n.wrapping_shl(2)).wrapping_add(4), h);
        if h != 0 {
            m.w32(l, n.wrapping_add(1));
            return 1;
        }
    }
    0
}

/// 0x21ed98: a random file of the list, 0 when it is empty.
pub fn list_pick_random(m: &mut dyn Mem, e: &mut dyn LoadEnv, l: u32) -> u32 {
    if m.r32(l) == 0 {
        return 0;
    }
    let r = e.call(m, RAND, &[]);
    let n = m.r32(l);
    m.r32(l.wrapping_add((r % n).wrapping_shl(2)).wrapping_add(4))
}

/// 0x21ee70.
pub fn box_init(m: &mut dyn Mem, b: u32) {
    m.w32(b, 0xffff_ffff);
    for k in [0x10, 0x14, 0x18, 0x1c] {
        m.w32(b + k, 0);
    }
    m.w32(b + 0xd8, 0xffff_ffff);
    m.w32(b + 0xd4, 0);
    m.w32(b + 0x184, 3);
    m.w32(b + 0x174, 0xffff_ffff);
    for k in [0x170, 0x178, 0x17c, 0x180, 0x188, 0x18c] {
        m.w32(b + k, 0);
    }
    for s in 0..15 {
        for k in [0x20, 0x24, 0x28] {
            m.w32(b + k + s * 0xc, 0);
        }
    }
    for s in 0..3 {
        m.w32(b + 0xe0 + s * 0x30, 0xffff_ffff);
        m.w32(b + 0xe4 + s * 0x30, 0xffff_ffff);
        m.w32(b + 0xe8 + s * 0x30, 0);
        m.w32(b + 0x100 + s * 0x30, s);
    }
}

/// 0x2201f8: stop the sound in slot `slot`, fading the cheer by `fade` when it was the last one.
pub fn stop_slot(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32, slot: i32, fade: f32) {
    if slot < 0 {
        return;
    }
    let off = (slot as u32).wrapping_mul(0xc);
    let flag = b.wrapping_add(0x20).wrapping_add(off);
    if m.r32(flag) == 0 {
        return;
    }
    let used = m.r32(b + 0xd4).wrapping_sub(1);
    let entry = m.r32(b.wrapping_add(0x28).wrapping_add(off));
    m.w32(flag, 0);
    m.w32(b + 0xd4, used);
    m.w32(b.wrapping_add(0x28).wrapping_add(off), 0);
    let rc = m.r32(entry.wrapping_add(8)).wrapping_sub(1);
    m.w32(entry.wrapping_add(8), rc);
    if rc == 0 {
        let voice = m.r32(entry.wrapping_add(4));
        if (voice as i32) >= 0 {
            let a = m.r32(AUDIO);
            e.call(m, VOICE_STOP, &[a, voice, 1, fade.to_bits()]);
        }
        m.w32(entry, 0xffff_ffff);
        m.w32(entry.wrapping_add(4), 0xffff_ffff);
        m.w32(b + 0x18c, 0);
        let v = m.r32(b + 0x170).wrapping_sub(1);
        m.w32(b + 0x170, v);
    }
    if m.r32(b + 0xd4) != 0 {
        return;
    }
    if f32::from_bits(QUARTER) < f32::from_bits(m.r32(b + 0x188)) {
        m.w32(b + 0x188, QUARTER);
    }
    let voice = m.r32(b + 0x174);
    if (voice as i32) >= 0 {
        if fade == 0.0 {
            let a = m.r32(AUDIO);
            e.call(m, VOICE_STOP, &[a, voice, 2, fade.to_bits()]);
        }
        m.w32(b + 0x174, 0xffff_ffff);
        if m.r32(b + 0x180) == 2 {
            m.w32(b + 0x180, 1);
        }
    }
    m.w32(b + 0x178, 0);
    m.w32(b, 0xffff_ffff);
}

/// 0x220338: every slot in use.
pub fn stop_all(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32, fade: f32) {
    for s in 0..15u32 {
        if m.r32(b + 0x20 + s * 0xc) != 0 {
            stop_slot(m, e, b, s as i32, fade);
        }
    }
}

/// 0x2206c8.
pub fn arm(m: &mut dyn Mem, b: u32) {
    if m.r32(b + 0x180) == 0 {
        m.w32(b + 0x180, 1);
    }
}

/// 0x220720.
pub fn stop_cheer(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32) {
    if f32::from_bits(QUARTER) < f32::from_bits(m.r32(b + 0x188)) {
        m.w32(b + 0x188, QUARTER);
    }
    let voice = m.r32(b + 0x174);
    m.w32(b + 0x180, 0);
    if (voice as i32) >= 0 {
        let a = m.r32(AUDIO);
        e.call(m, VOICE_STOP, &[a, voice, 2, 0]);
        m.w32(b + 0x174, 0xffff_ffff);
    }
}

/// 0x2206e0.
pub fn stop(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32) {
    if m.r32(b + 0x180) == 2 {
        stop_cheer(m, e, b);
    }
    m.w32(b + 0x180, 0);
}

/// 0x220968: tick every box of the crowd system `s` (+0x3728 the count, +0x372c the boxes).
pub fn update_boxes(m: &mut dyn Mem, e: &mut dyn LoadEnv, s: u32) {
    if (m.r32(s + 0x3728) as i32) > 0 {
        let mut i = 0u32;
        loop {
            let a = m.r32(s + 0x372c).wrapping_add(i.wrapping_mul(0x190));
            i += 1;
            e.call(m, BOX_UPDATE, &[a]);
            if (i as i32) >= m.r32(s + 0x3728) as i32 {
                break;
            }
        }
    }
}

// ---- the voice requests (board row G10m) ----

pub const MEMSET: u32 = 0x2fb468;
pub const MONITOR_FIND: u32 = 0x20c200;
pub const MONITOR_SET: u32 = 0x20ad38;
pub const PLAY_PENDING: u32 = 0x20c948;
pub const ENTRY_BEGIN: u32 = 0x222160;
pub const ENTRY_PLAY: u32 = 0x222358;
/// The eight bytes every request starts with in its +0x44 row.
const REQ_WORDS: u32 = 0x39fbd8;

fn arr(m: &dyn Mem, a: u32, off: u32, i: u32) -> u32 {
    m.r32(a.wrapping_add(off)).wrapping_add(i.wrapping_mul(4))
}

/// `lq` / `sq` ignore the low four bits of the address.
fn quad(m: &dyn Mem, p: u32) -> [u32; 4] {
    let p = p & !0xf;
    [m.r32(p), m.r32(p + 4), m.r32(p + 8), m.r32(p + 12)]
}

fn put_quad(m: &mut dyn Mem, p: u32, q: [u32; 4]) {
    let p = p & !0xf;
    for k in 0..4 {
        m.w32(p + 4 * k, q[k as usize]);
    }
}

/// The new request every voice start builds (the audio system's queue, inlined at each use): the next entry `i` of the
/// parallel arrays with its defaults (see `sndqueue::queue_push`).
fn push_request(m: &mut dyn Mem, e: &mut dyn LoadEnv, a: u32) -> u32 {
    let i = m.r32(a + 0x20).wrapping_add(1);
    m.w32(a + 0x20, i);
    let rec = m.r32(a + 0x24).wrapping_add(i.wrapping_mul(0x18));
    for k in 0..0x18 {
        let v = m.r8(a + 8 + k);
        m.w8(rec.wrapping_add(k), v);
    }
    m.w32(a.wrapping_add(i.wrapping_mul(4)).wrapping_add(0x28), 0);
    m.w32(arr(m, a, 0x38, i), 0);
    let first = m.r8(rec) as i8 as i32 as u32;
    m.w32(arr(m, a, 0x3c, i), first);
    m.w32(arr(m, a, 0x40, i), 0);
    e.call(m, MEMSET, &[a.wrapping_add(i.wrapping_mul(8)).wrapping_add(0x44), 0, 8]);
    m.w32(arr(m, a, 0x68, i), 0);
    m.w32(arr(m, a, 0x6c, i), 0);
    let row = m.r32(a + 0x74).wrapping_add(i.wrapping_mul(16));
    m.w32(row + 4, 0);
    m.w32(row, 100);
    m.w32(arr(m, a, 0x70, i), 0);
    m.w32(arr(m, a, 0x64, i), 0);
    m.w32(row + 8, 0x5a);
    m.w32(row + 0xc, 0x32);
    m.w32(arr(m, a, 0x78, i), 0);
    m.w32(arr(m, a, 0x7c, i), 1);
    i
}

/// The request's volume: kept whole in +0x3c, clamped to the byte 0..127 into its record.
fn set_volume(m: &mut dyn Mem, a: u32, i: u32, vol: u32) {
    let p = arr(m, a, 0x3c, i);
    m.w32(p, vol);
    let rec = m.r32(a + 0x24).wrapping_add(i.wrapping_mul(0x18));
    let v = vol as i32;
    let byte = if v < 0x7f {
        if v < 0 {
            0
        } else {
            m.r8(p)
        }
    } else {
        0x7f
    };
    m.w8(rec, byte);
}

fn copy_req_words(m: &mut dyn Mem, a: u32, i: u32) {
    let row = a.wrapping_add(i.wrapping_mul(8)).wrapping_add(0x44);
    for k in 0..8 {
        let v = m.r8(REQ_WORDS + k);
        m.w8(row + k, v);
    }
}

/// The part every request shares once it is the newest entry: the sound's kind, the box's sound, the request words,
/// the box as owner and the listener; then the pending sounds are played. The voice they give.
fn finish_request(m: &mut dyn Mem, e: &mut dyn LoadEnv, a: u32, i: u32, b: u32, listener: u32) -> u32 {
    m.w32(arr(m, a, 0x64, i), b);
    m.w32(arr(m, a, 0x7c, i), 0);
    m.w32(arr(m, a, 0x68, i), listener);
    e.call(m, PLAY_PENDING, &[a])
}

/// 0x21fc68: a slot's value, when the slot is in use.
pub fn set_slot_value(m: &mut dyn Mem, b: u32, slot: i32, v: u32) {
    if slot >= 0 {
        let s = b.wrapping_add((slot as u32).wrapping_mul(0xc));
        if m.r32(s.wrapping_add(0x20)) != 0 {
            m.w32(s.wrapping_add(0x24), v);
        }
    }
}

/// 0x21fc98: a sound of the audio queue `src` joins the box: the slot index, or -1 when the box is full or the sound
/// has no emitter. `id` is the sound the box counts it under.
pub fn start_sound(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32, src: u32, id: u32) -> i32 {
    let used = m.r32(b + 0xd4);
    if (used as i32) >= 15 {
        return -1;
    }
    let cnt = m.r32(src + 0x20);
    let em = m.r32(src.wrapping_add(0x28).wrapping_add(cnt.wrapping_mul(4)));
    if em == 0 {
        return -1;
    }
    let vols = m.r32(src + 0x3c);
    let mut slot = 0u32;
    let mut fp = 0u32;
    for k in 0..15u32 {
        if m.r32(b + 0x20 + k * 0xc) == 0 {
            slot = b + 0x20 + k * 0xc;
            fp = k;
            break;
        }
    }
    m.w32(b + 0xd4, used.wrapping_add(1));
    m.w32(slot, em);
    let vol = m.r32(vols.wrapping_add(cnt.wrapping_mul(4)));
    m.w32(slot.wrapping_add(4), vol);
    for k in 0..3u32 {
        let ent = b + 0xe0 + k * 0x30;
        if m.r32(ent) == id {
            m.w32(slot.wrapping_add(8), ent);
            let c = m.r32(ent + 8).wrapping_add(1);
            m.w32(ent + 8, c);
            break;
        }
    }
    if m.r32(b + 0xd4) == 1 {
        let c = m.r32(src + 0x20);
        let l = m.r32(arr(m, src, 0x68, c));
        m.w32(b, l);
        let v = m.r32(arr(m, src, 0x40, c));
        m.w32(b + 0x17c, v);
        let q = quad(m, em);
        put_quad(m, b + 0x10, q);
    }
    let mut ent = m.r32(slot.wrapping_add(8));
    if ent != 0 {
        return fp as i32;
    }
    let em = m.r32(slot);
    for k in 0..3u32 {
        let en = b + 0xe0 + k * 0x30;
        if m.r32(en + 8) == 0 {
            m.w32(en, id);
            m.w32(en + 8, 1);
            m.w32(slot.wrapping_add(8), en);
            let v = m.r32(b + 0x170).wrapping_add(1);
            m.w32(b + 0x170, v);
            ent = en;
            break;
        }
    }
    let q = quad(m, em);
    put_quad(m, ent.wrapping_add(0x10), q);
    let a = m.r32(AUDIO);
    let kind = m.r32(b + 0xd8);
    let i = push_request(m, e, a);
    m.w32(arr(m, a, 0xa40, i), kind);
    m.w32(arr(m, a, 0xa44, i), id);
    let en = m.r32(slot.wrapping_add(8));
    m.w32(a.wrapping_add(i.wrapping_mul(4)).wrapping_add(0x28), en.wrapping_add(0x10));
    let vol = m.r32(slot.wrapping_add(4));
    set_volume(m, a, i, vol);
    let l = m.r32(b);
    m.w32(arr(m, a, 0x78, i), 1);
    let v = m.r32(b + 0x17c);
    m.w32(arr(m, a, 0x40, i), v);
    copy_req_words(m, a, i);
    let voice = finish_request(m, e, a, i, b, l);
    let en = m.r32(slot.wrapping_add(8));
    m.w32(en.wrapping_add(4), voice);
    fp as i32
}

/// The cheer's request: the file `file` of the box's kind, volume `vol`, owner the box; the voice it got.
fn cheer_request(m: &mut dyn Mem, e: &mut dyn LoadEnv, a: u32, b: u32, vol: u32) -> u32 {
    let i = m.r32(a + 0x20);
    m.w32(a.wrapping_add(i.wrapping_mul(4)).wrapping_add(0x28), b + 0x10);
    set_volume(m, a, i, vol);
    let v = m.r32(b + 0x17c);
    m.w32(arr(m, a, 0x40, i), v);
    m.w32(arr(m, a, 0x78, i), 1);
    let l = m.r32(b);
    m.w32(arr(m, a, 0x68, i), l);
    e.call(m, ENTRY_PLAY, &[a])
}

/// 0x2203a8 (`start` false) and 0x220538 (`start` true): start the crowd's cheer `k`.
fn begin_cheer(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32, k: i32, start: bool) {
    stop_cheer(m, e, b);
    if m.r32(b + 0xd4) == 0 {
        return;
    }
    let a = m.r32(AUDIO);
    m.w32(b + 0x184, k as u32);
    let row = a.wrapping_add((k as u32).wrapping_mul(0x54)).wrapping_add(0x3730);
    let x = f32::from_bits(m.r32(row + 0x48)) + f32::from_bits(m.r32(row + 0x4c));
    m.w32(b + 0x188, x.to_bits());
    let file = list_pick_random(m, e, if start { row + 0x18 } else { row });
    if file == 0 {
        return;
    }
    let kind = m.r32(b + 0xd8);
    e.call(m, ENTRY_BEGIN, &[a, kind, file]);
    let vol = m.r32(b + 0x178);
    let voice = cheer_request(m, e, a, b, vol);
    m.w32(b + 0x174, voice);
    m.w32(b + 0x18c, if start { !(k as u32) } else { (k as u32).wrapping_add(1) });
}

/// 0x2203a8.
pub fn play_cheer(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32, k: i32) {
    begin_cheer(m, e, b, k, false);
}

/// 0x220538.
pub fn start_cheer(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32, k: i32) {
    begin_cheer(m, e, b, k, true);
}

/// 0x21efb0: one box's tick. The sounds in its slots are blended into one position per voice (and one for the box),
/// the voices start or follow it, and the cheer is started / followed. `sp` is the frame (0x130 bytes: +0x70 and
/// +0x74 are the two words the audio object's virtual call fills).
pub fn update(m: &mut dyn Mem, e: &mut dyn LoadEnv, b: u32, sp: u32) {
    let n = m.r32(b + 0xd4) as i32;
    if n == 0 {
        return;
    }
    let nvoice = m.r32(b + 0x170) as i32;
    let mut vpos = [[0u32; 4]; 3];
    let mut wmax = [0i32; 3];
    let mut cnt = [0i32; 3];
    // the first slot in use
    let mut t6 = 0u32;
    let mut a1 = 1u32;
    let (mut p0, mut ent, mut t2);
    loop {
        let off = t6.wrapping_mul(12);
        t6 = a1;
        t2 = b.wrapping_add(0x20).wrapping_add(off);
        p0 = m.r32(t2);
        ent = m.r32(t2.wrapping_add(8));
        if p0 != 0 {
            break;
        }
        if (t6 as i32) < 15 {
            a1 = t6.wrapping_add(1);
            continue;
        }
        break;
    }
    let first = quad(m, p0);
    let mut pos = first;
    let k0 = m.r32(ent.wrapping_add(0x20)) as usize;
    vpos[k0] = first;
    let mut s7 = m.r32(t2.wrapping_add(4)) as i32;
    wmax[k0] = s7;
    let mut s0 = 0i32;
    let mut s4 = ent;
    let mut s6 = 1i32;
    if s6 < n {
        let few = nvoice < 2;
        let mut t5 = t6.wrapping_mul(12).wrapping_add(0x20);
        loop {
            let s1 = s6 + 1;
            let mut t2 = b.wrapping_add(t5);
            let t0;
            loop {
                t5 = t5.wrapping_add(0xc);
                s4 = m.r32(t2.wrapping_add(8));
                let w = m.r32(t2);
                t6 = t6.wrapping_add(1);
                if w != 0 {
                    t0 = w;
                    break;
                }
                if (t6 as i32) < 15 {
                    t2 = t2.wrapping_add(0xc);
                    continue;
                }
                t0 = w;
                break;
            }
            let t1 = m.r32(t2.wrapping_add(4)) as i32;
            let sx = f32::from_bits(m.r32(t0));
            let sy = f32::from_bits(m.r32(t0.wrapping_add(4)));
            let sz = f32::from_bits(m.r32(t0.wrapping_add(8)));
            let f4 = (t1 as f32) / (s7.wrapping_add(t1) as f32);
            for (k, s) in [sx, sy, sz].into_iter().enumerate() {
                let c = f32::from_bits(pos[k]);
                pos[k] = (c + (s - c) * f4).to_bits();
            }
            if s7 < t1 {
                s0 = s0.wrapping_add(s7 >> 2);
                s7 = t1;
            } else {
                s0 = s0.wrapping_add(t1 >> 2);
            }
            s6 = s1;
            if !few {
                let k = m.r32(s4.wrapping_add(0x20)) as usize;
                let t3 = wmax[k];
                let f4 = (t1 as f32) / (t3.wrapping_add(t1) as f32);
                for (j, s) in [sx, sy, sz].into_iter().enumerate() {
                    let c = f32::from_bits(vpos[k][j]);
                    vpos[k][j] = (c + (s - c) * f4).to_bits();
                }
                if t3 < t1 {
                    cnt[k] = cnt[k].wrapping_add(t3 >> 2);
                    wmax[k] = m.r32(t2.wrapping_add(4)) as i32;
                } else {
                    cnt[k] = cnt[k].wrapping_add(t1 >> 2);
                }
            }
            if !(s6 < n) {
                break;
            }
        }
    }
    s7 = s7.wrapping_add(s0);
    let a = m.r32(AUDIO);
    let kind = m.r32(b + 0xd8);
    if nvoice == 1 {
        let voice = m.r32(s4.wrapping_add(4));
        if (voice as i32) >= 0 {
            put_quad(m, s4.wrapping_add(0x10), pos);
            let mon = e.call(m, MONITOR_FIND, &[a, voice, 1]);
            if mon != 0 {
                m.w32(mon.wrapping_add(0x64), s7 as u32);
            } else {
                let id = m.r32(s4);
                let i = push_request(m, e, a);
                m.w32(arr(m, a, 0xa40, i), kind);
                m.w32(arr(m, a, 0x7c, i), nvoice as u32);
                m.w32(arr(m, a, 0xa44, i), id);
                m.w32(a.wrapping_add(i.wrapping_mul(4)).wrapping_add(0x28), s4.wrapping_add(0x10));
                set_volume(m, a, i, s7 as u32);
                let v = m.r32(b + 0x17c);
                m.w32(arr(m, a, 0x40, i), v);
                m.w32(arr(m, a, 0x78, i), 1);
                copy_req_words(m, a, i);
                let l = m.r32(b);
                let v = finish_request(m, e, a, i, b, l);
                m.w32(s4.wrapping_add(4), v);
            }
        }
    } else {
        for s6 in 0..3u32 {
            let s4 = b + 0xe0 + s6 * 0x30;
            if m.r32(s4 + 8) == 0 {
                continue;
            }
            let voice = m.r32(s4 + 4);
            if (voice as i32) < 0 {
                continue;
            }
            let k = m.r32(s4 + 0x20) as usize;
            put_quad(m, s4 + 0x10, vpos[k]);
            let mon = e.call(m, MONITOR_FIND, &[a, voice, 1]);
            if mon != 0 {
                m.w32(mon.wrapping_add(0x64), wmax[k].wrapping_add(cnt[k]) as u32);
            } else {
                let id = m.r32(s4);
                let i = push_request(m, e, a);
                m.w32(arr(m, a, 0xa40, i), kind);
                m.w32(arr(m, a, 0x7c, i), 1);
                m.w32(arr(m, a, 0xa44, i), id);
                m.w32(a.wrapping_add(i.wrapping_mul(4)).wrapping_add(0x28), s4 + 0x10);
                set_volume(m, a, i, wmax[k].wrapping_add(cnt[k]) as u32);
                let v = m.r32(b + 0x17c);
                m.w32(arr(m, a, 0x40, i), v);
                let l = m.r32(b);
                m.w32(arr(m, a, 0x68, i), l);
                m.w32(arr(m, a, 0x78, i), 1);
                copy_req_words(m, a, i);
                let v = finish_request(m, e, a, i, b, l);
                m.w32(s4 + 4, v);
            }
        }
    }
    // the cheer
    m.w32(sp + 0x70, 0x1000);
    m.w32(sp + 0x74, 0x1000);
    let state = m.r32(b + 0x180);
    let mut go = true;
    if state != 0 {
        let vt = m.r32(a + 4);
        let off = m.r16(vt + 0x80) as i16 as i32 as u32;
        let target = m.r32(vt + 0x84);
        let l = m.r32(b);
        let s0 = e.call(m, target, &[a.wrapping_add(off), l, sp + 0x70, sp + 0x74]);
        if s0.wrapping_sub(1) < 2 {
            m.w32(b + 0x18c, s0);
        }
        if m.r32(b + 0x180) == 1 && s0 != 3 {
            stop_cheer(m, e, b);
            let row = a.wrapping_add(s0.wrapping_mul(0x54));
            let file = list_pick_random(m, e, row.wrapping_add(0x3760));
            if file != 0 {
                e.call(m, ENTRY_BEGIN, &[a, kind, file]);
                let v74 = m.r32(sp + 0x74) as i32;
                let vol = (s7.wrapping_mul(v74) >> 12) as u32;
                let voice = cheer_request(m, e, a, b, vol);
                m.w32(b + 0x174, voice);
                if (voice as i32) < 0 {
                    go = false;
                } else {
                    m.w32(b + 0x180, 2);
                }
            }
        }
    }
    if go {
        let voice = m.r32(b + 0x174);
        if (voice as i32) >= 0 {
            let mon = e.call(m, MONITOR_FIND, &[a, voice, 2]);
            if mon == 0 {
                stop_cheer(m, e, b);
                m.w32(b + 0x18c, 0);
            } else {
                let v = if m.r32(b + 0x180) == 2 {
                    let v70 = m.r32(sp + 0x70);
                    e.call(m, MONITOR_SET, &[mon, v70]);
                    let v74 = m.r32(sp + 0x74) as i32;
                    (s7.wrapping_mul(v74) >> 12) as u32
                } else {
                    s7 as u32
                };
                m.w32(mon.wrapping_add(0x64), v);
            }
        }
    }
    put_quad(m, b + 0x10, pos);
    m.w32(b + 0x178, s7 as u32);
}

// ---- cController, cCrowdSys, IniReader, Monitor & PathX tail (row G10t) ----

pub const MEM_DELETE: u32 = 0x114f80;
pub const BANK_MONITOR_VF5: u32 = 0x20c0f8;
pub const BANK_MONITOR_VF6: u32 = 0x20c148;
pub const CROWD_SOUND_LIST_CTOR: u32 = 0x21eca8;
pub const PTR_ARRAY_FREE_ALL: u32 = 0x21edf0;
pub const CROWD_SYS_FREE: u32 = 0x220bb8;
pub const CROWD_SYS_STOP_ALL: u32 = 0x221330;
pub const INI_SKIP_SPACE: u32 = 0x2217f0;
pub const INI_READ_TOKEN: u32 = 0x221c90;
pub const SPEECH_SYS_INIT: u32 = 0x228728;
pub const SPEECH_SYS_DTOR: u32 = 0x2287d8;
pub const FILE_GET_SIZE: u32 = 0x2c7ec8;
pub const FILE_OPEN_SYNC: u32 = 0x2c8d98;
pub const FILE_READ_SYNC: u32 = 0x2c8e08;
pub const FILE_CLOSE_SYNC: u32 = 0x2c8e48;
pub const EAMEM_ALLOC0: u32 = 0x2ccf98;
pub const SND_LOCK: u32 = 0x2d0908;
pub const SND_UNLOCK: u32 = 0x2d0938;
pub const SND_BANK_REGISTER: u32 = 0x2d0f98;
pub const SND_BANK_GET_RESIDENT_SIZE: u32 = 0x2d1428;
pub const SND_BANK_RELOCATE: u32 = 0x2d1448;
pub const SND_HANDLE_SET_PITCH_BEND: u32 = 0x2d6a18;
pub const SSCANF: u32 = 0x2fcaa0;

pub const VT_CONTROLLER: u32 = 0x39fad0;
pub const VT_CROWD_SYS: u32 = 0x39fc40;
pub const FMT_FLOAT: u32 = 0x39fd48;
pub const NAME_PATHX_BKH: u32 = 0x39fec8;
pub const NAME_PATHX_BNK: u32 = 0x39fed8;
pub const G_MONITOR: u32 = 0x343128;
pub const G_PATHX_TASK: u32 = 0x34313c;
pub const G_PATHX_SLOTS: u32 = 0x343158;

/// `CrowdSoundList_Destruct` 0x21ece0(this, flags): call `PtrArray_FreeAll(this)` and `Mem_Delete(this)` when
/// `flags & 1 != 0`.
pub fn crowd_sound_list_destruct(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32, flags: u32) {
    e.call(m, PTR_ARRAY_FREE_ALL, &[this]);
    if (flags & 1) != 0 {
        e.call(m, MEM_DELETE, &[this]);
    }
}

/// `CrowdSoundSet_Destruct` 0x221538(this, flags): destruct the three embedded `CrowdSoundList`s at `+0x30`, `+0x18`,
/// `+0x00` with flags `2`, then `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn crowd_sound_set_destruct(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32, flags: u32) {
    crowd_sound_list_destruct(m, e, this.wrapping_add(0x30), 2);
    crowd_sound_list_destruct(m, e, this.wrapping_add(0x18), 2);
    crowd_sound_list_destruct(m, e, this, 2);
    if (flags & 1) != 0 {
        e.call(m, MEM_DELETE, &[this]);
    }
}

/// `cCrowdSys::cCrowdSys` 0x220850(this): init `cSpeechSys`, clear `+0x3728`/`+0x372c`, set vtable `0x39fc40`,
/// construct 3×3 `CrowdSoundList`s at `+0x3730` (stride `0x54`), publish `gAudio` (`0x343120`), and return `this`.
pub fn crowd_sys_ctor(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32) -> u32 {
    e.call(m, SPEECH_SYS_INIT, &[this]);
    m.w32(this + 0x3728, 0);
    m.w32(this + 0x372c, 0);
    m.w32(this + 4, VT_CROWD_SYS);
    let mut set = this.wrapping_add(0x3730);
    for _ in 0..3 {
        e.call(m, CROWD_SOUND_LIST_CTOR, &[set]);
        e.call(m, CROWD_SOUND_LIST_CTOR, &[set.wrapping_add(0x18)]);
        e.call(m, CROWD_SOUND_LIST_CTOR, &[set.wrapping_add(0x30)]);
        set = set.wrapping_add(0x54);
    }
    m.w32(AUDIO, this);
    this
}

/// `cCrowdSys::~cCrowdSys` 0x2208e0(this, flags): restore vtable `0x39fc40`, call `CrowdSys_Free`, clear `gAudio`,
/// destruct the 3 `CrowdSoundSet`s in reverse (`+0x37d8`, `+0x3784`, `+0x3730`), and tail-call `cSpeechSys::~cSpeechSys`.
pub fn crowd_sys_dtor(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32, flags: u32) {
    m.w32(this + 4, VT_CROWD_SYS);
    e.call(m, CROWD_SYS_FREE, &[this]);
    m.w32(AUDIO, 0);
    let start = this.wrapping_add(0x3730);
    if start != 0 {
        let mut cur = this.wrapping_add(0x382c);
        while cur != start {
            cur = cur.wrapping_sub(0x54);
            crowd_sound_set_destruct(m, e, cur, 0);
        }
    }
    e.call(m, SPEECH_SYS_DTOR, &[this, flags]);
}

/// `cController::cController` 0x21e8a0(this): construct base `cCrowdSys`, zero `+0x382c`, set vtable `0x39fad0`,
/// and return `this`.
pub fn controller_ctor(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32) -> u32 {
    crowd_sys_ctor(m, e, this);
    m.w32(this + 0x382c, 0);
    m.w32(this + 4, VT_CONTROLLER);
    this
}

/// `cController::~cController` 0x21e8d8(this, flags): restore vtable `0x39fad0` and tail-call `cCrowdSys::~cCrowdSys`.
pub fn controller_dtor(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32, flags: u32) {
    m.w32(this + 4, VT_CONTROLLER);
    crowd_sys_dtor(m, e, this, flags);
}

/// `SoundId_Split` 0x220f80(_this, packed, out_lo, out_hi): split `packed` into `packed & 0xff` and `(packed as i32) >> 8`.
pub fn sound_id_split(m: &mut dyn Mem, packed: u32, out_lo: u32, out_hi: u32) {
    m.w32(out_lo, packed & 0xff);
    m.w32(out_hi, ((packed as i32) >> 8) as u32);
}

/// `cCrowdSys::vf5_2213c8` 0x2213c8(this): call `CrowdSys_StopAll(this)` and tail-call `cBankMonitor::vf5(this)`.
pub fn crowd_sys_vf5(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32) {
    e.call(m, CROWD_SYS_STOP_ALL, &[this]);
    e.call(m, BANK_MONITOR_VF5, &[this]);
}

/// `cCrowdSys::vf6_2214a8` 0x2214a8(this, arg64): forward `(this, arg64)` to `cBankMonitor::vf6` (`0x20c148`).
pub fn crowd_sys_vf6(m: &mut dyn Mem, e: &mut dyn LoadEnv, this: u32, arg64: u64) -> u32 {
    e.call(m, BANK_MONITOR_VF6, &[this, arg64 as u32, (arg64 >> 32) as u32])
}

/// `Audio_SetHandlePitchBend` 0x2215a8(obj, bend): under `SND_Lock`/`SND_Unlock`, call
/// `SndHandle_SetPitchBend(*(obj+4), bend)`.
pub fn audio_set_handle_pitch_bend(m: &mut dyn Mem, e: &mut dyn LoadEnv, obj: u32, bend: u32) {
    e.call(m, SND_LOCK, &[]);
    let h = m.r32(obj + 4);
    e.call(m, SND_HANDLE_SET_PITCH_BEND, &[h, bend]);
    e.call(m, SND_UNLOCK, &[]);
}

/// `IniReader_ParseFloat` 0x221d58(reader, out_sp): skip whitespace, read next token into `reader+0xc`, parse with
/// `sscanf(reader+0xc, "%f", out_sp)`, and return the parsed `f32`.
pub fn ini_reader_parse_float(m: &mut dyn Mem, e: &mut dyn LoadEnv, reader: u32, out_sp: u32) -> f32 {
    e.call(m, INI_SKIP_SPACE, &[reader]);
    e.call(m, INI_READ_TOKEN, &[reader]);
    e.call(m, SSCANF, &[reader.wrapping_add(0xc), FMT_FLOAT, out_sp]);
    f32::from_bits(m.r32(out_sp))
}

/// `Monitor_TickCallback` 0x222ae0(): invoke virtual `+0x2c` (this-adjust `+0x28`) on `gMonitor` (`0x343128`).
pub fn monitor_tick_callback(m: &mut dyn Mem, e: &mut dyn LoadEnv) {
    let mon = m.r32(G_MONITOR);
    let vt = m.r32(mon + 4);
    let adj = (m.r16(vt + 0x28) as i16) as i32 as u32;
    let f = m.r32(vt + 0x2c);
    e.call(m, f, &[mon.wrapping_add(adj)]);
}

/// `PathX_FindFreeSlot` 0x222fb0(): return the first index `0..24` in `gPathXSlots` (`0x343158`, stride `0x3c`)
/// whose first word is `< 0`, or `24` if none.
pub fn pathx_find_free_slot(m: &dyn Mem) -> u32 {
    for i in 0..24u32 {
        if (m.r32(G_PATHX_SLOTS + i * 0x3c) as i32) < 0 {
            return i;
        }
    }
    24
}

/// `PathX_ReadFile` 0x223098(path, sp): query `File_GetSize`, allocate `"PATHXbnk"` via `EAMem_Alloc0`, read the
/// whole file synchronously (`File_OpenSync` / `File_ReadSync` / `File_CloseSync`), and return the buffer pointer.
pub fn pathx_read_file(m: &mut dyn Mem, e: &mut dyn LoadEnv, path: u32, sp: u32) -> u32 {
    let sz = e.call(m, FILE_GET_SIZE, &[path]);
    let buf = e.call(m, EAMEM_ALLOC0, &[NAME_PATHX_BNK, sz, 0]);
    e.call(m, FILE_OPEN_SYNC, &[path, 1, 0, sp]);
    let fh = m.r32(sp);
    e.call(m, FILE_READ_SYNC, &[fh, 0, buf, sz, 0]);
    let fh2 = m.r32(sp);
    e.call(m, FILE_CLOSE_SYNC, &[fh2, 0]);
    buf
}

/// `PathX_LoadBank` 0x222fe8(slot, path, sp): read bank file via `PathX_ReadFile`, store pointer at
/// `gPathXSlots[slot]+0x20` (returning `-0x27` if null), register with `SndBank_Register`, and if status `== 7`,
/// allocate `"PATHXbkh"` resident header (`SndBank_GetResidentSize`) and relocate via `SndBank_Relocate`. Returns
/// the bank ID written to `*sp`.
pub fn pathx_load_bank(m: &mut dyn Mem, e: &mut dyn LoadEnv, slot: u32, path: u32, sp: u32) -> u32 {
    m.w32(sp, 0xffff_ffff);
    let buf = pathx_read_file(m, e, path, sp.wrapping_add(0x10));
    let dst = G_PATHX_SLOTS.wrapping_add(slot.wrapping_mul(0x3c)).wrapping_add(0x20);
    m.w32(dst, buf);
    if buf == 0 {
        return (-0x27i32) as u32;
    }
    let rc = e.call(m, SND_BANK_REGISTER, &[sp, buf]);
    if rc == 7 {
        let bank_id = m.r32(sp);
        let res_sz = e.call(m, SND_BANK_GET_RESIDENT_SIZE, &[bank_id]);
        let res = e.call(m, EAMEM_ALLOC0, &[NAME_PATHX_BKH, res_sz, 0]);
        if res != 0 {
            let bank_id2 = m.r32(sp);
            e.call(m, SND_BANK_RELOCATE, &[res, bank_id2]);
            m.w32(dst, res);
        }
    }
    m.r32(sp)
}

/// `PathX_PollTask` 0x223128(): invoke `*(0x34313c)` if non-null and return `0`.
pub fn pathx_poll_task(m: &mut dyn Mem, e: &mut dyn LoadEnv) -> u32 {
    let cb = m.r32(G_PATHX_TASK);
    if cb != 0 {
        e.call(m, cb, &[]);
    }
    0
}
