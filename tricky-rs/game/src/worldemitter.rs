//! The world sound emitters' small functions (board row G10a2), each the game's and checked in the function runner
//! (`tests/worldemitter.rs`): `WorldEmitter_Clear` 0x22d080, `_FindFree` 0x22d188, `_Find` 0x22d1b8, `_Matches`
//! 0x22bf30, `_Falloff` 0x22bc40, `_Stop` 0x22c5a0, `_StopAll` 0x22d408, `_UpdateAll` 0x22d370.
//!
//! The emitter table is 40 records of 0x50 bytes (20 words) a manager points at with its first word. Word 0 is the
//! state (0 free, 1 playing), +4 / +8 / +0x3c the key `Matches` compares, +0x30 the voice handle (-1 none), +0x34 a
//! "still wanted" flag `UpdateAll` clears every pass, +0x38 which voice engine the handle is in, +0x40 -1 once stopped.
//!
//! The cone's angles take `math::asin`, not `math::asin_ee`: `tests/worldemitter.rs` runs the original with every
//! float rule nearest (as this port's floats are) and feeds it arguments beyond 0.7071, where the EE's chopped square
//! root no longer matches (tried 2026-10-17).

/// Words in an emitter record and the number of records.
pub const WORDS: usize = 20;
pub const COUNT: usize = 40;

/// `WorldEmitter_Clear`: free the record and forget its voice.
pub fn clear(e: &mut [u32]) {
    e[0] = 0;
    e[0x10] = 0xffff_ffff;
    e[0xd] = 0;
    e[0xc] = 0xffff_ffff;
}

/// `WorldEmitter_Matches`: the record's key words (+4, +8, +0x3c) equal `a`, `b`, `c`.
pub fn matches(e: &[u32], a: u32, b: u32, c: u32) -> bool {
    a == e[1] && b == e[2] && c == e[0xf]
}

/// `WorldEmitter_FindFree`: the first record whose state is 0.
pub fn find_free(t: &[u32]) -> Option<usize> {
    (0..COUNT).find(|i| t[i * WORDS] == 0)
}

/// `WorldEmitter_Find`: the first in-use record (state not 0) that matches the key.
pub fn find(t: &[u32], a: u32, b: u32, c: u32) -> Option<usize> {
    (0..COUNT).find(|&i| t[i * WORDS] != 0 && matches(&t[i * WORDS..], a, b, c))
}

/// `WorldEmitter_Falloff`: the volume factor of distance fraction `x` for curve `kind` (0 square, 1 and 3 hyperbolic,
/// 2 linear, 4 squared linear, 5 flat to 0.7 then linear). Another kind returns `f3`, whatever the register held.
pub fn falloff(kind: f32, x: f32, f3: f32) -> f32 {
    let k = if kind.is_nan() { -1 } else { kind as i32 };
    match k {
        0 => 1.0 - x * x,
        1 => 1.0 - x / (1.5 - x * 0.5),
        2 => 1.0 - x,
        3 => {
            let t = 1.0 - x;
            t / (1.5 - t * 0.5)
        }
        4 => {
            let t = 1.0 - x;
            t * t
        }
        5 => {
            if x <= f32::from_bits(0x3f33_3333) {
                1.0
            } else {
                (1.0 - x) / f32::from_bits(0x3e99_999a)
            }
        }
        _ => f3,
    }
}

/// What the emitters call out to: the two voice engines' stop (0x20c2a0, 0x220f98: the global at 0x343050, the
/// handle, the fade 0.25), and the per-record update (0x22bf60).
pub trait Env {
    fn global(&mut self) -> u32;
    fn stop_a(&mut self, global: u32, handle: u32);
    fn stop_b(&mut self, global: u32, handle: u32);
    fn update(&mut self, index: usize);
}

/// `WorldEmitter_Stop`: stop the record's voice in the engine it is in and forget it.
pub fn stop(e: &mut [u32], env: &mut dyn Env) {
    let g = env.global();
    if e[0xe] == 0 {
        env.stop_a(g, e[0xc]);
    } else {
        env.stop_b(g, e[0xc]);
    }
    e[0x10] = 0xffff_ffff;
    e[0xc] = 0xffff_ffff;
}

/// `WorldEmitter_StopAll`: free and stop every playing record.
pub fn stop_all(t: &mut [u32], env: &mut dyn Env) {
    for i in 0..COUNT {
        if t[i * WORDS] == 1 {
            t[i * WORDS] = 0;
            stop(&mut t[i * WORDS..(i + 1) * WORDS], env);
        }
    }
}

/// `WorldEmitter_UpdateAll`: every playing record not marked wanted is freed and stopped, the others updated;
/// the wanted mark is cleared on each.
pub fn update_all(t: &mut [u32], env: &mut dyn Env) {
    for i in 0..COUNT {
        if t[i * WORDS] == 1 {
            if t[i * WORDS + 0xd] == 0 {
                t[i * WORDS] = 0;
                stop(&mut t[i * WORDS..(i + 1) * WORDS], env);
            } else {
                env.update(i);
            }
            t[i * WORDS + 0xd] = 0;
        }
    }
}

/// Reads of the game's memory by address (a word at a time), for the functions that walk the emitter grid.
pub trait Mem {
    fn u32(&self, addr: u32) -> u32;
    fn f32(&self, addr: u32) -> f32 {
        f32::from_bits(self.u32(addr))
    }
}

/// `WorldEmitter_GatherForListener`'s call out: `FUN_0022d260` (the volume the falloff gave, the object at 0x34370c (`global`),
/// the instance, the record's index, the caller's third argument, the record's word +4, 1 for a faded sound or 0, the
/// sound's centre).
pub trait GatherEnv {
    fn add_sound(&mut self, volume: f32, global: u32, instance: u32, index: u32, arg3: u64, record_word_1: u32, faded: u32, centre: [f32; 3]);
}

/// `Vec3_Dot` 0x137140.
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `WorldEmitter_GatherForListener` 0x22b370: the emitters near a listener at `listener` (the address of its three
/// floats) in the grid of the emitter manager at `sys` (+0x10 width, +0x14 height, +0x18 the cells of 0x14 bytes
/// whose +0x10 heads a list of {instance group, next}, +0x1c / +0x20 the grid's origin, +0x24 the cell size). Each
/// group is an instance (position at +0x30) and a block of records (count at +4, records from +8: kind 0 a sphere,
/// 1 a box, 2 a cone, 3 a plain radius; of 0x1c, 0x30, 0x30 and 0x18 bytes). A record the listener is inside gives
/// one `add_sound` with the volume the record's falloff curve gives. The pointers to the records are kept on the
/// original's stack four to a group: more than four records in a group is not modelled.
pub fn gather_for_listener(m: &dyn Mem, sys: u32, listener: u32, arg3: u64, env: &mut dyn GatherEnv) {
    let cell = m.f32(sys + 0x24);
    let lis = [m.f32(listener), m.f32(listener + 4), m.f32(listener + 8)];
    let w = m.u32(sys + 0x10) as i32;
    let yy = (((lis[1] - m.f32(sys + 0x20)) + cell) / cell) as i32;
    let xx = (((lis[0] - m.f32(sys + 0x1c)) + cell) / cell) as i32;
    let idx = w.wrapping_mul(yy.wrapping_sub(1)).wrapping_add(xx.wrapping_sub(1));
    if idx < 0 || idx >= w.wrapping_mul(m.u32(sys + 0x14) as i32) {
        return;
    }
    let mut node = m.u32((idx as u32).wrapping_mul(0x14).wrapping_add(m.u32(sys + 0x18)) + 0x10);
    if node == 0 {
        return;
    }
    let global = m.u32(0x34370c);
    let half_pi = f32::from_bits(0x3fc9_0fdb);
    while node != 0 {
        let group = m.u32(node);
        let inst = m.u32(group);
        let p = [m.f32(inst + 0x30), m.f32(inst + 0x34), m.f32(inst + 0x38)];
        let block = m.u32(group + 4);
        let count = m.u32(block + 4) as i32;
        // the records' addresses, by their kinds' sizes
        let mut recs = Vec::new();
        let mut at = block + 8;
        for _ in 0..count.max(0) {
            recs.push(at);
            at = at.wrapping_add(match m.u32(at) {
                0 => 0x1c,
                1 | 2 => 0x30,
                3 => 0x18,
                _ => 0,
            });
        }
        for (i, &r) in recs.iter().enumerate() {
            let i = i as u32;
            let off = [m.f32(r + 8), m.f32(r + 0xc), m.f32(r + 0x10)];
            let c = [p[0] + off[0], p[1] + off[1], p[2] + off[2]];
            let d = [lis[0] - c[0], lis[1] - c[1], lis[2] - c[2]];
            let kind = m.u32(r);
            match kind {
                0 => {
                    let dist = dot3(d, d).sqrt().abs();
                    let rad = m.f32(r + 0x14);
                    if dist < rad {
                        let f = super::worldemitter::falloff(m.f32(r + 0x18), dist / rad, 0.0);
                        env.add_sound(f, global, inst, i, arg3, m.u32(r + 4), 1, c);
                    }
                }
                1 => {
                    let a = [m.f32(r + 0x20), m.f32(r + 0x24), m.f32(r + 0x28)];
                    // up x a, normalised, then a x that
                    let u = [0.0f32, 0.0, 1.0];
                    let r0 = u[1] * a[2] - u[2] * a[1];
                    let r1 = u[2] * a[0] - u[0] * a[2];
                    let r2 = u[0] * a[1] - u[1] * a[0];
                    let q = dot3([r0, r1, r2], [r0, r1, r2]).sqrt();
                    let inv = 1.0 / q;
                    let (n0, n1, n2) = (r0 * inv, r1 * inv, r2 * inv);
                    let s0 = a[1] * n2 - a[2] * n1;
                    let s1 = a[2] * n0 - a[0] * n2;
                    let s2 = a[0] * n1 - a[1] * n0;
                    let e5 = m.f32(r + 0x14);
                    let e6 = m.f32(r + 0x18);
                    let e7 = m.f32(r + 0x1c);
                    let sd = (s0 * d[0] + s1 * d[1]) + s2 * d[2];
                    let ad = (a[0] * d[0] + a[1] * d[1]) + a[2] * d[2];
                    let nd = (n0 * d[0] + n1 * d[1]) + n2 * d[2];
                    let t_a = if e5 == 0.0 { 0.0 } else { (ad * ad) / (e5 * e5) };
                    let t_n = if e6 == 0.0 { 0.0 } else { (nd * nd) / (e6 * e6) };
                    let t_s = if e7 == 0.0 { 0.0 } else { (sd * sd) / (e7 * e7) };
                    let total = (t_a + t_n) + t_s;
                    if total < 1.0 {
                        let f = super::worldemitter::falloff(m.f32(r + 0x2c), total, 0.0);
                        env.add_sound(f, global, inst, i, arg3, m.u32(r + 4), 1, c);
                    }
                }
                2 => {
                    let dist = dot3(d, d).sqrt().abs();
                    let rad = m.f32(r + 0x14);
                    if dist < rad {
                        let inv = 1.0 / dist;
                        let dn = [d[0] * inv, d[1] * inv, d[2] * inv];
                        let dir = [m.f32(r + 0x18), m.f32(r + 0x1c), m.f32(r + 0x20)];
                        let cosine = (dn[0] * dir[0] + dn[1] * dir[1]) + dn[2] * dir[2];
                        let cone = m.f32(r + 0x28);
                        if cone < cosine {
                            let f1 = super::worldemitter::falloff(m.f32(r + 0x24), dist / rad, 0.0);
                            let wide = half_pi - crate::math::asin(cone);
                            let ang = half_pi - crate::math::asin(cosine);
                            let f2 = super::worldemitter::falloff(m.f32(r + 0x2c), ang / wide, 0.0);
                            env.add_sound(f1 * f2, global, inst, i, arg3, m.u32(r + 4), 1, c);
                        }
                    }
                }
                3 => {
                    let dist = dot3(d, d).sqrt().abs();
                    if dist < m.f32(r + 0x14) {
                        env.add_sound(1.0, global, inst, i, arg3, m.u32(r + 4), 0, c);
                    }
                }
                _ => {}
            }
        }
        node = m.u32(node + 4);
    }
}

/// The game's `(int)(v * 127.0)` (`mul.s`, `cvt.w.s`: toward zero, out of range or NaN by the sign).
fn volume_int(v: f32) -> i32 {
    let f = v * 127.0;
    let bits = f.to_bits();
    if (bits >> 23) & 0xff >= 158 {
        if bits >> 31 != 0 {
            i32::MIN
        } else {
            i32::MAX
        }
    } else {
        f as i32
    }
}

/// What `WorldEmitter_Update` calls out to.
pub trait UpdateEnv: crate::sndqueue::QueueEnv {
    /// 0x221000 (the audio system, the handle, the volume).
    fn engine_volume(&mut self, handle: i32, volume: i32);
    /// `BankMonitor_Find` 0x20c200 (the audio system, the handle, 1): the monitor or 0.
    fn bank_find(&mut self, handle: i32) -> u32;
    /// A word stored to the monitor.
    fn poke(&mut self, addr: u32, v: u32);
    /// 0x22c638 (the record, its key).
    fn key_known(&mut self, addr: u32, key: u32) -> bool;
    /// 0x22c610 (the record, its key, two out words, the rider): the two words when it finds the sound.
    fn resolve(&mut self, addr: u32, key: u32, rider: u32) -> Option<(u32, u32)>;
    /// 0x22dc88 (the grid at 0x34370c, +4, +8, the record).
    fn grid_blocks(&mut self, a: u32, b: u32, addr: u32) -> bool;
    /// `*(gApp[0x730] + rider * 0x80 + 0xa4)`.
    fn rider_bank(&mut self, rider: u32) -> i32;
    /// `Audio_GetListenerRider` 0x20ed20 (the audio system, -1).
    fn listener_rider(&mut self) -> i64;
    /// 0x22dc38 (the grid, the key).
    fn ambient_blocked(&mut self, key: u32) -> bool;
    /// 0x22c648 (the record, the key, the record's +0x40): the bank written through the pointer.
    fn load_trigger_banks(&mut self, addr: u32, key: u32, bank: &mut u32) -> bool;
    /// `Audio_LoadAmbientBank` 0x21a548 (the audio system, the bank).
    fn load_ambient_bank(&mut self, bank: u32) -> i64;
    /// 0x22e240 (the grid, the key).
    fn has_queued(&mut self, key: u32) -> bool;
    /// 0x22e270 (the grid, +4).
    fn is_queued_other(&mut self, a: u32) -> bool;
    /// The audio system's +0x372c.
    fn voice_table(&mut self) -> u32;
    /// 0x21fc98 (the table entry, the audio system, the second word).
    fn voice_start(&mut self, entry: u32, arg: u32) -> i64;
}

/// 0x2212a0: start the voice of the last queued entry and drop the entry; the handle (-1 without a table).
fn finish_voice(q: &mut crate::sndqueue::Queue, env: &mut dyn UpdateEnv, arg: u32) -> u32 {
    let table = env.voice_table();
    if table == 0 {
        return 0xffff_ffff;
    }
    let i = q.count as usize;
    let r = env.voice_start(q.a68[i].wrapping_mul(400).wrapping_add(table), arg);
    let mut v = r as u32;
    if r >= 0 {
        v = (r as u32) << 8 | q.a68[i];
    }
    q.count -= 1;
    v
}

/// `WorldEmitter_Update` 0x22bf60: keep a record's voice going. With a voice handle: pass the volume on to the
/// engine it is in. Without one: look the sound up and either put a voice on the queue (the engine kind, at once)
/// or queue a sound (the other kind, behind the grid's checks and the ambient bank for kind 6), or free the
/// record when anything refuses. `addr` is the record's address.
pub fn update(e: &mut [u32], addr: u32, q: &mut crate::sndqueue::Queue, env: &mut dyn UpdateEnv) {
    use crate::sndqueue::{queue_begin, queue_push, queue_volume};
    let handle = e[0xc];
    if handle != 0xffff_ffff {
        let v = volume_int(f32::from_bits(e[4]));
        if e[0xe] != 0 {
            env.engine_volume(handle as i32, v);
            return;
        }
        let b = env.bank_find(handle as i32);
        if b != 0 {
            env.poke(b.wrapping_add(0x64), v as u32);
        }
        return;
    }
    if env.key_known(addr, e[3]) {
        let Some((_, arg)) = env.resolve(addr, e[3], e[0xf]) else {
            clear(e);
            return;
        };
        e[0xe] = 1;
        let i = queue_push(q);
        queue_volume(q, i, volume_int(f32::from_bits(e[4])) as i64);
        q.a40[i] = env.get_slot(4);
        if e[5] != 0 {
            q.a78[i] = 1;
            q.emitter[i] = addr.wrapping_add(0x20);
            q.a7c[i] = 0;
        }
        q.a68[i] = e[0xf];
        e[0xc] = finish_voice(q, env, arg);
        return;
    }
    if env.grid_blocks(e[1], e[2], addr) {
        clear(e);
        return;
    }
    let Some((kind, id)) = env.resolve(addr, e[3], e[0xf]) else {
        clear(e);
        return;
    };
    e[0xe] = 0;
    let mut loaded: i64 = -1;
    if kind == 6 {
        let mine = env.rider_bank(e[0xf]) as i64;
        let listener = env.listener_rider();
        if mine != listener || env.ambient_blocked(e[3]) {
            clear(e);
            return;
        }
        if e[0x10] == 0xffff_ffff {
            let mut bank = e[0x10];
            let ok = env.load_trigger_banks(addr, e[3], &mut bank);
            e[0x10] = bank;
            if !ok {
                clear(e);
                return;
            }
        }
        loaded = env.load_ambient_bank(e[0x10]);
    }
    if env.has_queued(e[3]) && !env.is_queued_other(e[1]) {
        clear(e);
        return;
    }
    if kind == 6 && loaded != 1 {
        return;
    }
    let i = queue_begin(q, kind, id, Some(volume_int(f32::from_bits(e[4])) as i64));
    q.a40[i] = env.get_slot(4);
    if e[5] != 0 {
        q.a78[i] = 1;
        q.emitter[i] = addr.wrapping_add(0x20);
        q.a7c[i] = 0;
    }
    e[0xc] = env.play_pending() as u32;
}

// ---- AudioTGroups & WorldTriggerMgr tail (row G10r, on engine memory) ----

use crate::eamem;
use crate::pf::Mem as PfMem;
use crate::ps2f::F;

pub const MEM_DELETE: u32 = 0x114f80;
pub const MEM_FREE: u32 = 0x14a040;
pub const AUDIO_TGROUPS_BIND: u32 = 0x22a5f0;
pub const AUDIO_TGROUPS_ADD_TO_GRID: u32 = 0x22b008;
pub const WORLD_EMITTER_STOP: u32 = 0x22c5a0;
pub const FILESYS_EXISTS: u32 = 0x2c7e38;
pub const FILE_GET_SIZE: u32 = 0x2c7ec8;
pub const FILE_LOAD: u32 = 0x2c8a70;
pub const EAMEM_ALLOC: u32 = 0x2ccf70;
pub const EAMEM_FREE: u32 = 0x2ccfc0;

pub const G_WORLD_GRID: u32 = 0x346488;
pub const G_WORLD_TRIGGER_MGR: u32 = 0x34370c;

pub const NAME_AUDIO_TGROUPS: u32 = 0x3a0888;
pub const NAME_TEMP_GRID_NODES: u32 = 0x3a0898;
pub const NAME_GRID_NODES: u32 = 0x3a08a8;
pub const NAME_WORLD_TRIGGER_INSTANCES: u32 = 0x3a15e0;

/// Calls out for the `AudioTGroups_*` and `WorldTriggerMgr_*` functions on engine memory.
pub trait WorldTriggerEnv {
    fn call(&mut self, m: &mut PfMem, target: u32, args: &[u32]) -> u32;
}

/// `AudioTGroups_Init` 0x22a470(this): zero all fields `+0x00..=+0x30` and return `this`.
pub fn tgroups_init(m: &mut PfMem, this: u32) -> u32 {
    for off in (0..=0x30).step_by(4) {
        m.w32(this + off, 0);
    }
    this
}

/// `AudioTGroups_Free` 0x22a588(this): release the raw file buffer (`EAMem_Free` `+0`), grid cells (`Mem_Free`
/// `+0x18`), and grid node array (`Mem_Free` `+0x2c`), zeroing each pointer after freeing.
pub fn tgroups_free(m: &mut PfMem, env: &mut dyn WorldTriggerEnv, this: u32) {
    let p0 = m.r32(this);
    if p0 != 0 {
        env.call(m, EAMEM_FREE, &[p0]);
        m.w32(this, 0);
    }
    let p1 = m.r32(this + 0x18);
    if p1 != 0 {
        env.call(m, MEM_FREE, &[p1]);
        m.w32(this + 0x18, 0);
    }
    let p2 = m.r32(this + 0x2c);
    if p2 != 0 {
        env.call(m, MEM_FREE, &[p2]);
        m.w32(this + 0x2c, 0);
    }
}

/// `AudioTGroups_Delete` 0x22a4c8(this, flags): free buffers via `AudioTGroups_Free`, then `Mem_Delete(this)` when
/// `flags & 1 != 0`.
pub fn tgroups_delete(m: &mut PfMem, env: &mut dyn WorldTriggerEnv, this: u32, flags: u32) {
    tgroups_free(m, env, this);
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `AudioTGroups_Load` 0x22a518(this, path): if `FileSys_Exists(path)`, record `File_GetSize(path)` at `+4`,
/// `File_Load(path, 0)` at `+0`, and call `AudioTGroups_Bind(this)`; always build the spatial grid via
/// `AudioTGroups_BuildGrid(this)` and return 1.
pub fn tgroups_load(m: &mut PfMem, env: &mut dyn WorldTriggerEnv, this: u32, path: u32) -> u32 {
    if env.call(m, FILESYS_EXISTS, &[path]) != 0 {
        let sz = env.call(m, FILE_GET_SIZE, &[path]);
        m.w32(this + 4, sz);
        let data = env.call(m, FILE_LOAD, &[path, 0]);
        m.w32(this, data);
        env.call(m, AUDIO_TGROUPS_BIND, &[this]);
    }
    tgroups_build_grid(m, env, this);
    1
}

/// `AudioTGroups_BuildGrid` 0x22a6d8(this): allocate and populate the 2D trigger grid from `gWorldGrid` (`0x346488`),
/// insert every trigger group via `AudioTGroups_AddToGrid` `0x22b008` into a temporary node pool, then compact into a
/// tightly sized `"GRID NODES"` allocation, rebasing node `next` and cell head pointers and freeing the temp pool.
pub fn tgroups_build_grid(m: &mut PfMem, env: &mut dyn WorldTriggerEnv, this: u32) -> u32 {
    let g = m.r32(G_WORLD_GRID);
    let nx = if g != 0 { m.r32(g + 0x2c) } else { 0 };
    m.w32(this + 0x10, nx);
    let ny = if g != 0 { m.r32(g + 0x30) } else { 0 };
    m.w32(this + 0x14, ny);
    let ncells = nx.wrapping_mul(ny);
    let bytes = ncells.wrapping_mul(0x14);
    let cells = env.call(m, EAMEM_ALLOC, &[NAME_AUDIO_TGROUPS, bytes, 0]);
    if cells != 0 {
        eamem::set(m, cells, 0xdead_c0ed, bytes);
    }
    let g = m.r32(G_WORLD_GRID);
    m.w32(this + 0x18, cells);
    let ox = m.r32(g + 4);
    let oy = m.r32(g + 8);
    m.w32(this + 0x1c, ox);
    m.w32(this + 0x20, oy);
    let g = m.r32(G_WORLD_GRID);
    let cell_sz = m.r32(g + 0x28);
    m.w32(this + 0x24, cell_sz);

    let mut ix: i32 = 0;
    let mut nx_cur = m.r32(this + 0x10) as i32;
    if nx_cur > 0 {
        let mut ny_cur = m.r32(this + 0x14) as i32;
        while ix < nx_cur {
            if ny_cur > 0 {
                let mut iy: i32 = 0;
                let fx0 = F(ix as f32);
                let ix1 = ix.wrapping_add(1);
                let mut cells_cur = m.r32(this + 0x18);
                while iy < ny_cur {
                    let cs = F(f32::from_bits(m.r32(this + 0x24)));
                    let fy0 = F(iy as f32);
                    let org_x = F(f32::from_bits(m.r32(this + 0x1c)));
                    let org_y = F(f32::from_bits(m.r32(this + 0x20)));
                    let idx = (iy as u32).wrapping_mul(nx_cur as u32).wrapping_add(ix as u32);
                    let off = idx.wrapping_mul(0x14);
                    let min_x = org_x + cs * fy0;
                    let min_y = org_y + cs * fx0;
                    let cell_a = cells_cur.wrapping_add(off);
                    m.w32(cell_a, min_x.0.to_bits());
                    m.w32(cell_a + 4, min_y.0.to_bits());

                    let iy1 = iy.wrapping_add(1);
                    let fx1 = F(ix1 as f32);
                    let fy1 = F(iy1 as f32);
                    let cells_b = m.r32(this + 0x18);
                    let cs2 = F(f32::from_bits(m.r32(this + 0x24)));
                    let org_x2 = F(f32::from_bits(m.r32(this + 0x1c)));
                    let org_y2 = F(f32::from_bits(m.r32(this + 0x20)));
                    let max_x = org_x2 + cs2 * fy1;
                    let max_y = org_y2 + cs2 * fx1;
                    let cell_b = cells_b.wrapping_add(off);
                    m.w32(cell_b + 8, max_x.0.to_bits());
                    m.w32(cell_b + 0xc, max_y.0.to_bits());

                    cells_cur = m.r32(this + 0x18);
                    ny_cur = m.r32(this + 0x14) as i32;
                    m.w32(cells_cur.wrapping_add(off) + 0x10, 0);
                    iy = iy1;
                    if iy < ny_cur {
                        nx_cur = m.r32(this + 0x10) as i32;
                    }
                }
                nx_cur = m.r32(this + 0x10) as i32;
                ix = ix1;
            } else {
                ix = ix.wrapping_add(1);
            }
        }
    }

    let count = m.r32(this + 8);
    if count != 0 {
        m.w32(this + 0x28, count << 2);
        let tmp_bytes = count << 5;
        let tmp = env.call(m, EAMEM_ALLOC, &[NAME_TEMP_GRID_NODES, tmp_bytes, 0x10]);
        if tmp != 0 {
            eamem::set(m, tmp, 0xdead_c0ed, tmp_bytes);
        }
        m.w32(this + 0x2c, tmp);
        m.w32(this + 0x30, 0);
        let mut i: i32 = 0;
        while i < m.r32(this + 8) as i32 {
            let arr = m.r32(this + 0xc);
            env.call(m, AUDIO_TGROUPS_ADD_TO_GRID, &[this, arr.wrapping_add((i as u32) << 3)]);
            i += 1;
        }
        let used = m.r32(this + 0x30);
        let final_bytes = used << 3;
        let nodes = env.call(m, EAMEM_ALLOC, &[NAME_GRID_NODES, final_bytes, 0]);
        if nodes != 0 {
            eamem::set(m, nodes, 0xdead_c0ed, final_bytes);
        }
        m.w32(this + 0x2c, nodes);
        let used = m.r32(this + 0x30);
        eamem::copy(m, nodes, tmp, used << 3);

        let used_i = m.r32(this + 0x30) as i32;
        let nx_reloc = m.r32(this + 0x10) as i32;
        for k in 0..used_i {
            let base = m.r32(this + 0x2c);
            let node = base.wrapping_add((k as u32) << 3);
            let next = m.r32(node + 4);
            if next != 0 {
                m.w32(node + 4, base.wrapping_add(next.wrapping_sub(tmp)));
            }
        }

        if nx_reloc > 0 {
            let ny_reloc = m.r32(this + 0x14) as i32;
            for ix in 0..nx_reloc {
                if ny_reloc > 0 {
                    let cells_base = m.r32(this + 0x18);
                    for iy in 0..ny_reloc {
                        let idx = (iy as u32).wrapping_mul(nx_reloc as u32).wrapping_add(ix as u32);
                        let cell = cells_base.wrapping_add(idx.wrapping_mul(0x14));
                        let head = m.r32(cell + 0x10);
                        if head != 0 {
                            m.w32(cell + 0x10, m.r32(this + 0x2c).wrapping_add(head.wrapping_sub(tmp)));
                        }
                    }
                }
            }
        }

        env.call(m, MEM_DELETE, &[tmp]);
    }
    1
}

/// `AudioTGroup_GetShapeBounds` 0x22acc0(_this, shape, out_min, out_max): compute the AABB of trigger shape `shape`.
/// Kinds 0, 2, 3 use `center ± radius` (`+8/+0xc/+0x10`, `+0x14`). Kind 1 (box) builds a right-handed basis
/// `(u, normalize((0,0,1) × u), u × v)` and offsets `center ∓ (u*hx + v*hy + w*hz)` (preserving the original's
/// un-absoluted sum). Other kinds leave `out_min` / `out_max` untouched.
pub fn tgroup_get_shape_bounds(m: &mut PfMem, shape: u32, out_min: u32, out_max: u32) {
    let kind = m.r32(shape);
    match kind {
        0 | 2 | 3 => {
            let r = F(f32::from_bits(m.r32(shape + 0x14)));
            let cx = F(f32::from_bits(m.r32(shape + 8)));
            let cy = F(f32::from_bits(m.r32(shape + 0xc)));
            let cz = F(f32::from_bits(m.r32(shape + 0x10)));
            m.w32(out_min, (cx - r).0.to_bits());
            m.w32(out_min + 4, (cy - r).0.to_bits());
            m.w32(out_min + 8, (cz - r).0.to_bits());
            m.w32(out_max, (cx + r).0.to_bits());
            m.w32(out_max + 4, (cy + r).0.to_bits());
            m.w32(out_max + 8, (cz + r).0.to_bits());
        }
        1 => {
            let ux = F(f32::from_bits(m.r32(shape + 0x20)));
            let uy = F(f32::from_bits(m.r32(shape + 0x24)));
            let uz = F(f32::from_bits(m.r32(shape + 0x28)));
            let z = F(0.0);
            let one = F(1.0);
            let rx = z * uz - one * uy;
            let ry = one * ux - z * uz;
            let rz = z * uy - z * ux;
            let dot = (rx * rx + ry * ry) + rz * rz;
            let inv_len = one / dot.sqrt();

            let cx = F(f32::from_bits(m.r32(shape + 8)));
            let cy = F(f32::from_bits(m.r32(shape + 0xc)));
            let cz = F(f32::from_bits(m.r32(shape + 0x10)));
            let vx = rx * inv_len;
            let vy = ry * inv_len;
            let vz = rz * inv_len;
            let wx = uy * vz - uz * vy;
            let wy = uz * vx - ux * vz;
            let wz = ux * vy - uy * vx;

            let hx = F(f32::from_bits(m.r32(shape + 0x14)));
            let hy = F(f32::from_bits(m.r32(shape + 0x18)));
            let hz = F(f32::from_bits(m.r32(shape + 0x1c)));

            let dx = (ux * hx + vx * hy) + wx * hz;
            let dy = (uy * hx + vy * hy) + wy * hz;
            let dz = (uz * hx + vz * hy) + wz * hz;

            m.w32(out_min, (cx - dx).0.to_bits());
            m.w32(out_min + 4, (cy - dy).0.to_bits());
            m.w32(out_min + 8, (cz - dz).0.to_bits());
            m.w32(out_max, (cx + dx).0.to_bits());
            m.w32(out_max + 4, (cy + dy).0.to_bits());
            m.w32(out_max + 8, (cz + dz).0.to_bits());
        }
        _ => {}
    }
}

/// `WorldInstance_GetSoundKeys` 0x22baf8(_this, inst, out0, out1, out2, out3): walk up to 4 trigger shapes at
/// `inst->+0xf4->+4` (count at `+4`, stream at `+8`, strides 0x1c / 0x30 / 0x30 / 0x18 for kinds 0..=3), zero all
/// four out words, and copy each shape's `+4` key word when the first shape's kind is in `0..=3` (reproducing the
/// original loop's un-incremented `$t3` check).
pub fn world_instance_get_sound_keys(
    m: &mut PfMem,
    inst: u32,
    out0: u32,
    out1: u32,
    out2: u32,
    out3: u32,
) {
    let p1 = m.r32(inst + 0xf4);
    let p2 = m.r32(p1 + 4);
    let n = m.r32(p2 + 4) as i32;
    let mut ptrs = [0u32; 4];
    let mut cur = p2.wrapping_add(8);
    for slot in ptrs.iter_mut().take(n.clamp(0, 4) as usize) {
        *slot = cur;
        let kind = m.r32(cur);
        cur = cur.wrapping_add(match kind {
            0 => 0x1c,
            1 | 2 => 0x30,
            3 => 0x18,
            _ => 0,
        });
    }
    m.w32(out0, 0);
    m.w32(out1, 0);
    m.w32(out2, 0);
    m.w32(out3, 0);
    if n > 0 {
        let first_kind = m.r32(ptrs[0]);
        let valid = matches!(first_kind, 0..=3);
        let mut key = 0u32;
        for (i, &ptr) in ptrs.iter().enumerate().take(n.min(4) as usize) {
            if valid {
                key = m.r32(ptr + 4);
            }
            match i {
                0 => m.w32(out0, key),
                1 => m.w32(out1, key),
                2 => m.w32(out2, key),
                3 => m.w32(out3, key),
                _ => {}
            }
        }
    }
}

/// `WorldTriggerMgr_Destruct` 0x22d168(this, flags): call `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn mgr_destruct(m: &mut PfMem, env: &mut dyn WorldTriggerEnv, this: u32, flags: u32) {
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `WorldTriggerMgr_Destroy` 0x22be88(): destroy `gWorldTriggerMgr` (`0x34370c`) with flags `3` if non-null and clear
/// the global pointer.
pub fn mgr_destroy(m: &mut PfMem, env: &mut dyn WorldTriggerEnv) {
    let p = m.r32(G_WORLD_TRIGGER_MGR);
    if p != 0 {
        mgr_destruct(m, env, p, 3);
    }
    m.w32(G_WORLD_TRIGGER_MGR, 0);
}

/// `WorldEmitter_Construct` 0x22bec0(this): initialize a 0x50-byte emitter record (zero state/keys/pos, `w = 1.0`,
/// handle/bank `-1`) and return `this`.
pub fn world_emitter_construct(m: &mut PfMem, this: u32) -> u32 {
    m.w32(this + 0x40, 0xffff_ffff);
    m.w32(this + 0x2c, 0x3f80_0000);
    m.w32(this + 0x00, 0);
    m.w32(this + 0x04, 0);
    m.w32(this + 0x08, 0);
    m.w32(this + 0x0c, 0);
    m.w32(this + 0x10, 0);
    m.w32(this + 0x14, 0);
    m.w32(this + 0x20, 0);
    m.w32(this + 0x24, 0);
    m.w32(this + 0x28, 0);
    m.w32(this + 0x30, 0xffff_ffff);
    m.w32(this + 0x34, 0);
    m.w32(this + 0x38, 0);
    this
}

/// `WorldTriggerMgr_Init` 0x22d098(this): initialize flags/caches at `+0x442c..=+0x4444`, allocate the 40-emitter
/// array (`0xc90` bytes with count cookie `40` at `+0`, records at `+0x10`), construct all 40 emitters, store the
/// base at `+0`, and return `this`.
pub fn mgr_init(m: &mut PfMem, env: &mut dyn WorldTriggerEnv, this: u32) -> u32 {
    m.w32(this + 0x442c, 0);
    m.w32(this + 0x4438, 0);
    m.w32(this + 0x4434, 0xffff_ffff);
    m.w32(this + 0x4444, 1);
    m.w32(this + 0x4430, 0xffff_ffff);
    m.w32(this + 0x443c, 1);
    m.w32(this + 0x4440, 1);
    let raw = env.call(m, EAMEM_ALLOC, &[NAME_WORLD_TRIGGER_INSTANCES, 0xc90, 0]);
    if raw != 0 {
        eamem::set(m, raw, 0xdead_c0ed, 0xc90);
    }
    m.w32(raw, COUNT as u32);
    let base = raw.wrapping_add(0x10);
    for i in 0..COUNT as u32 {
        world_emitter_construct(m, base.wrapping_add(i * 0x50));
    }
    m.w32(this, base);
    this
}

fn find_emitter_mem(m: &PfMem, this: u32, k1: u32, k2: u32, k3: u32) -> u32 {
    let base = m.r32(this);
    for i in 0..COUNT as u32 {
        let rec = base.wrapping_add(i * 0x50);
        if m.r32(rec) != 0 && m.r32(rec + 4) == k1 && m.r32(rec + 8) == k2 && m.r32(rec + 0x3c) == k3 {
            return rec;
        }
    }
    0
}

fn find_free_emitter_mem(m: &PfMem, this: u32) -> u32 {
    let base = m.r32(this);
    for i in 0..COUNT as u32 {
        let rec = base.wrapping_add(i * 0x50);
        if m.r32(rec) == 0 {
            return rec;
        }
    }
    0
}

/// `WorldEmitter_Acquire` 0x22d260(this, k1, k2, k3, sound_key, is_3d, pos_ptr, vol): when enabled (`+0x443c != 0`),
/// update the volume and `still_wanted` flag of an existing matching emitter, or claim the first free emitter slot
/// and populate its keys, sound key, volume, 3D flag, position `(x, y, z, 1.0)`, and `still_wanted = 1`.
#[allow(clippy::too_many_arguments)]
pub fn world_emitter_acquire(
    m: &mut PfMem,
    this: u32,
    k1: u32,
    k2: u32,
    k3: u32,
    sound_key: u32,
    is_3d: u32,
    pos_ptr: u32,
    vol: f32,
) {
    let px = m.r32(pos_ptr);
    let py = m.r32(pos_ptr + 4);
    let pz = m.r32(pos_ptr + 8);
    if m.r32(this + 0x443c) == 0 {
        return;
    }
    let existing = find_emitter_mem(m, this, k1, k2, k3);
    if existing != 0 {
        m.w32(existing + 0x10, vol.to_bits());
        m.w32(existing + 0x34, 1);
        return;
    }
    let slot = find_free_emitter_mem(m, this);
    m.w32(slot + 0x04, k1);
    m.w32(slot + 0x00, 1);
    m.w32(slot + 0x08, k2);
    m.w32(slot + 0x3c, k3);
    m.w32(slot + 0x0c, sound_key);
    m.w32(slot + 0x10, vol.to_bits());
    m.w32(slot + 0x14, is_3d);
    m.w32(slot + 0x20, px);
    m.w32(slot + 0x24, py);
    m.w32(slot + 0x28, pz);
    m.w32(slot + 0x2c, 0x3f80_0000);
    m.w32(slot + 0x34, 1);
}

/// `WorldTriggerMgr_StopKey` 0x22d480(this, sound_key): for every active (`state == 1`) emitter matching `sound_key`
/// at `+0xc`, clear its state to 0 and call `WorldEmitter_Stop` `0x22c5a0`.
pub fn mgr_stop_key(m: &mut PfMem, env: &mut dyn WorldTriggerEnv, this: u32, sound_key: u32) {
    let mut base = m.r32(this);
    for i in 0..COUNT as u32 {
        let rec = base.wrapping_add(i * 0x50);
        if m.r32(rec) == 1 && m.r32(rec + 0xc) == sound_key {
            m.w32(rec, 0);
            env.call(m, WORLD_EMITTER_STOP, &[rec]);
            base = m.r32(this);
        }
    }
}

fn was_played_mem(m: &mut PfMem, this: u32, id: u32) -> u32 {
    if id == m.r32(this + 0x4434) {
        return m.r32(this + 0x4438);
    }
    let count = m.r32(this + 0x442c) as i32;
    m.w32(this + 0x4434, id);
    let mut i = count - 1;
    while i >= 0 {
        if m.r32(this + 0x410c + (i as u32) * 4) == id {
            m.w32(this + 0x4438, 1);
            return 1;
        }
        i -= 1;
    }
    m.w32(this + 0x4438, 0);
    0
}

/// `WorldEmitter_MarkPlayed` 0x22e2d0(this, id): if `WorldEmitter_WasPlayed(this, id)` is 0, append `id` to the
/// 200-entry played list at `+0x410c` (count `+0x442c`) when `< 200`, and invalidate the cache key `+0x4434 = -1`.
pub fn world_emitter_mark_played(m: &mut PfMem, this: u32, id: u32) {
    if was_played_mem(m, this, id) != 0 {
        return;
    }
    let n = m.r32(this + 0x442c) as i32;
    if n < 0xc8 {
        m.w32(this + 0x410c + (n as u32) * 4, id);
        m.w32(this + 0x442c, (n + 1) as u32);
    }
    m.w32(this + 0x4434, 0xffff_ffff);
}

/// `WorldTriggerMgr_Reset` 0x22e360(this): reset played list count, query cache, and enable flags at `+0x442c..=+0x4444`.
pub fn mgr_reset(m: &mut PfMem, this: u32) {
    m.w32(this + 0x4434, 0xffff_ffff);
    m.w32(this + 0x4444, 1);
    m.w32(this + 0x442c, 0);
    m.w32(this + 0x4430, 0xffff_ffff);
    m.w32(this + 0x4438, 0);
    m.w32(this + 0x4440, 1);
}

/// `SoundId_IsAmbientExtra` 0x22e3f0(_this, id): returns 1 if `id == 0x66`, else 0.
pub fn sound_id_is_ambient_extra(id: u32) -> u32 {
    u32::from(id == 0x66)
}

/// `SoundId_IsAmbient` 0x22e388(_this, id): returns 1 if `id` falls in `[0x4f, 0x60]`, `[0x67, 0x93]`, `[0x9f, 0xb2]`,
/// `[0xb7, 0xb9]`, or `SoundId_IsAmbientExtra(id)` (`0x66`), else 0.
pub fn sound_id_is_ambient(id: u32) -> u32 {
    if id.wrapping_sub(0x4f) < 0x12
        || id.wrapping_sub(0x67) < 0x2d
        || id.wrapping_sub(0x9f) < 0x14
        || id.wrapping_sub(0xb7) < 3
        || sound_id_is_ambient_extra(id) != 0
    {
        1
    } else {
        0
    }
}
