//! The ground, landing, air, rail and wipe-out control states as the game runs them (board rows G1i1, G1i2a, G1i2b):
//! - Ground & landing (`rider+0x428`): cruise 3 (`CruiseControl_Update` 0x102fc8), prewind 0xe (`PrewindState_Update`
//!   0x1050e8), jump 10 (`JumpControl_Update` 0x1046b0), normal landing 0xc (`LandNormalControl_Update` 0x104e68) and
//!   hard landing 0xb (`LandHardControl_Update` 0x104800).
//! - Air: natural air 0xd (`NaturalAirControl_Update` 0x1000e0) and spin 0x10 (`SpinState_Update` 0x100908 +
//!   `SpinState_UpdateTrickClips` 0x1272e8).
//! - Rail: rail slide 0xf (`RailSlideControl_Enter` 0x106dc8 / `RailSlideControl_Update` 0x1073e0 /
//!   `RailSlideControl_Exit` 0x1075f0), handing off to prewind 0xe and jump 10 for rail hops.
//! - Wipe-out & get-up: 0x13 (`WipeOutControl`, inline reset check in `Boarder_UpdateControl` 0x11c9a0) and 0x14
//!   (`GetUpState_Update` 0x108198).
//! Each runs on the pad word `cPlayer::GetControls` 0x151250 packs for it, after the tick's motion (`cBoarder::vf7`
//! 0x117198's order). They set targets; the now/rate/target records (brake +0x1fc, crouch +0x208, steer +0x214, the
//! wind-ups +0x220 and +0x22c) move toward them before the next tick's motion, by their rate × 1/60.

use super::{ground_tick, uber_grab, Input, Rider, RiderSpeech, RIDERS};
use crate::boarder::{self, Fields, Prewind, PrewindCall, SteerState};
use crate::control::{self, ControlEnv};
use crate::controls::{get_controls, ControlsIn, Mapper};
use crate::finish;
use crate::ps2f::F;
use crate::rail_control::{self, RailCall, RailControl, RailRider};

/// The game's 1/60 s (0x3c888889): what the records move by their rate each tick (`cBoarder::vf7`)
#[cfg(test)]
const DT: f32 = f32::from_bits(0x3c88_8889);

/// A now / rate / target record
pub type Rec = [f32; 3];

/// The records the ground and air states set (the steer in the game's sign: the port's is the other way)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Records {
    /// +0x1fc brake, +0x208 crouch, +0x214 steer, +0x220 the spin wind-up, +0x22c the flip wind-up
    pub brake: Rec,
    pub crouch: Rec,
    pub steer: Rec,
    pub spin: Rec,
    pub flip: Rec,
}

/// The control states (rider+0x428)
pub const SCRIPTED: u32 = 1;
pub const CRUISE: u32 = 3;
pub const JUMP: u32 = 10;
pub const LAND_HARD: u32 = 0xb;
pub const LAND_NORMAL: u32 = 0xc;
pub const NATURAL_AIR: u32 = 0xd;
pub const PREWIND: u32 = 0xe;
pub const RAIL: u32 = 0xf;
pub const SPIN: u32 = 0x10;
pub const WIPEOUT: u32 = 0x13;
pub const GET_UP: u32 = 0x14;
pub const RESET: u32 = 0x16;

/// Hosted trick-clip controller for `SpinState_UpdateTrickClips` 0x1272e8 and `AnimClip_Advance` 0x15a030 marker
/// flags (`+0x74` past, `+0x75` crossed this tick) on `TRICK_SLOTS[ch][board]` (0x333098).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrickClips {
    /// `rider+0x1e4`: active trick slot (`0x1a` = none)
    pub trick_id: u32,
    /// `rider+0x1e8`: stage (`0` grab, `1` tweak, `3` wait-rise, `4` uber)
    pub stage: u32,
    /// `rider+0x1ec`: ticks held at marker 2
    pub hold_ticks: u32,
    /// current layer-0 trick clip id (`0x300` = none)
    pub clip: u32,
    /// current layer-0 clip category (`0xe` grab, `0xf` tweak, `0x10` uber, `8` none)
    pub clip_type: u32,
    /// index `0..15` into `TRICK_SLOTS[ch][board]` / `RiderData::rows`
    pub row: Option<usize>,
    /// current frame (30 fps)
    pub frame: f32,
    /// controller clip speed (`0.78212035 + tricks * 0.8057834 / 255.0` for grab/tweak, `1.0` for uber)
    pub speed: f32,
    /// hold frame (`m2`) while grab/tweak is held; `None` once `AnimCtl_StopClipsInCategory` releases it or for uber
    pub stop: Option<f32>,
    /// markers `[m1, m2, m3, m4]` from the `.afl` clip header (`+0xc..+0x14`, `0xffff` = unused; `AnimClip_Advance` 0x15a030)
    pub markers: [u16; 4],
    /// clip length in frames
    pub len: f32,
    /// `+0x74`: bit `k-1` set while frame is past marker `k` (`AnimCtl_TestClipFlag` 0x15ffb8)
    pub past: u8,
    /// `+0x75`: bit `k-1` set when marker `k` was crossed this tick (`AnimCtl_TestClipBit` 0x160008)
    pub crossed: u8,
    /// whether the active uber has crossed marker 1 and incremented `ubers_done`
    pub uber_scored: bool,
}

impl Default for TrickClips {
    fn default() -> Self {
        Self {
            trick_id: 0x1a,
            stage: 0,
            hold_ticks: 0,
            clip: 0x300,
            clip_type: 8,
            row: None,
            frame: 0.0,
            speed: 1.0,
            stop: None,
            markers: [tricky_data::afl::MARKER_UNUSED; 4],
            len: 0.0,
            past: 0,
            crossed: 0,
            uber_scored: false,
        }
    }
}

impl TrickClips {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `AnimCtl_TestClipFlag` 0x15ffb8(flag, 0)
    pub fn flag(&self, n: u32) -> bool {
        self.clip != 0x300 && (self.past as u32 >> (n.wrapping_sub(1) & 31)) & 1 != 0
    }

    /// `AnimCtl_TestClipBit` 0x160008(flag, 0)
    pub fn bit(&self, n: u32) -> bool {
        self.clip != 0x300 && (self.crossed as u32 >> (n.wrapping_sub(1) & 31)) & 1 != 0
    }

    /// `Landing_CheckAngles` 0x12ba78 / `Rail_TryCapture` 0x125cb8 clip gate: types `0xe..=0x10` without flag 4.
    pub fn is_unsafe(&self) -> bool {
        matches!(self.clip_type, 0xe..=0x10) && !self.flag(4)
    }

    /// `AnimClip_Advance` 0x15a030 on the active trick clip (`dt` in seconds).
    pub fn advance(&mut self, dt: f32) {
        self.past = 0;
        self.crossed = 0;
        if self.clip == 0x300 {
            return;
        }
        let step = dt * self.speed * 30.0;
        let old = self.frame;
        let new = old + step;
        let lo = old.min(new);
        let hi = old.max(new);
        for (k, &mk) in self.markers.iter().enumerate() {
            if mk == tricky_data::afl::MARKER_UNUSED {
                continue;
            }
            let f = mk as f32;
            if f < hi {
                self.past |= 1 << k;
                if lo <= f {
                    self.crossed |= 1 << k;
                }
            }
        }
        let mut at = new;
        if let Some(stop) = self.stop {
            if stop < at {
                at = stop;
            }
        }
        let last = (self.len - 1.0).max(0.0);
        if last < at {
            if self.stop.is_none() && last <= old {
                self.clip = 0x300;
                self.clip_type = 8;
                self.row = None;
                self.frame = 0.0;
                self.past = 0;
                self.crossed = 0;
                self.uber_scored = false;
                return;
            }
            at = last;
        }
        self.frame = at;
    }

    /// `AnimCtl_GetClipDuration` 0x15fec8: `len / (speed * 30.0)`
    fn clip_duration(&self) -> f32 {
        if self.clip == 0x300 || self.speed <= 0.0 {
            0.0
        } else {
            self.len / (self.speed * 30.0)
        }
    }

    /// Begin a grab (`0xe`), tweak (`0xf`), or uber (`0x10`) clip on layer 0.
    pub(super) fn begin(&mut self, clip: u32, clip_type: u32, row: usize, speed: f32, markers: [u16; 4], len: f32, hold_m2: bool) {
        self.clip = clip;
        self.clip_type = clip_type;
        self.row = Some(row);
        self.frame = 0.0;
        self.speed = speed;
        self.stop = if hold_m2 && markers[1] != tricky_data::afl::MARKER_UNUSED {
            Some(markers[1] as f32)
        } else {
            None
        };
        self.markers = markers;
        self.len = len;
        self.past = 0;
        self.crossed = 0;
        self.uber_scored = false;
    }
}

/// The game's calls the states make, on the port's rider fields
struct Env<'a> {
    /// the meter (+0x1c) and the boost level (+0x130)
    meter: &'a mut f32,
    boost_level: &'a mut f32,
    tricky: bool,
    speech: &'a mut Vec<RiderSpeech>,
    /// layer-0 clip id answered to `LandHardControl_Update` 0x104800 (`0x1ff` when done, else `0x23e`) without AnimHost
    clip_id: i32,
    /// live animation controller when attached (row G5d2)
    anim: Option<&'a crate::animhost::RiderAnim>,
    /// `Boarder_SetState`'s new state, `Jump_ApplyImpulse` asked
    state: Option<u32>,
    jumped: bool,
    /// `Boarder_RequestReset` asked, with the state's store of +0x304
    reset: Option<u32>,
}

/// The brake, crouch and steer of a state's rider as `finish`'s helpers take them
fn fin(r: &control::Rider, boost_level: f32) -> finish::Rider {
    finish::Rider {
        brake: r.brake,
        brake_rate: r.brake_rate,
        brake_target: r.brake_target,
        crouch: r.crouch,
        crouch_rate: r.crouch_rate,
        crouch_target: r.crouch_target,
        steer: r.steer,
        steer_rate: r.steer_rate,
        steer_target: r.steer_target,
        boost_level,
        control: r.control,
        class: r.class,
        surface: r.surface,
        vel: r.vel,
        ..Default::default()
    }
}

fn unfin(f: &finish::Rider, r: &mut control::Rider) {
    (r.brake, r.brake_rate, r.brake_target) = (f.brake, f.brake_rate, f.brake_target);
    (r.crouch, r.crouch_rate, r.crouch_target) = (f.crouch, f.crouch_rate, f.crouch_target);
    (r.steer, r.steer_rate, r.steer_target) = (f.steer, f.steer_rate, f.steer_target);
}

impl ControlEnv for Env<'_> {
    fn clip(&mut self, layer: u32) -> i32 {
        match self.anim {
            Some(a) => a.clip(layer) as i32,
            None => self.clip_id,
        }
    }
    fn clip_type(&mut self, layer: u32) -> i32 {
        match self.anim {
            Some(a) => a.clip_type(layer) as i32,
            None => 0,
        }
    }
    fn clip_at_end(&mut self, layer: u32) -> bool {
        match self.anim {
            Some(a) => a.at_end(layer),
            None => true,
        }
    }
    fn layer_phase(&mut self, layer: u32) -> f32 {
        match self.anim {
            Some(a) => a.layer_phase(layer),
            None => 0.0,
        }
    }
    fn play_clip(&mut self, _r: &mut control::Rider, clip: i32) {
        if let Some(a) = self.anim {
            if clip >= 0 {
                a.play_clip(clip as u32, 0, 5, &mut crate::rand::rand_game);
            }
        }
    }
    fn set_state(&mut self, _r: &mut control::Rider, s: u32) {
        self.state = Some(s);
    }
    fn set_motion(&mut self, _r: &mut control::Rider, _m: u32) {}
    /// `Boarder_SetSteerTarget` 0x126948
    fn set_steer_target(&mut self, r: &mut control::Rider, v: f32) {
        let s = SteerState { boost_level: *self.boost_level, class: r.class, vel: r.vel, steer: r.steer, surface: r.surface };
        (r.steer_target, r.steer_rate) = boarder::set_steer_target(v, &s);
    }
    /// `Boarder_SetCrouchTarget` 0x126c00
    fn set_crouch_target(&mut self, r: &mut control::Rider, v: f32) {
        let mut f = Fields { x1fc: r.brake, crouch: r.crouch, crouch_rate: r.crouch_rate, crouch_target: r.crouch_target, ..Default::default() };
        boarder::set_crouch_target(v, &mut f);
        (r.crouch_rate, r.crouch_target) = (f.crouch_rate, f.crouch_target);
    }
    /// `Boarder_SetBrakeCrouchInput` 0x126b38
    fn set_brake_crouch_input(&mut self, r: &mut control::Rider, brake: f32, crouch: f32) {
        let mut f = fin(r, *self.boost_level);
        finish::set_brake_crouch_input(brake, crouch, &mut f);
        unfin(&f, r);
    }
    /// `Boarder_SetBrakeTarget` 0x126c78
    fn set_brake_target(&mut self, r: &mut control::Rider, v: f32) {
        let mut f = fin(r, *self.boost_level);
        finish::set_brake_target(v, &mut f);
        unfin(&f, r);
    }
    /// `Boarder_UpdateBoost` 0x11d038 on the meter and the boost level (the flame's fields, +0x250.., not kept: the
    /// flame is not drawn from them yet)
    fn update_boost(&mut self, _r: &mut control::Rider, a: u32, b: u32) {
        let mut f = Fields { meter: *self.meter, boost_level: *self.boost_level, ..Default::default() };
        // the boost sounds play: the app has no replay (row E1b2g7d3b4e), as the game's test with no replay object
        for c in boarder::update_boost(a != 0, b != 0, true, self.tricky, &mut f) {
            if c == boarder::Call::BoostEmpty {
                self.speech.push(RiderSpeech::BoostEmpty);
            }
        }
        (*self.meter, *self.boost_level) = (f.meter, f.boost_level);
    }
    // STANDIN (row G5d): the lean animation, the turn of a bump clip, the celebrations, taunts and flourishes are the
    // animation's
    fn update_lean_anim(&mut self, _r: &mut control::Rider, _a: f32, _b: f32) {}
    fn rotate_about_axis(&mut self, _r: &mut control::Rider, _angle: f32) {}
    /// `Boarder_RequestReset` 0x118f10 (the reset button, or off the course): the rider resets after the state's
    /// update ([`Rider::request_reset`]), with the +0x304 the state stored
    fn request_reset(&mut self, r: &mut control::Rider) {
        self.reset = Some(r.reset_flag);
    }
    fn jump_impulse(&mut self, _r: &mut control::Rider, _min: f32) {
        self.jumped = true;
    }
    /// `Takeoff_SetSpinRates` 0x126e30 enters the spin state (its rates are set at the take-off, `Rider::spin_enter_game`)
    fn takeoff_spin_rates(&mut self, _r: &mut control::Rider) {
        self.state = Some(SPIN);
    }
    fn play_celebrate(&mut self, _r: &mut control::Rider) {}
    fn play_disappointed(&mut self, _r: &mut control::Rider) {}
    fn play_wind_up(&mut self, _r: &mut control::Rider) {}
    fn play_takeoff(&mut self, _r: &mut control::Rider) {}
    fn ai_taunt_check(&mut self, _r: &mut control::Rider) {}
    fn random_flourish(&mut self, _r: &mut control::Rider) {}
    // STANDIN: the rider's virtual +0x7c(1) (not ported)
    fn stop(&mut self, _r: &mut control::Rider) {}
    // STANDIN: no other riders for the look back (a head turn, row G5d)
    fn race_riders(&mut self) -> i32 {
        0
    }
    fn race_state(&mut self) -> i32 {
        0
    }
    fn cam_start(&mut self, _index: i32, _script: &str) {}
    fn run_control(&mut self, _r: &mut control::Rider, _control: u32) {}
}

impl Rider {
    /// The records' "now" from the port's values (a tick of the port's own code may have changed them)
    pub(super) fn records_from_port(&mut self) {
        self.rec.brake[0] = self.brake_v;
        self.rec.crouch[0] = self.crouch_v;
        self.rec.steer[0] = -self.steer;
        self.rec.spin[0] = self.wind;
        self.rec.flip[0] = self.wind_flip;
    }

    /// The port's values from the records
    pub(super) fn port_from_records(&mut self) {
        self.brake_v = self.rec.brake[0];
        self.crouch_v = self.rec.crouch[0];
        self.steer = -self.rec.steer[0];
        self.wind = self.rec.spin[0];
        self.wind_flip = self.rec.flip[0];
    }

    /// `cBoarder::vf7` 0x117198 before the motion, on the snow: each record moves its rate × 1/60 toward its target
    /// (`boarder_tick::ease`). Runtime ticks run this inside `boarder_tick::boarder_tick` (`rider/vf7.rs`).
    #[cfg(test)]
    pub(super) fn ease_records(&mut self) {
        self.records_from_port();
        for r in [&mut self.rec.brake, &mut self.rec.crouch, &mut self.rec.steer, &mut self.rec.spin, &mut self.rec.flip] {
            r[0] = crate::boarder_tick::ease(r[0], r[2], r[1] * DT);
        }
        self.port_from_records();
    }

    /// `Boarder_SetState` 0x11f608 for the ground, landing, air, rail and wipe-out states, with their enters/exits:
    /// - `RailSlideControl_Exit` 0x1075f0 (when leaving 0xf): clears boost (`Boarder_UpdateBoost(0, 0)`) and finishes
    ///   any queued quarter-turn clip (`RailSlideControl_PlayQueuedClip` 0x107628).
    /// - `CruiseControl_Enter` 0x102fb0: clears cruise state and `rider+0x1dc`.
    /// - `PrewindControl_Enter` 0x105070: steer & brake target 0 at 6.6666665/s, `prewind_started = 0`.
    /// - `NaturalAirControl_Enter` 0x1000b8: brake target 0 at 4.0/s, `natural_air_ctl.steer = steer`.
    /// - `RailSlideControl_Enter` 0x106dc8: clears rail control state, zeroes rail spin (`Score_UpdateSpin(0)`), sets
    ///   brake target 0, and swaps `rail_kind` `1 <-> 2` for alpine goofy riders.
    /// - `SpinControl_Enter` 0x100598: enters `spin_enter_game` if no spin state is active yet.
    /// - `WipeOutControl_Enter` 0x108110: no record mutation.
    pub fn set_control(&mut self, s: u32) {
        if s == self.ctl {
            return;
        }
        if self.ctl == RAIL {
            // `RailSlideControl_Exit` 0x1075f0
            self.boost_lvl = 0.0;
            self.boosting = false;
            if self.rail_ctl.queued != 0 {
                let mut rr = self.rail_rider();
                let mut calls = Vec::new();
                rail_control::play_queued_clip(&mut self.rail_ctl, &mut rr, &mut calls);
                self.from_rail_rider(&rr);
            }
            self.rail_turn_t = 0.0;
        }
        self.ctl = s;
        match s {
            CRUISE => {
                self.cruise = control::Cruise::default();
                self.jump_c = 0.0;
            }
            PREWIND => {
                let r = f32::from_bits(0x40d5_5555);
                self.rec.steer[1..].copy_from_slice(&[r, 0.0]);
                self.rec.brake[1..].copy_from_slice(&[r, 0.0]);
                self.prewind_started = 0;
                if let Some(a) = &self.anim {
                    a.blend_out_all();
                    a.play_clip(0x217, 0, 5, &mut crate::rand::rand_game);
                }
            }
            NATURAL_AIR => {
                self.records_from_port();
                self.rec.brake[1..].copy_from_slice(&[4.0, 0.0]);
                self.natural_air_ctl.steer = self.rec.steer[0];
                self.spin_st = None;
            }
            RAIL => {
                self.records_from_port();
                self.rail_ctl = RailControl::default();
                self.rail_turn_t = 0.0;
                self.sc.update_spin(0);
                let mut f = fin(&self.control_rider(), self.boost_lvl);
                finish::set_brake_target(0.0, &mut f);
                self.rec.brake = [f.brake, f.brake_rate, f.brake_target];
                self.brake_v = f.brake;
                if self.stats.kind == 2 && self.data.goofy {
                    if self.rail_kind == 1 {
                        self.rail_kind = 2;
                    } else if self.rail_kind == 2 {
                        self.rail_kind = 1;
                    }
                }
                if let Some(a) = &self.anim {
                    a.blend_out_all();
                    let clip = match self.rail_kind { 4 => 0x257, 3 => 0x256, 2 => 0x255, _ => 0x254 };
                    a.play_clip(clip, 0, 5, &mut crate::rand::rand_game);
                }
            }
            SPIN => {
                if self.spin_st.is_none() {
                    self.spin_enter_game();
                }
            }
            _ => {}
        }
    }

    fn rail_rider(&self) -> RailRider {
        let alp = self.stats.kind == 2 && self.data.goofy;
        let stance = if alp { (!self.switch) as u32 } else { self.switch as u32 };
        RailRider {
            kind: self.rail_kind,
            stance,
            x46d4: stance,
            x46e0: 0.0,
            crouch: self.rec.crouch[0],
            class: self.stats.kind as u32,
            stats40: self.data.goofy as u32,
        }
    }

    fn from_rail_rider(&mut self, rr: &RailRider) {
        self.rail_kind = rr.kind;
        let alp = self.stats.kind == 2 && self.data.goofy;
        self.set_switch(if alp { rr.stance == 0 } else { rr.stance != 0 });
    }

    /// The state's rider: the records, the velocity and the board's forward in the game's cm and axes
    fn control_rider(&self) -> control::Rider {
        let [b, c, s, sp, fl] = [self.rec.brake, self.rec.crouch, self.rec.steer, self.rec.spin, self.rec.flip];
        control::Rider {
            vel: ground_tick::to_game(self.vel, 100.0, 0.0),
            board: ground_tick::to_game(self.forward(), 1.0, 0.0),
            brake: b[0],
            brake_rate: b[1],
            brake_target: b[2],
            crouch: c[0],
            crouch_rate: c[1],
            crouch_target: c[2],
            steer: s[0],
            steer_rate: s[1],
            steer_target: s[2],
            x220: sp[0],
            x224: sp[1],
            x228: sp[2],
            x22c: fl[0],
            x230: fl[1],
            x234: fl[2],
            lean: self.lean[0],
            lean_rate: self.lean[1],
            lean_target: self.lean[2],
            air: self.air_time,
            surface: if self.rail.is_some() {
                if self.rail_word != 0 { self.rail_word } else { self.surface.max(1) as u32 }
            } else {
                self.surface as u32
            },
            // STANDIN (row G1i2): the finish (+0x418) as not yet crossed; the app's own finish takes the rider over
            finish: -1,
            class: self.stats.kind as u32,
            control: self.ctl,
            trick: -1,
            filter: u32::MAX,
            goofy: self.data.goofy,
            ..Default::default()
        }
    }

    fn from_control_rider(&mut self, r: &control::Rider) {
        self.rec.brake = [r.brake, r.brake_rate, r.brake_target];
        self.rec.crouch = [r.crouch, r.crouch_rate, r.crouch_target];
        self.rec.steer = [r.steer, r.steer_rate, r.steer_target];
        self.rec.spin = [r.x220, r.x224, r.x228];
        self.rec.flip = [r.x22c, r.x230, r.x234];
        self.lean = [r.lean, r.lean_rate, r.lean_target];
    }

    /// `NaturalAirControl_Update` 0x1000e0 (state `0xd`): settles brake/steer/lean/windups and handles air crouch &
    /// reset button (`pad & 2`), transitioning to `SPIN` (`0x10`) once settled.
    pub(super) fn natural_air_control(&mut self, input: &Input) {
        self.read_pad(input);
        let map = self.button_map();
        let mut trick = 0x1a;
        let mut slope = false;
        let pad = get_controls(
            ControlsIn { state: NATURAL_AIR, paused: false },
            &mut Mapper { map: &map, pad: &self.pad_dev },
            &mut trick,
            &mut || 0,
            &mut slope,
        );
        let tricky = self.tricky();
        let (mut meter, mut level) = (self.boost, self.boost_lvl);
        let mut speech = std::mem::take(&mut self.speech);
        let anim = self.anim.clone();
        let mut env = Env { meter: &mut meter, boost_level: &mut level, tricky, speech: &mut speech, clip_id: 0, anim: anim.as_ref(), state: None, jumped: false, reset: None };
        let mut cr = self.control_rider();
        let mut na = self.natural_air_ctl;
        na.update(&mut env, pad, &mut cr);
        self.natural_air_ctl = na;
        let state = env.state;
        if env.reset.is_some() {
            // `NaturalAirControl_Update` does not write +0x304 before `Boarder_RequestReset`
            self.reset_req = Some(None);
        }
        self.speech = speech;
        self.from_control_rider(&cr);
        (self.boost, self.boost_lvl) = (meter, level);
        self.port_from_records();
        if let Some(s) = state {
            self.set_control(s);
        }
    }

    /// `SpinState_UpdateTrickClips` 0x1272e8 hosted on `Rider::trick_ctl` (`TRICK_SLOTS[ch][board]`, 0x333098).
    /// Runs every tick `SpinState_Update` 0x100908 emits `SpinCall::TrickClips(held, trick)`: starts any of the
    /// rider's 15 grab combos, scores grab holds (`Score_StartGrab` 0x155ee8), promotes to tweak (`stage = 1`) or
    /// uber (`stage = 4`, when `uber_ready()` and the row has an uber clip `!= 0x300`), scores releases / ubers
    /// (`Scoring_OnTrickEvent`), and syncs the port's animation fields (`grab`, `grab_hold`, `grab_out`, `uber`, ...).
    pub(super) fn trick_clips_step(&mut self, held: u32, trick: u32) -> u32 {
        let ch = RIDERS.iter().position(|r| r.name == self.data.name).unwrap_or(0);
        let board = (self.stats.kind as usize).min(2);
        let rows = &tricky_data::trick_slots::TRICK_SLOTS[ch][board];
        let was_active = self.trick_ctl.clip_type != 8;
        let mut tc = self.trick_ctl;

        if tc.stage == 3 {
            if !(0.0 < self.vel.y * 100.0) {
                self.trick_ctl = tc;
                self.sync_trick_port();
                return 0;
            }
            tc.stage = 0;
        }
        let speed = (F(self.stats.bytes[12] as f32) * F(f32::from_bits(0x3f4e_47d2)) * F(f32::from_bits(0x3b80_8081))
            + F(f32::from_bits(0x3f48_390a)))
            .0;

        let ret = 'step: {
            if tc.trick_id == 0x1a {
                if trick != 0x1a {
                    let t = tc.clip_type;
                    let go = if t == 0xe || t == 0xf {
                        tc.flag(3) || !tc.flag(1)
                    } else if t == 0x10 {
                        tc.flag(3)
                    } else {
                        true
                    };
                    if go {
                        if let Some(i) = rows.iter().position(|r| r[0] as u32 == trick) {
                            let clip = rows[i][1] as u32;
                            let cm = self.clip_markers_for(clip);
                            tc.begin(clip, 0xe, i, speed, cm.markers, cm.frames as f32, true);
                        }
                        tc.trick_id = trick;
                        tc.hold_ticks = 0;
                        tc.stage = 0;
                        break 'step 1;
                    }
                }
            } else {
                if tc.flag(2) {
                    if tc.hold_ticks == 0 && tc.clip != 0x300 {
                        self.sc.start_grab(tc.clip);
                        if let Some(i) = tc.row {
                            self.grab_done[i] = true;
                        }
                    }
                    tc.hold_ticks = tc.hold_ticks.wrapping_add(1);
                }
                if tc.stage == 4 && tc.clip_type == 0x10 && !tc.flag(3) {
                    if tc.bit(1) {
                        let d = tc.clip_duration();
                        self.sc.end_grab(d, tc.clip);
                        if !tc.uber_scored {
                            tc.uber_scored = true;
                            self.ubers_done = self.ubers_done.saturating_add(1);
                        }
                    }
                    break 'step 1;
                }
                let same = tc.trick_id == trick;
                let end = if held == (tc.stage != 0) as u32 {
                    if same {
                        break 'step 1;
                    }
                    true
                } else {
                    !(same && tc.stage == 0)
                };
                if end {
                    let t = tc.clip_type;
                    if t == 0xf {
                        tc.stop = None;
                    } else if t == 0xe {
                        if !tc.flag(2) {
                            if !tc.flag(1) {
                                tc.clip = 0x300;
                                tc.clip_type = 8;
                                tc.row = None;
                                tc.frame = 0.0;
                                tc.past = 0;
                                tc.crossed = 0;
                            } else if tc.clip != 0x300 {
                                self.sc.start_grab(tc.clip);
                                if let Some(i) = tc.row {
                                    self.grab_done[i] = true;
                                }
                            }
                        }
                        tc.stop = None;
                    }
                    tc.stage = 0;
                    tc.hold_ticks = 0;
                    tc.trick_id = 0x1a;
                } else if tc.clip_type == 0xe && tc.flag(2) {
                    if let Some(i) = rows.iter().position(|r| r[1] as u32 == tc.clip) {
                        let uber = rows[i][3] as u32;
                        if 0.0 < self.uber_timer && uber != 0x300 {
                            let row = &self.data.rows[i];
                            let sig = crate::trickdata::signature_grab(self.data.name) == Some(row.grab);
                            if let Some(uid) = self.ubers.iter().position(|u| {
                                (!row.uber_clip.is_empty() && uber_grab(&u.0) == uber_grab(row.uber_clip) && !u.0.contains("UT_SIG"))
                                    || (sig && u.0.contains("UT_SIG"))
                            }) {
                                self.uber_id = uid;
                            }
                            let cm = self.clip_markers_for(uber);
                            let len = (cm.frames as f32).max(4.0);
                            tc.begin(uber, 0x10, i, 1.0, cm.markers, len, false);
                            if let Some(rate) = crate::score::grab_rate(uber) {
                                self.sc.grab_rate = rate;
                            }
                            tc.stage = 4;
                        } else {
                            let tweak = rows[i][2] as u32;
                            let cm = self.clip_markers_for(tweak);
                            tc.begin(tweak, 0xf, i, speed, cm.markers, cm.frames as f32, true);
                            // start tweak at hold frame so release plays smoothly from peak
                            if let Some(m2) = cm.marker(2) {
                                tc.frame = m2;
                            }
                            if let Some(rate) = crate::score::grab_rate(tweak) {
                                self.sc.grab_rate = rate;
                            }
                            self.grab_tweaked[i] = true;
                            tc.stage = 1;
                        }
                    }
                }
            }
            let t = tc.clip_type;
            if t == 0xe || t == 0xf {
                if tc.bit(2) {
                    self.sc.end_grab(0.0, tc.clip);
                }
                break 'step 1;
            }
            if t != 0x10 {
                break 'step 0;
            }
            if tc.bit(1) {
                let d = tc.clip_duration();
                self.sc.end_grab(d, tc.clip);
                if !tc.uber_scored {
                    tc.uber_scored = true;
                    self.ubers_done = self.ubers_done.saturating_add(1);
                }
            }
            1
        };

        self.trick_ctl = tc;
        if was_active || tc.clip_type != 8 {
            self.sync_trick_port();
        }
        ret
    }

    /// Mirror `Rider::trick_ctl` into the port's animation fields (`grab`, `grab_hold`, `grab_out`, `uber`, ...).
    pub(super) fn sync_trick_port(&mut self) {
        let tc = self.trick_ctl;
        match tc.clip_type {
            0xe | 0xf => {
                let idx = tc.row.map_or(0, |i| (i + 1) as u8);
                self.uber = 0.0;
                self.grab_tw = tc.clip_type == 0xf;
                if tc.trick_id != 0x1a {
                    self.grab = idx;
                    self.grab_hold = tc.frame;
                    self.grab_out = None;
                    self.grab_scored = tc.hold_ticks > 0;
                } else {
                    self.grab = 0;
                    self.grab_hold = 0.0;
                    self.grab_out = (idx != 0).then_some((idx, tc.frame));
                    self.grab_scored = false;
                }
            }
            0x10 => {
                self.grab = 0;
                self.grab_hold = 0.0;
                self.grab_out = None;
                self.grab_tw = false;
                self.grab_scored = false;
                self.uber = tc.frame.max(0.01);
            }
            _ => {
                self.grab = 0;
                self.grab_hold = 0.0;
                self.grab_out = None;
                self.grab_tw = false;
                self.grab_scored = false;
                self.uber = 0.0;
            }
        }
    }

    /// After the snow's motion, `cBoarder::vf7`'s control part: the pad word for the state (`cPlayer::GetControls`
    /// on the rider's button map, [`Rider::button_map`]) and the state's update (`Cruise` 3, `Prewind` 0xe, `Jump` 10,
    /// `LandNormal` 0xc, `LandHard` 0xb). A rider back on the snow in another state is put in cruise (prewind with the
    /// jump button held). Returns true when the rider jumped (`JumpControl_Update`'s `Jump_ApplyImpulse`, motion 1,
    /// `Takeoff_SetSpinRates`).
    pub(super) fn ground_control(&mut self, input: &Input) -> bool {
        if !matches!(self.ctl, CRUISE | PREWIND | JUMP | LAND_NORMAL | LAND_HARD) {
            self.set_control(if input.jump { PREWIND } else { CRUISE });
        }
        self.read_pad(input);
        let map = self.button_map();
        let mut trick = 0x1a;
        let mut slope = false;
        let mut pad = get_controls(ControlsIn { state: self.ctl, paused: false }, &mut Mapper { map: &map, pad: &self.pad_dev }, &mut trick, &mut || 0, &mut slope);
        // STANDIN: the app's steer and wind-up as the word's six-bit sticks, past the pad's dead zone (0.38): the
        // computer riders and the autopilot steer by fractions (`cComputer::GetControls` 0x1384b8 writes its word
        // directly too); keys give the same as the pad (cruise/landing: steer at bit 5; prewind: turn at 17, spin
        // and flip wind-ups at 5 and 11)
        let six = |a: f32| ((a.clamp(-1.0, 1.0) * 31.0) as i32 as u32) & 0x3f;
        let put = |w: &mut u32, shift: u32, v: u32| *w = *w & !(0x3f << shift) | v << shift;
        match self.ctl {
            CRUISE | LAND_NORMAL | LAND_HARD => put(&mut pad, 5, six(-input.steer)),
            PREWIND => {
                put(&mut pad, 17, six(-input.steer));
                put(&mut pad, 5, six(input.wind.unwrap_or(input.steer)));
                put(&mut pad, 11, six(-input.flip));
            }
            _ => {}
        }
        let tricky = self.tricky();
        let (mut meter, mut level) = (self.boost, self.boost_lvl);
        let mut speech = std::mem::take(&mut self.speech);
        let clip_id = if self.ctl == LAND_HARD && self.hard_land <= 0.0 { 0x1ff } else { 0x23e };
        let anim = self.anim.clone();
        let mut env = Env { meter: &mut meter, boost_level: &mut level, tricky, speech: &mut speech, clip_id, anim: anim.as_ref(), state: None, jumped: false, reset: None };
        let mut cr = self.control_rider();
        match self.ctl {
            CRUISE => {
                let mut c = self.cruise;
                c.update(&mut env, pad, &mut cr);
                self.cruise = c;
            }
            JUMP => control::jump(&mut env, pad, &mut cr),
            LAND_HARD => control::land_hard(&mut env, pad, &mut cr),
            LAND_NORMAL => {
                let mut ln = self.land_normal_ctl;
                ln.update(&mut env, pad, &mut cr);
                self.land_normal_ctl = ln;
            }
            _ => {}
        }
        let (state, jumped) = (env.state, env.jumped);
        if let Some(f) = env.reset {
            self.reset_req = Some(Some(f));
        }
        self.speech = speech;
        if self.ctl == PREWIND {
            let mut p = Prewind {
                f: Fields { meter, boost_level: level, x1fc: cr.brake, crouch: cr.crouch, crouch_rate: cr.crouch_rate, crouch_target: cr.crouch_target, ..Default::default() },
                started: self.prewind_started,
                kind: if self.rail.is_some() { self.rail_kind } else { 0 },
                x418: -1,
                spin: self.rec.spin,
                flip: self.rec.flip,
                x1c8: self.jump_c,
                vel: cr.vel,
                board: cr.board,
                steer: cr.steer,
                steer_rate: cr.steer_rate,
                steer_target: cr.steer_target,
                surface: cr.surface,
                switch: self.switch as u32,
                class: cr.class,
                x304: 0,
                // (only "more than five" is read: `Boarder_IsTricky`)
                ubers: if tricky { 6 } else { 0 },
                windup_stat: self.stats.bytes[crate::stats::WINDUP],
            };
            let calls = boarder::prewind_update(pad, true, &mut p);
            (meter, level) = (p.f.meter, p.f.boost_level);
            (cr.crouch, cr.crouch_rate, cr.crouch_target) = (p.f.crouch, p.f.crouch_rate, p.f.crouch_target);
            (cr.steer, cr.steer_rate, cr.steer_target) = (p.steer, p.steer_rate, p.steer_target);
            self.from_control_rider(&cr);
            (self.rec.spin, self.rec.flip, self.jump_c, self.prewind_started) = (p.spin, p.flip, p.x1c8, p.started);
            for c in calls {
                match c {
                    PrewindCall::State(s) => self.set_control(s),
                    PrewindCall::Boost(boarder::Call::BoostEmpty) => self.speech.push(RiderSpeech::BoostEmpty),
                    // `Boarder_RequestReset` after `PrewindState_Update`'s store of +0x304 (0x105580)
                    PrewindCall::Reset => self.reset_req = Some(Some(p.x304)),
                    PrewindCall::Clip(clip) => {
                        if let Some(a) = &self.anim {
                            a.play_clip(clip, 0, 5, &mut crate::rand::rand_game);
                        }
                    }
                    _ => {}
                }
            }
        } else {
            self.from_control_rider(&cr);
        }
        (self.boost, self.boost_lvl) = (meter, level);
        if let Some(s) = state {
            self.set_control(s);
        }
        self.port_from_records();
        // (the port's crouch kept for the jump: its prewind pose and the air's late release read it)
        self.charge = if self.ctl == PREWIND { self.crouch_v } else { 0.0 };
        if jumped {
            // `Takeoff_SetSpinRates` clears the wind-ups once their rates are taken (`Rider::take_off` keeps them)
            self.rec.spin = [0.0; 3];
            self.rec.flip = [0.0; 3];
        }
        jumped
    }

    /// After the rail's motion (`RailSlideMotion_Update` 0x10b0d0), `cBoarder::vf7`'s control part on a rail:
    /// `RailSlideControl_Update` 0x1073e0 (state `0xf`), handing off to `PrewindState_Update` 0x1050e8 (`0xe`) and
    /// `JumpControl_Update` 0x1046b0 (`10`) once the jump button is pressed. Returns true when `JumpControl_Update`
    /// fires the rail hop (`Jump_ApplyImpulse`, motion 1, `Takeoff_SetSpinRates`).
    pub(super) fn rail_control(&mut self, input: &Input, dt: f32) -> bool {
        if matches!(self.ctl, PREWIND | JUMP) {
            return self.ground_control(input);
        }
        if self.ctl != RAIL {
            self.set_control(RAIL);
        }
        self.read_pad(input);
        let map = self.button_map();
        let mut trick = 0x1a;
        let mut slope = false;
        let mut pad = get_controls(
            ControlsIn { state: RAIL, paused: false },
            &mut Mapper { map: &map, pad: &self.pad_dev },
            &mut trick,
            &mut || 0,
            &mut slope,
        );
        // STANDIN: keyboard/gamepad input into state 0xf's six-bit sticks (`axis(4)` steer at bit 5, `axis(7)`
        // quarter-turn at bit 17 when boost + steer is held past 0.5, matching `BTNMAP0.DAT`'s d-pad `RailSpin`).
        let six = |a: f32| ((a.clamp(-1.0, 1.0) * 31.0) as i32 as u32) & 0x3f;
        let put = |w: &mut u32, shift: u32, v: u32| *w = *w & !(0x3f << shift) | v << shift;
        put(&mut pad, 5, six(-input.steer));
        put(&mut pad, 17, if input.boost && input.steer.abs() > 0.5 { six(-input.steer) } else { 0 });

        if self.rail_ctl.queued != 0 {
            self.rail_turn_t = (self.rail_turn_t - dt).max(0.0);
        }
        let mut rc = self.rail_ctl;
        let mut rr = self.rail_rider();
        let turn_done = match &self.anim {
            Some(a) => a.at_end(0),
            None => self.rail_turn_t <= 0.0,
        };
        let calls = rail_control::update(&mut rc, &mut rr, pad, &mut || turn_done);
        self.rail_ctl = rc;
        self.from_rail_rider(&rr);
        let mut next_state = None;
        for c in calls {
            match c {
                RailCall::Boost(b) => {
                    let mut f = Fields { meter: self.boost, boost_level: self.boost_lvl, ..Default::default() };
                    // `RailSlideControl_Update` 0x1073e0 calls `Boarder_UpdateBoost(rider, (pad >> 4) & 1, 0)`
                    let _ = boarder::update_boost(b != 0, false, true, self.tricky(), &mut f);
                    (self.boost, self.boost_lvl) = (f.meter, f.boost_level);
                }
                RailCall::SteerTarget(bits) => {
                    let (t, rate) = boarder::set_steer_target_unscaled(f32::from_bits(bits), self.rec.steer[0]);
                    self.rec.steer[1] = rate;
                    self.rec.steer[2] = t;
                }
                RailCall::CrouchTarget(bits) => {
                    let mut f = Fields { x1fc: self.rec.brake[0], crouch: self.rec.crouch[0], crouch_rate: self.rec.crouch[1], crouch_target: self.rec.crouch[2], ..Default::default() };
                    boarder::set_crouch_target(f32::from_bits(bits), &mut f);
                    (self.rec.crouch[1], self.rec.crouch[2]) = (f.crouch_rate, f.crouch_target);
                }
                RailCall::Prewind => next_state = Some(PREWIND),
                RailCall::TurnLayers(bits) => {
                    let angle = f32::from_bits(bits);
                    self.rail_goal += angle;
                    if let Some(a) = &self.anim {
                        a.turn_all_layers(angle);
                    }
                    self.rail_turn_t = 28.0 / 60.0;
                }
                RailCall::ScoreSpin(deg) => self.sc.update_spin(deg),
                // `RailSlideControl_Update` does not write +0x304 before `Boarder_RequestReset`
                RailCall::Reset => self.reset_req = Some(None),
                RailCall::Anim15ef48 => {
                    if let Some(a) = &self.anim {
                        a.set_stance(0);
                        a.set_all_layers_stance(0);
                    }
                }
                RailCall::PlayClip(clip) => {
                    if let Some(a) = &self.anim {
                        a.play_clip(clip, 0, 5, &mut crate::rand::rand_game);
                    }
                }
                RailCall::ClipAtEnd | RailCall::OnClipQueued => {}
            }
        }
        self.port_from_records();
        self.charge = 0.0;
        if let Some(s) = next_state {
            self.set_control(s);
        }
        false
    }

    /// After the wipe-out or get-up motion (`WipeoutMotion_Update` 0x10c2f0 / `GetUpMotion_Update` 0x1102c8),
    /// `cBoarder::vf7`'s control part: state `0x13` (`WipeOutControl`, inline reset check in `Boarder_UpdateControl`
    /// 0x11c9a0) or `0x14` (`GetUpState_Update` 0x108198).
    pub(super) fn wipeout_control(&mut self, input: &Input) {
        if !matches!(self.ctl, WIPEOUT | GET_UP) {
            self.set_control(WIPEOUT);
        }
        self.read_pad(input);
        let map = self.button_map();
        let mut trick = 0x1a;
        let mut slope = false;
        let pad = get_controls(
            ControlsIn { state: self.ctl, paused: false },
            &mut Mapper { map: &map, pad: &self.pad_dev },
            &mut trick,
            &mut || 0,
            &mut slope,
        );
        let tricky = self.tricky();
        let (mut meter, mut level) = (self.boost, self.boost_lvl);
        let mut speech = std::mem::take(&mut self.speech);
        let clip_id = self.wipe.as_ref().map_or(0x1ff, |w| w.clip as i32);
        let anim = self.anim.clone();
        let mut env = Env { meter: &mut meter, boost_level: &mut level, tricky, speech: &mut speech, clip_id, anim: anim.as_ref(), state: None, jumped: false, reset: None };
        let mut cr = self.control_rider();
        if self.ctl == GET_UP {
            control::get_up_state(&mut env, pad, &mut cr);
        } else {
            control::update_control(&mut env, pad, &mut cr);
        }
        let state = env.state;
        if let Some(f) = env.reset {
            self.reset_req = Some(Some(f));
        }
        self.speech = speech;
        if let Some(s) = state {
            self.set_control(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn natural_air_settles_enters_spin_and_handles_reset() {
        let mut r = Rider::new(Vec3::ZERO, 0.0);
        r.grounded = false;
        r.motion = 1;
        r.steer = -0.5;
        r.rec.steer = [0.5, 0.0, 0.5];
        r.set_control(NATURAL_AIR);
        assert_eq!(r.ctl, NATURAL_AIR);
        assert_eq!(r.natural_air_ctl.steer, 0.5);
        assert!(r.spin_st.is_none());

        // Ease steer/brake back to 0 and tick NaturalAirControl_Update until it transitions into SPIN (0x10).
        let idle = Input::default();
        for _ in 0..30 {
            r.ease_records();
            if r.ctl == NATURAL_AIR {
                r.natural_air_control(&idle);
            }
        }
        assert_eq!(r.ctl, SPIN, "NaturalAir should settle into SPIN (0x10)");
        assert!(r.spin_st.is_some(), "entering SPIN should initialize spin_st");

        // Reset button in NaturalAir sets reset_req = Some(None) (no +0x304 write).
        r.set_control(NATURAL_AIR);
        r.reset_req = None;
        r.natural_air_control(&Input { reset: true, ..Default::default() });
        assert_eq!(r.reset_req, Some(None));
    }

    #[test]
    fn trick_clips_runs_grab_tweak_uber_and_release_scoring() {
        let mut r = Rider::new(Vec3::ZERO, 0.0);
        r.grounded = false;
        r.motion = 1;
        r.set_control(SPIN);
        r.begin_chain(0, false);

        // Hold grab combo 0 (L2 -> bit 4 in Input::grab) -> enters grab clip (0xe), reaches marker 2 (m2=20 for
        // Eddie's bxT_STALEFISH at speed ~0.87 -> ~47 ticks at 60 Hz), scores grab.
        let grab_in = Input { grab: 4, ..Default::default() };
        for _ in 0..55 {
            r.spin_tick(&grab_in, 1.0 / 60.0);
            r.sc.tick(1000.0);
        }
        assert_eq!(r.trick_ctl.clip_type, 0xe);
        assert!(r.trick_ctl.flag(2), "grab should reach hold marker 2");
        assert!(r.trick_ctl.hold_ticks > 0);
        assert!(r.grab != 0 && r.grab_scored);
        assert!(!r.grab_safe(), "held grab must not be safe to land");

        // Press tweak while holding grab (no uber window) -> promotes to tweak (0xf, stage 1).
        let tweak_in = Input { grab: 4, tweak: true, ..Default::default() };
        r.spin_tick(&tweak_in, 1.0 / 60.0);
        assert_eq!(r.trick_ctl.clip_type, 0xf);
        assert_eq!(r.trick_ctl.stage, 1);
        assert!(r.grab_tw);

        // Release both -> stops tweak, scores TRICK_EVENT, plays out past marker 4 -> grab_safe() becomes true.
        let idle = Input::default();
        for _ in 0..40 {
            r.spin_tick(&idle, 1.0 / 60.0);
        }
        assert_eq!(r.trick_ctl.trick_id, 0x1a);
        assert!(r.grab_safe(), "released grab/tweak past marker 4 must be safe to land");
        assert_ne!(r.sc.grabs[0], 0, "grab/tweak release must record in score.grabs");

        // With uber window open, holding grab combo 0 + tweak promotes to uber (0x10, stage 4) and plays through.
        r.uber_timer = 15.0;
        r.ubers_done = 0;
        for _ in 0..55 {
            r.spin_tick(&grab_in, 1.0 / 60.0);
        }
        r.spin_tick(&tweak_in, 1.0 / 60.0);
        assert_eq!(r.trick_ctl.clip_type, 0x10);
        assert_eq!(r.trick_ctl.stage, 4);
        assert!(!r.grab_safe(), "in-progress uber must not be safe to land");
        // Release buttons: uber still plays all the way through, increments ubers_done, and becomes safe.
        for _ in 0..130 {
            r.spin_tick(&idle, 1.0 / 60.0);
        }
        assert_eq!(r.ubers_done, 1, "completed uber must score and increment ubers_done");
        assert!(r.grab_safe(), "completed uber must be safe to land");
    }

    #[test]
    fn landing_normal_and_hard_control_states_transition_to_cruise_or_prewind() {
        let mut r = Rider::new(Vec3::ZERO, 0.0);
        r.grounded = true;
        r.motion = 2;

        // LandNormal (0xc): eases crouch up to land_normal_ctl.crouch then switches to CRUISE (3).
        r.land_normal_ctl.crouch = 0.5;
        r.rec.crouch = [0.0, 0.0, 0.0];
        r.crouch_v = 0.0;
        r.set_control(LAND_NORMAL);
        assert_eq!(r.ctl, LAND_NORMAL);
        for _ in 0..15 {
            r.ease_records();
            r.ground_control(&Input::default());
        }
        assert_eq!(r.ctl, CRUISE, "LandNormal should transition to CRUISE once crouched");

        // LandHard (0xb): holds 0xb while hard_land > 0, transitions to PREWIND (0xe) when clip ends with jump held.
        r.hard_land = 0.5;
        r.set_control(LAND_HARD);
        r.ground_control(&Input { jump: true, ..Default::default() });
        assert_eq!(r.ctl, LAND_HARD, "LandHard must hold while clip plays");
        r.hard_land = 0.0;
        r.ground_control(&Input { jump: true, ..Default::default() });
        assert_eq!(r.ctl, PREWIND, "LandHard should transition to PREWIND when jump is held at clip end");
    }

    #[test]
    fn rail_control_boosts_quarter_turns_prewinds_hops_and_resets() {
        let mut r = Rider::new(Vec3::ZERO, 0.0);
        r.rail = Some((0, 0.0, 10.0));
        r.rail_kind = 1;
        r.vel = Vec3::new(0.0, 0.0, -10.0);
        r.boost = 0.8;
        r.set_control(RAIL);
        assert_eq!(r.ctl, RAIL);

        // Boosting on a rail sets boost_lvl > 0 and drains meter (`Boarder_UpdateBoost(1, 0)`).
        let b0 = r.boost;
        assert!(!r.rail_control(&Input { boost: true, ..Default::default() }, 1.0 / 60.0));
        assert!(r.boost_lvl > 0.0 && r.boost < b0, "rail boost should set boost_lvl and drain meter");

        // Boost + right steer triggers a quarter turn (`RailSlideControl_CommitQuarterTurn`): queues clip, rotates
        // rail_goal by +pi/2, and scores +90 deg; after 28 ticks `play_queued_clip` updates rail_kind 1 -> 4.
        let g0 = r.rail_goal;
        r.rail_control(&Input { boost: true, steer: 1.0, ..Default::default() }, 1.0 / 60.0);
        assert_eq!(r.rail_ctl.queued, 1);
        assert_eq!(r.rail_ctl.spin, 90);
        assert!((r.rail_goal - (g0 + std::f32::consts::FRAC_PI_2)).abs() < 1e-4);
        for _ in 0..30 {
            r.rail_control(&Input::default(), 1.0 / 60.0);
        }
        assert_eq!(r.rail_ctl.queued, 0);
        assert_eq!(r.rail_kind, 4, "quarter turn right from kind 1 should settle into kind 4");

        // Pressing jump enters PREWIND (0xe), holding winds up, releasing enters JUMP (10) and hops into SPIN (0x10).
        r.rail_control(&Input { jump: true, ..Default::default() }, 1.0 / 60.0);
        assert_eq!(r.ctl, PREWIND, "jump press on rail should enter PREWIND");
        for _ in 0..20 {
            r.ease_records();
            assert!(!r.rail_control(&Input { jump: true, steer: 1.0, ..Default::default() }, 1.0 / 60.0));
        }
        assert!(r.crouch_v > 0.5 && r.wind > 0.3);
        let mut hopped = false;
        for _ in 0..20 {
            r.ease_records();
            if r.rail_control(&Input::default(), 1.0 / 60.0) {
                hopped = true;
                break;
            }
        }
        assert!(hopped, "releasing jump on rail should hop via Prewind -> Jump");
        assert_eq!(r.ctl, SPIN);

        // Reset button on a rail sets reset_req = Some(None) (no +0x304 store).
        r.set_control(RAIL);
        r.reset_req = None;
        r.rail_control(&Input { reset: true, ..Default::default() }, 1.0 / 60.0);
        assert_eq!(r.reset_req, Some(None));
    }

    #[test]
    fn wipeout_and_getup_control_handle_reset_button_and_out_of_bounds() {
        let mut r = Rider::new(Vec3::ZERO, 0.0);
        r.surface = 1;
        r.set_control(WIPEOUT);
        r.reset_req = None;

        // No reset when on-course and reset button not pressed.
        r.wipeout_control(&Input::default());
        assert_eq!(r.reset_req, None);

        // Reset button in WIPEOUT (0x13) sets reset_req = Some(Some(0)) (+0x304 = 0, charges meter).
        r.wipeout_control(&Input { reset: true, ..Default::default() });
        assert_eq!(r.reset_req, Some(Some(0)));

        // Out-of-bounds (surface 0) in WIPEOUT sets reset_req = Some(Some(1)) (+0x304 = 1, free reset).
        r.reset_req = None;
        r.surface = 0;
        r.wipeout_control(&Input::default());
        assert_eq!(r.reset_req, Some(Some(1)));

        // GET_UP (0x14): reset button sets Some(Some(0)); settled clip (0x1ff when wipe is None) transitions to CRUISE.
        r.set_control(GET_UP);
        r.surface = 1;
        r.reset_req = None;
        r.wipeout_control(&Input { reset: true, ..Default::default() });
        assert_eq!(r.reset_req, Some(Some(0)));
        assert_eq!(r.ctl, CRUISE, "GetUpState_Update with clip 0x1ff should transition to CRUISE");
    }
}
