//! The landing as the game decides it (board row G20c3; TRICKY_DISC for gari's `.pbd`, `.ltg` and `.ssf`, skipped
//! without it): a rider in the air over gari's snow with its attitude kept lands through `Landing_CheckAngles` and
//! `Landing_ChooseState` (`Rider::land_as_game`). Level and forwards: clean, no hard landing. Turned round: clean too
//! (soft and slow enough), riding switch. Nose up 50° (the roll under −π/4) or falling 30 m/s into the snow (over
//! 27.77): a hard landing (state 0xb) for its landing clip. An uber not yet done (a clip of type 0xe short of its
//! safe point): refused, down.

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

/// Flat snow (normal z over 0.97, a skiable surface, clear of touch objects along the drop path) under one of the
/// rails' points, 3 m down at most
fn flat_snow(cg: &CourseGround, pbd: &tricky_data::pbd::Pbd) -> V4 {
    for seg in pbd.segments.iter() {
        let f = |i: usize| f32::from_bits(seg[i]);
        for dx in [0.0f32, 400.0, -400.0] {
            let p = [f(12) + dx, f(13) + dx, f(14) + 100.0, 1.0];
            let mut q = tricky_game::worldline::WorldLine::new(0.0, p, [p[0], p[1], p[2] - 600.0, 1.0], 0);
            let (t, h) = cg.ray_cast(&mut q, false, None, &|_| false);
            if let Some(h) = h.filter(|_| 0.0 <= t) {
                if 0.97 < h.normal[2] && !matches!(h.surface, 0 | 6 | 10) {
                    // Check the air drop corridor (+40 cm, flying along +x) is free of bouncy touch objects (row E8q).
                    let clear = [0.0f32, 100.0, 200.0].into_iter().all(|ox| {
                        let r = Rider::new(port([h.pos[0] + ox, h.pos[1], h.pos[2] + 40.0, 1.0]), -std::f32::consts::FRAC_PI_2);
                        let b = r.bind_body();
                        let mut iq = tricky_game::worldboarder::WorldBoarder::new(&b, 1);
                        iq.front_only = true;
                        cg.query_instances(&mut iq, false, &|_| false).0 < 0.0
                    });
                    if clear {
                        return h.pos;
                    }
                }
            }
        }
    }
    panic!("no flat snow");
}

/// A rider 40 cm over the snow at `at`, flying along +x at 1000 cm/s and down at `fall`, its board turned `turn`
/// about the up axis from the travel and pitched nose down by `nose`: ticks until it is down on the snow or put down
fn drop(world: &CollisionWorld, at: V4, turn: f32, nose: f32, fall: f32) -> Rider {
    drop_full(world, at, turn, nose, fall, &|_| {})
}

/// [`drop`] level and forwards, with `set` done to the rider first
fn drop_with(world: &CollisionWorld, at: V4, set: impl Fn(&mut Rider)) -> Rider {
    drop_full(world, at, 0.0, 0.0, 200.0, &set)
}

fn drop_full(world: &CollisionWorld, at: V4, turn: f32, nose: f32, fall: f32, set: &dyn Fn(&mut Rider)) -> Rider {
    let heading = turn;
    let mut r = Rider::new(port([at[0], at[1], at[2] + 40.0, 1.0]), tricky_game::rider::wrap(heading - std::f32::consts::FRAC_PI_2));
    let (fx, fy) = (heading.cos(), heading.sin());
    let up = [nose.sin() * fx, nose.sin() * fy, nose.cos(), 0.0];
    r.att = Some((tricky_game::orient::placed_attitude(heading, Some(up)), r.yaw));
    r.grounded = false;
    r.air_time = 0.5;
    r.vel = port([1000.0, 0.0, -fall, 0.0]);
    set(&mut r);
    for _ in 0..30 {
        r.step(world, &Rails::default(), Input::default(), 1.0 / 60.0);
        if r.grounded || r.crashed > 0.0 {
            break;
        }
    }
    r
}

#[test]
fn landings_are_the_games() {
    let Some((cg, pbd)) = course() else { return };
    let at = flat_snow(&cg, &pbd);
    let mut world = CollisionWorld::default();
    world.min_y = -1e9;
    world.course = Some(Arc::new(cg));
    let pi = std::f32::consts::PI;
    let clean = drop(&world, at, 0.0, 0.0, 200.0);
    println!("clean: grounded {} crashed {} hard {:.2} switch {}", clean.grounded, clean.crashed, clean.hard_land, clean.switch);
    assert!(clean.grounded && clean.crashed <= 0.0 && clean.hard_land == 0.0, "a clean landing");
    let back = drop(&world, at, pi, 0.0, 200.0);
    println!("turned round: grounded {} crashed {} hard {:.2} switch {}", back.grounded, back.crashed, back.hard_land, back.switch);
    assert!(back.grounded && back.crashed <= 0.0 && back.switch != clean.switch && back.hard_land == 0.0, "backwards: switch, clean");
    let nose_up = drop(&world, at, 0.0, -50f32.to_radians(), 200.0);
    println!("nose up 50°: grounded {} crashed {} cause {} hard {:.2}", nose_up.grounded, nose_up.crashed, nose_up.cause, nose_up.hard_land);
    // (`Landing_ChooseState`: forwards, the roll under −π/4, firm: clip 0x242; held for its length)
    let len = |clip: u16| tricky_game::rider::wipeout::clip_frames(clip, nose_up.stats.kind as u8) / 30.0;
    assert!(nose_up.crashed <= 0.0 && nose_up.hard_land == len(0x242), "nose up: hard, clip 0x242's length ({})", len(0x242));
    let fast = drop(&world, at, 0.0, 0.0, 3000.0);
    println!("falling 30 m/s: grounded {} crashed {} hard {:.2}", fast.grounded, fast.crashed, fast.hard_land);
    // (forwards, soft, not firm: clip 0x23e)
    assert!(fast.crashed <= 0.0 && fast.hard_land == len(0x23e), "fast into the snow: hard, clip 0x23e's length ({})", len(0x23e));
    // an uber short of its fourth marker (clip type 0x10 without flag 4): the check refuses the landing
    let uber = drop_with(&world, at, |r| {
        r.uber = 1.0;
    });
    println!("uber cut short: crashed {} cause {}", uber.crashed, uber.cause);
    assert!(uber.crashed > 0.0 && uber.cause == "grab", "an uber short of marker 4: down");

    // E8t: an uber past its real `.afl` fourth marker lands cleanly; a `0xffff` fourth marker always crashes.
    let mut bank = tricky_data::afl::ClipMarkerBank::new();
    bank.insert("bxUT_MTINDY", tricky_data::afl::ClipMarkers { frames: 61, markers: [10, tricky_data::afl::MARKER_UNUSED, 51, 51] });
    bank.insert("bxUT_NOM4", tricky_data::afl::ClipMarkers { frames: 61, markers: [10, tricky_data::afl::MARKER_UNUSED, 51, tricky_data::afl::MARKER_UNUSED] });
    let bank = Arc::new(bank);
    let uber_ok = drop_with(&world, at, |r| {
        r.attach_clip_markers(bank.clone());
        r.ubers = vec![("bxUT_MTINDY".into(), 61.0)];
        r.uber_id = 0;
        r.uber = 52.0; // strictly past m4 = 51, well before clip end 61
    });
    assert!(uber_ok.grounded && uber_ok.crashed <= 0.0, "uber past real .afl marker 4 must land cleanly");
    let uber_nom4 = drop_with(&world, at, |r| {
        r.attach_clip_markers(bank.clone());
        r.ubers = vec![("bxUT_NOM4".into(), 61.0)];
        r.uber_id = 0;
        r.uber = 60.0;
    });
    assert!(uber_nom4.crashed > 0.0 && uber_nom4.cause == "grab", "0xffff fourth marker must never clear landing gate");

    // E8r: mid-backflip (`flip = pi`, even with upright `att`): `Landing_CheckAngles` folds `flip` into `att`
    // via `SpinControl_Exit` 0x1029d0 before checking angles -> upside-down landing crashes!
    let mid_flip = drop_with(&world, at, |r| {
        r.flip = pi;
    });
    println!("mid backflip: crashed {} cause {}", mid_flip.crashed, mid_flip.cause);
    assert!(mid_flip.crashed > 0.0 && mid_flip.cause == "landing angle", "upside-down flip must crash via Landing_CheckAngles");

    // E8r: short hop (`air_time = 0.05 <= 0.15`) STILL goes through `AirMotion_Update` landing (no artificial
    // `air_time > 0.15` gate): clears `predict.t_land`, sets `land_fall > 0`, and catches a bad flip/grab!
    let short_hop = drop_with(&world, at, |r| {
        r.air_time = 0.02;
        r.predict.t_land = 0.5;
        r.predict.word = u32::MAX;
    });
    assert!(short_hop.grounded && short_hop.crashed <= 0.0, "clean short hop lands");
    assert_eq!(short_hop.predict.t_land, 0.0, "AirPredict_ClearLandTime 0x123f00 clears predict.t_land");
    assert!(short_hop.land_fall > 0.0, "AirMotion_Update records rider+0x798 fall speed");

    // E8r: live `AnimHost` landing uses real `PoseBlend` pivot bone, blends out layer-1 type 0x1a clip, and
    // transitions cleanly into `LAND_NORMAL` (0xc) on clip `0x1ff`.
    if let Ok(elf_path) = std::env::var("TRICKY_ELF") {
        if let (Ok(elf), Ok(disc)) = (std::fs::read(&elf_path), std::env::var("TRICKY_DISC")) {
            let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc).unwrap());
            files.mount_game_archives().unwrap();
            let body = tricky_data::character::CharModel::rider_from_disc(&mut files, "mac").unwrap();
            let brd = tricky_data::character::CharModel::board_from_disc(&mut files, Some("Fr")).unwrap();
            let body_bones = body.slot_raw_bones(0);
            let brd_bones = brd.slot_raw_bones(1);
            let roster = [tricky_game::animhost::RosterEntry { character: 2, board_class: 1, load_uber: true }];
            let mut host = tricky_game::animhost::AnimHost::from_disc(&elf, &mut files, &roster).unwrap();
            let slot = host.add_rider(2, 1, 1.0, &[(0, &body_bones), (1, &brd_bones)]);
            host.set_board_ik_points(slot, &brd.ik_points);
            let anim = tricky_game::animhost::RiderAnim::new(Arc::new(std::sync::Mutex::new(host)), slot);
            let live = drop_with(&world, at, |r| {
                r.data = &tricky_game::trickdata::RIDERS[2];
                r.stats.kind = 1;
                r.attach_anim(anim.clone());
            });
            assert!(live.grounded && live.crashed <= 0.0, "live AnimHost clean landing");
            assert_eq!(live.ctl, tricky_game::rider::control_tick::LAND_NORMAL, "Landing_ChooseState -> LAND_NORMAL (0xc)");
            assert_eq!(anim.clip(0), 0x1ff, "Landing_ChooseState plays clip 0x1ff");
            assert!(live.land_fall > 0.0, "recorded landing fall speed");
        }
    }
}
