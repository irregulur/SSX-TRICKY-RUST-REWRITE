//! Board effects on the game's own ported code (board row E1b2g3): `Boarder_UpdateEffects` 0x1350c0
//! (`tricky_game::effects::plan`) running `Fx_BoardPuff_Update` 0x12eca8 (`puff::puff_update`),
//! `Fx_BoardTrail_Update` 0x12fef0 (`boardtrail::update`), `Fx_SurfaceSpray` 0x1311b0 (`surfacespray::update`),
//! `Fx_BrakeSpray_Update` 0x12e6e8 (`brakefx::brake_update`, with `Fx_BrakeFan_Update` 0x12d108),
//! `Fx_SpeedDust_Update` 0x12f4c0 (`puff::dust_update`), plus `Fx_SpawnLandingPuffs` 0x12e0e8
//! (`brakefx::spawn_landing_puffs` -> `Fx_LandingSplash` 0x12ccc8), `Fx_BrakeSpray_Emit` 0x12e008
//! (`brakespray::emit` -> `Fx_Splash_EmitAtPoint` 0x12c7e0), and `cBoarderRender::vf7` 0x1356f8's
//! `cParticleMan` trails (`ParticleGroup_Add` 0x1d9060, `ParticleGroup_UpdateTrail` 0x1d9208,
//! `ParticleGroup_Remove` 0x1d9138). Every random draw comes from `Rand_Fx` 0x23d4a8 (`tricky_game::rand::rand_fx`).
//!
//! Drawn by `BoarderRender_DrawEffects` 0x135170 (`boardfx::draw_effects` -> `puff_draw`, `trail_draw`,
//! `fxdraw::surface_spray_draw`, `spray_draw` -> `fxdraw::spray_draw_particles`, `fxdraw::speed_dust_draw`)
//! and `cParticleMan::vf2` 0x1d9900 (`particleman::draw` -> `draw_slot`), with `Fx_SurfaceSpray_Draw` feeding
//! `particledraw::draw_program6` 0x1e2a60 -> VU1 program 6 (`particlevu::run6`). The sixth effect in
//! `Boarder_UpdateEffects`, `Fx_BoardTrack` 0x132648 (+0x8020), runs in `tracks.rs` (it makes no `Rand_Fx` draws).

use crate::rider::Rider;
use crate::{ui, CharLib, GameTick, Mode, Opponents, RiderRes};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use std::collections::HashMap;
use tricky_game::boardfx;
use tricky_game::boardtrail::{self, Trail, TrailIn};
use tricky_game::brakefx::{self, BrakeFx, FanIn, LandingPuffsIn};
use tricky_game::brakespray::{self, SprayEnv};
use tricky_game::cull::{self, CullEnv};
use tricky_game::effects::{self, Effect};
use tricky_game::fxdraw;
use tricky_game::particledraw::{self, ParticleDrawEnv};
use tricky_game::particleman::{self, ParticleMemEnv};
use tricky_game::particlevu;
use tricky_game::pf::Mem;
use tricky_game::puff::{self, Dust, DustIn, Puffs, PuffIn};
use tricky_game::rand::rand_fx;
use tricky_game::riderdraw::{RiderDrawEnv, APP};
use tricky_game::splash::{self, SplashRider};
use tricky_game::surface::{surf as surf_word, surf_int};
use tricky_game::surfacespray::{self, Light, Spray as SurfaceSpray, SprayIn};
use tricky_game::vu::Vu;

type V4 = [f32; 4];

// Emulated EE memory layout for the ported draw functions and cParticleMan.
const APP_OBJ: u32 = 0x0100_0000;
const TEX_TABLE: u32 = APP_OBJ + 0x400;
const REND: u32 = 0x0101_0000;
const VT: u32 = 0x0102_0000;
const TS: u32 = 0x0102_0400;
const GAME: u32 = 0x0103_0000;
const MAT: u32 = 0x0104_0000;
const PM: u32 = 0x0105_0000;
const PM_POOL: u32 = 0x0106_0000;
const SCRATCH: u32 = 0x0107_0000;
const PKT: u32 = 0x0108_0000;
const FX_BASE: u32 = 0x0110_0000;
const FX_STRIDE: u32 = 0x0001_0000;

/// Screen scale for VU1 program 6 (ndc to ftoi4 integers, invertible, as in `snow.rs`).
const VU_S: f32 = 4096.0;
const VU_Z: f32 = 1_048_576.0;
/// Program 6 block place in VU1 memory (20 quadwords 7..27, before packet buffer 0 at 47).
const TOP: u32 = 7;

/// Sprite index table matching `gApp+0x30` (`particles::TEX_NAMES`).
const SPRITES: [u32; 38] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37,
];

/// STANDIN: fixed snow light colour `[0, r, g, b]` matching `tracks.rs`'s fixed sun, for the rider's
/// light set `*(rider+0x5ae0) + 0x470` (row G7b).
const LIGHT_COLOUR: V4 = [0.0, 0.85, 0.88, 0.95];

#[derive(Component)]
pub struct SprayMesh;

struct RiderFx {
    puffs: Puffs,
    trail: Trail,
    spray: SurfaceSpray,
    light: Light,
    brake: BrakeFx,
    dust: Dust,
    pm_active: bool,
    pm_slots: [i32; 2],
}

impl Default for RiderFx {
    fn default() -> Self {
        Self {
            puffs: Puffs::default(),
            trail: Trail::default(),
            spray: SurfaceSpray::default(),
            light: Light { f0c: 0.0, f10: 0.0, f14: 0.0 },
            brake: BrakeFx::default(),
            dust: Dust::default(),
            pm_active: false,
            pm_slots: [-1, -1],
        }
    }
}

#[derive(Resource)]
pub struct Spray {
    riders: Vec<RiderFx>,
    m: Mem,
    vu: Vu,
    buckets: HashMap<(u32, u8), Handle<Mesh>>,
    textures: Vec<Option<Handle<Image>>>,
    dot: Handle<Image>,
    loaded: bool,
    was_menu: bool,
}

struct PmMem;
impl ParticleMemEnv for PmMem {
    fn bx_ctor(&mut self, _: &mut Mem, _: u32) {}
    fn bx_dtor(&mut self, _: &mut Mem, _: u32, _: u32) {}
    fn alloc(&mut self, _: &mut Mem, _: u32, _: u32, _: u32) -> u32 {
        PM_POOL
    }
    fn mem_set(&mut self, m: &mut Mem, ptr: u32, pat: u32, bytes: u32) {
        for o in (0..bytes).step_by(4) {
            m.w32(ptr + o, pat);
        }
    }
    fn free(&mut self, _: &mut Mem, _: u32) {}
}

fn empty_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    m.insert_indices(Indices::U32(vec![0, 1, 2]));
    m
}

fn g2b_mat() -> Mat4 {
    Mat4::from_cols(
        Vec4::new(0.01, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -0.01, 0.0),
        Vec4::new(0.0, 0.01, 0.0, 0.0),
        Vec4::W,
    )
}

fn game_pos(v: Vec3) -> V4 {
    [v.x * 100.0, -v.z * 100.0, v.y * 100.0, 1.0]
}

fn game_vel(v: Vec3) -> V4 {
    [v.x * 100.0, -v.z * 100.0, v.y * 100.0, 0.0]
}

fn game_dir(v: Vec3) -> V4 {
    [v.x, -v.z, v.y, 0.0]
}

fn game_to_bevy_pos(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0] * 0.01, p[2] * 0.01, -p[1] * 0.01)
}

fn wf(m: &mut Mem, a: u32, v: f32) {
    m.w32(a, v.to_bits());
}

fn rf(m: &Mem, a: u32) -> f32 {
    f32::from_bits(m.r32(a))
}

fn write_words(m: &mut Mem, base: u32, words: &[u32]) {
    for (i, &w) in words.iter().enumerate() {
        m.w32(base + 4 * i as u32, w);
    }
}

fn init_mem() -> Mem {
    let mut m = Mem::default();
    for (base, len) in [
        (APP, 4u32),
        (APP_OBJ, 0x800),
        (REND, 0x2000),
        (VT, 0x300),
        (TS, 0x40),
        (GAME, 0x300),
        (MAT, 0x60),
        (PM, 0x1000),
        (PM_POOL, particleman::POOL_BYTES + 0x100),
        (SCRATCH, 0x4000),
        (PKT, 0x2000),
    ] {
        m.regions.push((base, vec![0; len as usize]));
    }
    m.w32(APP, APP_OBJ);
    m.w32(APP_OBJ + 0x30, TEX_TABLE);
    for (i, &tex) in SPRITES.iter().enumerate() {
        m.w32(TEX_TABLE + 4 * i as u32, tex);
    }
    m.w32(APP_OBJ + 0x724, REND);
    m.w32(APP_OBJ + 0x730, GAME);
    m.w32(REND + 0x10, VT);
    m.w32(REND + 0x470, TS);
    m.w32(REND + 0x680, PKT);
    m.w32(REND + 0x17d0, 1);
    for off in [0x1c0u32, 0x1c8, 0x1e0, 0x1e8, 0x1f0, 0x1f8, 0x220, 0x240, 0x258, 0x280] {
        m.w32(VT + off + 4, off);
    }
    m.w32(GAME + 0x298, 1);
    m.w32(GAME + 0x1c, GAME + 0x100);
    particleman::init(&mut m, &mut PmMem, PM);
    m
}

fn init_vu() -> Vu {
    let mut vu = Vu::new();
    // GIF tags program 6's chain 0x31c260 loads at 47 and 533 (EOP, PRE, PRIM 0x56 sprite+tex+blend, 5 regs ST/RGBAQ/XYZ2/ST/XYZ2)
    for at in [47, 533] {
        vu.mem[at] = [0x8000, 0x502b_4000, 0x52512, 0];
    }
    vu
}

fn ensure_fx_region(m: &mut Mem, slot: usize) -> u32 {
    let base = FX_BASE + FX_STRIDE * slot as u32;
    if !m.regions.iter().any(|(b, _)| *b == base) {
        m.regions.push((base, vec![0; 0xd000]));
    }
    base
}

fn rider_motion(r: &Rider) -> u32 {
    if r.crashed > 0.0 {
        5
    } else if r.rail.is_some() {
        3
    } else if r.grounded {
        2
    } else {
        1
    }
}

struct BrakeUpdateEnv<'a> {
    splash: &'a mut splash::Splash,
    rider: SplashRider,
}

impl SprayEnv for BrakeUpdateEnv<'_> {
    fn rand_fx(&mut self) -> u32 {
        rand_fx()
    }
    fn splash(&mut self, a2: V4, a3: V4, a4: V4) {
        splash::emit_at_point(self.splash, &self.rider, a2, a3, a4, &mut rand_fx);
    }
}

struct PlainRandEnv;
impl SprayEnv for PlainRandEnv {
    fn rand_fx(&mut self) -> u32 {
        rand_fx()
    }
    fn splash(&mut self, _: V4, _: V4, _: V4) {}
}

/// Write `cBoarderRender::vf7` 0x1356f8's `0xe0`-byte `ParticleGroup_Add` config at `SCRATCH` and add the slot.
fn pm_add_tip(m: &mut Mem, tip: V4) -> i32 {
    for o in (0..particleman::CFG_BYTES).step_by(4) {
        m.w32(SCRATCH + o, 0);
    }
    m.w32(SCRATCH, 0x3ee4_1d84); // rate
    m.w32(SCRATCH + 0x0c, 0x3c); // life_base = 60 ticks
    m.w32(SCRATCH + 0x20, 0x3f80_0000); // start_col a
    m.w32(SCRATCH + 0x24, 0x3fa8_1d88); // start_col r
    m.w32(SCRATCH + 0x28, 0x3fa5_031b); // start_col g
    m.w32(SCRATCH + 0x2c, 0x3f85_57fc); // start_col b
    m.w32(SCRATCH + 0x4c, 0x3f9e_b5c5); // end_col b
    m.w32(SCRATCH + 0x60, 0x41b3_1770); // sz0
    m.w32(SCRATCH + 0x68, 0x4020_0887); // sz1
    m.w32(SCRATCH + 0x70, 0x3f7d_87f5); // drag
    wf(m, SCRATCH + 0x88, f32::from_bits(0xc251_4355) / 60.0); // gravity.z
    wf(m, SCRATCH + 0x8c, 1.0);
    m.w32(SCRATCH + 0x90, 0x50 / 4); // texture "str3" (gApp+0x30 + 0x50)
    for k in 0..4 {
        wf(m, SCRATCH + 0xa0 + 4 * k as u32, tip[k]);
    }
    wf(m, SCRATCH + 0xbc, 1.0);
    wf(m, SCRATCH + 0xcc, 1.0);
    wf(m, SCRATCH + 0xd0, 1.0);
    particleman::group_add(m, PM, SCRATCH)
}

/// STANDIN: approximate left/right hand trail points from the board bone frame (until the full rider
/// skeleton bones `0x2f0/0x330` and `0x1f0/0x230` run, row G5).
fn trail_tips(bone: &[V4; 4]) -> [V4; 2] {
    let mut tips = [[0.0f32; 4]; 2];
    for (i, sgn) in [-1.0f32, 1.0].into_iter().enumerate() {
        for k in 0..3 {
            tips[i][k] = bone[3][k] + bone[1][k] * 110.0 + bone[2][k] * (35.0 * sgn);
        }
        tips[i][3] = 1.0;
    }
    tips
}

impl Spray {
    fn reset_all(&mut self) {
        for fx in self.riders.iter_mut() {
            if fx.pm_active {
                for s in fx.pm_slots {
                    if s >= 0 {
                        particleman::group_remove(&mut self.m, PM, s);
                    }
                }
            }
            *fx = RiderFx::default();
        }
    }

    /// Run `Boarder_UpdateEffects` 0x1350c0 and `cBoarderRender::vf7` 0x1356f8's particle trail update for one
    /// rider tick, consuming any pending landing puffs (`Fx_SpawnLandingPuffs` 0x12e0e8) and impact bursts
    /// (`Fx_BrakeSpray_Emit` 0x12e008). Every random number comes from `Rand_Fx` 0x23d4a8.
    fn step_rider(&mut self, slot: usize, r: &mut Rider) {
        while self.riders.len() <= slot {
            self.riders.push(RiderFx::default());
        }
        let motion = rider_motion(r);
        let surf = (r.surface as usize).min(19);
        let bone = r.board_bone_frame();
        let frame = r.board_frame();
        let pos = game_pos(r.pos);
        let vel = game_vel(r.vel);
        let up = game_dir(r.normal);
        // STANDIN: ground point as rider pos on snow (matches `tracks.rs`)
        let ground = pos;
        let flipped = (r.switch != r.data.goofy) as u32;
        let f13c = (r.spin_timer > 0.0) as u32;
        let cfg58 = surf_word(surf, 22);
        let fx = &mut self.riders[slot];

        // Pending clean-landing puffs (`AirMotion_Update` -> `Boarder_SpawnLandingPuffs` 0x136e90 -> `Fx_SpawnLandingPuffs` 0x12e0e8)
        if let Some(vn) = r.land_puffs_vn.take() {
            let lin = LandingPuffsIn {
                mode: r.hit_kind,
                scale: cfg58,
                pos,
                vel,
                up,
                ground,
                fwd: bone[0],
                side: bone[2],
                colour: LIGHT_COLOUR,
            };
            brakefx::spawn_landing_puffs(&mut fx.brake, vn, &lin, &mut PlainRandEnv);
        }

        // Pending crash/impact brake spray (`cBoarder::EmitBrakeSpray` 0x11a798 -> `Fx_BrakeSpray_Emit` 0x12e008)
        if let Some((cp, cv, cn)) = r.crash_spray.take() {
            let mut env = BrakeUpdateEnv {
                splash: &mut fx.brake.splash,
                rider: SplashRider { mode: r.hit_kind, pos: vel, colour: LIGHT_COLOUR, scale: cfg58 },
            };
            brakespray::emit(
                &mut fx.brake.pool,
                r.hit_kind,
                LIGHT_COLOUR,
                game_pos(cp),
                game_vel(cv),
                game_dir(cn),
                &mut env,
            );
        }

        // `Boarder_UpdateEffects` 0x1350c0 in exact `effects::plan` order (Track runs in `tracks.rs`, draws no Rand_Fx)
        for (eff, _) in effects::plan(1, 0) {
            match eff {
                Effect::Puff => {
                    let pin = PuffIn {
                        hit: r.hit_kind,
                        motion,
                        surface: r.surface as u32,
                        turn: r.steer,
                        at: bone[3],
                        axis: frame[2],
                        colour: LIGHT_COLOUR,
                    };
                    puff::puff_update(&mut fx.puffs, &pin, &mut rand_fx);
                }
                Effect::Trail => {
                    let tin = TrailIn {
                        motion,
                        hit: r.hit_kind,
                        f1fc: r.brake_v,
                        f214: r.steer,
                        flipped,
                        w4a60: bone[0],
                        up,
                        vel,
                        ground,
                        colour: LIGHT_COLOUR,
                        cfg: [surf_word(surf, 22), surf_word(surf, 23), surf_word(surf, 24)],
                    };
                    boardtrail::update(&mut fx.trail, &tin, &mut rand_fx);
                }
                Effect::Spray => {
                    let mut cfg = [0u32; 9];
                    for k in 0..9 {
                        cfg[k] = surf_int(surf, 13 + k);
                    }
                    let sin = SprayIn {
                        vel,
                        f214: r.steer,
                        flipped,
                        surface: r.surface as u32,
                        hit: r.hit_kind,
                        ground,
                        motion,
                        f13c,
                        w4a60: bone[0],
                        w4a70: bone[1],
                        w4a80: bone[2],
                        w4a90: bone[3],
                        v180: frame[0],
                        v190: frame[1],
                        v1a0: frame[2],
                        colour: LIGHT_COLOUR,
                        cfg,
                    };
                    surfacespray::update(&mut fx.spray, &sin, &mut fx.light, &SPRITES, &mut rand_fx);
                }
                Effect::Brake => {
                    let fin = FanIn {
                        hit: r.hit_kind,
                        motion,
                        f1fc: r.brake_v,
                        cfg58,
                        vel,
                        up,
                        ground,
                        w4a60: bone[0],
                        colour: LIGHT_COLOUR,
                    };
                    brakefx::brake_update(&mut fx.brake, &fin, &mut PlainRandEnv);
                }
                Effect::Dust => {
                    let din = DustIn {
                        hit: r.hit_kind,
                        motion,
                        surface: r.surface as u32,
                        ground,
                        other: pos,
                        vel,
                        up,
                        colour: LIGHT_COLOUR,
                    };
                    puff::dust_update(&mut fx.dust, &din, &mut rand_fx);
                }
                Effect::Track => {}
            }
        }

        // `cBoarderRender::vf7` 0x1356f8: spin-boost glow trails via `cParticleMan`
        let tips = trail_tips(&bone);
        if f13c == 0 {
            if fx.pm_active {
                fx.pm_active = false;
                for s in fx.pm_slots.iter_mut() {
                    if *s >= 0 {
                        particleman::group_remove(&mut self.m, PM, *s);
                        *s = -1;
                    }
                }
            }
        } else {
            if !fx.pm_active {
                fx.pm_active = true;
                fx.pm_slots[0] = pm_add_tip(&mut self.m, tips[0]);
                fx.pm_slots[1] = pm_add_tip(&mut self.m, tips[1]);
            }
            let tick_vel = vel.map(|x| x * f32::from_bits(0x3c88_8889));
            for (i, &s) in fx.pm_slots.iter().enumerate() {
                if s >= 0 {
                    let cfg_ptr = particleman::group_get(&self.m, PM, s as u32);
                    if cfg_ptr != 0 {
                        wf(&mut self.m, cfg_ptr + 0xd0, 1.0);
                    }
                    for k in 0..4 {
                        wf(&mut self.m, SCRATCH + 4 * k as u32, tips[i][k]);
                        wf(&mut self.m, SCRATCH + 0x10 + 4 * k as u32, tick_vel[k]);
                    }
                    particleman::update_trail(&mut self.m, PM, s, SCRATCH, SCRATCH + 0x10, &mut rand_fx);
                }
            }
        }
    }

    fn sync_rider_to_mem(&mut self, slot: usize) -> u32 {
        let base = ensure_fx_region(&mut self.m, slot);
        let fx = &self.riders[slot];
        write_words(&mut self.m, base, &fx.puffs.w);
        write_words(&mut self.m, base + 0x1c20, &fx.trail.w);
        write_words(&mut self.m, base + 0x4b60, &fx.spray.w);
        let b = base + 0x99b0;
        for (i, e) in fx.brake.pool.entries.iter().enumerate() {
            write_words(&mut self.m, b + 0x50 * i as u32, e);
        }
        self.m.w32(b + 0x960, fx.brake.pool.count as u32);
        self.m.w32(b + 0x964, fx.brake.pool.head as u32);
        wf(&mut self.m, b + 0x968, fx.brake.pool.recent);
        wf(&mut self.m, b + 0x96c, fx.brake.pool.level);
        let sp = b + 0x970;
        for k in 0..4 {
            wf(&mut self.m, sp + 4 * k as u32, fx.brake.splash.last[k]);
        }
        wf(&mut self.m, sp + 0x10, fx.brake.splash.gathered);
        wf(&mut self.m, sp + 0x14, fx.brake.splash.timer);
        self.m.w32(sp + 0x18, fx.brake.splash.count as u32);
        self.m.w32(sp + 0x1c, fx.brake.splash.slot as u32);
        for (i, ring) in fx.brake.splash.rings.iter().enumerate() {
            write_words(&mut self.m, sp + 0x20 + 0x3c0 * i as u32, ring);
        }
        write_words(&mut self.m, base + 0xb600, &fx.dust.w);
        base
    }
}

#[derive(Default)]
struct BucketBuf {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

/// GS ALPHA mode (`drawlist.rs` `GsPkt_SetAlpha` 0x1c03e8): 5/8 = `0x48` additive (`Cs*As + Cd`),
/// 4 = `0x46` darken/multiply, 3 = `0x44` standard alpha blend.
fn blend_bucket(blend: u32) -> u8 {
    match blend {
        5 | 8 => 1, // AlphaMode::Add
        4 => 2,     // AlphaMode::Multiply
        _ => 0,     // AlphaMode::Blend
    }
}

struct DrawHost<'a> {
    vu: &'a mut Vu,
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    inv_view: Mat4,
    tex: u32,
    blend: u32,
    out: HashMap<(u32, u8), BucketBuf>,
}

impl DrawHost<'_> {
    fn bucket(&mut self) -> &mut BucketBuf {
        self.out.entry((self.tex, blend_bucket(self.blend))).or_default()
    }

    fn emit_sprite_240(&mut self, m: &Mem, args: &[u32]) {
        let p = [rf(m, args[0]), rf(m, args[0] + 4), rf(m, args[0] + 8)];
        let sx = rf(m, args[1]) * 0.01;
        let sy = rf(m, args[1] + 4) * 0.01;
        if sx <= 0.0 && sy <= 0.0 {
            return;
        }
        let u0 = rf(m, args[2]);
        let v0 = rf(m, args[2] + 4);
        let u1 = rf(m, args[3]);
        let v1 = rf(m, args[3] + 4);
        // `renderdraw::sprite` 0x1dd5c8: colour quadword is [alpha, r, g, b]
        let a = rf(m, args[4]).clamp(0.0, 1.0);
        if a <= 0.001 {
            return;
        }
        let col = [
            rf(m, args[4] + 4).clamp(0.0, 2.0),
            rf(m, args[4] + 8).clamp(0.0, 2.0),
            rf(m, args[4] + 12).clamp(0.0, 2.0),
            a,
        ];
        let ctr = game_to_bevy_pos(p) - self.eye;
        let rx = self.right * sx;
        let uy = self.up * sy;
        let buf = self.bucket();
        let base = buf.pos.len() as u32;
        for (pos, uv) in [
            (ctr - rx - uy, [u0, v1]),
            (ctr + rx - uy, [u1, v1]),
            (ctr + rx + uy, [u1, v0]),
            (ctr - rx + uy, [u0, v0]),
        ] {
            buf.pos.push(pos.to_array());
            buf.uv.push(uv);
            buf.col.push(col);
        }
        buf.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn emit_strip_220(&mut self, m: &Mem, args: &[u32]) {
        let count = args[0] as usize;
        if count < 3 {
            return;
        }
        let verts = args[1];
        let eye = self.eye;
        let buf = self.bucket();
        let base = buf.pos.len() as u32;
        for i in 0..count as u32 {
            let v = verts + 0x30 * i;
            let uv = [rf(m, v), rf(m, v + 4)];
            let col = [
                (m.r32(v + 0x10) as i32 as f32 / 128.0).clamp(0.0, 2.0),
                (m.r32(v + 0x14) as i32 as f32 / 128.0).clamp(0.0, 2.0),
                (m.r32(v + 0x18) as i32 as f32 / 128.0).clamp(0.0, 2.0),
                (m.r32(v + 0x1c) as i32 as f32 / 128.0).clamp(0.0, 1.0),
            ];
            let p = [rf(m, v + 0x20), rf(m, v + 0x24), rf(m, v + 0x28)];
            buf.pos.push((game_to_bevy_pos(p) - eye).to_array());
            buf.uv.push(uv);
            buf.col.push(col);
        }
        for i in 0..(count - 2) as u32 {
            buf.idx.extend_from_slice(&[base + i, base + i + 1, base + i + 2]);
        }
    }

    fn run_program6_packet(&mut self, m: &Mem, start: u32) {
        let block = start + 0x20;
        for q in 0..20u32 {
            self.vu.mem[(TOP + q) as usize] = [0, 1, 2, 3].map(|k| m.r32(block + 0x10 * q + 4 * k));
        }
        self.vu.top = TOP;
        self.vu.gs.clear();
        self.vu.kicks.clear();
        particlevu::run6(self.vu, 0);
        let inv = self.inv_view;
        let eye = self.eye;
        let back = |x: i32, y: i32, z: i32| {
            inv.project_point3(Vec3::new(x as f32 / 16.0 / VU_S, y as f32 / 16.0 / VU_S, z as f32 / 16.0 / VU_Z)) - eye
        };
        let mut sprites = Vec::new();
        for &(_, from) in &self.vu.kicks {
            let n = (self.vu.gs[from][0] & 0x7fff) as usize;
            for d in self.vu.gs[from + 1..].chunks(5).take(n) {
                let st0 = [f32::from_bits(d[0][0]), f32::from_bits(d[0][1])];
                let st1 = [f32::from_bits(d[3][0]), f32::from_bits(d[3][1])];
                sprites.push((st0, st1, d[1], d[2].map(|w| w as i32), d[4].map(|w| w as i32)));
            }
        }
        if sprites.is_empty() {
            return;
        }
        let buf = self.bucket();
        for (st0, st1, c, a, b) in sprites {
            let col = [
                (c[0] as f32 / 128.0).clamp(0.0, 2.0),
                (c[1] as f32 / 128.0).clamp(0.0, 2.0),
                (c[2] as f32 / 128.0).clamp(0.0, 2.0),
                (c[3] as f32 / 128.0).clamp(0.0, 1.0),
            ];
            let k = buf.pos.len() as u32;
            for (x, y, u, v) in [
                (a[0], a[1], st0[0], st0[1]),
                (b[0], a[1], st1[0], st0[1]),
                (b[0], b[1], st1[0], st1[1]),
                (a[0], b[1], st0[0], st1[1]),
            ] {
                buf.pos.push(back(x, y, a[2]).to_array());
                buf.uv.push([u, v]);
                buf.col.push(col);
            }
            buf.idx.extend_from_slice(&[k, k + 1, k + 2, k, k + 2, k + 3]);
        }
    }

    fn dispatch_vcall(&mut self, m: &mut Mem, func: u32, this: u32, args: &[u32]) -> u32 {
        match func {
            0x1e0 => {
                if let Some(&b) = args.first() {
                    self.blend = b;
                }
                0
            }
            0x1f0 => {
                if args.get(1).copied().unwrap_or(0) == 0 {
                    if let Some(&t) = args.first() {
                        self.tex = t;
                    }
                }
                0
            }
            0x220 => {
                self.emit_strip_220(m, args);
                0
            }
            0x240 => {
                self.emit_sprite_240(m, args);
                0
            }
            0x258 => {
                m.w32(REND + 0x680, PKT);
                particledraw::draw_program6(m, self, this, args[0]);
                0
            }
            0x280 => cull::box_test(m, MAT, args[0], args[1]),
            _ => 0,
        }
    }
}

impl CullEnv for DrawHost<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) {
        if func == particledraw::SUBMIT {
            self.run_program6_packet(m, args[1]);
        }
    }
}

impl ParticleDrawEnv for DrawHost<'_> {
    fn vcall(&mut self, m: &mut Mem, func: u32, this: u32, args: &[u32]) -> u32 {
        self.dispatch_vcall(m, func, this, args)
    }
}

impl RiderDrawEnv for DrawHost<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        match func {
            boardfx::PUFF_DRAW => boardfx::puff_draw(m, self, args[0], SCRATCH),
            boardfx::TRAIL_DRAW => boardfx::trail_draw(m, self, args[0], SCRATCH),
            boardfx::SURFACE_SPRAY_DRAW => fxdraw::surface_spray_draw(m, self, args[0]),
            boardfx::SPRAY_DRAW => boardfx::spray_draw(m, self, args[0], SCRATCH),
            boardfx::SPRAY_PARTICLES_DRAW => fxdraw::spray_draw_particles(m, self, args[0], SCRATCH),
            boardfx::SPEED_DUST_DRAW => fxdraw::speed_dust_draw(m, self, args[0], SCRATCH),
            // `Fx_BoardTrack_Draw` 0x1331a8 runs in `tracks.rs`
            boardfx::BOARD_TRACK_DRAW => {}
            particleman::DRAW_SLOT => {
                particleman::draw_slot(m, self, args[0], args[1] as i32, SCRATCH);
            }
            particledraw::SUBMIT => self.run_program6_packet(m, args[1]),
            _ => {}
        }
        0
    }

    fn vcall(&mut self, m: &mut Mem, func: u32, this: u32, args: &[u32]) -> u32 {
        self.dispatch_vcall(m, func, this, args)
    }
}

pub fn setup_spray(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Fallback soft round sprite when `chars/particles/<name>.png` is not extracted.
    let n = 32u32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = ((x as f32 + 0.5) / n as f32 * 2.0 - 1.0, (y as f32 + 0.5) / n as f32 * 2.0 - 1.0);
            let a = (1.0 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0).powf(1.5);
            px.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    let dot = images.add(Image::new(
        bevy::render::render_resource::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        px,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    commands.insert_resource(Spray {
        riders: Vec::new(),
        m: init_mem(),
        vu: init_vu(),
        buckets: HashMap::new(),
        textures: Vec::new(),
        dot,
        loaded: false,
        was_menu: true,
    });
}

#[allow(clippy::too_many_arguments)]
pub fn spray(
    tick: Res<GameTick>,
    mut rider: ResMut<RiderRes>,
    mut opponents: ResMut<Opponents>,
    game: Res<ui::Game>,
    mode: Res<Mode>,
    mut spray: ResMut<Spray>,
    lib: Res<CharLib>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut holders: Query<&mut Transform, With<SprayMesh>>,
) {
    if game.screen == ui::Screen::Paused {
        return;
    }
    if !spray.loaded {
        spray.loaded = true;
        // STANDIN: our tools' PNG export of `data/textures/particle.ssh` (chars/particles), else a soft dot,
        // for the game's own `.ssh` reader.
        let dir = lib.dir.join("particles");
        spray.textures = crate::particles::TEX_NAMES
            .iter()
            .map(|n| {
                let img = image::open(dir.join(format!("{n}.png"))).ok()?.to_rgba8();
                let (w, h) = img.dimensions();
                Some(images.add(Image::new(
                    bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    bevy::render::render_resource::TextureDimension::D2,
                    img.into_raw(),
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::default(),
                )))
            })
            .collect();
    }

    let in_menu = game.screen == ui::Screen::Menu;
    if in_menu && !spray.was_menu {
        spray.reset_all();
    }
    spray.was_menu = in_menu;

    let spray_test = std::env::var("TRICKY_SPRAYTEST").is_ok();
    // Run the game's 60 Hz ticks (`GameTick`, row E1a2).
    for _ in 0..tick.n {
        if *mode == Mode::Ride && !in_menu {
            spray.step_rider(0, &mut rider.0);
            for (i, o) in opponents.0.iter_mut().enumerate() {
                spray.step_rider(i + 1, &mut o.rider);
            }
        }
        if spray_test {
            // A rider carving hard on the spot, to inspect the ported board effects.
            let mut r = rider.0.clone();
            r.grounded = true;
            r.crashed = 0.0;
            r.rail = None;
            r.normal = Vec3::Y;
            r.vel = crate::rider::heading(r.yaw) * 14.0;
            r.steer = 1.0;
            r.brake_v = 0.0;
            r.surface = 1;
            spray.step_rider(0, &mut r);
        }
    }

    let Ok((camera, gt)) = cam.single() else { return };
    let view = camera.clip_from_view() * gt.compute_matrix().inverse();
    let clip = view * g2b_mat();
    let eye = gt.translation();
    let right = gt.right().as_vec3();
    let up = gt.up().as_vec3();
    for mut tf in holders.iter_mut() {
        tf.translation = eye;
    }

    let s = &mut *spray;
    for (k, c) in clip.to_cols_array_2d().iter().enumerate() {
        for (i, &x) in c.iter().enumerate() {
            wf(&mut s.m, MAT + 0x10 * k as u32 + 4 * i as u32, x);
        }
        s.vu.mem[k] = c.map(f32::to_bits);
    }
    let p = camera.clip_from_view();
    s.vu.mem[4] = [VU_S, VU_S, VU_Z, 0.0].map(f32::to_bits);
    s.vu.mem[5] = [0.0f32; 4].map(f32::to_bits);
    s.vu.mem[6] = [p.x_axis.x * 0.01, p.y_axis.y * 0.01, 0.0, 0.0].map(f32::to_bits);

    let count = s.riders.len();
    let mut addrs = Vec::with_capacity(count);
    for i in 0..count {
        addrs.push(s.sync_rider_to_mem(i));
    }

    let mut host = DrawHost {
        vu: &mut s.vu,
        eye,
        right,
        up,
        inv_view: view.inverse(),
        tex: 0,
        blend: 3,
        out: HashMap::new(),
    };
    for addr in addrs {
        boardfx::draw_effects(&mut s.m, &mut host, addr);
    }
    particleman::draw(&mut s.m, &mut host, PM);

    if std::env::var("TRICKY_SPRAYDBG").is_ok() {
        if let Some(fx) = s.riders.first() {
            let tris: usize = host.out.values().map(|b| b.idx.len() / 3).sum();
            println!(
                "spray: puffs {} trail {} surface_emitters {} brake_pool {} splash_rings {} dust {} pm_used {} | buckets {} tris {}",
                fx.puffs.w[0x1c10 / 4],
                fx.trail.w[0x50 / 4],
                fx.spray.w[0],
                fx.brake.pool.count,
                fx.brake.splash.count,
                fx.dust.w[0xd50 / 4],
                s.m.r32(PM + 0xb5c),
                host.out.len(),
                tris,
            );
        }
    }

    for &key in host.out.keys() {
        if s.buckets.contains_key(&key) {
            continue;
        }
        let tex = s.textures.get(key.0 as usize).cloned().flatten().unwrap_or_else(|| s.dot.clone());
        let alpha_mode = match key.1 {
            1 => AlphaMode::Add,
            2 => AlphaMode::Multiply,
            _ => AlphaMode::Blend,
        };
        let h = meshes.add(empty_mesh());
        commands.spawn((
            Mesh3d(h.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(tex),
                unlit: true,
                alpha_mode,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_translation(eye),
            bevy::render::view::NoFrustumCulling,
            SprayMesh,
        ));
        s.buckets.insert(key, h);
    }

    let keys: Vec<((u32, u8), Handle<Mesh>)> = s.buckets.iter().map(|(&k, h)| (k, h.clone())).collect();
    for (key, h) in keys {
        let Some(m) = meshes.get_mut(&h) else { continue };
        let Some(buf) = host.out.remove(&key) else {
            *m = empty_mesh();
            continue;
        };
        if buf.idx.is_empty() {
            *m = empty_mesh();
            continue;
        }
        let n = buf.pos.len();
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, buf.pos);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, buf.uv);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, buf.col);
        m.insert_indices(Indices::U32(buf.idx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_spray() -> Spray {
        Spray {
            riders: Vec::new(),
            m: init_mem(),
            vu: init_vu(),
            buckets: HashMap::new(),
            textures: Vec::new(),
            dot: Handle::default(),
            loaded: true,
            was_menu: false,
        }
    }

    #[test]
    fn board_effects_step_and_draw_through_vu1_program6_and_rand_fx() {
        let run_once = || {
            tricky_game::rand::streams().seed_fx(0x1350_c001);
            let mut sp = test_spray();
            let mut r = Rider::new(Vec3::new(0.0, 0.0, 0.0), 0.0);
            r.grounded = true;
            r.normal = Vec3::Y;
            r.vel = Vec3::new(0.0, 0.0, -18.0);
            r.steer = 0.9;
            r.surface = 3; // powder: puffs + trail + surface spray + speed dust
            r.spin_timer = 5.0; // activates cParticleMan trails
            r.land_puffs_vn = Some(-950.0); // triggers Fx_SpawnLandingPuffs -> Fx_LandingSplash + brake pool
            r.crash_spray = Some((Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -10.0), Vec3::Y));

            for t in 0..20 {
                r.pos.z -= 0.3;
                if t == 10 {
                    r.brake_v = 1.0; // triggers Fx_BrakeFan_Update + brake spray
                }
                if t == 15 {
                    r.surface = 13; // metal sparks
                    r.rail = Some((0, 0.0, 0.0));
                }
                sp.step_rider(0, &mut r);
            }
            (sp, r, tricky_game::rand::streams().fx)
        };

        let (mut sp, mut r, fx_state_a) = run_once();
        let (_, _, fx_state_b) = run_once();
        assert_eq!(fx_state_a, fx_state_b, "board effects must draw deterministically from Rand_Fx");
        assert_ne!(fx_state_a, tricky_game::rand::Rand::seeded(0x1350_c001), "board effects must advance Rand_Fx");

        let fx = &sp.riders[0];
        assert!(fx.puffs.w[0x1c10 / 4] > 0, "board puffs should spawn on powder");
        assert!(fx.trail.w[0x50 / 4] > 0, "board trail segments should spawn while carving");
        assert!(fx.spray.w[0] > 0, "surface spray emitters should spawn");
        assert!(fx.brake.pool.count > 0, "brake/landing spray pool should have particles");
        assert!(fx.brake.splash.count > 0, "landing/brake splash rings should be active");
        assert!(fx.dust.w[0xd50 / 4] > 0, "speed dust should spawn on powder");
        assert!(sp.m.r32(PM + 0xb5c) > 0, "cParticleMan trail particles should spawn while spin_timer > 0");

        // Render from a camera looking at the rider at (0, 0, -6)
        let eye = Vec3::new(4.0, 3.0, 2.0);
        let gt = Transform::from_translation(eye).looking_at(r.pos, Vec3::Y);
        let proj = Mat4::perspective_rh(60f32.to_radians(), 4.0 / 3.0, 0.2, 500.0);
        let view = proj * gt.compute_matrix().inverse();
        let clip = view * g2b_mat();
        for (k, c) in clip.to_cols_array_2d().iter().enumerate() {
            for (i, &x) in c.iter().enumerate() {
                wf(&mut sp.m, MAT + 0x10 * k as u32 + 4 * i as u32, x);
            }
            sp.vu.mem[k] = c.map(f32::to_bits);
        }
        sp.vu.mem[4] = [VU_S, VU_S, VU_Z, 0.0].map(f32::to_bits);
        sp.vu.mem[5] = [0.0f32; 4].map(f32::to_bits);
        sp.vu.mem[6] = [proj.x_axis.x * 0.01, proj.y_axis.y * 0.01, 0.0, 0.0].map(f32::to_bits);

        let addr = sp.sync_rider_to_mem(0);
        let mut host = DrawHost {
            vu: &mut sp.vu,
            eye,
            right: gt.right().as_vec3(),
            up: gt.up().as_vec3(),
            inv_view: view.inverse(),
            tex: 0,
            blend: 3,
            out: HashMap::new(),
        };
        boardfx::draw_effects(&mut sp.m, &mut host, addr);
        particleman::draw(&mut sp.m, &mut host, PM);

        let total_tris: usize = host.out.values().map(|b| b.idx.len() / 3).sum();
        assert!(total_tris > 100, "expected rendered triangles from VU1 program 6 + strips + sprites, got {total_tris}");
        assert!(host.out.len() >= 3, "expected multiple texture/blend buckets, got {}", host.out.len());

        // Turn off spin_timer -> cParticleMan removes both slots cleanly
        r.spin_timer = 0.0;
        sp.step_rider(0, &mut r);
        assert!(!sp.riders[0].pm_active);
        assert_eq!(sp.m.r32(PM + 0xc), 0, "all cParticleMan slots should be released");
        assert_eq!(sp.m.r32(PM + 0xb5c), 0, "all cParticleMan particles should return to free pool");
    }
}
