//! Upside-down wipe-out collision body setup and initial `Ragdoll_Contacts` 0x10e190 push (board row E7c3j):
//! - Verifies `Boarder_UpdatePose` 0x129628 + `Boarder_UpdateSkeleton` 0x12ae18 + `CollBody_UpdateFromBones`
//!   0x2361b0 bit-for-bit against `SLUS_203.26` in `r5900::Runner` for motion 5 / state 0x13 (`Wipeout`) with
//!   the upside-down attitude `up = unit(-0.08, -0.16, -0.98)` on real disc impact & crunch clips (`0x2e4`,
//!   `0x2eb`, `0x2df`).
//! - Verifies an upside-down head-first fall onto `alaska` snow with live `AnimHost`: enters `WipeoutMotion`
//!   immediately on the crash tick (`Boarder_SetState(0x13)` -> `Boarder_SetMotion(5)` -> `WipeoutMotion_Enter`
//!   0x10c198), plays a head-impact clip (`0x2e4` / `0x2eb`), keeps `Rider::att` and `AnimHost` synced across
//!   every tick, never shifts the posed `CollBody` by `rag.height` (+0x1bc), and has no 3.1 m jump on the
//!   initial `Ragdoll_Contacts` push.
//! - Skips disc/ELF-dependent tests gracefully when `TRICKY_DISC` or `TRICKY_ELF` is not set.

use glam::Vec3;
use std::sync::{Arc, Mutex};
use tricky_data::character::CharModel;
use tricky_game::animhost::{AnimHost, AnimRiderInputs, PoseUpdateIn, RiderAnim, RosterEntry};
use tricky_game::collide::CollisionWorld;
use tricky_game::course_ground::CourseGround;
use tricky_game::orient::{self, Orient};
use tricky_game::rails::Rails;
use tricky_game::rider::wipeout::{CLIP_CRUNCH, clip_name};
use tricky_game::rider::{control_tick, Input, Rider};

type V4 = [f32; 4];

fn disc_and_elf() -> Option<(Vec<u8>, tricky_data::big::Files)> {
    let disc = std::env::var("TRICKY_DISC").ok()?;
    let elf_path = std::env::var("TRICKY_ELF").ok()?;
    let elf = std::fs::read(&elf_path).ok()?;
    let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc).ok()?);
    files.mount_game_archives().ok()?;
    Some((elf, files))
}

fn alaska_course(files: &mut tricky_data::big::Files) -> Option<(CourseGround, tricky_data::pbd::Pbd)> {
    files.mount("data/models/alaska.big").ok()?;
    let mut get = |ext: &str| files.load(&format!("|data/models/alaska.{ext}")).ok().flatten();
    let (pbd, ltg, ssf) = (get("pbd")?, get("ltg")?, get("ssf")?);
    Some((CourseGround::new(&pbd, &ltg, &ssf)?, tricky_data::pbd::Pbd::parse(&pbd)?))
}

fn port(v: V4) -> Vec3 {
    Vec3::new(v[0], v[2], -v[1]) * 0.01
}

/// Build an exact `Orient` whose board up (`+0x1a0`) is `unit(target_up)`.
fn upside_down_orient(target_up: [f32; 3]) -> Orient {
    let len = (target_up[0] * target_up[0] + target_up[1] * target_up[1] + target_up[2] * target_up[2]).sqrt();
    let up = [target_up[0] / len, target_up[1] / len, target_up[2] / len, 0.0];
    let q = orient::rotate_vec_to_vec([1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], up);
    let mut o = Orient { q, ..Default::default() };
    orient::update_orientation(&mut o);
    o
}

/// Find a wide, flat skiable snow patch on `cg` clear of touch objects and cliffs along the drop corridor.
fn flat_snow(cg: &CourseGround, pbd: &tricky_data::pbd::Pbd) -> V4 {
    for seg in pbd.segments.iter() {
        let f = |i: usize| f32::from_bits(seg[i]);
        for dx in [0.0f32, 300.0, -300.0] {
            let p = [f(12) + dx, f(13) + dx, f(14) + 150.0, 1.0];
            let mut q = tricky_game::worldline::WorldLine::new(0.0, p, [p[0], p[1], p[2] - 800.0, 1.0], 0);
            let (t, h) = cg.ray_cast(&mut q, false, None, &|_| false);
            if let Some(h) = h.filter(|_| 0.0 <= t) {
                if 0.96 < h.normal[2] && !matches!(h.surface, 0 | 6 | 10) {
                    let clear = [(0.0f32, 0.0f32), (200.0, 0.0), (400.0, 0.0), (200.0, 200.0), (200.0, -200.0)]
                        .into_iter()
                        .all(|(ox, oy)| {
                            let sp = [h.pos[0] + ox, h.pos[1] + oy, h.pos[2] + 200.0, 1.0];
                            let mut sq = tricky_game::worldline::WorldLine::new(0.0, sp, [sp[0], sp[1], sp[2] - 500.0, 1.0], 0);
                            let (st, sh) = cg.ray_cast(&mut sq, false, None, &|_| false);
                            let flat = sh.filter(|_| 0.0 <= st).is_some_and(|sh| {
                                sh.normal[2] > 0.92 && (sh.pos[2] - h.pos[2]).abs() < 120.0 && !matches!(sh.surface, 0 | 6 | 10)
                            });
                            let r = Rider::new(port([h.pos[0] + ox, h.pos[1] + oy, h.pos[2] + 150.0, 1.0]), -std::f32::consts::FRAC_PI_2);
                            let b = r.bind_body();
                            let mut iq = tricky_game::worldboarder::WorldBoarder::new(&b, 1);
                            iq.front_only = true;
                            flat && cg.query_instances(&mut iq, false, &|_| false).0 < 0.0
                        });
                    if clear {
                        return h.pos;
                    }
                }
            }
        }
    }
    panic!("no flat snow found on alaska");
}

#[test]
fn upside_down_wipeout_pose_and_collbody_match_original_runner() {
    let Some((elf, mut files)) = disc_and_elf() else { return };
    let body_model = CharModel::rider_from_disc(&mut files, "eddie").expect("load eddie");
    let board_model = CharModel::board_from_disc(&mut files, Some("Bx")).expect("load Bx board");
    let body_bones = body_model.slot_raw_bones(0);
    let board_bones = board_model.slot_raw_bones(1);
    assert_eq!(body_bones.len(), 19);
    assert_eq!(board_bones.len(), 1);

    let roster = [RosterEntry { character: 0, board_class: 0, load_uber: true }];
    let mut host = AnimHost::from_disc(&elf, &mut files, &roster).expect("init AnimHost");
    let slot = host.add_rider(0, 0, 1.0, &[(0, &body_bones), (1, &board_bones)]);
    host.set_board_ik_points(slot, &board_model.ik_points);

    let mut r = r5900::Runner::from_elf(&elf).expect("load elf");
    for a in [
        0x12ae18u32, // Boarder_UpdateSkeleton
        0x2083d8,    // RiderSkin_SetRootMatrix
        0x2083e8,    // Boarder_SetSkinChildMatrices
        0x208398,    // Boarder_GetBoneWorldPos
        0x2083f8,    // Boarder_BuildBoneMatrices
        0x2057a0,    // SkinPart_SetChildMatrices
        0x2059f8,    // Skel_GetBoneWorldPos
        0x205a78,    // Boarder_PoseBonesFromAngles
        0x205e98,    // Boarder_SolveLimbIk
        0x250c10,    // Math_SinCos
        0x250fd0,    // Math_Asin
        0x102f50,    // Vec4_Scale
        0x15fdd0,    // AnimCtl_GetClip
        0x15eec0,    // AnimCtl_GetClipType
        0x2361b0,    // CollBody_UpdateFromBones
        0x236050,    // CollBody_InitRider
        0x2cc808,    // EAMem_Set
    ] {
        r.allow(a);
    }

    let mut rng_state = 0x1234_5678_9abc_def0u64;
    let mut next_rng = || {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (rng_state >> 32) as u32
    };

    // Exact E7c3j upside-down attitude: up = (-0.08, -0.16, -0.98) in game axes.
    let o = upside_down_orient([-0.08, -0.16, -0.98]);
    assert!(o.up[2] < -0.97, "board up must point straight down: {:?}", o.up);
    let rider_addr = AnimHost::rider_addr(slot);

    for clip in [0x2e4u32, 0x2eb, CLIP_CRUNCH as u32] {
        host.blend_out_all(slot);
        host.play_clip(slot, clip, 0, 5, &mut next_rng);
        host.sync_rider(slot, &AnimRiderInputs { board_class: 0, ..Default::default() });
        for _ in 0..3 {
            host.flush_ended(slot, &mut next_rng);
            host.update_layers(slot, 1.0, &mut next_rng);
        }

        // Exact `Boarder_UpdatePose` 0x129628 inputs during motion 5 / state 0x13:
        // `up` is rider+0x2c0 (smoothed ground normal ~+Z), `height` is rider+0x308 (`body_height` = 0.0,
        // never `rag.height` +0x1bc!).
        let pin = PoseUpdateIn {
            quat: o.q,
            steer: 0.0,
            motion: 5,
            state: control_tick::WIPEOUT,
            class: 0,
            natural_stance: 0,
            pos: [500.0, -1200.0, 350.0, 1.0],
            up: [0.02, -0.04, 0.999, 0.0],
            normal: [0.0, 0.0, 1.0, 0.0],
            height: 0.0,
            side: o.side,
            lean: 0.0,
            scale: 1.0,
            spin: 0.0,
            flip: 0.0,
            x1c0: 0.0,
            x1cc: 0.0,
        };
        let snap = host.update_pose(slot, &pin);

        // Verify the posed head sphere (sphere 0) sits ~144 cm BELOW the upside-down board pos (`z = 350`).
        let head_z = snap.coll_body.spheres[0].0[2];
        assert!(
            head_z < pin.pos[2] - 80.0 && head_z > pin.pos[2] - 180.0,
            "clip {clip:#x}: upside-down posed head_z={head_z} expected below board pos_z={}",
            pin.pos[2]
        );

        // Run the exact PS2 `Boarder_UpdatePose` 0x129628 + `CollBody_UpdateFromBones` 0x2361b0 in `r5900::Runner`.
        for &(base, ref bytes) in &host.mem().regions {
            r.mem.write(base, bytes);
        }
        let put4 = |m: &mut r5900::Mem, at: u32, v: [f32; 4]| {
            for (k, &x) in v.iter().enumerate() {
                m.write_f32(at + 4 * k as u32, x);
            }
        };
        let stats = r.mem.read_u32(rider_addr + 0x464);
        r.mem.write_u32(stats + 0x40, pin.natural_stance);
        r.mem.write_f32(stats + 0x60, pin.scale);
        r.mem.write_u32(stats + 0x78, 0);
        put4(&mut r.mem, rider_addr + 0x140, pin.pos);
        put4(&mut r.mem, rider_addr + 0x170, pin.quat);
        r.mem.write_f32(rider_addr + 0x1c0, 0.0);
        r.mem.write_f32(rider_addr + 0x1cc, 0.0);
        r.mem.write_f32(rider_addr + 0x1d0, 0.0);
        r.mem.write_f32(rider_addr + 0x1d4, 0.0);
        r.mem.write_f32(rider_addr + 0x214, pin.steer);
        r.mem.write_f32(rider_addr + 0x244, pin.lean);
        put4(&mut r.mem, rider_addr + 0x2a0, pin.normal);
        put4(&mut r.mem, rider_addr + 0x2c0, pin.up);
        r.mem.write_f32(rider_addr + 0x308, pin.height);
        put4(&mut r.mem, rider_addr + 0x330, pin.side);
        r.mem.write_u32(rider_addr + 0x420, pin.class);
        r.mem.write_u32(rider_addr + 0x424, pin.motion);
        r.mem.write_u32(rider_addr + 0x428, pin.state);

        let body_part = r.mem.read_u32(rider_addr + 0x4494);
        let body_mats_ptr = r.mem.read_u32(body_part + 4);
        r.mem.write(body_mats_ptr, &vec![0u8; 19 * 0x40]);
        r.mem.write(rider_addr + 0x4a60, &[0u8; 0x80]);

        r.cpu.set_gpr(4, rider_addr as u64);
        r.call(0x129628).expect("Boarder_UpdatePose");

        // Compare all 19 world bone matrices bit-for-bit against `SLUS_203.26`.
        for b in 0..19u32 {
            let theirs: Vec<u32> = (0..16).map(|i| r.mem.read_u32(body_mats_ptr + b * 0x40 + 4 * i)).collect();
            let ours: Vec<u32> = snap.body_world_mats[b as usize].iter().flat_map(|row| row.map(f32::to_bits)).collect();
            assert_eq!(theirs, ours, "clip {clip:#x} bone {b} world matrix mismatch");
        }

        // Compare `CollBody_UpdateFromBones` 0x2361b0 against `snap.coll_body` bit-for-bit!
        let cb_addr = rider_addr + 0x470;
        let saved_rules = r.float_rules;
        r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
        r.cpu.set_gpr(4, cb_addr as u64);
        r.cpu.set_f(12, pin.scale);
        r.call(0x236050).expect("CollBody_InitRider");
        r.mem.write_u32(cb_addr, body_mats_ptr);
        r.cpu.set_gpr(4, cb_addr as u64);
        r.call(0x2361b0).expect("CollBody_UpdateFromBones");
        r.float_rules = saved_rules;

        for k in 0..4u32 {
            assert_eq!(
                r.mem.read_u32(cb_addr + 0x10 + 4 * k),
                snap.coll_body.center[k as usize].to_bits(),
                "clip {clip:#x} center[{k}]"
            );
        }
        assert_eq!(r.mem.read_u32(cb_addr + 0x20), snap.coll_body.radius.to_bits());
        for s in 0..18u32 {
            let base = cb_addr + 0x30 + s * 0x20;
            for k in 0..4u32 {
                assert_eq!(
                    r.mem.read_u32(base + 4 * k),
                    snap.coll_body.spheres[s as usize].0[k as usize].to_bits(),
                    "clip {clip:#x} sphere[{s}][{k}]"
                );
            }
            assert_eq!(
                r.mem.read_u32(base + 0x10),
                snap.coll_body.spheres[s as usize].1.to_bits(),
                "clip {clip:#x} sphere[{s}] radius"
            );
        }
    }
}

#[test]
fn upside_down_head_first_landing_on_alaska_uses_posed_body_and_small_push() {
    let Some((elf, mut files)) = disc_and_elf() else { return };
    let Some((cg, pbd)) = alaska_course(&mut files) else { return };
    let body_model = CharModel::rider_from_disc(&mut files, "eddie").expect("load eddie");
    let board_model = CharModel::board_from_disc(&mut files, Some("Bx")).expect("load Bx board");
    let body_bones = body_model.slot_raw_bones(0);
    let board_bones = board_model.slot_raw_bones(1);

    let roster = [RosterEntry { character: 0, board_class: 0, load_uber: true }];
    let mut host = AnimHost::from_disc(&elf, &mut files, &roster).expect("init AnimHost");
    let slot = host.add_rider(0, 0, 1.0, &[(0, &body_bones), (1, &board_bones)]);
    host.set_board_ik_points(slot, &board_model.ik_points);
    let anim = RiderAnim::new(Arc::new(Mutex::new(host)), slot);

    let at = flat_snow(&cg, &pbd);
    let mut world = CollisionWorld::default();
    world.min_y = -1e9;
    world.course = Some(Arc::new(cg));

    // Drop the rider upside down (`up = (-0.08, -0.16, -0.98)`) from 220 cm above alaska snow.
    // The head sphere is ~145 cm below the board, so it reaches snow cleanly before the board does.
    let o = upside_down_orient([-0.08, -0.16, -0.98]);
    let mut r = Rider::new(port([at[0], at[1], at[2] + 220.0, 1.0]), -std::f32::consts::FRAC_PI_2);
    r.grounded = false;
    r.set_motion(1);
    r.ctl = control_tick::NATURAL_AIR;
    r.air_time = 0.4;
    r.vel = port([600.0, 0.0, -400.0, 0.0]);
    r.att = Some((o, r.yaw));
    r.attach_anim(anim.clone());

    let mut crashed_tick = None;
    let mut max_tumble_step_m = 0.0f32;
    let mut max_center_step_m = 0.0f32;
    let mut prev_center = anim.pose_snapshot().expect("initial pose_snapshot").coll_body.center;
    let mut att_changed_in_wipe = false;
    let mut crash_q = [0.0f32; 4];

    for tick in 0..90 {
        let before = r.pos;
        let was_crashed = r.crashed > 0.0;
        r.step(&world, &Rails::default(), Input::default(), 1.0 / 60.0);
        let delta = (r.pos - before).length();
        let snap = anim.pose_snapshot().expect("pose_snapshot present");
        let dc = ((snap.coll_body.center[0] - prev_center[0]).powi(2)
            + (snap.coll_body.center[1] - prev_center[1]).powi(2)
            + (snap.coll_body.center[2] - prev_center[2]).powi(2))
        .sqrt()
            * 0.01;
        prev_center = snap.coll_body.center;
        if !was_crashed && r.crashed > 0.0 {
            crashed_tick = Some(tick);
            let crash_clip = anim.clip(0);
            crash_q = r.att.expect("att set on crash").0.q;
            assert!(r.wipe.is_some(), "Boarder_Wipeout must enter WipeoutMotion immediately on crash tick");
            assert_eq!(r.motion, 5, "motion must be 5 on crash tick");
            assert_eq!(r.ctl, control_tick::WIPEOUT, "ctl must be 0x13 on crash tick");
            // `Wipeout_PlayImpactClip` 0x10f9a0: `up · normal ≈ -0.98 < -0.7` selects cmCI_HEAD (0x2e4) or cmI_HEAD (0x2eb).
            assert!(
                matches!(crash_clip, 0x2e4 | 0x2eb),
                "upside-down head-first impact must play head impact clip (0x2e4/0x2eb), got {crash_clip:#x} ({})",
                clip_name(crash_clip as u16)
            );
        }
        if let Some(ct) = crashed_tick {
            if tick >= ct {
                if r.motion == 5 && tick <= ct + 15 {
                    max_tumble_step_m = max_tumble_step_m.max(delta);
                    max_center_step_m = max_center_step_m.max(dc);
                }
                assert!(
                    delta < 2.5,
                    "tick {tick} (ct+{}): unexpected >2.5 m jolt ({delta:.3} m)",
                    tick - ct
                );
                if let Some((cur_o, _)) = r.att {
                    let dq = (0..4).map(|k| (cur_o.q[k] - crash_q[k]).abs()).fold(0.0f32, f32::max);
                    if dq > 1e-3 {
                        att_changed_in_wipe = true;
                    }
                }
                // Cached `AnimHost` collision body must remain near the rider's actual position (never shifted
                // hundreds of cm into the snow by `rag.height` +0x1bc!).
                let rpos = [r.pos.x * 100.0, -r.pos.z * 100.0, r.pos.y * 100.0];
                let d_body = ((snap.coll_body.center[0] - rpos[0]).powi(2)
                    + (snap.coll_body.center[1] - rpos[1]).powi(2)
                    + (snap.coll_body.center[2] - rpos[2]).powi(2))
                .sqrt();
                assert!(
                    d_body < 160.0,
                    "tick {tick}: posed CollBody center drifted {d_body:.1} cm from rider pos (likely wrong PoseUpdateIn height/up)"
                );
            }
        }
    }

    assert!(crashed_tick.is_some(), "upside-down rider falling onto alaska snow must wipe out");
    assert!(att_changed_in_wipe, "Rider::att must track the tumbling ragdoll quaternion during wipe-out");
    assert!(
        max_tumble_step_m < 0.45,
        "initial Ragdoll_Contacts push and first 15 tumble ticks must have no 3.1 m jolt: max_tumble_step_m = {max_tumble_step_m:.3} m"
    );
    assert!(
        max_center_step_m < 0.45,
        "posed CollBody bounding center must move smoothly across crash & initial tumble: max_center_step_m = {max_center_step_m:.3} m"
    );
}

#[test]
fn upside_down_flip_wipeout_has_no_jolt_with_or_without_anim() {
    let Some((elf, mut files)) = disc_and_elf() else { return };
    let Some((cg, pbd)) = alaska_course(&mut files) else { return };
    let body_model = CharModel::rider_from_disc(&mut files, "eddie").expect("load eddie");
    let board_model = CharModel::board_from_disc(&mut files, Some("Bx")).expect("load Bx board");
    let body_bones = body_model.slot_raw_bones(0);
    let board_bones = board_model.slot_raw_bones(1);
    let at = flat_snow(&cg, &pbd);
    let mut world = CollisionWorld::default();
    world.min_y = -1e9;
    world.course = Some(Arc::new(cg));

    for use_anim in [true, false] {
        let mut r = Rider::new(port([at[0], at[1], at[2] + 180.0, 1.0]), -std::f32::consts::FRAC_PI_2);
        r.grounded = false;
        r.set_motion(1);
        r.set_control(control_tick::SPIN);
        r.flip = std::f32::consts::PI;
        r.air_time = 0.4;
        r.vel = port([600.0, 0.0, -400.0, 0.0]);
        let anim = if use_anim {
            let roster = [RosterEntry { character: 0, board_class: 0, load_uber: true }];
            let mut host = AnimHost::from_disc(&elf, &mut files, &roster).expect("init AnimHost");
            let slot = host.add_rider(0, 0, 1.0, &[(0, &body_bones), (1, &board_bones)]);
            host.set_board_ik_points(slot, &board_model.ik_points);
            let a = RiderAnim::new(Arc::new(Mutex::new(host)), slot);
            r.attach_anim(a.clone());
            Some(a)
        } else {
            None
        };

        let mut crashed_tick = None;
        let mut max_tumble_step_m = 0.0f32;
        let mut max_center_step_m = 0.0f32;
        let mut prev_center = anim.as_ref().and_then(|a| a.pose_snapshot()).map(|s| s.coll_body.center);
        for tick in 0..80 {
            let before = r.pos;
            let was_crashed = r.crashed > 0.0;
            r.step(&world, &Rails::default(), Input::default(), 1.0 / 60.0);
            let delta = (r.pos - before).length();
            if !was_crashed && r.crashed > 0.0 {
                crashed_tick = Some(tick);
            }
            if let Some(a) = &anim {
                let c = a.pose_snapshot().expect("pose_snapshot").coll_body.center;
                if let Some(pc) = prev_center {
                    let dc = ((c[0] - pc[0]).powi(2) + (c[1] - pc[1]).powi(2) + (c[2] - pc[2]).powi(2)).sqrt() * 0.01;
                    if r.motion == 5 {
                        max_center_step_m = max_center_step_m.max(dc);
                    }
                }
                prev_center = Some(c);
            }
            // Measure every motion-5 tick after the crash tick (`SpinControl_Exit` folds `flip = pi` into `att`
            // on the crash tick itself, moving `rider+0x140` by `2 * pivot_z` while keeping the pivot/body fixed).
            if let Some(ct) = crashed_tick {
                if tick > ct && r.motion == 5 {
                    max_tumble_step_m = max_tumble_step_m.max(delta);
                }
            }
        }
        assert!(crashed_tick.is_some(), "use_anim={use_anim}: flipped upside-down drop must wipe out");
        assert!(
            max_tumble_step_m < 0.55,
            "use_anim={use_anim}: flipped upside-down Ragdoll_Contacts push & tumble must not jump out of snow: max_tumble_step_m={max_tumble_step_m:.3} m"
        );
        if use_anim {
            assert!(
                max_center_step_m < 0.55,
                "use_anim=true: CollBody center must stay put across SpinControl_Exit & Ragdoll_Contacts: max_center_step_m={max_center_step_m:.3} m"
            );
        }
    }
}
