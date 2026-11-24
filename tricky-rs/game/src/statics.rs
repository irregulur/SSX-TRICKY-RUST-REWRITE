//! The static initializers that only write one header's three constant vectors (row X2a): forty of them, one per
//! source file that includes the header, each at its own place. Each is 124 bytes of `lui`/`mtc1`/`swc1`; their
//! places were read from the disassembly and the writes are checked in the runner (`tests/statics.rs`).

use crate::pf::Mem;

/// The header's three vectors, x y z (w is not written): (1, 0.966, 0), (1, 0.474, 0), (1, 0.0757, 0.002) as the
/// game's bits, 0x10 bytes apart
pub const HEADER_VECTORS: [[u32; 3]; 3] = [
    [0x3f80_0000, 0x3f76_e979, 0],
    [0x3f80_0000, 0x3ef2_fec5, 0],
    [0x3f80_0000, 0x3d9b_3d08, 0x3b03_126f],
];

/// The forty initializers and where each writes the vectors
pub const HEADER_INITS: [(u32, u32); 40] = [
    (0x105b10, 0x3bbd30), // `StaticInit_PrewindControl`
    (0x106480, 0x3bbd60), // `StaticInit_GateLaunchControl`
    (0x106548, 0x3bbd90), // `StaticInit_LessonWaitControl`
    (0x10afe8, 0x3bbed0), // `StaticInit_GroundMotion2`
    (0x1165c8, 0x3bc070), // `StaticInit_AIWorld`
    (0x1167a8, 0x3bc0a0), // `StaticInit_AIWorld2`
    (0x126888, 0x3bc190), // `StaticInit_Boarder2`
    (0x1283f0, 0x3bc1c0), // `StaticInit_Boarder3`
    (0x12bff0, 0x3bc1f0), // `StaticInit_Takeoff`
    (0x138160, 0x3bc260), // `StaticInit_AIComputer`
    (0x13a520, 0x3bc290), // `StaticInit_Computer`
    (0x13d4b8, 0x3bc2f0), // `StaticInit_ScriptNode`
    (0x1616a0, 0x3bf150), // `StaticInit_AnimCtl`
    (0x1682e0, 0x3cbfc8), // `StaticInit_Rivals`
    (0x16d910, 0x3cd860), // `StaticInit_Circuit`
    (0x16dbe8, 0x3cd890), // `StaticInit_CamScript`
    (0x16de00, 0x3cd8c0), // `StaticInit_CamCamera`
    (0x16e630, 0x3cd8f0), // `StaticInit_Cml`
    (0x16e6f0, 0x3cd920), // `StaticInit_CamDirector`
    (0x1703c0, 0x3cd980), // `StaticInit_Camera`
    (0x171308, 0x3cd9b8), // `StaticInit_CamPath`
    (0x172168, 0x3cda18), // `StaticInit_CamScene`
    (0x1730e0, 0x3cda48), // `StaticInit_CamCamera2`
    (0x1790b8, 0x3cdbe8), // `StaticInit_OfflineObject2`
    (0x179138, 0x3cdc18), // `StaticInit_OfflineObject3`
    (0x1791b8, 0x3cdc48), // `StaticInit_OfflineObject4`
    (0x19e6a0, 0x3ce2a8), // `StaticInit_Font`
    (0x1c8cd0, 0x3d0d70), // `StaticInit_TerrainObjNode`
    (0x1d9a68, 0x3d0ec0), // `StaticInit_ParticleMan`
    (0x201018, 0x3d2110), // `StaticInit_Instance`
    (0x2023a8, 0x3d2140), // `StaticInit_AsyncBdrLoad`
    (0x221428, 0x3d2a38), // `StaticInit_CrowdSys`
    (0x222888, 0x3d2a68), // `StaticInit_MidiSys`
    (0x225d80, 0x3d2a98), // `StaticInit_Music`
    (0x228e38, 0x3d2ac8), // `StaticInit_SpeechSys`
    (0x22b240, 0x3d2b48), // `StaticInit_AudioTGroups`
    (0x22bd68, 0x3d2b78), // `StaticInit_WorldEmitter`
    (0x22e420, 0x3d2ba8), // `StaticInit_WorldTriggerMgr`
    (0x22e628, 0x3d2bd8), // `StaticInit_WorldTriggerMgr2`
    (0x251b20, 0x3d2cd8), // `StaticInit_Spline`
];

/// One of the forty initializers ([`HEADER_INITS`]): the three vectors written at `base`
pub fn init_header_vectors(m: &mut Mem, base: u32) {
    for (k, v) in HEADER_VECTORS.iter().enumerate() {
        for (j, &w) in v.iter().enumerate() {
            m.w32(base + 0x10 * k as u32 + 4 * j as u32, w);
        }
    }
}
