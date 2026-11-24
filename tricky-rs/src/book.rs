//! The trick book on the career block (`Score_TrickBookCheck` 0x1579e0, `Trick_GetInfo` 0x154638 / `0x31e210`,
//! `TrickDef_Load` 0x154580, row C15b): six chapters of five tricks per rider read from the player's disc
//! (`TRICKY_DISC`, `DATA/TUTORIAL/TRICKDEF.DAT`), checked directly against `Career::trick_book_check` with a
//! `BookEnv` over the race, score message (`Scoring_PushMessage(1.2, score, 0x11, 0, 0)` `0x157330`), and
//! `Sfx_OnTrickBookDone` (`0x2190f0`). Progress is kept in the career block (`rec + 0x14`: `+4` committed bits,
//! `+8` pending/forced bits, `+0x310c` this-race bits), saved in `tricky-career.sav` instead of `tricky-book.json`.
//!
//! When `TRICKY_DISC` is absent, falls back to exported `chars/trickbook.json` name matching (STANDIN).

use bevy::prelude::*;
use std::collections::HashMap;
use tricky_game::career::{BookEnv, Career};

/// Result of running `Career::trick_book_check` (`0x1579e0`) for one landed trick id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BookCheckOut {
    /// Names of any trick book entries newly ticked off on the current unfinished page.
    pub ticked_names: Vec<String>,
    /// 1-based chapter number (`1..=6`) if this trick completed the active chapter.
    pub finished_chapter: Option<usize>,
    /// True when `BookEnv::book_done_message` fired (`Scoring_PushMessage(..., 0x11, ...)` + `Sfx_OnTrickBookDone`).
    pub book_done: bool,
    /// True when a tutorial lesson target trick matched (`BookEnv::lesson_done`).
    pub lesson_done: bool,
}

/// The `Trick_GetInfo` (`0x31e210`) table (`DATA/TUTORIAL/TRICKDEF.DAT`: 12 riders × 30 `TrickDef`) plus
/// optional name-only fallback when `TRICKY_DISC` is absent.
#[derive(Resource, Default)]
pub struct Book {
    /// 12 riders × 30 `TrickDef` records from `DATA/TUTORIAL/TRICKDEF.DAT` (`Trick_GetInfo` `0x154638`).
    pub defs: Option<Vec<Vec<tricky_data::trickdef::TrickDef>>>,
    /// Fallback trick names when `TRICKY_DISC` is absent (STANDIN: `chars/trickbook.json`).
    pub tricks: HashMap<String, Vec<String>>,
    /// Fallback bitmask when `TRICKY_DISC` is absent (STANDIN: in-memory fallback when no `TRICKDEF.DAT`).
    pub fallback_done: HashMap<String, u32>,
}

/// The part of a trick name the fallback matcher compares when `TRICKDEF.DAT` is absent.
pub fn key(name: &str) -> String {
    let name = name.split("  +").next().unwrap_or(name);
    let mut w: Vec<&str> = name.split_whitespace().filter(|t| *t != "FS" && *t != "BS").collect();
    while matches!(w.first(), Some(&"Late") | Some(&"Switch")) {
        w.remove(0);
    }
    if w.len() >= 2 && w[0] == "Rail" && w[1] == "To" {
        w.drain(0..2);
    }
    loop {
        let n = w.len();
        if n >= 1 && w[n - 1] == "Air" {
            w.pop();
            continue;
        }
        if n >= 2 && w[n - 2] == "To" && (w[n - 1] == "Fakie" || w[n - 1] == "Rail") {
            w.truncate(n - 2);
            continue;
        }
        break;
    }
    w.join(" ").to_ascii_lowercase()
}

struct RaceBookEnv<'a> {
    book: &'a Book,
    score: &'a mut tricky_game::score::Score,
    mode_allows: bool,
    player: i8,
    lesson_trick: Option<u32>,
    lesson_done: bool,
    book_done: bool,
}

impl BookEnv for RaceBookEnv<'_> {
    fn mode_allows(&self) -> bool {
        self.mode_allows
    }
    fn player(&self) -> i8 {
        self.player
    }
    fn lesson_trick(&self) -> Option<u32> {
        self.lesson_trick
    }
    fn trick_info(&self, rider: u32, t: u32) -> [u32; 2] {
        self.book.trick_info(rider, t)
    }
    fn lesson_done(&mut self) {
        self.lesson_done = true;
    }
    fn book_done_message(&mut self) {
        // `Scoring_PushMessage(1.2, score, 0x11, 0, 0)` 0x157330
        self.score.push_message(1.2f32.to_bits(), 0x11, 0, 0);
        self.book_done = true;
    }
}

impl Book {
    /// Load `DATA/TUTORIAL/TRICKDEF.DAT` directly from `TRICKY_DISC` (`TrickDef_Load` 0x154580), or fall back
    /// to `chars/trickbook.json` when no disc is mounted.
    pub fn load(chars_dir: &std::path::Path) -> Self {
        if let Some(defs) = std::env::var("TRICKY_DISC").ok().and_then(|p| {
            let disc = tricky_data::disc::Disc::open(&p).map_err(|e| eprintln!("disc {p}: {e}")).ok()?;
            let mut files = tricky_data::big::Files::new(disc);
            let bytes = files.load("data/tutorial/trickdef.dat").ok()??;
            tricky_data::trickdef::read(&bytes).map_err(|e| eprintln!("{e}")).ok()
        }) {
            return Self::from_defs(defs);
        }
        // STANDIN: exported `chars/trickbook.json` fallback when `TRICKY_DISC` is absent.
        let mut b = Book::default();
        if let Ok(text) = std::fs::read_to_string(chars_dir.join("trickbook.json")) {
            if let Ok(v) = serde_json::from_str::<HashMap<String, Vec<serde_json::Value>>>(&text) {
                for (set, list) in v {
                    let who = set.split('_').nth(1).unwrap_or(&set).to_ascii_lowercase();
                    b.tricks.insert(who, list.iter().filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from)).collect());
                }
            }
        }
        b
    }

    pub fn from_defs(defs: Vec<Vec<tricky_data::trickdef::TrickDef>>) -> Self {
        let mut tricks = HashMap::new();
        for (r, list) in defs.iter().enumerate() {
            if let Some(&name) = tricky_data::trickdef::RIDERS.get(r) {
                tricks.insert(name.to_string(), list.iter().map(|t| t.name().trim().to_string()).collect());
            }
        }
        Self { defs: Some(defs), tricks, fallback_done: HashMap::new() }
    }

    /// `Trick_GetInfo` (`0x154638`, table `0x31e210`): the two id/flags words of trick `t` (`0..30`) for `rider` (`0..12`).
    pub fn trick_info(&self, rider: u32, t: u32) -> [u32; 2] {
        self.defs
            .as_ref()
            .and_then(|d| d.get(rider as usize))
            .and_then(|r| r.get(t as usize))
            .map_or([0, 0], |td| td.words)
    }

    /// The character's completed + pending trick-book bitmask from the career block (`rec + 0x14`: `+4 | +8`).
    pub fn bits_in_career(career: &Career, ch: u32) -> u32 {
        let rec = 0x100 + 0x1c4 * ch.min(11) as usize;
        let book = rec + 0x14;
        career.r32(book + 4) | career.r32(book + 8)
    }

    /// The first chapter not yet finished (`0..6`, `6` when the book is complete) in the career block.
    pub fn chapter_in_career(career: &Career, ch: u32) -> usize {
        let bits = Self::bits_in_career(career, ch);
        (0..6).find(|c| (bits >> (c * 5)) & 31 != 31).unwrap_or(6)
    }

    /// Whether all 6 chapters (30 tricks) are complete for character `who` in `career`.
    pub fn complete_in_career(&self, career: &Career, who: &str) -> bool {
        if self.defs.is_some() {
            let ch = crate::ui::CareerRes::character(who);
            Self::chapter_in_career(career, ch) == 6
        } else {
            let bits = self.fallback_done.get(&who.to_ascii_lowercase()).copied().unwrap_or(0);
            (0..6).all(|c| (bits >> (c * 5)) & 31 == 31)
        }
    }

    /// `Score_TrickBookCheck` (`0x1579e0`) over `career` + `score`, using `TRICKDEF.DAT` (`Trick_GetInfo` `0x154638`)
    /// and the lesson/replay gates (`mode_allows`, `lesson_trick`).
    pub fn check_career(
        &self,
        career: &mut Career,
        score: &mut tricky_game::score::Score,
        id: [u32; 2],
        mode_allows: bool,
        lesson_trick: Option<u32>,
    ) -> BookCheckOut {
        if self.defs.is_none() {
            return BookCheckOut::default();
        }
        let ch = if career.r32(0x3114) != 0 {
            ((career.ptr(0x3114) - 0x100) / 0x1c4) as u32
        } else {
            return BookCheckOut::default();
        };
        let before = Self::bits_in_career(career, ch);
        let page_before = Self::chapter_in_career(career, ch);
        let (book_done, lesson_done) = {
            let mut env = RaceBookEnv {
                book: self,
                score,
                mode_allows,
                player: 0,
                lesson_trick,
                lesson_done: false,
                book_done: false,
            };
            career.trick_book_check(&mut env, id);
            (env.book_done, env.lesson_done)
        };
        let after = Self::bits_in_career(career, ch);
        let newly = after & !before;
        let mut ticked_names = Vec::new();
        if newly != 0 {
            if let Some(list) = self.defs.as_ref().and_then(|d| d.get(ch as usize)) {
                for t in 0..30usize {
                    if (newly >> t) & 1 != 0 {
                        if let Some(td) = list.get(t) {
                            ticked_names.push(td.name().trim().to_string());
                        }
                    }
                }
            }
        }
        let finished_chapter = (newly != 0
            && page_before < 6
            && ((after >> (page_before * 5)) & 31 == 31))
            .then_some(page_before + 1);
        BookCheckOut { ticked_names, finished_chapter, book_done, lesson_done }
    }

    /// STANDIN name-based fallback when `TRICKY_DISC` is absent (no `TRICKDEF.DAT`).
    pub fn check_fallback(&mut self, who: &str, trick: &str) -> Option<(String, Option<usize>)> {
        let who = who.to_ascii_lowercase();
        let list = self.tricks.get(&who)?;
        let bits = self.fallback_done.get(&who).copied().unwrap_or(0);
        let c = (0..6).find(|c| (bits >> (c * 5)) & 31 != 31).unwrap_or(6);
        if c >= 6 {
            return None;
        }
        let k = key(trick);
        let t = (c * 5..c * 5 + 5).find(|t| bits & (1 << t) == 0 && list.get(*t).is_some_and(|n| key(n) == k))?;
        let name = list[t].clone();
        let bits = bits | (1 << t);
        self.fallback_done.insert(who, bits);
        let finished = ((bits >> (c * 5)) & 31 == 31).then_some(c + 1);
        Some((name, finished))
    }

    /// The menu's line: the chapter, how far through it, and the next trick to do, using the game's strings
    /// (`Loc_GetString` 0x2c5160: `0x22a` Trick Book Completed, `0xe1a` Trick Book, `0xe64` Chapter %d).
    pub fn summary(&self, career: &Career, who: &str, text: &crate::loc::GameText) -> String {
        let key_name = who.to_ascii_lowercase();
        let Some(list) = self.tricks.get(&key_name) else {
            return String::new();
        };
        let bits = if self.defs.is_some() {
            Self::bits_in_career(career, crate::ui::CareerRes::character(who))
        } else {
            self.fallback_done.get(&key_name).copied().unwrap_or(0)
        };
        let c = (0..6).find(|c| (bits >> (c * 5)) & 31 != 31).unwrap_or(6);
        if c >= 6 {
            return text.trick_book_completed();
        }
        let n = (0..5).filter(|i| bits & (1 << (c * 5 + i)) != 0).count();
        let next = (c * 5..c * 5 + 5)
            .find(|t| bits & (1 << t) == 0)
            .and_then(|t| list.get(t))
            .cloned()
            .unwrap_or_default();
        format!("{} {}: {n}/5   {next}", text.trick_book(), text.chapter(c + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(key("BS 360 Mute"), key("FS 360 Mute  +470"));
        assert_eq!(key("Indy Air"), key("Indy"));
        assert_eq!(key("BS 900 Tail WAG To Fakie"), key("Switch FS 900 Tail WAG"));
    }

    #[test]
    fn career_trick_book_check_uses_trickdef_and_lesson_gate() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
        let mut files = tricky_data::big::Files::new(disc);
        let bytes = files.load("data/tutorial/trickdef.dat").expect("load").expect("trickdef.dat");
        let defs = tricky_data::trickdef::read(&bytes).expect("parse trickdef.dat");
        let book = Book::from_defs(defs);

        let (mut career, _) = tricky_game::career::CareerFile::open(None, 0, 1, None);
        for k in 0..4 {
            career.clear_player_slot(k);
        }
        career.assign_player_rider(0, 0, 0, &tricky_game::cheats::Cheats::default()); // Eddie = 0
        let mut sc = tricky_game::rider::new_score();

        // Lesson gate: with lesson_trick = Some(0), matching trick 0 reports lesson_done without ticking career bits.
        let t0 = book.trick_info(0, 0);
        let out = book.check_career(&mut career, &mut sc, t0, true, Some(0));
        assert!(out.lesson_done && !out.book_done && out.ticked_names.is_empty());
        assert_eq!(Book::bits_in_career(&career, 0), 0);

        // Mode gate: mode_allows = false does nothing.
        let out = book.check_career(&mut career, &mut sc, t0, false, None);
        assert!(!out.book_done && Book::bits_in_career(&career, 0) == 0);

        // Chapter 2 trick 5 while chapter 1 is incomplete is ignored.
        let t5 = book.trick_info(0, 5);
        let out = book.check_career(&mut career, &mut sc, t5, true, None);
        assert!(!out.book_done && Book::bits_in_career(&career, 0) == 0);

        // Landing all 5 tricks of chapter 1 ticks bits 0..5, pushes score msg 0x11, and finishes chapter 1.
        for i in 0..5u32 {
            let ti = book.trick_info(0, i);
            let out = book.check_career(&mut career, &mut sc, ti, true, None);
            assert!(out.book_done, "chapter 1 trick {i} should tick");
            assert!(!out.ticked_names.is_empty());
            if i == 4 {
                assert_eq!(out.finished_chapter, Some(1));
            }
        }
        assert_eq!(Book::bits_in_career(&career, 0) & 31, 31);
        assert_eq!(Book::chapter_in_career(&career, 0), 1);
        assert!(sc.timed.iter().any(|m| m.kind == 0x11));

        // Now chapter 2 trick 5 ticks cleanly!
        let out = book.check_career(&mut career, &mut sc, t5, true, None);
        assert!(out.book_done && (Book::bits_in_career(&career, 0) & (1 << 5)) != 0);
    }
}
