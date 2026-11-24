//! The rest of the input (board row E10b): the disc's button maps read as the game reads them
//! (`Pad_LoadButtonMap` 0x179a88 `data/config/btnmap%d.dat`, `Pad_ParseButtonMap` 0x179b18 with `Parse_FindSection`
//! 0x179d98, `Parse_SkipSpace` 0x17a148, `Parse_SkipLine` 0x17a208, the section end `FUN_0017a1a8`, newlib's
//! `sscanf` on the game's pattern, `Pad_ParseButtonNames` 0x179e60 and the pair parsers) and bound to a rider's
//! mapper as `Pad_SetupVibration` 0x152770 does; the front end's key repeat (`PadDevice_UpdateRepeat` 0x17a958);
//! the pad's rumble (`cPS2Device` vf3 / vf4 / vf5 and the motor timers at the end of its poll, vf9 0x17b270).
//! Checked in the function runner (`tests/pad.rs`).

use crate::controls::ButtonMap;

/// `Pad_ParseButtonNames`' table (0x337a90): names in the order they are tried, and their bits.
pub const BUTTON_NAMES: [(&str, u32); 47] = [
    ("NONE", 0xffffffff),
    ("ALL", 0x0),
    ("DPAD_UP", 0x1),
    ("DPAD_DOWN", 0x2),
    ("DPAD_RIGHT", 0x4),
    ("DPAD_LEFT", 0x8),
    ("LJOY_UP", 0x20000),
    ("LJOY_DOWN", 0x40000),
    ("LJOY_RIGHT", 0x80000),
    ("LJOY_LEFT", 0x100000),
    ("RJOY_UP", 0x200000),
    ("RJOY_DOWN", 0x400000),
    ("RJOY_RIGHT", 0x800000),
    ("RJOY_LEFT", 0x1000000),
    ("SQUARE", 0x10),
    ("TRIANGLE", 0x20),
    ("CIRCLE", 0x40),
    ("X", 0x800),
    ("LJOY", 0x4000),
    ("RJOY", 0x8000),
    ("START", 0x80),
    ("SELECT", 0x100),
    ("LSHIFT2", 0x1000),
    ("RSHIFT2", 0x2000),
    ("LSHIFT", 0x200),
    ("RSHIFT", 0x400),
    ("ANALOG_NULL", 0x0),
    ("ANALOG_JOY_L_X", 0x1),
    ("ANALOG_JOY_L_POS_X", 0x2),
    ("ANALOG_JOY_L_NEG_X", 0x4),
    ("ANALOG_JOY_L_Y", 0x8),
    ("ANALOG_JOY_L_POS_Y", 0x10),
    ("ANALOG_JOY_L_NEG_Y", 0x20),
    ("ANALOG_JOY_R_X", 0x40),
    ("ANALOG_JOY_R_POS_X", 0x80),
    ("ANALOG_JOY_R_NEG_X", 0x100),
    ("ANALOG_JOY_R_Y", 0x200),
    ("ANALOG_JOY_R_POS_Y", 0x400),
    ("ANALOG_JOY_R_NEG_Y", 0x800),
    ("ANALOG_PAD_X", 0x1000),
    ("ANALOG_PAD_POS_X", 0x2000),
    ("ANALOG_PAD_NEG_X", 0x4000),
    ("ANALOG_PAD_Y", 0x8000),
    ("ANALOG_PAD_POS_Y", 0x10000),
    ("ANALOG_PAD_NEG_Y", 0x20000),
    ("true", 0x1),
    ("false", 0x0),
];

/// The file the game reads for button map `n` (the rider's stats +0x6c)
pub fn button_map_path(n: u32) -> String {
    format!("data/config/btnmap{n}.dat")
}

/// A button map file as parsed (the parse context, 0x220 bytes on `Pad_SetupVibration`'s stack): +8 / +0xc /
/// +0x10 the counts, +0x14 the `IS_BOOL` pairs (pressed, not pressed), +0x84 the `IS_BOOLGRAB` pairs, +0xfc the
/// `IS_ANALOG` pairs (mask, negate).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PadConfig {
    pub bools: Vec<(u32, u32)>,
    pub grabs: Vec<(u32, u32)>,
    pub analog: Vec<(u32, u32)>,
}

/// The parse context (+0 the text, +4 what is left of it).
struct Ctx<'a> {
    text: &'a [u8],
    at: usize,
    left: i32,
}

impl Ctx<'_> {
    fn ch(&self, k: usize) -> u8 {
        *self.text.get(self.at + k).unwrap_or(&0)
    }

    /// `Parse_SkipSpace` 0x17a148: past characters below '!' (signed: bytes from 0x80 count as space) while any
    /// are left; true while some are.
    fn skip_space(&mut self) -> bool {
        let mut n = self.left;
        if n < 1 {
            return false;
        }
        let c = self.ch(0) as i8;
        if c != 0 && c < b'!' as i8 {
            loop {
                n -= 1;
                self.left = n;
                self.at += 1;
                let d = self.ch(0) as i8;
                if d == 0 {
                    break;
                }
                if (b' ' as i8) < d || n < 1 {
                    break;
                }
            }
        }
        0 < n
    }

    /// `Parse_SkipLine` 0x17a208: to the next newline and past it; true while characters are left.
    fn skip_line(&mut self) -> bool {
        let mut n = self.left;
        if n < 1 {
            return false;
        }
        if self.ch(0) != 0 {
            if self.ch(0) != b'\n' {
                loop {
                    n -= 1;
                    self.at += 1;
                    self.left = n;
                    if self.ch(0) == 0 {
                        return 0 < n;
                    }
                    if self.ch(0) == b'\n' || n < 1 {
                        break;
                    }
                }
            }
            if self.ch(0) != 0 {
                self.at += 1;
                self.left = n - 1;
                n -= 1;
            }
        }
        0 < n
    }

    /// `strncmp`(text here, s, len s) == 0
    fn starts(&self, s: &[u8]) -> bool {
        for (k, b) in s.iter().enumerate() {
            let c = self.ch(k);
            if c != *b {
                return false;
            }
            if c == 0 {
                return true;
            }
        }
        true
    }

    /// `FUN_0017a1a8`: the end of a section ("};", its line skipped) or of the text
    fn section_end(&mut self) -> bool {
        if 0 < self.left {
            if self.skip_space() {
                if !self.starts(b"};") {
                    return false;
                }
                self.skip_line();
            }
            return true;
        }
        false
    }

    /// `Parse_FindSection` 0x179d98: a line starting with the name whose next line starts with '{'; the text left
    /// after that line.
    fn find_section(&mut self, name: &[u8]) -> bool {
        loop {
            if !self.skip_space() {
                return false;
            }
            if self.starts(name) {
                if !self.skip_line() {
                    return false;
                }
                if !self.skip_space() {
                    return false;
                }
                if self.starts(b"{") {
                    return self.skip_line();
                }
            }
            if !self.skip_line() {
                return false;
            }
        }
    }
}

/// newlib's `sscanf`(text, "{%*[ \t]%*[a-zA-Z],%*[ \t](%[0-9a-zA-Z_| \t]),%*[ \t](%[0-9a-zA-Z_| \t])"): the two
/// button lists of an entry; the count of lists read (each `%[` needs a character at least).
pub fn scan_entry(text: &[u8]) -> (usize, Vec<u8>, Vec<u8>) {
    let mut p = 0usize;
    let at = |p: usize| *text.get(p).unwrap_or(&0);
    let lit = |p: &mut usize, c: u8| -> bool {
        if at(*p) == c && c != 0 {
            *p += 1;
            true
        } else {
            false
        }
    };
    let set = |p: &mut usize, ok: &dyn Fn(u8) -> bool| -> Option<Vec<u8>> {
        let s = *p;
        while at(*p) != 0 && ok(at(*p)) {
            *p += 1;
        }
        if *p == s {
            None
        } else {
            Some(text[s..*p].to_vec())
        }
    };
    let space = |c: u8| c == b' ' || c == b'\t';
    let alpha = |c: u8| c.is_ascii_alphabetic();
    let list = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'|' || c == b' ' || c == b'\t';
    let mut a = vec![];
    let mut b = vec![];
    if !lit(&mut p, b'{') || set(&mut p, &space).is_none() || set(&mut p, &alpha).is_none() || !lit(&mut p, b',') || set(&mut p, &space).is_none() || !lit(&mut p, b'(') {
        return (0, a, b);
    }
    match set(&mut p, &list) {
        Some(s) => a = s,
        None => return (0, a, b),
    }
    if !lit(&mut p, b')') || !lit(&mut p, b',') || set(&mut p, &space).is_none() || !lit(&mut p, b'(') {
        return (1, a, b);
    }
    match set(&mut p, &list) {
        Some(s) => b = s,
        None => return (1, a, b),
    }
    (2, a, b)
}

/// `Pad_ParseButtonNames` 0x179e60: the names in a list (separated by spaces, tabs and '|'), each the first in
/// the table it starts with that ends there, their bits or-ed; an unknown name ends the list.
pub fn parse_button_names(s: &[u8]) -> u32 {
    let mut bits = 0u32;
    let mut p = 0usize;
    let at = |p: usize| *s.get(p).unwrap_or(&0);
    if at(0) == 0 {
        return 0;
    }
    loop {
        let mut found = false;
        for (name, b) in BUTTON_NAMES.iter() {
            let n = name.len();
            let rest = &s[p.min(s.len())..];
            let len = rest.iter().position(|c| *c == 0).unwrap_or(rest.len());
            let matches = name.bytes().enumerate().all(|(k, c)| at(p + k) == c);
            if matches {
                let c = at(p + n.min(len));
                if c == 0 || c == b' ' || c == b'|' || c == b'\t' {
                    p += n;
                    bits |= b;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return bits;
        }
        loop {
            let c = at(p);
            if c == 0 {
                return bits;
            }
            if c != b' ' && c != b'|' && c != b'\t' {
                break;
            }
            p += 1;
        }
        if at(p) == 0 {
            return bits;
        }
    }
}

/// `Pad_ParseButtonMap` 0x179b18 on a button map file's text (`Pad_LoadButtonMap` hands it the whole file).
pub fn parse_button_map(text: &[u8]) -> PadConfig {
    let mut cfg = PadConfig::default();
    let mut c = Ctx { text, at: 0, left: text.len() as i32 };
    for (k, name) in [&b"IS_BOOL"[..], b"IS_BOOLGRAB", b"IS_ANALOG"].iter().enumerate() {
        let mut ok = c.find_section(name);
        while ok && !c.section_end() {
            if !c.skip_space() {
                if k == 2 {
                    return cfg;
                }
                break;
            }
            let (n, a, b) = scan_entry(&text[c.at.min(text.len())..]);
            if n == 2 {
                let (pa, pb) = (parse_button_names(&a), parse_button_names(&b));
                match k {
                    0 => cfg.bools.push((pa, pb)),
                    1 => cfg.grabs.push((pa, pb)),
                    _ => cfg.analog.push((pa, (pb != 0) as u32)),
                }
            }
            ok = c.skip_line();
        }
    }
    cfg
}

/// `Pad_SetupVibration` 0x152770's binding of a parsed map to a rider's mapper: the 15 grab combos to the rider's
/// trick slots (`slots`, as `ButtonMap::btnmap` takes them), the 14 buttons to actions 0x1b..0x28 (`Input_BindButton`:
/// pressed, and pressed or-ed into the looked-at set), the 16 axes (`Input_BindAxis`), and the "shown" buttons
/// (`Input_SetShownButton` 0x179850, `cInput +0x1cc`: grab pressed bits and button pressed bits; unbound slots 0).
pub fn bind(cfg: &PadConfig, slots: [u16; 15]) -> ButtonMap {
    let mut m = ButtonMap { pressed: [crate::controls::NONE; 0x29], looked_at: [crate::controls::NONE; 0x29], axes: [(0, false); 16], shown: [0; 0x29] };
    let mut button = |a: usize, pressed: u32, other: u32| {
        m.pressed[a] = pressed;
        m.looked_at[a] = other | pressed;
        m.shown[a] = pressed;
    };
    for (k, s) in slots.iter().enumerate() {
        let (p, o) = cfg.grabs.get(k).copied().unwrap_or((0, 0));
        button(*s as usize, p, o);
    }
    for k in 0..14 {
        let (p, o) = cfg.bools.get(k).copied().unwrap_or((0, 0));
        button(0x1b + k, p, o);
    }
    for k in 0..16 {
        let (mask, neg) = cfg.analog.get(k).copied().unwrap_or((0, 0));
        m.axes[k] = (mask, neg != 0);
    }
    m
}

/// The mapper's three tables as `Pad_GetConfig` 0x17a290 copies them out of a `cInput` and `Pad_SetConfig`
/// 0x17a2f8 back in (`EAMem_Copy`): +4 the pressed buttons (0xa4 bytes), +0xa8 the looked-at sets (0xa4), +0x14c the
/// axes (0x80: mask, negate). The shown buttons (+0x1cc) are not among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadTables {
    pub pressed: [u32; 0x29],
    pub looked_at: [u32; 0x29],
    pub axes: [(u32, bool); 16],
}

/// `Pad_GetConfig` 0x17a290(input, pressed, looked_at, axes): the three tables copied out
pub fn get_config(m: &ButtonMap) -> PadTables {
    PadTables { pressed: m.pressed, looked_at: m.looked_at, axes: m.axes }
}

/// `Pad_SetConfig` 0x17a2f8(input, pressed, looked_at, axes): the three tables copied in
pub fn set_config(m: &mut ButtonMap, t: &PadTables) {
    m.pressed = t.pressed;
    m.looked_at = t.looked_at;
    m.axes = t.axes;
}

/// The Trick Tutorial's two pad maps (the lesson manager's +0xdc / +0x180 / +0x224 the player's, +0x2a4 / +0x348 /
/// +0x3ec the lesson's), swapped in and out around the preview (`cLessonPreviewHandler::Enter` 0x14e138 puts the
/// lesson's in, `Exit` 0x14e2b0 the player's back, both with `Pad_SetConfig` on the player's `cInput`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LessonPads {
    pub player: PadTables,
    pub lesson: PadTables,
}

impl LessonPads {
    /// `LessonMan_Init` 0x14aaa0's pad part: the player's tables kept (`Pad_GetConfig`); the default button map
    /// (`Pad_LoadButtonMap` with 0: `map0`, `None` when it fails to load) bound over them as `Pad_SetupVibration`
    /// binds a map but with `Input_BindButton` 0x179818 / `Input_BindAxis` 0x179838 alone (the 15 grab combos to the
    /// rider's trick slots, the 14 buttons to actions 0x1b..0x28, the 16 axes; nothing shown, every other entry left
    /// as the player's); the result kept as the lesson's (`Pad_GetConfig`) and the player's put back
    /// (`Pad_SetConfig`). So the lesson always plays on the default map, whichever map the player chose.
    pub fn new(input: &mut ButtonMap, map0: Option<&PadConfig>, slots: [u16; 15]) -> LessonPads {
        let player = get_config(input);
        if let Some(cfg) = map0 {
            let mut button = |a: usize, pressed: u32, other: u32| {
                input.pressed[a] = pressed;
                input.looked_at[a] = other | pressed;
            };
            for (k, s) in slots.iter().enumerate() {
                let (p, o) = cfg.grabs.get(k).copied().unwrap_or((0, 0));
                button(*s as usize, p, o);
            }
            for k in 0..14 {
                let (p, o) = cfg.bools.get(k).copied().unwrap_or((0, 0));
                button(0x1b + k, p, o);
            }
            for k in 0..16 {
                let (mask, neg) = cfg.analog.get(k).copied().unwrap_or((0, 0));
                input.axes[k] = (mask, neg != 0);
            }
        }
        let lesson = get_config(input);
        set_config(input, &player);
        LessonPads { player, lesson }
    }

    /// The preview's swap: the lesson's tables in (its Enter) or the player's back (its Exit)
    pub fn swap(&self, input: &mut ButtonMap, to_lesson: bool) {
        set_config(input, if to_lesson { &self.lesson } else { &self.player });
    }
}

/// The front end's keys (0x3857c0): a button (or direction) and the key it gives, plain and with a shoulder held.
pub const KEYS: [(u32, u32, u32); 14] = [
    (0x100, 0x1b, 0x1b),
    (0x80, 0xd, 0xd),
    (0x10, 0x61, 0x41),
    (0x20, 0x62, 0x42),
    (0x40, 0x63, 0x43),
    (0x800, 0x64, 0x44),
    (0x100008, 0x4b00, 0x4700),
    (0x80004, 0x4d00, 0x4f00),
    (0x40002, 0x5000, 0x5100),
    (0x20001, 0x4800, 0x4900),
    (0x400, 0x72, 0x72),
    (0x2000, 0x52, 0x52),
    (0x200, 0x6c, 0x6c),
    (0x1000, 0x4c, 0x4c),
];

/// A pad's key repeat (`cPadDevice` +8 repeat on, +0x10 the buttons, +0x2c this frame's key, +0x30 the key held,
/// +0x34 the frames held).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyRepeat {
    pub repeat: u32,
    pub key: u32,
    pub held: u32,
    pub frames: i32,
}

impl KeyRepeat {
    /// `PadDevice_UpdateRepeat` 0x17a958: the first of `KEYS` whose buttons are down gives the key (the shifted
    /// one while L1, R1, L2 or R2 is held); a new key is given at once and held (−7 frames); a key held gives
    /// itself again after more than 7 frames (then every 8) when repeat is on.
    pub fn update(&mut self, buttons: u32) {
        let mut k = 0;
        for (mask, plain, shifted) in KEYS {
            if buttons & mask != 0 {
                k = if buttons & 0x3600 == 0 { plain } else { shifted };
                break;
            }
        }
        self.key = 0;
        if k == 0 || self.held != k {
            self.key = k;
            self.frames = -7;
            self.held = k;
        } else {
            self.frames += 1;
            if 7 < self.frames && self.repeat != 0 {
                self.key = k;
                self.frames = 0;
            }
        }
    }
}

/// A pad's rumble (`cPS2Device` +0x48 allowed, +0x78 / +0x79 the small and large motors, +0x80 to send, +0x84 /
/// +0x88 their frames left, −1 for on until told).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rumble {
    pub allowed: u32,
    pub small: u8,
    pub large: u8,
    pub send: u32,
    pub small_frames: i32,
    pub large_frames: i32,
}

impl Default for Rumble {
    /// as `cPS2Device::cPS2Device` 0x17ac40 leaves it
    fn default() -> Self {
        Rumble { allowed: 1, small: 0, large: 0, send: 1, small_frames: -1, large_frames: -1 }
    }
}

impl Rumble {
    /// vf3 0x17b590: the small motor on or off, for `frames`
    pub fn set_small(&mut self, on: u8, frames: i32) {
        if self.allowed != 0 {
            self.small = on;
            self.send = 1;
            self.small_frames = frames;
        }
    }

    /// vf4 0x17b5b0: the large motor at `amount` (× 255, truncated) for `frames`
    pub fn set_large(&mut self, amount: f32, frames: i32) {
        if self.allowed != 0 {
            self.send = 1;
            self.large_frames = frames;
            self.large = (amount * 255.0) as i32 as u8;
        }
    }

    /// vf5 0x17b5e8: rumble allowed or not (both motors off on a change)
    pub fn allow(&mut self, on: u32) {
        if self.allowed != on {
            self.allowed = on;
            self.send = 1;
            self.small = 0;
            self.large = 0;
        }
    }

    /// The end of the poll (vf9 0x17b270): with the pad ready (state 10) each motor's frames count down to 0 and
    /// it stops there (sent); not ready, both off. `Some(motors)` when they are sent (`scePadSetActDirect`).
    pub fn tick(&mut self, ready: bool) -> Option<(u8, u8)> {
        let mut large;
        if ready {
            let s = self.small_frames;
            if s < 1 {
                if s == 0 {
                    self.small_frames = -1;
                    self.send = 1;
                    self.small = 0;
                }
            } else {
                self.small_frames = s - 1;
            }
            large = self.large_frames;
            if large < 1 {
                if large == 0 {
                    self.large_frames = -1;
                    self.send = 1;
                    self.large = 0;
                }
                return self.flush();
            }
            large -= 1;
        } else {
            self.large = 0;
            self.small_frames = -1;
            self.small = 0;
            large = -1;
        }
        self.large_frames = large;
        self.flush()
    }

    fn flush(&mut self) -> Option<(u8, u8)> {
        if self.send != 0 {
            self.send = 0;
            Some((self.small, self.large))
        } else {
            None
        }
    }
}

/// The pad config and devices on the game's bytes (row E10d). The config (`Pad_ParseButtonMap`'s output): +8 the
/// button pairs' count, +0xc the grab pairs', +0x10 the axes'; pairs of words from +0x14 (buttons: pressed,
/// others), +0x84 (grabs) and +0xfc (axes: mask, negate), 8 bytes a pair.
pub mod bytes {
    use crate::fe3dstate::FeEnv;
    use crate::pf::Mem;

    /// `Pad_ParseButtonNames` 0x179e60, `Mem_Delete` 0x114f80, `scePadPortClose` 0x2e74c0
    pub const PARSE_NAMES: u32 = 0x179e60;
    pub const MEM_DELETE: u32 = 0x114f80;
    pub const PAD_PORT_CLOSE: u32 = 0x2e74c0;
    /// `cPadDevice`'s and `cPS2Device`'s tables (at +0x38)
    pub const PAD_DEVICE_VT: u32 = 0x385868;
    pub const PS2_DEVICE_VT: u32 = 0x385908;

    fn pair(m: &mut Mem, e: &mut dyn FeEnv, cfg: u32, count: u32, at: u32, a: u32, b: u32, flag: bool) {
        let x = e.call(m, PARSE_NAMES, &[cfg, a]);
        let n = m.r32(cfg + count);
        m.w32(cfg + at + 8 * n, x);
        let y = e.call(m, PARSE_NAMES, &[cfg, b]);
        let n = m.r32(cfg + count);
        m.w32(cfg + at + 4 + 8 * n, if flag { (y != 0) as u32 } else { y });
        m.w32(cfg + count, n.wrapping_add(1));
    }

    /// `Pad_ParseButtonPair` 0x179fb0(cfg, a, b): a button pair (the names of `a` pressed, of `b` the others) added
    pub fn parse_button_pair(m: &mut Mem, e: &mut dyn FeEnv, cfg: u32, a: u32, b: u32) {
        pair(m, e, cfg, 8, 0x14, a, b, false);
    }
    /// `Pad_ParseButtonPair2` 0x17a038(cfg, a, b): a grab pair added
    pub fn parse_grab_pair(m: &mut Mem, e: &mut dyn FeEnv, cfg: u32, a: u32, b: u32) {
        pair(m, e, cfg, 0xc, 0x84, a, b, false);
    }
    /// `Pad_ParseAxisPair` 0x17a0a0(cfg, a, b): an axis added (the names of `a` its mask, whether `b` names any its
    /// negate)
    pub fn parse_axis_pair(m: &mut Mem, e: &mut dyn FeEnv, cfg: u32, a: u32, b: u32) {
        pair(m, e, cfg, 0x10, 0xfc, a, b, true);
    }

    /// The getters (cfg, i): `PadCfg_ButtonPressed` 0x17a018 (+0x14), `PadCfg_ButtonOthers` 0x17a028 (+0x18),
    /// `PadCfg_AxisMask` 0x17a108 (+0xfc), `PadCfg_AxisNegate` 0x17a118 (+0x100), `PadCfg_GrabPressed` 0x17a128
    /// (+0x84), `PadCfg_GrabOthers` 0x17a138 (+0x88): the word at that offset of pair `i`
    pub fn get(m: &Mem, cfg: u32, at: u32, i: u32) -> u32 {
        m.r32(cfg.wrapping_add(i << 3).wrapping_add(at))
    }

    /// `cPadDevice::cPadDevice` 0x17aa08(d): the table, +0x30 = -1, +8 = 1, the rest (+0xc..+0x34) zero; `d`
    pub fn pad_device_construct(m: &mut Mem, d: u32) -> u32 {
        m.w32(d + 0x38, PAD_DEVICE_VT);
        m.w32(d + 0x30, u32::MAX);
        m.w32(d + 8, 1);
        for at in [0x10, 0xc, 0x14, 0x18, 0x2c, 0x34, 0x1c, 0x20, 0x24, 0x28] {
            m.w32(d + at, 0);
        }
        d
    }

    /// `cPadDevice::~cPadDevice` 0x17ab58(d, how): the table back; deleted when `how` has bit 0. Its virtuals
    /// vf3 0x17aa58, vf4 0x17aa60, vf5 0x17aa90, vf6 0x17aa88, vf7 0x17aa80 do nothing, vf1 0x17aa68 answers 0 and
    /// vf2 0x17aa70 0.0; `cPS2Device::vf8` 0x17ad60 answers 0x2c
    pub fn pad_device_destruct(m: &mut Mem, e: &mut dyn FeEnv, d: u32, how: u32) {
        m.w32(d + 0x38, PAD_DEVICE_VT);
        if how & 1 != 0 {
            e.call(m, MEM_DELETE, &[d]);
        }
    }

    /// `cPS2Device::~cPS2Device` 0x17ad10(d, how): its table back, its pad port (+0x3c, slot +0x40) closed, then the
    /// pad device's destructor
    pub fn ps2_device_destruct(m: &mut Mem, e: &mut dyn FeEnv, d: u32, how: u32) {
        m.w32(d + 0x38, PS2_DEVICE_VT);
        e.call(m, PAD_PORT_CLOSE, &[m.r32(d + 0x3c), m.r32(d + 0x40)]);
        pad_device_destruct(m, e, d, how);
    }
}
