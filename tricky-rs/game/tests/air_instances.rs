//! `Boarder_InstanceCollisions` 0x125088 in the air (`AirMotion_Update` 0x108378 at 0x1089f8, board row E8q;
//! needs `TRICKY_DISC`, skipped without it):
//! - Flying into a Garibaldi knockable touch object spawns a `cRollerNode` (`Instance_OnPlayerContact` 0x13bd40,
//!   object flags `& ~0x22 | 0x45`), records `Sfx_InstanceCollision`, and on subsequent mid-air contacts kicks the
//!   moving roller (`roller::rider_kick`, applying rigid-body impulse and waking an asleep roller via `Roller_OnHit`).
//! - Flying into Elysium's fixed bouncy object `HalfPipeThing_Metal` 3403 (proven live in `port-notes/world.md`
//!   E8c4: flags 0xa100a3, mass 1e30, bounce 0.5) bounces the rider along the face normal by `(1 + 0.5) * v_in`,
//!   accumulates `knocks` (+0x300) at low speed, deducts `-0.02` meter (`Boarder_Stumble` 0x124dd0 without entering
//!   ground state 2) at moderate speed, and wipes out (`Boarder_WallImpactCheck` 0x126540 -> `Boarder_Wipeout`
//!   0x11d830) at high speed (`1600 cm/s`, matching the live capture at `1541 cm/s`).

use glam::Vec3;
use std::sync::Arc;
use tricky_game::collide::CollisionWorld;
use tricky_game::course_ground::CourseGround;
use tricky_game::rails::Rails;
use tricky_game::rider::{Input, Rider};

type V4 = [f32; 4];

fn load_course(name: &str) -> Option<CourseGround> {
    let disc = std::env::var("TRICKY_DISC").ok()?;
    let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc).ok()?);
    files.mount(&format!("data/models/{name}.big")).ok()?;
    let mut get = |ext: &str| files.load(&format!("|data/models/{name}.{ext}")).ok().flatten();
    let (pbd, ltg, ssf) = (get("pbd")?, get("ltg")?, get("ssf")?);
    CourseGround::new(&pbd, &ltg, &ssf)
}

fn port_pos(v: V4) -> Vec3 {
    Vec3::new(v[0], v[2], -v[1]) * 0.01
}

fn port_dir(v: V4) -> Vec3 {
    Vec3::new(v[0], v[2], -v[1])
}

fn game_vec(v: Vec3) -> [f32; 3] {
    [v.x, -v.z, v.y]
}

fn apply_mat(m: [V4; 4], v: V4) -> V4 {
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        out[c] = m[0][c] * v[0] + m[1][c] * v[1] + m[2][c] * v[2] + m[3][c] * v[3];
    }
    out
}

fn mul_mat(a: [V4; 4], b: [V4; 4]) -> [V4; 4] {
    [apply_mat(a, b[0]), apply_mat(a, b[1]), apply_mat(a, b[2]), apply_mat(a, b[3])]
}

fn air_rider(pos_cm: V4, vel_cm: V4) -> Rider {
    let heading = tricky_game::jump::atan2_inline(vel_cm[1], vel_cm[0]);
    let yaw = tricky_game::rider::wrap(heading - std::f32::consts::FRAC_PI_2);
    let mut r = Rider::new(port_pos(pos_cm), yaw);
    // Consume the initial `Boarder_PlaceAt` ground-snap ray against an empty world so the rider starts truly airborne.
    let mut empty = CollisionWorld::default();
    empty.min_y = -1e9;
    r.step(&empty, &Rails::default(), Input::default(), 0.0);
    r.pos = port_pos(pos_cm);
    r.yaw = yaw;
    r.grounded = false;
    r.set_motion(1);
    r.ctl = 0xd; // NATURAL_AIR
    r.air_time = 0.5;
    r.boost = 0.5;
    r.vel = port_dir(vel_cm) * 0.01;
    r
}

#[test]
fn air_knocks_garibaldi_prop_and_kicks_roller() {
    let Some(cg) = load_course("gari") else { return };
    // Collect live knockable touch objects in non-blank INodes (flags & 0x20, surface == -1) whose event-1 script
    // spawns a cRollerNode, then find the first one that `World_QueryInstances` returns directly (unshadowed).
    let candidates: Vec<(usize, V4)> = cg.with_world(|w| {
        let mut out = Vec::new();
        for cell in w.cells.iter().flatten() {
            for node in cell.nodes.iter().take(w.nodes_per_cell.max(0) as usize) {
                if node.blank {
                    continue;
                }
                for &i in node.objects.iter().take(node.n_objects.max(0) as usize) {
                    let o = &w.objects[i];
                    if o.flags & 0x20 == 0 || o.surface != -1 {
                        continue;
                    }
                    let s = cg.ssf.event_script(i, 1);
                    let spawns = s >= 0
                        && cg.ssf.effects.get(s as usize).is_some_and(|ops| {
                            ops.iter().any(|op| op.op == 0 && op.args.first() == Some(&0))
                        });
                    if spawns && w.trees.get(o.group as usize).is_some_and(|t| !t.is_empty()) {
                        let center = [
                            (o.min[0] + o.max[0]) * 0.5,
                            (o.min[1] + o.max[1]) * 0.5,
                            (o.min[2] + o.max[2]) * 0.5,
                            1.0,
                        ];
                        out.push((i, center));
                    }
                }
            }
        }
        out
    });
    let (idx, hit_pos) = candidates
        .into_iter()
        .find_map(|(i, c)| {
            for dz in (-120..=40).step_by(20) {
                let pos = [c[0], c[1], c[2] + dz as f32, 1.0];
                let r = air_rider(pos, [1200.0, 0.0, 150.0, 0.0]);
                let b = r.bind_body();
                let mut q = tricky_game::worldboarder::WorldBoarder::new(&b, 1);
                q.front_only = true;
                let (t, h) = cg.query_instances(&mut q, false, &|_| false);
                if t >= 0.0 && h.and_then(|h| h.object) == Some(i) {
                    return Some((i, pos));
                }
            }
            None
        })
        .expect("gari has an unshadowed live knockable touch object");

    let cg = Arc::new(cg);
    let mut world = CollisionWorld::default();
    world.min_y = -1e9;
    world.course = Some(cg.clone());

    // Step 1: airborne rider flies into the knockable prop -> spawns cRollerNode and records Sfx_InstanceCollision.
    let mut r = air_rider(hit_pos, [1200.0, 0.0, 150.0, 0.0]);
    r.step(&world, &Rails::default(), Input::default(), 1.0 / 60.0);
    assert!(!r.grounded, "rider must remain airborne");
    assert!(cg.knock.lock().unwrap().frame(idx as u32).is_some(), "airborne rider spawned cRollerNode for gari object {idx}");
    assert_eq!(cg.object_flags(idx) & 0x22, 0, "object {idx} left static casts");
    assert_eq!(cg.object_flags(idx) & 0x45, 0x45, "object {idx} has roller flags");
    assert!(
        cg.knock.lock().unwrap().knocks.iter().any(|&(o, _)| o == idx as u32),
        "Sfx_InstanceCollision recorded for object {idx}"
    );

    // Step 2: advance the roller 1 frame so its world-space box & sphere tree update, then fly an airborne rider into
    // the moving roller and verify `roller::rider_kick` imparts a rigid-body impulse; then put it asleep and verify
    // `Roller_OnHit` wakes it and plays `Sfx_InstanceCollision`.
    cg.knock.lock().unwrap().knocks.clear();
    cg.knock.lock().unwrap().tick(&cg, false, &|_| false, 1.0 / 60.0);
    let (rpos, p_before) = {
        let kn = cg.knock.lock().unwrap();
        let ro = kn.rollers.iter().find(|r| r.object == idx as u32).expect("roller alive");
        let center = [
            (ro.body.lo[0] + ro.body.hi[0]) * 0.5,
            (ro.body.lo[1] + ro.body.hi[1]) * 0.5,
            (ro.body.lo[2] + ro.body.hi[2]) * 0.5,
            1.0,
        ];
        (center, ro.body.rb.p)
    };
    let mut hit_offset = None;
    'kick: for dx in (-60..=0).step_by(20) {
        for dz in (-120..=40).step_by(20) {
            let mut r = air_rider([rpos[0] + dx as f32, rpos[1], rpos[2] + dz as f32, 1.0], [1500.0, 0.0, 150.0, 0.0]);
            r.step(&world, &Rails::default(), Input::default(), 1.0 / 60.0);
            let p_after = cg.knock.lock().unwrap().rollers.iter().find(|r| r.object == idx as u32).unwrap().body.rb.p;
            if p_after != p_before {
                hit_offset = Some((dx as f32, dz as f32));
                break 'kick;
            }
        }
    }
    let (dx, dz) = hit_offset.expect("airborne rider kicked awake roller via roller::rider_kick");

    // Put the roller completely at rest & asleep, then kick it again in mid-air: `Roller_OnHit` wakes it and records SFX.
    {
        let mut kn = cg.knock.lock().unwrap();
        kn.knocks.clear();
        let ro = kn.rollers.iter_mut().find(|r| r.object == idx as u32).unwrap();
        ro.body.asleep = 1;
        ro.body.rb.p = [0.0; 4];
        ro.body.rb.l = [0.0; 4];
        ro.body.rb.v = [0.0; 4];
        ro.body.rb.w = [0.0; 4];
    }
    let mut r = air_rider([rpos[0] + dx, rpos[1], rpos[2] + dz, 1.0], [1500.0, 0.0, 150.0, 0.0]);
    r.step(&world, &Rails::default(), Input::default(), 1.0 / 60.0);
    let kn = cg.knock.lock().unwrap();
    let ro = kn.rollers.iter().find(|r| r.object == idx as u32).unwrap();
    assert_eq!(ro.body.asleep, 0, "asleep roller woken by mid-air rider kick");
    assert!(kn.knocks.iter().any(|&(o, _)| o == idx as u32), "Roller_OnHit recorded Sfx_InstanceCollision");
}

#[test]
fn air_bounces_stumbles_and_crashes_on_elysium_fixed_object() {
    let Some(cg) = load_course("elysium") else { return };
    // Elysium `HalfPipeThing_Metal` 3403 (world.md E8c4): fixed bouncy touch object (flags 0xa100a3, mass 1e30, bounce 0.5).
    let idx = 3403usize;
    let id = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    // Extract actual world-space horizontal triangle centroids + normals from 3403's `.ssf` collision mesh.
    let tris: Vec<(V4, V4)> = cg.with_world(|w| {
        let o = &w.objects[idx];
        assert!(o.flags & 0xa0 == 0xa0 && o.surface == -1, "3403 is a bouncy touch object: {:#x} surf {}", o.flags, o.surface);
        let parts = &w.models[o.model];
        let mut pmat = vec![id; parts.len()];
        let mut n_mesh = 0usize;
        let mut out = Vec::new();
        for (k, p) in parts.iter().enumerate() {
            let local = p.matrix.unwrap_or(id);
            pmat[k] = if p.parent == -1 { local } else { mul_mat(pmat[p.parent as usize], local) };
            let m = mul_mat(o.rows, pmat[k]);
            if p.mesh.is_none() {
                continue;
            }
            if let Some(cm) = w.collision.get(o.group as usize).and_then(|g| g.get(n_mesh)) {
                for (ti, &[a, b, c]) in cm.tris.iter().enumerate() {
                    let (va, vb, vc) = (cm.verts[a as usize], cm.verts[b as usize], cm.verts[c as usize]);
                    let mid = [(va[0] + vb[0] + vc[0]) / 3.0, (va[1] + vb[1] + vc[1]) / 3.0, (va[2] + vb[2] + vc[2]) / 3.0, 1.0];
                    let pl = cm.planes[ti];
                    let wn = apply_mat(m, [pl[0], pl[1], pl[2], 0.0]);
                    let len = (wn[0] * wn[0] + wn[1] * wn[1] + wn[2] * wn[2]).sqrt();
                    if len > 0.5 && (wn[2] / len).abs() < 0.25 {
                        out.push((apply_mat(m, mid), [wn[0] / len, wn[1] / len, wn[2] / len, 0.0]));
                    }
                }
            }
            n_mesh += 1;
        }
        out
    });
    assert!(!tris.is_empty(), "3403 has horizontal collision triangles");

    // Find an airborne rider pose near a horizontal face of 3403 where `World_QueryInstances` returns 3403 cleanly.
    let mut contact = None;
    'outer: for (surf_p, surf_n) in tris {
        for d in (10..=100).step_by(15) {
            for dz in [-90, -60, -30] {
                let pos = [surf_p[0] + surf_n[0] * d as f32, surf_p[1] + surf_n[1] * d as f32, surf_p[2] + dz as f32, 1.0];
                let r = air_rider(pos, [-surf_n[0] * 400.0, -surf_n[1] * 400.0, 150.0, 0.0]);
                let b = r.bind_body();
                let mut q = tricky_game::worldboarder::WorldBoarder::new(&b, 1);
                q.front_only = true;
                let (t, h) = cg.query_instances(&mut q, false, &|_| false);
                if let Some(h) = h.filter(|h| t >= 0.0 && h.object == Some(idx)) {
                    let align = h.normal[0] * surf_n[0] + h.normal[1] * surf_n[1] + h.normal[2] * surf_n[2];
                    if h.normal[2].abs() < 0.25 && align > 0.9 {
                        contact = Some((pos, h.normal));
                        break 'outer;
                    }
                }
            }
        }
    }
    let (hit_pos, hit_n) = contact.expect("found airborne overlap on Elysium object 3403");

    let cg = Arc::new(cg);
    let mut world = CollisionWorld::default();
    world.min_y = -1e9;
    world.course = Some(cg.clone());

    let run = |speed_cm: f32| -> Rider {
        cg.knock.lock().unwrap().knocks.clear();
        // Start 1 tick before `hit_pos` along `hit_n` so RK4 lands cleanly on `hit_pos`.
        let dt = 1.0 / 60.0;
        let pos = [
            hit_pos[0] + hit_n[0] * speed_cm * dt,
            hit_pos[1] + hit_n[1] * speed_cm * dt,
            hit_pos[2] - 150.0 * dt,
            1.0,
        ];
        let vel = [-hit_n[0] * speed_cm, -hit_n[1] * speed_cm, 150.0, 0.0];
        let mut r = air_rider(pos, vel);
        r.step(&world, &Rails::default(), Input::default(), dt);
        assert!(
            cg.knock.lock().unwrap().knocks.iter().any(|&(o, _)| o == idx as u32),
            "Sfx_InstanceCollision recorded on 3403 at {speed_cm} cm/s (gnd={} mot={} cr={} cause={} knocks={:?})",
            r.grounded, r.motion, r.crashed, r.cause, cg.knock.lock().unwrap().knocks
        );
        r
    };

    // 1. Low speed (400 cm/s -> bounce speed 600 < 833.3333): bounces off face, increments air `knocks` (+0x300), stays up.
    let slow = run(400.0);
    let sv = game_vec(slow.vel * 100.0);
    let sv_n = sv[0] * hit_n[0] + sv[1] * hit_n[1] + sv[2] * hit_n[2];
    assert!(sv_n > 100.0, "bounced back along face normal: v·n = {sv_n}");
    assert!(slow.crashed <= 0.0, "low-speed hit did not wipe out");
    assert!(slow.impact > 0.0, "impact accumulated");
    assert!(slow.knocks > 0.0, "low-speed air knock incremented rider+0x300: {}", slow.knocks);

    // 2. Moderate speed (1000 cm/s -> s = 1500, above stumble limit 0 and below crash limit 1944.44):
    //    bounces off face, deducts -0.02 meter (`Boarder_Stumble` in air), does NOT enter ground stumble, stays up.
    let med = run(1000.0);
    let mv = game_vec(med.vel * 100.0);
    let mv_n = mv[0] * hit_n[0] + mv[1] * hit_n[1] + mv[2] * hit_n[2];
    assert!(mv_n > 300.0, "bounced back along face normal: v·n = {mv_n}");
    assert!(med.crashed <= 0.0, "moderate-speed hit did not wipe out");
    assert!(med.boost < 0.5, "air stumble deducted meter (was 0.5, now {})", med.boost);
    assert_eq!(med.stumble, 0.0, "air stumble does not enter ground stumble state 2");

    // 3. High speed (1600 cm/s -> s = 2400 > 1944.44): bounces and triggers `Boarder_Wipeout` (motion 5, state 0x13).
    let fast = run(1600.0);
    assert!(fast.crashed > 0.0, "high-speed hit on 3403 wiped out rider");
    assert_eq!(fast.motion, 5, "wipe-out motion 5");
    assert_eq!(fast.ctl, 0x13, "wipe-out control state 0x13");
}
