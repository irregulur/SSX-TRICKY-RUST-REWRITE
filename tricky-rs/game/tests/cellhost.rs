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
