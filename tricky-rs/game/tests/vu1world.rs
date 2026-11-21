//! World meshes' VU1 program (board row E3a6): program 5 (chain 0x319e30, 818 instruction pairs, BASE 0x18,
//! OFFSET 0x1bd) driven as `cPS2GraphicsMan::DrawModel` 0x1e1f70 and `CourseDma_ResolveMeshContext` 0x256080
//! (`CourseDma_PatchContext` 0x240a98 / `CourseDma_RebuildChain` 0x240c78) drive it on real `.pbd` mesh chains
//! from the player's disc (TRICKY_ELF + TRICKY_DISC; skipped without them).
//!
//! Proves every part of E3a3's `.pbd` mesh layout (`tricky_data::pbd::context_strips` and `strip_triangles`):
//! - every `ref` packet across all 11 courses + skies is one of the context's three sentinels: `+0x14` (`MSCAL 0x26`,
//!   first batch), `+0x18` (`MSCAL 0x3c`, continuation batches), or `+0x10` (UV-scroll `STMOD 1; STROW`);
//! - the GS GIFTAG (header words 0..3) and strip/vertex counts (words 4 and 6);
//! - the masked rows (`STCYCL` CL=3 WL=1, `STMASK`, `STROW`, `STCOL`) filling UV `z = 1.0` (0x1000 in 4.12) and
//!   normal/position `w = 1.0` (0x8000 / 0x0000);
//! - the strip winding and ADC bit (VU1 sets `XYZ2.w = 0xFFFF` on vertices 0 and 1 of every strip, `0` from vertex 2
//!   onward, kicking GS `TRISTRIP` primitives).

use std::collections::BTreeMap;

use r5900::vu1::Vu1;
use r5900::Runner;
use tricky_data::big::Files;
use tricky_data::disc::Disc;
use tricky_data::pbd;

const COURSES: &[&str] = &[
    "gari", "snowdream", "elysium", "mesablanca", "merqury", "aloha",
    "pipedream", "untracked", "megaplex", "trick", "alaska",
];

fn u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

fn load_pbd(files: &mut Files, folder: &str, name: &str) -> Option<Vec<u8>> {
    let big = files.load(&format!("data/models/{folder}.big")).ok().flatten()?;
    let data = tricky_data::big::file_data(&big, &format!("Data/models/{name}.pbd"))?.to_vec();
    Some(tricky_data::compress::load(data.clone()).unwrap_or(data))
}

/// Apply `CourseDma_PatchContext` 0x240a98 (static UVs: `scroll_addr == None`) or
/// `CourseDma_RebuildChain` 0x240c78 (scrolling UVs: `scroll_addr == Some(base + scroll_off)`)
/// to a copy of the `.pbd` mesh context's DMA chain, returning the patched chain bytes.
fn resolve_chain(b: &[u8], start: usize, base: u32, p2: u32, p3: u32, p4: u32, rebuild: bool) -> (Vec<u8>, usize) {
    // Find chain byte length on disc (through `ret` id=6 or `end` id=7)
    let mut at = start;
    loop {
        let lo = u32_le(b, at);
        let qwc = (lo & 0xffff) as usize;
        let id = (lo >> 28) & 7;
        at += 16 + if id == 3 { 0 } else { qwc * 16 };
        if id == 6 || id == 7 {
            break;
        }
    }
    let len = at - start;
    let src = &b[start..start + len];
    let mut dst = vec![0u8; len];
    let w_u64 = |buf: &mut [u8], o: usize, v: u64| buf[o..o + 8].copy_from_slice(&v.to_le_bytes());
    let r_u64 = |buf: &[u8], o: usize| u64::from_le_bytes(buf[o..o + 8].try_into().unwrap());

    if !rebuild {
        // CourseDma_PatchContext 0x240a98
        let mut s = 0usize;
        let mut d = 16usize;
        let mut total_qwc = 0u64;
        // Preserve tag 0's upper bits (id=1 cnt)
        let first_lo = r_u64(src, 0);
        w_u64(&mut dst, 0, first_lo);
        w_u64(&mut dst, 8, 0);
        loop {
            let tag = r_u64(src, s);
            let qwc = (tag & 0xffff) as usize;
            let addr = (tag >> 32) as u32;
            let id = ((tag >> 28) & 7) as u32;
            match id {
                1 => {
                    dst[d..d + qwc * 16].copy_from_slice(&src[s + 16..s + 16 + qwc * 16]);
                    total_qwc += qwc as u64;
                    s += 16 + qwc * 16;
                    d += qwc * 16;
                }
                3 => {
                    if addr == p2 {
                        w_u64(&mut dst, d, 0);
                        w_u64(&mut dst, d + 8, (0x1400_0026u64) << 32);
                        total_qwc += 1;
                        d += 16;
                    } else if addr == p3 {
                        w_u64(&mut dst, d, 0);
                        w_u64(&mut dst, d + 8, (0x1400_003cu64) << 32);
                        total_qwc += 1;
                        d += 16;
                    } else if addr == p4 {
                        w_u64(&mut dst, d, 0x3000_0000_0500_0001u64);
                        w_u64(&mut dst, d + 8, 0);
                        total_qwc += 1;
                        d += 16;
                    } else {
                        panic!("unexpected ref addr {addr:#x} (p2={p2:#x} p3={p3:#x} p4={p4:#x})");
                    }
                    s += 16;
                }
                6 => {
                    let bytes = (qwc + 1) * 16;
                    dst[d..d + bytes].copy_from_slice(&src[s..s + bytes]);
                    w_u64(&mut dst, 0, (first_lo & !0xffff) | (total_qwc & 0xffff));
                    return (dst, len);
                }
                other => panic!("unexpected DMA id {other}"),
            }
        }
    } else {
        // CourseDma_RebuildChain 0x240c78
        let mut s = 0usize;
        let mut d = 16usize;
        let mut cur_cnt = 0usize;
        let mut qwc_acc = 0u64;
        loop {
            let tag = r_u64(src, s);
            let qwc = (tag & 0xffff) as usize;
            let addr = (tag >> 32) as u32;
            let id = ((tag >> 28) & 7) as u32;
            match id {
                1 => {
                    dst[d..d + qwc * 16].copy_from_slice(&src[s + 16..s + 16 + qwc * 16]);
                    qwc_acc += qwc as u64;
                    s += 16 + qwc * 16;
                    d += qwc * 16;
                }
                3 => {
                    if addr == p2 {
                        w_u64(&mut dst, d, 0);
                        w_u64(&mut dst, d + 8, (0x1400_0026u64) << 32);
                        qwc_acc += 1;
                        d += 16;
                    } else if addr == p3 {
                        w_u64(&mut dst, d, 0);
                        w_u64(&mut dst, d + 8, (0x1400_003cu64) << 32);
                        qwc_acc += 1;
                        d += 16;
                    } else if addr == p4 {
                        w_u64(&mut dst, cur_cnt, qwc_acc | 0x1000_0000);
                        w_u64(&mut dst, cur_cnt + 8, 0);
                        qwc_acc = 0;
                        let lo32 = (tag & 0xffff_ffff) as u32;
                        let new_addr = base.wrapping_add(addr);
                        w_u64(&mut dst, d, ((new_addr as u64) << 32) | lo32 as u64);
                        w_u64(&mut dst, d + 8, 0);
                        cur_cnt = d + 16;
                        d += 32;
                    } else {
                        panic!("unexpected ref addr {addr:#x}");
                    }
                    s += 16;
                }
                6 => {
                    if qwc_acc > 0 {
                        w_u64(&mut dst, cur_cnt, qwc_acc | 0x1000_0000);
                        w_u64(&mut dst, cur_cnt + 8, 0);
                    }
                    let bytes = (qwc + 1) * 16;
                    dst[d..d + bytes].copy_from_slice(&src[s..s + bytes]);
                    return (dst, len);
                }
                other => panic!("unexpected DMA id {other}"),
            }
        }
    }
}

#[test]
fn audit_all_pbd_world_mesh_vif_packets() {
    let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
    let mut files = Files::new(Disc::open(&disc_path).unwrap());

    let mut total_contexts = 0usize;
    let mut total_batches = 0usize;
    let mut ref_kinds = [0usize; 4]; // [mscal_26, mscal_3c, uv_scroll, other]
    let mut giftags: BTreeMap<[u32; 4], usize> = BTreeMap::new();
    let mut stmasks: BTreeMap<u32, usize> = BTreeMap::new();
    let mut strows: BTreeMap<[u32; 4], usize> = BTreeMap::new();
    let mut stcols: BTreeMap<[u32; 4], usize> = BTreeMap::new();
    let mut stcycls: BTreeMap<u32, usize> = BTreeMap::new();
    let mut stmods: BTreeMap<u32, usize> = BTreeMap::new();
    let mut hdr_w5: BTreeMap<u32, usize> = BTreeMap::new();
    let mut hdr_w7: BTreeMap<u32, usize> = BTreeMap::new();
    let mut len_y_vals: BTreeMap<u8, usize> = BTreeMap::new();
    let mut n_verts_eq_sum = 0usize;
    let mut n_verts_diff = 0usize;
    let mut nloop_eq_nverts = 0usize;
    let mut nloop_diff = 0usize;
    let mut model_flags: BTreeMap<u32, usize> = BTreeMap::new();

    for &course in COURSES {
        for sky in [false, true] {
            let name = if sky { format!("{course}_sky") } else { course.to_string() };
            let Some(b) = load_pbd(&mut files, course, &name) else { continue };
            let pbd = pbd::Pbd::parse(&b).unwrap();
            let vif_base = pbd.header.offset(0x84) as usize;
            for model in &pbd.models {
                *model_flags.entry(model.head[4]).or_default() += 1;
                for mesh in &model.meshes {
                    for cx in &mesh.contexts {
                        total_contexts += 1;
                        let c_off = cx.offset as usize;
                        let p4 = u32_le(&b, c_off + 0x10);
                        let p2 = u32_le(&b, c_off + 0x14);
                        let p3 = u32_le(&b, c_off + 0x18);
                        let start = vif_base + cx.words[3] as usize;

                        let mut at = start;
                        let mut unpacks: Vec<(u32, u32, usize, &[u8])> = Vec::new();
                        loop {
                            let lo = u32_le(&b, at);
                            let hi = u32_le(&b, at + 4);
                            let qwc = (lo & 0xffff) as usize;
                            let id = (lo >> 28) & 7;
                            if id == 3 {
                                if hi == p2 { ref_kinds[0] += 1; }
                                else if hi == p3 { ref_kinds[1] += 1; }
                                else if hi == p4 { ref_kinds[2] += 1; }
                                else { ref_kinds[3] += 1; }
                                at += 16;
                                continue;
                            }
                            let body = &b[at + 8..at + 16 + qwc * 16];
                            let mut i = 0;
                            while i + 4 <= body.len() {
                                let w = u32_le(body, i);
                                i += 4;
                                let cmd = (w >> 24) & 0x7f;
                                let num = ((w >> 16) & 0xff) as usize;
                                let imm = w & 0xffff;
                                match cmd {
                                    0x00 => {}
                                    0x01 => { *stcycls.entry(imm).or_default() += 1; }
                                    0x05 => { *stmods.entry(imm).or_default() += 1; }
                                    0x20 => { *stmasks.entry(u32_le(body, i)).or_default() += 1; i += 4; }
                                    0x30 => {
                                        let r = [u32_le(body, i), u32_le(body, i+4), u32_le(body, i+8), u32_le(body, i+12)];
                                        *strows.entry(r).or_default() += 1;
                                        i += 16;
                                    }
                                    0x31 => {
                                        let c = [u32_le(body, i), u32_le(body, i+4), u32_le(body, i+8), u32_le(body, i+12)];
                                        *stcols.entry(c).or_default() += 1;
                                        i += 16;
                                    }
                                    0x60..=0x7f => {
                                        let comps = (((cmd >> 2) & 3) + 1) as usize;
                                        let bits = [32usize, 16, 8, 5][(cmd & 3) as usize];
                                        let n = (bits * comps * num).div_ceil(32) * 4;
                                        unpacks.push((cmd, imm, num, &body[i..i + n]));
                                        i += n;
                                    }
                                    other => panic!("{name} cx {c_off:#x}: unexpected VIF cmd {other:#x}"),
                                }
                            }
                            at += 16 + qwc * 16;
                            if id == 6 || id == 7 { break; }
                        }
                        assert_eq!(unpacks.len() % 5, 0);
                        for batch in unpacks.chunks_exact(5) {
                            total_batches += 1;
                            let hdr = batch[0].3;
                            let lens = batch[1].3;
                            let pos = batch[4].3;
                            // Zero out NLOOP (low 15 bits of word 0) to group GIFTAG templates
                            let mut gt = [u32_le(hdr, 0), u32_le(hdr, 4), u32_le(hdr, 8), u32_le(hdr, 12)];
                            let nloop = (gt[0] & 0x7fff) as usize;
                            gt[0] &= !0x7fff;
                            *giftags.entry(gt).or_default() += 1;
                            let n_strips = u32_le(hdr, 16) as usize;
                            *hdr_w5.entry(u32_le(hdr, 20)).or_default() += 1;
                            let n_verts = u32_le(hdr, 24) as usize;
                            *hdr_w7.entry(u32_le(hdr, 28)).or_default() += 1;
                            let count = pos.len() / 6;
                            if n_verts == count { n_verts_eq_sum += 1; } else { n_verts_diff += 1; }
                            if nloop == count { nloop_eq_nverts += 1; } else { nloop_diff += 1; }
                            for s in 0..n_strips {
                                *len_y_vals.entry(lens[2 * s + 1]).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    println!("Audited {total_contexts} contexts, {total_batches} batches across 11 courses + skies (model+0x10 flags: {:?})", model_flags.iter().map(|(k,v)| (format!("{k:#x}"), v)).collect::<Vec<_>>());
    println!("ref kinds [MSCAL 0x26, MSCAL 0x3c, UV scroll, OTHER]: {ref_kinds:?}");
    println!("GIFTAG templates (NLOOP masked): {:?}", giftags.iter().map(|(k,v)| (k.map(|w| format!("{w:#010x}")), v)).collect::<Vec<_>>());
    println!("STMASKs: {:?}", stmasks.iter().map(|(k,v)| (format!("{k:#010x}"), v)).collect::<Vec<_>>());
    println!("STROWs:  {:?}", strows.iter().map(|(k,v)| (k.map(|w| format!("{w:#010x}")), v)).collect::<Vec<_>>());
    println!("STCOLs:  {:?}", stcols.iter().map(|(k,v)| (k.map(|w| format!("{w:#010x}")), v)).collect::<Vec<_>>());
    println!("STCYCLs: {:?}", stcycls.iter().map(|(k,v)| (format!("CL={} WL={}", k & 0xff, (k >> 8) & 0xff), v)).collect::<Vec<_>>());
    println!("STMODs:  {stmods:?}");
    println!("hdr w5: {hdr_w5:?}, hdr w7: {hdr_w7:?}, V2-8 y bytes: {len_y_vals:?}");
    println!("n_verts==pos_count: {n_verts_eq_sum} eq / {n_verts_diff} diff; NLOOP==pos_count: {nloop_eq_nverts} eq / {nloop_diff} diff");

    assert_eq!(ref_kinds[3], 0, "no unknown ref tags exist");
    assert_eq!(n_verts_diff, 0, "hdr word 6 always equals batch vertex count");
    assert_eq!(nloop_diff, 0, "GIFTAG NLOOP always equals batch vertex count");
}

#[test]
fn real_pbd_meshes_run_through_vu1_program_5() {
    let (Some(mut r), Ok(disc_path)) = (Runner::from_env(), std::env::var("TRICKY_DISC")) else { return };
    let mut files = Files::new(Disc::open(&disc_path).unwrap());
    let b = load_pbd(&mut files, "elysium", "elysium").unwrap();
    let parsed = pbd::Pbd::parse(&b).unwrap();
    let vif_base = parsed.header.offset(0x84) as usize;

    // Load VU1 program 5 (0x319e30) once
    let mut base_vu = Vu1::default();
    base_vu.run_chain(&mut r.mem, 0x319e30).unwrap();
    assert_eq!(base_vu.base, 0x18);
    assert_eq!(base_vu.offset, 0x1bd);

    // Put identity View*Proj at VU1[0..3], viewport scale (1,1,1,0) at VU1[4], offset (2048,2048,1000,0) at VU1[5]
    // so every vertex with small world coords lands cleanly inside the frustum without clipping!
    let set_vu_qw = |vu: &mut Vu1, qw: usize, vals: [f32; 4]| {
        for k in 0..4 {
            vu.data[qw * 16 + k * 4..qw * 16 + k * 4 + 4].copy_from_slice(&vals[k].to_le_bytes());
        }
    };
    // Tiny View*Proj scale (1/1_000_000) so even km-scale meshes stay strictly inside the clip volume (-1..1)
    let vp_s = 1.0 / 1_000_000.0;
    set_vu_qw(&mut base_vu, 0, [vp_s, 0.0, 0.0, 0.0]);
    set_vu_qw(&mut base_vu, 1, [0.0, vp_s, 0.0, 0.0]);
    set_vu_qw(&mut base_vu, 2, [0.0, 0.0, vp_s, 0.0]);
    set_vu_qw(&mut base_vu, 3, [0.0, 0.0, 0.0, 1.0]);
    // Viewport scale & offset (w = 0 so ftoi4.xyzw writes w = 0 -> ADC = 0 on vertices >= 2!)
    let (sc, off) = (256.0f32, 2048.0f32);
    set_vu_qw(&mut base_vu, 4, [sc, sc, sc, 0.0]);
    set_vu_qw(&mut base_vu, 5, [off, off, off, 0.0]);

    let chain_buf = r.mem.alloc(0x4_0000);
    let draw_pkt = r.mem.alloc(0x200);

    let mut tested_contexts = 0usize;
    let mut tested_strips = 0usize;
    let mut tested_verts = 0usize;

    for model in &parsed.models {
        let scale = [0, 1, 2].map(|k| f32::from_bits(model.head[6 + k]));
        for mesh in &model.meshes {
            for cx in &mesh.contexts {
                let c_off = cx.offset as usize;
                let p4 = u32_le(&b, c_off + 0x10);
                let p2 = u32_le(&b, c_off + 0x14);
                let p3 = u32_le(&b, c_off + 0x18);
                let start = vif_base + cx.words[3] as usize;
                let strips = pbd::context_strips(&b, start, scale).unwrap();
                if strips.is_empty() { continue; }

                // Test BOTH static (CourseDma_PatchContext) and scrolling (CourseDma_RebuildChain, du=0.25, dv=-0.5)
                for &scroll in &[false, true] {
                    let (patched, _) = resolve_chain(&b, start, chain_buf, p2, p3, p4, scroll);
                    r.mem.write(chain_buf, &patched).unwrap();
                    let (du, dv) = if scroll { (0.25f32, -0.5f32) } else { (0.0, 0.0) };
                    if scroll {
                        // DrawModel 0x1e1f70 writes STMOD 1; STROW (int)(du*4096), (int)(dv*4096) at chain_buf + p4
                        // (the 16-byte buffer placed immediately after the chain's `ret` tag; R2=0x1000 and R3=0x1000
                        // come from the first two words of the next `cnt` packet!)
                        let sb = chain_buf + p4;
                        r.mem.write_u32(sb + 0, 0x0500_0001);
                        r.mem.write_u32(sb + 4, 0x3000_0000);
                        r.mem.write_u32(sb + 8, (du * 4096.0) as i32 as u32);
                        r.mem.write_u32(sb + 12, (dv * 4096.0) as i32 as u32);
                    }

                    // Build DrawModel's 0xf0-byte submission packet:
                    // cnt 12 -> UNPACK V4-32 11 QWs to TOPS+0 (0x6c0b8000):
                    //   0..3: diag(scale.x, scale.y, scale.z, 1)
                    //   4..6: light dirs = identity (so n·L = (nx, ny, nz))
                    //   7..9: light cols = (100,0,0,0), (0,100,0,0), (0,0,100,0)
                    //   10:   ambient = (50, 50, 50, 128)
                    // call chain_buf
                    // end 0
                    let mut pkt = vec![0u8; 0x100];
                    let w32 = |buf: &mut [u8], o: usize, v: u32| buf[o..o + 4].copy_from_slice(&v.to_le_bytes());
                    let wf = |buf: &mut [u8], o: usize, v: f32| w32(buf, o, v.to_bits());
                    w32(&mut pkt, 0x00, 0x1000_000c); // cnt 12
                    w32(&mut pkt, 0x1c, 0x6c0b_8000); // UNPACK V4-32 num=11 addr=0 flg=1
                    // Model matrix scaled by vertex scale (rows 0..3 at +0x20..+0x5f)
                    wf(&mut pkt, 0x20, scale[0]);
                    wf(&mut pkt, 0x34, scale[1]);
                    wf(&mut pkt, 0x48, scale[2]);
                    wf(&mut pkt, 0x5c, 1.0);
                    // Light directions (3 columns = identity at +0x60..+0x8f)
                    wf(&mut pkt, 0x60, 1.0);
                    wf(&mut pkt, 0x74, 1.0);
                    wf(&mut pkt, 0x88, 1.0);
                    // Light colours (3 at +0x90..+0xbf) + ambient (+0xc0..+0xcf)
                    wf(&mut pkt, 0x90, 100.0);
                    wf(&mut pkt, 0xa4, 100.0);
                    wf(&mut pkt, 0xb8, 100.0);
                    wf(&mut pkt, 0xc0, 50.0);
                    wf(&mut pkt, 0xc4, 50.0);
                    wf(&mut pkt, 0xc8, 50.0);
                    wf(&mut pkt, 0xcc, 128.0);
                    // call chain_buf
                    w32(&mut pkt, 0xd0, 0x5000_0000);
                    w32(&mut pkt, 0xd4, chain_buf);
                    // end 0
                    w32(&mut pkt, 0xe0, 0x7000_0000);
                    r.mem.write(draw_pkt, &pkt).unwrap();

                    let mut vu = Vu1::default();
                    vu.micro = base_vu.micro.clone();
                    vu.data = base_vu.data.clone();
                    vu.base = base_vu.base;
                    vu.offset = base_vu.offset;
                    vu.tops = base_vu.tops;
                    vu.cl = 4;
                    vu.wl = 4;

                    vu.run_chain(&mut r.mem, draw_pkt).unwrap_or_else(|e| panic!("cx {c_off:#x} scroll={scroll}: {e}"));

                    // Collect all vertices kicked across all batches (XGKICKs) in this context
                    let mut gs_verts: Vec<([f32; 3], [u32; 4], [u32; 4])> = Vec::new();
                    for (ki, &(_vu_at, gs_start)) in vu.kicks.iter().enumerate() {
                        let gs_end = vu.kicks.get(ki + 1).map_or(vu.gs.len(), |x| x.1);
                        let giftag = vu.gs[gs_start];
                        let nloop = (giftag[0] & 0x7fff) as usize;
                        assert_eq!(gs_end - gs_start, 1 + nloop * 3, "each vertex is ST + RGBAQ + XYZ2");
                        for v in 0..nloop {
                            let stq = vu.gs[gs_start + 1 + v * 3];
                            let rgba = vu.gs[gs_start + 1 + v * 3 + 1];
                            let xyz2 = vu.gs[gs_start + 1 + v * 3 + 2];
                            let st = [f32::from_bits(stq[0]), f32::from_bits(stq[1]), f32::from_bits(stq[2])];
                            gs_verts.push((st, rgba, xyz2));
                        }
                    }

                    let expected_verts: usize = strips.iter().map(|s| s.len()).sum();
                    assert_eq!(gs_verts.len(), expected_verts, "vertex count mismatch cx {c_off:#x}");

                    // Verify EVERY vertex against pbd::context_strips: ST, RGBA, XYZ2, AND ADC bit 15!
                    let mut idx = 0;
                    for strip in &strips {
                        for (vi, v) in strip.iter().enumerate() {
                            let (st, rgba, xyz2) = gs_verts[idx];
                            idx += 1;

                            // 1. ADC bit (bit 15 of XYZ2.w): 1 on vertices 0 and 1, 0 on vertices >= 2!
                            let adc = (xyz2[3] >> 15) & 1;
                            assert_eq!(adc, if vi < 2 { 1 } else { 0 }, "ADC bit mismatch cx {c_off:#x} strip vert {vi}");

                            // 2. Position: screen = off + pos * vp_s * sc (12.4 fixed-point)
                            for axis in 0..3 {
                                let want = ((off + v.pos[axis] * vp_s * sc) * 16.0) as i32;
                                let got = xyz2[axis] as i32;
                                assert!((got - want).abs() <= 1, "pos axis {axis} got {got} want {want}");
                            }

                            // 3. UV + Q: Q = 1.0 (since W = 1.0), S = u + du, T = v + dv
                            assert!((st[2] - 1.0).abs() < 1e-4, "Q={}", st[2]);
                            assert!((st[0] - (v.uv[0] + du)).abs() < 1e-3, "S={} want {} (scroll={scroll})", st[0], v.uv[0] + du);
                            assert!((st[1] - (v.uv[1] + dv)).abs() < 1e-3, "T={} want {} (scroll={scroll})", st[1], v.uv[1] + dv);

                            // 4. Lighting from normal: 50 + 100 * max(0, n[c]), clamped to 255, alpha = 128
                            for c in 0..3 {
                                let want = (50.0 + 100.0 * v.nrm[c].max(0.0)).min(255.0) as u32;
                                assert!((rgba[c] as i32 - want as i32).abs() <= 1, "col[{c}] got {} want {want}", rgba[c]);
                            }
                            assert_eq!(rgba[3], 128);
                        }
                        if !scroll { tested_strips += 1; }
                    }
                    if !scroll {
                        tested_contexts += 1;
                        tested_verts += expected_verts;
                    }
                }
            }
        }
    }
    println!("VU1 program 5 verified {tested_contexts} real Elysium contexts, {tested_strips} strips, {tested_verts} vertices (both static and UV-scrolled)!");
    assert!(tested_verts > 5000);
}
