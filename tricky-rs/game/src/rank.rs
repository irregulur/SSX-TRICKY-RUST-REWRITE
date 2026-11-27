//! The score ranks (board row G2e) on the game's bytes: the six ranks' table at 0x338120, 0x14 bytes a rank: +0
//! the least points (float: 0, 1001, 2501, 4001, 7501, 11501), +4..+0x10 its colour (four floats).
//!
//! Proven from the disassembly of `SLUS_203.26` (read by hand: Ghidra was not running).

use crate::pf::Mem;

/// The rank table
pub const RANKS: u32 = 0x338120;

fn copy_colour(m: &mut Mem, from: u32, to: u32) {
    for k in 0..4 {
        m.w32(to + 4 * k, m.r32(from + 4 + 4 * k));
    }
}

/// `Score_GetRankForPoints` 0x19e7a0(points, colour): the highest rank whose least points (≤, as floats; the
/// points converted from an int) the points reach, else 0; with `colour` not 0 its colour is stored there (rank
/// 0's first, so a NaN threshold leaves it)
pub fn rank_for_points(m: &mut Mem, points: i32, colour: u32) -> u32 {
    if colour != 0 {
        copy_colour(m, RANKS, colour);
    }
    let p = points as f32;
    for r in (0..6u32).rev() {
        let e = RANKS + 0x14 * r;
        if f32::from_bits(m.r32(e)) <= p {
            if colour != 0 {
                copy_colour(m, e, colour);
            }
            return r;
        }
    }
    0
}

/// `Score_GetRankColour` 0x19e840(rank, colour): rank's colour stored at `colour` (any rank: the table is read
/// past its ends as it lies)
pub fn rank_colour(m: &mut Mem, rank: u32, colour: u32) {
    copy_colour(m, RANKS.wrapping_add(rank.wrapping_mul(0x14)), colour);
}

/// `Score_GetTrickName` 0x157978(score): the trick's name at the score's +0x250
pub fn trick_name(score: u32) -> u32 {
    score.wrapping_add(0x250)
}
