//! The app's exact chase camera on the ported game code (board row E11d): replaces `race.rs`'s `ChaseCam` stand-in
//! with `cBxCamera::vf1` 0x177078, `Camera_TrackRider` 0x173270, `CamScript_CutTo` 0x174c98 / `Camera_GotoNamed`
//! 0x174888, `CamCamera_EvalPosition` 0x16e900, `Camera_CollideWorld` 0x16fe60 / `Camera_CollidePush` 0x16fb08,
//! `CamCamera_EvalLookAt` 0x1722e8, `Camera_SmoothOutput` 0x174238, and `Game_UpdateCameras` 0x17f390, driven by the
//! named camera parameter blocks read directly from `COMMONOB.CML` (and the course's `.cml`) and the 26-slot rider
//! list built from our riders.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Quat, Vec3};

use crate::camera::{
    self, CamEnv, CamOut, CamP, Entry, LastDir, Line, LookEnv, LookEntry, LookP, LookRider, LookView, RayRes, Shake,
    SmoothEnv, StageOut, TargetRider, TrackRider, Tracker, WorldEnv,
};
use crate::camera_update::{self, CamEntry, EntryEnv};
use crate::camscript::{self, CutEnv, ScriptEnv};
use crate::collide::CollisionWorld;
use crate::orient;
use crate::ps2f::F;
use crate::rider::{ground_tick, placed_heading, Rider};
use crate::worldline::WorldLine;

/// The game's 1/60 s tick (`0x3c888889`).
const TICK: f32 = f32::from_bits(0x3c88_8889);

/// The four chase modes cycled by the camera button (`Camera_ApplyModeIndex` 0x173eb8 / `Camera_CycleMode` 0x173e70):
/// `(mode_id, named camera in COMMONOB.CML)`.
pub const MODE_PRESETS: [(u32, &str); 4] = [
    (2, "chase board"),
    (4, "chase near"),
    (5, "chase far"),
    (6, "chase over"),
];

/// The camera shake table from `SLUS_203.26` at `0x337798` (`0xbd` = 189 floats; `CamCamera_EvalPosition` 0x16e900).
const SHAKE_WORDS: [u32; 189] = [
    0x3bfadf2f, 0xbcbe61d0, 0xbd3d9dbb, 0xbd5c8a7a, 0xbd29003f, 0xbcbffef4, 0xbc3becab, 0x3a480410,
    0x3ca56bca, 0x3d004fb1, 0x3ceec9e2, 0x3cdebfa9, 0x3ca9d1f6, 0x3cd10907, 0x3c8793de, 0x3c134acb,
    0x3c1e0a42, 0x3c46b053, 0x3c9c1b54, 0x3c2ba7b9, 0xbaac18f8, 0xbcc3f1c7, 0xbd09b414, 0xbd480e8d,
    0xbda5ecc4, 0xbdef9336, 0xbde88c61, 0xbd878290, 0x3a5d82fd, 0x3d126528, 0x3d93d85a, 0x3e151e32,
    0x3e452b4e, 0x3e37b80b, 0x3e0c269a, 0x3da6601c, 0x3d151505, 0xbd880f99, 0xbe3aed57, 0xbe711a97,
    0xbe6a569b, 0xbe581ba8, 0xbe37e069, 0xbe0b0ff1, 0xbd876966, 0x3c21272d, 0x3d904253, 0x3e23c31e,
    0x3e84e161, 0x3e989d6b, 0x3e94cee6, 0x3e86c82e, 0x3e703298, 0x3e57b891, 0x3e3eaee2, 0x3e228c37,
    0x3de60b2d, 0x3da25bfb, 0x3d66a012, 0x3d19a416, 0x3d026139, 0x3d131ca8, 0x3d3e7d13, 0x3d375c8e,
    0x3d387ad1, 0x3d529aa2, 0x3da41249, 0x3dad17c6, 0x3d9d29dc, 0x3d85b185, 0x3d50ebab, 0x3ca2424a,
    0x3cfa7653, 0x3d599fe4, 0x3cf82907, 0xbcb6f3b2, 0xbd40fece, 0xbd8fa051, 0xbdb5ffe7, 0xbdbd4e90,
    0xbdbcdbbe, 0xbdc25072, 0xbdda78f2, 0xbddede12, 0xbdb46f58, 0xbd51cc10, 0xbc9df97b, 0x3cdd0d8d,
    0x3d96ebd5, 0x3daca46e, 0x3dde14be, 0x3e25f17c, 0x3e65db34, 0x3e7574b4, 0x3e6a0770, 0x3e4c8084,
    0x3e402e66, 0x3e1bc409, 0x3df1c21a, 0x3dd2c1f4, 0x3dee9c45, 0x3e03532e, 0x3e027cbf, 0x3de193b4,
    0x3de6b9c3, 0x3deb19a4, 0x3da486ad, 0x3cec3761, 0xbc92fd33, 0xbd2ab042, 0xbd1097c8, 0xbcba2be0,
    0xbd263843, 0xbd91654d, 0xbdad36b5, 0xbda22dd8, 0xbdbe7afa, 0xbe28259e, 0xbe84625f, 0xbe98b5cc,
    0xbea3cbc5, 0xbebdde7a, 0xbecff66f, 0xbec95fab, 0xbeb2ef0b, 0xbe9cc533, 0xbe907fed, 0xbe759536,
    0xbe32120e, 0xbe167558, 0xbdfa27af, 0xbdb13be2, 0xbd4205ff, 0xbd27e953, 0xbdb2745c, 0xbe03e037,
    0xbdf6bdb2, 0xbded6239, 0xbe0686a5, 0xbe0bc6eb, 0xbdfcac8e, 0xbdd95140, 0xbdc35ac9, 0xbdb53587,
    0xbc76cacd, 0x3d63a29c, 0x3d6d4c34, 0x3d50fb65, 0x3d49de05, 0x3d3f572e, 0x3cda89b9, 0xbc9df97b,
    0xbd0ac083, 0xbcb4cc25, 0xbcd44dcb, 0xbd22d624, 0xbd17b95a, 0xbc9e6eeb, 0x3c58c6d6, 0x3cb9799e,
    0x3c955715, 0x3c82cd3a, 0x3c2ec4a4, 0x3bc36545, 0xbc47d5ed, 0xbcabef07, 0xba84a516, 0x3cbf1c76,
    0x3d51a544, 0x3d8a2a0b, 0x3db4013f, 0x3e010731, 0x3e12a6b1, 0x3e16e3b4, 0x3e08b828, 0x3dd7de0d,
    0x3d634656, 0xbd215875, 0xbe1b239a, 0xbe4ba9d2, 0xbe58d196, 0xbe6e54b5, 0xbe803d57, 0xbe81e96c,
    0xbe3346dc, 0xbdb2b021, 0xbd3f6c27, 0xbc925fe9, 0x3c8e086c,
];

fn shake_table() -> [f32; 189] {
    SHAKE_WORDS.map(f32::from_bits)
}

/// One named camera parameter block from a `.cml` file (`COMMONOB.CML`, `0x174` bytes): view (`CamP`, `+0x00..+0x120`),
/// lens (`LookP`, `+0x120..+0x168`), near plane (`+0x168`), and cut-latch flag (`+0x170`).
#[derive(Clone, Debug)]
pub struct CamBlock {
    pub pos: CamP,
    pub look: LookP,
    pub f168: f32,
    pub w170: u32,
}

impl CamBlock {
    /// Parse a raw `0x174`-byte camera record body from `COMMONOB.CML` (or a course `.cml`).
    pub fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 0x174 {
            return None;
        }
        let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let i = |o: usize| u(o) as i32;
        let f = |o: usize| f32::from_bits(u(o));
        let pos = CamP {
            w00: i(0x00),
            w04: i(0x04),
            w10: i(0x10),
            f14: f(0x14),
            w18: i(0x18),
            w1c: u(0x1c),
            w20: i(0x20),
            w24: u(0x24),
            w28: i(0x28),
            w2c: i(0x2c),
            w30: i(0x30),
            base: [f(0x3c), f(0x68), f(0x94)],
            f_b8: f(0xb8),
            f_c4: f(0xc4),
            w_c8: i(0xc8),
            f_cc: f(0xcc),
            f_d0: f(0xd0),
            w_d4: i(0xd4),
            w_d8: i(0xd8),
            f_dc: f(0xdc),
            w_e0: i(0xe0),
            w_f8: i(0xf8),
            w_fc: i(0xfc),
            f_100: f(0x100),
            w_108: i(0x108),
            f_10c: f(0x10c),
            f_110: f(0x110),
            f_114: f(0x114),
            f_118: f(0x118),
        };
        let l = 0x120;
        let look = LookP {
            w00: i(l),
            w04: i(l + 0x04),
            off: [f(l + 0x08), f(l + 0x0c), f(l + 0x10)],
            f14: f(l + 0x14),
            f18: f(l + 0x18),
            f1c: f(l + 0x1c),
            f20: f(l + 0x20),
            f24: f(l + 0x24),
            f28: f(l + 0x28),
            w2c: i(l + 0x2c),
            w30: i(l + 0x30),
            w34: i(l + 0x34),
            w38: i(l + 0x38),
            f3c: f(l + 0x3c),
            w40: i(l + 0x40),
        };
        Some(Self { pos, look, f168: f(0x168), w170: u(0x170) })
    }
}

/// STANDIN: fallback `COMMONOB.CML` chase preset blocks used only when no `.cml` file is loaded (headless unit tests
/// and level folders without `Camera/commonob.cml`); values match `DATA/CAMERA/COMMONOB.CML` verbatim.
fn fallback_presets() -> HashMap<String, CamBlock> {
    let make = |f14, w1c, w24, base_z, f_cc, f_d0, f_100, f_10c, f_110, f_114, f_118, off_z, f18, f20, f24, f28, w2c, w30, w34, f168| CamBlock {
        pos: CamP {
            w00: 1, w04: 0, w10: 1, f14, w18: 1, w1c, w20: 1, w24, w28: 0, w2c: 4, w30: 0,
            base: [0.0, 0.0, base_z], f_b8: 0.0, f_c4: 0.0,
            w_c8: 1, f_cc, f_d0, w_d4: 1, w_d8: 0, f_dc: 0.0, w_e0: 0,
            w_f8: 1, w_fc: 0, f_100, w_108: 0, f_10c, f_110, f_114, f_118,
        },
        look: LookP {
            w00: 1, w04: 0, off: [0.0, 0.0, off_z], f14: 0.0, f18, f1c: 0.0,
            f20, f24, f28, w2c, w30, w34, w38: 0, f3c: 1000.0, w40: 0,
        },
        f168,
        w170: 0,
    };
    let mut m = HashMap::new();
    m.insert("chase board".into(), make(6.0, 2, 0x4000_0000, 0.0, 150.0, 0.0, -0.5, -1.1, 0.6, 1.5, 1.0, 0.0, 0.27, 1.2, 4.0, 2.0, 1, 1, 0, 10.0));
    m.insert("chase near".into(), make(16.0, 3, 0x4040_0000, 35.0, 180.0, 30.0, -0.5, -2.1, 0.8, 2.0, 1.0, 100.0, -0.16, 1.65, 5.0, 4.0, 1, 0, 0xdead_c0ed_u32 as i32, 15.0));
    m.insert("chase far".into(), make(15.0, 4, 0x4100_0000, 0.0, 500.0, 0.0, -0.7, -1.35, 1.0, 2.0, 1.5, 100.0, 0.03, 1.65, 5.0, 8.0, 1, 0, 0xdead_c0ed_u32 as i32, 30.0));
    m.insert("chase over".into(), make(15.0, 4, 0x4100_0000, 0.0, 1000.0, 0.0, -0.85, -1.5, 0.6, 2.0, 2.0, 100.0, 0.15, 1.65, 25.0, 4.0, 0, 0, 0xdead_c0ed_u32 as i32, 50.0));
    m
}

/// The exact game chase camera (`cBxCamera` 0x3368b0 + `Game_UpdateCameras` 0x17f390), replacing `race.rs`'s
/// `ChaseCam` stand-in (row E11d).
pub struct ExactCam {
    /// Last Bevy-space camera position (metres, Y up).
    pub pos: Vec3,
    /// Last Bevy-space look-at point (5 m ahead along camera forward).
    pub look: Vec3,
    /// Camera mode index `0..=3` (`chase board`, `chase near`, `chase far`, `chase over`; default `2`).
    pub mode: usize,
    /// Last camera roll (radians, from `Camera_SmoothOutput` `+0x440`).
    pub roll: f32,
    /// Last horizontal field of view (radians, from `Camera_SmoothOutput` `+0x444`).
    pub fov: f32,
    /// Last near plane distance (cm, from `CamScript_CutTo` `+0x448`).
    pub near: f32,
    /// Last 4×4 transposed view matrix built by `Game_UpdateCameras` 0x17f390 (`race +0xc0`, cm, `+X right, +Y up, +Z ahead`).
    pub view_matrix: [[f32; 4]; 4],
    /// Last camera basis in Bevy space (`(right, up, forward)`, unit vectors) derived directly from `Game_UpdateCameras`.
    pub basis: (Vec3, Vec3, Vec3),
    /// The game's `cBxCamera` object (`0xec0` bytes, `u32` words).
    obj: Vec<u32>,
    /// The 26 rider trackers (`Camera_SetFocusRider` 0x173c50 / `Camera_UpdateRiderTracks` 0x174010).
    trackers: [Tracker; 26],
    /// Loaded named camera blocks from `COMMONOB.CML` (plus course `.cml` / `scripts.cml`).
    blocks: HashMap<String, CamBlock>,
    /// Optional raw `Cml` handle to look up additional named cameras on demand.
    cml: Option<Arc<tricky_data::intro::Cml>>,
    /// Camera shake read indices (`0x335718` / `0x33571c`).
    shake: Shake,
    /// Last non-zero flat direction to target (`0x335724` / `0x335728`).
    last_dir: LastDir,
    /// `Camera_CollideWorld` call counter (`0x335720`).
    collide_counter: u32,
    /// Accumulated time for fixed 60 Hz camera ticks.
    acc: f32,
    /// Whether the camera has been stepped at least once since creation / reset.
    initialized: bool,
    /// Focus rider's `respawns` count as of the last tick (a change triggers `Camera_Cut`).
    seen_respawns: u32,
}

impl Default for ExactCam {
    fn default() -> Self {
        let mut obj = vec![0u32; 0xec0 / 4];
        // `Camera_Reset` 0x173a98 initial state: mode index 2 ("chase far", mode 5), cut pending
        obj[0] = 2;
        obj[1] = 1; // +0x04 cut pending
        obj[2] = 0; // +0x08 cut2
        obj[0x10 / 4] = 500.0f32.to_bits();
        obj[0x428 / 4] = MODE_PRESETS[2].0;
        obj[0x444 / 4] = 1.65f32.to_bits();
        obj[0x448 / 4] = 30.0f32.to_bits();
        obj[0x49c / 4] = 1.65f32.to_bits();
        obj[0x4a0 / 4] = 30.0f32.to_bits();
        Self {
            pos: Vec3::ZERO,
            look: Vec3::NEG_Z * 5.0,
            mode: 2,
            roll: 0.0,
            fov: 1.65,
            near: 30.0,
            view_matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            basis: (Vec3::X, Vec3::Y, Vec3::NEG_Z),
            obj,
            trackers: std::array::from_fn(|_| Tracker::default()),
            blocks: fallback_presets(),
            cml: None,
            shake: Shake { i0: 0, i1: 0 },
            last_dir: LastDir { x: 0.0, y: 1.0 },
            collide_counter: 0,
            acc: 0.0,
            initialized: false,
            seen_respawns: 0,
        }
    }
}

impl ExactCam {
    /// Load named camera blocks from the course's / `COMMONOB.CML` / `SCRIPTS.CML` bundle (`Cml`).
    pub fn set_cml(&mut self, cml: Arc<tricky_data::intro::Cml>) {
        if self.cml.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cml)) {
            return;
        }
        for name in ["chase board", "chase near", "chase far", "chase over", "chase reverse", "chase eyes", "replay cam", "replay start"] {
            if let Some(raw) = cml.raw_camera(name) {
                if let Some(blk) = CamBlock::parse(raw) {
                    self.blocks.insert(name.into(), blk);
                }
            }
        }
        self.cml = Some(cml);
    }

    /// Whether `COMMONOB.CML` camera blocks have been loaded.
    pub fn has_cml(&self) -> bool {
        self.cml.is_some()
    }

    /// Look up a named camera block by name (from the cached `COMMONOB.CML` blocks or on-demand from `Cml`).
    pub fn block(&self, name: &str) -> Option<CamBlock> {
        if let Some(b) = self.blocks.get(name) {
            return Some(b.clone());
        }
        self.cml.as_ref()?.raw_camera(name).and_then(CamBlock::parse)
    }

    /// `Camera_CycleMode` 0x173e70 + `Camera_ApplyModeIndex` 0x173eb8: cycle `0..=3` (board, near, far, over) and cut.
    pub fn cycle(&mut self, step: i32) {
        self.mode = (self.mode as i32 + step).rem_euclid(MODE_PRESETS.len() as i32) as usize;
        self.obj[0] = self.mode as u32;
        self.obj[0x428 / 4] = MODE_PRESETS[self.mode].0;
        self.cut();
    }

    /// Set mode index `0..=3` directly (`Camera_SetModeIndex` 0x173eb0).
    pub fn set_mode_index(&mut self, idx: usize) {
        self.mode = idx.min(MODE_PRESETS.len() - 1);
        self.obj[0] = self.mode as u32;
        self.obj[0x428 / 4] = MODE_PRESETS[self.mode].0;
        self.cut();
    }

    /// `Camera_Cut` 0x177db0: snap to the wanted position/angles on the next tick.
    pub fn cut(&mut self) {
        self.obj[1] = 1;
        self.obj[2] = 0;
    }

    /// Name of the active chase camera (`COMMONOB.CML`).
    pub fn name(&self) -> &'static str {
        MODE_PRESETS[self.mode].1
    }

    /// Current Bevy camera rotation quaternion from `Game_UpdateCameras` 0x17f390 (`Mat3(right, up, -forward)`).
    pub fn rotation(&self) -> Quat {
        let (r, u, f) = self.basis;
        Quat::from_mat3(&Mat3::from_cols(r, u, -f)).normalize()
    }

    /// Update the camera following a single rider (convenience wrapper around [`ExactCam::update_riders`]).
    pub fn update(&mut self, world: &CollisionWorld, r: &Rider, dt: f32) -> (Vec3, Vec3) {
        self.update_riders(world, &[r], 0, dt)
    }

    /// Step the exact game camera for `dt` seconds with all race riders (`riders[focus]` in slot 0, others in slots
    /// `1..`, prev/next in slots `10`/`11`, by character in slots `14..26`, matching `Camera_SetFocusRider` 0x173c50).
    /// Returns `(pos, look)` in Bevy metres; `roll`, `fov`, `near`, `basis`, and `view_matrix` are updated on `self`.
    pub fn update_riders(&mut self, world: &CollisionWorld, riders: &[&Rider], focus: usize, dt: f32) -> (Vec3, Vec3) {
        let Some(&focus_rider) = riders.get(focus) else {
            return (self.pos, self.look);
        };
        self.obj[0] = self.mode as u32;
        self.obj[0x428 / 4] = MODE_PRESETS[self.mode].0;
        if !self.initialized || focus_rider.respawns != self.seen_respawns {
            self.seen_respawns = focus_rider.respawns;
            self.cut();
        }
        // Cut if focus rider jumped far away (e.g. teleport pad)
        if self.initialized {
            let dist = self.pos.distance(focus_rider.pos);
            if dist > 40.0 {
                self.cut();
            }
        }
        self.acc += dt;
        let mut ran = 0;
        while self.acc >= TICK * 0.999 && ran < 4 {
            self.acc -= TICK;
            self.step_tick(world, riders, focus);
            ran += 1;
        }
        if ran == 0 && (!self.initialized || self.obj[1] != 0) {
            self.step_tick(world, riders, focus);
        }
        (self.pos, self.look)
    }

    /// One 60 Hz tick of `cBxCamera::vf1` 0x177078 + `CamScript_CutTo` 0x174c98 + `Game_UpdateCameras` 0x17f390.
    fn step_tick(&mut self, world: &CollisionWorld, riders: &[&Rider], focus: usize) {
        // 1. Build the 26-slot rider assignment (`Camera_SetFocusRider` 0x173c50)
        let mut slots: [Option<&Rider>; 26] = [None; 26];
        slots[0] = riders.get(focus).copied();
        let mut next = 1usize;
        for (i, &r) in riders.iter().enumerate() {
            if i != focus && next < 8 {
                slots[next] = Some(r);
                next += 1;
            }
            if i == focus {
                if i > 0 {
                    slots[10] = riders.get(i - 1).copied();
                }
                if i + 1 < riders.len() {
                    slots[11] = riders.get(i + 1).copied();
                }
            }
            // slots 14..26: by character index
            if let Some(ch) = crate::trickdata::RIDERS.iter().position(|d| std::ptr::eq(d, r.data)) {
                if ch < 12 {
                    slots[14 + ch] = Some(r);
                }
            }
        }

        // 2. `Camera_UpdateRiderTracks` 0x174010 -> `Camera_TrackRider` 0x173270 on each active rider slot
        let cut_force = self.obj[1] as i64;
        for i in 0..riders.len().min(8) {
            let tr = slots[i].map(track_rider_of);
            camera::track_rider(&mut self.trackers[i], tr.as_ref(), cut_force);
        }
        // Slot 8 (+0x5e8): focus rider with its predicted landing point (+0x57c0)
        if let Some(r) = slots[0] {
            slots[8] = Some(r);
            self.trackers[8].marker = [r.land_point[0], r.land_point[1], r.land_point[2]];
        }
        for i in 10..26 {
            if let Some(r) = slots[i] {
                let tr = track_rider_of(r);
                self.trackers[i] = Tracker { speed: tr.f1b0, dir: [0.0; 3], marker: tr.v4ad0 };
            }
        }

        // 3. `cBxCamera::vf1` 0x177078 clock (+0x10) and wipe-out blend (+0x20 / obj[8])
        let t = F(f32::from_bits(self.obj[0x10 / 4])) + F(TICK);
        self.obj[0x1c / 4] = 0;
        self.obj[0x10 / 4] = t.0.to_bits();
        let mode = self.obj[0x428 / 4];
        if let Some(fr) = slots[0] {
            let motion = fr.motion;
            if mode != 1 && fr.ctl != 0x12 {
                let a = F(f32::from_bits(self.obj[0x20 / 4]));
                if self.obj[0xeb4 / 4] == 0 && (motion == 5 || motion == 6) {
                    let n = a + F(f32::from_bits(0x3cee_eeef));
                    let v = if f32::from_bits(0x3f7a_e148) < n.0 { 0x3f7a_e148 } else { n.0.to_bits() };
                    self.obj[0x20 / 4] = v;
                } else if 0.0 < a.0 {
                    let n = a - F(f32::from_bits(0x3b44_9ba6));
                    self.obj[0x20 / 4] = if n.0 < 0.0 { 0 } else { n.0.to_bits() };
                }
            }
            if motion != 5 && motion != 6 {
                self.obj[0xeb4 / 4] = 0;
            }
        }

        // Build `Entry` and `LookEntry` arrays from the 26 slots
        let entries: Vec<Entry> = (0..26)
            .map(|i| Entry {
                rider: slots[i].map(target_rider_of),
                w4: self.trackers[i].speed,
                v8: self.trackers[i].dir,
                p14: self.trackers[i].marker,
            })
            .collect();
        let look_entries: Vec<LookEntry> = (0..26)
            .map(|i| LookEntry {
                rider: slots[i].map(look_rider_of),
                p14: self.trackers[i].marker,
            })
            .collect();
        let focus_bone = slots[0].map_or([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]], |r| r.board_bone_frame());

        // 4. `CamScript_CutTo` 0x174c98 -> `Camera_GotoNamed` 0x174888 -> `EvalPosition` + `EvalLookAt` + `SmoothOutput`
        let name = MODE_PRESETS[self.mode].1;
        let block = self.block(name).unwrap_or_else(|| fallback_presets().remove("chase far").unwrap());
        let table = shake_table();
        let mut cut_env = HostCutEnv {
            world,
            block,
            entries: &entries,
            look_entries: &look_entries,
            focus_bone,
            shake: &mut self.shake,
            shake_table: &table,
            last_dir: &mut self.last_dir,
            collide_counter: &mut self.collide_counter,
        };
        camscript::cut_to(&mut self.obj, name.as_bytes(), &mut cut_env);

        // 5. `Game_UpdateCameras` 0x17f390: build exact 4×4 view matrix (+0xc0), pos (+0xb0), half-fov (+0x100)
        let entry = CamEntry {
            pos: [
                f32::from_bits(self.obj[0x42c / 4]),
                f32::from_bits(self.obj[0x430 / 4]),
                f32::from_bits(self.obj[0x434 / 4]),
            ],
            yaw: f32::from_bits(self.obj[0x438 / 4]),
            pitch: f32::from_bits(self.obj[0x43c / 4]),
            roll: f32::from_bits(self.obj[0x440 / 4]),
            fov: f32::from_bits(self.obj[0x444 / 4]),
            near: f32::from_bits(self.obj[0x448 / 4]),
            w20: 0,
        };
        let mut game_obj = [0u32; 0x2a0 / 4];
        game_obj[0x298 / 4] = 1;
        game_obj[0xa0 / 4] = 0;
        struct OneEntry(CamEntry);
        impl EntryEnv for OneEntry {
            fn entry(&mut self, _: u32) -> CamEntry {
                self.0
            }
        }
        camera_update::update_cameras(&mut game_obj, &mut OneEntry(entry));
        let mut vm = [[0.0f32; 4]; 4];
        for r in 0..4 {
            for c in 0..4 {
                vm[r][c] = f32::from_bits(game_obj[0xc0 / 4 + 4 * r + c]);
            }
        }
        self.view_matrix = vm;
        // Columns 0, 1, 2 of the transposed view matrix are game-space right (`side`), up (`back`), forward (`v`)
        let side = [vm[0][0], vm[1][0], vm[2][0]];
        let up = [vm[0][1], vm[1][1], vm[2][1]];
        let fwd = [vm[0][2], vm[1][2], vm[2][2]];
        let to_bevy = |v: [f32; 3]| Vec3::new(v[0], v[2], -v[1]);
        self.pos = to_bevy(entry.pos) * 0.01;
        let br = to_bevy(side).normalize_or(Vec3::X);
        let bu = to_bevy(up).normalize_or(Vec3::Y);
        let bf = to_bevy(fwd).normalize_or(Vec3::NEG_Z);
        self.basis = (br, bu, bf);
        self.look = self.pos + bf * 5.0;
        self.roll = entry.roll;
        self.fov = f32::from_bits(game_obj[0x100 / 4]) * 2.0;
        self.near = f32::from_bits(game_obj[0x104 / 4]);
        self.initialized = true;
    }
}

fn rider_orient(r: &Rider) -> orient::Orient {
    match r.att {
        Some((o, _)) => o,
        None => orient::placed_attitude(placed_heading(r.yaw), None),
    }
}

fn track_rider_of(r: &Rider) -> TrackRider {
    let o = rider_orient(r);
    let bf = r.board_frame();
    TrackRider {
        w424: r.motion as i32,
        w428: r.ctl as i32,
        v150: ground_tick::game_dir(r.vel * 100.0),
        v180: o.fwd,
        f1b0: o.yaw,
        v4ad0: [bf[3][0], bf[3][1], bf[3][2]],
    }
}

fn target_rider_of(r: &Rider) -> TargetRider {
    let bb = r.board_bone_frame();
    let bone = [bb[3][0], bb[3][1], bb[3][2]];
    let v2d0 = if r.rail_grid { [r.rail_hit[0], r.rail_hit[1], r.rail_hit[2]] } else { bone };
    TargetRider {
        f130: if r.speed_timer > 0.0 { 1.0 } else { r.boost_lvl },
        flag40: r.data.goofy,
        v2d0,
        v4a90: bone,
        t4498: bone,
    }
}

fn look_rider_of(r: &Rider) -> LookRider {
    let bb = r.board_bone_frame();
    let bone = [bb[3][0], bb[3][1], bb[3][2]];
    let v2d0 = if r.rail_grid { [r.rail_hit[0], r.rail_hit[1], r.rail_hit[2]] } else { bone };
    let n = ground_tick::game_dir(r.normal);
    LookRider {
        motion: r.motion as i32,
        f57b8: (r.land_time - r.air_time).max(0.0),
        v57d0: [r.land_vel[0], r.land_vel[1], r.land_vel[2]],
        v57e0: r.land_normal,
        w290: r.surface as i32,
        v2a0: [n[0], n[1], n[2]],
        v2d0,
        f464_60: r.stats.scale,
        v4a90: bone,
        t4498: bone,
    }
}

/// `WorldEnv` for `Camera_CollideWorld` 0x16fe60 and `Camera_CollidePush` 0x16fb08 on `CollisionWorld` (`CourseGround`
/// when a disc course is loaded, else `wipeout::mesh_line`).
struct CamWorld<'a> {
    world: &'a CollisionWorld,
    lines: Vec<([f32; 4], [f32; 4], i32, f32)>,
}

impl<'a> CamWorld<'a> {
    fn new(world: &'a CollisionWorld) -> Self {
        Self { world, lines: Vec::new() }
    }
    fn cast(&self, line: Line, res: &mut RayRes) -> f32 {
        let &(a, b, mode, f12) = &self.lines[line.main as usize];
        match &self.world.course {
            Some(cg) => {
                let mut q = WorldLine::new(f12, a, b, mode as u32);
                let (t, h) = cg.ray_cast(&mut q, self.world.showoff, None, &|i| self.world.course_skips(i));
                if let Some(h) = h.filter(|_| 0.0 < t) {
                    res.hit = h.pos;
                    res.normal = h.normal;
                    return t;
                }
                -1.0
            }
            None => {
                let from = ground_tick::port_dir(a) * 0.01;
                let to = ground_tick::port_dir(b) * 0.01;
                if let Some((t, hit, normal)) = self.world.raycast_hit(from, to) {
                    let h = ground_tick::game_dir(hit * 100.0);
                    let n = ground_tick::game_dir(normal);
                    res.hit = [h[0], h[1], h[2], 1.0];
                    res.normal = [n[0], n[1], n[2], 0.0];
                    return t;
                }
                -1.0
            }
        }
    }
}

impl WorldEnv for CamWorld<'_> {
    fn line(&mut self, a: [f32; 4], b: [f32; 4], mode: i32, f12: f32) -> u32 {
        let id = self.lines.len() as u32;
        self.lines.push((a, b, mode, f12));
        id
    }
    fn ray_cast(&mut self, line: Line, res: &mut RayRes) -> f32 {
        self.cast(line, res)
    }
    /// `World_IsBlocked` 0x25c690: `World_RayCast` > 0, else `World_QueryInstances` > 0 on a solid camera-blocking
    /// object (`flags & 0x80 != 0`).
    fn is_blocked(&mut self, line: Line, res: &mut RayRes) -> bool {
        if 0.0 < self.cast(line, res) {
            return true;
        }
        let Some(cg) = &self.world.course else { return false };
        let &(a, b, mode, f12) = &self.lines[line.main as usize];
        let mut q = WorldLine::new(f12, a, b, mode as u32);
        let (t, h) = cg.query_instances(&mut q, self.world.showoff, &|i| self.world.course_skips(i));
        if let Some(h) = h.filter(|_| 0.0 < t) {
            if let Some(o) = h.object {
                if cg.object_flags(o) & 0x80 != 0 {
                    res.hit = h.pos;
                    res.normal = h.normal;
                    return true;
                }
            }
        }
        false
    }
    fn push(&mut self, k: f32, pos: &mut [f32; 3], target: [f32; 3]) {
        let mut inner = CamWorld::new(self.world);
        camera::collide_push(k, pos, target, &mut inner);
    }
}

struct HostPosEnv<'a> {
    world: &'a CollisionWorld,
    focus_bone: [[f32; 4]; 4],
    collide_counter: &'a mut u32,
}

impl CamEnv for HostPosEnv<'_> {
    fn stage_area_to_world(&mut self, _: i32, v: [f32; 4]) -> StageOut {
        StageOut { ret: false, v, yaw: 0.0 }
    }
    fn matrix(&mut self, _: i32) -> [[f32; 4]; 4] {
        self.focus_bone
    }
    fn collide_world(&mut self, radius: f32, pos: &mut [f32; 3], target: [f32; 3]) -> u64 {
        let mut w = CamWorld::new(self.world);
        camera::collide_world(radius, pos, target, self.collide_counter, &mut w) as u64
    }
}

struct HostLookEnv {
    focus_bone: [[f32; 4]; 4],
}

impl LookEnv for HostLookEnv {
    fn translation(&mut self, _: i32) -> [f32; 3] {
        [self.focus_bone[3][0], self.focus_bone[3][1], self.focus_bone[3][2]]
    }
}

struct HostSmoothEnv<'a> {
    world: &'a CollisionWorld,
}

impl SmoothEnv for HostSmoothEnv<'_> {
    fn collide_push(&mut self, k: f32, pos: &mut [f32; 3], target: [f32; 3]) {
        let mut w = CamWorld::new(self.world);
        camera::collide_push(k, pos, target, &mut w);
    }
}

struct HostCutEnv<'a> {
    world: &'a CollisionWorld,
    block: CamBlock,
    entries: &'a [Entry],
    look_entries: &'a [LookEntry],
    focus_bone: [[f32; 4]; 4],
    shake: &'a mut Shake,
    shake_table: &'a [f32],
    last_dir: &'a mut LastDir,
    collide_counter: &'a mut u32,
}

impl crate::campath::KeyEnv for HostCutEnv<'_> {
    fn key(&mut self, _: i32) -> crate::campath::Key {
        crate::campath::missing_key()
    }
}

impl ScriptEnv for HostCutEnv<'_> {
    fn owner(&mut self, _: u32) -> crate::campath::Owner {
        crate::campath::Owner { count: 0, speed: 0.0 }
    }
    fn smooth(&mut self, obj: &mut [u32]) {
        let mut env = HostSmoothEnv { world: self.world };
        camera::smooth_output(obj, &mut env);
    }
    fn cut_fade(&mut self, _: &mut [u32]) {}
    fn audio_cut(&mut self) {}
    fn find_owner(&mut self) -> u32 { 0 }
    fn first_position(&mut self, _: u32) -> [f32; 3] { [0.0; 3] }
    fn audio_stop(&mut self) {}
    fn entry_count(&mut self) -> i32 { 0 }
    fn reset_rider_anim(&mut self, _: i32) {}
}

impl CutEnv for HostCutEnv<'_> {
    fn class_open(&mut self, _: &[u8]) -> u32 { 1 }
    fn class_close(&mut self, _: u32) {}
    fn class_word_170(&mut self, _: u32) -> u32 { self.block.w170 }
    fn class_f168(&mut self, _: u32) -> f32 { self.block.f168 }

    fn eval_pos(&mut self, obj: &mut [u32], flag: u32) -> u32 {
        let mut out = CamOut {
            pos: [f32::from_bits(obj[0x484 / 4]), f32::from_bits(obj[0x488 / 4]), f32::from_bits(obj[0x48c / 4])],
            v24: [f32::from_bits(obj[0x4a8 / 4]), f32::from_bits(obj[0x4ac / 4]), f32::from_bits(obj[0x4b0 / 4])],
            w30: obj[0x4b4 / 4],
            w38: obj[0x4bc / 4],
            w3c: obj[0x4c0 / 4],
            w40: obj[0x4c4 / 4],
            w44: obj[0x4c8 / 4],
            yaw: f32::from_bits(obj[0x4d0 / 4]),
            pitch: f32::from_bits(obj[0x4d4 / 4]),
        };
        let mut env = HostPosEnv { world: self.world, focus_bone: self.focus_bone, collide_counter: self.collide_counter };
        let res = camera::eval_position(&mut self.block.pos, self.entries, &mut out, flag != 0, &mut env, self.shake, self.shake_table);
        for k in 0..3 {
            obj[0x484 / 4 + k] = out.pos[k].to_bits();
            obj[0x4a8 / 4 + k] = out.v24[k].to_bits();
        }
        obj[0x4b4 / 4] = out.w30;
        obj[0x4bc / 4] = out.w38;
        obj[0x4c0 / 4] = out.w3c;
        obj[0x4c4 / 4] = out.w40;
        obj[0x4c8 / 4] = out.w44;
        obj[0x4d0 / 4] = out.yaw.to_bits();
        obj[0x4d4 / 4] = out.pitch.to_bits();
        res.ret as u32
    }

    fn eval_ang(&mut self, obj: &mut [u32], _flag: u32, eac: u32, t2: u32) {
        let mut view = LookView {
            pos: [f32::from_bits(obj[0x484 / 4]), f32::from_bits(obj[0x488 / 4]), f32::from_bits(obj[0x48c / 4])],
            yaw: f32::from_bits(obj[0x490 / 4]),
            pitch: f32::from_bits(obj[0x494 / 4]),
            roll: f32::from_bits(obj[0x498 / 4]),
            fov: f32::from_bits(obj[0x49c / 4]),
            f48: f32::from_bits(obj[0x4cc / 4]),
        };
        let mut timer = f32::from_bits(obj[0x10 / 4]);
        let mut env = HostLookEnv { focus_bone: self.focus_bone };
        camera::eval_look_at(&self.block.look, self.look_entries, &mut view, self.last_dir, &mut timer, eac as i64, t2 as i64, 0.0, &mut env);
        obj[0x10 / 4] = timer.to_bits();
        obj[0x490 / 4] = view.yaw.to_bits();
        obj[0x494 / 4] = view.pitch.to_bits();
        obj[0x498 / 4] = view.roll.to_bits();
        obj[0x49c / 4] = view.fov.to_bits();
        obj[0x4cc / 4] = view.f48.to_bits();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_cam_orbits_behind_and_looks_at_rider_across_all_four_modes() {
        let world = CollisionWorld::default();
        let mut r = Rider::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        r.grounded = true;
        r.motion = 2;
        r.vel = Vec3::new(0.0, 0.0, -15.0); // moving along Bevy -Z (= game +Y)

        let mut cam = ExactCam::default();
        for (idx, (_, name)) in MODE_PRESETS.iter().enumerate() {
            cam.set_mode_index(idx);
            assert_eq!(cam.name(), *name);
            let (pos, look) = cam.update(&world, &r, 1.0 / 60.0);
            // Camera sits behind (+Z in Bevy) and above (+Y in Bevy) the rider moving along -Z
            assert!(pos.z > r.pos.z + 0.5, "{name}: cam should sit behind rider, pos={pos:?} rider={:?}", r.pos);
            assert!(pos.y > r.pos.y + 0.3, "{name}: cam should sit above rider, pos={pos:?} rider={:?}", r.pos);
            // Forward vector points from camera toward rider (negative Z in Bevy)
            let fwd = (look - pos).normalize();
            assert!(fwd.z < -0.5, "{name}: cam forward should look down -Z toward rider, fwd={fwd:?}");
            // Right vector points along +X, up vector points along +Y
            assert!(cam.basis.0.x > 0.9, "{name}: right should be +X, basis={:?}", cam.basis);
            assert!(cam.basis.1.y > 0.5, "{name}: up should be +Y, basis={:?}", cam.basis);
            // View matrix has orthonormal columns matching game right/up/forward
            let vm = cam.view_matrix;
            let len_r = (vm[0][0].powi(2) + vm[1][0].powi(2) + vm[2][0].powi(2)).sqrt();
            let len_u = (vm[0][1].powi(2) + vm[1][1].powi(2) + vm[2][1].powi(2)).sqrt();
            let len_f = (vm[0][2].powi(2) + vm[1][2].powi(2) + vm[2][2].powi(2)).sqrt();
            assert!((len_r - 1.0).abs() < 1e-4 && (len_u - 1.0).abs() < 1e-4 && (len_f - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn exact_cam_loads_commonob_cml_from_disc_and_boost_pulls_near_cam_back() {
        let Ok(disc) = std::env::var("TRICKY_DISC") else { return };
        let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc).unwrap());
        let cml = Arc::new(tricky_data::intro::Cml::from_disc(&mut files, 0).unwrap());

        let mut cam = ExactCam::default();
        cam.set_cml(cml);
        assert!(cam.has_cml());
        let near = cam.block("chase near").expect("chase near in COMMONOB.CML");
        assert_eq!(near.pos.f_cc, 180.0);
        assert_eq!(near.pos.f_d0, 30.0);
        assert_eq!(near.pos.base[2], 35.0);
        let far = cam.block("chase far").expect("chase far in COMMONOB.CML");
        assert_eq!(far.pos.f_cc, 500.0);
        assert_eq!(far.look.f20, 1.65);

        let world = CollisionWorld::default();
        let mut r = Rider::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        r.grounded = true;
        r.motion = 2;
        r.vel = Vec3::new(0.0, 0.0, -15.0);

        cam.set_mode_index(1); // chase near (180 + 30 * boost_lvl cm)
        let (p0, _) = cam.update(&world, &r, 1.0 / 60.0);
        r.boost_lvl = 1.0;
        cam.cut();
        let (p1, _) = cam.update(&world, &r, 1.0 / 60.0);
        let d0 = p0.distance(r.pos);
        let d1 = p1.distance(r.pos);
        assert!(d1 > d0 + 0.2, "boost should pull chase near camera back by ~30 cm: d0={d0:.3} d1={d1:.3}");
    }

    #[test]
    fn exact_cam_wipeout_blend_wall_collide_landing_dip_and_multi_rider_slots() {
        let mut world = CollisionWorld::default();
        world.min_y = -1e6;
        // Put a solid vertical wall at z = +2.5 m (behind rider at z = 0 looking along -Z)
        let q = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        world.add_ex([q(-10.0, -5.0, 2.5), q(10.0, 10.0, 2.5), q(10.0, -5.0, 2.5)], None, false, 1, false);
        world.add_ex([q(-10.0, -5.0, 2.5), q(-10.0, 10.0, 2.5), q(10.0, 10.0, 2.5)], None, false, 1, false);

        let mut p = Rider::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        p.grounded = true;
        p.motion = 2;
        p.vel = Vec3::new(0.0, 0.0, -15.0);
        let mut opp = Rider::new(Vec3::new(3.0, 2.0, -4.0), 0.0);
        opp.grounded = true;
        opp.motion = 2;
        opp.vel = Vec3::new(0.0, 0.0, -14.0);
        opp.data = &crate::trickdata::RIDERS[1];

        let mut cam = ExactCam::default();
        // 1. Wall collision (`Camera_CollideWorld` 0x16fe60): "chase far" (5 m behind) is blocked by wall at z = +2.5
        let (pos, _) = cam.update_riders(&world, &[&p, &opp], 0, 1.0 / 60.0);
        assert!(pos.z < 2.5, "Camera_CollideWorld should pull camera in front of wall at z=2.5, got {pos:?}");
        assert!(cam.collide_counter > 0);
        // Multi-rider tracker slot 1 tracks opponent
        assert!(cam.trackers[1].dir[1] > 0.5, "opponent tracker slot 1 should track +Y game velocity");

        // 2. Wipe-out blend (`cBxCamera::vf1` 0x177078): ramps obj[0x20/4] up toward 0.98 in motion 5 (open world)
        let open = CollisionWorld::default();
        let mut wcam = ExactCam::default();
        wcam.update(&open, &p, 1.0 / 60.0);
        p.motion = 5;
        p.crashed = 1.0;
        for _ in 0..20 {
            wcam.update(&open, &p, 1.0 / 60.0);
        }
        let blend = f32::from_bits(wcam.obj[0x20 / 4]);
        assert!(blend > 0.5 && blend <= 0.98, "wipe-out blend should climb in motion 5: {blend}");

        // 3. Hard landing dip (`CamCamera_EvalLookAt` 0x1722e8): air (< 2 ticks to land, > 1000 cm/s into slope) resets
        //    dip timer to 0, then landing on snow (motion 2) dips pitch downward for 5 ticks.
        let mut dcam = ExactCam::default();
        let mut air = Rider::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        air.grounded = false;
        air.motion = 1;
        air.vel = Vec3::new(0.0, -25.0, -15.0);
        air.land_time = 0.5;
        air.air_time = 0.0;
        air.land_vel = [0.0, 1500.0, -2500.0, 0.0];
        air.land_normal = [0.0, 0.0, 1.0, 0.0];
        // Initialize camera on a first cut tick, then step an imminent-landing tick (< 2 ticks left).
        dcam.update(&open, &air, 1.0 / 60.0);
        air.land_time = 0.01;
        dcam.update(&open, &air, 1.0 / 60.0);
        let t_after_air = f32::from_bits(dcam.obj[0x10 / 4]);
        assert_eq!(t_after_air, 0.0, "imminent hard landing should reset dip timer to 0");
        air.grounded = true;
        air.motion = 2;
        dcam.update(&open, &air, 1.0 / 60.0);
        let pitch_dip = f32::from_bits(dcam.obj[0x494 / 4]);
        for _ in 0..10 {
            dcam.update(&open, &air, 1.0 / 60.0);
        }
        let pitch_later = f32::from_bits(dcam.obj[0x494 / 4]);
        assert!(pitch_dip < pitch_later, "landing dip should pitch look-at down during first 5 ticks: {pitch_dip} vs {pitch_later}");
    }
}
