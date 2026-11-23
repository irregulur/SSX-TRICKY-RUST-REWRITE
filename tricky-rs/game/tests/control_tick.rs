//! The ground's control states in `Rider::step` (row G1i1): cruise, prewind and jump on the game's pad word.

use glam::Vec3;
use tricky_game::collide::CollisionWorld;
use tricky_game::rails::Rails;
use tricky_game::rider::control_tick::{CRUISE, JUMP, PREWIND, RESET, SPIN};
use tricky_game::rider::{Input, Rider};

/// A long slope falling along −z, 20 m wide
fn slope(len: f32, drop: f32) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let y = |z: f32| drop * (z / len);
    let q = |x: f32, z: f32| Vec3::new(x, y(z), z);
    w.add_ex([q(-10.0, 0.0), q(10.0, 0.0), q(-10.0, -len)], None, true, 1, false);
    w.add_ex([q(10.0, 0.0), q(10.0, -len), q(-10.0, -len)], None, true, 1, false);
    w
}

fn rider() -> Rider {
    let mut r = Rider::new(Vec3::new(0.0, 0.0, -1.0), 0.0);
    r.grounded = true;
    r.surface = 1;
    r.normal = Vec3::Y;
    r.vel = Vec3::new(0.0, 0.0, -10.0);
    r
}

#[test]
fn the_jump_button_winds_up_and_jumps_through_the_games_states() {
    let world = slope(300.0, 40.0);
    let rails = Rails::default();
    let mut r = rider();
    let step = |r: &mut Rider, i: Input| r.step(&world, &rails, i, 1.0 / 60.0);
    for _ in 0..60 {
        step(&mut r, Input::default());
    }
    assert_eq!(r.ctl, CRUISE);
    // the button down: cruise brakes to 0, then the prewind crouches and winds up a spin to the right
    let hold = Input { jump: true, steer: 1.0, ..Input::default() };
    let mut states = vec![];
    for _ in 0..40 {
        step(&mut r, hold);
        states.push(r.ctl);
    }
    assert!(states.contains(&PREWIND), "{states:?}");
    assert!(r.grounded);
    assert!(0.9 < r.crouch_v, "crouched: {}", r.crouch_v);
    assert!(0.5 < r.wind, "wound up: {}", r.wind);
    assert!(0.0 < r.charge);
    // let go: the jump state lets the crouch out (13.3333 a second), then the impulse and the spin state
    let mut seen = vec![];
    for _ in 0..20 {
        step(&mut r, Input::default());
        seen.push(r.ctl);
        if !r.grounded {
            break;
        }
    }
    assert!(seen.contains(&JUMP), "{seen:?}");
    assert!(!r.grounded, "jumped: {seen:?}");
    assert_eq!(r.ctl, SPIN);
    assert!(0.5 < r.takeoff_wind.0, "the take-off's spin wind-up: {:?}", r.takeoff_wind);
    assert!(r.vel.y > 3.0, "the jump's impulse: {:?}", r.vel);
}

#[test]
fn a_tap_jumps_without_a_wind_up() {
    let world = slope(300.0, 40.0);
    let rails = Rails::default();
    let mut r = rider();
    for _ in 0..60 {
        r.step(&world, &rails, Input::default(), 1.0 / 60.0);
    }
    r.step(&world, &rails, Input { jump: true, ..Input::default() }, 1.0 / 60.0);
    let mut left = false;
    for _ in 0..30 {
        r.step(&world, &rails, Input::default(), 1.0 / 60.0);
        left |= !r.grounded;
    }
    assert!(left, "a tap jumps too (the smallest jump, 630.8786 cm/s)");
    assert_eq!(r.takeoff_wind, (0.0, 0.0));
}

#[test]
fn tuck_crouches_and_brake_brakes_in_cruise() {
    let world = slope(300.0, 40.0);
    let rails = Rails::default();
    let mut tuck = rider();
    let mut brake = rider();
    // let the placed riders settle onto the slope (placed 1.0146 m up in NaturalAir, where you cannot brake in mid-air)
    for _ in 0..30 {
        tuck.step(&world, &rails, Input::default(), 1.0 / 60.0);
        brake.step(&world, &rails, Input::default(), 1.0 / 60.0);
    }
    for _ in 0..30 {
        tuck.step(&world, &rails, Input { tuck: true, ..Input::default() }, 1.0 / 60.0);
        brake.step(&world, &rails, Input { brake: true, ..Input::default() }, 1.0 / 60.0);
    }
    assert_eq!((tuck.ctl, brake.ctl), (CRUISE, CRUISE));
    assert!(0.9 < tuck.crouch_v && tuck.brake_v == 0.0, "tuck: crouch {} brake {}", tuck.crouch_v, tuck.brake_v);
    assert!(0.9 < brake.brake_v && brake.crouch_v == 0.0, "brake: crouch {} brake {}", brake.crouch_v, brake.brake_v);
    assert!(tuck.vel.length() > brake.vel.length());
}


#[test]
fn the_button_map_binds_the_riders_trick_slots() {
    // `Pad_SetupVibration` 0x152770: combo k (easyA L2, easyB R1, ...) to the slot of row k at the character and the
    // board's kind
    use tricky_game::controls::{LSHIFT, LSHIFT2, RSHIFT};
    use tricky_game::trickdata::{BOARDS, RIDERS};
    for (ch, d) in RIDERS.iter().enumerate() {
        for b in &BOARDS[ch][..3] {
            let mut r = rider();
            r.data = d;
            r.stats.kind = b.kind;
            let m = r.button_map();
            let rows = &tricky_data::trick_slots::TRICK_SLOTS[ch][b.kind as usize];
            // the last combos bound win a shared slot, so look at the first three only where they differ
            for (k, combo) in [(0, LSHIFT2), (1, RSHIFT), (3, LSHIFT)] {
                let slot = rows[k][0] as usize;
                if rows.iter().skip(k + 1).all(|r| r[0] as usize != slot) {
                    assert_eq!(m.pressed[slot], combo, "{} kind {} row {k}", d.name, b.kind);
                }
            }
        }
    }
}

/// The reset button (select, action 0x25) on the snow goes through the cruise state's `Boarder_RequestReset` with
/// +0x304 = 0, so `Boarder_PlaceAt` charges the meter (`Score_BankPoints`' −0.12); a reset the game asks itself
/// (+0x304 = 1: off the course, the decays, the floor) does not (row E1b2g7d3b4f)
#[test]
fn only_the_reset_button_costs_the_meter() {
    let world = slope(300.0, 40.0);
    let rails = Rails::default();
    let mut r = rider();
    r.safe = (Vec3::new(0.0, 5.0, -200.0), 0.0);
    let step = |r: &mut Rider, i: Input| r.step(&world, &rails, i, 1.0 / 60.0);
    for _ in 0..30 {
        step(&mut r, Input::default());
    }
    assert_eq!(r.ctl, CRUISE);
    r.boost = 0.5;
    let n = r.respawns;
    step(&mut r, Input { reset: true, ..Input::default() });
    // the reset state (`ResetState_Update` 0x106a90) places the rider past 0.8 s
    assert!(r.reset.is_some(), "the button resets");
    while r.reset.is_some() {
        step(&mut r, Input::default());
    }
    assert_eq!(r.respawns, n + 1, "the button resets");
    assert!(r.boost < 0.45, "and costs the meter: {}", r.boost);
    assert_eq!(r.reset_flag, 0);
    // a reset the game asks for itself
    r.boost = 0.5;
    r.request_reset(Some(1));
    while r.reset.is_some() {
        step(&mut r, Input::default());
    }
    assert_eq!((r.respawns, r.boost, r.reset_flag), (n + 2, 0.5, 0));
}

/// Off the course (surface 0) the cruise state asks for the reset itself (+0x304 = 1): no charge
#[test]
fn off_the_course_the_control_resets_free() {
    let mut world = CollisionWorld::default();
    let q = |x: f32, z: f32| Vec3::new(x, -z * 0.1, z);
    world.add_ex([q(-10.0, 0.0), q(10.0, 0.0), q(-10.0, -300.0)], None, true, 0, false);
    world.add_ex([q(10.0, 0.0), q(10.0, -300.0), q(-10.0, -300.0)], None, true, 0, false);
    let rails = Rails::default();
    let mut r = rider();
    r.safe = (Vec3::new(0.0, 5.0, -200.0), 0.0);
    r.boost = 0.5;
    let n = r.respawns;
    for _ in 0..60 {
        r.step(&world, &rails, Input::default(), 1.0 / 60.0);
        if r.respawns != n {
            break;
        }
    }
    assert_eq!(r.respawns, n + 1);
    // (the ticks before it drain a little; the button's charge would be 0.12 scaled by the meter stat)
    assert!(0.49 < r.boost, "{}", r.boost);
}

/// Row G1i2c: `Rider::step` runs `cBoarder::vf7` 0x117198 (`boarder_tick::boarder_tick`) in the game's exact order:
/// eases/timers/height/band before motion, `Boarder_UpdateMotion`, speed limit + `Boarder_AddSpeedSample` between
/// motion and control, `Boarder_UpdateControl`, impact/knocks decays & world-floor reset, `Score_Tick`, and
/// `Boarder_UpdateLeanTargets` last; reset state (motion 4, state 0x16) runs inside `vf7` too.
#[test]
fn rider_step_runs_vf7_order_speed_cap_decays_and_reset_eases() {
    let world = slope(300.0, 40.0);
    let rails = Rails::default();
    let mut r = rider();
    // settle onto the slope first so the ground motion stays on the snow (motion 2)
    for _ in 0..30 {
        r.step(&world, &rails, Input::default(), 1.0 / 60.0);
    }
    assert!(r.grounded && r.motion == 2);
    let samples0 = r.speed_samples;
    let sum0 = r.speed_sum;
    r.vel = r.vel.normalize() * 35.0;
    r.speed_timer = 1.0;
    r.spin_timer = 2.0;
    r.body_height = 0.0;
    let t0 = r.vf7.travelled;

    // 1. Ground tick: `before_motion` decrements timers & eases height/band; `after_motion` caps speed & samples it;
    //    `Boarder_UpdateLeanTargets` aims lean for the next tick.
    r.step(&world, &rails, Input { steer: 1.0, ..Input::default() }, 1.0 / 60.0);
    assert_eq!(r.speed_samples, samples0 + 1.0, "Boarder_AddSpeedSample should record each tick");
    assert!(r.speed_sum > sum0);
    assert!(r.vf7.travelled > t0, "vf7 distance along forward should advance");
    assert!((r.speed_timer - (1.0 - f32::from_bits(0x3c88_8889))).abs() < 1e-6);
    assert!((r.spin_timer - (2.0 - f32::from_bits(0x3c88_8889))).abs() < 1e-6);
    // Boost timer > 0 raises speed limit to 3347.2222 cm/s (33.472222 m/s), capping 35 m/s down to it.
    assert!(r.vel.length() <= 33.48, "vf7 should cap ground speed: {}", r.vel.length());

    // 2. Impact threshold reset (> 4.492007 after * 0.95603746 decay) triggers free reset (+0x304 = 1, motion 4, state 0x16)
    //    and zeroes `impact`.
    r.impact = 10.0;
    r.body_height = 5.0;
    r.step(&world, &rails, Input::default(), 1.0 / 60.0);
    assert!(r.reset.is_some(), "impact overflow should trigger vf7 reset");
    assert_eq!((r.motion, r.ctl, r.reset_flag), (4, RESET, 1));
    assert_eq!(r.impact, 0.0, "vf7 zeroes impact after requesting reset");

    // 3. During reset (motion 4, state 0x16), vf7 still runs its eases (body_height eases toward 0, timers count down).
    let h_before = r.body_height;
    let st_before = r.speed_timer;
    r.step(&world, &rails, Input::default(), 1.0 / 60.0);
    assert!(r.body_height < h_before, "body_height should ease toward 0 during reset: {} -> {}", h_before, r.body_height);
    assert!(r.speed_timer < st_before, "boost timer should count down during reset");
    assert_eq!(r.motion, 4);
    assert_eq!(r.ctl, RESET);

    // 4. Air motion (motion 1) is NOT capped to vf7's 2788.84 cm/s ground speed limit.
    let mut air = Rider::new(Vec3::new(0.0, 200.0, 0.0), 0.0);
    air.grounded = false;
    air.vel = Vec3::new(0.0, 0.0, -31.0);
    air.step(&world, &rails, Input::default(), 1.0 / 60.0);
    assert!(air.vel.length() > 30.0, "air motion must not be speed-capped to SPEED_CAP by vf7: {}", air.vel.length());

    // 5. World-floor reset runs inside vf7::after_motion (free reset, +0x304 = 1).
    let mut fell = Rider::new(Vec3::new(0.0, world.min_y - 40.0, 0.0), 0.0);
    fell.grounded = false;
    fell.step(&world, &rails, Input::default(), 1.0 / 60.0);
    assert!(fell.reset.is_some(), "below world floor should trigger vf7 reset");
    assert_eq!((fell.motion, fell.ctl, fell.reset_flag), (4, RESET, 1));
}

/// Row E1b2g7d3b4h2a: the reset button works end-to-end through `Rider::step` across air (`NATURAL_AIR` 0xd, `SPIN`
/// 0x10), rail (`RAIL` 0xf), and wipe-out / get-up (`WIPEOUT` 0x13, `GET_UP` 0x14), and `cBoarder::vf7` 0x117198
/// runs eases & decays (`brake`, `crouch`, `steer`, `lean`, `body_height`, `impact`, `knocks`, `impact_n`) during
/// reset while `ResetState_Update` 0x106a90 ramps `fade.0` (+0x448) from 1 -> 0 over 0.5 s and `fade.1` (+0x44c)
/// from 0 -> 1 past 0.2 s, restoring `(1.0, 0.0)` on placement past 0.8 s.
#[test]
fn reset_button_in_air_rail_wipeout_and_vf7_during_reset() {
    use tricky_game::rider::control_tick::{GET_UP, NATURAL_AIR, RAIL, SPIN, WIPEOUT};
    let world = slope(300.0, 40.0);
    let rails = Rails::default();
    let dt = 1.0 / 60.0;
    let safe = (Vec3::new(0.0, 15.0, -180.0), 0.0);

    // 1. NATURAL_AIR (0xd): reset button starts reset (no +0x304 write), ramps fade +0x448 and +0x44c, runs vf7
    //    eases & decays while stationary, and places the rider on the 49th tick with fade restored to (1.0, 0.0).
    let mut r = Rider::new(Vec3::new(0.0, 50.0, -20.0), 0.0);
    r.grounded = false;
    r.motion = 1;
    r.vel = Vec3::new(0.0, 0.0, -10.0);
    r.safe = safe;
    r.set_control(NATURAL_AIR);
    r.reset_flag = 1; // NaturalAirControl_Update does not overwrite +0x304
    r.crouch_v = 0.8;
    r.steer = -0.6;
    r.rec.crouch = [0.8, 4.0, 0.0];
    r.rec.steer = [0.6, 5.0, 0.0];
    r.body_height = 6.0;
    r.impact = 2.0;
    r.knocks = 3.0;
    r.impact_n = [1.0, 0.0, 0.0, 0.0];
    let n0 = r.respawns;
    r.step(&world, &rails, Input { reset: true, ..Input::default() }, dt);
    assert!(r.reset.is_some(), "reset button in NATURAL_AIR must enter reset state");
    assert_eq!((r.motion, r.ctl, r.reset_flag), (4, RESET, 1));
    let pos_at_reset = r.pos;
    let (c0, s0, h0, imp0, kn0, in0) = (r.rec.crouch[0], r.rec.steer[0], r.body_height, r.impact, r.knocks, r.impact_n[0]);
    let mut saw_mid_fade = false;
    let mut saw_zero_fade = false;
    let mut saw_screen_fade = false;
    let mut ticks = 0u32;
    while r.reset.is_some() {
        r.step(&world, &rails, Input::default(), dt);
        ticks += 1;
        if ticks == 1 {
            // Stationary (motion 4) while vf7 eases records & decays impact/knocks/crash_normal
            assert_eq!(r.pos, pos_at_reset, "rider must stay stationary during reset");
            assert!(r.rec.crouch[0] < c0, "crouch record must ease during reset");
            assert!(r.rec.steer[0] < s0, "steer record must ease during reset");
            assert!(r.body_height < h0, "body_height must ease during reset");
            assert!(r.impact < imp0, "impact must decay during reset");
            assert!(r.knocks < kn0, "knocks must decay during reset");
            assert!(r.impact_n[0] < in0, "crash_normal must decay during reset");
        }
        if 0.0 < r.fade.0 && r.fade.0 < f32::from_bits(0x3f73_3333) {
            saw_mid_fade = true;
        }
        if r.fade.0 == 0.0 {
            saw_zero_fade = true;
        }
        if r.fade.1 > 0.0 {
            saw_screen_fade = true;
        }
        assert!(ticks < 60, "reset must complete around 49 ticks");
    }
    assert!(saw_mid_fade, "rider +0x448 fade must pass through (0, 0.95)");
    assert!(saw_zero_fade, "rider +0x448 fade must reach 0.0 past 0.5 s");
    assert!(saw_screen_fade, "screen +0x44c fade must rise past 0.2 s");
    assert_eq!(r.fade, (1.0, 0.0), "ResetWaitControl_Exit restores fade to (1.0, 0.0)");
    assert_eq!(r.respawns, n0 + 1);

    // 2. SPIN (0x10) after takeoff (with spin_st active): reset button enters reset and places rider.
    let mut r = rider();
    r.safe = safe;
    for _ in 0..20 {
        r.step(&world, &rails, Input::default(), dt);
    }
    // Prewind then release jump to enter SPIN (0x10) with spin_st initialized
    for _ in 0..10 {
        r.step(&world, &rails, Input { jump: true, ..Input::default() }, dt);
    }
    for _ in 0..15 {
        r.step(&world, &rails, Input::default(), dt);
        if r.ctl == SPIN && r.spin_st.is_some() {
            break;
        }
    }
    assert_eq!(r.ctl, SPIN, "jump should enter SPIN");
    assert!(r.spin_st.is_some());
    r.step(&world, &rails, Input { reset: true, ..Input::default() }, dt);
    assert!(r.reset.is_some(), "reset button in SPIN must enter reset state");
    assert_eq!((r.motion, r.ctl), (4, RESET));

    // 3. RAIL (0xf): reset button enters reset state through Rider::step.
    let one_rail = Rails(
        vec![tricky_game::rails::Rail::new(vec![Vec3::new(0.0, 10.0, -10.0), Vec3::new(0.0, 10.0, -80.0)]).unwrap()],
        Default::default(),
    );
    let mut r = Rider::new(Vec3::new(0.0, 10.0, -20.0), 0.0);
    r.grounded = false;
    r.rail = Some((0, 10.0, 10.0));
    r.motion = 3;
    r.vel = Vec3::new(0.0, 0.0, -10.0);
    r.safe = safe;
    r.set_control(RAIL);
    r.step(&world, &one_rail, Input { reset: true, ..Input::default() }, dt);
    assert!(r.reset.is_some(), "reset button on RAIL must enter reset state");
    assert_eq!((r.motion, r.ctl), (4, RESET));

    // 4. WIPEOUT (0x13) and GET_UP (0x14): reset button sets +0x304 = 0, enters reset, and charges meter.
    for state in [WIPEOUT, GET_UP] {
        let mut r = rider();
        r.safe = safe;
        for _ in 0..15 {
            r.step(&world, &rails, Input::default(), dt);
        }
        r.crashed = 2.0;
        r.grounded = false;
        r.motion = if state == GET_UP { 6 } else { 5 };
        r.surface = 1;
        r.boost = 0.5;
        r.reset_flag = 1;
        r.set_control(state);
        r.step(&world, &rails, Input { reset: true, ..Input::default() }, dt);
        assert!(r.reset.is_some(), "reset button in state {state:#x} must enter reset state");
        assert_eq!((r.motion, r.ctl, r.reset_flag), (4, RESET, 0), "state {state:#x} must clear +0x304 for button");
        while r.reset.is_some() {
            r.step(&world, &rails, Input::default(), dt);
        }
        assert!(r.boost < 0.45, "button reset from state {state:#x} must charge meter: {}", r.boost);
    }
}
