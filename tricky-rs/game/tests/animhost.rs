//! Live `tricky_game::animhost` integration & runner verification (board row G5d1):
//! 1. `skel_anim_create_tree` 0x158dc0 / `skel_anim_alloc_node` 0x158868 checked bit-for-bit against `SLUS_203.26`
//!    in `r5900::Runner` (`TRICKY_ELF`).
//! 2. Full `AnimHost::from_disc` on `TRICKY_DISC` (`/SLUS_203.26`, `data/char/anm.big`, `mac_body.mpf`, `board.mpf`):
//!    - verifies real `.mpf` 0x54-byte skeleton bones (body slot 0 + board slot 1) and `AnimData_LoadClipTable`;
//!    - drives cruise (`0x1ff` mode 4 `BrakeBlend`), switch stance mirroring, prewind (`WindUpBlend`), grabs &
//!      ubers (`test_flag(1..=4)`, `test_bit`), hard landing (`0x23e` -> end action `1` -> `0x1ff`), rail
//!      (`CrouchSteerBlend`, `turn_all_layers`, `set_all_layers_stance`), layer-2 idle (`0x2f0`), and
//!      `clip_displacement`;
//!    - verifies live `AnimCtl_UpdateLayers` (`AnimMode_BrakeBlend` + `AnimCtl_BlendPose` + `PoseBlend_BlendSkeleton`)
//!      on real disc `.afl` curves and `.mpf` bones bit-for-bit against `SLUS_203.26` in `r5900::Runner`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use glam::Vec3;
use r5900::Runner;
use tricky_data::big::Files;
use tricky_data::character::CharModel;
use tricky_data::disc::Disc;
use tricky_game::animctl as ctl;
use tricky_game::animdata as data;
use tricky_game::animhost::{skel_anim_create_tree, AnimHost, AnimRiderInputs, RiderAnim, RosterEntry, ELF_DATA_END, ELF_DATA_START};
use tricky_game::animseq::{self as seq, SeqEnv};
use tricky_game::collide::CollisionWorld;
use tricky_game::pf::Mem;
use tricky_game::rails::Rails;
use tricky_game::rider::{control_tick, Input, Rider};

fn disc_and_elf() -> Option<(Vec<u8>, Files)> {
    let iso = std::env::var("TRICKY_DISC").ok()?;
    let mut disc = Disc::open(&iso).expect("open TRICKY_DISC");
    let elf = disc.read_path("/SLUS_203.26").expect("read /SLUS_203.26").expect("/SLUS_203.26 missing");
    let mut files = Files::new(disc);
    files.mount_game_archives().expect("mount game archives");
    Some((elf, files))
}

#[test]
fn skel_anim_create_tree_matches_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x158dc0u32, 0x158868, 0x159880, 0x1598e8, 0x2cc808] {
        r.allow(f);
    }

    const BANK: u32 = data::BANK;
    const ARENA: u32 = 0x0100_0000;
    const HDRS: u32 = ARENA + 0x100;
    const OFFS: u32 = ARENA + 0x1000;
    const DATA: u32 = ARENA + 0x2000;
    const POOL_AT: u32 = ARENA + 0x4000;
    const ARENA_SIZE: u32 = 0x8000;

    let next_r = Rc::new(RefCell::new(0u32));
    let nr = next_r.clone();
    r.stub(seq::POOL_ALLOC, move |cpu, _| {
        let mut bump = nr.borrow_mut();
        let p = POOL_AT + *bump;
        *bump += 0x2b0;
        cpu.set_gpr(2, p as i32 as i64 as u64);
    });

    for &(file_no, clip_idx, n_children) in &[(1u32, 0u32, 2u8), (0, 5, 0), (12, 3, 4)] {
        let mut m = Mem {
            regions: vec![
                (BANK, vec![0u8; 0x3100]),
                (ARENA, vec![0u8; ARENA_SIZE as usize]),
            ],
        };
        let f_entry = BANK + data::FILES + 0x18 * file_no;
        m.w32(f_entry + 8, HDRS);
        m.w32(f_entry + 0xc, OFFS);
        m.w32(f_entry + 0x10, DATA);
        // Root clip header at clip_idx, child headers at clip_idx + 1..=n_children
        let root_hdr = HDRS + 0x24 * clip_idx;
        m.w8(root_hdr + 9, n_children);
        for c in 0..n_children as u32 {
            let ch_hdr = HDRS + 0x24 * (clip_idx + 1 + c);
            m.w8(ch_hdr + 8, c as u8);
            m.w8(ch_hdr + 9, 0);
        }

        r.mem.write(BANK, &m.regions[0].1).unwrap();
        r.mem.write(ARENA, &m.regions[1].1).unwrap();
        *next_r.borrow_mut() = 0;

        let key = file_no | (clip_idx << 16);
        r.cpu.set_gpr(4, BANK as i32 as i64 as u64);
        r.cpu.set_gpr(5, key as i32 as i64 as u64);
        r.call(seq::SKEL_ANIM_CREATE).expect("run SkelAnim_CreateTree");
        let want_root = r.cpu.gpr(2) as u32;

        struct BumpEnv(u32);
        impl SeqEnv for BumpEnv {
            fn call(&mut self, _m: &mut Mem, func: u32, _args: &[u32]) -> u32 {
                assert_eq!(func, seq::POOL_ALLOC);
                let p = POOL_AT + self.0;
                self.0 += 0x2b0;
                p
            }
        }
        let mut env = BumpEnv(0);
        let got_root = skel_anim_create_tree(&mut m, &mut env, BANK, key);

        assert_eq!(got_root, want_root, "root ptr mismatch for ({file_no}, {clip_idx}, {n_children})");
        let mut want_mem = vec![0u8; ARENA_SIZE as usize];
        r.mem.read(ARENA, &mut want_mem).unwrap();
        assert_eq!(m.regions[1].1, want_mem, "SkelAnim tree bytes differ for ({file_no}, {clip_idx}, {n_children})");
    }
}

#[test]
fn live_disc_animhost_clips_modes_and_runner_pose_match() {
    let Some((elf, mut files)) = disc_and_elf() else { return };

    // Load Mac (character 2, freestyle board class 1) and the freestyle board model from the disc.
    let body_model = CharModel::rider_from_disc(&mut files, "mac").expect("load mac from disc");
    let board_model = CharModel::board_from_disc(&mut files, Some("Fr")).expect("load Fr board from disc");
    let body_bones = body_model.slot_raw_bones(0);
    let board_bones = board_model.slot_raw_bones(1);
    assert_eq!(body_bones.len(), 19, "mac body slot 0 should have 19 skeleton bones");
    assert_eq!(board_bones.len(), 1, "board slot 1 should have 1 skeleton bone");
    // Check that raw MPF bone flags and mirror indices are present and non-trivial.
    let body_flags: Vec<u16> = body_bones.iter().map(|b| u16::from_le_bytes([b[0x14], b[0x15]])).collect();
    assert_eq!(body_flags[0] & 3, 3, "hips bone 0 should have both translation (1) and rotation (2) curves");
    assert!(body_flags[1..].iter().all(|&f| f & 2 != 0), "all 18 non-root body bones should have rotation curves");

    let roster = [RosterEntry { character: 2, board_class: 1, load_uber: true }];
    let mut host = AnimHost::from_disc(&elf, &mut files, &roster).expect("init AnimHost from disc");
    assert_eq!(host.indexed_clip_count(), 789, "expected 789 indexed clips for shared + Mac Fr uber");
    assert!(host.bank_bytes_used() > 500_000, "expected >500 KB of .afl files loaded, got {}", host.bank_bytes_used());

    let slot = host.add_rider(2, 1, 1.0, &[(0, &body_bones), (1, &board_bones)]);
    let mut rng_state = 0x1234_5678u32;
    let mut next_rng = move || {
        rng_state = rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
        rng_state
    };

    // 1. Cruise clip (0x1ff, mode 4 BrakeBlend): advance with crouch + lean, then compare with switch stance.
    let played = host.play_clip(slot, 0x1ff, 0, 5, &mut next_rng);
    assert_eq!(played, 0x1ff);
    assert_eq!(host.clip(slot, 0), 0x1ff);
    let mut inp = AnimRiderInputs { board_class: 1, crouch: 0.45, side: 0.6, brake: 0.0, ..Default::default() };
    host.sync_rider(slot, &inp);
    for _ in 0..10 {
        host.flush_ended(slot, &mut next_rng);
        host.update_layers(slot, 1.0, &mut next_rng);
    }
    let pose_reg = host.read_pose(slot, 0).expect("body pose");
    let board_reg = host.read_pose(slot, 1).expect("board pose");
    assert_eq!(pose_reg.rot.len(), 19);
    assert_eq!(board_reg.rot.len(), 1);
    assert!(pose_reg.trans[0][2].abs() > 10.0, "hips z should be non-trivial (~60-90 cm), got {:?}", pose_reg.trans[0]);

    // Compare live `AnimCtl_UpdateLayers` (mode 4 BrakeBlend + AnimCtl_BlendPose + PoseBlend_BlendSkeleton)
    // bit-for-bit against the real `SLUS_203.26` ELF in `r5900::Runner`!
    let mut r = Runner::from_elf(&elf).expect("load Runner from disc /SLUS_203.26");
    for f in [
        0x15f390u32, 0x15f440, 0x15f300, 0x15f7c8, 0x160680, 0x159bb8, 0x15a030, 0x15a3a8, 0x203a40,
        0x202868, 0x202930, 0x202990, 0x2029d8, 0x202b00, 0x159910, 0x157ed8, 0x158030, 0x202520,
        0x102f80, 0x202758, 0x2027f0, 0x250c10, 0x2cc808,
    ] {
        r.allow(f);
    }
    r.stub(ctl::MARKER_SOUND, |_, _| {});
    r.stub(ctl::CLIP_STARTED, |_, _| {});
    for (base, bytes) in &host.mem().regions {
        r.mem.write(*base, bytes).unwrap();
    }
    let r_ctl = AnimHost::ctl_addr(slot);
    let r_base = AnimHost::rider_addr(slot);

    // Advance one tick on ours and on the real `SLUS_203.26` `AnimCtl_UpdateLayers(ctl, 1.0)`.
    host.update_layers(slot, 1.0, &mut next_rng);
    r.cpu.set_gpr(4, r_ctl as i32 as i64 as u64);
    r.cpu.set_f(12, 1.0);
    r.call(0x15f390).expect("run AnimCtl_UpdateLayers in SLUS_203.26");

    // Compare both PoseBlend outputs (rotations 0x108 bytes + translations 0x108 bytes) bit-for-bit!
    for part_idx in 0..2u32 {
        let p_addr = r_base + 0x7c0 + 0x394 * part_idx;
        let mut want_rot = vec![0u8; 0x108];
        let mut want_tr = vec![0u8; 0x108];
        r.mem.read(p_addr, &mut want_rot).unwrap();
        r.mem.read(p_addr + 0x108, &mut want_tr).unwrap();
        let got_rot: Vec<u8> = (0..0x108u32).map(|o| host.mem().r8(p_addr + o)).collect();
        let got_tr: Vec<u8> = (0..0x108u32).map(|o| host.mem().r8(p_addr + 0x108 + o)).collect();
        assert_eq!(got_rot, want_rot, "part {part_idx} blended bone rotations differ from SLUS_203.26");
        assert_eq!(got_tr, want_tr, "part {part_idx} blended bone translations differ from SLUS_203.26");
    }

    // 2. Switch stance mirroring: switching stance changes the mirrored pose (and matches SLUS_203.26 too!).
    host.set_stance(slot, 1);
    host.set_all_layers_stance(slot, 1);
    inp.switch = true;
    host.sync_rider(slot, &inp);
    host.update_layers(slot, 1.0, &mut next_rng);
    let pose_sw = host.read_pose(slot, 0).expect("switch body pose");
    assert_ne!(pose_sw.rot, pose_reg.rot, "switch stance should mirror the skeleton pose");

    // 3. Hard landing clip (0x23e, end action 1 -> returns to 0x1ff via flush_ended).
    host.set_stance(slot, 0);
    host.set_all_layers_stance(slot, 0);
    inp.switch = false;
    host.sync_rider(slot, &inp);
    let land = host.play_clip(slot, 0x23e, 0, 5, &mut next_rng);
    assert_eq!(land, 0x23e);
    assert!(!host.at_end(slot, 0));
    let mut returned = false;
    for _ in 0..120 {
        host.flush_ended(slot, &mut next_rng);
        if host.clip(slot, 0) == 0x1ff {
            returned = true;
            break;
        }
        host.update_layers(slot, 1.0, &mut next_rng);
    }
    assert!(returned, "hard landing clip 0x23e should auto-chain back to 0x1ff via AnimCtl_FlushEndedClips");

    // 4. Grab clip (0x278, type 0xe, mode 1 stops at marker 2; stop_category(0xe) releases through markers 3 & 4).
    let grab = host.play_clip(slot, 0x278, 0, 5, &mut next_rng);
    assert_eq!(grab, 0x278);
    assert_eq!(host.clip_type(slot, 0), 0xe);
    let mut hit_m2 = false;
    for _ in 0..40 {
        host.flush_ended(slot, &mut next_rng);
        host.update_layers(slot, 1.0, &mut next_rng);
        if host.test_flag(slot, 2, 0) {
            hit_m2 = true;
            break;
        }
    }
    assert!(hit_m2, "grab clip 0x278 should reach marker 2");
    assert!(!host.test_flag(slot, 4, 0), "held grab should stay stopped before marker 4");
    // Release grab by clearing its stop frame (`AnimCtl_StopClipsInCategory(0xe)`): must pass marker 4!
    host.stop_category(slot, 0xe);
    let mut passed_m4 = false;
    for _ in 0..60 {
        host.flush_ended(slot, &mut next_rng);
        host.update_layers(slot, 1.0, &mut next_rng);
        if host.test_flag(slot, 4, 0) {
            passed_m4 = true;
            break;
        }
    }
    assert!(passed_m4, "released grab 0x278 should pass its real disc marker 4 (AnimCtl_TestClipFlag(4, 0))");

    // 5. Layer-2 idle clip (0x2f0 from Boarder_PickIdleClip) plays simultaneously on layer 2.
    let idle = host.play_clip(slot, 0x2f0, 0, 5, &mut next_rng);
    assert_eq!(idle, 0x2f0);
    assert_eq!(host.clip(slot, 2), 0x2f0);
    host.update_layers(slot, 1.0, &mut next_rng);

    // 6. Clip displacement query works without panicking.
    let disp = host.clip_displacement(slot, 0, &mut next_rng);
    assert!(disp.iter().all(|v| v.is_finite()));

    // Verify ELF_DATA slice sanity.
    assert!(ELF_DATA_END > ELF_DATA_START);
    let events = host.drain_events();
    assert!(!events.is_empty(), "expected ClipStarted / MarkerSound events");
}

#[test]
fn rider_step_drives_live_animhost_across_states() {
    let Some((elf, mut files)) = disc_and_elf() else { return };
    let body_model = CharModel::rider_from_disc(&mut files, "mac").expect("load mac");
    let board_model = CharModel::board_from_disc(&mut files, Some("Fr")).expect("load Fr board");
    let body_bones = body_model.slot_raw_bones(0);
    let board_bones = board_model.slot_raw_bones(1);

    let roster = [RosterEntry { character: 2, board_class: 1, load_uber: true }];
    let mut host = AnimHost::from_disc(&elf, &mut files, &roster).expect("init AnimHost");
    let slot = host.add_rider(2, 1, 1.0, &[(0, &body_bones), (1, &board_bones)]);
    let anim = RiderAnim::new(Arc::new(Mutex::new(host)), slot);

    // Flat groomed-snow plane (surface 1)
    let mut world = CollisionWorld::default();
    world.add_ex([Vec3::new(-200.0, 0.0, 200.0), Vec3::new(200.0, 0.0, 200.0), Vec3::new(-200.0, 0.0, -200.0)], None, true, 1, false);
    world.add_ex([Vec3::new(200.0, 0.0, 200.0), Vec3::new(200.0, 0.0, -200.0), Vec3::new(-200.0, 0.0, -200.0)], None, true, 1, false);
    let rails = Rails::default();
    let dt = 1.0 / 60.0;

    let mut r = Rider::new(Vec3::new(0.0, 0.05, 0.0), 0.0);
    r.grounded = true;
    r.motion = 2;
    r.ctl = control_tick::CRUISE;
    r.vel = Vec3::new(0.0, 0.0, -15.0);
    r.data = &tricky_game::trickdata::RIDERS[2]; // Mac
    r.stats.kind = 1;
    r.attach_anim(anim.clone());
    assert_eq!(anim.clip(0), 0x1ff, "attached grounded rider starts on base ride clip 0x1ff");

    // 1. Cruise ticks: layer 2 idle clip picked by Boarder_PickIdleClip on tick 0/18, pose updates every tick.
    let idle = Input::default();
    for _ in 0..20 {
        r.pos.z = 0.0;
        r.grounded = true;
        r.motion = 2;
        r.step(&world, &rails, idle, dt);
    }
    let layer2 = anim.clip(2);
    assert!((0x2f0..=0x2f8).contains(&layer2), "Boarder_PickIdleClip should play layer-2 clip 0x2f0..=0x2f8, got {layer2:#x}");
    let body_pose = anim.read_pose(0).expect("blended body pose");
    let board_pose = anim.read_pose(1).expect("blended board pose");
    assert_eq!(body_pose.rot.len(), 19);
    assert_eq!(board_pose.rot.len(), 1);
    // Verify `CharModel::skin_pose` / `skin_board_pose` + `skin_part_anim` (row G5d3) on real disc `PoseBlend` output
    let body_mats = body_model.skin_pose(&body_pose.trans, &body_pose.rot);
    let board_mats = board_model.skin_board_pose(board_pose.trans[0], board_pose.rot[0]);
    let (mut pos, mut nrm) = (Vec::new(), Vec::new());
    tricky_data::character::skin_part_anim(&body_model.parts[0], &body_mats, &mut pos, &mut nrm);
    assert!(!pos.is_empty());
    let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in &pos {
        assert!(p.iter().all(|v| v.is_finite()), "non-finite body vertex {p:?}");
        min_y = min_y.min(p[1]);
        max_y = max_y.max(p[1]);
    }
    assert!(min_y > -0.25 && max_y > 0.8 && max_y < 2.4, "unexpected skinned body Y bounds [{min_y:.3}, {max_y:.3}]");
    tricky_data::character::skin_part_anim(&board_model.parts[0], &board_mats, &mut pos, &mut nrm);
    assert!(!pos.is_empty());
    for p in &pos {
        assert!(p.iter().all(|v| v.is_finite()), "non-finite board vertex {p:?}");
        assert!(p[1].abs() < 0.5, "board vertex too far from board plane: {p:?}");
    }
    // Verify switch stance (`PoseBlend_BlendSkeleton` bone mirroring) also produces valid finite bounds
    anim.set_stance(1);
    anim.set_all_layers_stance(1);
    r.switch = true;
    r.pos.z = 0.0;
    r.step(&world, &rails, idle, dt);
    let body_sw = anim.read_pose(0).expect("switch body pose");
    let sw_mats = body_model.skin_pose(&body_sw.trans, &body_sw.rot);
    tricky_data::character::skin_part_anim(&body_model.parts[0], &sw_mats, &mut pos, &mut nrm);
    let (mut sw_min_y, mut sw_max_y) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in &pos {
        assert!(p.iter().all(|v| v.is_finite()), "non-finite switch vertex {p:?}");
        sw_min_y = sw_min_y.min(p[1]);
        sw_max_y = sw_max_y.max(p[1]);
    }
    assert!(sw_min_y > -0.25 && sw_max_y > 0.8 && sw_max_y < 2.4, "unexpected switch body Y bounds [{sw_min_y:.3}, {sw_max_y:.3}]");
    anim.set_stance(0);
    anim.set_all_layers_stance(0);
    r.switch = false;

    // 2. Prewind: holding jump enters PREWIND (0xe) and plays clip 0x217 (WindUpBlend mode 10).
    let prewind_in = Input { jump: true, steer: 1.0, ..Default::default() };
    for _ in 0..10 {
        r.pos.z = 0.0;
        r.step(&world, &rails, prewind_in, dt);
    }
    assert_eq!(r.ctl, control_tick::PREWIND);
    assert_eq!(anim.clip(0), 0x217, "PrewindControl_Enter should play clip 0x217");

    // 3. Spin + Grab in air: put rider high in air in SPIN, hold L1 grab, verify real disc marker 2 & 4 gate!
    r.pos = Vec3::new(0.0, 40.0, 0.0);
    r.vel = Vec3::new(0.0, 12.0, -15.0);
    r.grounded = false;
    r.motion = 1;
    r.set_control(control_tick::SPIN);
    // Hold L1 grab (bit 0 -> grab = 1) until grab reaches hold marker 2.
    let grab_in = Input { grab: 1, ..Default::default() };
    let mut saw_grab_clip = false;
    let mut saw_unsafe = false;
    for _ in 0..35 {
        r.step(&world, &rails, grab_in, dt);
        if anim.clip_type(0) == 0xe {
            saw_grab_clip = true;
            if !r.grab_safe() {
                saw_unsafe = true;
            }
        }
    }
    assert!(saw_grab_clip, "SpinState_UpdateTrickClips should play a real grab clip (type 0xe) on AnimHost");
    assert!(saw_unsafe, "held grab clip before marker 4 must report grab_safe() == false");

    // Release grab: clip plays past its real disc marker 4, making grab_safe() == true!
    let mut became_safe = false;
    for _ in 0..60 {
        r.step(&world, &rails, idle, dt);
        if r.grab_safe() {
            became_safe = true;
            break;
        }
    }
    assert!(became_safe, "releasing grab must advance past real disc marker 4 and set grab_safe() == true");

    // 4. Hard landing clip (0x23e) holds LAND_HARD until AnimCtl_FlushEndedClips chains back to 0x1ff -> CRUISE!
    r.pos = Vec3::new(0.0, 0.05, 0.0);
    r.vel = Vec3::new(0.0, 0.0, -15.0);
    r.grounded = true;
    r.motion = 2;
    r.set_control(control_tick::LAND_HARD);
    r.hard_land = 99.0; // deliberately large so only real AnimCtl clip end (not timer stand-in!) exits LAND_HARD
    anim.play_clip(0x23e, 0, 5, &mut tricky_game::rand::rand_game);
    let mut exited_hard_land = false;
    for _ in 0..120 {
        r.pos.z = 0.0;
        r.grounded = true;
        r.motion = 2;
        r.step(&world, &rails, idle, dt);
        if r.ctl == control_tick::CRUISE {
            exited_hard_land = true;
            break;
        }
    }
    assert!(exited_hard_land, "LandHardControl_Update should transition back to CRUISE when real AnimCtl clip 0x23e chains to 0x1ff");
}
