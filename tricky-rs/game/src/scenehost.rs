//! Host driving the ported pre-race and post-race scene state machines (board row G11e):
//! - `RaceScene` (`0x32edc0`: `RaceScene_Reset` `0x169688`, `RaceScene_SetupFromPicks` `0x169698`,
//!   `RaceScene_Select` `0x1699a0`)
//! - `PreRace` (`cPreRaceHandler`: `Enter` `0x112b88`, `Update` `0x112d10`, `Exit` `0x113468`)
//! - `EndRace` (`cEndRaceHandler`: `Enter` `0x113e10`, `Update` `0x1141e0`)
//! - `CamScript_StartScene` (`0x176850`) + `CamAnimation_InitScoped` (`0x16d9d0`) / `CamAnimation` (`0x16d990`)
//!   + `RaceScene_PlayCastClip` (`0x169be8`) actor-to-rider assignment and clip resolution.

use crate::career::Career;
use crate::prerace::{self, EndRace, EndRaceEnv, PreRace, PreRaceEnv, RaceView, PRELOAD};
use crate::scene::{RaceScene, SceneEnv, SCENES};
use tricky_data::intro::{Cml, SceneActor};

/// Character scope names in `SLUS_203.26` (`PTR_s_eddie_00336650`, 12 pointers), passed to
/// `CamAnimation_InitScoped` (`0x16d9d0`) by `CamScript_StartScene` (`0x176850`).
pub const SCOPE_NAMES: [&str; 12] = [
    "eddie", "kaori", "luther", "mac", "moby", "zoe", "jp", "elise", "psymon", "seeiah", "brodi", "marisol",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    PreRace,
    EndRace,
    Done,
}

/// Events emitted by one `PreRace::update` or `EndRace::update` tick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepOut {
    /// Non-`"PreLoad"` camera script started by `CamScript_Start` (`RaceScene_Play` `0x110a40`).
    pub start_script: Option<String>,
    /// `CamScript_Stop` ran without starting a new non-`"PreLoad"` script.
    pub stop_script: bool,
    /// `PreRace` reached `race_start()` (`state == 0x15`) or `EndRace` reached `state >= 8` (results).
    pub done: bool,
    /// `PreRace` placed riders at the start (`Boarder_PlaceAtStart`).
    pub place_at_start: bool,
}

/// Live race-scene controller bridging `Career` (`+0x2b18` selection from `Career_FinalizeRoster` `0x168b88`),
/// `RaceScene` (`0x32edc0`), `PreRace` (`cPreRaceHandler`), and `EndRace` (`cEndRaceHandler`).
#[derive(Clone, Debug, Default)]
pub struct SceneHost {
    pub race_scene: RaceScene,
    pub pre: PreRace,
    pub end: EndRace,
    pub phase: Phase,
    pub end_clock: f32,
    pub active_script: Option<String>,
}

struct HostEnv<'a> {
    cam_running: bool,
    skip_pressed: bool,
    skip_latched: u32,
    start_script: Option<String>,
    stop_script: bool,
    race_started: bool,
    place_at_start: bool,
    marker_fn: &'a mut dyn FnMut(i32) -> i32,
    // EndRace query data (indexed by career roster order 0..6)
    rider_counts: [[i32; 6]; 6],
    rider_places: [i32; 6],
    rider_times: [i32; 6],
    rider_scores: [i32; 6],
}

impl<'a> HostEnv<'a> {
    fn new(cam_running: bool, skip_pressed: bool, marker_fn: &'a mut dyn FnMut(i32) -> i32) -> Self {
        Self {
            cam_running,
            skip_pressed,
            skip_latched: 0,
            start_script: None,
            stop_script: false,
            race_started: false,
            place_at_start: false,
            marker_fn,
            rider_counts: [[0; 6]; 6],
            rider_places: [0; 6],
            rider_times: [0; 6],
            rider_scores: [0; 6],
        }
    }
}

impl SceneEnv for HostEnv<'_> {
    fn race_rider0(&mut self) -> u32 {
        1
    }

    fn first_marker_frame(&mut self, _rider: u32, clip: i32) -> i32 {
        (self.marker_fn)(clip)
    }
}

impl PreRaceEnv for HostEnv<'_> {
    fn update_streaming(&mut self) {}
    fn loading(&mut self) -> bool {
        false
    }
    fn cam_stop(&mut self) {
        self.cam_running = false;
        self.stop_script = true;
    }
    fn cam_start(&mut self, script: &str) {
        if script.is_empty() || script.eq_ignore_ascii_case(PRELOAD) {
            self.cam_running = false;
        } else {
            self.start_script = Some(script.to_string());
            self.stop_script = false;
            self.cam_running = true;
        }
    }
    fn cam_running(&mut self) -> bool {
        self.cam_running
    }
    fn speech_mode(&mut self, _a: u32, _b: u32) {}
    fn init_char_samples(&mut self) {}
    fn cast_sample(&mut self, _v: i32) {}
    fn stop_char_script(&mut self) {}
    fn free_char_samples(&mut self) {}
    fn fade_in(&mut self) {}
    fn fade_out(&mut self) {}
    fn rider_reset(&mut self, _i: usize) {}
    fn place_at_start(&mut self, _i: usize, _gate: bool) {
        self.place_at_start = true;
    }
    fn hud_show(&mut self, _item: u32) {}
    fn hud_hide(&mut self, _item: u32) {}
    fn stop_intro_stream(&mut self) {}
    fn race_start(&mut self) {
        self.race_started = true;
    }
    fn race_entered(&mut self) {}
    fn player_of(&mut self, _k: usize) -> i8 {
        0
    }
    fn overlays_hidden(&mut self) {}
    fn single_view(&mut self) {}
    fn restore_views(&mut self) {}
    fn run_world_script(&mut self, _name: &str) {}
    fn device_count(&mut self) -> i32 {
        1
    }
    fn device_pressed(&mut self, i: i32) -> bool {
        i == 0 && self.skip_pressed
    }
    fn take_device(&mut self, _i: i32) -> (u32, u32) {
        let held = if self.skip_pressed { 0x880 } else { 0 };
        (self.skip_latched, held)
    }
    fn latch_device(&mut self, _i: i32, latched: u32) {
        self.skip_latched = latched;
    }
    fn app_mode(&mut self) -> u32 {
        0
    }
}

impl EndRaceEnv for HostEnv<'_> {
    fn clear_ai_targets(&mut self) {}
    fn move_leader_to_slot0(&mut self) {}
    fn check_new_records(&mut self, _c: &mut Career) {}
    fn freeze(&mut self, _i: usize) {}
    fn rider_count(&mut self, i: usize, j: usize) -> i32 {
        self.rider_counts.get(i).and_then(|r| r.get(j)).copied().unwrap_or(0)
    }
    fn rider_place(&mut self, i: usize) -> i32 {
        self.rider_places.get(i).copied().unwrap_or(0)
    }
    fn rider_time(&mut self, i: usize) -> i32 {
        self.rider_times.get(i).copied().unwrap_or(0)
    }
    fn rider_score(&mut self, i: usize) -> i32 {
        self.rider_scores.get(i).copied().unwrap_or(0)
    }
    fn results_shown(&mut self) -> bool {
        false
    }
    fn results_busy(&mut self) -> bool {
        false
    }
    fn open_results_menu(&mut self) {}
    fn replay_rider_sounds(&mut self) {}
    fn restart_race(&mut self) {}
}

impl SceneHost {
    /// `RaceScene_Reset` `0x169688` + `RaceScene_SetupFromPicks` `0x169698` on the career's `+0x2b18` selection
    /// written by `Career_FinalizeRoster` (`0x168b88`).
    pub fn setup_from_career(&mut self, career: &Career, rand_fx: &mut dyn FnMut() -> u32) {
        self.race_scene.reset();
        let sel = prerace::selection(career);
        self.race_scene.setup_from_picks(&sel, rand_fx);
        self.pre = PreRace::default();
        self.end = EndRace::default();
        self.phase = Phase::Idle;
        self.end_clock = 0.0;
        self.active_script = None;
    }

    /// `RaceScene_GetAnimList` `0x169948`: the `.afl` archive indices (`FILES` `0x3258b0`) needed for all 15 scenes.
    pub fn anim_file_ids(&self) -> &[i32] {
        &self.race_scene.riders[..self.race_scene.rider_count()]
    }

    /// `cPreRaceHandler::Enter` `0x112b88`.
    pub fn enter_pre_race(&mut self, career: &mut Career, riders: i32, restarted: bool) {
        let mut no_marker = |_: i32| -1;
        let mut env = HostEnv::new(false, false, &mut no_marker);
        let rv = RaceView { riders, players: 1, one_pad: false, restarted: restarted as u32, replayed: 0 };
        self.pre.enter(&mut env, career, &rv);
        self.phase = Phase::PreRace;
        self.active_script = None;
    }

    /// One tick of `cPreRaceHandler::Update` `0x112d10` (and `Exit` `0x113468` when `race_start` fires).
    pub fn step_pre_race(
        &mut self,
        career: &Career,
        riders: i32,
        cam_running: bool,
        skip_pressed: bool,
        marker_fn: &mut dyn FnMut(i32) -> i32,
    ) -> StepOut {
        if self.phase != Phase::PreRace {
            return StepOut::default();
        }
        let rv = RaceView { riders, players: 1, one_pad: false, restarted: 0, replayed: 0 };
        let mut env = HostEnv::new(cam_running, skip_pressed, marker_fn);
        self.pre.update(&mut env, career, &rv, &mut self.race_scene);
        // STANDIN: auto-advance pre-race tip/HUD states 0x10/0x11 (`hud_show(0x12)` / `hud_show(3)`) because the
        // app does not show modal tip overlays before the 3-2-1 countdown.
        let mut guard = 0;
        while matches!(self.pre.state, 0x10 | 0x11) && guard < 4 {
            env.skip_pressed = true;
            env.skip_latched = 0;
            self.pre.update(&mut env, career, &rv, &mut self.race_scene);
            guard += 1;
        }
        let done = env.race_started || self.pre.state >= 0x15;
        if done {
            self.pre.exit(&mut env);
            self.phase = Phase::Idle;
            self.active_script = None;
        } else if let Some(ref s) = env.start_script {
            self.active_script = Some(s.clone());
        } else if env.stop_script {
            self.active_script = None;
        }
        StepOut {
            start_script: env.start_script,
            stop_script: env.stop_script,
            done,
            place_at_start: env.place_at_start,
        }
    }

    /// `cEndRaceHandler::Enter` `0x113e10`: chooses rival scene `end.first` (`5..14` or `0x10`) and win/lose scene
    /// `end.second` (`3` or `4`) from rivalry counts, places, times, and scores (indexed in career roster order).
    pub fn enter_end_race(
        &mut self,
        career: &mut Career,
        rider_counts: [[i32; 6]; 6],
        rider_places: [i32; 6],
        rider_times: [i32; 6],
        rider_scores: [i32; 6],
    ) {
        let mut no_marker = |_: i32| -1;
        let mut env = HostEnv::new(false, false, &mut no_marker);
        env.rider_counts = rider_counts;
        env.rider_places = rider_places;
        env.rider_times = rider_times;
        env.rider_scores = rider_scores;
        self.end.enter(&mut env, career);
        self.end_clock = 0.0;
        self.phase = Phase::EndRace;
        self.active_script = None;
    }

    /// One tick of `cEndRaceHandler::Update` `0x1141e0`.
    pub fn step_end_race(
        &mut self,
        career: &Career,
        riders: i32,
        cam_running: bool,
        skip_pressed: bool,
        marker_fn: &mut dyn FnMut(i32) -> i32,
    ) -> StepOut {
        if self.phase != Phase::EndRace {
            return StepOut::default();
        }
        let rv = RaceView { riders, players: 1, one_pad: false, restarted: 0, replayed: 0 };
        let mut env = HostEnv::new(cam_running, skip_pressed, marker_fn);
        self.end.update(&mut env, career, &rv, &mut self.race_scene, &mut self.end_clock, &[]);
        let done = self.end.state >= 8;
        if done {
            self.phase = Phase::Done;
            self.active_script = None;
        } else if let Some(ref s) = env.start_script {
            self.active_script = Some(s.clone());
        } else if env.stop_script {
            self.active_script = None;
        }
        StepOut {
            start_script: env.start_script,
            stop_script: env.stop_script,
            done,
            place_at_start: false,
        }
    }

    /// `RaceScene_PlayCastClip` `0x169be8`: look up `clip_key` (`slot.a`) in `race_scene.slots[current]` and
    /// resolve `slot.b` via `AnimClip_ResolveAnmIndex` (`0x15ada8`) to `(clip_id, clip_name)`.
    pub fn resolve_cast_clip(&self, clip_key: i32, board_class: u32, character: u32) -> Option<(u32, String)> {
        let cur = usize::try_from(self.race_scene.current).ok()?;
        if cur >= SCENES {
            return None;
        }
        for s in self.race_scene.slots[cur] {
            if s.a == clip_key && s.b >= 0 {
                let cid = s.b as u32;
                let name = crate::animids::resolve_clip_name(cid, board_class, character)?;
                return Some((cid, name.to_string()));
            }
        }
        None
    }

    /// `CamScript_StartScene` `0x176850` + `CamAnimation_InitScoped` `0x16d9d0` / `CamAnimation` `0x16d990` +
    /// `RaceScene_StartCastClip` `0x16da10` + `RaceScene_PlayCastClip` `0x169be8`:
    /// assigns CML type-5 scene actors to riders (two-pass: explicit player 0 / character selectors `0xe..=0x19`
    /// first, then unassigned riders for `kind == 0xd`) and resolves each actor's `.afl` clip name from
    /// `RaceScene.slots[current]`.
    /// Returns `(clips_per_actor, actor_slot_per_rider)` matching `tricky_data::intro::SceneState`.
    pub fn assign_scene_actors(
        &self,
        actors: &[SceneActor],
        rider_chars: &[u32],
        rider_boards: &[u32],
        player_idx: usize,
        cml: &Cml,
    ) -> (Vec<String>, Vec<Option<usize>>) {
        let n = rider_chars.len();
        let mut claimed = vec![false; n];
        // Pass 1: mark riders claimed by explicit player (`0`) or character (`0xe..=0x19`) selectors.
        for a in actors {
            match a.kind {
                0 => {
                    if player_idx < n {
                        claimed[player_idx] = true;
                    }
                }
                0xe..=0x19 => {
                    let want_ch = a.kind - 0xe;
                    if let Some(i) = rider_chars.iter().position(|&c| c == want_ch) {
                        claimed[i] = true;
                    }
                }
                _ => {}
            }
        }
        let unassigned: Vec<usize> = (0..n).filter(|&i| !claimed[i]).collect();
        let mut next_free = 0usize;

        let mut clips = vec![String::new(); actors.len()];
        let mut slot_of = vec![None; n];
        // Pass 2: assign actors in order and resolve their clip from CML type 1 -> RaceScene slot.
        for (actor_idx, a) in actors.iter().enumerate() {
            let rider_idx = match a.kind {
                0 => (player_idx < n).then_some(player_idx),
                0xd => {
                    let r = unassigned.get(next_free).copied();
                    if r.is_some() {
                        next_free += 1;
                    }
                    r
                }
                0xe..=0x19 => {
                    let want_ch = a.kind - 0xe;
                    rider_chars.iter().position(|&c| c == want_ch)
                }
                _ => None,
            };
            let Some(ri) = rider_idx else { continue };
            slot_of[ri] = Some(actor_idx);
            let ch = rider_chars[ri];
            let bc = rider_boards.get(ri).copied().unwrap_or(0);
            let scope = SCOPE_NAMES.get(ch as usize).copied();
            let keys = cml.anim_keys(&a.anim, scope);
            if let Some(&key) = keys.first() {
                if let Some((_, name)) = self.resolve_cast_clip(key, bc, ch) {
                    clips[actor_idx] = name;
                }
            }
        }
        (clips, slot_of)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::{CareerFile, InitialRelations};
    use crate::rand::Streams;
    use crate::roster::{CircuitHeats, PlayerRiderEnv, PreRaceSel, RosterData, RosterPrepareEnv};

    #[test]
    fn disc_pre_race_and_end_race_drive_real_cml_scripts_and_clips() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let mut disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
        let elf = disc.read_path("/SLUS_203.26").expect("read elf").expect("SLUS_203.26");
        let data = RosterData::from_elf(&elf).expect("RosterData::from_elf");
        let mut files = tricky_data::big::Files::new(disc);
        files.mount_game_archives().expect("mount archives");
        let cml = Cml::from_disc(&mut files, 0).expect("Garibaldi CML");

        let (mut career, _) = CareerFile::open(None, 0, 1, None);
        for k in 0..4 {
            career.clear_player_slot(k);
        }
        career.assign_player_rider(0, 0, 3, &crate::cheats::Cheats::default()); // Mac
        career.w32(0x2af8, 0); // Garibaldi
        career.w32(0x2afc, 4); // Circuit Race
        career.w32(0x320c + 0x14, 0); // Heat 0
        career.w32(0x27dc, 6);

        let mut streams = Streams::default();
        streams.seed_fx(0x4242_1337);
        let mut sel = PreRaceSel::default();
        let mut heats = CircuitHeats::default();
        let mut prep = RosterPrepareEnv {
            data: &data,
            sel: &mut sel,
            heats: &mut heats,
            env: PlayerRiderEnv::default(),
            rel: &InitialRelations,
            rand_fx: || streams.fx(),
        };
        career.prepare_race(&mut prep);

        let mut host = SceneHost::default();
        host.setup_from_career(&career, &mut || streams.fx());
        assert!(host.race_scene.has_selection);
        assert!(host.anim_file_ids().len() > 0, "RaceScene_GetAnimList must list scene .afl files");

        let rows = career.race_roster();
        let p_gate = rows.iter().position(|r| r.is_human()).expect("human row");
        // App order: player at index 0, then non-human opponents in career row order
        let mut app_chars = vec![rows[p_gate].character];
        let mut app_boards = vec![rows[p_gate].board_kind];
        for r in &rows {
            if !r.is_human() {
                app_chars.push(r.character);
                app_boards.push(r.board_kind);
            }
        }

        // Drive PreRace through all intro scenes (RaceLoaded, STG_*, GAT_*)
        host.enter_pre_race(&mut career, 6, false);
        let mut pre_scripts = Vec::new();
        let mut resolved_scene_clips = 0usize;
        for _ in 0..20 {
            let step = host.step_pre_race(&career, 6, false, false, &mut |_| -1);
            if let Some(sc_name) = step.start_script {
                assert!(cml.has(&sc_name), "PreRace script {sc_name} must exist in Garibaldi CML");
                let mut d = tricky_data::intro::Director::new(&cml, &[sc_name.as_str()]);
                for _ in 0..600 {
                    d.step(&cml, 1.0 / 60.0, &mut || streams.fx());
                    if let Some(ref sname) = d.scene {
                        let actors = cml.scene_actors(sname);
                        if !actors.is_empty() {
                            let (clips, slot_of) = host.assign_scene_actors(&actors, &app_chars, &app_boards, 0, &cml);
                            let non_empty = clips.iter().filter(|c| !c.is_empty()).count();
                            assert!(non_empty > 0, "scene {sname} under script {sc_name} should resolve clips: {clips:?}");
                            assert!(slot_of.iter().any(|s| s.is_some()));
                            resolved_scene_clips += non_empty;
                            break;
                        }
                    }
                    if d.done {
                        break;
                    }
                }
                pre_scripts.push(sc_name);
            }
            if step.done {
                break;
            }
        }
        assert_eq!(host.phase, Phase::Idle);
        assert!(pre_scripts.len() >= 2, "Heat 0 race must run fly/stg/gat scripts, got {pre_scripts:?}");
        assert!(resolved_scene_clips > 0, "at least one pre-race scene must resolve real RaceScene clips");

        // Heat 0 (qualifying heat): EndRace plays rival confrontation (`FL_<RIV>_VS_USR`), while `FL_WIN_*` is
        // `Null` (0x39) in `0x37c1b8` because medal/win celebrations only run on the final heat (`u == 2`) or
        // single events (`u == 3`).
        let mut counts = [[0i32; 6]; 6];
        let rival_career_idx = (0..6).find(|&i| i != p_gate).unwrap();
        counts[rival_career_idx][p_gate] = 4; // > 2 -> rival scene 10 + k
        let mut places = [1, 2, 3, 4, 5, 6];
        places[p_gate] = 0; // 1st place -> win (scene 3)
        let times = [10000; 6];
        let scores = [0; 6];
        host.enter_end_race(&mut career, counts, places, times, scores);
        assert!((10..15).contains(&host.end.first), "rival scene expected, got {}", host.end.first);
        assert_eq!(host.end.second, 3, "win scene 3 expected");

        let mut end_scripts = Vec::new();
        let mut post_clips = 0usize;
        for _ in 0..20 {
            let step = host.step_end_race(&career, 6, false, false, &mut |_| -1);
            if let Some(sc_name) = step.start_script {
                assert!(cml.has(&sc_name), "EndRace script {sc_name} must exist in Garibaldi CML");
                let mut d = tricky_data::intro::Director::new(&cml, &[sc_name.as_str()]);
                for _ in 0..600 {
                    d.step(&cml, 1.0 / 60.0, &mut || streams.fx());
                    if let Some(ref sname) = d.scene {
                        let actors = cml.scene_actors(sname);
                        if !actors.is_empty() {
                            let (clips, _) = host.assign_scene_actors(&actors, &app_chars, &app_boards, 0, &cml);
                            let non_empty = clips.iter().filter(|c| !c.is_empty()).count();
                            assert!(non_empty > 0, "post scene {sname} under {sc_name} must resolve clips: {clips:?}");
                            post_clips += non_empty;
                            break;
                        }
                    }
                    if d.done {
                        break;
                    }
                }
                end_scripts.push(sc_name);
            }
            if step.done {
                break;
            }
        }
        assert_eq!(host.phase, Phase::Done);
        assert_eq!(end_scripts.len(), 1, "heat 0 plays rival scene only (win is Null), got {end_scripts:?}");
        assert!(end_scripts[0].starts_with("FL_") && end_scripts[0].ends_with("_VS_USR"), "{end_scripts:?}");
        assert!(post_clips >= 2, "rival scene must resolve real player/rival/common clips");

        // Final heat (`heat = 2`): Career_FinalizeRoster resolves BOTH rival (`10..14`) and win (`3`, `FL_WIN_*`).
        career.w32(0x320c + 0x14, 2);
        let mut prep2 = RosterPrepareEnv {
            data: &data,
            sel: &mut sel,
            heats: &mut heats,
            env: PlayerRiderEnv::default(),
            rel: &InitialRelations,
            rand_fx: || streams.fx(),
        };
        career.prepare_race(&mut prep2);
        host.setup_from_career(&career, &mut || streams.fx());
        let rows2 = career.race_roster();
        let p_gate2 = rows2.iter().position(|r| r.is_human()).expect("human row");
        let mut app_chars2 = vec![rows2[p_gate2].character];
        let mut app_boards2 = vec![rows2[p_gate2].board_kind];
        for r in &rows2 {
            if !r.is_human() {
                app_chars2.push(r.character);
                app_boards2.push(r.board_kind);
            }
        }
        let mut counts2 = [[0i32; 6]; 6];
        let rival2 = (0..6).find(|&i| i != p_gate2).unwrap();
        counts2[rival2][p_gate2] = 4;
        let mut places2 = [1, 2, 3, 4, 5, 6];
        places2[p_gate2] = 0;
        host.enter_end_race(&mut career, counts2, places2, times, scores);
        let mut final_scripts = Vec::new();
        let mut final_clips = 0usize;
        for _ in 0..20 {
            let step = host.step_end_race(&career, 6, false, false, &mut |_| -1);
            if let Some(sc_name) = step.start_script {
                assert!(cml.has(&sc_name), "Final EndRace script {sc_name} must exist in Garibaldi CML");
                let mut d = tricky_data::intro::Director::new(&cml, &[sc_name.as_str()]);
                for _ in 0..600 {
                    d.step(&cml, 1.0 / 60.0, &mut || streams.fx());
                    if let Some(ref sname) = d.scene {
                        let actors = cml.scene_actors(sname);
                        if !actors.is_empty() {
                            let (clips, _) = host.assign_scene_actors(&actors, &app_chars2, &app_boards2, 0, &cml);
                            let non_empty = clips.iter().filter(|c| !c.is_empty()).count();
                            assert!(non_empty > 0, "final scene {sname} under {sc_name} must resolve clips: {clips:?}");
                            final_clips += non_empty;
                            break;
                        }
                    }
                    if d.done {
                        break;
                    }
                }
                final_scripts.push(sc_name);
            }
            if step.done {
                break;
            }
        }
        assert_eq!(host.phase, Phase::Done);
        assert_eq!(final_scripts.len(), 2, "final heat plays rival + win scripts, got {final_scripts:?}");
        assert!(final_scripts[0].starts_with("FL_") && final_scripts[0].ends_with("_VS_USR"), "{final_scripts:?}");
        assert!(final_scripts[1].starts_with("FL_WIN_"), "{final_scripts:?}");
        assert!(final_clips >= 4, "both final scenes must resolve real clips, got {final_clips}");
    }

    #[test]
    fn disc_pre_race_skip_and_showoff_loss_paths() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let mut disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
        let elf = disc.read_path("/SLUS_203.26").expect("read elf").expect("SLUS_203.26");
        let data = RosterData::from_elf(&elf).expect("RosterData::from_elf");
        let mut files = tricky_data::big::Files::new(disc);
        files.mount_game_archives().expect("mount archives");
        let cml = Cml::from_disc(&mut files, 0).expect("Garibaldi CML");

        let (mut career, _) = CareerFile::open(None, 0, 1, None);
        for k in 0..4 {
            career.clear_player_slot(k);
        }
        career.assign_player_rider(0, 0, 7, &crate::cheats::Cheats::default()); // Elise
        career.w32(0x2af8, 0); // Garibaldi
        career.w32(0x2afc, 4); // Circuit Race
        career.w32(0x320c + 0x14, 0);
        career.w32(0x27dc, 6);

        let mut streams = Streams::default();
        streams.seed_fx(0x9999_0001);
        let mut sel = PreRaceSel::default();
        let mut heats = CircuitHeats::default();
        let mut prep = RosterPrepareEnv {
            data: &data,
            sel: &mut sel,
            heats: &mut heats,
            env: PlayerRiderEnv::default(),
            rel: &InitialRelations,
            rand_fx: || streams.fx(),
        };
        career.prepare_race(&mut prep);

        let mut host = SceneHost::default();
        host.setup_from_career(&career, &mut || streams.fx());
        host.enter_pre_race(&mut career, 6, false);
        // Step 1 starts scene 0 (RaceLoaded); skipping scene 0 advances to scene 1 (STG_*); skipping scene 1
        // skips scene 2 too (`state = 0xe -> 0x15` done) per `cPreRaceHandler::Update` 0x112d10.
        let s0 = host.step_pre_race(&career, 6, false, false, &mut |_| -1);
        assert!(s0.start_script.is_some());
        let s1 = host.step_pre_race(&career, 6, true, true, &mut |_| -1);
        assert!(s1.start_script.is_some());
        let s2 = host.step_pre_race(&career, 6, true, true, &mut |_| -1);
        assert!(s2.done && s2.place_at_start, "skipping scene 1 must skip scene 2 straight to start");

        // Show-off (mode 5) with score 0 -> loss (`end.second == 4`, `FL_LOS_*`)
        career.w32(0x2afc, 5);
        career.w32(0x27dc, 1);
        let mut prep_so = RosterPrepareEnv {
            data: &data,
            sel: &mut sel,
            heats: &mut heats,
            env: PlayerRiderEnv::default(),
            rel: &InitialRelations,
            rand_fx: || streams.fx(),
        };
        career.prepare_race(&mut prep_so);
        host.setup_from_career(&career, &mut || streams.fx());
        host.enter_end_race(&mut career, [[0; 6]; 6], [0; 6], [10000; 6], [0; 6]);
        assert_eq!(host.end.first, 0x10);
        assert_eq!(host.end.second, 4);
        let step = host.step_end_race(&career, 1, false, false, &mut |_| -1);
        let sc = step.start_script.expect("show-off loss script");
        assert!(sc.starts_with("FL_LOS_") && cml.has(&sc), "expected FL_LOS_* in CML, got {sc}");
        let end_skip = host.step_end_race(&career, 1, true, true, &mut |_| -1);
        assert!(end_skip.done, "skipping post-race scene must advance to results");
    }
}
