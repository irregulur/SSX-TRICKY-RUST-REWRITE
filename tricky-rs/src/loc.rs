//! The game's localised strings (`DATA/LANG/AMERICAN.LOC`, `Loc_LoadForRegion` 0x17c150, `Loc_GetString` 0x2c5160,
//! board row E5a2), looked up by the exact `SLUS_203.26` string IDs the game's HUD, pause, results and front-end
//! callers ask for.
//!
//! Loaded once from `TRICKY_DISC` at startup. When no disc is provided, each helper falls back to a `STANDIN` English
//! literal so a folder-only run still renders legible menus and HUD labels.

use tricky_data::loc::LocFile;

/// Loaded `DATA/LANG/AMERICAN.LOC` string table (`Loc_GetString` 0x2c5160).
#[derive(Default)]
pub struct GameText {
    file: Option<LocFile>,
}

/// `cFE3DStateCustomBoard` locked-board help IDs (`LOCK_TEXTS` 0x3afb78, boards 2..=11).
pub const LOCKED_BOARD_IDS: [u32; 10] = [
    0x0cb4, 0x0cb5, 0x0cb6, 0x0cb7, 0x0cb8, 0x0cb9, 0x0cba, 0x0cbb, 0x0cbc, 0x0cbd,
];

/// `cFE3DStateCustomBody` locked-outfit help IDs (`HELP_TEXTS` 0x3afba0, outfits 1..=6).
pub const LOCKED_OUTFIT_IDS: [u32; 6] = [0x0cad, 0x0cae, 0x0caf, 0x0cb0, 0x0cb1, 0x0dcf];

/// `cResultsOverlay::cResultsOverlay` (0x1af66c..0x1af6ac -> `MODE_TITLES` 0x3384b0 -> `Results_BuildCareerPage`
/// 0x1af950): Quarter Final, Semi Final, Final results titles.
pub const RACE_ROUND_TITLE_IDS: [u32; 3] = [0x0e52, 0x0e55, 0x0e42];

/// `HUD_DrawPlayerText` 0x1a4f70 title-font medal strings (Gold, Silver, Bronze): caps and lowercase.
pub const MEDAL_CAPS_IDS: [u32; 3] = [0x0e35, 0x0e3b, 0x0e32];
pub const MEDAL_LOWER_IDS: [u32; 3] = [0x0e36, 0x0e3c, 0x0e33];

/// `cFE3DStateCustomBoard::cFE3DStateCustomBoard` 0x28c690 (and `cFE3DStateCharSelect`): Edging, Speed, Stability,
/// Tricks stat labels (`kCharTitleEdging`, `Speed`, `Stability`, `Tricks`).
pub const STAT_LABEL_IDS: [u32; 4] = [0x0025, 0x0027, 0x0028, 0x0029];

/// `cUnlockedOverlay::cUnlockedOverlay` 0x1b58c0 (`AWARD_SINGLE` 0x338268, `AWARD_PLURAL` 0x338240):
/// unlock line format string IDs indexed by `circuit::Unlock::kind` (`0..10`).
pub const UNLOCK_SINGLE_IDS: [u32; 10] = [0x023b, 0x023d, 0x0237, 0x0235, 0x0233, 0x0239, 0x0db3, 0x0e1e, 0x022a, 0x023f];
pub const UNLOCK_PLURAL_IDS: [u32; 10] = [0x023a, 0x023c, 0x0236, 0x0234, 0x0232, 0x0238, 0x0db4, 0x0e1f, 0x022a, 0x023e];

/// `WStr_Format` 0x17ce38 single-`%d` substitution for localised templates (`0x116`, `0xe29`, `0xe3f`, `0xe64`).
pub fn fmt_d(template: &str, value: i32) -> String {
    if template.contains("%d") {
        template.replacen("%d", &value.to_string(), 1)
    } else {
        format!("{template} {value}")
    }
}

/// `WStr_Format` 0x17ce38 `%d` + `%s`/`%S` substitution (`Results_AddAwardLine` 0x1b5bf8 kinds `0..=3`).
fn fmt_d_str(template: &str, value: i32, text: &str) -> String {
    let s = template.replacen("%d", &value.to_string(), 1);
    if s.contains("%s") {
        s.replacen("%s", text, 1)
    } else if s.contains("%S") {
        s.replacen("%S", text, 1)
    } else {
        format!("{s} ({text})")
    }
}

/// `WStr_Format` 0x17ce38 `%d` + `%d` substitution (`Results_AddAwardLine` 0x1b5bf8 kind 5).
fn fmt_dd(template: &str, first: i32, second: i32) -> String {
    template
        .replacen("%d", &first.to_string(), 1)
        .replacen("%d", &second.to_string(), 1)
}

impl GameText {
    /// Load `/DATA/LANG/AMERICAN.LOC` from `TRICKY_DISC` (`Loc_LoadForRegion` 0x17c150).
    pub fn load() -> Self {
        let file = std::env::var("TRICKY_DISC")
            .ok()
            .and_then(|p| tricky_data::disc::Disc::open(&p).ok())
            .and_then(|mut d| d.read_path("/DATA/LANG/AMERICAN.LOC").ok().flatten())
            .map(LocFile::new);
        Self { file }
    }

    pub fn from_file(file: LocFile) -> Self {
        Self { file: Some(file) }
    }

    pub fn empty() -> Self {
        Self { file: None }
    }

    pub fn is_loaded(&self) -> bool {
        self.file.is_some()
    }

    /// `Loc_GetString` 0x2c5160.
    pub fn get(&self, id: u32) -> Option<String> {
        self.file.as_ref().and_then(|l| l.get(id))
    }

    /// `Loc_GetString` 0x2c5160 with a `STANDIN` fallback used only when `TRICKY_DISC` is absent.
    pub fn or(&self, id: u32, standin: &str) -> String {
        self.get(id).unwrap_or_else(|| standin.to_string())
    }

    /// `Loc_GetString` 0x2c5160, falling back to `#<hex>` when the disc is absent (for `fe_flow` labels).
    pub fn get_or_hex(&self, id: u32) -> String {
        self.get(id).unwrap_or_else(|| format!("#{id:x}"))
    }

    // --- HUD (`HUD_DrawPlayerText` 0x1a4f70, `HUD_DrawPlayerSprites` 0x1a2c40, overlays) ---

    /// Place ordinal suffix (`HUD_DrawPlayerText` 0x1a4f70 `hudtext::PLACE_NAMES`: 0xe44, 0xe54, 0xe58, 0xe45).
    pub fn place_suffix(&self, place_zero_based: usize) -> String {
        let k = place_zero_based.min(3);
        // STANDIN fallbacks only when TRICKY_DISC is absent
        const FALLBACK: [&str; 4] = ["st", "nd", "rd", "th"];
        self.or(tricky_game::hudtext::PLACE_NAMES[k], FALLBACK[k])
    }

    /// Place ordinal (`Results_BuildCareerPage` 0x1af950 / `Results_BuildStandingsPage` 0x1b0188 `"%d%s"`).
    pub fn ordinal(&self, place_zero_based: usize) -> String {
        format!("{}{}", place_zero_based + 1, self.place_suffix(place_zero_based))
    }

    /// `HUD_DrawPlayerText` 0x1a4f70 (split marker kind 2: `kHUD_lap` 0xe27).
    pub fn lap(&self) -> String {
        self.or(0x0e27, "LAP")
    }

    /// `HUD_DrawPlayerText` 0x1a4f70 (score message kind 13: `kHUD_finallap` 0x115 / `kHUD_lapstogo` 0x116).
    pub fn laps_left(&self, left: i32) -> String {
        if left == 1 {
            self.or(0x0115, "FINAL LAP")
        } else {
            fmt_d(&self.or(0x0116, "%d LAPS TO GO"), left)
        }
    }

    /// `HUD_DrawPlayerSprites` 0x1a2c40 (score message kind 10: `kOvCheckpoint` 0xe40).
    pub fn checkpoint(&self) -> String {
        self.or(0x0e40, "CHECKPOINT")
    }

    /// `HUD_DrawPlayerSprites` 0x1a2c40 (`kOvTimeBonus` 0xe59) + `HUD_DrawPlayerText` 0x1a4f70 (`kHUD_numseconds` 0xe29).
    pub fn time_bonus(&self, seconds: i32) -> String {
        format!("{}  {}", self.or(0x0e59, "TIME BONUS"), fmt_d(&self.or(0x0e29, "%d seconds"), seconds))
    }

    /// `HUD_DrawPlayerSprites` 0x1a2c40 (score message kind 4: `kHUDKnockdown` 0xe93).
    pub fn knockdown(&self) -> String {
        self.or(0x0e93, "KNOCKDOWN!")
    }

    /// `HUD_DrawPlayerText` 0x1a4f70 (flags 0x20000: `kHUD_ubertrick` 0xdf2).
    pub fn uber_trick(&self) -> String {
        self.or(0x0df2, "UBER TRICK")
    }

    /// `cCountdownOverlay::Draw` 0x1ae4b0 (`kGO` 0xeaf).
    pub fn go(&self) -> String {
        self.or(0x0eaf, "GO!")
    }

    /// `cPopFinishOverlay::DrawText` 0x1aeb78 (`kOvFinish` 0xe43).
    pub fn finish(&self) -> String {
        self.or(0x0e43, "FINISH")
    }

    /// `cPopFinishOverlay::DrawText` 0x1aeb78 (`kOvTimeUp` 0xe5b).
    pub fn time_up(&self) -> String {
        self.or(0x0e5b, "TIME UP")
    }

    /// `HUD_DrawPlayerText` 0x1a4f70 (flags 0x1000000: Show-Off medal target caption).
    /// `next_medal`: `Some(2)` -> Bronze (`0xe32`), `Some(1)` -> For Silver (`0xe26`), `Some(0)` -> For Gold (`0xe25`),
    /// `None` -> Gold (`0xe35`) + Medal (`0xe28`).
    pub fn showoff_goal(&self, next_medal: Option<usize>, target: Option<u32>) -> String {
        match (next_medal, target) {
            (Some(2), Some(s)) => format!("{} {s}", self.or(0x0e32, "BRONZE")),
            (Some(1), Some(s)) => format!("{} {s}", self.or(0x0e26, "FOR SILVER")),
            (Some(0), Some(s)) => format!("{} {s}", self.or(0x0e25, "FOR GOLD")),
            _ => format!("{} {}", self.medal_caps(0), self.medal_word()),
        }
    }

    /// `HUD_DrawPlayerText` 0x1a4f70 (`0xe35` GOLD / `0xe3b` SILVER / `0xe32` BRONZE).
    pub fn medal_caps(&self, medal: usize) -> String {
        const FALLBACK: [&str; 3] = ["GOLD", "SILVER", "BRONZE"];
        let i = medal.min(2);
        self.or(MEDAL_CAPS_IDS[i], FALLBACK[i])
    }

    /// Title-font lowercase medals (`0xe36` gold / `0xe3c` silver / `0xe33` bronze).
    pub fn medal_lower(&self, medal: usize) -> String {
        const FALLBACK: [&str; 3] = ["gold", "silver", "bronze"];
        let i = medal.min(2);
        self.or(MEDAL_LOWER_IDS[i], FALLBACK[i])
    }

    /// `HUD_DrawPlayerText` 0x1a4f70 (`kHUD_medal` 0xe28).
    pub fn medal_word(&self) -> String {
        self.or(0x0e28, "MEDAL")
    }

    // --- Pause menu (`PauseMenu_BuildMainPage` 0x1ac678) ---

    /// `PauseMenu_BuildMainPage` 0x1ac678 (`kPauseMainTitle` 0xe63).
    pub fn pause_title(&self) -> String {
        self.or(0x0e63, "Pause")
    }

    /// `PauseMenu_BuildMainPage` 0x1ac678 (`kPauseMainContinue` 0x1db).
    pub fn pause_continue(&self) -> String {
        self.or(0x01db, "continue")
    }

    /// `PauseMenu_BuildMainPage` 0x1ac678 (`kPauseMainRestart` 0x1e0).
    pub fn pause_restart(&self) -> String {
        self.or(0x01e0, "restart")
    }

    /// `PauseMenu_BuildMainPage` 0x1ac678 (`kPauseMainQuit` 0x1de).
    pub fn pause_quit(&self) -> String {
        self.or(0x01de, "quit")
    }

    // --- Front end & Career (`Course_GetNameStringId` 0x16b650, `Career_GetStatusName` 0x1634c0) ---

    /// `Course_GetNameStringId` 0x16b650 (`tricky_game::courses::NAME_IDS`).
    pub fn course_name(&self, course_no: u32, fallback: &str) -> String {
        let id = tricky_game::courses::by_course(&tricky_game::courses::NAME_IDS, course_no as u64);
        self.or(id, fallback)
    }

    /// Resolve a level folder name through `courses::by_file` to `Course_GetNameStringId` 0x16b650.
    pub fn course_name_by_folder(&self, folder: &str) -> String {
        if let Some(c) = tricky_data::aip::big_name(folder).and_then(tricky_game::courses::by_file) {
            self.course_name(c.number, c.name)
        } else {
            folder.to_string()
        }
    }

    /// `Career_GetPlayerLevel` 0x164840 + `Career_GetStatusName` 0x1634c0 (`0x17..=0x20`: Master .. Newbie).
    pub fn status_name(&self, points: u32) -> String {
        let level = tricky_game::career::level_for_points(points as i32);
        const FALLBACK: [&str; 10] = [
            "Master", "Sensei", "Superstar", "Champ", "Veteran", "Star", "Natural", "Contender", "Rookie", "Newbie",
        ];
        let fb = FALLBACK[(level as usize).min(9)];
        tricky_game::career::status_name_id(level as i32)
            .map_or_else(|| fb.to_string(), |id| self.or(id, fb))
    }

    /// `cFE3DStateCustomBoard::cFE3DStateCustomBoard` 0x28c690 (`0x25`, `0x27`, `0x28`, `0x29`).
    pub fn stat_labels(&self) -> [String; 4] {
        [
            self.or(STAT_LABEL_IDS[0], "EDGING:"),
            self.or(STAT_LABEL_IDS[1], "SPEED:"),
            self.or(STAT_LABEL_IDS[2], "STABILITY:"),
            self.or(STAT_LABEL_IDS[3], "TRICKS:"),
        ]
    }

    /// `kCharTitlePoints` 0x26.
    pub fn points_label(&self) -> String {
        self.or(0x0026, "Points")
    }

    /// `fe_flow::EVENT_TEXTS` (`kFEEventRace` 0xe05, `kFEEventShowOff` 0xe06).
    pub fn event_race(&self) -> String {
        self.or(0x0e05, "Race")
    }

    pub fn event_showoff(&self) -> String {
        self.or(0x0e06, "Showoff")
    }

    /// `kFE_VenueRaceRecord` 0x71 / `kFE_VenueShowoffRecord` 0x72.
    pub fn race_record(&self) -> String {
        self.or(0x0071, "Race Record")
    }

    pub fn showoff_record(&self) -> String {
        self.or(0x0072, "Showoff Record")
    }

    /// `kOVBestScore` 0xe3f (`"Current Best Score: %d"`).
    pub fn best_score(&self, score: u32) -> String {
        fmt_d(&self.or(0x0e3f, "Current Best Score: %d"), score as i32)
    }

    /// `cFE3DStateCustomBoard` (`LOCK_TEXTS` 0x3afb78, boards 2..=11).
    pub fn locked_board_hint(&self, board: usize) -> Option<String> {
        let id = *LOCKED_BOARD_IDS.get(board.checked_sub(2)?)?;
        self.get(id)
    }

    /// `cFE3DStateCustomBody` (`HELP_TEXTS` 0x3afba0, outfits 1..=6).
    pub fn locked_outfit_hint(&self, outfit: usize) -> Option<String> {
        let id = *LOCKED_OUTFIT_IDS.get(outfit.checked_sub(1)?)?;
        self.get(id)
    }

    /// `kTrackLocked` 0x229 (`"Not Available"`).
    pub fn locked(&self) -> String {
        self.or(0x0229, "Not Available")
    }

    // --- Results (`cResultsOverlay` 0x1af66c, `Results_BuildCareerPage` 0x1af950, `Results_BuildMedalPage` 0x1b0bb8) ---

    /// `cResultsOverlay::cResultsOverlay` (0x1af66c..0x1af6ac -> `MODE_TITLES` 0x3384b0: `0xe52`, `0xe55`, `0xe42`).
    pub fn race_round_title(&self, round: usize) -> String {
        const FALLBACK: [&str; 3] = ["Quarter Final Results", "Semi Final Results", "Final Results"];
        let i = round.min(2);
        self.or(RACE_ROUND_TITLE_IDS[i], FALLBACK[i])
    }

    /// `Results_BuildMedalPage` 0x1b0bb8 (`kOvShowoffResults` 0xe56).
    pub fn showoff_results_title(&self) -> String {
        self.or(0x0e56, "Showoff Results")
    }

    /// `Results_BuildCareerPage` 0x1af950 (`kOvRaceQualifiedSemi` 0x19f for round 0, `kOvRaceQualifiedFinal` 0x19e).
    pub fn race_qualified(&self, round: usize) -> String {
        if round == 0 {
            self.or(0x019f, "CONGRATULATIONS!  You've qualified for the semi final round.")
        } else {
            self.or(0x019e, "CONGRATULATIONS!  You've qualified for the final round.")
        }
    }

    /// `Results_BuildCareerPage` 0x1af950 (`kOvRaceEliminated` 0x19c).
    pub fn race_eliminated(&self) -> String {
        self.or(0x019c, "SORRY!  You failed to qualify for the next round.")
    }

    /// `Results_SetMedalVerdict` 0x1b1ba8 (`kWonNoMedal` 0x249).
    pub fn won_no_medal(&self) -> String {
        self.or(0x0249, "You didn't win  ....")
    }

    /// `HUD_FormatTime` 0x1a6ba0 (`kOvDidNotFinish` 0x15d).
    pub fn dnf(&self) -> String {
        self.or(0x015d, "DNF")
    }

    /// `Results_BuildRiderStatsRows` 0x1b0998 (`kResultScore` 0xdc4).
    pub fn score_label(&self) -> String {
        self.or(0x0dc4, "score")
    }

    /// `Results_BuildCareerPage` 0x1af950 (`kOvTitleTime` 0xe61).
    pub fn time_label(&self) -> String {
        self.or(0x0e61, "time")
    }

    /// `Results_BuildCareerPage` 0x1af950 (`kOvOptionContinue` 0x18d, `kOvOptionRestart` 0x191, `kOvOptionQuitToMain` 0x18e).
    pub fn opt_continue(&self) -> String {
        self.or(0x018d, "continue").trim().to_string()
    }

    pub fn opt_restart(&self) -> String {
        self.or(0x0191, "restart").trim().to_string()
    }

    pub fn opt_quit(&self) -> String {
        self.or(0x018e, "quit").trim().to_string()
    }

    // --- Trick Book (`fe_flow::MODIFY_ITEMS` 0xe1a, `pausedraw.rs` 0xe64, 0x1b5a90 0x22a, 0x246b50 0xc9e) ---

    /// `fe_flow::MODIFY_ITEMS` (`kFETrickBook` 0xe1a).
    pub fn trick_book(&self) -> String {
        self.or(0x0e1a, "Trick Book")
    }

    /// `pausedraw.rs` (`kPauseTrickChapter` 0xe64: `"Chapter %d"`).
    pub fn chapter(&self, ch: usize) -> String {
        fmt_d(&self.or(0x0e64, "Chapter %d"), ch as i32)
    }

    /// `kTrickBookChapterCompleted` 0xc9e (0x246b50: `"COMPLETED"`).
    pub fn chapter_completed(&self) -> String {
        self.or(0x0c9e, "COMPLETED")
    }

    /// `kTrickBookCompleted` 0x22a (0x1b5a90: `"Trick Book Completed - 1 new board"`).
    pub fn trick_book_completed(&self) -> String {
        self.or(0x022a, "Trick Book Completed - 1 new board")
    }

    // --- Unlocked Overlay (`cUnlockedOverlay` 0x1b58c0, `Results_AddAwardLine` 0x1b5bf8, `Reset` 0x1b5ef8, `DrawText` 0x1b65f0) ---

    /// `cUnlockedOverlay::DrawText` 0x1b65f0 (`0xdf6` `"CONGRATULATIONS, "` + player name, `0xdf7` `"You have unlocked:"`).
    pub fn unlocked_header(&self, name: &str) -> String {
        format!("{}{name}\n{}", self.or(0x0df6, "CONGRATULATIONS, "), self.or(0x0df7, "You have unlocked:"))
    }

    /// `cUnlockedOverlay::DrawText` 0x1b65f0 final-circuit unlock note (`0xc9c` Race Circuit / `0xc9d` Showoff Circuit).
    pub fn unlock_final_note(&self, is_race: bool) -> String {
        if is_race {
            self.or(0x0c9c, "Congratulations!  You have beaten the Race Circuit.  You've been invited to go heli-boarding on Untracked.  Have fun and check it out anytime in Freeride mode.")
        } else {
            self.or(0x0c9d, "Congratulations!  You have beaten the Showoff Circuit.  You are invited to the juggernaut of trick venues atop a city skyscraper.  Have fun and check it out anytime in Freeride or Single Event mode.")
        }
    }

    /// `cUnlockedOverlay::Reset` 0x1b5ef8 + `Results_AddAwardLine` 0x1b5bf8 + `cUnlockedOverlay::DrawText` 0x1b65f0:
    /// aggregate `circuit.unlocks` per player slot, format award lines in the game's order `[5, 9, 8, 4, 3, 2, 1, 0, 6]`
    /// (skipping kind 7 medals and non-final kind 2 courses), and append the final-circuit note when unlocked.
    pub fn format_unlocks(
        &self,
        circuit: &tricky_game::circuit::Circuit,
        career: &tricky_game::career::Career,
        is_race: bool,
    ) -> Vec<String> {
        const SINGLE_FB: [&str; 10] = [
            "%d New Showoff Venue (%s)",
            "%d New Race Venue (%s)",
            "%d New Course (%s)",
            "%d New Character (%S)",
            "%d New Board",
            "%d New Outfit (Trick Chapter %d completed)",
            "%d New Rider Ranking",
            "Top %d Record Times",
            "Trick Book Completed - 1 new board",
            "%d Entry In Trick Book",
        ];
        const PLURAL_FB: [&str; 10] = [
            "%d More Showoff Venues!",
            "%d More Race Venues!",
            "%d More Courses!",
            "%d More Characters!",
            "%d More Boards!",
            "%d More Outfits!",
            "%d New Rider Rankings",
            "Top %d Record Scores",
            "Trick Book Completed - 1 new board",
            "%d Entries In Trick Book",
        ];

        let mut counts = [[0i32; 10]; 4];
        let mut values = [[0i32; 10]; 4];
        let mut flagged = [false; 4];
        for u in &circuit.unlocks {
            let k = u.kind as usize;
            if k >= 10 { continue; }
            let slot = career.find_player_slot(u.who as i8);
            let p = if slot >= 0 && (slot as usize) < 4 { slot as usize } else { 0 };
            if k == 9 {
                counts[p][9] = u.value;
            } else {
                counts[p][k] += 1;
            }
            values[p][k] = u.value;
            flagged[p] = true;
        }

        let course_label = |course_no: u32| -> String {
            let fb = tricky_game::courses::COURSES
                .iter()
                .find(|c| c.number == course_no)
                .map_or("course", |c| c.name);
            self.course_name(course_no, fb)
        };

        let mut lines = Vec::new();
        for p in 0..4usize {
            if !flagged[p] { continue; }
            for kind in [5usize, 9, 8, 4, 3, 2, 1, 0, 6] {
                let n = counts[p][kind];
                if n <= 0 { continue; }
                let val = values[p][kind];
                let line = if kind == 8 {
                    self.trick_book_completed()
                } else if n >= 2 {
                    fmt_d(&self.or(UNLOCK_PLURAL_IDS[kind], PLURAL_FB[kind]), n)
                } else {
                    let tmpl = self.or(UNLOCK_SINGLE_IDS[kind], SINGLE_FB[kind]);
                    match kind {
                        0 | 1 => fmt_d_str(&tmpl, n, &course_label(val as u32)),
                        2 => {
                            if val != 6 && val != 7 { continue; }
                            fmt_d_str(&tmpl, n, &course_label(val as u32))
                        }
                        3 => {
                            let rider = tricky_game::rivals::NAMES
                                .get(val as usize)
                                .copied()
                                .unwrap_or("Rider");
                            fmt_d_str(&tmpl, n, rider)
                        }
                        5 => {
                            let pid = career.player_id(p);
                            let pages = (0..5u32)
                                .filter(|&pg| career.tricks_done_in_page(pid, pg) == 5)
                                .count() as i32;
                            fmt_dd(&tmpl, n, if pages > 0 { pages } else { val })
                        }
                        _ => fmt_d(&tmpl, n),
                    }
                };
                lines.push(line);
            }
        }
        if is_race && circuit.race_final != 0 {
            lines.push(self.unlock_final_note(true));
        } else if !is_race && circuit.showoff_final != 0 {
            lines.push(self.unlock_final_note(false));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallbacks_without_disc_are_non_empty() {
        let t = GameText::empty();
        assert!(!t.is_loaded());
        assert_eq!(t.ordinal(0), "1st");
        assert_eq!(t.ordinal(1), "2nd");
        assert_eq!(t.ordinal(2), "3rd");
        assert_eq!(t.ordinal(3), "4th");
        assert_eq!(t.laps_left(1), "FINAL LAP");
        assert_eq!(t.laps_left(3), "3 LAPS TO GO");
        assert_eq!(t.time_bonus(5), "TIME BONUS  5 seconds");
        assert_eq!(t.status_name(240), "Master");
        assert_eq!(t.status_name(0), "Newbie");
        assert_eq!(t.go(), "GO!");
        assert_eq!(t.finish(), "FINISH");
        assert_eq!(t.time_up(), "TIME UP");
    }

    #[test]
    fn disc_american_loc_matches_all_game_caller_ids() {
        let Ok(iso) = std::env::var("TRICKY_DISC") else { return };
        let mut disc = tricky_data::disc::Disc::open(&iso).expect("open disc");
        let raw = disc.read_path("/DATA/LANG/AMERICAN.LOC").expect("read loc").expect("american.loc");
        let t = GameText::from_file(LocFile::new(raw));
        assert!(t.is_loaded());

        // HUD_DrawPlayerText 0x1a4f70 & HUD_DrawPlayerSprites 0x1a2c40
        assert_eq!(t.ordinal(0), "1st");
        assert_eq!(t.ordinal(1), "2nd");
        assert_eq!(t.ordinal(2), "3rd");
        assert_eq!(t.ordinal(3), "4th");
        assert_eq!(t.ordinal(5), "6th");
        assert_eq!(t.lap(), "LAP");
        assert_eq!(t.laps_left(1), "FINAL LAP");
        assert_eq!(t.laps_left(3), "3 LAPS TO GO");
        assert_eq!(t.checkpoint(), "CHECKPOINT");
        assert_eq!(t.time_bonus(10), "TIME BONUS  10 seconds");
        assert_eq!(t.knockdown(), "KNOCKDOWN!");
        assert_eq!(t.uber_trick(), "UBER TRICK");
        assert_eq!(t.showoff_goal(Some(2), Some(50000)), "BRONZE 50000");
        assert_eq!(t.showoff_goal(Some(1), Some(100000)), "FOR SILVER 100000");
        assert_eq!(t.showoff_goal(Some(0), Some(150000)), "FOR GOLD 150000");
        assert_eq!(t.showoff_goal(None, None), "GOLD MEDAL");
        assert_eq!(t.medal_caps(0), "GOLD");
        assert_eq!(t.medal_caps(1), "SILVER");
        assert_eq!(t.medal_caps(2), "BRONZE");
        assert_eq!(t.medal_lower(0), "gold");
        assert_eq!(t.medal_lower(1), "silver");
        assert_eq!(t.medal_lower(2), "bronze");

        // Overlays: cCountdownOverlay::Draw 0x1ae4b0, cPopFinishOverlay::DrawText 0x1aeb78
        assert_eq!(t.go(), "GO!");
        assert_eq!(t.finish(), "FINISH");
        assert_eq!(t.time_up(), "TIME UP");

        // PauseMenu_BuildMainPage 0x1ac678
        assert_eq!(t.pause_title(), "Pause");
        assert_eq!(t.pause_continue(), "continue");
        assert_eq!(t.pause_restart(), "restart");
        assert_eq!(t.pause_quit(), "quit");

        // Course_GetNameStringId 0x16b650
        assert_eq!(t.course_name_by_folder("gari"), "garibaldi");
        assert_eq!(t.course_name_by_folder("snowdream"), "snowdream");
        assert_eq!(t.course_name_by_folder("elysium"), "elysium alps");
        assert_eq!(t.course_name_by_folder("mesablanca"), "mesablanca");
        assert_eq!(t.course_name_by_folder("merqury"), "merqury city");
        assert_eq!(t.course_name_by_folder("aloha"), "aloha ice jam");
        assert_eq!(t.course_name_by_folder("pipedream"), "pipedream");
        assert_eq!(t.course_name_by_folder("untracked"), "untracked");
        assert_eq!(t.course_name_by_folder("megaplex"), "tokyo megaplex");
        assert_eq!(t.course_name_by_folder("alaska"), "alaska");

        // Career_GetStatusName 0x1634c0 (0x17..=0x20)
        assert_eq!(t.status_name(240), "Master");
        assert_eq!(t.status_name(200), "Sensei");
        assert_eq!(t.status_name(160), "Superstar");
        assert_eq!(t.status_name(120), "Champ");
        assert_eq!(t.status_name(90), "Veteran");
        assert_eq!(t.status_name(60), "Star");
        assert_eq!(t.status_name(35), "Natural");
        assert_eq!(t.status_name(20), "Contender");
        assert_eq!(t.status_name(5), "Rookie");
        assert_eq!(t.status_name(0), "Newbie");

        // Front end & board/outfit lock hints
        assert_eq!(t.stat_labels(), ["EDGING:", "SPEED:", "STABILITY:", "TRICKS:"]);
        assert_eq!(t.points_label(), "Points");
        assert_eq!(t.event_race(), "Race");
        assert_eq!(t.event_showoff(), "Showoff");
        assert_eq!(t.race_record(), "Race Record");
        assert_eq!(t.showoff_record(), "Showoff Record");
        assert_eq!(t.best_score(12345), "Current Best Score: 12345");
        assert_eq!(t.locked(), "Not Available");
        assert_eq!(t.locked_board_hint(2).as_deref(), Some("You must attain Rookie rank to unlock this board."));
        assert_eq!(t.locked_board_hint(11).as_deref(), Some("You must complete this rider's Trick Book to unlock this board."));
        assert_eq!(t.locked_outfit_hint(1).as_deref(), Some("Complete all Chapter 1 tricks in Trick Book to unlock this outfit."));
        assert_eq!(t.locked_outfit_hint(6).as_deref(), Some("Obtain a MASTER ranking to unlock the Chrome outfit."));

        // Results: cResultsOverlay 0x1af66c, Results_BuildCareerPage 0x1af950, Results_BuildMedalPage 0x1b0bb8
        assert_eq!(t.race_round_title(0), "Quarter Final Results");
        assert_eq!(t.race_round_title(1), "Semi Final Results");
        assert_eq!(t.race_round_title(2), "Final Results");
        assert_eq!(t.showoff_results_title(), "Showoff Results");
        assert_eq!(t.race_qualified(0), "CONGRATULATIONS!  You've qualified for the semi final round.");
        assert_eq!(t.race_qualified(1), "CONGRATULATIONS!  You've qualified for the final round.");
        assert_eq!(t.race_eliminated(), "SORRY!  You failed to qualify for the next round.");
        assert_eq!(t.won_no_medal(), "You didn't win  ....");
        assert_eq!(t.dnf(), "DNF");
        assert_eq!(t.score_label(), "score");
        assert_eq!(t.time_label(), "time");
        assert_eq!(t.opt_continue(), "continue");
        assert_eq!(t.opt_restart(), "restart");
        assert_eq!(t.opt_quit(), "quit");

        // Trick book
        assert_eq!(t.trick_book(), "Trick Book");
        assert_eq!(t.chapter(3), "Chapter 3");
        assert_eq!(t.chapter_completed(), "COMPLETED");
        assert_eq!(t.trick_book_completed(), "Trick Book Completed - 1 new board");

        // cUnlockedOverlay 0x1b58c0 / 0x1b5bf8 / 0x1b5ef8 / 0x1b65f0
        assert_eq!(t.unlocked_header("Mac"), "CONGRATULATIONS, Mac\nYou have unlocked:");
        let mut career = tricky_game::career::Career::default();
        let mut streams = tricky_game::rand::Streams::default();
        career.init_new_game(0, &mut streams, None);
        career.assign_player_rider(0, 0, 3, &tricky_game::cheats::Cheats::default());
        let mut ci = tricky_game::circuit::Circuit::default();
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 7, value: 0 }); // medal: skipped on unlock list
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 2, value: 1 }); // non-final course: skipped
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 1, value: 1 }); // snowdream race venue
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 3, value: 10 }); // Brodi
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 4, value: 2 }); // board
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 6, value: 8 }); // ranking
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 9, value: 2 }); // 2 trick book entries
        let lines = t.format_unlocks(&ci, &career, true);
        assert_eq!(lines, vec![
            "2 Entries In Trick Book",
            "1 New Board",
            "1 New Character (Brodi)",
            "1 New Race Venue (snowdream)",
            "1 New Rider Ranking",
        ]);
        ci.unlocks.clear();
        ci.unlocks.push(tricky_game::circuit::Unlock { who: 0, kind: 2, value: 7 }); // Untracked
        ci.race_final = 1;
        let fin = t.format_unlocks(&ci, &career, true);
        assert_eq!(fin.len(), 2);
        assert_eq!(fin[0], "1 New Course (untracked)");
        assert!(fin[1].contains("Untracked"));
    }
}
