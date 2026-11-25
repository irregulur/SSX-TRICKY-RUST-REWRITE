//! The course's patch cache (`cBxCache` / `cAIFwdDiffCache`, board row E3h): `n` (+0) records of `size` (+4) bytes
//! at +0xc, with an 8-byte entry each at +8 (hits since the last tick, then last tick's hits − 1). The world keeps
//! one at `cWorld`+0x28 (`cCourseResolve`'s constructor 0x25ece8, `CollCache_Init` from `Course_LoadRenderData`
//! 0x25edf0, freed by `Course_Unload` 0x25f498) holding 0x1cd0-byte records: the patch's key (its address) at +0,
//! the VU0 patch program's output from +0x10. `cAIWorld::vf1` 0x115e28 ages the global one at 0x3464b0 each tick;
//! `World_LookupCollCache` 0x25c750 looks patches up. Every call out goes through [`CacheEnv`].

use crate::pf::Mem;

/// The calls out
pub trait CacheEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

pub const MEM_DELETE: u32 = 0x114f80;
pub const MEM_FREE: u32 = 0x14a040;
pub const EAMEM_ALLOC: u32 = 0x2ccf70;
pub const EAMEM_SET: u32 = 0x2cc808;
pub const VU0_RUN_PATCH_PROGRAM: u32 = 0x2576a8;
pub const VU0_COPY_OUT_DATA: u32 = 0x257708;
pub const VU0_GET_VI1: u32 = 0x257780;
/// The VU0 the patch program runs on
pub const VU0: u32 = 0x346480;
/// `cBxCache`'s and `cAIFwdDiffCache`'s tables
pub const VT_BXCACHE: u32 = 0x3a6518;
pub const VT_FWDDIFF: u32 = 0x3a6508;
/// The two allocations' names
pub const NAME_RECORDS: u32 = 0x3a64e8;
pub const NAME_ENTRIES: u32 = 0x3a64f8;
/// What fresh allocations are filled with
pub const FILL: u32 = 0xdead_c0ed;
/// A collision record's size
pub const COLL_RECORD: u32 = 0x1cd0;
/// The hit count's cap, and the score a free entry beats
const CAP: i32 = 2_000_000_000;

/// `cBxCache::cBxCache` 0x251ba0(c): empty, its table; `c`
pub fn construct(m: &mut Mem, c: u32) -> u32 {
    m.w32(c, 0);
    m.w32(c + 0x10, VT_BXCACHE);
    m.w32(c + 4, 0);
    m.w32(c + 0xc, 0);
    m.w32(c + 8, 0);
    c
}

/// `cBxCache::~cBxCache` 0x251bc8(c, how): its table, freed, deleted on bit 0
pub fn destruct(m: &mut Mem, e: &mut dyn CacheEnv, c: u32, how: u32) {
    m.w32(c + 0x10, VT_BXCACHE);
    free(m, e, c);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[c]);
    }
}

/// `cAIFwdDiffCache::cAIFwdDiffCache` 0x251ea8(c): a `cBxCache` with its own table; `c`
pub fn construct_fwd_diff(m: &mut Mem, c: u32) -> u32 {
    construct(m, c);
    m.w32(c + 0x10, VT_FWDDIFF);
    c
}

/// `cAIFwdDiffCache::~cAIFwdDiffCache` 0x251ee0(c, how): its table, then `cBxCache`'s destructor
pub fn destruct_fwd_diff(m: &mut Mem, e: &mut dyn CacheEnv, c: u32, how: u32) {
    m.w32(c + 0x10, VT_FWDDIFF);
    destruct(m, e, c, how);
}

/// `CacheBuffer_Init` 0x251c20(c, n, size): freed, then `n` records of `size` and `n` entries allocated (each
/// filled with 0xdeadc0ed when it came), the entries zeroed
pub fn init(m: &mut Mem, e: &mut dyn CacheEnv, c: u32, n: u32, size: u32) {
    free(m, e, c);
    let bytes = n.wrapping_mul(size);
    m.w32(c + 4, size);
    m.w32(c, n);
    let p = e.call(m, EAMEM_ALLOC, &[NAME_RECORDS, bytes, 0]);
    if p != 0 {
        e.call(m, EAMEM_SET, &[p, FILL, bytes]);
    }
    m.w32(c + 0xc, p);
    let bytes = n << 3;
    let p = e.call(m, EAMEM_ALLOC, &[NAME_ENTRIES, bytes, 0]);
    if p != 0 {
        e.call(m, EAMEM_SET, &[p, FILL, bytes]);
    }
    m.w32(c + 8, p);
    let mut i = 0i32;
    while i < m.r32(c) as i32 {
        let q = ((i as u32) << 3).wrapping_add(p);
        i += 1;
        m.w32(q + 4, 0);
        m.w32(q, 0);
    }
}

/// `CacheBuffer_Free` 0x251d18(c): the entries and records freed (when there are any), all four fields cleared
pub fn free(m: &mut Mem, e: &mut dyn CacheEnv, c: u32) {
    let p = m.r32(c + 8);
    if p != 0 {
        e.call(m, MEM_FREE, &[p]);
    }
    let p = m.r32(c + 0xc);
    if p != 0 {
        e.call(m, MEM_FREE, &[p]);
    }
    m.w32(c + 0xc, 0);
    m.w32(c, 0);
    m.w32(c + 4, 0);
    m.w32(c + 8, 0);
}

/// `PatchCache_Age` 0x251d70(c): each entry's hits become last tick's hits − 1, and are cleared
pub fn age(m: &mut Mem, c: u32) {
    let mut i = 0i32;
    if (m.r32(c) as i32) <= 0 {
        return;
    }
    let p = m.r32(c + 8);
    loop {
        let q = ((i as u32) << 3).wrapping_add(p);
        i += 1;
        let v = m.r32(q);
        m.w32(q, 0);
        m.w32(q + 4, v.wrapping_sub(1));
        if i >= m.r32(c) as i32 {
            break;
        }
    }
}

/// `CacheBuffer_Evict` 0x251db8(c): the first entry whose two counts sum to ≤ 0, else the one with the smallest
/// sum (the first of equals, none under 2000000000 → the first); its hits set to 2; its record
pub fn evict(m: &mut Mem, c: u32) -> u32 {
    let p = m.r32(c + 8);
    let n = m.r32(c) as i32;
    let base = m.r32(c + 0xc);
    let (mut best, mut pick, mut i) = (CAP, 0i32, 0i32);
    if n > 0 {
        let mut q = p;
        let mut sum = m.r32(q + 4).wrapping_add(m.r32(q)) as i32;
        while sum > 0 {
            if sum < best {
                best = sum;
                pick = i;
            }
            i += 1;
            if i >= n {
                break;
            }
            q = q.wrapping_add(8);
            sum = m.r32(q + 4).wrapping_add(m.r32(q)) as i32;
        }
    }
    let at = if i < n { i } else { pick } as u32;
    m.w32((at << 3).wrapping_add(p), 2);
    at.wrapping_mul(m.r32(c + 4)).wrapping_add(base)
}

/// `CacheBuffer_Touch` 0x251e60(c, i): entry `i` (when in range) gets a hit, up to 2000000000
pub fn touch(m: &mut Mem, c: u32, i: u32) {
    if (i as i32) < 0 || (i as i32) >= m.r32(c) as i32 {
        return;
    }
    let q = (i << 3).wrapping_add(m.r32(c + 8));
    let v = m.r32(q) as i32;
    if v < CAP {
        m.w32(q, v.wrapping_add(1) as u32);
    }
}

/// `CollCache_Init` 0x251ef8(c, n): `n` 0x1cd0-byte records, each record's key cleared
pub fn init_coll(m: &mut Mem, e: &mut dyn CacheEnv, c: u32, n: u32) {
    init(m, e, c, n, COLL_RECORD);
    let k = m.r32(c) as i32;
    let mut q = m.r32(c + 0xc);
    if k <= 0 {
        return;
    }
    for _ in 0..k {
        m.w32(q, 0);
        q = q.wrapping_add(COLL_RECORD);
    }
}

/// `CacheBuffer_Lookup` 0x251f50(c, key, out): the 0x1cd0-byte record holding `key` gets a hit; without one, the
/// evicted record takes `key`, the VU0 patch program runs on the patch (`key`+0x50), the record is cleared
/// ([`clear_entry`]) and gets VU0's output from +0x10. `*out` = the record
pub fn lookup(m: &mut Mem, e: &mut dyn CacheEnv, c: u32, key: u32, out: u32) {
    let mut q = m.r32(c + 0xc);
    let n = m.r32(c) as i32;
    let mut i = 0i32;
    while i < n {
        if m.r32(q) == key {
            touch(m, c, i as u32);
            if i < m.r32(c) as i32 {
                m.w32(out, q);
                return;
            }
            break;
        }
        i += 1;
        q = q.wrapping_add(COLL_RECORD);
    }
    let q = evict(m, c);
    m.w32(q, key);
    e.call(m, VU0_RUN_PATCH_PROGRAM, &[VU0, key.wrapping_add(0x50)]);
    clear_entry(m, q);
    e.call(m, VU0_GET_VI1, &[VU0]);
    e.call(m, VU0_COPY_OUT_DATA, &[VU0, q + 0x10]);
    m.w32(out, q);
}

/// `CacheBuffer_ClearEntry` 0x252028(c, rec): words +0, +0x10 and +0x20 of each 0x40-byte block (81 blocks from
/// +0x650, then 9 from +0x1a90) set to 0x501502f9 (1e10 as a float)
pub fn clear_entry(m: &mut Mem, rec: u32) {
    for (at, k) in [(0x650u32, 81u32), (0x1a90, 9)] {
        for j in 0..k {
            let q = rec.wrapping_add(at + j * 0x40);
            for o in [0, 0x10, 0x20] {
                m.w32(q + o, 0x5015_02f9);
            }
        }
    }
}

/// `CacheBuffer_EvictThunk` 0x2520a0(c): [`evict`]
pub fn evict_thunk(m: &mut Mem, c: u32) -> u32 {
    evict(m, c)
}
