//! Host input layer (board row E10c3): converts Bevy keyboard and gamepad state into PS2 libpad words + sticks
//! (`inputring::Record`), runs `Input_ReadDevices` 0x17a668 -> `InputRecorder_Playback` 0x17a740 -> `Ps2Pad::play`
//! 0x17b278, loads `DATA/CONFIG/BTNMAP<n>.DAT` (`Pad_LoadButtonMap` 0x179a88 -> `Pad_ParseButtonMap` 0x179b18 ->
//! `Pad_SetupVibration` 0x152770 including `cInput +0x1cc` shown buttons), and ticks `cPS2Device::vf9` 0x17b270
//! (`input::Rumble`) for connected Bevy gamepads.

use bevy::input::gamepad::{GamepadAxis, GamepadButton, GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use std::time::Duration;
use tricky_game::controls::PadDevice;
use tricky_game::input::{parse_button_map, PadConfig, Rumble};
use tricky_game::inputring::{Input as InputList, Record};
use tricky_game::rider::Rider;

/// Libpad button bit constants (`controls::PS2_BITS` 0x337c08, pressed = 1).
pub const LIBPAD_L2: u16 = 0x0001;
pub const LIBPAD_R2: u16 = 0x0002;
pub const LIBPAD_L1: u16 = 0x0004;
pub const LIBPAD_R1: u16 = 0x0008;
pub const LIBPAD_TRIANGLE: u16 = 0x0010;
pub const LIBPAD_CIRCLE: u16 = 0x0020;
pub const LIBPAD_CROSS: u16 = 0x0040;
pub const LIBPAD_SQUARE: u16 = 0x0080;
pub const LIBPAD_SELECT: u16 = 0x0100;
pub const LIBPAD_L3: u16 = 0x0200;
pub const LIBPAD_R3: u16 = 0x0400;
pub const LIBPAD_START: u16 = 0x0800;
pub const LIBPAD_UP: u16 = 0x1000;
pub const LIBPAD_RIGHT: u16 = 0x2000;
pub const LIBPAD_DOWN: u16 = 0x4000;
pub const LIBPAD_LEFT: u16 = 0x8000;

/// Hardware-independent snapshot of Bevy keyboard + gamepad state for one frame, converted to a PS2 libpad [`Record`].
#[derive(Clone, Copy, Debug, Default)]
pub struct RawHostInput {
    /// Physical gamepad analog sticks in Bevy axes (`+X` right, `+Y` up)
    pub pad_left: [f32; 2],
    pub pad_right: [f32; 2],
    /// Physical gamepad libpad buttons (pressed = 1)
    pub pad_buttons: u16,
    /// Keyboard digital steering (`-1` left, `+1` right), tuck/brake (`+1` up/W, `-1` down/S), flip (`+1` E, `-1` Q)
    pub key_steer: i8,
    pub key_tuck_brake: i8,
    pub key_flip: i8,
    /// Keyboard libpad buttons (Cross, Square, L1/R1/L2/R2, Select, Start, R3)
    pub key_buttons: u16,
    /// Keyboard shove (`KeyF`: right stick X)
    pub key_shove: bool,
    /// Whether `SELECT` (reset) is allowed through this tick (false while in menu/start gate/finished)
    pub allow_reset: bool,
}

fn stick_byte(v: f32) -> u32 {
    ((v.clamp(-1.0, 1.0) * 127.5) + 127.5).round().clamp(0.0, 255.0) as u32
}

/// Converts [`RawHostInput`] into a PS2 libpad [`Record`] (`cPS2Device` vf8/vf9: 16-bit inverted-to-active-high
/// button word + `[lx, ly, rx, ry]` stick bytes with right/down positive).
pub fn pack_record(raw: &RawHostInput) -> Record {
    let mut buttons = raw.pad_buttons | raw.key_buttons;
    // Left stick: keyboard overrides its axis when held, else physical gamepad analog stick passes through untouched.
    let lx = if raw.key_steer != 0 { raw.key_steer as f32 } else { raw.pad_left[0] };
    // Bevy +Y is up; PS2 stick +Y is down (`(byte - 127.5) / 127.5`), so negate Y.
    let ly = if raw.key_tuck_brake != 0 { -(raw.key_tuck_brake as f32) } else { -raw.pad_left[1] };
    // Right stick: shove key pushes right stick X (`ShoveSide` axis 5), else physical right stick.
    let rx = if raw.key_shove { 1.0 } else { raw.pad_right[0] };
    let ry = -raw.pad_right[1];

    // Keyboard steering also drives D-Pad L/R and E/Q drive D-Pad U/D so BTNMAP0.DAT (which maps Spin/Flip/Prewind
    // axes to ANALOG_PAD_X / ANALOG_PAD_Y) works with keyboard controls while keeping physical gamepads 1:1.
    if raw.key_steer < 0 { buttons |= LIBPAD_LEFT; }
    if raw.key_steer > 0 { buttons |= LIBPAD_RIGHT; }
    if raw.key_flip > 0 { buttons |= LIBPAD_UP; }
    if raw.key_flip < 0 { buttons |= LIBPAD_DOWN; }
    if !raw.allow_reset { buttons &= !LIBPAD_SELECT; }

    Record {
        raw: [0; 16],
        valid: 1,
        analog: 1,
        buttons,
        sticks: [stick_byte(lx), stick_byte(ly), stick_byte(rx), stick_byte(ry)],
    }
}

/// Reads Bevy's keyboard + first connected [`Gamepad`] into a PS2 libpad [`Record`].
pub fn bevy_to_record(keys: &ButtonInput<KeyCode>, pad: Option<&Gamepad>, allow_reset: bool) -> Record {
    let key = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
    let mut key_buttons = 0u16;
    if keys.pressed(KeyCode::Space) { key_buttons |= LIBPAD_CROSS; }
    if keys.pressed(KeyCode::KeyK) || key(KeyCode::ShiftLeft, KeyCode::ShiftRight) { key_buttons |= LIBPAD_SQUARE; }
    if keys.pressed(KeyCode::KeyJ) { key_buttons |= LIBPAD_L1; }
    if keys.pressed(KeyCode::KeyL) { key_buttons |= LIBPAD_R1; }
    if keys.pressed(KeyCode::KeyU) { key_buttons |= LIBPAD_L2; }
    if keys.pressed(KeyCode::KeyO) { key_buttons |= LIBPAD_R2; }
    if keys.pressed(KeyCode::Backspace) { key_buttons |= LIBPAD_SELECT; }
    if keys.pressed(KeyCode::Enter) { key_buttons |= LIBPAD_START; }
    if keys.pressed(KeyCode::KeyF) { key_buttons |= LIBPAD_R3; }

    let mut raw = RawHostInput {
        key_steer: key(KeyCode::KeyD, KeyCode::ArrowRight) as i8 - key(KeyCode::KeyA, KeyCode::ArrowLeft) as i8,
        key_tuck_brake: key(KeyCode::KeyW, KeyCode::ArrowUp) as i8 - key(KeyCode::KeyS, KeyCode::ArrowDown) as i8,
        key_flip: keys.pressed(KeyCode::KeyE) as i8 - keys.pressed(KeyCode::KeyQ) as i8,
        key_buttons,
        key_shove: keys.pressed(KeyCode::KeyF),
        allow_reset,
        ..Default::default()
    };

    if let Some(p) = pad {
        raw.pad_left = [
            p.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
            p.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
        ];
        raw.pad_right = [
            p.get(GamepadAxis::RightStickX).unwrap_or(0.0),
            p.get(GamepadAxis::RightStickY).unwrap_or(0.0),
        ];
        let mut pb = 0u16;
        if p.pressed(GamepadButton::DPadUp) { pb |= LIBPAD_UP; }
        if p.pressed(GamepadButton::DPadDown) { pb |= LIBPAD_DOWN; }
        if p.pressed(GamepadButton::DPadRight) { pb |= LIBPAD_RIGHT; }
        if p.pressed(GamepadButton::DPadLeft) { pb |= LIBPAD_LEFT; }
        if p.pressed(GamepadButton::South) { pb |= LIBPAD_CROSS; }
        if p.pressed(GamepadButton::West) { pb |= LIBPAD_SQUARE; }
        if p.pressed(GamepadButton::North) { pb |= LIBPAD_TRIANGLE; }
        if p.pressed(GamepadButton::East) { pb |= LIBPAD_CIRCLE; }
        if p.pressed(GamepadButton::LeftTrigger) { pb |= LIBPAD_L1; }
        if p.pressed(GamepadButton::RightTrigger) { pb |= LIBPAD_R1; }
        if p.pressed(GamepadButton::LeftTrigger2) { pb |= LIBPAD_L2; }
        if p.pressed(GamepadButton::RightTrigger2) { pb |= LIBPAD_R2; }
        if p.pressed(GamepadButton::Select) { pb |= LIBPAD_SELECT; }
        if p.pressed(GamepadButton::Start) { pb |= LIBPAD_START; }
        if p.pressed(GamepadButton::LeftThumb) { pb |= LIBPAD_L3; }
        if p.pressed(GamepadButton::RightThumb) { pb |= LIBPAD_R3; }
        raw.pad_buttons = pb;
    }

    pack_record(&raw)
}

/// Host resource holding the game's `cInput` device list + 8-frame ring (`tricky_game::inputring::Input`), the
/// parsed disc `DATA/CONFIG/BTNMAP<n>.DAT` configs (`Pad_LoadButtonMap` 0x179a88 / `Pad_ParseButtonMap` 0x179b18),
/// and the last sent pad rumble motor levels (`cPS2Device::vf9` 0x17b270).
#[derive(Resource)]
pub struct PadHost {
    pub input: InputList,
    pub configs: [Option<PadConfig>; 2],
    pub config_idx: usize,
    pub last_motors: (u8, u8),
}

impl Default for PadHost {
    fn default() -> Self {
        Self { input: InputList::new(), configs: [None, None], config_idx: 0, last_motors: (0, 0) }
    }
}

impl PadHost {
    /// Opens `TRICKY_DISC` (if set) and parses `data/config/btnmap0.dat` and `data/config/btnmap1.dat` via
    /// `input::parse_button_map` (`Pad_LoadButtonMap` 0x179a88 / `Pad_ParseButtonMap` 0x179b18).
    pub fn load() -> Self {
        let mut host = Self::default();
        if let Ok(disc_path) = std::env::var("TRICKY_DISC") {
            if let Ok(disc) = tricky_data::disc::Disc::open(&disc_path) {
                let mut files = tricky_data::big::Files::new(disc);
                let _ = files.mount_game_archives();
                for n in 0..2usize {
                    let path = format!("data/config/btnmap{n}.dat");
                    if let Ok(Some(bytes)) = files.load(&path) {
                        host.configs[n] = Some(parse_button_map(&bytes));
                    }
                }
                let loaded = host.configs.iter().filter(|c| c.is_some()).count();
                if loaded > 0 {
                    println!("input: loaded {loaded} BTNMAP<n>.DAT config(s) from disc");
                }
            }
        }
        host
    }

    /// One 60 Hz tick through the game's input ring: `Input_ReadDevices` 0x17a668 -> `InputRecorder_Playback`
    /// 0x17a740 -> `Ps2Pad::play` 0x17b278 (updating `PadDevice` buttons/prev/sticks and `KeyRepeat`).
    pub fn step_tick(&mut self, rec: Record) {
        self.input.read_devices(&[rec, Record::none(false)]);
        self.input.playback();
    }

    /// Current `PadDevice` for player 1 (`pads[0].dev`) produced by `Ps2Pad::play`.
    pub fn pad_dev(&self) -> PadDevice {
        self.input.pads[0].dev
    }

    /// Active parsed `BTNMAP<n>.DAT` config (if loaded from disc).
    pub fn active_config(&self) -> Option<PadConfig> {
        self.configs[self.config_idx.min(1)].clone()
    }

    /// `cInput +0x1cc` "shown" button masks for the rider's active map (`Input_SetShownButton` 0x179850).
    #[allow(dead_code)]
    pub fn shown_buttons(&self, rider: &Rider) -> [u32; 0x29] {
        rider.button_map().shown
    }

    /// Ticks `cPS2Device::vf9` 0x17b270 motor timers on `pad_rumble`, mirrors into `self.input.pads[0].rumble`, and
    /// returns `Some((small, large))` whenever `scePadSetActDirect` would be called.
    pub fn sync_rumble(&mut self, pad_rumble: &mut Rumble, ready: bool) -> Option<(u8, u8)> {
        let sent = pad_rumble.tick(ready);
        self.input.pads[0].rumble = *pad_rumble;
        if let Some(motors) = sent {
            self.last_motors = motors;
        }
        sent
    }

    /// Forwards `cPS2Device` `(small, large)` motor levels (`0/1`, `0..=255`) to Bevy's gamepad rumble event channel.
    pub fn emit_bevy_rumble(gamepad: Entity, motors: (u8, u8), out: &mut EventWriter<GamepadRumbleRequest>) {
        if motors == (0, 0) {
            out.write(GamepadRumbleRequest::Stop { gamepad });
        } else {
            out.write(GamepadRumbleRequest::Add {
                gamepad,
                duration: Duration::from_millis(120),
                intensity: GamepadRumbleIntensity {
                    strong_motor: motors.1 as f32 / 255.0,
                    weak_motor: if motors.0 != 0 { 1.0 } else { 0.0 },
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tricky_game::controls::{get_controls, ButtonMap, ControlsIn, Mapper};

    #[test]
    fn pack_record_through_ps2pad_play_and_mapper_drives_get_controls() {
        let mut host = PadHost::default();
        let slots: [u16; 15] = std::array::from_fn(|k| k as u16);
        let map = ButtonMap::btnmap(0, slots);

        // Tick 1: press Cross (jump/antic) + Square (boost) + steer right + tuck up on keyboard
        let rec1 = pack_record(&RawHostInput {
            key_steer: 1,
            key_tuck_brake: 1,
            key_buttons: LIBPAD_CROSS | LIBPAD_SQUARE | LIBPAD_SELECT,
            allow_reset: true,
            ..Default::default()
        });
        host.step_tick(rec1);
        let dev1 = host.pad_dev();
        let mut trick = 0x1a;
        let mut slope = false;
        // Cruise state (3): bit 0 = reset pressed, bit 1 = jump pressed, bit 2 = jump held, bit 3 = boost held,
        // bit 4 = boost pressed, bits 5..10 = steer, bits 11..16 = crouch
        let word1 = get_controls(
            ControlsIn { state: 3, paused: false },
            &mut Mapper { map: &map, pad: &dev1 },
            &mut trick,
            &mut || 0,
            &mut slope,
        );
        assert_eq!(word1 & 1, 1, "SELECT pressed on tick 1 sets reset bit 0");
        assert_eq!((word1 >> 1) & 1, 1, "CROSS pressed on tick 1 sets jump-edge bit 1");
        assert_eq!((word1 >> 2) & 1, 1, "CROSS held sets jump-held bit 2");
        assert_eq!((word1 >> 3) & 1, 1, "SQUARE held sets boost-held bit 3");
        assert_eq!((word1 >> 4) & 1, 1, "SQUARE pressed on tick 1 sets boost-edge bit 4");
        let crouch1 = ((word1 << 15) as i32 >> 26) as f32 / 31.0;
        assert!(crouch1 > 0.9, "W/Up maps to Crouch axis (+1.0): {crouch1}");
        let steer1 = ((word1 << 21) as i32 >> 26) as f32 / 31.0;
        assert!(steer1 < -0.9, "D/Right maps to Turn axis (-1.0 in game sign): {steer1}");

        // Tick 2: same buttons still held -> edge bits (reset bit 0, jump-edge bit 1, boost-edge bit 4) clear via prev!
        host.step_tick(rec1);
        let dev2 = host.pad_dev();
        let word2 = get_controls(
            ControlsIn { state: 3, paused: false },
            &mut Mapper { map: &map, pad: &dev2 },
            &mut trick,
            &mut || 0,
            &mut slope,
        );
        assert_eq!(word2 & 1, 0, "held SELECT must not re-fire reset edge bit 0");
        assert_eq!((word2 >> 1) & 1, 0, "held CROSS must not re-fire jump edge bit 1");
        assert_eq!((word2 >> 2) & 1, 1, "held CROSS keeps jump-held bit 2");
        assert_eq!((word2 >> 3) & 1, 1, "held SQUARE keeps boost-held bit 3");
        assert_eq!((word2 >> 4) & 1, 0, "held SQUARE must not re-fire boost edge bit 4");

        // Spin state (0x10) with L2 grab + E front flip (D-pad Up) + K tweak
        let rec3 = pack_record(&RawHostInput {
            key_flip: 1,
            key_buttons: LIBPAD_L2 | LIBPAD_SQUARE,
            allow_reset: true,
            ..Default::default()
        });
        host.step_tick(rec3);
        let dev3 = host.pad_dev();
        let word3 = get_controls(
            ControlsIn { state: 0x10, paused: false },
            &mut Mapper { map: &map, pad: &dev3 },
            &mut trick,
            &mut || 0,
            &mut slope,
        );
        assert_eq!(word3 & 0xff, 0, "L2 maps to grab slot 0");
        assert_eq!((word3 >> 9) & 1, 1, "SQUARE maps to tweak bit 9");
        assert_eq!((word3 >> 14) & 3, 3, "E (D-pad Up) maps to flip axis direction -1 (2-bit 3)");
    }

    #[test]
    fn disc_btnmaps_bind_shown_buttons_and_vibration_ticks() {
        let mut host = PadHost::load();
        let mut r = Rider::new(Vec3::ZERO, 0.0);
        r.set_host_pad(host.pad_dev(), host.active_config());
        let bm = r.button_map();
        // Action 0x1b (Antic) shows X (0x800), 0x1d (Boost) shows SQUARE (0x10), 0x25 (Reset) shows SELECT (0x100)
        assert_eq!(bm.shown[0x1b], tricky_game::controls::X);
        assert_eq!(bm.shown[0x1d], tricky_game::controls::SQUARE);
        assert_eq!(bm.shown[0x25], tricky_game::controls::SELECT);
        assert_eq!(host.shown_buttons(&r), bm.shown);

        // Vibration object (`Pad_SetupVibration` -> `Rumble_Add` -> `Rumble_Update` -> `cPS2Device::vf9` tick)
        r.rumble_add(0.9);
        assert!(r.vibration.level > 1.5);
        r.rumble_update();
        let sent = host.sync_rumble(&mut r.pad_rumble, true);
        assert!(matches!(sent, Some((1, l)) if l > 0), "expected active small+large motors: {sent:?}");
        // Decay to zero
        for _ in 0..120 {
            r.rumble_update();
            host.sync_rumble(&mut r.pad_rumble, true);
        }
        assert_eq!(host.last_motors, (0, 0), "motors must stop after vibration decays");
    }
}
