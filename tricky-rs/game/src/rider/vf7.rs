//! `cBoarder::vf7` 0x117198 on the app's rider (row G1i2c): [`Rider::step`] runs [`boarder_tick`] (G1h, checked bit
//! for bit) on a [`TickRider`] view of the rider, its [`TickEnv`] calling `Boarder_UpdateMotion` 0x11c770
//! ([`Rider::motion_update`]), `Boarder_AddSpeedSample` 0x155dc8, `Boarder_UpdateControl` 0x11c9a0
//! ([`Rider::control_update`]), `Boarder_RequestReset` 0x118f10, `Score_Tick` 0x156e80, and
//! `Boarder_UpdateLeanTargets` 0x129500 in the game's exact order.
//!
//! The view: the port keeps most of the boarder in its own units (m, y up); each call of the env writes back the
//! fields `boarder_tick` changed since it last read them (so a field it did not touch is not rounded through cm and
//! back), runs, and reads the view again. The boarder's fields the port does not keep (the weather +0x18, the
//! distance +0x164, the pitch +0x168, the records +0x1f0, +0x268, +0x250, +0x25c and the rates and targets of +0x284,
//! +0x274..+0x280, +0x30c, the events) live in [`Rider::vf7`].

use super::{control_tick, ground_tick, placed_heading, Input, Rider};
use crate::boarder_tick::{boarder_tick, TickEnv, TickGame, TickRider};
use crate::collide::CollisionWorld;
use crate::orient;
use crate::rails::Rails;
use crate::surface::SURFACE_TABLE;

type V4 = [f32; 4];

/// `cBoarder::vf7` 0x117198 for one step of the rider. `resetting`: the reset state (motion 4, state 0x16) was active
/// entering this step (its motion does nothing and its control is `ResetState_Update` 0x106a90).
pub(super) fn tick(r: &mut Rider, world: &CollisionWorld, rails: &Rails, raw_input: Input, ctl_input: Input, dt: f32, resetting: bool) {
    // gWorld+0xc, the world's lowest height. STANDIN without the course's files (a levels folder): 30 m under the
    // drawn mesh's lowest point, as [`Rider::below_world`]
    let world_floor = match &world.course {
        Some(c) => c.floor,
        None => (world.min_y - 30.0) * 100.0 + 10000.0,
    };
    let row = SURFACE_TABLE[(r.surface as usize).min(19)];
    let game = TickGame {
        // inferred: the mode the host keeps for `Boarder_RequestReset` (0x32de8c) taken as game+0x1c -> +0x1c; it only
        // picks calls the host makes itself (the race line, the riders' collisions)
        mode: r.reset_ctx.mode,
        // STANDIN: the snow's density (game+0x78) is the host's (`main.rs`'s snow), not read into the rider's weather
        density: 0.0,
        world_floor,
        row_1c: row[7],
        row_20: row[8],
        row_28: row[10],
    };
    // the time scale (+0x12c) as the host steps the rider: a computer rider's step is scaled by its own
    r.vf7.time_scale = dt * 60.0;
    let mut t = r.vf7_view();
    let seen = t;
    let mut g = Glue { r, world, rails, raw_input, ctl_input, dt, resetting, seen };
    boarder_tick(&mut g, &game, &mut t);
    g.load(&t);
}

struct Glue<'a> {
    r: &'a mut Rider,
    world: &'a CollisionWorld,
    rails: &'a Rails,
    raw_input: Input,
    ctl_input: Input,
    dt: f32,
    resetting: bool,
    /// the view as the rider last gave it
    seen: TickRider,
}

impl Glue<'_> {
    fn load(&mut self, t: &TickRider) {
        let seen = self.seen;
        self.r.vf7_load(t, &seen);
    }
    /// a call of the env on the rider: what the tick changed written back, the call, the view read again
    fn with(&mut self, t: &mut TickRider, f: impl FnOnce(&mut Self)) {
        self.load(t);
        f(self);
        *t = self.r.vf7_view();
        self.seen = *t;
    }
}

impl TickEnv for Glue<'_> {
    /// STANDIN: `Boarder_GatherWorldEmitters` 0x1166f0 ([`crate::boarder_misc::gather_world_emitters`]) not run: the
    /// app's world sounds do not read its list yet
    fn gather_world_emitters(&mut self, _: &mut TickRider) {}
    /// the host does not step a paused game
    fn is_paused(&mut self) -> bool {
        false
    }
    /// `Boarder_UpdateMotion` 0x11c770
    fn update_motion(&mut self, t: &mut TickRider) {
        self.with(t, |g| g.r.motion_update(g.world, g.rails, g.ctl_input, g.dt));
    }
    /// `Boarder_AddSpeedSample` 0x155dc8
    fn add_speed_sample(&mut self, _: &mut TickRider, speed: f32) {
        crate::control::add_speed_sample(speed, &mut self.r.speed_sum, &mut self.r.speed_samples);
    }
    /// the host's (`aiglue`, the race line and the path ahead of every rider)
    fn update_race_line(&mut self, _: &mut TickRider) {}
    fn update_path_ahead(&mut self, _: &mut TickRider) {}
    /// each state's glue packs its own word (`controls::get_controls` for the state, `control_tick`)
    fn get_controls(&mut self, _: &mut TickRider) -> [u32; 4] {
        [0; 4]
    }
    /// `Boarder_UpdateControl` 0x11c9a0
    fn update_control(&mut self, t: &mut TickRider, _: &[u32; 4]) {
        self.with(t, |g| g.r.control_update(g.world, g.rails, g.raw_input, g.ctl_input, g.dt));
    }
    /// `Boarder_RequestReset` 0x118f10 with the +0x304 = 1 the tick stored (written back by `with`)
    fn request_reset(&mut self, t: &mut TickRider, _: f32) {
        self.with(t, |g| g.r.request_reset(None));
    }
    /// `Score_Tick` 0x156e80 (+ `Boarder_DrainMeter` 0x11b1f8 outside reset, which in `SLUS_203.26` runs per-rider in
    /// the race tick 0x113870)
    fn score_tick(&mut self, t: &mut TickRider) {
        self.with(t, |g| {
            let drain = !g.resetting && g.r.reset.is_none();
            g.r.score_ticks(g.dt, drain);
        });
    }
    /// `Boarder_PickIdleClip` 0x11d4e8 (`boarder_misc::pick_idle_clip`, row G5d2): every 18th tick picks the
    /// layer-2 idle/wind clip (`0x2f0..=0x2f8`) when `AnimHost` is attached.
    fn pick_idle_clip(&mut self, t: &mut TickRider) {
        let Some(a) = &self.r.anim else { return };
        let bf = self.r.board_frame();
        let kind = super::RIDERS.iter().position(|r| r.name == self.r.data.name).unwrap_or(0) as i32;
        let ir = crate::boarder_misc::IdleRider {
            state: t.state,
            vel: t.vel,
            fwd: bf[0],
            side: bf[1],
            up: bf[2],
            kind,
        };
        if let Some(clip) = crate::boarder_misc::pick_idle_clip(&ir, self.r.anim_tick, &mut |layer| a.clip(layer) as i64) {
            a.play_clip(clip, 0, 5, &mut crate::rand::rand_game);
        }
    }
    /// the host's (`main.rs` runs `Boarder_RiderCollisions` 0x123fc8 over every pair of riders)
    fn rider_collisions(&mut self, _: &mut TickRider) {}
    /// the host's (`rivals`)
    fn update_grudge_timeout(&mut self, _: &mut TickRider) {}
    /// `Boarder_UpdateLeanTargets` 0x129500
    fn update_lean_targets(&mut self, t: &mut TickRider) {
        self.with(t, |g| g.r.lean_aim());
    }
    /// the host draws
    fn render_update(&mut self, _: &mut TickRider) {}
}

fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits()
}

fn same4(a: V4, b: V4) -> bool {
    a.iter().zip(b).all(|(x, y)| same(*x, y))
}

impl Rider {
    /// The boarder's fields `cBoarder::vf7` reads, from the port's (cm and z up)
    fn vf7_view(&mut self) -> TickRider {
        let mut t = self.vf7;
        self.records_from_port();
        t.active = 1;
        t.meter = self.boost;
        t.boost_level = self.boost_lvl;
        t.boost_time = self.speed_timer;
        t.spin_boost_time = self.spin_timer;
        t.pos_z = ground_tick::to_game(self.pos, 100.0, 1.0)[2];
        t.vel = ground_tick::to_game(self.vel, 100.0, 0.0);
        let o = match &self.att {
            Some((o, _)) => *o,
            None => orient::placed_attitude(placed_heading(self.yaw), None),
        };
        t.fwd = o.fwd;
        t.right = o.side;
        t.board_up = self.board_frame()[2];
        t.speed_limit = self.cap * 100.0;
        t.eases[1] = self.rec.brake;
        t.eases[2] = self.rec.crouch;
        t.eases[3] = self.rec.steer;
        t.eases[5] = self.rec.spin;
        t.eases[6] = self.rec.flip;
        t.eases[7] = self.lean;
        t.eases[10][0] = self.rail_blend;
        t.surface = self.surface as u32;
        t.x294 = self.band.0 * 100.0;
        t.x298 = self.band.1 * 100.0;
        (t.impact, t.crash_normal, t.knocks) = (self.impact, self.impact_n, self.knocks);
        t.reset_flag = self.reset_flag;
        t.height = self.body_height;
        t.class = self.stats.kind as u32;
        t.motion = self.motion;
        t.state = if self.reset.is_some() { control_tick::RESET } else { self.ctl };
        t
    }

    /// The fields `cBoarder::vf7` changed since `seen`, back into the port's
    fn vf7_load(&mut self, t: &TickRider, seen: &TickRider) {
        self.vf7 = *t;
        if !same(t.meter, seen.meter) {
            self.boost = t.meter;
        }
        if !same(t.boost_level, seen.boost_level) {
            self.boost_lvl = t.boost_level;
        }
        if !same(t.boost_time, seen.boost_time) {
            self.speed_timer = t.boost_time;
        }
        if !same(t.spin_boost_time, seen.spin_boost_time) {
            self.spin_timer = t.spin_boost_time;
        }
        if !same4(t.vel, seen.vel) {
            self.vel = ground_tick::port_dir(t.vel) * 0.01;
            if let Some((_, _, ref mut s)) = self.rail {
                let mag = self.vel.length();
                if mag < s.abs() {
                    *s = s.signum() * mag;
                }
            }
            if let Some(w) = &mut self.wipe {
                w.set_vel(t.vel);
            }
        }
        if !same(t.speed_limit, seen.speed_limit) {
            self.cap = t.speed_limit * 0.01;
        }
        self.rec.brake = t.eases[1];
        self.rec.crouch = t.eases[2];
        self.rec.steer = t.eases[3];
        self.rec.spin = t.eases[5];
        self.rec.flip = t.eases[6];
        self.port_from_records();
        self.lean = t.eases[7];
        self.rail_blend = t.eases[10][0];
        if !same(t.x294, seen.x294) {
            self.band.0 = t.x294 * 0.01;
        }
        if !same(t.x298, seen.x298) {
            self.band.1 = t.x298 * 0.01;
        }
        if !same(t.impact, seen.impact) {
            self.impact = t.impact;
        }
        if !same4(t.crash_normal, seen.crash_normal) {
            self.impact_n = t.crash_normal;
        }
        if !same(t.knocks, seen.knocks) {
            self.knocks = t.knocks;
        }
        self.reset_flag = t.reset_flag;
        self.body_height = t.height;
    }
}
