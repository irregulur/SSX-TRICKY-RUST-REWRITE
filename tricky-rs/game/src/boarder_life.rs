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

pub const BOARDER_HANDLER_VT: u32 = 0x3661f0;
pub const BOARDER_HANDLER_VBASE_VT: u32 = 0x366250;
pub const BOARDER_CTOR: u32 = 0x116828;
pub const BOARDER_DTOR: u32 = 0x116990;
pub const BOARDER_RENDER_CTOR: u32 = 0x135278;
pub const BOARDER_RENDER_DTOR: u32 = 0x1355c8;
pub const RIGID_BODY_CTOR: u32 = 0x1537d0;
pub const SORT_OBJ_NODE_CTOR: u32 = 0x17d0e0;
pub const SORT_OBJ_NODE_DTOR: u32 = 0x17dd00;
pub const BROADPHASE_SET_BOX: u32 = 0x123098;
pub const BROADPHASE_FIT_BOX: u32 = 0x14fa70;
pub const GAPP: u32 = 0x337c58;

/// The 28 embedded motion/control subobjects of `cBoarderHandler` in construction order:
/// `(state_off, vbase_ptr_off, back_off, vtable)`. Destruction runs in exact reverse order.
pub const HANDLER_STATES: [(u32, u32, u32, u32); 28] = [
    (0x480, 0x20, 0x030, 0x367590), // cAirMotion
    (0x4b0, 0x0c, 0x010, 0x3674e0), // cGroundMotion
    (0x4c0, 0x10, 0x020, 0x367430), // cRailSlideMotion
    (0x4e0, 0x00, 0x010, 0x367380), // cStaticMotion
    (0x4f0, 0x60, 0x100, 0x3672d0), // cWipeOutMotion (calls RigidBody_Construct at +0x70 = o+0x560)
    (0x5f0, 0x0c, 0x010, 0x367220), // cWipeOutRecoverMotion
    (0x600, 0x00, 0x010, 0x367170), // cVoidControl
    (0x610, 0x10, 0x020, 0x3670c0), // cBumpControl
    (0x630, 0x14, 0x020, 0x367010), // cCruiseControl
    (0x650, 0x00, 0x010, 0x366f60), // cChangeFakieControl
    (0x660, 0x00, 0x010, 0x366eb0), // cFakieCruiseControl
    (0x670, 0x10, 0x020, 0x366e00), // cFinishLineControl
    (0x690, 0x00, 0x010, 0x366ca0), // cGate2BaseControl
    (0x6a0, 0x14, 0x020, 0x366b40), // cGateAnticipateControl
    (0x6c0, 0x00, 0x010, 0x366a90), // cGateLaunchControl
    (0x6d0, 0x00, 0x010, 0x3669e0), // cJumpControl
    (0x6e0, 0x00, 0x010, 0x366930), // cLandHardControl
    (0x6f0, 0x0c, 0x010, 0x366880), // cLandNormalControl
    (0x700, 0x0c, 0x010, 0x3667d0), // cNaturalAirControl
    (0x710, 0x0c, 0x010, 0x366720), // cPrewindControl
    (0x720, 0x14, 0x020, 0x366670), // cRailSlideControl
    (0x740, 0x70, 0x080, 0x3665c0), // cSpinControl
    (0x7c0, 0x00, 0x010, 0x366510), // cSitAndWaitControl
    (0x7d0, 0x00, 0x010, 0x366460), // cGetupFromSitControl
    (0x7e0, 0x00, 0x010, 0x3663b0), // cWipeOutControl
    (0x7f0, 0x00, 0x010, 0x366300), // cWipeOutRecoverControl
    (0x800, 0x0c, 0x010, 0x366bf0), // cLessonWaitControl
    (0x810, 0x18, 0x020, 0x366d50), // cResetWaitControl
];

fn copy_state_vbase_vt(m: &mut Mem, state: u32, at: u32, back: u32, vt: u32, copy: u32) {
    let b = m.r32(state.wrapping_add(at));
    m.w32(b, vt);
    for k in 0..0xb0 / 4 {
        let w = m.r32(vt + 4 * k);
        m.w32(copy + 4 * k, w);
    }
    let adj = (m.r16(vt + 8) as u32).wrapping_add(state.wrapping_sub(b.wrapping_sub(back)));
    m.w32(b, copy);
    m.w16(copy + 8, adj as u16);
}

fn copy_handler_vbase_vt(m: &mut Mem, o: u32, b: u32, copy: u32) {
    for k in 0..0xb0 / 4 {
        let w = m.r32(BOARDER_HANDLER_VBASE_VT + 4 * k);
        m.w32(copy + 4 * k, w);
    }
    let delta = o.wrapping_sub(b.wrapping_sub(0x860)) as u16;
    for off in [0x08u32, 0x40] {
        let adj = m.r16(BOARDER_HANDLER_VBASE_VT + off).wrapping_add(delta);
        m.w16(copy + off, adj);
    }
    m.w32(b, copy);
}

/// `cBoarderHandler::cBoarderHandler` 0x11b348(o, in_charge, rider_idx; sp on entry): when `in_charge != 0`, fills
/// all 29 virtual-base slots with `o + 0x860` and constructs `cBoarder` (`0x116828`); constructs `cBoarderRender`
/// (`0x135278`, `0`); runs all 28 embedded motion/control subobject constructors (each installing its `0xb0`-byte
/// adjusted stack vtable copy at `sp - 0x1560 + 0xb0 * i`, with `cWipeOutMotion` calling `RigidBody_Construct`
/// `0x1537d0` on `o + 0x560`); constructs `cBxSortObjNode` (`0x17d0e0` on `o + 0x82c`, `rider_idx as i32`, `0x3ef`);
/// installs `BOARDER_HANDLER_VBASE_VT` (`0x366250`, or its adjusted stack copy at `sp - 0x220` when `in_charge == 0`)
/// and `BOARDER_HANDLER_VT` (`0x3661f0`) at `o + 0x834`; links `b + 0x5ae0 = o` and `b + 0x784 = o + 0x82c`; sets
/// and fits the broadphase box (`0x123098`, `0x14fa70`); and returns `o`
pub fn boarder_handler_construct(
    m: &mut Mem,
    e: &mut dyn FeEnv,
    o: u32,
    in_charge: u32,
    rider_idx: i16,
    sp: u32,
) -> u32 {
    let frame = sp.wrapping_sub(0x1560);
    if in_charge != 0 {
        let vb = o.wrapping_add(0x860);
        for &(state_off, at, _, _) in HANDLER_STATES.iter().rev() {
            m.w32(o.wrapping_add(state_off + at), vb);
        }
        m.w32(o, vb);
        e.call(m, BOARDER_CTOR, &[vb]);
    }
    e.call(m, BOARDER_RENDER_CTOR, &[o, 0]);
    for (i, &(state_off, at, back, vt)) in HANDLER_STATES.iter().enumerate() {
        let state = o.wrapping_add(state_off);
        copy_state_vbase_vt(m, state, at, back, vt, frame.wrapping_add(0xb0 * i as u32));
        if i == 4 {
            e.call(m, RIGID_BODY_CTOR, &[state.wrapping_add(0x70)]);
        }
    }
    let sort_node = o.wrapping_add(0x82c);
    e.call(m, SORT_OBJ_NODE_CTOR, &[sort_node, rider_idx as i32 as u32, 0x3ef]);
    let b = m.r32(o);
    m.w32(b, BOARDER_HANDLER_VBASE_VT);
    if in_charge == 0 {
        copy_handler_vbase_vt(m, o, b, frame.wrapping_add(0x1340));
    }
    m.w32(o.wrapping_add(0x834), BOARDER_HANDLER_VT);
    m.w32(b.wrapping_add(0x5ae0), o);
    let proxy = b.wrapping_add(0x700);
    m.w32(proxy.wrapping_add(0x84), sort_node);
    e.call(m, BROADPHASE_SET_BOX, &[proxy, b.wrapping_add(0x6e0)]);
    let b2 = m.r32(o);
    let bp = m.r32(m.r32(m.r32(GAPP) + 0x730) + 0x30);
    e.call(m, BROADPHASE_FIT_BOX, &[bp, b2.wrapping_add(0x700)]);
    o
}

/// `cBoarderHandler::~cBoarderHandler` 0x11f770(o, how; sp on entry): restores `BOARDER_HANDLER_VT` (`0x3661f0`) at
/// `o + 0x834` and `BOARDER_HANDLER_VBASE_VT` (`0x366250`, or its adjusted stack copy at `sp - 0x210` when `how == 0`)
/// at `*o`, destroys `cBxSortObjNode` (`0x17dd00`, `0`) at `o + 0x82c`, runs all 28 embedded state/motion destructors
/// in reverse order (sharing stack copy `sp - 0x2c0`), destroys `cBoarderRender` (`0x1355c8`, `0`), destroys virtual
/// base `cBoarder` (`0x116990`, `0`) on `how & 2`, and deletes `o` (`0x114f80`) on `how & 1`
pub fn boarder_handler_destruct(m: &mut Mem, e: &mut dyn FeEnv, o: u32, how: u32, sp: u32) {
    let frame = sp.wrapping_sub(0x2c0);
    m.w32(o.wrapping_add(0x834), BOARDER_HANDLER_VT);
    let b = m.r32(o);
    m.w32(b, BOARDER_HANDLER_VBASE_VT);
    if how == 0 {
        copy_handler_vbase_vt(m, o, b, frame.wrapping_add(0xb0));
    }
    e.call(m, SORT_OBJ_NODE_DTOR, &[o.wrapping_add(0x82c), 0]);
    for &(state_off, at, back, vt) in HANDLER_STATES.iter().rev() {
        copy_state_vbase_vt(m, o.wrapping_add(state_off), at, back, vt, frame);
    }
    e.call(m, BOARDER_RENDER_DTOR, &[o, 0]);
    if how & 2 != 0 {
        e.call(m, BOARDER_DTOR, &[m.r32(o), 0]);
    }
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[o]);
    }
}
