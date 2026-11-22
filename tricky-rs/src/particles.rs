//! Level particle emitters on the game's own ported code (board row E1b2g4): every level emitter
//! (`cEmitterNode` 0x147cf8, `cParticleNode` 0x147f88, `CollideEmitterNode_CreateParticles` 0x148748 — all five
//! callers in `SLUS_203.26`) builds a 0x170-byte particle system via `ParticleSys_Create` 0x1d8490
//! (`tricky_game::emitter::create`), drawing its nine random factors from `Rand_Fx` 0x23d4a8
//! (`ParticleSys_RandomizeFactors` 0x1d5180 at 0x1d85c4) and computing its frustum AABB via
//! `ParticleSys_CalcBounds` 0x1d5b60 (`emitter::calc_bounds`).
//!
//! Each 60 Hz tick runs `ParticleNode_Update` 0x1d8b20 (`emitter::node_update`), which steps continuous emitters
//! through `ParticleSys_StepEmitter` 0x1d5a30 (advancing their nine VU random factors with `Rand_VuNext` 0x1d5a08)
//! and one-shot bursts through `ParticleSys_AdvanceTime` 0x1d59f0 until their lifetime reaches 0. Each frame draws
//! live systems through `ParticleSys_Draw` 0x1d8bc8 (`particledraw::particle_sys_draw` -> `cull::box_test` ->
//! `particledraw::draw_program6` 0x1e2a60 -> VU1 program 6 `particlevu::run6`).

use crate::logic::EmitterDef;
use crate::{ui, GameTick, LevelRes, Opponents, RiderRes};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use std::collections::HashMap;
use tricky_game::cull::{self, CullEnv};
use tricky_game::emitter::{self, SYS_WORDS};
use tricky_game::particledraw::{self, ParticleDrawEnv, APP, BLENDS, SUBMIT};
use tricky_game::particlevu;
use tricky_game::pf::Mem;
use tricky_game::rand::rand_fx;
use tricky_game::vu::Vu;

// Emulated EE memory layout for `particledraw::particle_sys_draw` and `draw_program6`.
const APP_OBJ: u32 = 0x0100_0000;
const TEX_TABLE: u32 = APP_OBJ + 0x400;
const REND: u32 = 0x0101_0000;
const VT: u32 = 0x0102_0000;
const TS: u32 = 0x0102_0400;
const MAT: u32 = 0x0103_0000;
const SYS_BUF: u32 = 0x0104_0000;
const PKT: u32 = 0x0105_0000;

/// Screen scale for VU1 program 6 (ndc to ftoi4 integers, invertible, as in `snow.rs` and `spray.rs`).
const VU_S: f32 = 4096.0;
const VU_Z: f32 = 1_048_576.0;
/// Program 6 block place in VU1 memory (20 quadwords 7..27, before packet buffer 0 at 47).
const TOP: u32 = 7;

/// `particledraw::BLENDS` (0x392cf8) blend lookup table from `SLUS_203.26` (U50 kind -> GS blend mode).
const BLEND_TABLE: [u32; 11] = [5, 3, 2, 1, 4, 0, 6, 7, 0, 0, 0];

/// One active level emitter (`ParticleSys`, 0x170 bytes = 92 words).
struct Node {
    sys: [u32; SYS_WORDS],
    origin: Vec3,
    one_shot: bool,
}

#[derive(Resource)]
pub struct Particles {
    nodes: Vec<Node>,
    /// trigger objects touched (for their script sounds; drained by `worldsound`)
    pub fired: Vec<usize>,
    key: Option<std::path::PathBuf>,
    cool: HashMap<usize, f32>,
    m: Mem,
    vu: Vu,
    /// one mesh per (texture, GS alpha bucket): the game's particle sprites when found, else a soft dot
    buckets: HashMap<(u32, u8), Handle<Mesh>>,
    textures: Vec<Option<Handle<Image>>>,
    dot: Handle<Image>,
    loaded: bool,
}

/// The particle sheet's names in the game's order (table 0x33edf0), as `chars/particles/<name>.png`.
pub(crate) const TEX_NAMES: [&str; 38] = [
    "part", "snfl", "clod", "spry", "halo", "brk1", "brk2", "brk3", "ndl1", "ndl2", "swd1", "swd2",
    "swp1", "swp2", "cnf1", "cnf2", "blb1", "blb2", "str1", "str2", "str3", "nois", "strk", "tral",
    "ex06", "ex07", "ex08", "ex09", "lens", "blnk", "mip1", "mip1", "mip2", "beam", "fog0", "spec",
    "envr", "exlm",
];

#[derive(Component)]
pub struct ParticleMesh;

fn empty() -> Mesh {
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

fn g2b(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, -v.y) * 0.01
}

fn wf(m: &mut Mem, a: u32, v: f32) {
    m.w32(a, v.to_bits());
}

fn init_mem() -> Mem {
    let mut m = Mem::default();
    for (base, len) in [
        (APP, 4u32),
        (BLENDS, 44),
        (APP_OBJ, 0x800),
        (REND, 0x2000),
        (VT, 0x300),
        (TS, 0x40),
        (MAT, 0x60),
        (SYS_BUF, 0x200),
        (PKT, 0x1000),
    ] {
        m.regions.push((base, vec![0; len as usize]));
    }
    m.w32(APP, APP_OBJ);
    for (i, &b) in BLEND_TABLE.iter().enumerate() {
        m.w32(BLENDS + 4 * i as u32, b);
    }
    m.w32(APP_OBJ + 0x30, TEX_TABLE);
    for i in 0..TEX_NAMES.len() as u32 {
        m.w32(TEX_TABLE + 4 * i, i);
    }
    m.w32(APP_OBJ + 0x724, REND);
    m.w32(REND + 0x10, VT);
    m.w32(REND + 0x470, TS);
    m.w32(REND + 0x680, PKT);
    m.w32(REND + 0x17d0, 1);
    for off in [0x1c0u32, 0x1c8, 0x1e0, 0x1e8, 0x1f0, 0x258, 0x280] {
        m.w32(VT + off + 4, off);
    }
    m
}

fn init_vu() -> Vu {
    let mut vu = Vu::new();
    // GIF tags program 6's chain 0x31c260 loads at 47 and 533 (EOP, PRE, PRIM 0x56 sprite+tex+blend, 5 regs)
    for at in [47, 533] {
        vu.mem[at] = [0x8000, 0x502b_4000, 0x52512, 0];
    }
    vu
}

fn inst_matrix(inst: &crate::level::Instance) -> Mat4 {
    Mat4::from_scale_rotation_translation(inst.scale.into(), crate::level::quat4(inst.rotation), inst.location.into())
}

/// Pack `EmitterDef` back into the 51-word `type2Sub0` record layout expected by `ParticleSys_Create` 0x1d8490.
fn def_to_record(def: &EmitterDef) -> [u32; 51] {
    let mut r = [0u32; 51];
    r[0] = def.n;
    r[1] = def.trail;
    r[2] = def.dur.to_bits();
    r[3] = def.k.to_bits();
    r[4] = def.size.to_bits();
    r[5] = def.life.to_bits();
    r[6] = def.size_sp.to_bits();
    r[7] = def.life_sp.to_bits();
    r[8] = def.trail_dt.to_bits();
    for k in 0..3 {
        r[9 + k] = def.origin[k].to_bits();
        r[12 + k] = def.area_a[k].to_bits();
        r[15 + k] = def.area_b[k].to_bits();
        r[18 + k] = def.vel[k].to_bits();
        r[21 + k] = def.vs[0][k].to_bits();
        r[24 + k] = def.vs[1][k].to_bits();
        r[27 + k] = def.vs[2][k].to_bits();
        r[30 + k] = def.accel[k].to_bits();
    }
    // `EmitterDef::c[i]` stores `[r, g, b, a]`; the game's record stores `[a, r, g, b]` at U33..U48.
    for i in 0..4 {
        let base = 33 + 4 * i;
        r[base] = def.c[i][3].to_bits();
        r[base + 1] = def.c[i][0].to_bits();
        r[base + 2] = def.c[i][1].to_bits();
        r[base + 3] = def.c[i][2].to_bits();
    }
    r[49] = def.texture;
    r[50] = def.blend;
    r
}

fn create_node(def: &EmitterDef, m: Mat4) -> Node {
    let mut sys = [0u32; SYS_WORDS];
    let rec = def_to_record(def);
    let mat = m.to_cols_array_2d();
    emitter::create(&mut sys, &mat, &rec, &mut rand_fx);
    Node { sys, origin: g2b(m.w_axis.truncate()), one_shot: def.dur >= 0.0 }
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

#[derive(Default)]
struct BucketBuf {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

struct DrawHost<'a> {
    vu: &'a mut Vu,
    eye: Vec3,
    inv_view: Mat4,
    tex: u32,
    blend: u32,
    has_tex: &'a [bool],
    out: HashMap<(u32, u8), BucketBuf>,
}

impl DrawHost<'_> {
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
        let tex_key = if self.has_tex.get(self.tex as usize).copied().unwrap_or(false) { self.tex } else { u32::MAX };
        let bucket_mode = blend_bucket(self.blend);
        let buf = self.out.entry((tex_key, bucket_mode)).or_default();
        for (st0, st1, c, a, b) in sprites {
            let col = if bucket_mode == 2 {
                let k = 1.0 - (c[3] as f32 / 128.0).clamp(0.0, 1.0);
                [k, k, k, 1.0]
            } else {
                [
                    (c[0] as f32 / 128.0).clamp(0.0, 2.0),
                    (c[1] as f32 / 128.0).clamp(0.0, 2.0),
                    (c[2] as f32 / 128.0).clamp(0.0, 2.0),
                    (c[3] as f32 / 128.0).clamp(0.0, 1.0),
                ]
            };
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
}

impl CullEnv for DrawHost<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) {
        if func == SUBMIT {
            self.run_program6_packet(m, args[1]);
        }
    }
}

impl ParticleDrawEnv for DrawHost<'_> {
    fn vcall(&mut self, m: &mut Mem, func: u32, this: u32, args: &[u32]) -> u32 {
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

pub fn setup_particles(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
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
    let img = images.add(Image::new(
        bevy::render::render_resource::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        px,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    commands.insert_resource(Particles {
        nodes: Vec::new(),
        fired: Vec::new(),
        key: None,
        cool: HashMap::new(),
        m: init_mem(),
        vu: init_vu(),
        buckets: HashMap::new(),
        textures: Vec::new(),
        dot: img,
        loaded: false,
    });
}

#[allow(clippy::too_many_arguments)]
pub fn particles(
    tick: Res<GameTick>,
    game: Res<ui::Game>,
    level: Res<LevelRes>,
    world: Res<crate::World>,
    rider: Res<RiderRes>,
    opponents: Res<Opponents>,
    mut ps: ResMut<Particles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
    lib: Res<crate::CharLib>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut holders: Query<&mut Transform, With<ParticleMesh>>,
) {
    let level = &level.0;
    // STANDIN: our tools' PNG export of `data/textures/particle.ssh` (chars/particles), else a soft dot,
    // for the game's own `.ssh` reader.
    if !ps.loaded {
        ps.loaded = true;
        let dir = lib.dir.join("particles");
        ps.textures = TEX_NAMES
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
        println!("particle sprites: {} of {}", ps.textures.iter().flatten().count(), TEX_NAMES.len());
    }

    // A new level: create its wake-up emitters via `ParticleSys_Create` 0x1d8490 (9 `Rand_Fx` draws each).
    if ps.key.as_ref() != Some(&level.dir) {
        ps.key = Some(level.dir.clone());
        ps.cool.clear();
        ps.nodes = level
            .logic
            .emit_wake
            .iter()
            .filter_map(|(i, def)| {
                let inst = level.instances.get(*i)?;
                Some(create_node(def, inst_matrix(inst)))
            })
            .collect();
    }

    // Step the frame's 60 Hz game ticks (`GameTick`, row E1a2).
    let paused = game.screen == ui::Screen::Paused;
    if !paused {
        let dt = tick.dt();
        for v in ps.cool.values_mut() {
            *v -= dt;
        }
        let bodies: Vec<Vec3> = std::iter::once(rider.0.pos)
            .chain(opponents.0.iter().map(|o| o.rider.pos))
            .map(|p| p + Vec3::Y * 0.8)
            .collect();
        let mut fire: Vec<usize> = Vec::new();
        for (&inst, &(lo, hi)) in &world.0.emit_boxes {
            if ps.cool.get(&inst).is_some_and(|c| *c > 0.0) {
                continue;
            }
            if bodies.iter().any(|b| b.cmpge(lo - 0.3).all() && b.cmple(hi + 0.3).all()) {
                fire.push(inst);
            }
        }
        for inst in fire {
            ps.cool.insert(inst, 1.0);
            ps.fired.push(inst);
            if ps.fired.len() > 64 {
                ps.fired.remove(0);
            }
            let Some(list) = level.logic.emit_touch.get(&inst) else { continue };
            for (owner, def) in list.clone() {
                let Some(oi) = level.instances.get(owner) else { continue };
                ps.nodes.push(create_node(&def, inst_matrix(oi)));
            }
        }
        for _ in 0..tick.n {
            for n in ps.nodes.iter_mut() {
                emitter::node_update(&mut n.sys);
            }
        }
        ps.nodes.retain(|n| !n.one_shot || f32::from_bits(n.sys[0]) != 0.0);
    }

    let Ok((camera, gt)) = cam.single() else { return };
    let view = camera.clip_from_view() * gt.compute_matrix().inverse();
    let clip = view * g2b_mat();
    let eye = gt.translation();
    for mut tf in holders.iter_mut() {
        tf.translation = eye;
    }

    let ps = &mut *ps;
    for (k, c) in clip.to_cols_array_2d().iter().enumerate() {
        for (i, &x) in c.iter().enumerate() {
            wf(&mut ps.m, MAT + 0x10 * k as u32 + 4 * i as u32, x);
        }
        ps.vu.mem[k] = c.map(f32::to_bits);
    }
    let p = camera.clip_from_view();
    ps.vu.mem[4] = [VU_S, VU_S, VU_Z, 0.0].map(f32::to_bits);
    ps.vu.mem[5] = [0.0f32; 4].map(f32::to_bits);
    ps.vu.mem[6] = [p.x_axis.x * 0.01, p.y_axis.y * 0.01, 0.0, 0.0].map(f32::to_bits);

    let has_tex: Vec<bool> = ps.textures.iter().map(|t| t.is_some()).collect();
    let mut host = DrawHost {
        vu: &mut ps.vu,
        eye,
        inv_view: view.inverse(),
        tex: 0,
        blend: 3,
        has_tex: &has_tex,
        out: HashMap::new(),
    };
    for n in &ps.nodes {
        if n.origin.distance(eye) > 250.0 || n.sys[4] == 0 {
            continue;
        }
        for (i, &w) in n.sys.iter().enumerate() {
            ps.m.w32(SYS_BUF + 4 * i as u32, w);
        }
        particledraw::particle_sys_draw(&mut ps.m, &mut host, SYS_BUF);
    }

    if std::env::var("TRICKY_PARTDBG").is_ok() {
        eprintln!(
            "particles: nodes {} buckets {} quads {}",
            ps.nodes.len(),
            host.out.len(),
            host.out.values().map(|b| b.idx.len() / 6).sum::<usize>()
        );
    }

    for &key in host.out.keys() {
        if ps.buckets.contains_key(&key) {
            continue;
        }
        let tex = ps.textures.get(key.0 as usize).cloned().flatten().unwrap_or_else(|| ps.dot.clone());
        let mode = match key.1 {
            1 => AlphaMode::Add,
            2 => AlphaMode::Multiply,
            _ => AlphaMode::Blend,
        };
        let h = meshes.add(empty());
        commands.spawn((
            Mesh3d(h.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(tex),
                unlit: true,
                alpha_mode: mode,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_translation(eye),
            bevy::render::view::NoFrustumCulling,
            ParticleMesh,
        ));
        ps.buckets.insert(key, h);
    }

    let buckets: Vec<((u32, u8), Handle<Mesh>)> = ps.buckets.iter().map(|(&k, h)| (k, h.clone())).collect();
    for (key, h) in buckets {
        let Some(m) = meshes.get_mut(&h) else { continue };
        let Some(buf) = host.out.remove(&key) else {
            *m = empty();
            continue;
        };
        if buf.idx.is_empty() {
            *m = empty();
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

    fn sample_def(dur: f32) -> EmitterDef {
        EmitterDef {
            n: 16,
            trail: 2,
            dur,
            k: 2.0,
            size: 20.0,
            size_sp: 6.0,
            life: 0.6,
            life_sp: 0.2,
            trail_dt: 0.03,
            origin: [0.0, 0.0, 0.0],
            area_a: [40.0, 0.0, 0.0],
            area_b: [0.0, 40.0, 0.0],
            vel: [0.0, 0.0, 250.0],
            vs: [[60.0, 0.0, 0.0], [0.0, 60.0, 0.0], [0.0, 0.0, 40.0]],
            accel: [0.0, 0.0, -100.0],
            c: [[1.0, 0.9, 0.8, 0.8], [0.8, 0.8, 1.0, 0.0], [0.1, 0.1, 0.1, 0.0], [0.0; 4]],
            texture: 1,
            blend: 0,
        }
    }

    #[test]
    fn level_emitters_draw_through_particle_sys_draw_vu1_program6_and_rand_fx() {
        let build = || {
            tricky_game::rand::streams().seed_fx(0x1d84_9001);
            let cont = create_node(&sample_def(-1.0), Mat4::IDENTITY);
            let burst = create_node(&sample_def(0.2), Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0)));
            (cont, burst, tricky_game::rand::streams().fx)
        };
        let (mut cont, mut burst, fx_a) = build();
        let (_, _, fx_b) = build();
        assert_eq!(fx_a, fx_b, "level emitters must draw deterministically from Rand_Fx");
        assert_ne!(fx_a, tricky_game::rand::Rand::seeded(0x1d84_9001), "ParticleSys_Create must draw from Rand_Fx");

        // Stepping continuous emitter advances its VU random factors via emitter::step (`Rand_VuNext`)
        let factor0_before = cont.sys[4 + 0x28 / 4];
        for _ in 0..12 {
            emitter::node_update(&mut cont.sys);
            emitter::node_update(&mut burst.sys);
        }
        assert_ne!(cont.sys[4 + 0x28 / 4], factor0_before, "continuous emitter must step VU random factors");
        assert!(f32::from_bits(burst.sys[0]) > 0.0, "one-shot burst should still be alive after 12 ticks");

        let mut mem = init_mem();
        let mut vu = init_vu();
        let eye = Vec3::new(0.0, 3.0, 6.0);
        let gt = Transform::from_translation(eye).looking_at(Vec3::new(0.5, 1.0, 0.0), Vec3::Y);
        let proj = Mat4::perspective_rh(60f32.to_radians(), 4.0 / 3.0, 0.2, 500.0);
        let view = proj * gt.compute_matrix().inverse();
        let clip = view * g2b_mat();
        for (k, c) in clip.to_cols_array_2d().iter().enumerate() {
            for (i, &x) in c.iter().enumerate() {
                wf(&mut mem, MAT + 0x10 * k as u32 + 4 * i as u32, x);
            }
            vu.mem[k] = c.map(f32::to_bits);
        }
        vu.mem[4] = [VU_S, VU_S, VU_Z, 0.0].map(f32::to_bits);
        vu.mem[5] = [0.0f32; 4].map(f32::to_bits);
        vu.mem[6] = [proj.x_axis.x * 0.01, proj.y_axis.y * 0.01, 0.0, 0.0].map(f32::to_bits);

        let has_tex = vec![true; TEX_NAMES.len()];
        let mut host = DrawHost {
            vu: &mut vu,
            eye,
            inv_view: view.inverse(),
            tex: 0,
            blend: 3,
            has_tex: &has_tex,
            out: HashMap::new(),
        };
        for node in [&cont, &burst] {
            for (i, &w) in node.sys.iter().enumerate() {
                mem.w32(SYS_BUF + 4 * i as u32, w);
            }
            particledraw::particle_sys_draw(&mut mem, &mut host, SYS_BUF);
        }
        let quads: usize = host.out.values().map(|b| b.idx.len() / 6).sum();
        assert!(quads >= 10, "expected VU1 program 6 quads from ParticleSys_Draw, got {quads}");
        // U50 blend 0 maps through BLEND_TABLE[0] = 5 -> GS 0x48 additive bucket (1)
        assert!(host.out.contains_key(&(1, 1)), "expected texture 1 in additive bucket 1");

        // Stepping one-shot burst past dur + max_life (0.2 + 0.7 = 0.9s = 54 ticks) reaches lifetime 0
        for _ in 0..50 {
            emitter::node_update(&mut burst.sys);
        }
        assert_eq!(f32::from_bits(burst.sys[0]), 0.0, "one-shot burst must finish at lifetime 0");
    }
}
