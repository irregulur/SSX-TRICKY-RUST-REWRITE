//! Out-of-bounds resets via live `.ssf` trigger scripts' op 13 (`TriggerScript_ExecOp` 0x13bfd0 at `0x13c3b4`:
//! `*(rider + 0x304) = 1; Boarder_RequestReset(rider, 101.46)`, board row E1b2g7d3b4h2b). Needs `TRICKY_DISC`;
//! tests are skipped cleanly when `TRICKY_DISC` is unset.

use glam::Vec3;
use std::sync::Arc;
use tricky_game::collide::CollisionWorld;
use tricky_game::course_ground::CourseGround;
use tricky_game::knockables::Rider as ScriptRider;
use tricky_game::rails::Rails;
use tricky_game::rider::{control_tick, Input, Rider};
use tricky_game::walls::Contact;

type V4 = [f32; 4];

fn load_course(name: &str) -> Option<CourseGround> {
    let disc = std::env::var("TRICKY_DISC").ok()?;
    let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc).ok()?);
    files.mount(&format!("data/models/{name}.big")).ok()?;
    let mut get = |ext: &str| files.load(&format!("|data/models/{name}.{ext}")).ok().flatten();
    let (pbd, ltg, ssf) = (get("pbd")?, get("ltg")?, get("ssf")?);
    CourseGround::new(&pbd, &ltg, &ssf)
}

fn object_center(cg: &CourseGround, obj: usize) -> V4 {
    cg.with_world(|w| {
        let o = &w.objects[obj];
        [(o.min[0] + o.max[0]) * 0.5, (o.min[1] + o.max[1]) * 0.5, (o.min[2] + o.max[2]) * 0.5, 1.0]
    })
}

fn game_to_port(p: V4) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1]) * 0.01
}

/// Find a world-space point (cm, z up) on a face of an op-13 static touch object (`flags & 0x20`, `surface == -1`,
/// `mode == 1`, no event-0 controller) so placing a rider right next to it triggers `World_QueryInstances`.
fn find_static_op13_face(cg: &CourseGround) -> Option<(usize, V4, V4)> {
    cg.with_world(|w| {
        for (o, obj) in w.objects.iter().enumerate() {
            if obj.flags & 0x20 == 0 || obj.surface != -1 || obj.mode != 1 {
                continue;
            }
            if cg.ssf.event_script(o, 0) >= 0 {
                continue;
            }
            let s1 = cg.ssf.event_script(o, 1);
            if s1 < 0 || !cg.ssf.effects[s1 as usize].iter().any(|op| op.op == 13) {
                continue;
            }
            let parts = &w.models[obj.model];
            let mut n_mesh = 0usize;
            for p in parts {
                if p.mesh.is_none() {
                    continue;
                }
                let cm = w.collision.get(obj.group as usize).and_then(|g| g.get(n_mesh));
                n_mesh += 1;
                let Some(cm) = cm else { continue };
                let Some(tri) = cm.tris.first() else { continue };
                let Some(plane) = cm.planes.first() else { continue };
                let mut local = [0.0f32; 4];
                for &vi in tri {
                    let v = cm.verts[vi as usize];
                    for k in 0..3 {
                        local[k] += v[k] / 3.0;
                    }
                }
                local[3] = 1.0;
                // Transform local centroid and plane normal by object matrix (rows)
                let m = obj.rows;
                let apply = |v: V4| -> V4 {
                    [
                        v[0] * m[0][0] + v[1] * m[1][0] + v[2] * m[2][0] + v[3] * m[3][0],
                        v[0] * m[0][1] + v[1] * m[1][1] + v[2] * m[2][1] + v[3] * m[3][1],
                        v[0] * m[0][2] + v[1] * m[1][2] + v[2] * m[2][2] + v[3] * m[3][2],
                        v[3],
                    ]
                };
                let wp = apply(local);
                let wn = apply([plane[0], plane[1], plane[2], 0.0]);
                return Some((o, wp, wn));
            }
        }
        None
    })
}

#[test]
fn gari_air_contact_with_op13_oob_box_resets_rider_without_mesh_standin() {
    let Some(cg) = load_course("gari") else { return };
    let (obj, wp, wn) = find_static_op13_face(&cg).expect("gari has a mode-1 static op-13 OOB box");
    let mut world = CollisionWorld::default();
    world.course = Some(Arc::new(cg));
    // Notice `world.resets` is empty: `world.in_reset` is false everywhere!
    let pos_cm = [wp[0] + wn[0] * 15.0, wp[1] + wn[1] * 15.0, wp[2] + wn[2] * 15.0 - 80.0, 1.0];
    let pos = game_to_port(pos_cm);
    assert!(!world.in_reset(pos + Vec3::Y * 0.8, 0.45), "mesh stand-in must be empty");

    let mut r = Rider::new(pos, 0.0);
    r.slot = 0;
    r.grounded = false;
    r.motion = 1; // AirMotion_Update -> Boarder_InstanceCollisions 0x125088
    r.ctl = control_tick::NATURAL_AIR;
    r.vel = game_to_port([-wn[0] * 500.0, -wn[1] * 500.0, -wn[2] * 500.0, 0.0]);

    let rails = Rails::default();
    r.step(&world, &rails, Input::default(), 1.0 / 60.0);

    assert!(r.reset.is_some(), "op 13 on gari OOB object {obj} puts rider into reset synchronously");
    assert_eq!(r.reset_flag, 1, "op 13 sets rider+0x304 = 1 before Boarder_RequestReset");
    assert_eq!(r.motion, 4, "reset enters stationary motion 4");
    assert_eq!(r.ctl, control_tick::RESET, "reset enters control state 0x16");
}

#[test]
fn gari_uvscroll_river_op13_respects_cooldown_and_resets_only_touching_rider() {
    let Some(cg) = load_course("gari") else { return };
    let mut rand = || 0u32;
    // Garibaldi river water 1712: mode 3 sphere tree, event 0 = cUVScrollNode (10), event 1 = [op 13 [0]].
    let at_1712 = object_center(&cg, 1712);
    cg.tick_cells(&[at_1712], 4, at_1712, false, &mut rand);
    assert_eq!(cg.controller_type(1712), Some(10), "cell activation spawns cUVScrollNode on river 1712");

    // Walk `1712`'s sphere tree (`mode == 3`) to an actual leaf sphere in world space so `World_QueryInstances`
    // tests real river geometry (proving `cUVScrollNode` type 10 is NOT skipped by `CourseGround::driven`).
    let leaf_world = cg.with_world(|w| {
        let obj = &w.objects[1712];
        let tree = &w.trees[obj.group as usize][0];
        let mut c = tree.centre;
        let mut idx = 0usize;
        for level in 0..tree.depth as usize {
            let mask = tree.masks.get(idx).copied().unwrap_or(0);
            let Some(oct) = (0..8).find(|&k| (mask >> k) & 1 != 0) else { break };
            let half = tree.levels[level + 1].1;
            let stride = tree.levels[level].2 as usize;
            for axis in 0..3 {
                c[axis] += tree.dirs[oct][axis] * half;
            }
            idx += (oct + 1) * stride;
        }
        let m = obj.rows;
        [
            c[0] * m[0][0] + c[1] * m[1][0] + c[2] * m[2][0] + m[3][0],
            c[0] * m[0][1] + c[1] * m[1][1] + c[2] * m[2][1] + m[3][1],
            c[0] * m[0][2] + c[1] * m[1][2] + c[2] * m[2][2] + m[3][2],
            1.0,
        ]
    });
    let body = tricky_game::collbody::CollBody {
        center: leaf_world,
        radius: 150.0,
        count: 1,
        x24: -1,
        spheres: [(leaf_world, 120.0); 18],
    };
    let mut q = tricky_game::worldboarder::WorldBoarder::new(&body, 1);
    q.front_only = true;
    let (t, hit) = cg.query_instances(&mut q, false, &|_| false);
    assert!(t >= 0.0 && hit.is_some_and(|h| h.object == Some(1712)), "World_QueryInstances hits cUVScrollNode river object 1712");

    let contact = Contact { pos: at_1712, dir: [0.0, 1.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0, 0.0], speed_in: 500.0 };
    let ai_rider = ScriptRider { id: 4, human: false, vel: [0.0, 500.0, 0.0, 0.0] };

    // During initial 30-tick `cUVScrollNode` cooldown (0x141ff0), `OnContact` 0x142690 blocks event 1.
    cg.knock.lock().unwrap().contact(&cg, 1712, ai_rider, &contact, [0; 16]);
    assert!(!cg.take_rider_reset(4), "cUVScrollNode initial 30-tick cooldown blocks contact");

    // Tick 31 frames (1 pending creation frame + 30 cooldown ticks): cooldown reaches 0.
    for _ in 0..31 {
        cg.tick_cells(&[at_1712], 4, at_1712, false, &mut rand);
    }
    cg.knock.lock().unwrap().contact(&cg, 1712, ai_rider, &contact, [0; 16]);
    assert!(!cg.take_rider_reset(1), "human rider (slot 0 -> handle 1) must not be reset by AI contact");
    assert!(cg.take_rider_reset(4), "AI rider (slot 3 -> handle 4) receives op 13 reset");

    // `cUVScrollNode::OnContact` 0x142690 rearmed cooldown to 30 ticks, so immediate re-contact is blocked.
    cg.knock.lock().unwrap().contact(&cg, 1712, ai_rider, &contact, [0; 16]);
    assert!(!cg.take_rider_reset(4), "rearmed 30-tick cooldown blocks immediate repeat contact");
}

#[test]
fn alaska_delayed_water_op13_waits_half_second_then_resets_rider() {
    let Some(cg) = load_course("alaska") else { return };
    let mut rand = || 0u32;
    // Alaska water 549: event 0 = cUVScrollNode (10), event 1 = effect 36 = [op 4 [0.5], op 13 [0]].
    let at_549 = object_center(&cg, 549);
    for _ in 0..32 {
        cg.tick_cells(&[at_549], 4, at_549, false, &mut rand);
    }
    assert_eq!(cg.controller_type(549), Some(10));

    let contact = Contact { pos: at_549, dir: [0.0, 1.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0, 0.0], speed_in: 400.0 };
    let human = ScriptRider { id: 1, human: true, vel: [0.0, 400.0, 0.0, 0.0] };
    cg.knock.lock().unwrap().contact(&cg, 549, human, &contact, [0; 16]);
    assert!(!cg.take_rider_reset(1), "Alaska water waits 0.5 s before op 13");

    let mut world = CollisionWorld::default();
    world.course = Some(Arc::new(cg));

    let mut r_human = Rider::new(game_to_port(at_549), 0.0);
    r_human.slot = 0;
    let mut r_ai = Rider::new(game_to_port(at_549), 0.0);
    r_ai.slot = 2;
    r_ai.ai = true;

    // 29 frames (< 0.5 s): thread is still waiting.
    for _ in 0..29 {
        world.tick_knockables(1.0 / 60.0);
        r_human.apply_script_reset(&world);
        r_ai.apply_script_reset(&world);
    }
    assert!(r_human.reset.is_none(), "not yet reset before 0.5 s");
    assert!(r_ai.reset.is_none());

    // Frame 30 (0.5 s elapsed): `op 13` fires for handle 1 only.
    world.tick_knockables(1.0 / 60.0);
    r_human.apply_script_reset(&world);
    r_ai.apply_script_reset(&world);
    assert!(r_human.reset.is_some(), "human resets after Alaska's 0.5 s op-4 wait -> op 13");
    assert_eq!(r_human.reset_flag, 1);
    assert!(r_ai.reset.is_none(), "other riders remain unaffected");
}

#[test]
fn megaplex_rotating_door_and_snowdream_debounce_op13_follow_controller_gates() {
    let mut rand = || 0u32;
    let contact = Contact { pos: [0.0; 4], dir: [0.0, 1.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0, 0.0], speed_in: 500.0 };
    let rider = ScriptRider { id: 1, human: true, vel: [0.0, 500.0, 0.0, 0.0] };

    // Megaplex rotating door 183: event 0 = cAnimObjectNode (0x100, rearm = 1.0 s = 60 ticks), event 1 = [op 13 [0]].
    if let Some(cg) = load_course("megaple") {
        let at_183 = object_center(&cg, 183);
        cg.tick_cells(&[at_183], 4, at_183, false, &mut rand);
        assert_eq!(cg.controller_type(183), Some(0x100));
        // `cAnimObjectNode::OnContact` 0x198eb8 starts with cooldown == 0 -> runs op 13 immediately and arms rearm!
        cg.knock.lock().unwrap().contact(&cg, 183, rider, &contact, [0; 16]);
        assert!(cg.take_rider_reset(1), "Megaplex rotating door 183 resets rider on contact");
        // Immediate repeat contact blocked while cooldown > 0.
        cg.knock.lock().unwrap().contact(&cg, 183, rider, &contact, [0; 16]);
        assert!(!cg.take_rider_reset(1), "cAnimObjectNode cooldown blocks repeat contact");
    }

    // Snowdream 1087: event 0 = cDebounceNode (2, 4.0 s = 240 ticks), event 1 = [op 13 [0]].
    if let Some(cg) = load_course("snow") {
        let at_1087 = object_center(&cg, 1087);
        cg.tick_cells(&[at_1087], 4, at_1087, false, &mut rand);
        assert_eq!(cg.controller_type(1087), Some(2));
        // Blocked while cDebounceNode is attached.
        cg.knock.lock().unwrap().contact(&cg, 1087, rider, &contact, [0; 16]);
        assert!(!cg.take_rider_reset(1), "cDebounceNode blocks contact while active");
        // Advance past 4.0 s (pending frame + 240 ticks + 1): debounce finishes and detaches.
        for _ in 0..245 {
            cg.tick_cells(&[at_1087], 4, at_1087, false, &mut rand);
        }
        assert_eq!(cg.controller_type(1087), None, "cDebounceNode detached after 4.0 s");
        cg.knock.lock().unwrap().contact(&cg, 1087, rider, &contact, [0; 16]);
        assert!(cg.take_rider_reset(1), "Snowdream 1087 resets rider once debounce expires");
    }
}
