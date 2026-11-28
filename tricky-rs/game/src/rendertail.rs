//! Remaining `render` system functions on engine memory (`pf::Mem`), ported 1:1 from `SLUS_203.26` and
//! verified bit-for-bit in `r5900::Runner`:
//! - Row G7s (`tests/render_misc_tail.rs`): boarder-effects, tex-flip, model/mesh-anim, particle helpers,
//!   mesh position scaling, owned buffers, masked sphere-list draw, and sphere-tree cache / RLE encoder.

use crate::eamem;
use crate::pf::Mem;
use crate::ps2f::F;

type V4 = [F; 4];

// Global addresses in SLUS_203.26.
pub const G_APP: u32 = 0x0033_7c58;
pub const G_SPH_CACHE_A_PTRS: u32 = 0x0034_39a0;
pub const G_SPH_CACHE_A_IDS: u32 = 0x0034_39a4;
pub const G_SPH_CACHE_A_CAP: u32 = 0x0034_39a8;
pub const G_SPH_CACHE_A_CUR: u32 = 0x0034_39ac;
pub const G_SPH_CACHE_B_PTRS: u32 = 0x0034_39b0;
pub const G_SPH_CACHE_B_IDS: u32 = 0x0034_39b4;
pub const G_SPH_CACHE_B_CAP: u32 = 0x0034_39b8;
pub const G_SPH_CACHE_B_CUR: u32 = 0x0034_39bc;
pub const G_SPH_CACHE_DEPTH_B: u32 = 0x0034_39c0;
pub const G_SCRIPT_POOL: u32 = 0x0034_5e48;
pub const STR_MESH_ANIM_FRAMES: u32 = 0x0038_c338;
pub const STR_SPHERE_TREE_CACHE: u32 = 0x003a_3ff0;
pub const G_STREAK_COLOR_LOW: u32 = 0x003b_c230;
pub const G_STREAK_COLOR_MID: u32 = 0x003b_c240;
pub const G_STREAK_COLOR_HIGH: u32 = 0x003b_c250;

// Vtables in SLUS_203.26.
pub const VT_TEX_FLIP: u32 = 0x0036_d038;
pub const VT_MESH_ANIM_FRAME: u32 = 0x0039_1af8;
pub const VT_ANIM_FRAME: u32 = 0x0039_1b10;
pub const VT_SKEL_SEQUENCE: u32 = 0x0039_e470;
pub const VT_SPHERE_TREE_BUF_CACHE: u32 = 0x003a_40f8;

// Callee addresses in SLUS_203.26.
pub const VEC4_SCALE: u32 = 0x0010_2f50;
pub const MEM_DELETE: u32 = 0x0011_4f80;
pub const FX_BRAKE_SPRAY_RESET: u32 = 0x0012_dfd0;
pub const FX_SPAWN_LANDING_PUFFS: u32 = 0x0012_e0e8;
pub const FX_BOARD_PUFF_DESTRUCT: u32 = 0x0012_ec78;
pub const FX_BOARD_PUFF_RESET: u32 = 0x0012_ec98;
pub const FX_SPEED_DUST_DESTRUCT: u32 = 0x0012_f490;
pub const FX_SPEED_DUST_RESET: u32 = 0x0012_f4b0;
pub const FX_BOARD_TRAIL_RESET: u32 = 0x0012_fed8;
pub const FX_SURFACE_SPRAY_RESET: u32 = 0x0013_1160;
pub const FX_BOARD_TRACK_RESET: u32 = 0x0013_2518;
pub const BOARDER_RENDER_RESET_EFFECTS: u32 = 0x0013_5200;
pub const BOARDER_RENDER_GET_HAND_STREAKS: u32 = 0x0013_6c48;
pub const BOARDER_RENDER_COPY_MATRIX: u32 = 0x0013_6e38;
pub const MEM_FREE: u32 = 0x0014_a040;
pub const ANIM_FRAME_SET_KEY: u32 = 0x001c_b0d8;
pub const ANIM_FRAME_CLEAR: u32 = 0x001c_b150;
pub const ANIM_FRAME_SET_IDENTITY: u32 = 0x001c_b960;
pub const SKEL_SEQ_VF1: u32 = 0x0020_3cf8;
pub const MDF_MEM_BANK_INIT: u32 = 0x0020_3e98;
pub const MEMPOOL_ALLOC: u32 = 0x0023_d3b0;
pub const EAMEM_ALLOC: u32 = 0x002c_cf70;

const ONE_F32_BITS: u32 = 0x3f80_0000;

/// Calls out from render engine-memory functions. Any `$f12` argument is passed as its `u32` IEEE bits after the
/// GPR arguments.
pub trait RenderEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32;
}

fn call_gfx_vf(m: &mut Mem, env: &mut dyn RenderEnv, gfx: u32, off: u32, extra: &[u32]) -> u32 {
    let vt = m.r32(gfx + 0x10);
    let adj = m.r16(vt + off) as i16 as i32 as u32;
    let f = m.r32(vt + off + 4);
    let mut args = Vec::with_capacity(1 + extra.len());
    args.push(gfx.wrapping_add(adj));
    args.extend_from_slice(extra);
    env.call(m, f, &args)
}

fn fill_bbox_pair(m: &mut Mem, inst: u32, min_dst: u32, max_dst: u32) {
    for k in 0..3u32 {
        m.w32(min_dst + k * 4, m.r32(inst + 0xcc + k * 4));
    }
    m.w32(min_dst + 0xc, ONE_F32_BITS);
    for k in 0..3u32 {
        m.w32(max_dst + k * 4, m.r32(inst + 0xd8 + k * 4));
    }
    m.w32(max_dst + 0xc, ONE_F32_BITS);
}

fn read_v4(m: &Mem, at: u32) -> V4 {
    [
        F(f32::from_bits(m.r32(at))),
        F(f32::from_bits(m.r32(at + 4))),
        F(f32::from_bits(m.r32(at + 8))),
        F(f32::from_bits(m.r32(at + 12))),
    ]
}

fn write_v4(m: &mut Mem, at: u32, v: V4) {
    for (k, c) in v.iter().enumerate() {
        m.w32(at + (k as u32) * 4, c.0.to_bits());
    }
}

fn cross_v4(a: V4, b: V4) -> V4 {
    [
        a[1] * b[2] - b[1] * a[2],
        a[2] * b[0] - b[2] * a[0],
        a[0] * b[1] - b[0] * a[1],
        F(0.0),
    ]
}

fn dot4_v4(a: V4, b: V4) -> F {
    let p = [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]];
    ((p[0] + p[1]) + p[2]) + p[3]
}

// ---- Row G7s: boarder effects, tex-flip, model/mesh-anim, particles, mesh scaling, sphtree cache ----

/// `RiderSkin_InitMem` 0x123060(this, a1, a2): initialize the two embedded `MdfMemBank`s at `this+0` and
/// `this+0x10` via `MdfMemBank_Init`.
pub fn rider_skin_init_mem(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, a1: u32, a2: u32) {
    env.call(m, MDF_MEM_BANK_INIT, &[this, a1]);
    env.call(m, MDF_MEM_BANK_INIT, &[this.wrapping_add(0x10), a2]);
}

/// `BoarderRender_ResetEffects` 0x135200(this): reset all six embedded boarder effect systems (`+0`, `+0x1c20`,
/// `+0x4b60`, `+0x8020`, `+0x99b0`, `+0xb600`).
pub fn boarder_render_reset_effects(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) {
    env.call(m, FX_BOARD_PUFF_RESET, &[this]);
    env.call(m, FX_BOARD_TRAIL_RESET, &[this.wrapping_add(0x1c20)]);
    env.call(m, FX_SURFACE_SPRAY_RESET, &[this.wrapping_add(0x4b60)]);
    env.call(m, FX_BOARD_TRACK_RESET, &[this.wrapping_add(0x8020)]);
    env.call(m, FX_BRAKE_SPRAY_RESET, &[this.wrapping_add(0x99b0)]);
    env.call(m, FX_SPEED_DUST_RESET, &[this.wrapping_add(0xb600)]);
}

/// `BoarderRender_SpawnLandingPuffs` 0x135268(this, a1, a2, a3): tail-call `Fx_SpawnLandingPuffs(this+0x99b0, ...)`.
pub fn boarder_render_spawn_landing_puffs(
    m: &mut Mem,
    env: &mut dyn RenderEnv,
    this: u32,
    a1: u32,
    a2: u32,
    a3: u32,
) {
    env.call(m, FX_SPAWN_LANDING_PUFFS, &[this.wrapping_add(0x99b0), a1, a2, a3]);
}

/// `BoarderRender_RecordStreaks` 0x136148(this): every 3rd race frame, sample hand streak points when boost/tricky
/// timers (`rider+0x130` / `+0x134`) are positive (selecting streak color from `0x3bc230`/`0x3bc240`/`0x3bc250`) or
/// decay count; then on every frame sample boarder matrix into `+0x200` ring when `rider+0x428 == 0x10` or decay.
pub fn boarder_render_record_streaks(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) {
    let app = m.r32(G_APP);
    let race = m.r32(m.r32(app + 0x730) + 0x1c);
    let frame = m.r32(race + 0x18) as i32;
    let mut rider = m.r32(this);
    if frame % 3 == 0 {
        let f1 = f32::from_bits(m.r32(rider + 0x130));
        let f0 = f32::from_bits(m.r32(rider + 0x134));
        if 0.0 < f1 || 0.0 < f0 {
            let src = if 0.0 < f0 {
                G_STREAK_COLOR_HIGH
            } else if f32::from_bits(0x3f2a_7e78) < f1 {
                G_STREAK_COLOR_HIGH
            } else if f32::from_bits(0x3eaa_d18b) < f1 {
                G_STREAK_COLOR_MID
            } else {
                G_STREAK_COLOR_LOW
            };
            m.w32(this + 0x1e8, 0);
            m.w32(this + 0x1ec, m.r32(src));
            m.w32(this + 0x1f0, m.r32(src + 4));
            m.w32(this + 0x1f4, m.r32(src + 8));
            let ring = this.wrapping_add(0x20);
            let idx = m.r32(ring + 0x1c0);
            env.call(m, BOARDER_RENDER_GET_HAND_STREAKS, &[this, ring.wrapping_add(idx.wrapping_shl(6))]);
            let next_idx = (m.r32(ring + 0x1c0) as i32).wrapping_add(1) % 7;
            let cnt = m.r32(ring + 0x1c4) as i32;
            m.w32(ring + 0x1c0, next_idx as u32);
            if cnt < 7 {
                m.w32(ring + 0x1c4, cnt.wrapping_add(1) as u32);
            }
            rider = m.r32(this);
        } else {
            let ring = this.wrapping_add(0x20);
            let cnt = m.r32(ring + 0x1c4) as i32;
            if cnt > 0 {
                m.w32(ring + 0x1c4, cnt.wrapping_sub(1) as u32);
            }
        }
    }
    let ring2 = this.wrapping_add(0x200);
    if m.r32(rider + 0x428) == 0x10 {
        let idx = m.r32(ring2 + 0x1c0);
        env.call(m, BOARDER_RENDER_COPY_MATRIX, &[this, ring2.wrapping_add(idx.wrapping_shl(6))]);
        let next_idx = (m.r32(ring2 + 0x1c0) as i32).wrapping_add(1) % 7;
        let cnt = m.r32(ring2 + 0x1c4) as i32;
        m.w32(ring2 + 0x1c0, next_idx as u32);
        if cnt < 7 {
            m.w32(ring2 + 0x1c4, cnt.wrapping_add(1) as u32);
        }
    } else {
        let cnt = m.r32(ring2 + 0x1c4) as i32;
        if cnt > 0 {
            m.w32(ring2 + 0x1c4, cnt.wrapping_sub(1) as u32);
        }
    }
}

/// `BoarderRender_Reset` 0x136ea0(this): reset effects at `*(this+0x3e0)` and clear both streak ring cursors/counts.
pub fn boarder_render_reset(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) {
    let fx = m.r32(this + 0x3e0);
    boarder_render_reset_effects(m, env, fx);
    m.w32(this + 0x1e0, 0);
    m.w32(this + 0x1e4, 0);
    m.w32(this + 0x3c0, 0);
    m.w32(this + 0x3c4, 0);
}

/// `BoarderRender_DestructEffects` 0x136f88(this, flags): destruct `SpeedDust` (`+0xb600`, flags 2), `BoardPuff`
/// (`+0`, flags 2), and `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn boarder_render_destruct_effects(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, flags: u32) {
    env.call(m, FX_SPEED_DUST_DESTRUCT, &[this.wrapping_add(0xb600), 2]);
    env.call(m, FX_BOARD_PUFF_DESTRUCT, &[this, 2]);
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `TexFlip_ApplyFrame` 0x142d40(this): for each material `0..*(this+8)`, copy frame `*(this+4)` texture handle
/// from `inst->+0xf0[i+1]->+0x44[frame+1]` (u16) into `this + i*0x48 + 0x34`.
pub fn tex_flip_apply_frame(m: &mut Mem, this: u32) {
    let count = m.r32(this + 8) as i32;
    if count <= 0 {
        return;
    }
    let inst = m.r32(this);
    for i in 0..(count as u32) {
        let tbl = m.r32(inst + 0xf0);
        let mat = m.r32(tbl + 4 + i * 4);
        let anim = m.r32(mat + 0x44);
        if anim != 0 {
            let frame = m.r32(this + 4);
            let h = m.r16(anim + frame.wrapping_shl(2) + 4);
            m.w16(this + i.wrapping_mul(0x48) + 0x34, h);
        }
    }
}

/// `TexFlip_Draw` 0x142da8(this, sp): frustum-cull instance bounds via `gfx` vtable `+0x284`; if visible, apply
/// current flip frame (`0x142d40`) and submit via `gfx` vtable `+0x2ac` with `this+0x10`.
pub fn tex_flip_draw(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, sp: u32) {
    let gfx = m.r32(m.r32(G_APP) + 0x724);
    let inst = m.r32(this);
    fill_bbox_pair(m, inst, sp, sp.wrapping_add(0x10));
    if call_gfx_vf(m, env, gfx, 0x280, &[sp, sp.wrapping_add(0x10)]) == 1 {
        return;
    }
    tex_flip_apply_frame(m, this);
    let inst2 = m.r32(this);
    call_gfx_vf(
        m,
        env,
        gfx,
        0x2a8,
        &[
            m.r32(inst2 + 0xc0),
            this.wrapping_add(0x10),
            inst2,
            inst2.wrapping_add(0x40),
            0,
            0,
        ],
    );
}

/// `cTexFlip::~cTexFlip` 0x1499e8(this, flags): restore vtable `0x36d038` at `this+0x274` and `Mem_Delete(this)`
/// when `flags & 1 != 0`.
pub fn tex_flip_dtor(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, flags: u32) {
    m.w32(this + 0x274, VT_TEX_FLIP);
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `Model_IsBoxCulled` 0x19aa18(this, sp): frustum-cull `*(this+0x58)` bounds via `gfx` vtable `+0x284`, returning
/// `1` if visible (`ret != 1`) or `0` if culled (`ret == 1`).
pub fn model_is_box_culled(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, sp: u32) -> u32 {
    let gfx = m.r32(m.r32(G_APP) + 0x724);
    let inst = m.r32(this + 0x58);
    fill_bbox_pair(m, inst, sp, sp.wrapping_add(0x10));
    let ret = call_gfx_vf(m, env, gfx, 0x280, &[sp, sp.wrapping_add(0x10)]);
    (ret != 1) as u32
}

/// `MeshAnim_AllocFrames` 0x19aab0(this): count animated parts on `(*(this+0x58))->+0xc0`, allocate an array of
/// `0xd0`-byte `cMeshAnimFrame`s (with 0x10-byte count header and `0xdeadc0ed` fill), clear/identity-init each,
/// and bind each part's animation key with `AnimFrame_SetKey(..., *(f32*)(this+0x24))`.
pub fn mesh_anim_alloc_frames(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) {
    let inst = m.r32(this + 0x58);
    let model = m.r32(inst + 0xc0);
    m.w32(this + 0x5c, 0);
    let mut n_parts = m.r32(model + 4) as i32;
    let mut count = 0i32;
    if n_parts > 0 {
        let mut p = m.r32(model + 8);
        let mut i = 0i32;
        while i < n_parts {
            if m.r32(p + 0x10) != 0 {
                count = count.wrapping_add(1);
                m.w32(this + 0x5c, count as u32);
                n_parts = m.r32(model + 4) as i32;
            }
            i += 1;
            p = p.wrapping_add(0x18);
        }
    }
    if count != 0 {
        let size = (count as u32).wrapping_mul(0xd0).wrapping_add(0x10);
        let buf = env.call(
            m,
            MEMPOOL_ALLOC,
            &[G_SCRIPT_POOL, size, 0x5000_0000, STR_MESH_ANIM_FRAMES, 0, 0],
        );
        if buf != 0 {
            eamem::set(m, buf, 0xdead_c0ed, size);
        }
        m.w32(buf, count as u32);
        let base = buf.wrapping_add(0x10);
        for k in 0..(count as u32) {
            let elem = base.wrapping_add(k.wrapping_mul(0xd0));
            m.w32(elem + 0x8c, VT_ANIM_FRAME);
            env.call(m, ANIM_FRAME_CLEAR, &[elem]);
            m.w32(elem + 0x8c, VT_MESH_ANIM_FRAME);
            env.call(m, ANIM_FRAME_SET_IDENTITY, &[elem]);
        }
        m.w32(this + 0x60, base);
        n_parts = m.r32(model + 4) as i32;
    } else {
        m.w32(this + 0x60, 0);
    }
    if n_parts > 0 {
        let mut p = m.r32(model + 8);
        let mut i = 0i32;
        let mut off = 0u32;
        while i < n_parts {
            let key = m.r32(p + 0x10);
            if key != 0 {
                let frame_ptr = m.r32(this + 0x60).wrapping_add(off);
                let rate_bits = m.r32(this + 0x24);
                env.call(m, ANIM_FRAME_SET_KEY, &[frame_ptr, key, rate_bits]);
                off = off.wrapping_add(0xd0);
                n_parts = m.r32(model + 4) as i32;
            }
            i += 1;
            p = p.wrapping_add(0x18);
        }
    }
}

/// `ParticleSys_SetSpawnPoint` 0x1d5338(this, pt): copy 16-byte vector `pt` to `this+0xa0` and zero `+0xb0` and `+0xc0`.
pub fn particle_sys_set_spawn_point(m: &mut Mem, this: u32, pt: u32) {
    for k in 0..4u32 {
        m.w32(this + 0xa0 + k * 4, m.r32(pt + k * 4));
        m.w32(this + 0xb0 + k * 4, 0);
        m.w32(this + 0xc0 + k * 4, 0);
    }
}

/// `ParticleSys_SetForce` 0x1d54e0(this, force_ptr, scale_f12): scale `force_ptr` by `*(f32*)(this+0xc)` then
/// multiply by `1.0 / (scale_f12 * *(f32*)(this+0x3c))` and store at `this+0xd0`.
pub fn particle_sys_set_force(m: &mut Mem, this: u32, force_ptr: u32, scale_f12: f32) {
    let s0 = F(f32::from_bits(m.r32(this + 0xc)));
    let f = read_v4(m, force_ptr);
    let scaled = [f[0] * s0, f[1] * s0, f[2] * s0, f[3] * s0];
    let denom = F(scale_f12) * F(f32::from_bits(m.r32(this + 0x3c)));
    let q = F(1.0) / denom;
    write_v4(m, this + 0xd0, [scaled[0] * q, scaled[1] * q, scaled[2] * q, scaled[3] * q]);
}

/// `ParticleSys_SetVelocityCone` 0x1d5570(this, vel_ptr, spread_f12): compute per-frame mean velocity (`+0x60`)
/// and orthogonal spread basis (`+0x70`, `+0x80`, `+0x90 = 0`) shifted by `-1.5 * (v70 + v80 + v90)`.
pub fn particle_sys_set_velocity_cone(m: &mut Mem, this: u32, vel_ptr: u32, spread_f12: f32) {
    let q0 = F(1.0) / F(f32::from_bits(m.r32(this + 0x3c)));
    let vel = read_v4(m, vel_ptr);
    let v60 = [vel[0] * q0, vel[1] * q0, vel[2] * q0, vel[3] * q0];
    write_v4(m, this + 0x60, v60);
    let len = dot4_v4(v60, v60).sqrt();
    let ax = if vel[0].0 == 0.0 {
        [F(1.0), F(0.0), F(0.0), F(0.0)]
    } else {
        [F(0.0), F(1.0), F(0.0), F(0.0)]
    };
    let c0 = cross_v4(vel, ax);
    let rsq = F(1.0) / dot4_v4(c0, c0).sqrt();
    let u0 = [c0[0] * rsq, c0[1] * rsq, c0[2] * rsq, c0[3] * rsq];
    let spread = F(spread_f12);
    let t1 = [u0[0] * spread, u0[1] * spread, u0[2] * spread, u0[3] * spread];
    let v70 = [t1[0] * len, t1[1] * len, t1[2] * len, t1[3] * len];
    write_v4(m, this + 0x70, v70);

    let c1 = cross_v4(v60, u0);
    let v80 = [c1[0] * spread, c1[1] * spread, c1[2] * spread, c1[3] * spread];
    let v90 = [F(0.0); 4];
    write_v4(m, this + 0x80, v80);
    write_v4(m, this + 0x90, v90);

    let sum0 = [v70[0] + v80[0], v70[1] + v80[1], v70[2] + v80[2], v70[3] + v80[3]];
    let sum1 = [sum0[0] + v90[0], sum0[1] + v90[1], sum0[2] + v90[2], sum0[3] + v90[3]];
    let k15 = F(1.5);
    let shift = [sum1[0] * k15, sum1[1] * k15, sum1[2] * k15, sum1[3] * k15];
    let v60_out = [v60[0] - shift[0], v60[1] - shift[1], v60[2] - shift[2], v60[3] - shift[3]];
    write_v4(m, this + 0x60, v60_out);
}

/// `ParticleNode_FreeMem` 0x1d8b00(this, flags): `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn particle_node_free_mem(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, flags: u32) {
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `cParticleMan::vf1_1d98f8` 0x1d98f8: no-op.
pub fn particle_man_vf1() {}

/// `SkelSeq_DtorMember34` 0x203d40(this): set vtable `0x39e470` at `this+0x34`, call `cSkelSequence::vf1_203cf8`,
/// and return `this`.
pub fn skel_seq_dtor_member34(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) -> u32 {
    m.w32(this + 0x34, VT_SKEL_SEQUENCE);
    env.call(m, SKEL_SEQ_VF1, &[this]);
    this
}

/// `MeshChunk_ScalePositions` 0x2049b0(chunk, num_morphs, scale_f12): walk DMA/VIF strip/UV/normal/position
/// packets, scale every vertex `(x, y, z)` by `scale_f12`, skip `num_morphs` morph packets, and return end ptr.
pub fn mesh_chunk_scale_positions(m: &mut Mem, chunk: u32, num_morphs: i32, scale_f12: f32) -> u32 {
    let mut p = chunk;
    let qwc0 = m.r32(p) & 0xffff;
    p = p.wrapping_add(0x10);
    let n_strips = m.r32(p + 0x20) as i32;
    let mut total_verts = 0i32;
    for i in 0..n_strips {
        total_verts = total_verts.wrapping_add(m.r32(p + 0x30 + (i as u32).wrapping_shl(4)) as i32);
    }
    p = p.wrapping_add(qwc0.wrapping_shl(4));

    let qwc1 = m.r32(p) & 0xffff;
    p = p.wrapping_add(0x10);
    let half = ((total_verts as u32).wrapping_add((total_verts as u32) >> 31) as i32) >> 1;
    let uv_off = if (total_verts.wrapping_shl(3) & 0xf) != 0 {
        half.wrapping_shl(4).wrapping_add(0x40)
    } else {
        half.wrapping_shl(4).wrapping_add(0x30)
    };
    let p_norm = p.wrapping_add(uv_off as u32);
    let nb = total_verts.wrapping_mul(6);
    let q_norm = if nb >= 0 { nb >> 4 } else { nb.wrapping_add(0xf) >> 4 };
    let norm_off = if (nb & 0xf) != 0 {
        q_norm.wrapping_shl(4).wrapping_add(0x40)
    } else {
        q_norm.wrapping_shl(4).wrapping_add(0x30)
    };
    let mut pos = p_norm.wrapping_add(norm_off as u32).wrapping_add(0x30);
    let scale = F(scale_f12);
    for _ in 0..total_verts {
        for k in 0..3u32 {
            let v = F(f32::from_bits(m.r32(pos + k * 4))) * scale;
            m.w32(pos + k * 4, v.0.to_bits());
        }
        pos = pos.wrapping_add(0xc);
    }
    p = p.wrapping_add(qwc1.wrapping_shl(4));

    let qwc2 = m.r32(p) & 0xffff;
    p = p.wrapping_add(0x10).wrapping_add(qwc2.wrapping_shl(4));

    for _ in 0..num_morphs {
        let qwcm = m.r32(p) & 0xffff;
        p = p.wrapping_add(0x10).wrapping_add(qwcm.wrapping_shl(4));
    }
    p
}

/// `Model_ScalePositions` 0x204f28(model, _unused, num_morphs, scale_f12): when `*(model) & 0x100 != 0`, scale
/// every chunk across every LOD group via `MeshChunk_ScalePositions`.
pub fn model_scale_positions(m: &mut Mem, model: u32, num_morphs: i32, scale_f12: f32) {
    if (m.r32(model) & 0x100) == 0 {
        return;
    }
    let mut n_groups = m.r32(model + 0xc) as i32;
    let mut g = 0i32;
    while g < n_groups {
        let grp_off = (g as u32).wrapping_shl(3);
        let mut n_chunks = m.r32(m.r32(model + 0x10).wrapping_add(grp_off).wrapping_add(4)) as i32;
        if n_chunks > 0 {
            let mut c = 0i32;
            while c < n_chunks {
                let arr = m.r32(m.r32(model + 0x10).wrapping_add(grp_off));
                let chunk = m.r32(arr.wrapping_add((c as u32).wrapping_mul(0xc)));
                mesh_chunk_scale_positions(m, chunk, num_morphs, scale_f12);
                c += 1;
                n_chunks = m.r32(m.r32(model + 0x10).wrapping_add(grp_off).wrapping_add(4)) as i32;
            }
            n_groups = m.r32(model + 0xc) as i32;
        }
        g += 1;
    }
}

/// `OwnedBuf_Delete1` 0x21e788 and `OwnedBuf_Delete2` 0x235e70(this, flags): free `*(this+4)` via `Mem_Free`
/// if non-null, then `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn owned_buf_delete(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, flags: u32) {
    let p = m.r32(this + 4);
    if p != 0 {
        env.call(m, MEM_FREE, &[p]);
    }
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `MaskedList_Draw` 0x238980(this, sp): draw bounding sphere at `this+0x10` (radius `this+0x20`, red `[1,0,0,1]`)
/// and each active sphere in mask `this+0x24` (`0..*(this+0x28)`, yellow `[1,1,0,0]`) via `gfx` vtable `+0x26c`.
pub fn masked_list_draw(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, sp: u32) {
    let gfx = m.r32(m.r32(G_APP) + 0x724);
    m.w32(sp, ONE_F32_BITS);
    m.w32(sp + 4, 0);
    m.w32(sp + 8, 0);
    m.w32(sp + 12, ONE_F32_BITS);
    call_gfx_vf(
        m,
        env,
        gfx,
        0x268,
        &[this.wrapping_add(0x10), sp, 0xffff_ffff, m.r32(this + 0x20)],
    );
    let mut count = m.r32(this + 0x28) as i32;
    let mut i = 0i32;
    while i < count {
        if (((m.r32(this + 0x24) as i32) >> (i & 31)) & 1) != 0 {
            m.w32(sp, ONE_F32_BITS);
            m.w32(sp + 4, ONE_F32_BITS);
            m.w32(sp + 8, 0);
            m.w32(sp + 12, 0);
            let off = (i as u32).wrapping_mul(0x20);
            call_gfx_vf(
                m,
                env,
                gfx,
                0x268,
                &[
                    this.wrapping_add(0x30).wrapping_add(off),
                    sp,
                    0xffff_ffff,
                    m.r32(this.wrapping_add(0x40).wrapping_add(off)),
                ],
            );
            count = m.r32(this + 0x28) as i32;
        }
        i += 1;
    }
}

/// `SphereTreeCache_Init` 0x238af0(_this, n_a, depth_a, n_b, depth_b): allocate global sphere-tree octree buffer
/// pools A (`0x3439a0..ac`) and B (`0x3439b0..c0`) with `0xdeadc0ed` fill and `-1` slot tags.
pub fn sphere_tree_cache_init(
    m: &mut Mem,
    env: &mut dyn RenderEnv,
    _this: u32,
    n_a: i32,
    depth_a: i32,
    n_b: i32,
    depth_b: i32,
) {
    m.w32(G_SPH_CACHE_DEPTH_B, depth_b as u32);
    let mut sz_a = 0u32;
    if depth_a >= 0 {
        let mut p = 1u32;
        for _ in 0..=depth_a {
            sz_a = sz_a.wrapping_add(p);
            p = p.wrapping_shl(3);
        }
    }
    let mut sz_b = 0u32;
    let db = m.r32(G_SPH_CACHE_DEPTH_B) as i32;
    if db >= 0 {
        let mut p = 1u32;
        for _ in 0..=db {
            sz_b = sz_b.wrapping_add(p);
            p = p.wrapping_shl(3);
        }
    }
    m.w32(G_SPH_CACHE_A_PTRS, 0);
    m.w32(G_SPH_CACHE_A_IDS, 0);
    m.w32(G_SPH_CACHE_B_PTRS, 0);
    m.w32(G_SPH_CACHE_B_IDS, 0);

    let alloc_filled = |m: &mut Mem, env: &mut dyn RenderEnv, bytes: u32| -> u32 {
        let p = env.call(m, EAMEM_ALLOC, &[STR_SPHERE_TREE_CACHE, bytes, 0]);
        if p != 0 {
            eamem::set(m, p, 0xdead_c0ed, bytes);
        }
        p
    };

    if n_a != 0 {
        let bytes = (n_a as u32).wrapping_shl(2);
        let p0 = alloc_filled(m, env, bytes);
        m.w32(G_SPH_CACHE_A_PTRS, p0);
        let p1 = alloc_filled(m, env, bytes);
        m.w32(G_SPH_CACHE_A_IDS, p1);
    }
    if n_b != 0 {
        let bytes = (n_b as u32).wrapping_shl(2);
        let p0 = alloc_filled(m, env, bytes);
        m.w32(G_SPH_CACHE_B_PTRS, p0);
        let p1 = alloc_filled(m, env, bytes);
        m.w32(G_SPH_CACHE_B_IDS, p1);
    }
    for i in 0..n_a {
        let off = (i as u32).wrapping_shl(2);
        let dst = m.r32(G_SPH_CACHE_A_PTRS).wrapping_add(off);
        let p = alloc_filled(m, env, sz_a);
        m.w32(dst, p);
        m.w32(m.r32(G_SPH_CACHE_A_IDS).wrapping_add(off), 0xffff_ffff);
    }
    for i in 0..n_b {
        let off = (i as u32).wrapping_shl(2);
        let dst = m.r32(G_SPH_CACHE_B_PTRS).wrapping_add(off);
        let p = alloc_filled(m, env, sz_b);
        m.w32(dst, p);
        m.w32(m.r32(G_SPH_CACHE_B_IDS).wrapping_add(off), 0xffff_ffff);
    }
    m.w32(G_SPH_CACHE_A_CAP, n_a as u32);
    m.w32(G_SPH_CACHE_B_CAP, n_b as u32);
    m.w32(G_SPH_CACHE_A_CUR, 0);
    m.w32(G_SPH_CACHE_B_CUR, 0);
}

/// `SphereTreeBufCache_FreeAll` 0x238dd0(): free every allocated buffer and array in global pools A and B.
pub fn sphere_tree_buf_cache_free_all(m: &mut Mem, env: &mut dyn RenderEnv) {
    if m.r32(G_SPH_CACHE_A_PTRS) != 0 {
        let mut cap = m.r32(G_SPH_CACHE_A_CAP) as i32;
        let mut i = 0i32;
        while i < cap {
            let p = m.r32(m.r32(G_SPH_CACHE_A_PTRS).wrapping_add((i as u32).wrapping_shl(2)));
            if p != 0 {
                env.call(m, MEM_FREE, &[p]);
            }
            cap = m.r32(G_SPH_CACHE_A_CAP) as i32;
            i += 1;
        }
        let arr = m.r32(G_SPH_CACHE_A_PTRS);
        if arr != 0 {
            env.call(m, MEM_FREE, &[arr]);
        }
        m.w32(G_SPH_CACHE_A_PTRS, 0);
    }
    let ids_a = m.r32(G_SPH_CACHE_A_IDS);
    if ids_a != 0 {
        env.call(m, MEM_FREE, &[ids_a]);
        m.w32(G_SPH_CACHE_A_IDS, 0);
    }
    if m.r32(G_SPH_CACHE_B_PTRS) != 0 {
        let mut cap = m.r32(G_SPH_CACHE_B_CAP) as i32;
        let mut i = 0i32;
        while i < cap {
            let p = m.r32(m.r32(G_SPH_CACHE_B_PTRS).wrapping_add((i as u32).wrapping_shl(2)));
            if p != 0 {
                env.call(m, MEM_FREE, &[p]);
            }
            cap = m.r32(G_SPH_CACHE_B_CAP) as i32;
            i += 1;
        }
        let arr = m.r32(G_SPH_CACHE_B_PTRS);
        if arr != 0 {
            env.call(m, MEM_FREE, &[arr]);
        }
        m.w32(G_SPH_CACHE_B_PTRS, 0);
    }
    let ids_b = m.r32(G_SPH_CACHE_B_IDS);
    if ids_b != 0 {
        env.call(m, MEM_FREE, &[ids_b]);
        m.w32(G_SPH_CACHE_B_IDS, 0);
    }
}

/// `Rle_Encode` 0x239960(dst, src, len, min_run): pack `src[0..len]` into signed-count RLE packets (matching
/// `SphTree_RleDecode` 0x239b20) with a terminating `0` byte, returning total bytes written.
pub fn rle_encode(m: &mut Mem, dst: u32, src: u32, len: u32, min_run_arg: i32) -> u32 {
    let min_run = if min_run_arg > 1 { min_run_arg } else { 2 };
    let end = src.wrapping_add(len);
    let mut s2 = src;
    let mut out = dst;
    if s2 >= end {
        eamem::set(m, out, 0, 1);
        return 1;
    }
    while s2 < end {
        let mut a2 = s2;
        let mut run = 0i32;
        let mut lit = 0i32;
        while a2 < end {
            let b0 = m.r8(a2);
            run = 1;
            let mut v1 = a2.wrapping_add(1);
            if m.r8(v1) == b0 && v1 < end {
                loop {
                    v1 = v1.wrapping_add(1);
                    let next_b = m.r8(v1);
                    run += 1;
                    if next_b != b0 || v1 >= end || run >= 0x7f {
                        break;
                    }
                }
            }
            if v1 == end && run >= 2 {
                run -= 1;
                v1 = end.wrapping_sub(1);
            }
            if run < min_run {
                lit += run;
                a2 = v1;
                run = 0;
            }
            if run != 0 || lit >= 0x7f {
                break;
            }
        }
        if lit >= 0x80 {
            lit = 0x7f;
        }
        if lit != 0 {
            eamem::set(m, out, lit.wrapping_neg() as u32, 1);
            out = out.wrapping_add(1);
            for _ in 0..lit {
                let b = m.r8(s2);
                m.w8(out, b);
                s2 = s2.wrapping_add(1);
                out = out.wrapping_add(1);
            }
        }
        if run != 0 {
            let r1 = run - 1;
            eamem::set(m, out, r1 as u32, 1);
            out = out.wrapping_add(1);
            m.w8(out, m.r8(s2));
            s2 = s2.wrapping_add(1).wrapping_add(r1 as u32);
            out = out.wrapping_add(1);
        }
    }
    eamem::set(m, out, 0, 1);
    out.wrapping_add(1).wrapping_sub(dst)
}

/// `SphereTreeBufCache::~SphereTreeBufCache` 0x23b520(this, flags): restore vtable `0x3a40f8` at `this+0` and
/// `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn sphere_tree_buf_cache_dtor(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, flags: u32) {
    m.w32(this, VT_SPHERE_TREE_BUF_CACHE);
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

// ---- Row G7t: cPS2GraphicsMan / cGraphicsMan / cPS2LightMan / cPS2GridMesh virtuals & matrix stack ----

pub const G_IDENTITY_MAT4: u32 = 0x0034_6440;

pub const VT_PS2_DRAW_STATE: u32 = 0x0039_0ad0;
pub const VT_PS2_GRAPHICS_MAN: u32 = 0x0039_3610;
pub const VT_GRAPHICS_MAN: u32 = 0x0039_3a20;

pub const LIGHT_MAN_DTOR: u32 = 0x001c_e110;
pub const GFX_BEGIN_BUFFER: u32 = 0x001e_40a0;
pub const GFX_FLUSH_IF_IDLE: u32 = 0x001e_40f0;
pub const GFX_PROGRAM_DISPLAY: u32 = 0x001e_4288;
pub const PS2_RENDER_UPDATE_MODEL_MATRICES: u32 = 0x001e_85b0;
pub const PS2_RENDER_FLUSH_STATE: u32 = 0x001e_88c8;

fn transform_v4(mat: &[V4; 4], v: V4) -> V4 {
    [0, 1, 2, 3].map(|i| ((mat[0][i] * v[0] + mat[1][i] * v[1]) + mat[2][i] * v[2]) + mat[3][i] * v[3])
}

fn read_m4(m: &Mem, at: u32) -> [V4; 4] {
    [
        read_v4(m, at),
        read_v4(m, at + 0x10),
        read_v4(m, at + 0x20),
        read_v4(m, at + 0x30),
    ]
}

fn write_m4(m: &mut Mem, at: u32, mat: [V4; 4]) {
    for (r, row) in mat.iter().enumerate() {
        write_v4(m, at + (r as u32) * 0x10, *row);
    }
}

/// `cGraphicsMan::~cGraphicsMan` 0x1dbab0 and `cPS2GraphicsMan::~cPS2GraphicsMan` 0x1ea130(this, flags):
/// restore `cGraphicsMan` vtable `0x393a20` at `this+0x10`, and `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn graphics_man_dtor(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, flags: u32) {
    m.w32(this + 0x10, VT_GRAPHICS_MAN);
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `cPS2GraphicsMan::vf36_1e8928` 0x1e8928(this, src): copy 20 bytes from `src` to `this+0x17d4`.
pub fn ps2_gfx_set_x17d4(m: &mut Mem, this: u32, src: u32) {
    for k in 0..5u32 {
        m.w32(this + 0x17d4 + k * 4, m.r32(src + k * 4));
    }
}

/// `cPS2GraphicsMan::vf23_1e8970` 0x1e8970(out, this): read active `0x34`-byte stack entry at
/// `this + 0x14 + *(this+0xe4)*0x34`, convert its four floats at `+4..+0x14` via `cvt.w.s` into `out[0..4]`.
pub fn ps2_gfx_get_viewport_i32(m: &mut Mem, out: u32, this: u32) -> u32 {
    let idx = m.r32(this + 0xe4);
    let entry = this.wrapping_add(0x14).wrapping_add(idx.wrapping_mul(0x34));
    for k in 0..4u32 {
        let f = f32::from_bits(m.r32(entry + 4 + k * 4));
        m.w32(out + k * 4, (f as i32) as u32);
    }
    out
}

/// `cPS2GraphicsMan::vf16_1e8cd8` 0x1e8cd8(this): push a copy of the top `0x34`-byte stack entry
/// (`this+0x14 + idx*0x34` -> `+0x48`) and increment `*(this+0xe4)`.
pub fn ps2_gfx_push_viewport(m: &mut Mem, this: u32) {
    let idx = m.r32(this + 0xe4);
    let base = this.wrapping_add(idx.wrapping_mul(0x34));
    for k in 0..13u32 {
        let v = m.r32(base + 0x14 + k * 4);
        m.w32(base + 0x48 + k * 4, v);
    }
    m.w32(this + 0xe4, idx.wrapping_add(1));
}

/// `cPS2GraphicsMan::vf11_1e8d60` 0x1e8d60(this): return `*(table + idx*0x94 + 0x84)` (`idx = +0x650`, `table = +0x654`).
pub fn ps2_gfx_get_mode_x84(m: &Mem, this: u32) -> u32 {
    let idx = m.r32(this + 0x650);
    let tbl = m.r32(this + 0x654);
    m.r32(tbl.wrapping_add(idx.wrapping_mul(0x94)).wrapping_add(0x84))
}

/// `cPS2GraphicsMan::vf10_1e8d80` 0x1e8d80(this): return `*(table + idx*0x94 + 0x80)`.
pub fn ps2_gfx_get_mode_x80(m: &Mem, this: u32) -> u32 {
    let idx = m.r32(this + 0x650);
    let tbl = m.r32(this + 0x654);
    m.r32(tbl.wrapping_add(idx.wrapping_mul(0x94)).wrapping_add(0x80))
}

/// `cPS2GraphicsMan::vf7_1e8da0` 0x1e8da0(this): return `*(this+0x650)`.
pub fn ps2_gfx_get_mode_index(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x650)
}

/// `cPS2GraphicsMan::vf8_1e8da8` 0x1e8da8(this, index): return `*(this+0x654) + index * 0x94`.
pub fn ps2_gfx_get_mode_entry(m: &Mem, this: u32, index: u32) -> u32 {
    m.r32(this + 0x654).wrapping_add(index.wrapping_mul(0x94))
}

/// `cPS2GraphicsMan::vf9_1e8dc0` 0x1e8dc0(this): return `*(this+0x64c)`.
pub fn ps2_gfx_get_mode_count(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x64c)
}

/// `cPS2GridMesh::vf1_1e8dc8` 0x1e8dc8(this): return `*(this+0x50)`.
pub fn ps2_grid_mesh_vf1(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x50)
}

/// `cPS2GraphicsMan::vf12_1e8e20` 0x1e8e20(this, rgb_ptr): pack `(u32)(r*255) | ((u32)(g*255) << 8) |
/// ((u32)(b*255) << 16)` into `*(this+0x1724)`.
pub fn ps2_gfx_set_clear_color(m: &mut Mem, this: u32, rgb_ptr: u32) {
    let k = F(255.0);
    let r = (F(f32::from_bits(m.r32(rgb_ptr))) * k).0 as u32;
    let g = (F(f32::from_bits(m.r32(rgb_ptr + 4))) * k).0 as u32;
    let b = (F(f32::from_bits(m.r32(rgb_ptr + 8))) * k).0 as u32;
    m.w32(this + 0x1724, r | (g << 8) | (b << 16));
}

/// `Gfx_FlipDisplay` 0x1e8ea0(a0, a1): tail-call `Gfx_ProgramDisplay(a0, a1, 1)`.
pub fn gfx_flip_display(m: &mut Mem, env: &mut dyn RenderEnv, a0: u32, a1: u32) -> u32 {
    env.call(m, GFX_PROGRAM_DISPLAY, &[a0, a1, 1])
}

/// `cPS2GraphicsMan::vf88_1e9038` 0x1e9038(this): dispatch `(*(this+0x484))->vf_0x84(this)`.
pub fn ps2_gfx_vf88(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) -> u32 {
    let obj = m.r32(this + 0x484);
    let vt = m.r32(obj);
    let adj = m.r16(vt + 0x80) as i16 as i32 as u32;
    let f = m.r32(vt + 0x84);
    env.call(m, f, &[obj.wrapping_add(adj), this])
}

/// `cPS2GraphicsMan::vf14_1e90e8` 0x1e90e8(this): call `Gfx_FlushIfIdle(this)`.
pub fn ps2_gfx_vf14(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) -> u32 {
    env.call(m, GFX_FLUSH_IF_IDLE, &[this])
}

/// `cPS2GraphicsMan::vf13_1e9108` 0x1e9108(this): call `Gfx_BeginBuffer(this)`.
pub fn ps2_gfx_vf13(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) -> u32 {
    env.call(m, GFX_BEGIN_BUFFER, &[this])
}

/// `PS2Render_CalcFog` 0x1e9128(this, pt): update model matrices (`0x1e85b0`), transform `pt` by matrix at
/// `this+0x5b0`, multiply `.w` by `*(this+0x63c)`, clamp to `[0.0, *(this+0x630)]`, and return `u16` truncated int.
pub fn ps2_render_calc_fog(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, pt: u32) -> u32 {
    env.call(m, PS2_RENDER_UPDATE_MODEL_MATRICES, &[this]);
    let vf5 = read_v4(m, this + 0x630);
    let v = read_v4(m, pt);
    let mat = read_m4(m, this + 0x5b0);
    let mut w = transform_v4(&mat, v)[3] * vf5[3];
    if w.0 < 0.0 {
        w = F(0.0);
    }
    if vf5[0].0 < w.0 {
        w = vf5[0];
    }
    ((w.0 as i32) as u16) as u32
}

/// `cPS2GraphicsMan::vf53_1e9628` 0x1e9628(this): call virtual `+0xc` on `*(this+0x470)` (vtable at `+0x18`),
/// set `this+0x474 = 1`, `this+0x478 = 1`, and tail-call `PS2Render_FlushState(this)`.
pub fn ps2_gfx_vf53(m: &mut Mem, env: &mut dyn RenderEnv, this: u32) -> u32 {
    let st = m.r32(this + 0x470);
    let vt = m.r32(st + 0x18);
    let adj = m.r16(vt + 8) as i16 as i32 as u32;
    let f = m.r32(vt + 0xc);
    env.call(m, f, &[st.wrapping_add(adj)]);
    m.w32(this + 0x478, 1);
    m.w32(this + 0x474, 1);
    env.call(m, PS2_RENDER_FLUSH_STATE, &[this])
}

/// `cPS2GraphicsMan::vf33_1e9880` 0x1e9880(this, vec): translate current matrix `*(this+0x3f4)` (`row3 = M * vec`)
/// and clear dirty cache flag `*(this+0x17d0)`.
pub fn ps2_gfx_mat_translate(m: &mut Mem, this: u32, vec: u32) {
    let mat_ptr = m.r32(this + 0x3f4);
    let mat = read_m4(m, mat_ptr);
    let v = read_v4(m, vec);
    let r3 = transform_v4(&mat, v);
    write_v4(m, mat_ptr + 0x30, r3);
    m.w32(this + 0x17d0, 0);
}

/// `cPS2GraphicsMan::vf32_1e98b8` 0x1e98b8(this): load `G_IDENTITY_MAT4` (`0x346440`) into `*(this+0x3f4)` and
/// clear `*(this+0x17d0)`.
pub fn ps2_gfx_mat_load_identity(m: &mut Mem, this: u32) {
    let mat_ptr = m.r32(this + 0x3f4);
    for k in 0..16u32 {
        m.w32(mat_ptr + k * 4, m.r32(G_IDENTITY_MAT4 + k * 4));
    }
    m.w32(this + 0x17d0, 0);
}

/// `cPS2GraphicsMan::vf31_1e98f0` 0x1e98f0(this, rhs): post-multiply `*(this+0x3f4)` by `rhs` in place and clear
/// `*(this+0x17d0)`.
pub fn ps2_gfx_mat_mul(m: &mut Mem, this: u32, rhs_ptr: u32) {
    let mat_ptr = m.r32(this + 0x3f4);
    let lhs = read_m4(m, mat_ptr);
    let rhs = read_m4(m, rhs_ptr);
    let out = [
        transform_v4(&lhs, rhs[0]),
        transform_v4(&lhs, rhs[1]),
        transform_v4(&lhs, rhs[2]),
        transform_v4(&lhs, rhs[3]),
    ];
    write_m4(m, mat_ptr, out);
    m.w32(this + 0x17d0, 0);
}

/// `cPS2GraphicsMan::vf29_1e9978` 0x1e9978(this, src): copy 64-byte matrix `src` into `*(this+0x3f4)` and clear
/// `*(this+0x17d0)`.
pub fn ps2_gfx_mat_load(m: &mut Mem, this: u32, src: u32) {
    let mat_ptr = m.r32(this + 0x3f4);
    for k in 0..16u32 {
        m.w32(mat_ptr + k * 4, m.r32(src + k * 4));
    }
    m.w32(this + 0x17d0, 0);
}

/// `cPS2GraphicsMan::vf27_1e9e88` 0x1e9e88(this): pop matrix stack (`+0x3f4 -= 0x40`, `+0x3f0 -= 1`) and clear
/// `*(this+0x17d0)`.
pub fn ps2_gfx_mat_pop(m: &mut Mem, this: u32) {
    let p = m.r32(this + 0x3f4).wrapping_sub(0x40);
    let d = m.r32(this + 0x3f0).wrapping_sub(1);
    m.w32(this + 0x17d0, 0);
    m.w32(this + 0x3f4, p);
    m.w32(this + 0x3f0, d);
}

/// `cPS2GraphicsMan::vf26_1e9ea8` 0x1e9ea8(this): duplicate top matrix (`+0x3f4` -> `+0x3f4+0x40`) and increment
/// `+0x3f4` by `0x40` and `+0x3f0` by `1`.
pub fn ps2_gfx_mat_push(m: &mut Mem, this: u32) {
    let p = m.r32(this + 0x3f4);
    for k in 0..16u32 {
        let v = m.r32(p + k * 4);
        m.w32(p + 0x40 + k * 4, v);
    }
    let p2 = m.r32(this + 0x3f4).wrapping_add(0x40);
    let d = m.r32(this + 0x3f0).wrapping_add(1);
    m.w32(this + 0x3f4, p2);
    m.w32(this + 0x3f0, d);
}

/// `cPS2GraphicsMan::vf96_1e9ff0` 0x1e9ff0(this): return `*(this+0x6c4)`.
pub fn ps2_gfx_vf96(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x6c4)
}

/// `cPS2GraphicsMan::vf65_1e9ff8` 0x1e9ff8(this): return `*(u8*)(*(this+0x470) + 0xe)`.
pub fn ps2_gfx_vf65(m: &Mem, this: u32) -> u32 {
    m.r8(m.r32(this + 0x470) + 0xe) as u32
}

/// `cPS2GraphicsMan::vf64_1ea008` 0x1ea008(this): return `*(u8*)(*(this+0x470) + 0x15)`.
pub fn ps2_gfx_vf64(m: &Mem, this: u32) -> u32 {
    m.r8(m.r32(this + 0x470) + 0x15) as u32
}

/// `cPS2GraphicsMan::vf37_1ea018` 0x1ea018(this): return `this + 0x17d4`.
pub fn ps2_gfx_vf37(this: u32) -> u32 {
    this.wrapping_add(0x17d4)
}

/// `cPS2GraphicsMan::vf19_1ea020` 0x1ea020(this, rect_i32_ptr): convert four `i32`s to `f32` and dispatch
/// virtual `+0x9c` (`delta +0x98`) with `$f12..$f15`.
pub fn ps2_gfx_vf19(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, rect_ptr: u32) -> u32 {
    let f12 = (m.r32(rect_ptr) as i32 as f32).to_bits();
    let f13 = (m.r32(rect_ptr + 4) as i32 as f32).to_bits();
    let f14 = (m.r32(rect_ptr + 8) as i32 as f32).to_bits();
    let f15 = (m.r32(rect_ptr + 12) as i32 as f32).to_bits();
    call_gfx_vf(m, env, this, 0x98, &[f12, f13, f14, f15])
}

/// `cPS2GraphicsMan::cPS2GraphicsMan` 0x1ea0c0(this): set vtable `0x393610` at `+0x10` and `tPS2DrawState` vtable
/// `0x390ad0` on the 4 embedded draw states at `+0x3fc + i*0x1c + 0x18`.
pub fn ps2_graphics_man_ctor(m: &mut Mem, this: u32) -> u32 {
    m.w32(this + 0x10, VT_PS2_GRAPHICS_MAN);
    for i in 0..4u32 {
        m.w32(this + 0x3fc + i * 0x1c + 0x18, VT_PS2_DRAW_STATE);
    }
    this
}

/// `cPS2LightMan::~cPS2LightMan` 0x1ea290(this, flags): tail-call `cLightMan::~cLightMan(this, flags)`.
pub fn ps2_light_man_dtor(m: &mut Mem, env: &mut dyn RenderEnv, this: u32, flags: u32) {
    env.call(m, LIGHT_MAN_DTOR, &[this, flags]);
}

/// No-op virtual stubs (`cPS2LightMan::vf14_1ea1d8`, `vf13_1ea1e0`, `vf12_1ea1e8`, `vf11_1ea1f0`, `vf10_1ea1f8`,
/// `vf9_1ea200`, `vf7_1ea208`, `vf5_1ea218`, `vf4_1ea220`, `vf3_1ea228`, `vf2_1ea230`,
/// `cGraphicsMan::vf95_1ea2e0`, `vf92_1ea2e8`, `vf86_1ea2f0`).
pub fn gfx_noop() {}

/// Return-0 virtual stubs (`cPS2LightMan::vf6_1ea210`, `vf1_1ea238`, `cGraphicsMan::vf96_1ea2d8`,
/// `vf83_1ea2f8`, `vf82_1ea300`).
pub fn gfx_ret0() -> u32 {
    0
}

/// Return-1 virtual stubs (`cGraphicsMan::vf14_1ea308`, `vf13_1ea310`).
pub fn gfx_ret1() -> u32 {
    1
}
