//! The computer rider's modes driving the port's rider down elysium's first AI path on the course's own collision
//! (board row G4e1; needs elysium's `.pbd`, `.ltg`, `.ssf` in TRICKY_PBD_DIR, TRICKY_LTG_DIR, TRICKY_SSF_DIR and its
//! `.aip` in TRICKY_AIP_DIR; skipped without them): a minute of 60 Hz ticks keeps it on the path and carries it on
//! along it.
use glam::Vec3;
use std::sync::Arc;
use tricky_game::aiglue::{AiRider, RaceView};
use tricky_game::aipath::Path;
use tricky_game::collide::CollisionWorld;
use tricky_game::course_ground::CourseGround;
use tricky_game::rails::Rails;
use tricky_game::rider::Rider;

#[test]
fn computer_rider_rides_its_path() {
    let (Ok(pdir), Ok(ldir), Ok(sdir), Ok(adir)) = (std::env::var("TRICKY_PBD_DIR"), std::env::var("TRICKY_LTG_DIR"), std::env::var("TRICKY_SSF_DIR"), std::env::var("TRICKY_AIP_DIR")) else { return };
    let Ok(aip) = std::fs::read(format!("{adir}/elysium.aip")) else { return };
    let read = |d: &str, e: &str| std::fs::read(format!("{d}/elysium.{e}")).unwrap();
    let cg = CourseGround::new(&read(&pdir, "pbd"), &read(&ldir, "ltg"), &read(&sdir, "ssf")).unwrap();
    let mut world = CollisionWorld::default();
    world.min_y = -1e9;
    world.course = Some(Arc::new(cg));
    let rails = Rails(Vec::new(), Default::default());
    let paths: Vec<Path> = tricky_data::aip::read(&aip).unwrap().ai_paths.iter().map(Path::from_aip).collect();

    // on the first path 50 m in, facing along it, on the snow
    let p = &paths[0];
    let at = |d: f32| {
        let v = p.position_at(d).0;
        Vec3::new(v[0], v[2], -v[1]) * 0.01
    };
    let (a, b) = (at(5000.0), at(5100.0));
    let dir = (b - a).normalize();
    let ground = world.ground(a, 50.0, 50.0).expect("snow under the path").y;
    let mut r = Rider::new(Vec3::new(a.x, ground + 0.05, a.z), f32::atan2(-dir.x, -dir.z));
    r.ai = true;
    r.vel = dir * 10.0;
    let mut ai = AiRider::new(&paths, r.pos, 1.0);
    let mut seed = 0x1234_5678u32;
    let mut rand = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (seed >> 16) as i32 & 0x7fff
    };
    let (mut worst, mut far, mut gone) = (0.0f32, 0, 0.0f32);
    for tick in 0..3600 {
        let view = RaceView { grudge: Default::default(), place: 0, me: 0, others: &[], recs: &[], tick, practice: false };
        let path = ai.follow.path;
        let input = ai.tick(&r, &paths, &view, &mut rand);
        // the way made along the path (none on the tick it changes path)
        if ai.follow.path == path {
            gone += ai.follow.along - ai.follow.last;
        }
        r.step(&world, &rails, input, 1.0 / 60.0);
        if r.crashed <= 0.0 {
            worst = worst.max(ai.follow.off);
            far += (ai.follow.off > 1500.0) as i32;
        }
    }
    eprintln!("{gone:.0} cm along the paths, on path {} at the end, worst {worst:.0} cm off, {far} ticks over 15 m off", ai.follow.path);
    assert!(gone > 50_000.0, "it rode {gone} cm along its paths");
    assert!(far < 60, "{far} ticks over 15 m off the path");
}

/// Row E1b2g7d3b4h2a: player's path tracking (`Boarder_UpdatePathAhead` 0x118090, `x41c == 1`) on real disc `.aip`
/// files (`TRICKY_DISC`, skipped without it): initializes from `start_positions` (`cAI::vf14` 0x111108), tracks the
/// rider across forks and path ends without drawing from `Rand_Game`, and advances in place on reset placement
/// (`Boarder_AdvanceOnPath` 0x118c08 + `ResetState_PlaceRider` 0x119220).
#[test]
fn player_follow_tracks_disc_aip_and_advances_on_reset() {
    let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
    let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc_path).unwrap());
    for course in ["gari", "elysium"] {
        let aip = tricky_data::aip::from_disc(&mut files, course, "aip").unwrap();
        let paths: Vec<Path> = aip.ai_paths.iter().map(Path::from_aip).collect();
        let start_idx = aip.start_positions.get(2).copied().map(|i| i as usize);
        let p0 = &paths[start_idx.unwrap_or(0)];
        let g2b = |v: [f32; 4]| Vec3::new(v[0], v[2], -v[1]) * 0.01;
        let start_pos = g2b(p0.position_at(0.0).0);
        let mut f = tricky_game::aiglue::follow_start(&paths, start_idx, start_pos).expect("player follow");
        if let Some(si) = start_idx {
            assert_eq!(f.path, si, "{course}: start_positions[2] should be chosen at gate");
        }
        // Move rider smoothly down its initial path and verify update_player_follow advances `along` and never touches Rand_Game.
        let seed_before = tricky_game::rand::streams().game_snapshot();
        let mut r = Rider::new(start_pos, 0.0);
        let mut switched = false;
        let init_path = f.path;
        let mut walk_d = 0.0f32;
        for tick in 0..400 {
            let cur_len = paths[f.path].length();
            walk_d = (walk_d + 120.0).min(cur_len);
            let (gp, _) = paths[f.path].position_at(walk_d);
            let gd = paths[f.path].direction_at(walk_d);
            r.pos = g2b(gp);
            r.vel = Vec3::new(gd[0], gd[2], -gd[1]) * 18.0;
            let prev_path = f.path;
            tricky_game::aiglue::update_player_follow(&paths, &mut f, &r, 0, tick);
            if f.path != prev_path {
                switched = true;
                walk_d = f.along;
            }
        }
        assert_eq!(tricky_game::rand::streams().game_snapshot(), seed_before, "{course}: player path follow (x41c=1) must not draw Rand_Game");
        assert!(switched || f.along > 5000.0, "{course}: follow should advance along course (path {init_path} -> {}, along {:.0})", f.path, f.along);

        // Reset placement advances `Follow` in place via Boarder_AdvanceOnPath + ResetState_GetRespawnPos.
        let before = (f.path, f.along);
        tricky_game::aiglue::advance_reset_follow(&paths, &mut f, 1500.0);
        assert!(f.path != before.0 || f.along >= before.1, "{course}: advance_reset_follow must advance along path");
        assert!(tricky_game::aiglue::reset_place(&paths, &f).is_some());
    }
}
