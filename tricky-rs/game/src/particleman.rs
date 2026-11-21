//! Trail particle manager (board row E1b2g3a): `cParticleMan` (`ParticleMan_Init` 0x1d8e30,
//! `cParticleMan::~cParticleMan` 0x1d8f50, `PoolList_Pop` 0x1d8fd8, `PoolList_Push` 0x1d9018,
//! `ParticleGroup_Add` 0x1d9060, `ParticleGroup_Get` 0x1d9108, `ParticleGroup_Remove` 0x1d9138,
//! `ParticleGroup_UpdateTrail` 0x1d9208, `ParticleMan_DrawSlot` 0x1d9700, `cParticleMan::vf2` 0x1d9900).
//!
//! Twelve trail slots (`0xf0` bytes each from `+0x10`) sharing a pool of 250 (`0xfa`) particles of `0x80` bytes
//! (32,000 bytes allocated at `+0xb50`, linked on a doubly-linked free list rooted at `+0xb58` with free count
//! `+0xb54` and used count `+0xb5c`). Used by `cBoarderRender::vf7` 0x1356f8 for the rider's hand/boot trails.
//! On the bytes at their addresses ([`Mem`]); checked word for word in the function runner (`tests/particleman.rs`).

use crate::animseq::{cvt_s_w, div};
use crate::pf::Mem;
use crate::ps2f::F;
use crate::riderdraw::{RiderDrawEnv, APP};
use crate::sndlib::cvt_w;

pub const VTABLE: u32 = 0x0039_2d30;
pub const ALLOC_TAG: u32 = 0x0039_2d18;
pub const POOL_BYTES: u32 = 32_000;
pub const POOL_COUNT: u32 = 250;
pub const NODE_BYTES: u32 = 0x80;
pub const SLOT_COUNT: u32 = 12;
pub const SLOT_BYTES: u32 = 0xf0;
pub const CFG_BYTES: u32 = 0xe0;

pub const BX_OBJ_NODE_CTOR: u32 = 0x0017_cf50;
pub const BX_OBJ_NODE_DTOR: u32 = 0x0017_cfe0;
pub const EA_MEM_ALLOC: u32 = 0x002c_cf70;
pub const EA_MEM_SET: u32 = 0x002c_c808;
pub const EA_MEM_COPY: u32 = 0x002c_e114;
pub const MEM_FREE: u32 = 0x0014_a040;
pub const VEC4_SCALE: u32 = 0x0010_2f50;
pub const RAND_FX: u32 = 0x0023_d4a8;
pub const DRAW_SLOT: u32 = 0x001d_9700;

const BIAS: u32 = 0x3da3_d70a;
const VEL_BLEND: u32 = 0x3e18_e3b1;

type V = [F; 4];

fn rf(m: &Mem, a: u32) -> F {
    F(f32::from_bits(m.r32(a)))
}
fn wf(m: &mut Mem, a: u32, v: F) {
    m.w32(a, v.0.to_bits());
}
fn rq(m: &Mem, a: u32) -> V {
    [0, 1, 2, 3].map(|k| rf(m, a.wrapping_add(4 * k)))
}
fn wq(m: &mut Mem, a: u32, v: V) {
    for k in 0..4 {
        wf(m, a.wrapping_add(4 * k), v[k as usize]);
    }
}
fn copy16(m: &mut Mem, dst: u32, src: u32) {
    for k in 0..4u32 {
        let w = m.r32(src.wrapping_add(4 * k));
        m.w32(dst.wrapping_add(4 * k), w);
    }
}
fn vadd(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}
fn vsub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}
fn vmulx(a: V, s: F) -> V {
    [a[0] * s, a[1] * s, a[2] * s, a[3] * s]
}
fn f(bits: u32) -> F {
    F(f32::from_bits(bits))
}
fn rand_signed(r: u32) -> F {
    let u = F(f32::from_bits((r & 0x7f_ffff) | 0x3f80_0000)) - F(1.0);
    (u + u) + F(-1.0)
}
fn slot_addr(this: u32, slot: i32) -> u32 {
    this.wrapping_add(slot.wrapping_mul(SLOT_BYTES as i32) as u32).wrapping_add(0x10)
}
fn renderer(m: &Mem) -> u32 {
    m.r32(m.r32(APP).wrapping_add(0x724))
}
fn vcall(m: &mut Mem, env: &mut dyn RiderDrawEnv, r: u32, off: u32, args: &[u32]) -> u32 {
    let vt = m.r32(r.wrapping_add(0x10));
    let this = r.wrapping_add(m.r16(vt.wrapping_add(off)) as i16 as i32 as u32);
    let func = m.r32(vt.wrapping_add(off).wrapping_add(4));
    env.vcall(m, func, this, args)
}

/// Calls out of `ParticleMan_Init` / `~cParticleMan` / `ParticleGroup_Add`.
pub trait ParticleMemEnv {
    /// `cBxObjNode::cBxObjNode` 0x17cf50(this)
    fn bx_ctor(&mut self, m: &mut Mem, this: u32);
    /// `cBxObjNode::~cBxObjNode` 0x17cfe0(this, flags)
    fn bx_dtor(&mut self, m: &mut Mem, this: u32, flags: u32);
    /// `EAMem_Alloc` 0x2ccf70(tag, bytes, align)
    fn alloc(&mut self, m: &mut Mem, tag: u32, bytes: u32, align: u32) -> u32;
    /// `EAMem_Set` 0x2cc808(ptr, pattern, bytes)
    fn mem_set(&mut self, m: &mut Mem, ptr: u32, pat: u32, bytes: u32);
    /// `Mem_Free` 0x14a040(ptr)
    fn free(&mut self, m: &mut Mem, ptr: u32);
}

/// `ParticleMan_Init` 0x1d8e30(this): constructs the `cBxObjNode` base, sets the `cParticleMan` vtable,
/// clears the 12 slots, allocates the 250-particle pool (32,000 bytes, filled with `0xdeadc0ed`) and links all
/// 250 nodes of `0x80` bytes on the free list (`+0xb50` base, `+0xb54` free count 250, `+0xb58` head, `+0xb5c` used 0).
pub fn init(m: &mut Mem, env: &mut dyn ParticleMemEnv, this: u32) -> u32 {
    env.bx_ctor(m, this);
    m.w32(this.wrapping_add(8), VTABLE);
    m.w32(this.wrapping_add(0xc), 0);
    for i in (0..SLOT_COUNT as i32).rev() {
        m.w32(slot_addr(this, i), 0);
    }
    let pool = env.alloc(m, ALLOC_TAG, POOL_BYTES, 0);
    if pool != 0 {
        env.mem_set(m, pool, 0xdead_c0ed, POOL_BYTES);
    }
    m.w32(this.wrapping_add(0xb50), pool);
    m.w32(this.wrapping_add(0xb58), pool);
    m.w32(this.wrapping_add(0xb54), POOL_COUNT);
    m.w32(this.wrapping_add(0xb5c), 0);
    m.w32(pool, 0);
    for i in 0..POOL_COUNT - 1 {
        let cur = pool.wrapping_add(i * NODE_BYTES);
        let next = cur.wrapping_add(NODE_BYTES);
        m.w32(cur.wrapping_add(4), next);
        m.w32(next, cur);
    }
    m.w32(pool.wrapping_add((POOL_COUNT - 1) * NODE_BYTES + 4), 0);
    this
}

/// `cParticleMan::~cParticleMan` 0x1d8f50(this, flags): clears active count and the 12 slots, frees the pool
/// if non-null, zeroes `+0xb50`, and calls `cBxObjNode::~cBxObjNode`.
pub fn destroy(m: &mut Mem, env: &mut dyn ParticleMemEnv, this: u32, flags: u32) {
    m.w32(this.wrapping_add(8), VTABLE);
    let pool = m.r32(this.wrapping_add(0xb50));
    m.w32(this.wrapping_add(0xc), 0);
    for i in (0..SLOT_COUNT as i32).rev() {
        m.w32(slot_addr(this, i), 0);
    }
    if pool != 0 {
        env.free(m, pool);
    }
    m.w32(this.wrapping_add(0xb50), 0);
    env.bx_dtor(m, this, flags);
}

/// `PoolList_Pop` 0x1d8fd8(this): pops the head node off the free list (`+0xb58`), updates `+0xb54` (free) and
/// `+0xb5c` (used), returning 0 when the free count is 0.
pub fn pool_pop(m: &mut Mem, this: u32) -> u32 {
    let free = m.r32(this.wrapping_add(0xb54));
    if free == 0 {
        return 0;
    }
    let head = m.r32(this.wrapping_add(0xb58));
    let next = m.r32(head.wrapping_add(4));
    m.w32(this.wrapping_add(0xb58), next);
    if next != 0 {
        m.w32(next, 0);
    }
    m.w32(this.wrapping_add(0xb54), free.wrapping_sub(1));
    m.w32(this.wrapping_add(0xb5c), m.r32(this.wrapping_add(0xb5c)).wrapping_add(1));
    head
}

/// `PoolList_Push` 0x1d9018(this, node): pushes `node` onto the head of the free list (`+0xb58`), updates
/// `+0xb54` (free) and `+0xb5c` (used), and returns 1.
pub fn pool_push(m: &mut Mem, this: u32, node: u32) -> u32 {
    let head = m.r32(this.wrapping_add(0xb58));
    if head == 0 {
        m.w32(node.wrapping_add(4), 0);
    } else {
        m.w32(node.wrapping_add(4), head);
        m.w32(head, node);
    }
    let free = m.r32(this.wrapping_add(0xb54));
    let used = m.r32(this.wrapping_add(0xb5c));
    m.w32(node, 0);
    m.w32(this.wrapping_add(0xb58), node);
    m.w32(this.wrapping_add(0xb5c), used.wrapping_sub(1));
    m.w32(this.wrapping_add(0xb54), free.wrapping_add(1));
    1
}

/// `ParticleGroup_Add` 0x1d9060(this, cfg): claims the first free slot (0..12) when active count `< 12`, sets
/// its `in_use = 1`, `head = 0`, `count = 0`, copies `0xe0` bytes from `cfg` into `slot + 0x10` (`EAMem_Copy`),
/// and returns the slot index (or `-1` if 12 slots are already active).
pub fn group_add(m: &mut Mem, this: u32, cfg: u32) -> i32 {
    let active = m.r32(this.wrapping_add(0xc)) as i32;
    if active >= SLOT_COUNT as i32 {
        return -1;
    }
    m.w32(this.wrapping_add(0xc), active.wrapping_add(1) as u32);
    let mut found = -1i32;
    let mut idx = 0i32;
    if m.r32(slot_addr(this, 0)) == 0 {
        found = 0;
    } else {
        idx = 1;
        while idx < SLOT_COUNT as i32 {
            if m.r32(slot_addr(this, idx)) == 0 {
                found = idx;
                break;
            }
            idx += 1;
        }
    }
    let s = slot_addr(this, idx);
    m.w32(s, 1);
    m.w32(s.wrapping_add(8), 0);
    m.w32(s.wrapping_add(4), 0);
    for off in 0..CFG_BYTES {
        let b = m.r8(cfg.wrapping_add(off));
        m.w8(s.wrapping_add(0x10).wrapping_add(off), b);
    }
    found
}

/// `ParticleGroup_Get` 0x1d9108(this, slot): returns `slot + 0x10` (the `0xe0`-byte config) when `(slot as u32)
/// < 13` (original's `0xd` bound) and `slot.in_use != 0`, else 0.
pub fn group_get(m: &Mem, this: u32, slot: u32) -> u32 {
    if slot < 0xd {
        let s = slot_addr(this, slot as i32);
        if m.r32(s) != 0 {
            return s.wrapping_add(0x10);
        }
    }
    0
}

/// `ParticleGroup_Remove` 0x1d9138(this, slot): returns 0 if active count is 0; else if the slot is in use,
/// returns all its particles to the pool (`PoolList_Push`), clears `head`, `count`, and `in_use`, decrements
/// active count, and returns 1.
pub fn group_remove(m: &mut Mem, this: u32, slot: i32) -> u32 {
    let active = m.r32(this.wrapping_add(0xc));
    if active == 0 {
        return 0;
    }
    let s = slot_addr(this, slot);
    if m.r32(s) != 0 {
        if m.r32(s.wrapping_add(8)) != 0 {
            let mut node = m.r32(s.wrapping_add(4));
            while node != 0 {
                let next = m.r32(node.wrapping_add(4));
                pool_push(m, this, node);
                m.w32(s.wrapping_add(8), m.r32(s.wrapping_add(8)).wrapping_sub(1));
                node = next;
            }
            m.w32(s.wrapping_add(8), 0);
            m.w32(s.wrapping_add(4), 0);
        }
        let active = m.r32(this.wrapping_add(0xc));
        m.w32(this.wrapping_add(0xc), active.wrapping_sub(1));
        m.w32(s, 0);
    }
    1
}

/// `ParticleGroup_UpdateTrail` 0x1d9208(this, slot, pos, vel): updates the slot's emitter position/velocity,
/// retires dead particles (`life == 0`) back to the pool and steps living ones (colour, size, velocity × drag,
/// pos += vel + gravity, life -= 1), then draws once from `Rand_Fx` for the spawn count (updating the fractional
/// accumulator at `slot+0x18`) and five times per spawned particle (lifetime, start/end size, start/end colour)
/// before popping from the pool and prepending to the slot's list.
pub fn update_trail(m: &mut Mem, this: u32, slot: i32, pos_ptr: u32, vel_ptr: u32, rand_fx: &mut dyn FnMut() -> u32) -> u32 {
    if m.r32(this.wrapping_add(0xc)) == 0 {
        return 0;
    }
    let s = slot_addr(this, slot);
    if m.r32(s) == 0 {
        return 0;
    }
    // emitter position and velocity
    copy16(m, s.wrapping_add(0xc0), s.wrapping_add(0xb0));
    copy16(m, s.wrapping_add(0xb0), pos_ptr);
    wf(m, s.wrapping_add(0xbc), F(1.0));
    let pos_in = rq(m, pos_ptr);
    let prev = rq(m, s.wrapping_add(0xc0));
    let vel_in = rq(m, vel_ptr);
    let d0 = vsub(pos_in, prev);
    let d1 = vsub(d0, vel_in);
    let scaled = vmulx(d1, f(VEL_BLEND));
    let em_vel = vadd(vel_in, scaled);
    wq(m, s.wrapping_add(0xd0), em_vel);
    wf(m, s.wrapping_add(0xdc), F(1.0));

    // step existing particles and retire dead ones
    let mut node = m.r32(s.wrapping_add(4));
    while node != 0 {
        let life = m.r32(node.wrapping_add(0x40));
        if life == 0 {
            let prev_p = m.r32(node);
            let next_p = m.r32(node.wrapping_add(4));
            if prev_p == 0 {
                if next_p == 0 {
                    m.w32(s.wrapping_add(4), 0);
                } else {
                    m.w32(next_p, 0);
                    m.w32(s.wrapping_add(4), next_p);
                }
            } else if next_p == 0 {
                m.w32(prev_p.wrapping_add(4), 0);
            } else {
                m.w32(prev_p.wrapping_add(4), next_p);
                m.w32(next_p, prev_p);
            }
            pool_push(m, this, node);
            m.w32(s.wrapping_add(8), m.r32(s.wrapping_add(8)).wrapping_sub(1));
            node = next_p;
        } else {
            let col = vadd(rq(m, node.wrapping_add(0x50)), rq(m, node.wrapping_add(0x70)));
            wq(m, node.wrapping_add(0x50), col);
            copy16(m, node.wrapping_add(0x20), node.wrapping_add(0x10));
            let sz = rf(m, node.wrapping_add(0x44)) + rf(m, node.wrapping_add(0x48));
            wf(m, node.wrapping_add(0x44), sz);
            let drag = rf(m, s.wrapping_add(0x80));
            let v = vmulx(rq(m, node.wrapping_add(0x30)), drag);
            wq(m, node.wrapping_add(0x30), v);
            let step = vadd(v, rq(m, s.wrapping_add(0x90)));
            let p = vadd(rq(m, node.wrapping_add(0x10)), step);
            wq(m, node.wrapping_add(0x10), p);
            wf(m, node.wrapping_add(0x1c), F(1.0));
            m.w32(node.wrapping_add(0x40), life.wrapping_sub(1));
            node = m.r32(node.wrapping_add(4));
        }
    }

    // spawn count (one Rand_Fx draw)
    let r0 = rand_signed(rand_fx());
    let f1 = (rf(m, s.wrapping_add(0x10)) + rf(m, s.wrapping_add(0x14)) * r0) + rf(m, s.wrapping_add(0x18));
    let n = cvt_w(f1.0);
    wf(m, s.wrapping_add(0x18), f1 - cvt_s_w(n));
    if n <= 0 {
        return 1;
    }

    // spawn n particles (five Rand_Fx draws each, before PoolList_Pop)
    for _ in 0..n {
        let r_life = rand_signed(rand_fx());
        let life_base = m.r32(s.wrapping_add(0x1c)) as i32;
        let life_var = cvt_s_w(m.r32(s.wrapping_add(0x20)) as i32);
        let life = life_base.wrapping_add(cvt_w((life_var * r_life).0));

        let r_sz0 = rand_signed(rand_fx());
        let sz0 = rf(m, s.wrapping_add(0x70)) + rf(m, s.wrapping_add(0x74)) * r_sz0;

        let r_sz1 = rand_signed(rand_fx());
        let sz1 = rf(m, s.wrapping_add(0x78)) + rf(m, s.wrapping_add(0x7c)) * r_sz1;

        let r_c0 = rand_signed(rand_fx());
        let c0 = vadd(rq(m, s.wrapping_add(0x30)), vmulx(rq(m, s.wrapping_add(0x40)), r_c0));

        let r_c1 = rand_signed(rand_fx());
        let c1 = vadd(rq(m, s.wrapping_add(0x50)), vmulx(rq(m, s.wrapping_add(0x60)), r_c1));

        let pnode = pool_pop(m, this);
        if pnode == 0 {
            return 1;
        }
        m.w32(pnode.wrapping_add(0x40), life as u32);
        let inv = div(F(1.0), cvt_s_w(life));
        let dsz = (sz1 - sz0) * inv;
        wq(m, pnode.wrapping_add(0x50), c0);
        wq(m, pnode.wrapping_add(0x60), c0);
        let dc = vmulx(vsub(c1, c0), inv);
        wq(m, pnode.wrapping_add(0x70), dc);
        copy16(m, pnode.wrapping_add(0x30), s.wrapping_add(0xd0));
        copy16(m, pnode.wrapping_add(0x10), s.wrapping_add(0xb0));
        copy16(m, pnode.wrapping_add(0x20), s.wrapping_add(0xb0));
        m.w32(pnode, 0);
        wf(m, pnode.wrapping_add(0x48), dsz);
        wf(m, pnode.wrapping_add(0x44), sz0);
        let head = m.r32(s.wrapping_add(4));
        m.w32(pnode.wrapping_add(4), head);
        if head != 0 {
            m.w32(head, pnode);
        }
        m.w32(s.wrapping_add(4), pnode);
        m.w32(s.wrapping_add(8), m.r32(s.wrapping_add(8)).wrapping_add(1));
    }
    1
}

/// `ParticleMan_DrawSlot` 0x1d9700(this, slot): draws the slot's emitter glow sprite (texture 0x1c from the
/// app's table, size `0x4196f9ba`, uv 0..0.5, alpha `glow_alpha * 1.6129689 + 0.37703115`, rgb
/// `0.9979741, 0.998004, 0.9905221`) then binds `slot+0xa0` and draws every particle in `slot.head` as a
/// sprite (size `node.size`, uv 0..1, colour `node+0x50`). Uses `scratch` (0x60 bytes) for the game's stack
/// temporaries.
pub fn draw_slot(m: &mut Mem, env: &mut dyn RiderDrawEnv, this: u32, slot: i32, scratch: u32) -> u32 {
    if m.r32(this.wrapping_add(0xc)) == 0 {
        return 0;
    }
    let s = slot_addr(this, slot);
    if m.r32(s) == 0 {
        return 0;
    }
    let mut node = m.r32(s.wrapping_add(4));
    let app = m.r32(APP);
    let r = m.r32(app.wrapping_add(0x724));
    let tex0 = m.r32(m.r32(app.wrapping_add(0x30)).wrapping_add(0x70));
    vcall(m, env, r, 0x1f0, &[tex0, 0]);

    let alpha = rf(m, s.wrapping_add(0xe0)) * f(0x3fce_75c4) + f(0x3ec1_0a3a);
    let sz_buf = scratch;
    let uv0_buf = scratch.wrapping_add(0x10);
    let uv1_buf = scratch.wrapping_add(0x20);
    let col_buf = scratch.wrapping_add(0x30);
    m.w32(sz_buf, 0x4196_f9ba);
    m.w32(sz_buf.wrapping_add(4), 0x4196_f9ba);
    m.w32(uv0_buf, 0);
    m.w32(uv0_buf.wrapping_add(4), 0);
    m.w32(uv1_buf, 0x3f00_0000);
    m.w32(uv1_buf.wrapping_add(4), 0x3f00_0000);
    wf(m, col_buf, alpha);
    m.w32(col_buf.wrapping_add(4), 0x3f7f_7b3b);
    m.w32(col_buf.wrapping_add(8), 0x3f7f_7d31);
    m.w32(col_buf.wrapping_add(0xc), 0x3f7d_92db);
    vcall(m, env, r, 0x240, &[s.wrapping_add(0xb0), sz_buf, uv0_buf, uv1_buf, col_buf]);

    let tex1 = m.r32(s.wrapping_add(0xa0));
    vcall(m, env, r, 0x1f0, &[tex1, 0]);

    let puv1_buf = scratch.wrapping_add(0x40);
    let pcol_buf = scratch.wrapping_add(0x50);
    while node != 0 {
        let sz = m.r32(node.wrapping_add(0x44));
        m.w32(sz_buf, sz);
        m.w32(sz_buf.wrapping_add(4), sz);
        m.w32(uv0_buf, 0);
        m.w32(uv0_buf.wrapping_add(4), 0);
        m.w32(puv1_buf, 0x3f80_0000);
        m.w32(puv1_buf.wrapping_add(4), 0x3f80_0000);
        copy16(m, pcol_buf, node.wrapping_add(0x50));
        vcall(m, env, r, 0x240, &[node.wrapping_add(0x10), sz_buf, uv0_buf, puv1_buf, pcol_buf]);
        node = m.r32(node.wrapping_add(4));
    }
    1
}

/// `cParticleMan::vf2` 0x1d9900(this): if any slot is active (`+0xc != 0`), sets render state (blend 3,
/// `+0x1e8(2, 0)`, `+0x1c8(0, 0, 0.08)`, `+0x1c0(1)`), calls `ParticleMan_DrawSlot` 0x1d9700 for every
/// in-use slot `0..12`, and restores render state.
pub fn draw(m: &mut Mem, env: &mut dyn RiderDrawEnv, this: u32) {
    if m.r32(this.wrapping_add(0xc)) == 0 {
        return;
    }
    let r = renderer(m);
    vcall(m, env, r, 0x1e0, &[3]);
    vcall(m, env, r, 0x1e8, &[2, 0]);
    vcall(m, env, r, 0x1c8, &[0, 0, BIAS]);
    vcall(m, env, r, 0x1c0, &[1]);
    for i in 0..SLOT_COUNT as i32 {
        if m.r32(slot_addr(this, i)) != 0 {
            env.call(m, DRAW_SLOT, &[this, i as u32]);
        }
    }
    vcall(m, env, r, 0x1e0, &[3]);
    vcall(m, env, r, 0x1c8, &[0, 2, BIAS]);
    vcall(m, env, r, 0x1c0, &[0]);
    vcall(m, env, r, 0x1e8, &[0, 0]);
}
