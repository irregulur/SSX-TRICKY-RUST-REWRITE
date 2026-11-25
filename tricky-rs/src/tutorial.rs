//! Trick Tutorial host in the app (board row C11e, game mode 6 on the `trick` course):
//! drives `tricky_game::lesson::{Lesson, RaceLesson}` (`LessonMan_Init` 0x14aaa0, `cLessonMan::vf1` 0x14b9b8,
//! `cAI::vf1` 0x1126f0 modes 6..10), `tricky_game::input::LessonPads` (`Pad_GetConfig` 0x17a290 / `Pad_SetConfig`
//! 0x17a2f8 swapping to default `BTNMAP0.DAT` on `cLessonPreviewHandler::Enter` 0x14e138 and restoring the player's
//! map on `Exit` 0x14e2b0 and tutorial exit), `tricky_data::inp::{Inp, Playback}` (`data/tutorial/<rider>.big` ->
//! `|data\tutorial\<rider><NN>.inp` playback hooks `0x14b7c8` / `0x14b8a8`), `fxrand::lesson_path_var`
//! (`Audio_OnRaceGo` 0x215288 mode 6), and `Score_TrickBookCheck` `0x1579e0` -> `Lesson_SetTrickSeen` `0x14c2a8`.

use bevy::prelude::*;
use tricky_data::inp::{Inp, Playback};
use tricky_game::career::Career;
use tricky_game::controls::{ButtonMap, PadDevice};
use tricky_game::input::{LessonPads, PadConfig, PadTables};
use tricky_game::lesson::{btn, recording_path, text, Env as LessonEnv, Lesson, RaceLesson, State, RIDER_SHORT};
use tricky_game::rider::Rider;

/// One-tick actions emitted by `Lesson` / `RaceLesson` for `main.rs`'s `ride` loop to apply to the rider/race.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TutorialActions {
    /// `Lesson_ResetScene` 0x14d7d0: reset world/rider/camera to start of the `trick` course.
    pub reset_scene: bool,
    /// `Lesson_RestartRider` 0x14d898: place rider back at the start point waiting for the lesson.
    pub restart_rider: bool,
    /// `Lesson_SetupRider` 0x14b768: `(boost_meter, uber_timer)` — page 5 gives `(1.0, 20.0)`, else `(0.0, 0.0)`.
    pub set_speed: Option<(f32, f32)>,
    /// `Boarder_Finish` + `Race_GoToEndLesson` 0x1117f0.
    pub rider_finish: bool,
    /// `cLessonPracticeHandler::Enter` 0x14e6d0 (`sb zero, 0x5a70`): clear the rider's displayed trick text.
    pub clear_trick_text: bool,
    /// `cAI::vf9` mode requested by `Lesson` (`3` Countdown/start run, `8` RestartLesson).
    pub race_mode: Option<u32>,
    /// `Pad_SetConfig` 0x17a2f8 swap requested (`Some(true)` -> lesson map 0, `Some(false)` -> player's map).
    pub pad_swap: Option<bool>,
    /// `cLessonExitHandler::Update`: leave tutorial back to the main menu.
    pub exit_to_menu: bool,
}

/// Live Trick Tutorial session resource (`cLessonMan` + `cAI` modes 6..10, row C11e).
#[derive(Resource)]
pub struct TutorialHost {
    pub active: bool,
    pub rider_idx: usize,
    pub race_lesson: RaceLesson,
    pub lesson: Lesson,
    pub pads: Option<LessonPads>,
    pub using_lesson_pad: bool,
    /// Mounted `data/tutorial/<rider>.big` archive bytes from `TRICKY_DISC` (`LessonMan_Init` 0x14aaa0).
    archive: Option<Vec<u8>>,
    /// Active demonstration recording (`|data\tutorial\<rider><NN>.inp`) and playback cursor (`0x14b7c8` / `0x14b8a8`).
    pub recording: Option<Inp>,
    pub playback: Playback,
    /// Exit confirmation dialog (`Dialog_Set` / `Dialog_PollButtons`).
    pub dialog_open: bool,
    pub dialog_strings: [u32; 3],
    pub dialog_yes: bool,
    /// Active HUD item bitmask (`hud_show` / `hud_hide`: `10` tutorial menu overlay, `0xe` attempt HUD).
    pub hud_mask: u32,
    /// `Audio_OnRaceGo` 0x215288 mode 6: `fxrand::lesson_path_var(Rand_Fx())` drawn when entering the tutorial.
    pub path_var: Option<u32>,
    /// `gApp+0x730 -> +0x54` done flag set by `cLessonExitHandler`.
    pub done: bool,
    /// Cached disc path for loading `data/tutorial/<rider>.big` (tests may inject `archive` directly).
    disc_path: Option<String>,
}

impl Default for TutorialHost {
    fn default() -> Self {
        Self {
            active: false,
            rider_idx: 0,
            race_lesson: RaceLesson::default(),
            lesson: Lesson::new(0),
            pads: None,
            using_lesson_pad: false,
            archive: None,
            recording: None,
            playback: Playback::default(),
            dialog_open: false,
            dialog_strings: [0; 3],
            dialog_yes: true,
            hud_mask: 0,
            path_var: None,
            done: false,
            disc_path: std::env::var("TRICKY_DISC").ok(),
        }
    }
}

impl TutorialHost {
    /// Start a new Trick Tutorial session (`LessonMan_Init` 0x14aaa0 -> `Lesson::start`, `RaceLesson` mode 6
    /// `LoadLesson`, `fxrand::lesson_path_var` 0x215288). `player_map` is the player's active `ButtonMap` before
    /// starting; `map0` is always `BTNMAP0.DAT` (`Pad_LoadButtonMap(0)`).
    pub fn start(
        &mut self,
        rider_idx: usize,
        player_map: &mut ButtonMap,
        map0: Option<&PadConfig>,
        slots: [u16; 15],
        rand_fx: &mut dyn FnMut() -> u32,
    ) {
        let rider_idx = rider_idx.min(RIDER_SHORT.len() - 1);
        let (lesson, started) = Lesson::start(rider_idx, player_map, map0, slots);
        self.active = true;
        self.rider_idx = rider_idx;
        self.lesson = lesson;
        self.race_lesson = RaceLesson::default();
        self.race_lesson.set_mode(6); // cLoadLessonHandler
        self.pads = Some(started.pads);
        self.using_lesson_pad = false;
        self.recording = None;
        self.playback = Playback::default();
        self.dialog_open = false;
        self.dialog_strings = [0; 3];
        self.dialog_yes = true;
        self.hud_mask = 0;
        self.done = false;
        self.path_var = Some(tricky_game::fxrand::lesson_path_var(rand_fx()));
        self.archive = self.load_archive(&started.archive);
    }

    /// Inject pre-loaded `data/tutorial/<rider>.big` bytes (for unit tests without re-opening the disc).
    #[cfg(test)]
    pub fn set_archive_bytes(&mut self, bytes: Vec<u8>) {
        self.archive = Some(bytes);
    }

    /// Leave the Trick Tutorial cleanly, restoring the player's own `PadTables` (`Pad_SetConfig` 0x17a2f8).
    pub fn exit(&mut self, rider: &mut Rider) {
        if self.using_lesson_pad {
            if let Some(pads) = self.pads {
                rider.pad_tables = Some(pads.player);
            }
            self.using_lesson_pad = false;
        }
        rider.pad_tables = None;
        self.active = false;
        self.recording = None;
        self.playback = Playback::default();
        self.archive = None;
        self.dialog_open = false;
        self.done = false;
    }

    /// Active `PadTables` for `Rider::pad_tables` (`Pad_SetConfig` 0x17a2f8): lesson map 0 during `State::Preview`,
    /// player's map otherwise.
    pub fn active_pad_tables(&self) -> Option<PadTables> {
        let pads = self.pads?;
        Some(if self.using_lesson_pad { pads.lesson } else { pads.player })
    }

    /// Whether the rider is currently riding an attempt (`Preview`, `Perform`, or `Practice` after countdown).
    pub fn is_riding_attempt(&self) -> bool {
        self.active && matches!(self.lesson.state, State::Preview | State::Perform | State::Practice)
    }

    /// Whether a tutorial menu or dialog is up (consuming navigation keys/buttons).
    pub fn menu_open(&self) -> bool {
        self.active
            && (self.dialog_open
                || matches!(
                    self.lesson.state,
                    State::Wait | State::PreviewEnd | State::PracticeEnd | State::End
                ))
    }

    /// Target trick index (`0..30`, `lesson+0x4e8`) for `Score_TrickBookCheck` `0x1579e0` (`lesson != 0`).
    pub fn active_trick(&self) -> Option<u32> {
        self.active.then_some(self.lesson.trick.max(0) as u32)
    }

    /// `Lesson_SetTrickSeen` `0x14c2a8` (`lesson+0x64 = 1`), called when `Book::check_career` fires `lesson_done`.
    pub fn on_trick_seen(&mut self) {
        if self.active {
            self.lesson.trick_seen = true;
        }
    }

    /// `Lesson_SetAttemptDone` `0x14d930` (`lesson+0x54 = 1` when 0), called on the rider's finish event (`cBoarder::vf10`).
    pub fn on_rider_finish(&mut self) {
        if self.active && !self.lesson.attempt_done {
            self.lesson.attempt_done = true;
        }
    }

    /// `FUN_0014b7c8` 0x14b7c8 (before `GetControls` in `State::Preview`): returns the recorded `PadDevice` for the
    /// current playback frame so the rider replays `|data\tutorial\<rider><NN>.inp` on the default map.
    pub fn preview_pad(&self) -> Option<PadDevice> {
        if !self.active || self.lesson.state != State::Preview {
            return None;
        }
        let inp = self.recording.as_ref()?;
        let p = self.playback.pad_for(inp)?;
        Some(PadDevice {
            prev: p.prev,
            buttons: p.now,
            dpad: [p.axes[0], p.axes[1]],
            left: [p.axes[2], p.axes[3]],
            right: [p.axes[4], p.axes[5]],
        })
    }

    /// `FUN_0014b8a8` 0x14b8a8 (after `GetControls` in `State::Preview` and `State::Perform`): advances the
    /// recording cursor while below `rec_len` and mirrors it into `lesson.rec_pos`.
    pub fn after_controls(&mut self) {
        if !self.active || !matches!(self.lesson.state, State::Preview | State::Perform) {
            return;
        }
        if let Some(inp) = &self.recording {
            self.playback.after_controls(inp);
            self.lesson.rec_pos = self.playback.pos;
        }
    }

    fn load_archive(&self, path: &str) -> Option<Vec<u8>> {
        let disc_path = self.disc_path.as_deref()?;
        let disc = tricky_data::disc::Disc::open(disc_path).ok()?;
        let mut files = tricky_data::big::Files::new(disc);
        files.load(path).ok().flatten()
    }

    /// Load `|data\tutorial\<rider><NN>.inp` (`recording_path`) from the mounted `data/tutorial/<rider>.big` archive.
    fn load_recording_entry(&mut self, rider: usize, trick: i32) -> u32 {
        let want = recording_path(rider, trick);
        let suffix = want.strip_prefix('|').unwrap_or(&want);
        let slash = suffix.replace('\\', "/");
        let Some(big) = self.archive.as_deref() else {
            self.recording = None;
            self.playback = Playback::default();
            return 0;
        };
        let entry = tricky_data::big::entries(big).into_iter().find(|e| {
            e.name.eq_ignore_ascii_case(suffix)
                || e.name.eq_ignore_ascii_case(&slash)
                || e.name.replace('\\', "/").eq_ignore_ascii_case(&slash)
        });
        let Some(e) = entry else {
            self.recording = None;
            self.playback = Playback::default();
            return 0;
        };
        let Some(raw) = tricky_data::big::file_data(big, &e.name) else {
            self.recording = None;
            self.playback = Playback::default();
            return 0;
        };
        let unpacked = tricky_data::compress::load(raw.to_vec()).unwrap_or_else(|| raw.to_vec());
        match Inp::parse(&unpacked) {
            Some(inp) => {
                let len = inp.len();
                self.recording = Some(inp);
                self.playback = Playback::default();
                len
            }
            None => {
                self.recording = None;
                self.playback = Playback::default();
                0
            }
        }
    }

    /// Convert Bevy keyboard + gamepad edge inputs into `lesson::btn` edge masks (`Lesson_HandleMenuInput` 0x14d6e0).
    pub fn menu_edges_from_input(keys: &ButtonInput<KeyCode>, pad: Option<&Gamepad>, menu_level: i32) -> u32 {
        let gp = |b: GamepadButton| pad.is_some_and(|p| p.just_pressed(b));
        let up = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) || gp(GamepadButton::DPadUp);
        let down =
            keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) || gp(GamepadButton::DPadDown);
        let left =
            keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) || gp(GamepadButton::DPadLeft);
        let right = keys.just_pressed(KeyCode::ArrowRight)
            || keys.just_pressed(KeyCode::KeyD)
            || gp(GamepadButton::DPadRight);
        let accept = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || gp(GamepadButton::South);
        let back = keys.just_pressed(KeyCode::Escape)
            || keys.just_pressed(KeyCode::Backspace)
            || gp(GamepadButton::East)
            || gp(GamepadButton::North);

        let mut b = 0u32;
        if menu_level == 2 {
            if up { b |= btn::MENU_UP; }
            if down { b |= btn::MENU_DOWN; }
        } else {
            // Page / trick list uses UP/DOWN (0x10_0008 / 0x8_0004: left/right on pad + up/down on keyboard)
            if up || left { b |= btn::UP; }
            if down || right { b |= btn::DOWN; }
        }
        if accept { b |= btn::ACCEPT; }
        if back { b |= btn::BACK; }
        b
    }

    /// Advance `RaceLesson` (`cAI::vf1` 0x1126f0 modes 6..10) and `Lesson` (`cLessonMan::vf1` 0x14b9b8) by one 60 Hz tick.
    pub fn tick(
        &mut self,
        pressed_edges: u32,
        trick_text_nonempty: bool,
        blocked: bool,
        rand_fx: &mut dyn FnMut() -> u32,
    ) -> TutorialActions {
        if !self.active {
            return TutorialActions::default();
        }
        let mut actions = TutorialActions::default();

        // Exit dialog choice toggle before `Dialog_PollButtons` runs inside `cLessonWaitHandler::Update`.
        if self.dialog_open && (pressed_edges & (btn::UP | btn::DOWN | btn::MENU_UP | btn::MENU_DOWN)) != 0 {
            self.dialog_yes = !self.dialog_yes;
        }
        let dialog_poll_res = if self.dialog_open {
            if (pressed_edges & btn::ACCEPT) != 0 {
                self.dialog_open = false;
                if self.dialog_yes { 1 } else { 2 }
            } else if (pressed_edges & btn::BACK) != 0 {
                self.dialog_open = false;
                2
            } else {
                0
            }
        } else {
            0
        };

        // Run `RaceLesson` and `Lesson` with a scoped `Env` adapter (borrowing fields disjointly).
        let mut race_lesson = self.race_lesson;
        let mut lesson = self.lesson.clone();
        let mut env = HostEnv {
            host: self,
            pressed: pressed_edges,
            trick_text_nonempty,
            blocked,
            dialog_poll_res,
            rand_fx,
            actions: &mut actions,
        };
        race_lesson.tick(&mut lesson, &mut env);
        lesson.tick(&mut env);
        self.race_lesson = race_lesson;
        self.lesson = lesson;
        if self.done {
            actions.exit_to_menu = true;
        }
        actions
    }

    /// Localized tutorial menu/status overlay text (`Loc_GetString` 0x2c5160 + `DATA/TUTORIAL/TRICKDEF.DAT`).
    pub fn panel_text(&self, book: &crate::book::Book, career: &Career, loc: &crate::loc::GameText) -> Option<String> {
        if !self.active {
            return None;
        }
        let ch = self.rider_idx as u32;
        let bits = if book.defs.is_some() { crate::book::Book::bits_in_career(career, ch) } else { 0 };
        let trick_name = |t: i32| -> String {
            book.defs
                .as_ref()
                .and_then(|d| d.get(self.rider_idx))
                .and_then(|r| r.get(t as usize))
                .map(|td| td.name().trim().to_string())
                .unwrap_or_else(|| format!("Trick {}", t + 1))
        };

        if self.dialog_open {
            let prompt = loc.or(self.dialog_strings[0], "Exit Trick Tutorial?");
            let yes = loc.or(self.dialog_strings[1], "Yes");
            let no = loc.or(self.dialog_strings[2], "No");
            let y = if self.dialog_yes { format!(">  {yes}  <") } else { yes };
            let n = if !self.dialog_yes { format!(">  {no}  <") } else { no };
            return Some(format!("{prompt}\n\n{y}    {n}\n\nEnter: Confirm    Esc: Cancel"));
        }

        let header = loc.or(text::PAGE_TITLE, "TRICK TUTORIAL");
        let chapter = loc.chapter((self.lesson.page + 1) as usize);
        let cur_trick = trick_name(self.lesson.trick);
        match self.lesson.state {
            State::Wait => match self.lesson.menu_level {
                0 => {
                    let mut out = format!("{header}\n\n");
                    for p in 0..tricky_game::lesson::PAGES {
                        let done = (0..5).filter(|i| (bits >> (p * 5 + i)) & 1 != 0).count();
                        let line = format!("{}  ({done}/5)", loc.chapter((p + 1) as usize));
                        if p == self.lesson.page {
                            out += &format!(">  {line}  <\n");
                        } else {
                            out += &format!("{line}\n");
                        }
                    }
                    out += "\nUp/Down: Chapter    Enter: Select";
                    Some(out)
                }
                1 => {
                    let mut out = format!("{header}  -  {chapter}\n\n");
                    let first = self.lesson.page * 5;
                    for t in first..first + 5 {
                        let mark = if (bits >> t) & 1 != 0 { " [x]" } else { "" };
                        let line = format!("{}{mark}", trick_name(t));
                        if t == self.lesson.trick {
                            out += &format!(">  {line}  <\n");
                        } else {
                            out += &format!("{line}\n");
                        }
                    }
                    out += "\nUp/Down: Trick    Enter: Select    Esc: Back";
                    Some(out)
                }
                _ => {
                    let mut out = format!("{header}  -  {chapter}\n{cur_trick}\n\n");
                    const FALLBACK: [&str; 4] = ["Watch Demo", "Try Trick", "Practice", "Quit Tutorial"];
                    for (i, &sid) in text::ACTIONS.iter().enumerate() {
                        let label = loc.or(sid, FALLBACK[i]);
                        if i as i32 == self.lesson.choice {
                            out += &format!(">  {label}  <\n");
                        } else {
                            out += &format!("{label}\n");
                        }
                    }
                    out += "\nUp/Down: Move    Enter: Select    Esc: Back";
                    Some(out)
                }
            },
            State::PreviewEnd => {
                let title = loc.or(text::AFTER_TITLE, "Demo Complete");
                let mut out = format!("{title}\n{cur_trick}\n\n");
                const FALLBACK: [&str; 4] = ["Select Trick", "Try Trick", "Watch Again", "Quit Tutorial"];
                for (i, &sid) in text::PREVIEW_END.iter().enumerate() {
                    let label = loc.or(sid, FALLBACK[i]);
                    if i as i32 == self.lesson.choice {
                        out += &format!(">  {label}  <\n");
                    } else {
                        out += &format!("{label}\n");
                    }
                }
                Some(out)
            }
            State::PracticeEnd | State::End => {
                let verdict = if self.lesson.state == State::End {
                    if self.lesson.trick_seen {
                        format!("{cur_trick}  -  COMPLETED!")
                    } else {
                        format!("{cur_trick}  -  TRY AGAIN")
                    }
                } else {
                    format!("{chapter}  -  Practice")
                };
                let mut out = format!("{verdict}\n\n");
                const FALLBACK: [&str; 3] = ["Select Trick", "Try Again", "Quit Tutorial"];
                for (i, &sid) in text::PRACTICE_END.iter().enumerate() {
                    let label = loc.or(sid, FALLBACK[i]);
                    if i as i32 == self.lesson.choice {
                        out += &format!(">  {label}  <\n");
                    } else {
                        out += &format!("{label}\n");
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }
}

struct HostEnv<'a> {
    host: &'a mut TutorialHost,
    pressed: u32,
    trick_text_nonempty: bool,
    blocked: bool,
    dialog_poll_res: i64,
    rand_fx: &'a mut dyn FnMut() -> u32,
    actions: &'a mut TutorialActions,
}

impl LessonEnv for HostEnv<'_> {
    fn dialog_active(&self) -> bool {
        self.host.dialog_open || self.dialog_poll_res != 0
    }
    fn blocked(&self) -> bool {
        self.blocked
    }
    fn cam_running(&self) -> bool {
        false
    }
    fn dialog_poll(&mut self) -> i64 {
        std::mem::take(&mut self.dialog_poll_res)
    }
    fn dialog_set(&mut self, strings: [u32; 3]) {
        self.host.dialog_open = true;
        self.host.dialog_strings = strings;
        self.host.dialog_yes = true;
    }
    fn pressed(&mut self) -> u32 {
        std::mem::take(&mut self.pressed)
    }
    fn nav_sound(&mut self) {}
    fn back_sound(&mut self) {}
    fn select_sound(&mut self) {}
    fn page_first_trick(&self, page: i32) -> i32 {
        page * 5
    }
    fn hud_show(&mut self, item: u32) {
        if item < 32 {
            self.host.hud_mask |= 1 << item;
        }
    }
    fn hud_hide(&mut self, item: u32) {
        if item < 32 {
            self.host.hud_mask &= !(1 << item);
        }
    }
    fn speech_clear(&mut self) {}
    fn speech_line(&mut self, _n: u32) {}
    fn cam_script(&mut self, _name: &str) {}
    fn cam_cut_fade(&mut self) {}
    fn reset_scene(&mut self) {
        self.actions.reset_scene = true;
    }
    fn set_mode(&mut self, mode: u32) {
        self.actions.race_mode = Some(mode);
        if mode == 8 {
            self.host.race_lesson.set_mode(8);
        }
    }
    fn restart_rider(&mut self) {
        self.actions.restart_rider = true;
    }
    fn rider_finish(&mut self) {
        self.actions.rider_finish = true;
        self.host.race_lesson.set_mode(10);
    }
    fn set_rider_speed(&mut self, a: f32, b: f32) {
        self.actions.set_speed = Some((a, b));
    }
    fn pad_map(&mut self, lesson: bool) {
        self.host.using_lesson_pad = lesson;
        self.actions.pad_swap = Some(lesson);
    }
    fn load_recording(&mut self, rider: usize, trick: i32) -> u32 {
        self.host.load_recording_entry(rider, trick)
    }
    fn replay_clear(&mut self) {}
    fn rand_fx(&mut self) -> u32 {
        (self.rand_fx)()
    }
    fn trick_text_nonempty(&self) -> bool {
        self.trick_text_nonempty
    }
    fn clear_trick_text(&mut self) {
        self.actions.clear_trick_text = true;
    }
    fn set_done(&mut self) {
        self.host.done = true;
    }
    fn load_step(&mut self) -> bool {
        true
    }
    fn clear_pad_key(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::padhost::PadHost;

    #[test]
    fn tutorial_loads_trick_course_swaps_pad_map_plays_inp_and_checks_trick_book() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };

        // 1. Real `trick` course loads from the disc (`E3a4`) with collision (`CourseGround`).
        let mut files = tricky_data::big::Files::new(tricky_data::disc::Disc::open(&disc_path).expect("open disc"));
        let level = tricky_data::level::Level::from_disc(&mut files, "trick", None).expect("trick course from disc");
        assert!(!level.patches.is_empty(), "trick course has terrain patches");
        let pbd = files.load("|data/models/trick.pbd").expect("pbd").expect("trick.pbd");
        let ltg = files.load("|data/models/trick.ltg").expect("ltg").expect("trick.ltg");
        let ssf = files.load("|data/models/trick.ssf").expect("ssf").expect("trick.ssf");
        assert!(tricky_game::course_ground::CourseGround::new(&pbd, &ltg, &ssf).is_some());

        // 2. Player starts on BTNMAP1.DAT (config 1), tutorial builds LessonPads with BTNMAP0.DAT (config 0).
        let mut pad_host = PadHost::load();
        pad_host.config_idx = 1;
        let mut rider = Rider::new(Vec3::ZERO, 0.0);
        rider.data = &tricky_game::trickdata::RIDERS[3]; // Mac (rider index 3)
        rider.pad_cfg = pad_host.active_config();
        let mut player_map = rider.button_map();
        let orig_pressed = player_map.pressed;
        let map0 = pad_host.config(0).cloned();
        let slots = rider.trick_slots();

        let mut host = TutorialHost::default();
        let mut fx_seed = 12345u32;
        host.start(3, &mut player_map, map0.as_ref(), slots, &mut || {
            fx_seed = fx_seed.wrapping_add(1);
            fx_seed
        });
        assert!(host.active);
        assert_eq!(host.path_var, Some(12346 % 100), "Audio_OnRaceGo mode 6 draws lesson_path_var");
        assert!(!host.using_lesson_pad);

        // Advance LoadLesson -> Intro -> Start -> Wait.
        let mut dummy_fx = || 0u32;
        for _ in 0..4 {
            let act = host.tick(0, false, false, &mut dummy_fx);
            if let Some(to_lesson) = act.pad_swap {
                rider.pad_tables = host.active_pad_tables();
                assert_eq!(host.using_lesson_pad, to_lesson);
            }
        }
        assert_eq!(host.lesson.state, State::Wait);
        assert_eq!(host.lesson.menu_level, 0);

        // Navigate: Accept page 0 -> Accept trick 0 -> Accept action 0 (Preview)!
        host.tick(btn::ACCEPT, false, false, &mut dummy_fx);
        assert_eq!(host.lesson.menu_level, 1);
        host.tick(btn::ACCEPT, false, false, &mut dummy_fx);
        assert_eq!(host.lesson.menu_level, 2);
        let act = host.tick(btn::ACCEPT, false, false, &mut dummy_fx);
        assert!(act.pad_swap.is_none());
        // Next tick switches into State::Preview -> enters Preview, swaps pad map to BTNMAP0.DAT, loads Mac01.inp!
        let act = host.tick(0, false, false, &mut dummy_fx);
        assert_eq!(host.lesson.state, State::Preview);
        assert_eq!(act.pad_swap, Some(true));
        assert_eq!(act.set_speed, Some((0.0, 0.0)), "page 0 resets boost/uber to (0, 0)");
        rider.pad_tables = host.active_pad_tables();
        assert!(host.recording.as_ref().is_some_and(|i| i.len() >= 1000), "Mac01.inp loaded from Mac.big");

        // Compare Rider::button_map() during Preview vs player's BTNMAP1.DAT: Preview uses BTNMAP0.DAT!
        let map0_expected = tricky_game::input::bind(map0.as_ref().unwrap(), slots);
        let preview_map = rider.button_map();
        assert_eq!(preview_map.pressed[0x1b..0x29], map0_expected.pressed[0x1b..0x29], "Preview uses BTNMAP0.DAT");

        // Play all recorded frames through `preview_pad` + `after_controls` until `PreviewEnd` restores player map!
        let len = host.lesson.rec_len;
        for _ in 0..len {
            assert!(host.preview_pad().is_some());
            host.after_controls();
        }
        assert_eq!(host.lesson.rec_pos, len);
        host.tick(0, false, false, &mut dummy_fx); // Preview Update sees rec_pos >= rec_len -> requests PreviewEnd
        let act = host.tick(0, false, false, &mut dummy_fx); // switches to PreviewEnd -> Preview Exit swaps map back!
        assert_eq!(host.lesson.state, State::PreviewEnd);
        assert_eq!(act.pad_swap, Some(false));
        rider.pad_tables = host.active_pad_tables();
        assert_eq!(rider.button_map().pressed, orig_pressed, "Preview Exit restores player's BTNMAP1.DAT");

        // From PreviewEnd choose choice 1 (Perform): test `Score_TrickBookCheck` lesson gate!
        host.tick(btn::MENU_DOWN, false, false, &mut dummy_fx);
        assert_eq!(host.lesson.choice, 1);
        host.tick(btn::ACCEPT, false, false, &mut dummy_fx);
        host.tick(0, false, false, &mut dummy_fx);
        assert_eq!(host.lesson.state, State::Perform);

        let td_bytes = files.load("data/tutorial/trickdef.dat").expect("load").expect("trickdef.dat");
        let book = Book::from_defs(tricky_data::trickdef::read(&td_bytes).expect("parse"));
        let (mut career, _) = tricky_game::career::CareerFile::open(None, 0, 1, None);
        for k in 0..4 {
            career.clear_player_slot(k);
        }
        career.assign_player_rider(0, 0, 3, &tricky_game::cheats::Cheats::default()); // Mac = 3
        let mut sc = tricky_game::rider::new_score();
        let target_id = book.trick_info(3, host.active_trick().unwrap());
        let out = book.check_career(&mut career, &mut sc, target_id, true, host.active_trick());
        assert!(out.lesson_done && !out.book_done, "tutorial checks lesson target without ticking career bits");
        host.on_trick_seen();
        assert!(host.lesson.trick_seen);

        // Rider finishes the run -> State::End sees trick_seen == true.
        host.on_rider_finish();
        host.tick(0, false, false, &mut dummy_fx);
        host.tick(0, false, false, &mut dummy_fx);
        assert_eq!(host.lesson.state, State::End);
        assert!(host.lesson.trick_seen);

        // Exit tutorial restores rider.pad_tables = None cleanly.
        host.exit(&mut rider);
        assert!(!host.active && rider.pad_tables.is_none());
        assert_eq!(rider.button_map().pressed, orig_pressed);
    }

    #[test]
    fn tutorial_page_5_sets_uber_meter_and_timer() {
        let rider = Rider::new(Vec3::ZERO, 0.0);
        let mut map = rider.button_map();
        let slots = rider.trick_slots();
        let mut host = TutorialHost::default();
        let mut fx = || 7u32;
        host.start(0, &mut map, None, slots, &mut fx);
        for _ in 0..4 {
            host.tick(0, false, false, &mut fx);
        }
        assert_eq!(host.lesson.state, State::Wait);
        // Move to page 5 (Uber Tricks).
        for _ in 0..5 {
            host.tick(btn::DOWN, false, false, &mut fx);
        }
        assert_eq!(host.lesson.page, 5);
        host.tick(btn::ACCEPT, false, false, &mut fx); // menu_level 1
        host.tick(btn::ACCEPT, false, false, &mut fx); // menu_level 2
        host.tick(btn::MENU_DOWN, false, false, &mut fx); // choice 1 = Perform
        host.tick(btn::MENU_DOWN, false, false, &mut fx); // choice 2 = Practice
        host.tick(btn::ACCEPT, false, false, &mut fx);
        let act = host.tick(0, false, false, &mut fx);
        assert_eq!(host.lesson.state, State::Practice);
        assert_eq!(act.set_speed, Some((1.0, 20.0)), "page 5 gives full boost (1.0) and 20s uber_timer");
        assert!(act.clear_trick_text, "Practice Enter clears displayed trick text (0x5a70)");
    }
}
