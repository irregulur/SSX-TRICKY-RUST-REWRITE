//! The air's wall on the course (board row G20c3c3; TRICKY_DISC for gari's `.pbd`, `.ltg` and `.ssf`, skipped
//! without it): `Rider::step` in the air casts the rider's body (`cWorldBoarder`, mask 0xcf) at the course as
//! `AirMotion_Update` does and runs its wall code (`air_motion::wall_push`, `wall_bump`). Flying upright into one of
//! gari's walls the rider is put down ("air wall"). A hit on one of its slopes (its normal 32–47° off the sky, handed
//! to `Rider::air_wall_hit`: over a plane slope the board lands before the body's 0xcf spheres reach it) pushes the
//! rider out by 1.1 × its depth and glances: the kept attitude turns toward the normal by the game's most
//! (0x3dd67750 rad) and the velocity's part into it goes.

use glam::Vec3;
use std::sync::Arc;
use tricky_game::collide::CollisionWorld;
use tricky_game::course_ground::CourseGround;
use tricky_game::rails::Rails;
use tricky_game::rider::{Input, Rider};

type V4 = [f32; 4];

fn course() -> Option<(CourseGround, tricky_data::pbd::Pbd)> {
    let disc = std::env::var("TRICKY_DISC").ok()?;
    let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc).ok()?);
    files.mount("data/models/gari.big").ok()?;
    let mut get = |ext: &str| files.load(&format!("|data/models/gari.{ext}")).ok().flatten();
    let (pbd, ltg, ssf) = (get("pbd")?, get("ltg")?, get("ssf")?);
    Some((CourseGround::new(&pbd, &ltg, &ssf)?, tricky_data::pbd::Pbd::parse(&pbd)?))
}
fn port(v: V4) -> Vec3 {
    Vec3::new(v[0], v[2], -v[1]) * 0.01
}
fn game(v: Vec3) -> V4 {
    [v.x * 100.0, -v.z * 100.0, v.y * 100.0, 1.0]
}

/// A surface whose normal's z is in `z`: a point on it, its normal and the horizontal ray direction that found it,
/// from rays 150 cm over gari's rail points
fn a_surface(cg: &CourseGround, pbd: &tricky_data::pbd::Pbd, z: std::ops::Range<f32>) -> (V4, V4, V4) {
    for seg in pbd.segments.iter().step_by(3) {
        let f = |i: usize| f32::from_bits(seg[i]);
        let p = [f(12), f(13), f(14) + 150.0, 1.0];
        for k in 0..16 {
            let a = k as f32 * std::f32::consts::TAU / 16.0;
            let rd = [a.cos(), a.sin(), 0.0, 0.0];
            let to = [p[0] + 1500.0 * rd[0], p[1] + 1500.0 * rd[1], p[2], 1.0];
            let mut q = tricky_game::worldline::WorldLine::new(0.0, p, to, 0);
            let (t, h) = cg.ray_cast(&mut q, false, None, &|_| false);
            if let Some(h) = h.filter(|_| 0.1 < t) {
                if z.contains(&h.normal[2]) && h.normal[0] * rd[0] + h.normal[1] * rd[1] < -0.3 {
                    return (h.pos, h.normal, rd);
                }
            }
        }
    }
    panic!("no surface with its normal's z in {z:?}");
}

/// A rider facing and flying along `face` (1000 cm/s, plus 200 cm/s along `drift`), upright, its board `d` cm back
/// from the point `at` along `back` and `dz` cm over it: one tick
fn fly(world: &CollisionWorld, at: V4, face: V4, drift: V4, back: V4, d: f32, dz: f32) -> Rider {
    let p = [at[0] + back[0] * d, at[1] + back[1] * d, at[2] + dz, 1.0];
    let heading = tricky_game::jump::atan2_inline(face[1], face[0]);
    let mut r = Rider::new(port(p), tricky_game::rider::wrap(heading - std::f32::consts::FRAC_PI_2));
    r.grounded = false;
    r.air_time = 0.5;
    r.vel = port([face[0] * 1000.0 + drift[0] * 200.0, face[1] * 1000.0 + drift[1] * 200.0, 0.0, 0.0]);
    r.step(world, &Rails::default(), Input::default(), 1.0 / 60.0);
    r
}

/// The first place back from the surface (distance, height) at which `hit` says the tick met it
fn reach(world: &CollisionWorld, at: V4, face: V4, drift: V4, back: V4, hit: &dyn Fn(&Rider) -> bool) -> (f32, f32, Rider) {
    for d in (0..=300).step_by(3) {
        for dz in (-200..=20).step_by(5) {
            let r = fly(world, at, face, drift, back, d as f32, dz as f32);
            if hit(&r) {
                return (d as f32, dz as f32, r);
            }
        }
    }
    panic!("never met");
}

#[test]
fn the_air_glances_off_gari_walls_as_the_game_does() {
    let Some((cg, pbd)) = course() else { return };
    let (slope, sn, _) = a_surface(&cg, &pbd, 0.68..0.85);
    let (wall, wn, wrd) = a_surface(&cg, &pbd, -0.15..0.15);
    let mut world = CollisionWorld::default();
    world.min_y = -1e9;
    world.course = Some(Arc::new(cg));
    // one of gari's walls ahead, upright: the body's cast hits it and the rider is put down (from in front of the
    // wall; at d = 0, dz = -200 the pre-RK4 down ray `piv + 200·up` touches `wall` itself and crashes as a landing)
    let (d, dz, r) = reach(&world, wall, wrd, wrd, [-wrd[0], -wrd[1], 0.0, 0.0], &|r: &Rider| r.crashed > 0.0 && r.cause == "air wall");
    println!("wall {wn:?} {d} cm back, {dz} cm over: crashed {} cause {}", r.crashed, r.cause);
    assert!(r.crashed > 0.0 && r.cause == "air wall", "put down by the wall");
    assert!(r.crash_spray.is_some(), "air wall crash emits cPlayer::vf11 spray");
    // a glance (over a plane slope the board lands before the 0xcf spheres reach it, so the hit is handed in): one of
    // gari's slopes (normal 32–47° off the sky) hit 5 cm deep by a rider flying into it upright: pushed out along
    // the normal by 1.1 × 5 cm, the attitude turned toward the normal by the most, the part into it gone
    let mut r = Rider::new(port(slope), 0.0);
    r.grounded = false;
    // 400 cm/s into it, 800 along it (square to the normal)
    let l = (sn[0] * sn[0] + sn[1] * sn[1]).sqrt();
    let t = [-sn[1] / l, sn[0] / l, 0.0];
    let v0 = [-sn[0] * 400.0 + t[0] * 800.0, -sn[1] * 400.0 + t[1] * 800.0, -sn[2] * 400.0, 0.0];
    r.vel = port(v0);
    let p0 = game(r.pos);
    r.air_wall_hit(&tricky_game::air_motion::Ray { frac: 5.0, point: slope, normal: sn, word: 0 });
    let p1 = game(r.pos);
    let push = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let v = game(r.vel);
    let into = v[0] * sn[0] + v[1] * sn[1] + v[2] * sn[2];
    let o = r.att.unwrap().0;
    let (before, after) = (sn[2].acos(), (o.up[0] * sn[0] + o.up[1] * sn[1] + o.up[2] * sn[2]).acos());
    println!("slope {sn:?}: pushed {push:?}, into it {into:.3} cm/s, up {after:.4} rad off its normal (was {before:.4}), crashed {}", r.crashed);
    for k in 0..3 {
        assert!((push[k] - sn[k] * 5.5).abs() < 0.01, "pushed along the normal 5.5 cm: {push:?}");
    }
    assert!(r.crashed <= 0.0 && into.abs() < 0.5, "a glance: into {into}");
    assert!((before - after - f32::from_bits(0x3dd6_7750)).abs() < 0.005, "turned by the most: {}", before - after);
    // the same into a wall (normal level): a crash
    let mut r = Rider::new(port(wall), 0.0);
    r.grounded = false;
    r.vel = port([-wn[0] * 800.0, -wn[1] * 800.0, 0.0, 0.0]);
    r.air_wall_hit(&tricky_game::air_motion::Ray { frac: 5.0, point: wall, normal: wn, word: 0 });
    assert!(r.crashed > 0.0, "a square wall puts it down");
    assert!(r.crash_spray.is_some(), "air_wall_hit crash records spray");
}

fn vertical_wall(z: f32, y_lo: f32, y_hi: f32) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    w.min_y = -1e9;
    let q = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    w.add_ex([q(-5.0, y_lo, z), q(5.0, y_hi, z), q(5.0, y_lo, z)], None, false, 1, false);
    w.add_ex([q(-5.0, y_lo, z), q(-5.0, y_hi, z), q(5.0, y_hi, z)], None, false, 1, false);
    w
}

#[test]
fn air_wall_has_no_bounce_or_fake_gates() {
    // E8p: `AirMotion_Update` 0x108378 (0x108a2c) has no `vin > 1`, `n.y < 0.7`, or `air_time > 0.15` gates and
    // never calls `Boarder_WallCollide` 0x126250's bounce branch in the air.
    let wall = vertical_wall(-10.0, -2.0, 5.0);

    // Slow inward speed (0.5 m/s) and brand-new air (0.01 s): crashes "air wall" (never bounces), pushes out, sprays & rumbles.
    let mut r = Rider::new(Vec3::new(0.0, 0.0, -9.80), 0.0);
    r.grounded = false;
    r.set_motion(1);
    r.ctl = 0xd;
    r.air_time = 0.01;
    r.vel = Vec3::new(0.0, 0.0, -0.5);
    r.step(&wall, &Rails::default(), Input::default(), 1.0 / 60.0);
    assert!(r.crashed > 0.0 && r.cause == "air wall", "slow/early air hit must crash, not bounce: crashed={} cause={}", r.crashed, r.cause);
    assert!(r.crash_spray.is_some(), "air wall crash emits spray (cPlayer::vf11)");
    assert!(r.pad_rumble.small_frames > 0, "air wall crash triggers Rumble_Impact");
    assert!(r.pos.z > -9.80, "pushed out of wall by 1.1 * depth: pos={:?}", r.pos);

    // Moving away from the wall (vn > 0): pushes out by 1.1 * depth, does not crash and does not bounce velocity.
    let mut r = Rider::new(Vec3::new(0.0, 0.0, -9.80), 0.0);
    r.grounded = false;
    r.set_motion(1);
    r.ctl = 0xd;
    r.air_time = 0.5;
    r.vel = Vec3::new(0.0, 0.0, 4.0);
    r.step(&wall, &Rails::default(), Input::default(), 1.0 / 60.0);
    assert!(r.crashed <= 0.0, "moving away from wall must not crash");
    assert!((r.vel.z - 4.0).abs() < 0.05, "moving away must not bounce velocity: {:?}", r.vel);

    // Glancing slope with n.y >= 0.7 (0.85): strips normal speed (no bounce) and turns attitude by up to 0x3dd67750 rad.
    let n_game = [0.0f32, -0.5267827, 0.85, 0.0];
    let mut r = Rider::new(Vec3::new(0.0, 5.0, 0.0), 0.0);
    r.grounded = false;
    r.set_motion(1);
    r.ctl = 0xd;
    r.air_time = 0.01;
    // 50 cm/s into slope (vin = 0.5 m/s, below old gate) + 800 cm/s tangent
    r.vel = port([800.0, -n_game[1] * 50.0, -n_game[2] * 50.0, 0.0]);
    r.air_wall_hit(&tricky_game::air_motion::Ray { frac: 4.0, point: [0.0, 0.0, 500.0, 1.0], normal: n_game, word: 0 });
    assert!(r.crashed <= 0.0, "glancing slope must not crash");
    let vg = game(r.vel);
    let vn_after = vg[0] * n_game[0] + vg[1] * n_game[1] + vg[2] * n_game[2];
    assert!(vn_after.abs() < 0.5, "glancing slope strips normal speed without bounce: vn_after={vn_after}");
    let o = r.att.unwrap().0;
    let before = n_game[2].acos();
    let after = (o.up[0] * n_game[0] + o.up[1] * n_game[1] + o.up[2] * n_game[2]).acos();
    assert!((before - after - f32::from_bits(0x3dd6_7750)).abs() < 0.005, "attitude turned by 0x3dd67750 rad: {}", before - after);
}

#[test]
fn air_wall_mask_0xcf_uses_upper_body_spheres_only() {
    // Mask 0xcf = spheres 0,1,2,3,6,7 (upper back, head, shoulders, hands); feet/board spheres (12..17) are excluded.
    let feet_only_wall = vertical_wall(-10.0, -0.2, 0.25);
    let head_wall = vertical_wall(-10.0, 1.15, 1.65);

    let make_flyer = || {
        let mut r = Rider::new(Vec3::new(0.0, 0.0, -9.85), 0.0);
        r.grounded = false;
        r.set_motion(1);
        r.ctl = 0xd;
        r.air_time = 0.5;
        r.vel = Vec3::new(0.0, 0.0, -2.0);
        r
    };

    let mut r_low = make_flyer();
    r_low.step(&feet_only_wall, &Rails::default(), Input::default(), 1.0 / 60.0);
    assert!(r_low.crashed <= 0.0, "mask 0xcf must not hit a wall that only overlaps feet/board spheres");

    let mut r_high = make_flyer();
    r_high.step(&head_wall, &Rails::default(), Input::default(), 1.0 / 60.0);
    assert!(r_high.crashed > 0.0 && r_high.cause == "air wall", "mask 0xcf must hit a wall at head/upper-back height");
}

#[test]
fn air_wall_crash_in_spin_runs_spin_control_exit() {
    // `AirMotion_Update` 0x108e10 -> `Boarder_Wipeout` -> `Boarder_SetState(0x13)` -> `SpinControl_Exit` 0x1029d0
    // folds remaining spin/flip into `att` and clears `spin`/`flip`/`spin_st`.
    let wall = vertical_wall(-10.0, -2.0, 5.0);
    let mut r = Rider::new(Vec3::new(0.0, 0.0, -9.45), 0.0);
    r.grounded = false;
    r.set_motion(1);
    r.set_control(0x10); // SPIN
    r.spin = 0.7;
    r.flip = 0.4;
    r.air_time = 0.5;
    r.vel = Vec3::new(0.0, 0.0, -10.0);
    assert!(r.spin_st.is_some());
    r.step(&wall, &Rails::default(), Input::default(), 1.0 / 60.0);
    assert!(r.crashed > 0.0 && r.cause == "air wall", "expected air wall crash");
    assert!(r.spin_st.is_none(), "SpinControl_Exit cleared spin_st");
    assert_eq!(r.spin, 0.0, "SpinControl_Exit cleared spin");
    assert_eq!(r.flip, 0.0, "SpinControl_Exit cleared flip");
    assert!(r.att.is_some(), "SpinControl_Exit folded spin/flip into att");
}
