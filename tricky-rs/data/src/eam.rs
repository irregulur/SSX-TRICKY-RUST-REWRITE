//! EA MIDI (`.eam`) sequence files (board row E1b2g7d3b7b1), played by EA's sound library
//! (`SndMidi_Play` 0x2d4440 / `SndMidi_RunEvents` 0x2d3f00, `SLUS_203.26`).
//!
//! Used by the crowd boxes (`DATA/CONFIG/CROWD.INF`: `Cheer10..30.eam`, `Ahh10..30.eam`,
//! `Anticipation10..30.eam` in `DATA/AUDIO/AUDIO.BIG`) on `Crowd.bnk`.
//!
//! Layout (proven from `SndMidi_Play` 0x2d4440 and `SndMidi_RunEvents` 0x2d3f00):
//! - Header:
//!   - `"MD"` (LE `u16` `0x444d`): 8-byte header, event stream starts at byte 8.
//!   - `"MIDx"` (LE `u32` `0x7844494d`): 4-byte header, event stream starts at byte 4.
//!   - Otherwise: raw event stream starting at byte 0.
//! - Event stream: packed 3-byte or 4-byte records:
//!   `[delta: u8, status: u8, data1: u8, (data2: u8)]`
//!   where `delta` is the **post-delay** in MIDI ticks (at 100 Hz when playback tempo is `0x1000` / 1.0x)
//!   to wait *after* executing this event before running the next event.
//!   - `0x80..=0x8f` Note Off (4 bytes): `data1 = note`, `data2 = velocity` (consumed and ignored).
//!   - `0x90..=0x9f` Note On (4 bytes): `data1 = note`, `data2 = velocity`.
//!   - `0xb0..=0xbf` Control Change (4 bytes): `data1 = controller`, `data2 = value`
//!     (`7` volume, `10` pan, `20` loop start, `30` loop end / count).
//!   - `0xc0..=0xcf` Program Change (3 bytes): `data1 = program`.
//!   - `0xe0..=0xef` Pitch Bend (4 bytes): `data1 = lsb`, `data2 = msb`.
//!   - `0xf0..=0xff` with `data1 == 0x2f` (`'/'`) (3 bytes): End of Track (waits for active voices, then stops).
//!   - Any other status (3 bytes, e.g. `0xff 0xf8` delay extension): no-op that waits `delta` ticks.

/// Container header recognized by `SndMidi_Play` 0x2d4440.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Header {
    /// 4-byte `"MIDx"` (`0x7844494d` LE) prefix.
    Midx,
    /// 8-byte `"MD"` (`0x444d` LE u16) prefix.
    Md,
    /// Unprefixed raw event stream.
    Raw,
}

/// Decoded MIDI event as dispatched by `SndMidi_RunEvents` 0x2d3f00.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// `0x80 | channel`: release voice matching `(channel, note)`.
    NoteOff { channel: u8, note: u8, velocity: u8 },
    /// `0x90 | channel`: start voice on `channel` with `note` and `velocity`.
    NoteOn { channel: u8, note: u8, velocity: u8 },
    /// `0xb0 | channel`: controller (`7` volume, `10` pan, `0x14` loop start, `0x1e` loop end).
    ControlChange { channel: u8, controller: u8, value: u8 },
    /// `0xc0 | channel`: select bank program for `channel` (3-byte event).
    ProgramChange { channel: u8, program: u8 },
    /// `0xe0 | channel`: 14-bit pitch bend (`lsb`, `msb`).
    PitchBend { channel: u8, lsb: u8, msb: u8 },
    /// `0xf0..=0xff` with `data1 == 0x2f` (`'/'`): stop once all voices finish.
    EndOfTrack,
    /// Any other 3-byte event (notably `status = 0xff, data1 = 0xf8` for >255-tick waits).
    Wait { status: u8, data1: u8 },
}

/// One event record in an `.eam` stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Post-delay in MIDI ticks (100 Hz at default speed) after this event executes.
    pub delta: u8,
    pub kind: EventKind,
}

/// Parsed `.eam` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eam {
    pub header: Header,
    pub payload_offset: usize,
    pub events: Vec<Event>,
}

/// Detect the `.eam` header and return `(header, payload_offset)` exactly as `SndMidi_Play` 0x2d4440 does.
pub fn header_of(bytes: &[u8]) -> Option<(Header, usize)> {
    if bytes.len() >= 2 && u16::from_le_bytes([bytes[0], bytes[1]]) == 0x444d {
        (bytes.len() >= 8).then_some((Header::Md, 8))
    } else if bytes.len() >= 4 && u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 0x7844_494d {
        Some((Header::Midx, 4))
    } else {
        Some((Header::Raw, 0))
    }
}

/// Return the raw event-stream slice after skipping the `"MD"` or `"MIDx"` header.
pub fn payload(bytes: &[u8]) -> Option<&[u8]> {
    let (_, off) = header_of(bytes)?;
    Some(&bytes[off..])
}

/// Parse an `.eam` sequence up through its first `EndOfTrack` (or end of buffer).
pub fn parse(bytes: &[u8]) -> Option<Eam> {
    let (header, payload_offset) = header_of(bytes)?;
    let s = &bytes[payload_offset..];
    let mut i = 0;
    let mut events = Vec::new();
    while i + 3 <= s.len() {
        let delta = s[i];
        let status = s[i + 1];
        let data1 = s[i + 2];
        i += 3;
        let cmd = status & 0xf0;
        let channel = status & 0x0f;
        let kind = match cmd {
            0x80 => {
                let velocity = *s.get(i)?;
                i += 1;
                EventKind::NoteOff { channel, note: data1, velocity }
            }
            0x90 => {
                let velocity = *s.get(i)?;
                i += 1;
                EventKind::NoteOn { channel, note: data1, velocity }
            }
            0xb0 => {
                let value = *s.get(i)?;
                i += 1;
                EventKind::ControlChange { channel, controller: data1, value }
            }
            0xc0 => EventKind::ProgramChange { channel, program: data1 },
            0xe0 => {
                let msb = *s.get(i)?;
                i += 1;
                EventKind::PitchBend { channel, lsb: data1, msb }
            }
            0xf0 if data1 == 0x2f => {
                events.push(Event { delta, kind: EventKind::EndOfTrack });
                break;
            }
            _ => EventKind::Wait { status, data1 },
        };
        events.push(Event { delta, kind });
    }
    Some(Eam { header, payload_offset, events })
}

impl Eam {
    /// Total ticks summed across the linear event stream (ignoring loop jumps).
    pub fn linear_ticks(&self) -> u32 {
        self.events.iter().map(|e| e.delta as u32).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_midx_md_and_raw_headers() {
        let midx = [
            b'M', b'I', b'D', b'x',
            1, 0xc0, 0x12,
            0, 0xb0, 0x0a, 0x40,
            2, 0xb0, 0x07, 0x7f,
            10, 0x90, 0x3c, 0x64,
            255, 0xff, 0xf8,
            0, 0xe0, 0x00, 0x40,
            0, 0x80, 0x3c, 0x00,
            0, 0xff, 0x2f,
            0x00, // trailing padding byte
        ];
        let e = parse(&midx).unwrap();
        assert_eq!(e.header, Header::Midx);
        assert_eq!(e.payload_offset, 4);
        assert_eq!(e.events, vec![
            Event { delta: 1, kind: EventKind::ProgramChange { channel: 0, program: 0x12 } },
            Event { delta: 0, kind: EventKind::ControlChange { channel: 0, controller: 10, value: 64 } },
            Event { delta: 2, kind: EventKind::ControlChange { channel: 0, controller: 7, value: 127 } },
            Event { delta: 10, kind: EventKind::NoteOn { channel: 0, note: 60, velocity: 100 } },
            Event { delta: 255, kind: EventKind::Wait { status: 0xff, data1: 0xf8 } },
            Event { delta: 0, kind: EventKind::PitchBend { channel: 0, lsb: 0, msb: 64 } },
            Event { delta: 0, kind: EventKind::NoteOff { channel: 0, note: 60, velocity: 0 } },
            Event { delta: 0, kind: EventKind::EndOfTrack },
        ]);
        assert_eq!(e.linear_ticks(), 1 + 2 + 10 + 255);

        let md = [b'M', b'D', 0, 0, 0, 0, 0, 0, 0, 0xff, 0x2f];
        let e = parse(&md).unwrap();
        assert_eq!(e.header, Header::Md);
        assert_eq!(e.payload_offset, 8);
        assert_eq!(e.events, vec![Event { delta: 0, kind: EventKind::EndOfTrack }]);
        assert!(parse(&[b'M', b'D', 0, 0]).is_none());

        let raw = [0, 0xc3, 5, 0, 0xff, 0x2f];
        let e = parse(&raw).unwrap();
        assert_eq!(e.header, Header::Raw);
        assert_eq!(e.payload_offset, 0);
        assert_eq!(e.events.len(), 2);
    }

    #[test]
    fn parse_all_disc_eam_files() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let disc = crate::disc::Disc::open(&disc_path).expect("open disc");
        let mut files = crate::big::Files::new(disc);
        files.mount_game_archives().expect("mount");
        let inf = files.load("data/config/crowd.inf").expect("load").expect("crowd.inf missing");
        let (sets, _) = crate::audiocfg::crowd_config(&inf, b"GARI");
        let mut names = std::collections::BTreeSet::new();
        for set in &sets {
            for list in [&set.trick, &set.fall, &set.anticipate] {
                for raw in list {
                    names.insert(String::from_utf8_lossy(raw).into_owned());
                }
            }
        }
        assert_eq!(names.len(), 9, "expected 9 crowd .eam files in CROWD.INF: {names:?}");
        for path in &names {
            let bytes = files.load(path).expect("load eam").unwrap_or_else(|| panic!("missing {path}"));
            let eam = parse(&bytes).unwrap_or_else(|| panic!("failed to parse {path}"));
            assert_eq!(eam.header, Header::Midx, "{path}");
            assert_eq!(eam.events.last().map(|e| e.kind), Some(EventKind::EndOfTrack), "{path}");
            assert!(
                eam.events.iter().any(|e| matches!(e.kind, EventKind::ProgramChange { .. })),
                "{path} missing program change"
            );
            assert!(
                eam.events.iter().any(|e| matches!(e.kind, EventKind::NoteOn { .. })),
                "{path} missing note on"
            );
            assert!(
                eam.events.iter().any(|e| matches!(e.kind, EventKind::NoteOff { .. })),
                "{path} missing note off"
            );
        }
    }
}
