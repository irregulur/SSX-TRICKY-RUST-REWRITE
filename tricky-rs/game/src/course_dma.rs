//! The course models' DMA resolve (board row E3i): each model's hierarchies' meshes' contexts hold a VIF DMA chain
//! (+0xc) written with chain-relative REF tags; at load the chain is rewritten in place for the PS2 (models with
//! flag 0x10 rebuilt into CNT runs and absolute REFs, the rest patched with each REF to the context's lightmap /
//! texture / matrix slots turned into a VU1 call). The world's resolver (`cCourseResolve`, the global at 0x346488)
//! and its destructor, and the visible list's construct and read, are here too. Every call out goes through
//! [`DmaEnv`].

use crate::pf::Mem;

/// The calls out
pub trait DmaEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32;
}

pub const MEM_DELETE: u32 = 0x114f80;
pub const RESOLVE_PS2_CONSTRUCT: u32 = 0x2555a8;
pub const COURSE_RESOLVE_DESTRUCT: u32 = 0x186168;
pub const CELLLIST_DESTRUCT: u32 = 0x260fc0;
pub const DEBUG_PRINT: u32 = 0x255310;
/// `EAMem_Copy`(dst, src, bytes)
pub const EAMEM_COPY: u32 = 0x2ce114;
/// `cCourseResolve`'s table
pub const VT: u32 = 0x388350;
/// The VIF code the patched REFs become: MSCAL to the micro program at 0x26 (first slot) or 0x3c (second)
const MSCAL: u32 = 0x1400_0000;
/// What a REF to the third slot becomes: a CNT of one quadword with a FLUSH-like code (as the game writes it)
const THIRD: u64 = 0x3000_0000_0500_0001;
/// A CNT tag's id
const CNT: u64 = 0x1000_0000;

fn print(m: &mut Mem, e: &mut dyn DmaEnv, s: u32) {
    e.call(m, DEBUG_PRINT, &[s]);
}

fn r64(m: &Mem, a: u32) -> u64 {
    m.r32(a) as u64 | (m.r32(a + 4) as u64) << 32
}
fn w64(m: &mut Mem, a: u32, v: u64) {
    m.w32(a, v as u32);
    m.w32(a + 4, (v >> 32) as u32);
}

/// A DMA tag's id (bits 28..30), its quadword count and its address (the high word)
fn tag(t: u64) -> (u32, u32, u32) {
    ((t as u32 >> 28) & 7, t as u32 & 0xffff, (t >> 32) as u32)
}

/// `CourseDma_TryResolveModels` 0x255df0(r, course): 4 without a course, 6 with over 4000 models, otherwise the
/// models (count +0x2c, first +0x6c) resolved when there are any; 0
pub fn try_resolve_models(m: &mut Mem, e: &mut dyn DmaEnv, r: u32, course: u32) -> u32 {
    if course == 0 {
        return 4;
    }
    let n = m.r32(course + 0x2c);
    if n as i32 >= 0xfa1 {
        return 6;
    }
    let first = m.r32(course + 0x6c);
    if first != 0 {
        resolve_models(m, e, r, n, first);
    }
    0
}

/// `CourseDma_ResolveModels` 0x255e48(r, n, model): `n` models, each the size its first word gives; 1
pub fn resolve_models(m: &mut Mem, e: &mut dyn DmaEnv, r: u32, n: u32, model: u32) -> u32 {
    print(m, e, 0x3a7570);
    let (mut n, mut p) = (n as i32, model);
    if n > 0 {
        loop {
            resolve_model(m, e, r, p);
            n = n.wrapping_sub(1);
            p = p.wrapping_add(m.r32(p));
            if n == 0 {
                break;
            }
        }
    }
    1
}

/// `CourseDma_ResolveModel` 0x255eb8(r, model): its hierarchies (count +4, first +8, the model's flags +0x10); 1
pub fn resolve_model(m: &mut Mem, e: &mut dyn DmaEnv, r: u32, model: u32) -> u32 {
    print(m, e, 0x3a7590);
    let (n, h, flags) = (m.r32(model + 4), m.r32(model + 8), m.r32(model + 0x10));
    resolve_model_hierarchies(m, e, r, n, h, flags);
    1
}

/// `CourseDma_ResolveModelHierarchies` 0x255f08(r, n, h, flags): each 0x18-byte hierarchy's three meshes (+4, +8,
/// +0xc), each once (one equal to an earlier one of the three is skipped); 1
pub fn resolve_model_hierarchies(m: &mut Mem, e: &mut dyn DmaEnv, r: u32, n: u32, h: u32, flags: u32) -> u32 {
    print(m, e, 0x3a75a8);
    let mut h = h;
    let mut i = 0i32;
    while i < n as i32 {
        print(m, e, 0x3a75d0);
        let a = m.r32(h + 4);
        if a != 0 {
            resolve_mesh(m, e, r, a, flags);
        }
        let b = m.r32(h + 8);
        if b != 0 && m.r32(h + 4) != b {
            resolve_mesh(m, e, r, b, flags);
        }
        let c = m.r32(h + 0xc);
        if c != 0 && m.r32(h + 4) != c && m.r32(h + 8) != c {
            resolve_mesh(m, e, r, c, flags);
        }
        i += 1;
        h = h.wrapping_add(0x18);
    }
    1
}

/// `CourseDma_ResolveMesh` 0x255ff0(r, mesh, flags): each of its contexts (count +0x20, the list +0x28); 1
pub fn resolve_mesh(m: &mut Mem, e: &mut dyn DmaEnv, r: u32, mesh: u32, flags: u32) -> u32 {
    print(m, e, 0x3a75f8);
    let mut p = m.r32(mesh + 0x28);
    let mut i = 0i32;
    while i < m.r32(mesh + 0x20) as i32 {
        let ctx = m.r32(p);
        i += 1;
        resolve_mesh_context(m, e, r, ctx, flags);
        p = p.wrapping_add(4);
    }
    1
}

/// `CourseDma_ResolveMeshContext` 0x256080(r, ctx, flags): the context's chain (+0xc) rebuilt (flag 0x10) or
/// patched, with its three slots' offsets (+0x14, +0x18, +0x10); then +0x10 = chain + (+0x10); 1
pub fn resolve_mesh_context(m: &mut Mem, e: &mut dyn DmaEnv, _r: u32, ctx: u32, flags: u32) -> u32 {
    print(m, e, 0x3a7610);
    let (chain, a, b, c) = (m.r32(ctx + 0xc), m.r32(ctx + 0x14), m.r32(ctx + 0x18), m.r32(ctx + 0x10));
    if flags & 0x10 != 0 {
        rebuild_chain(m, e, chain, a, b, c);
    } else {
        patch_context(m, e, chain, a, b, c);
    }
    let v = m.r32(ctx + 0xc).wrapping_add(m.r32(ctx + 0x10));
    m.w32(ctx + 0x10, v);
    1
}

/// `CourseDma_PatchContext` 0x240a98(chain, a, b, c): the chain (after its first tag) rewritten in place, up to
/// its END: CNT data kept (copied down), a REF to `a` / `b` → a quadword of 0 and MSCAL 0x26 / 0x3c, a REF to `c`
/// → 0x3000000005000001, 0 (a REF to anything else, or another tag, would loop for ever, as in the game); the END
/// and its data copied. The first tag's count = the quadwords written, its high word cleared.
pub fn patch_context(m: &mut Mem, e: &mut dyn DmaEnv, chain: u32, a: u32, b: u32, c: u32) {
    let (mut src, mut dst, mut n) = (chain, chain.wrapping_add(0x10), 0u32);
    loop {
        let (id, qwc, addr) = tag(r64(m, src));
        match id {
            1 => {
                src = src.wrapping_add(0x10);
                e.call(m, EAMEM_COPY, &[dst, src, qwc << 4]);
                dst = dst.wrapping_add(qwc << 4);
                n = n.wrapping_add(qwc);
                src = src.wrapping_add(qwc << 4);
            }
            3 if addr == a || addr == b => {
                w64(m, dst, 0);
                n = n.wrapping_add(1);
                let code = if addr == a { 0x26 } else { 0x3c };
                src = src.wrapping_add(0x10);
                w64(m, dst + 8, ((MSCAL | code) as u64) << 32);
                dst = dst.wrapping_add(0x10);
            }
            3 if addr == c => {
                n = n.wrapping_add(1);
                w64(m, dst, THIRD);
                src = src.wrapping_add(0x10);
                w64(m, dst + 8, 0);
                dst = dst.wrapping_add(0x10);
            }
            6 => {
                e.call(m, EAMEM_COPY, &[dst, src, (qwc + 1) << 4]);
                break;
            }
            // STANDIN: the game spins here for ever; we stop instead
            _ => panic!("CourseDma_PatchContext: tag {id} to {addr:#x} at {src:#x} never ends"),
        }
    }
    let first = r64(m, chain) & 0xffff_0000 | (n & 0xffff) as u64;
    w64(m, chain, first);
}

/// `CourseDma_RebuildChain` 0x240c78(chain, a, b, c): the chain rewritten in place into CNT runs, up to its END:
/// each run's data (CNT data, and a quadword of 0 and MSCAL 0x26 / 0x3c for a REF to `a` / `b`) counted into the
/// run's open tag; a REF to `c` closes the run, is written with its address made absolute (`chain` + it) and opens
/// a new run after a reserved quadword; the END closes the run when it has data and is copied with its data.
pub fn rebuild_chain(m: &mut Mem, e: &mut dyn DmaEnv, chain: u32, a: u32, b: u32, c: u32) {
    // t: the tag read, s: its data, dst: where the next is written, open: the run's tag, n: the run's quadwords
    let (mut t, mut dst, mut s, mut open, mut n) = (chain, chain.wrapping_add(0x10), chain.wrapping_add(0x10), chain, 0u32);
    let mut cur = r64(m, chain);
    loop {
        let (id, qwc, addr) = tag(cur);
        match id {
            1 => {
                e.call(m, EAMEM_COPY, &[dst, s, qwc << 4]);
                dst = dst.wrapping_add(qwc << 4);
                t = s.wrapping_add(qwc << 4);
                n = n.wrapping_add(qwc);
                cur = r64(m, t);
                s = t.wrapping_add(0x10);
            }
            3 if addr == a || addr == b => {
                w64(m, dst, 0);
                t = s;
                n = n.wrapping_add(1);
                let code = if addr == a { 0x26 } else { 0x3c };
                w64(m, dst + 8, ((MSCAL | code) as u64) << 32);
                dst = dst.wrapping_add(0x10);
                cur = r64(m, s);
                s = s.wrapping_add(0x10);
            }
            3 if addr == c => {
                w64(m, open + 8, 0);
                w64(m, open, (n as i32 as i64 as u64) | CNT);
                n = 0;
                let low = m.r32(t) as u64;
                t = s;
                w64(m, dst, (chain.wrapping_add(addr) as u64) << 32 | low);
                w64(m, dst + 8, 0);
                open = dst.wrapping_add(0x10);
                cur = r64(m, s);
                dst = dst.wrapping_add(0x20);
                s = s.wrapping_add(0x10);
            }
            6 => {
                if (n as i32) > 0 {
                    w64(m, open + 8, 0);
                    w64(m, open, n as u64 | CNT);
                }
                e.call(m, EAMEM_COPY, &[dst, t, (qwc + 1) << 4]);
                return;
            }
            // STANDIN: the game spins here for ever; we stop instead
            _ => panic!("CourseDma_RebuildChain: tag {id} to {addr:#x} at {t:#x} never ends"),
        }
    }
}

/// `cCourseResolve::cCourseResolve` 0x25ece8(r): the PS2 resolver's constructor, its table, the patch cache
/// (+0x28) and the visible list (+0x3c) made, +0x14 and +0xac8 cleared, the 0x200 words from +0x2c8 set to −1, then
/// its virtual at +0x14 of the table (`cCourseResolve_Common::vf1`); `r`
pub fn construct(m: &mut Mem, e: &mut dyn DmaEnv, r: u32) -> u32 {
    e.call(m, RESOLVE_PS2_CONSTRUCT, &[r]);
    m.w32(r + 0x10, VT);
    crate::patch_cache::construct_fwd_diff(m, r + 0x28);
    vis_construct(m, r + 0x3c);
    let vt = m.r32(r + 0x10);
    m.w32(r + 0x14, 0);
    m.w32(r + 0xac8, 0);
    for k in (0..0x200u32).rev() {
        m.w32(r + 0x2c8 + 4 * k, u32::MAX);
    }
    let me = r.wrapping_add(m.r16(vt + 0x10) as i16 as u32);
    let f = m.r32(vt + 0x14);
    e.call(m, f, &[me]);
    r
}

/// `World_Destruct` 0x25ed70(w, how): its virtual at +0x24 of the table, the cell list (+0x3c,
/// `CellList_Destruct`), the patch cache (+0x28) and the resolver (`cCourseResolve::~cCourseResolve` 0x186168)
/// destroyed; deleted on bit 0
pub fn world_destruct(m: &mut Mem, e: &mut dyn DmaEnv, w: u32, how: u32) {
    let vt = m.r32(w + 0x10);
    let me = w.wrapping_add(m.r16(vt + 0x20) as i16 as u32);
    let f = m.r32(vt + 0x24);
    e.call(m, f, &[me]);
    e.call(m, CELLLIST_DESTRUCT, &[w + 0x3c, 2]);
    let mut pe = CacheCalls(e);
    crate::patch_cache::destruct_fwd_diff(m, &mut pe, w + 0x28, 2);
    e.call(m, COURSE_RESOLVE_DESTRUCT, &[w, 2]);
    if how & 1 != 0 {
        e.call(m, MEM_DELETE, &[w]);
    }
}

/// The patch cache's calls out, through ours
struct CacheCalls<'a>(&'a mut dyn DmaEnv);
impl crate::patch_cache::CacheEnv for CacheCalls<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.0.call(m, func, args)
    }
}

/// `VisList_Construct` 0x25ec60(list): cleared (`VisList_Clear`); `list`
pub fn vis_construct(m: &mut Mem, list: u32) -> u32 {
    crate::terraindraw::vis_clear(m, list);
    list
}

/// `VisList_Get` 0x25ecb8(list, i): entry `i` (unchecked)
pub fn vis_get(m: &Mem, list: u32, i: u32) -> u32 {
    m.r32(list.wrapping_add(i << 2))
}
