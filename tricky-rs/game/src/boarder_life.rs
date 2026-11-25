//! The boarder's destructors and empty virtuals (board row G1x): the states' and motions' destructors (one body,
//! each with its own table), `cBoarder`'s and `cBoarderCache`'s destructors, the small parts' (`AirPredict`,
//! `Score`, `CollBody`), and `cBoarder`'s virtuals that do nothing or answer a constant. The calls out (`Mem_Delete`
//! and the parts' destructors that live elsewhere) go through [`FeEnv`].

use crate::fe3dstate::FeEnv;
use crate::pf::Mem;

pub const MEM_DELETE: u32 = 0x114f80;
pub const BOARDER_RESET_SKIN: u32 = 0x116a18;
pub const SHAPE_LIST_DESTRUCT: u32 = 0x203ff8;
pub const SKIN_DELETE: u32 = 0x207040;
/// `cBoarder`'s table 0x367640, `cBoarderCache`'s 0x3676f0
pub const BOARDER_VT: u32 = 0x367640;
pub const BOARDER_CACHE_VT: u32 = 0x3676f0;

/// The states' and motions' destructors: the address, the class's table (0xb0 bytes), where the object keeps its
/// boarder's address, and the distance the first adjust is reckoned from
pub const STATE_DESTRUCTORS: [(u32, u32, u32, u32, &str); 28] = [
    (0x1209f0, 0x366300, 0, 0x10, "cWipeOutRecoverControl"),
    (0x120b28, 0x3663b0, 0, 0x10, "cWipeOutControl"),
    (0x120c60, 0x366460, 0, 0x10, "cGetupFromSitControl"),
    (0x120d98, 0x366510, 0, 0x10, "cSitAndWaitControl"),
    (0x120ed8, 0x3665c0, 0x70, 0x80, "cSpinControl"),
    (0x121018, 0x366670, 0x14, 0x20, "cRailSlideControl"),
    (0x121158, 0x366720, 0xc, 0x10, "cPrewindControl"),
    (0x121298, 0x3667d0, 0xc, 0x10, "cNaturalAirControl"),
    (0x1213d8, 0x366880, 0xc, 0x10, "cLandNormalControl"),
    (0x121510, 0x366930, 0, 0x10, "cLandHardControl"),
    (0x121648, 0x3669e0, 0, 0x10, "cJumpControl"),
    (0x121780, 0x366a90, 0, 0x10, "cGateLaunchControl"),
    (0x1218c0, 0x366b40, 0x14, 0x20, "cGateAnticipateControl"),
    (0x121a00, 0x366bf0, 0xc, 0x10, "cLessonWaitControl"),
    (0x121b38, 0x366ca0, 0, 0x10, "cGate2BaseControl"),
    (0x121c78, 0x366d50, 0x18, 0x20, "cResetWaitControl"),
    (0x121db8, 0x366e00, 0x10, 0x20, "cFinishLineControl"),
    (0x121ef0, 0x366eb0, 0, 0x10, "cFakieCruiseControl"),
    (0x122028, 0x366f60, 0, 0x10, "cChangeFakieControl"),
    (0x122168, 0x367010, 0x14, 0x20, "cCruiseControl"),
    (0x1222a8, 0x3670c0, 0x10, 0x20, "cBumpControl"),
    (0x1223e0, 0x367170, 0, 0x10, "cVoidControl"),
    (0x122520, 0x367220, 0xc, 0x10, "cWipeOutRecoverMotion"),
    (0x122660, 0x3672d0, 0x60, 0x100, "cWipeOutMotion"),
    (0x122798, 0x367380, 0, 0x10, "cStaticMotion"),
    (0x1228d8, 0x367430, 0x10, 0x20, "cRailSlideMotion"),
    (0x122a18, 0x3674e0, 0xc, 0x10, "cGroundMotion"),
    (0x122b58, 0x367590, 0x20, 0x30, "cAirMotion"),
];

/// The 28 destructors of the states and motions (`cWipeOutRecoverControl::~cWipeOutRecoverControl` 0x1209f0 ..
/// `cAirMotion::~cAirMotion` 0x122b58; `d` its row of [`STATE_DESTRUCTORS`]), `sp` the stack pointer on entry. The
/// object's word at `d.2` points at its boarder (the virtual base), whose table becomes the class's. With `how` 0
/// (not in charge of the base) the boarder is instead given a copy of the table on the stack (0xb0 bytes at `sp` −
/// 0xe0, its first adjust (+8) moved by the object's distance from the boarder less `d.3`), as gcc's code does; else the
/// boarder is destroyed when `how` has bit 1 ([`boarder_destruct`] with 0) and the object deleted on bit 0
pub fn state_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32, d: (u32, u32, u32, u32, &str), sp: u32) {
    let (_, vt, at, back, _) = d;
    let b = m.r32(o + at);
    m.w32(b, vt);
    if how == 0 {
        let copy = sp.wrapping_sub(0xe0);
        for k in 0..0xb0 / 4 {
            let w = m.r32(vt + 4 * k);
            m.w32(copy + 4 * k, w);
        }
        let adj = (m.r16(vt + 8) as u32).wrapping_add(o.wrapping_sub(b.wrapping_sub(back)));
        m.w32(b, copy);
        m.w16(copy + 8, adj as u16);
        return;
    }
    if how & 2 != 0 {
        let b = m.r32(o + at);
        boarder_destruct(m, e, b, 0);
    }
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[o]);
    }
}

/// `cBoarder::~cBoarder` 0x116990(b, how): its table, the skin reset (`Boarder_ResetSkin` 0x116a18 with 1), then the
/// parts with 2: the score (+0x5820), the air prediction (+0x57b0), the shape list (+0x4838, 0x203ff8), the skin
/// (+0x79c, 0x207040), the collision body (+0x470); then [`cache_destruct`]
pub fn boarder_destruct(m: &mut Mem, e: &mut dyn FeEnv, b: u32, how: u32) {
    m.w32(b, BOARDER_VT);
    e.call(m, BOARDER_RESET_SKIN, &[b, 1]);
    part_destruct(m, e, b.wrapping_add(0x5820), 2);
    part_destruct(m, e, b.wrapping_add(0x57b0), 2);
    e.call(m, SHAPE_LIST_DESTRUCT, &[b.wrapping_add(0x4838), 2]);
    e.call(m, SKIN_DELETE, &[b.wrapping_add(0x79c), 2]);
    part_destruct(m, e, b.wrapping_add(0x470), 2);
    cache_destruct(m, e, b, how);
}

/// `cBoarderCache::~cBoarderCache` 0x122ff8(b, how): its table, deleted on bit 0
pub fn cache_destruct(m: &mut Mem, e: &mut dyn FeEnv, b: u32, how: u32) {
    m.w32(b, BOARDER_CACHE_VT);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[b]);
    }
}

/// `Score_Destruct` 0x1230a0, `AirPredict_Destruct` 0x1230c0 and `CollBody_Destruct` 0x236190 (p, how): `p`
/// deleted on bit 0
pub fn part_destruct(m: &mut Mem, e: &mut dyn FeEnv, p: u32, how: u32) {
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[p]);
    }
}

/// `cBoarder`'s virtuals that do nothing: vf12 0x11a7a0, vf20..vf14 0x122d60..0x122d90, vf9 0x122e50, vf4..vf1
/// 0x122fd8..0x122ff0
pub fn nothing(_m: &mut Mem, _b: u32) {}

/// `cBoarder::vf6` 0x122fc8 and `vf5` 0x122fd0: 0
pub fn zero(_m: &Mem, _b: u32) -> u32 {
    0
}

/// `cBoarder::vf13` 0x122e20: 1.0
pub fn one(_m: &Mem, _b: u32) -> f32 {
    1.0
}
