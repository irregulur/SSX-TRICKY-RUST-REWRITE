//! The course's lookups (board row E3f): the world (0x346488) keeps the resolved course at +4; its records by number
//! (particle instances, lights, spline segments), its hash tables (each sorted by name hash, 8 bytes an entry: the
//! hash, the record) and the binary search over them, the lookups by name hash, the resolver's two index tables, and
//! the loader's debug print (which prints nothing).

use crate::pf::Mem;

/// The world, whose +4 is the resolved course
pub const WORLD: u32 = 0x346488;

/// The kinds of the course's hash tables: where the hash block (course +0x80) keeps each one's count and table
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hash {
    Patch,
    Instance,
    Model,
    Light,
    Spline,
    CamAnim,
}
impl Hash {
    /// The count's offset in the hash block (the table follows it)
    pub fn at(self) -> u32 {
        match self {
            Hash::Patch => 4,
            Hash::Instance => 0xc,
            Hash::Model => 0x14,
            Hash::Light => 0x1c,
            Hash::Spline => 0x24,
            Hash::CamAnim => 0x2c,
        }
    }
}

/// A record by number: `n`'s record (`size` bytes each) from the course's table at `base`, 0 when `n` is not below
/// the count at `count` (signed)
fn record(m: &Mem, w: u32, n: u32, count: u32, base: u32, size: u32) -> u32 {
    let c = m.r32(w + 4);
    if (n as i32) < (m.r32(c + count) as i32) {
        m.r32(c + base).wrapping_add(n.wrapping_mul(size))
    } else {
        0
    }
}

/// `Course_GetParticleInstance` 0x254a50(w, n): 0x70 each, count +0x10, table +0x4c
pub fn particle_instance(m: &Mem, w: u32, n: u32) -> u32 {
    record(m, w, n, 0x10, 0x4c, 0x70)
}

/// `Course_GetLight` 0x254a80(w, n): 0x5c each, count +0x1c, table +0x58
pub fn light(m: &Mem, w: u32, n: u32) -> u32 {
    record(m, w, n, 0x1c, 0x58, 0x5c)
}

/// `Course_GetSplineSegment` 0x254b20(w, n): 0x28 each, count +0x20, table +0x5c
pub fn spline_segment(m: &Mem, w: u32, n: u32) -> u32 {
    record(m, w, n, 0x20, 0x5c, 0x28)
}

/// `Course_GetHashTables` 0x254ba8(w): the course's hash block (+0x80)
pub fn hash_tables(m: &Mem, w: u32) -> u32 {
    m.r32(m.r32(w + 4) + 0x80)
}

/// The hash counts (`Course_PatchHashCount` 0x254c28, `_InstanceHashCount` 0x254d08, `_ModelHashCount` 0x254c98,
/// `_LightHashCount` 0x254bb8, `_SplineHashCount` 0x254de8, `_CamAnimHashCount` 0x254d78) and tables (the same
/// names with `Table`, 0x38 bytes after each): the hash block's word, 0 without a block
pub fn hash_count(m: &Mem, w: u32, k: Hash) -> u32 {
    let b = hash_tables(m, w);
    if b == 0 {
        return 0;
    }
    m.r32(hash_tables(m, w) + k.at())
}
pub fn hash_table(m: &Mem, w: u32, k: Hash) -> u32 {
    let b = hash_tables(m, w);
    if b == 0 {
        return 0;
    }
    m.r32(hash_tables(m, w) + k.at() + 4)
}

/// `Course_HashBsearch` 0x254ed8(lo, hi, key, table): the index of `key` among the entries `lo..=hi` of `table`
/// (8 bytes each, sorted by hash, unsigned), -1 when absent
pub fn hash_bsearch(m: &Mem, lo: u32, hi: u32, key: u32, table: u32) -> u32 {
    let (mut lo, mut hi) = (lo, hi);
    while (hi.wrapping_sub(lo) as i32) >= 0 {
        let mid = (hi.wrapping_add(lo) as i32 >> 1) as u32;
        let e = m.r32((mid << 3).wrapping_add(table));
        if e == key {
            return mid;
        }
        if e < key {
            lo = mid.wrapping_add(1);
        } else {
            hi = mid.wrapping_sub(1);
        }
    }
    u32::MAX
}

/// The lookups by name hash: `Course_FindPatchByHash` 0x254fb8, `_FindInstanceByHash` 0x2550c8,
/// `_FindModelByHash` 0x255040, `_FindLightByHash` 0x254f30, `_FindSplineByHash` 0x2551d8, `_FindCamAnimByHash`
/// 0x255150 (w, hash): the record of the entry with `hash`, 0 without a table or the hash
pub fn find(m: &Mem, w: u32, k: Hash, hash: u32) -> u32 {
    if hash_table(m, w, k) == 0 {
        return 0;
    }
    let hi = hash_count(m, w, k).wrapping_sub(1);
    let t = hash_table(m, w, k);
    let i = hash_bsearch(m, 0, hi, hash, t);
    if (i as i32) < 0 {
        return 0;
    }
    m.r32((i << 3).wrapping_add(hash_table(m, w, k)) + 4)
}

/// `Course_FindObjectByHash` 0x255260(w, hash, kind): the record named `hash` among the patches (kind 1), instances
/// (2), lights (4), splines (5) and camera animations (6), in that order; kind -1 and 0 when none (or without a
/// spline table)
pub fn find_object(m: &mut Mem, w: u32, hash: u32, kind: u32) -> u32 {
    let mut found = (u32::MAX, 0);
    if hash_table(m, w, Hash::Spline) != 0 {
        for (k, n) in [(Hash::Patch, 1), (Hash::Instance, 2), (Hash::Light, 4), (Hash::Spline, 5), (Hash::CamAnim, 6)] {
            let r = find(m, w, k, hash);
            if r != 0 {
                found = (n, r);
                break;
            }
        }
    }
    m.w32(kind, found.0);
    found.1
}

/// `CourseResolve_IndexTable08` 0x254e58 / `_IndexTable0C` 0x254e98(r): the resolver's (`r`) table at +8 / +0xc
/// filled with the address of each of the course's records counted at +0x18 / +0x28 and starting at +0x54 / +0x64,
/// each record a count of words then the words; 1
pub fn index_table(m: &mut Mem, r: u32, out_at: u32, count_at: u32, first_at: u32) -> u32 {
    let c = m.r32(r + 4);
    let mut out = m.r32(r + out_at);
    let mut n = m.r32(c + count_at);
    let mut p = m.r32(c + first_at);
    if (n as i32) > 0 {
        loop {
            let len = m.r32(p);
            n = n.wrapping_sub(1);
            m.w32(out, p);
            out = out.wrapping_add(4);
            p = p.wrapping_add((len << 2).wrapping_add(4));
            if n == 0 {
                break;
            }
        }
    }
    1
}
pub fn index_table_08(m: &mut Mem, r: u32) -> u32 {
    index_table(m, r, 8, 0x18, 0x54)
}
pub fn index_table_0c(m: &mut Mem, r: u32) -> u32 {
    index_table(m, r, 0xc, 0x28, 0x64)
}

/// `Course_DebugPrint` 0x255310(format, ...): spills its arguments and prints nothing
pub fn debug_print() {}
