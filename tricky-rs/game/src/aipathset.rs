//! The course's path set on the game's bytes (board row E2p): the set at 0x337fb8 as `AIP_LoadPathsFromFile`
//! 0x197da0 leaves it (+0 the AI paths' count, +4 their 0x40-byte records, +8 the event paths' count, +0xc their
//! 0x3c-byte records, +0x10 the main event path, +0x14 the start positions, +0x2c the loaded file) and its small
//! functions: finding a path's index, the getters, an event on a path, freeing the set. Checked in the function
//! runner (`tests/aipathset.rs`). The geometry on paths is `crate::aipath`.

use crate::pf::Mem;

/// The course's path set
pub const PATHS: u32 = 0x337fb8;

/// `AIP_GetPathIndex` 0x198688(set, path): the event path's place in the set (records 0x3c apart from +0xc), or
/// −1 for 0 or a path not in it
pub fn path_index(m: &Mem, set: u32, path: u32) -> i32 {
    if path != 0 {
        let n = m.r32(set + 8) as i32;
        let mut k = 0;
        while k < n {
            if (k as u32).wrapping_mul(0x3c).wrapping_add(m.r32(set + 0xc)) == path {
                return k;
            }
            k += 1;
        }
    }
    -1
}

/// `AIP_GetLineIndex` 0x1986d8(set, path): the AI path's place in the set (records 0x40 apart from +4), or −1 for 0
/// or a path not in it
pub fn line_index(m: &Mem, set: u32, path: u32) -> i32 {
    if path != 0 {
        let n = m.r32(set) as i32;
        let mut k = 0;
        while k < n {
            if m.r32(set + 4).wrapping_add((k as u32).wrapping_mul(0x40)) == path {
                return k;
            }
            k += 1;
        }
    }
    -1
}

/// `AIPathSet_GetMainPath` 0x197ef0(set): the main event path (+0x10)
pub fn main_path(m: &Mem, set: u32) -> u32 {
    m.r32(set + 0x10)
}

/// `AIPathSet_GetPath` 0x197ef8(set, k): start position `k`'s AI path (+0x14 + 4k, unchecked)
pub fn get_path(m: &Mem, set: u32, k: u32) -> u32 {
    m.r32(set.wrapping_add(k.wrapping_mul(4)).wrapping_add(0x14))
}

/// `AIPath_FindEvent` 0x1977f0(path, kind): the path's first event (16 bytes each from +4, +0 their count) of that
/// kind (as its place in the event table), or 0
pub fn find_event(m: &Mem, path: u32, kind: u32) -> u32 {
    let n = m.r32(path) as i32;
    let mut e = m.r32(path + 4);
    let mut k = 0;
    while k < n {
        k += 1;
        if m.r32(e) == kind {
            return e;
        }
        e = e.wrapping_add(0x10);
    }
    0
}

/// `AIPath_Free` 0x197e90(set): the AI paths' records (+4), the event paths' (+0xc) and the loaded file (+0x2c)
/// given back (`Mem_Free` 0x14a040, our host's: `mem_free`) when there, each pointer cleared. The counts stay.
pub fn free(m: &mut Mem, set: u32, mem_free: &mut dyn FnMut(u32)) {
    for off in [4, 0xc, 0x2c] {
        let p = m.r32(set + off);
        if p != 0 {
            mem_free(p);
        }
        m.w32(set + off, 0);
    }
}

// `cPath::vf2` 0x197ce8 and `cPath::vf3` 0x197ce0 (the base class's parameter defaults and parameter setter, which
// `cAIPath` and `cEventPath` replace) do nothing; nothing calls them.
