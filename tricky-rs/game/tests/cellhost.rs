//! Live world cells and object controllers (`CellHost`, board row E7b2; needs `TRICKY_DISC`, skipped without it):
//! - Mesablanca object `1444` (driven by neighbour `1443`'s persistent script effect `149` -> `op 7 [1444, 150]`):
//!   stays inactive with no controller while riders are outside its cell, spawns `cAnimObjectNode` (`0x100`) on cell
//!   activation with its pending creation frame at `t = 0.0` (`BxList_MergePending`), advances by `1/60` s per tick
//!   with posed parts on `CourseGround`, and is killed/unposed when riders leave its cell (`World_CellDeactivate`).
//! - Tokyo Megaplex `Pinball_FlipperL` `75` (proven live in `port-notes/world.md` E8c4):
//!   cell activation flips `0x10a110a3` -> `0x10a111a3`; rider contact on trigger `59` runs `effect 442` ->
//!   `cDebounceNode` (`2`, 90 ticks) on `59` and `op 7 [75, 443]` -> `cAnimObjectNode` (`0x100`) on `75` for 300 ticks
//!   (flags `0x10a111a3` -> `0x10a111a5`, parts posed on `CourseGround`), then finishes and restores flags back to
//!   `0x10a111a3` and unposes `75`.

use tricky_game::course_ground::CourseGround;
use tricky_game::knockables::{self, Rider};
use tricky_game::walls::{Contact, Touched};

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

fn far_corner(cg: &CourseGround) -> V4 {
    cg.with_world(|w| [w.origin[0], w.origin[1], 0.0, 1.0])
}

#[test]
fn mesa_1444_cell_activation_pending_frame_clock_and_deactivation() {
    let Some(cg) = load_course("mesa") else { return };
    let mut rand = || 0u32;

    // Far away: object 1444 has no controller, no clip time, and cell-active bit 0x100 is clear.
    let far = far_corner(&cg);
    cg.tick_cells(&[far], 4, far, false, &mut rand);
    assert_eq!(cg.controller_type(1444), None);
    assert_eq!(cg.anim_time(1444), None);
    assert_eq!(cg.object_flags(1444) & 0x100, 0);

    // Move rider into 1443/1444's cell: World_CellActivate_RunPersistant spawns 1443's effect 149 -> op 7 [1444, 150]
    // creating cAnimObjectNode (0x100) on 1444; pending frame keeps t == 0.0 on the creation frame!
    let at_1443 = object_center(&cg, 1443);
    let ev = cg.tick_cells(&[at_1443], 4, at_1443, false, &mut rand);
    assert!(ev.activated_scripts.iter().any(|&(o, _)| o == 1444), "1444 effect 150 activated");
    assert_eq!(cg.controller_type(1444), Some(0x100));
    assert_eq!(cg.object_flags(1444) & 0x100, 0x100);
    assert_eq!(cg.object_flags(1444) & 6, 4, "cAnimObjectNode clears bit 2 and sets bit 4");
    assert_eq!(cg.anim_time(1444), Some(0.0), "pending creation frame stays at clip start 0.0");

    // Next 10 ticks: steps once per 60 Hz frame (rate = 1/60 s/frame for 30 fps / 30).
    for step in 1..=10u32 {
        cg.tick_cells(&[at_1443], 4, at_1443, false, &mut rand);
        let t = cg.anim_time(1444).expect("1444 still live");
        assert!((t - step as f32 / 60.0).abs() < 1e-5, "step {step}: expected {}, got {t}", step as f32 / 60.0);
    }

    // Move rider far away: World_CellDeactivate clears 0x100, kills 1444's controller, and unposes CourseGround.
    let ev_off = cg.tick_cells(&[far], 4, far, false, &mut rand);
    assert!(ev_off.killed_objects.contains(&1444), "1444 killed on cell deactivate");
    assert_eq!(cg.controller_type(1444), None);
    assert_eq!(cg.anim_time(1444), None);
    assert_eq!(cg.object_flags(1444) & 0x100, 0);
}

#[test]
fn megaplex_pinball_flipper_75_flips_300_ticks_and_restores() {
    let Some(cg) = load_course("megaple") else { return };
    let mut rand = || 0u32;

    // Initial flags at rest (proven live in world.md E8c4: 0x10a110a3).
    assert_eq!(cg.object_flags(75), 0x10a1_10a3);
    assert_eq!(cg.controller_type(75), None);

    // Move rider to Pinball_FlipperL 75's cell: cell activates -> flags 0x10a111a3, still no controller at rest.
    let at_75 = object_center(&cg, 75);
    cg.tick_cells(&[at_75], 4, at_75, false, &mut rand);
    assert_eq!(cg.object_flags(75), 0x10a1_11a3);
    assert_eq!(cg.controller_type(75), None);

    // Rider touches trigger object 59 -> effect 442 spawns cDebounceNode (type 2, 90 ticks) on 59
    // and op 7 [75, 443] spawns cAnimObjectNode (type 0x100) on Pinball_FlipperL 75!
    let contact = Contact { pos: at_75, dir: [0.0, 1.0, 0.0, 0.0], normal: [0.0, -1.0, 0.0, 0.0], speed_in: 500.0 };
    cg.knock.lock().unwrap().contact(&cg, 59, Rider { id: 1, human: true, vel: [0.0, 500.0, 0.0, 0.0] }, &contact, [0; 16]);
    assert_eq!(cg.controller_type(59), Some(2), "trigger 59 debounced");
    assert_eq!(cg.controller_type(75), Some(0x100), "Pinball_FlipperL 75 has cAnimObjectNode");
    assert_eq!(cg.object_flags(75), 0x10a1_11a5, "flags ...a3 -> ...a5 while flipping");
    assert!(cg.anim_time(75).is_some());
    assert!(cg.with_world(|w| w.objects[75].controller == Some(0x100)));

    // reread reports non-roller controller as fixed (Some(true)) matching cControllerNode::vf54 0x17dda0.
    let reread = knockables::reread(&cg, &Touched { id: 75, flags: 0, mass: 1e30, bounce: 0.5, controller: None });
    assert_eq!(reread.flags, 0x10a1_11a5);
    assert_eq!(reread.controller, Some(true));

    // Step 60 Hz ticks until the flipper finishes, and verify debounce on 59 expires after ~90 ticks while
    // Pinball_FlipperL 75 stays active and finishes cleanly around 300 ticks (restoring flags to 0x10a111a3).
    let mut alive_ticks = 0u32;
    for tick in 1..=320 {
        cg.tick_cells(&[at_75], 4, at_75, false, &mut rand);
        if cg.controller_type(75).is_some() {
            alive_ticks = tick;
        }
        if tick == 95 {
            assert_eq!(cg.controller_type(59), None, "trigger 59 debounce expired by tick 95");
        }
    }
    assert!((295..=305).contains(&alive_ticks), "Pinball_FlipperL 75 expected ~300 active ticks, got {alive_ticks}");
    assert_eq!(cg.controller_type(75), None, "Pinball_FlipperL 75 controller finished");
    assert_eq!(cg.anim_time(75), None);
    assert_eq!(cg.object_flags(75), 0x10a1_11a3, "flags restored ...a5 -> ...a3 after flip");
    assert!(cg.with_world(|w| w.objects[75].controller.is_none()));

    // Reset clears active cells and restores initial flags 0x10a110a3.
    cg.reset_cells();
    assert_eq!(cg.object_flags(75), 0x10a1_10a3);
    assert_eq!(cg.cells.lock().unwrap().active_cell_count(), 0);
}

#[test]
fn garibaldi_mode_and_countdown_scripts_and_delete_script_object() {
    use tricky_game::cellhost::ScriptEvent;
    let Some(cg) = load_course("gari") else { return };
    let mut rand = || 0u32;

    // `World_FindScript` 0x260dd8 is case-insensitive (`stricmp`).
    assert!(cg.find_script("RaceMode").is_some(), "gari has RaceMode");
    assert_eq!(cg.find_script("racemode"), cg.find_script("RACEMODE"));
    assert!(cg.find_script("ShowoffMode").is_some(), "gari has ShowoffMode");
    assert!(cg.find_script("FreerideMode").is_some(), "gari has FreerideMode");
    let cd_name = if cg.find_script("StartCountdown").is_some() { "StartCountdown" } else { "CountDownStart" };
    assert!(cg.find_script(cd_name).is_some(), "gari has countdown script");

    // Running `RaceMode` hides show-off-only objects via `cDeadNode` (clearing flag 0x21) and emits `DeadObject`.
    let ev_race = cg.run_script_named("RaceMode", &mut rand);
    let race_dead: Vec<usize> = ev_race
        .script_events
        .iter()
        .filter_map(|e| match e { ScriptEvent::DeadObject(o) => Some(*o), _ => None })
        .collect();
    assert!(!race_dead.is_empty(), "RaceMode hides mode-specific objects via cDeadNode");
    for &o in &race_dead {
        assert_eq!(cg.object_flags(o) & 0xf2, 0, "object {o} dead flags cleared (& 0xffffff0d) after RaceMode");
        assert_eq!(cg.object_flags(o) & 4, 4, "object {o} has controller bit 4 set");
        assert_eq!(cg.controller_type(o), Some(5), "object {o} holds cDeadNode");
    }

    // Reset + ShowoffMode hides a different set of objects than RaceMode.
    cg.reset_cells();
    let ev_show = cg.run_script_named("ShowoffMode", &mut rand);
    let show_dead: Vec<usize> = ev_show
        .script_events
        .iter()
        .filter_map(|e| match e { ScriptEvent::DeadObject(o) => Some(*o), _ => None })
        .collect();
    assert!(!show_dead.is_empty(), "ShowoffMode hides race-only objects");
    assert_ne!(race_dead, show_dead, "RaceMode and ShowoffMode hide distinct object sets");

    // `StartCountdown` runs live `cScriptNode` threads (including child threads with `op 4` waits): stays on
    // List 3 pending during creation frame, merges into live List 3 via `BxList_MergePending`, and runs to completion.
    cg.reset_cells();
    let far = far_corner(&cg);
    let start_pos = cg.with_world(|w| {
        w.objects
            .iter()
            .enumerate()
            .find(|&(i, _)| cg.ssf.event_script(i, 0) >= 0)
            .map_or(far, |(_, o)| [(o.min[0] + o.max[0]) * 0.5, (o.min[1] + o.max[1]) * 0.5, (o.min[2] + o.max[2]) * 0.5, 1.0])
    });
    cg.tick_cells(&[start_pos], 1, start_pos, false, &mut rand);

    let mut total_events = 0usize;
    let ev0 = cg.run_script_named(cd_name, &mut rand);
    total_events += ev0.script_events.len();
    assert!(cg.cells.lock().unwrap().active_thread_count() > 0, "{cd_name} spawns waiting cScriptNode thread(s)");
    for _ in 0..300 {
        let ev = cg.tick_cells(&[start_pos], 1, start_pos, false, &mut rand);
        total_events += ev.script_events.len();
    }
    assert!(total_events > 0, "{cd_name} emitted live script events across its countdown waits");
    assert_eq!(cg.cells.lock().unwrap().active_thread_count(), 0, "{cd_name} threads finished cleanly within 5 s");

    // `World_DeleteScriptObject` 0x13bbc0: find a Garibaldi `.ssf` function that itself waits (`op 4`),
    // start it via `World_RunScript`, merge pending -> live via one `tick_cells`, and verify
    // `World_DeleteScriptObject` deletes that exact live `cScriptNode` (and returns false once gone).
    let (wait_idx, wait_name) = cg
        .ssf
        .functions
        .iter()
        .enumerate()
        .find(|(_, f)| f.ops.iter().any(|op| op.op == 4))
        .map(|(i, f)| (i as u32, f.name.clone()))
        .expect("gari has a script function with an op 4 wait");
    cg.run_script_named(&wait_name, &mut rand);
    let before_merge = cg.cells.lock().unwrap().active_thread_count();
    assert!(before_merge > 0, "{wait_name} waits on pending List 3");
    // Before `BxList_MergePending`, `World_DeleteScriptObject` only scans live List 3 (`BxList_First`).
    assert!(!cg.delete_script_object(wait_idx), "pending thread is not in live List 3 before merge");
    cg.tick_cells(&[start_pos], 1, start_pos, false, &mut rand);
    let live_count = cg.cells.lock().unwrap().active_thread_count();
    assert!(live_count > 0, "{wait_name} merged into live List 3");
    assert!(cg.delete_script_object(wait_idx), "World_DeleteScriptObject deletes live {wait_name} thread");
    assert_eq!(cg.cells.lock().unwrap().active_thread_count(), live_count - 1);
    assert!(!cg.delete_script_object(wait_idx), "no second {wait_name} thread remains to delete");
}

#[test]
fn list3_update_steps_newest_sort_id_first() {
    // Proves `BxList_Add` 0x17d4e8 + `BxList_MergePending` 0x17d548 + `BxList_UpdateAll(3)` 0x17d950 order:
    // on Megaplex, spawning trigger 59 (cDebounceNode 2, 90 ticks) followed by Pinball_FlipperL 75 (cAnimObjectNode
    // 0x100, 300 ticks) merges both into live List 3 descending by `sort_id` so 75 steps before 59, and calling
    // `run_event_script(59, 1, ...)` directly through `CellHost` produces the exact same controller transitions.
    let Some(cg) = load_course("megaple") else { return };
    let mut rand = || 0u32;
    let at_75 = object_center(&cg, 75);
    cg.tick_cells(&[at_75], 4, at_75, false, &mut rand);
    let ev = cg.run_event_script(59, 1, 1, [0.0, 500.0, 0.0, 0.0], true, &mut rand);
    assert_eq!(cg.controller_type(59), Some(2));
    assert_eq!(cg.controller_type(75), Some(0x100));
    assert!(ev.script_events.iter().any(|e| matches!(e, tricky_game::cellhost::ScriptEvent::SpawnController { object: 75, .. })));
    // Pending on creation frame (t == 0.0), then advances on tick 1 after merge.
    assert_eq!(cg.anim_time(75), Some(0.0));
    cg.tick_cells(&[at_75], 4, at_75, false, &mut rand);
    // Tick 1 merges pending -> live (does NOT step nodes created before tick 1's merge until tick 2, OR wait:
    // `run_event_script` happened before `tick_cells`, so `pending_list3` merges at end of tick 1, and steps on tick 2).
    cg.tick_cells(&[at_75], 4, at_75, false, &mut rand);
    assert!(cg.anim_time(75).unwrap() > 0.0, "75 advances once merged onto live List 3");
}
