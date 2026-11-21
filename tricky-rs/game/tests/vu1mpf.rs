//! Character and board `.mpf` VU1 proof (board row E4d): VU1 chain `0x318080` (programs 8 and 9, 941
//! instruction pairs, `BASE 0xbe`, `OFFSET 0x16a`) driven as `cPS2GraphicsMan::vf91_1df478` 0x1df478 (regular
//! skinned meshes + facial morph targets), `vf93_1dfe30` 0x1dfe30 + `vf94_1dff88` 0x1dff88 (shadow meshes),
//! and `vf100_1df2d8` 0x1df2d8 / `vf99_1dee80` 0x1dee80 (matrix palette pre-transform `MSCAL 0x0e`) drive it
//! on real `.mpf` chains from the player's disc (`TRICKY_ELF` + `TRICKY_DISC`; skipped without them).
//!
//! Proves every part of `tricky_data::mpf`:
//! - All 27 `.mpf` files in `MDLPS2.BIG` and `BRDPS2.BIG` use only material flags `0x1` (regular), `0x11`
//!   (shadow), and `0x100` (face morph), with zero unknown VIF/MSCAL layouts.
//! - Regular multi-bone (`MSCAL 0x0` -> `0x096`) and single-bone (`MSCAL 0xa` -> `0x163`) batches: `V4-32`
//!   header, `STCYCL 3/1`, `V4-16` UV (`itof12`, `z = 14 + slot*4`, `w = 0x72 + slot*3`), `V3-16` normal
//!   (`itof15`, `/32768`), `V3-32` position, and strip ADC bits (`0xffff` on vertices 0 and 1, `0` from 2+).
//! - Shadow batches (`MSCAL 0x4` -> `0x278`, flags `0x10`): `V4-32` header (`NREG=1 XYZ2 TRISTRIP`) + `V4-32`
//!   `[pos.x, pos.y, pos.z, mat_addr]` (`mat_addr = 14 + slot*4`) matching `MpfMesh::shadow_lists`.
//! - Face morph expressions (`MSCAL 0x2` -> `0x06b` + `MSCAL 0x96`, flags `0x100`, 23 targets per face):
//!   paired double-buffered execution adding `itof15(delta_i8) * weight * scale * 2560.0` to base positions,
//!   matching `MpfMesh::morphs`.

use std::collections::BTreeMap;

use r5900::vu1::Vu1;
use r5900::Runner;
use tricky_data::big::Files;
use tricky_data::disc::Disc;
use tricky_data::mpf::{self, MpfFile, MpfPart};

const CHAIN_MPF: u32 = 0x318080;
const PALETTE_BASE: i32 = 14;
const NRM_MAT_BASE: i32 = 0x72;

fn u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

fn u16_le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(b[off..off + 2].try_into().unwrap())
}

fn i16_le(b: &[u8], off: usize) -> i16 {
    i16::from_le_bytes(b[off..off + 2].try_into().unwrap())
}

/// Measure a DMA chain's byte length from `start` through its `ret` (id=6) or `end` (id=7) tag.
fn chain_len(d: &[u8], start: usize) -> usize {
    let mut at = start;
    loop {
        let lo = u32_le(d, at);
        let qwc = (lo & 0xffff) as usize;
        let id = (lo >> 28) & 7;
        at += 16 + if id == 3 { 0 } else { qwc * 16 };
        if id == 6 || id == 7 {
            return at - start;
        }
    }
}

/// Write a VU1 quadword directly into VU1 data memory (in quadwords).
fn set_vu_qw(vu: &mut Vu1, qa: usize, v: [u32; 4]) {
    let at = (qa * 16) % vu.data.len();
    for (k, w) in v.into_iter().enumerate() {
        vu.data[at + 4 * k..at + 4 * k + 4].copy_from_slice(&w.to_le_bytes());
    }
}

fn set_vu_f4(vu: &mut Vu1, qa: usize, v: [f32; 4]) {
    set_vu_qw(vu, qa, v.map(f32::to_bits));
}

/// Deterministic per-slot translation so every vertex's weight-palette slot shows up uniquely in `XYZ2`.
fn slot_offset(slot: usize) -> [f32; 3] {
    [(slot as f32) * 3.0 + 1.0, (slot as f32) * 5.0 - 2.0, (slot as f32) * 7.0 + 4.0]
}

/// Prepare a fresh `Vu1` loaded with `SLUS_203.26`'s chain `0x318080`, linear orthographic projection
/// (scale `1e-4` so positions stay well inside the clip cube `[-1, 1]`, viewport scale `1e4` + offset `2048.0`
/// so `XYZ2` encodes `(2048 + pos + slot_offset) * 16`), unit light-direction matrix, and `n_pal` palette
/// translation matrices pre-multiplied via `MSCAL 0x0e` (as `vf100_1df2d8` / `vf99_1dee80` do).
fn setup_vu1(base: &Vu1, n_pal: usize) -> Vu1 {
    let mut vu = Vu1::default();
    vu.micro.copy_from_slice(&base.micro);
    vu.base = base.base;
    vu.offset = base.offset;
    vu.tops = base.tops;
    vu.cl = base.cl;
    vu.wl = base.wl;

    let vp_s = 1e-4f32;
    let inv_s = 1.0f32 / vp_s;
    // VU1[0..3]: projection matrix (diagonal vp_s, w = 1)
    set_vu_f4(&mut vu, 0, [vp_s, 0.0, 0.0, 0.0]);
    set_vu_f4(&mut vu, 1, [0.0, vp_s, 0.0, 0.0]);
    set_vu_f4(&mut vu, 2, [0.0, 0.0, vp_s, 0.0]);
    set_vu_f4(&mut vu, 3, [0.0, 0.0, 0.0, 1.0]);
    // VU1[4..5]: viewport scale and offset
    set_vu_f4(&mut vu, 4, [inv_s, inv_s, inv_s, 0.0]);
    set_vu_f4(&mut vu, 5, [2048.0, 2048.0, 2048.0, 0.0]);
    // VU1[7..9]: 3 light directions (identity -> dot_i = max(0, nrm_i))
    set_vu_f4(&mut vu, 7, [1.0, 0.0, 0.0, 0.0]);
    set_vu_f4(&mut vu, 8, [0.0, 1.0, 0.0, 0.0]);
    set_vu_f4(&mut vu, 9, [0.0, 0.0, 1.0, 0.0]);
    // VU1[10..13]: 3 light colors + ambient (RGB in 0..255, alpha = 128)
    set_vu_f4(&mut vu, 10, [100.0, 0.0, 0.0, 0.0]);
    set_vu_f4(&mut vu, 11, [0.0, 100.0, 0.0, 0.0]);
    set_vu_f4(&mut vu, 12, [0.0, 0.0, 100.0, 0.0]);
    set_vu_f4(&mut vu, 13, [40.0, 50.0, 60.0, 128.0]);
    // VU1[0x392..0x395]: GS setup tag (EOP, NLOOP=0) + identity envmap matrix (PS2Draw_WriteTexState prog 8)
    set_vu_qw(&mut vu, 0x392, [0x8000, 0x312e_4000, 0x412, 0]);
    set_vu_f4(&mut vu, 0x393, [1.0, 0.0, 0.0, 0.0]);
    set_vu_f4(&mut vu, 0x394, [0.0, 1.0, 0.0, 0.0]);
    set_vu_f4(&mut vu, 0x395, [0.0, 0.0, 1.0, 0.0]);

    // Upload n_pal 4x4 translation matrices at 0x0e (vf100_1df2d8 0x1df2d8) and run MSCAL 0x0e
    assert!(n_pal > 0 && n_pal <= 25, "palette size {n_pal}");
    for s in 0..n_pal {
        let [tx, ty, tz] = slot_offset(s);
        let qa = PALETTE_BASE as usize + s * 4;
        set_vu_f4(&mut vu, qa, [1.0, 0.0, 0.0, 0.0]);
        set_vu_f4(&mut vu, qa + 1, [0.0, 1.0, 0.0, 0.0]);
        set_vu_f4(&mut vu, qa + 2, [0.0, 0.0, 1.0, 0.0]);
        set_vu_f4(&mut vu, qa + 3, [tx, ty, tz, 1.0]);
    }
    set_vu_qw(&mut vu, 0x3ff, [n_pal as u32, 0, 0, 0]);
    vu.vif(&[0x1400_000e]).expect("MSCAL 0x0e matrix transform");
    // Reset double-buffer state after MSCAL 0x0e (vf100_1df2d8 uses FLUSH+MSCAL before mesh calls)
    vu.dbf = false;
    vu.tops = vu.base;
    vu.calls.clear();
    vu.kicks.clear();
    vu.gs.clear();
    vu
}

/// Build a tiny EE scratch DMA chain calling `chain_addr` and ending with `end`, then run it on `vu`.
fn run_called_chain(r: &mut Runner, vu: &mut Vu1, chain_bytes: &[u8]) {
    let base_addr = 0x0120_0000u32;
    let call_addr = 0x011f_0000u32;
    r.mem.write(base_addr, chain_bytes);
    // DMA call to base_addr, then DMA end
    let mut wrap = [0u8; 32];
    let tag0: u64 = ((base_addr as u64) << 32) | 0x5000_0000;
    let tag1: u64 = 0x7000_0000;
    wrap[0..8].copy_from_slice(&tag0.to_le_bytes());
    wrap[16..24].copy_from_slice(&tag1.to_le_bytes());
    r.mem.write(call_addr, &wrap);
    vu.run_chain(&mut r.mem, call_addr).expect("run_chain");
}

/// Extract palette table and material list record offsets from raw part data (same offsets as `SkinPart_Load`).
struct RawPart<'a> {
    data: &'a [u8],
    morph_count: usize,
    palettes: Vec<Vec<usize>>,
    /// Per material: (flags, list_chunk_start, Vec<(list_records: Vec<(r0, r1, r2)>)>)
    materials: Vec<(u32, usize, Vec<Vec<(usize, usize, usize)>>)>,
}

fn parse_raw_part<'a>(bytes: &'a [u8], entry: &mpf::PartEntry) -> RawPart<'a> {
    let data_at = u32_le(bytes, 8) as usize;
    let (doff, dsz) = entry.data();
    let d = &bytes[data_at + doff..data_at + doff + dsz];
    let mats_n = u16_le(&entry.raw, 0x40) as usize;
    let pals_n = u16_le(&entry.raw, 0x3e) as usize;
    let morph_count = u16_le(&entry.raw, 0x48) as usize;
    let pals_at = u32_le(&entry.raw, 0x2c) as usize;
    let mut palettes = Vec::with_capacity(pals_n);
    for i in 0..pals_n {
        let n = u32_le(d, pals_at + i * 8) as usize;
        let p = u32_le(d, pals_at + i * 8 + 4) as usize;
        palettes.push((0..n).map(|k| u32_le(d, p + k * 4) as usize).collect());
    }
    let mats_at = u32_le(&entry.raw, 0x20) as usize;
    let mut materials = Vec::with_capacity(mats_n);
    let mut chunk = 0usize;
    for m in 0..mats_n {
        let at = mats_at + m * 0x14;
        let flags = u32_le(d, at);
        let n_lists = u32_le(d, at + 0xc) as usize;
        let lists_at = u32_le(d, at + 0x10) as usize;
        let mut lists = Vec::with_capacity(n_lists);
        for l in 0..n_lists {
            let lp = u32_le(d, lists_at + l * 8) as usize;
            let ln = u32_le(d, lists_at + l * 8 + 4) as usize;
            let mut recs = Vec::with_capacity(ln);
            for k in 0..ln {
                recs.push((
                    u32_le(d, lp + k * 0xc) as usize,
                    u32_le(d, lp + k * 0xc + 4) as usize,
                    u32_le(d, lp + k * 0xc + 8) as usize,
                ));
            }
            lists.push(recs);
        }
        materials.push((flags, chunk, lists));
        chunk += n_lists;
    }
    RawPart { data: d, morph_count, palettes, materials }
}

/// Audit every `.mpf` file on the disc (`MDLPS2.BIG` and `BRDPS2.BIG`): verify every material flag, DMA/VIF
/// packet structure, `MSCAL` target, normal-matrix address, shadow batch layout, and facial morph chain.
#[test]
fn audit_all_mpf_chains_on_disc() {
    let Ok(disc_path) = std::env::var("TRICKY_DISC").or_else(|_| std::env::var("TRICKY_ISO")) else { return };
    let mut files = Files::new(Disc::open(&disc_path).unwrap());
    files.mount_game_archives().unwrap();

    let mut mpf_paths: Vec<String> = Vec::new();
    for r in mpf::names::RIDERS {
        mpf_paths.push(format!("|data/char/{r}_body.mpf"));
        mpf_paths.push(format!("|data/char/{r}_head.mpf"));
    }
    mpf_paths.push("|data/char/board.mpf".to_string());

    let mut flag_counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut mscal_counts: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut shadow_batches = 0usize;
    let mut shadow_verts = 0usize;
    let mut morph_chains = 0usize;
    let mut single_bone_batches = 0usize;
    let mut multi_bone_batches = 0usize;

    for path in &mpf_paths {
        let bytes = files.load(path).unwrap().unwrap_or_else(|| panic!("missing {path}"));
        let mpf_file = MpfFile::open(&bytes).unwrap();
        for entry in &mpf_file.parts {
            let part = mpf_file.part(entry).unwrap();
            let raw = parse_raw_part(&bytes, entry);
            assert_eq!(part.meshes.len(), raw.materials.len());

            for (m_idx, (flags, chunk0, lists)) in raw.materials.iter().enumerate() {
                assert!(matches!(*flags, 0x1 | 0x11 | 0x100), "{path} {}: unexpected flags {flags:#x}", entry.name);
                *flag_counts.entry(*flags).or_default() += 1;

                for (l_idx, recs) in lists.iter().enumerate() {
                    let pal = &raw.palettes[chunk0 + l_idx];
                    for (k_idx, &(r0, r1, r2)) in recs.iter().enumerate() {
                        let base_len = chain_len(raw.data, r0);
                        // Walk VIF codes in base chain
                        let mut unpacks: Vec<(u32, usize, u32, &[u8])> = Vec::new();
                        let mut at = r0;
                        loop {
                            let lo = u32_le(raw.data, at);
                            let qwc = (lo & 0xffff) as usize;
                            let id = (lo >> 28) & 7;
                            let body = &raw.data[at + 8..at + 16 + qwc * 16];
                            let mut i = 0;
                            while i + 4 <= body.len() {
                                let w = u32_le(body, i);
                                i += 4;
                                let cmd = (w >> 24) & 0x7f;
                                let num = ((w >> 16) & 0xff) as usize;
                                let imm = w & 0xffff;
                                match cmd {
                                    0x60..=0x7f => {
                                        let comps = ((cmd >> 2) & 3) as usize + 1;
                                        let bits = [32, 16, 8, 5][(cmd & 3) as usize];
                                        let n = ((bits * comps * num + 31) / 32) * 4;
                                        unpacks.push((cmd & 0x0f, num, imm, &body[i..i + n]));
                                        i += n;
                                    }
                                    0x30 | 0x31 => i += 16,
                                    0x20 => i += 4,
                                    0x14 | 0x15 | 0x17 => {
                                        *mscal_counts.entry((*flags, imm)).or_default() += 1;
                                        if *flags & 0x10 != 0 {
                                            assert_eq!(imm, 0x4, "{path} {}: shadow MSCAL {imm:#x}", entry.name);
                                            assert_eq!(unpacks.len(), 2);
                                            assert_eq!(unpacks[0].0, 0xc);
                                            assert_eq!(unpacks[1].0, 0xc);
                                            let hdr = unpacks[0].3;
                                            // GS GIFTAG: NREG=1, XYZ2 (0x4), TRISTRIP
                                            assert_eq!(u32_le(hdr, 8), 0x4);
                                            let n_strips = u32_le(hdr, 16) as usize;
                                            let n_verts = unpacks[1].1;
                                            assert_eq!(unpacks[1].2 & 0x3ff, (2 + n_strips) as u32);
                                            let sum_strip: usize = (0..n_strips).map(|s| u32_le(hdr, 32 + s * 16) as usize).sum();
                                            assert_eq!(sum_strip, n_verts);
                                            shadow_batches += 1;
                                            shadow_verts += n_verts;
                                            unpacks.clear();
                                        } else if imm == 0x0 || imm == 0xa {
                                            assert_eq!(unpacks.len(), 4);
                                            let hdr = unpacks[0].3;
                                            let uv = unpacks[1].3;
                                            let n_verts = unpacks[3].1;
                                            if imm == 0xa {
                                                single_bone_batches += 1;
                                                let s_mat = u32_le(hdr, 20) as i16;
                                                let s_nrm = u32_le(hdr, 24) as i16;
                                                for v in 0..n_verts {
                                                    assert_eq!(i16_le(uv, v * 8 + 4), s_mat);
                                                    assert_eq!(i16_le(uv, v * 8 + 6), s_nrm);
                                                }
                                            } else {
                                                multi_bone_batches += 1;
                                            }
                                            for v in 0..n_verts {
                                                let z = i16_le(uv, v * 8 + 4) as i32;
                                                let w = i16_le(uv, v * 8 + 6) as i32;
                                                assert!(z >= PALETTE_BASE && (z - PALETTE_BASE) % 4 == 0);
                                                let slot = (z - PALETTE_BASE) / 4;
                                                assert!((slot as usize) < pal.len());
                                                assert_eq!(w, NRM_MAT_BASE + slot * 3);
                                            }
                                            unpacks.clear();
                                        } else {
                                            assert!(matches!(imm, 0x6 | 0x8), "{path} {}: unexpected MSCAL {imm:#x}", entry.name);
                                            assert!(unpacks.is_empty());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            at += 16 + qwc * 16;
                            if id == 6 || id == 7 {
                                break;
                            }
                        }

                        if *flags & 0x100 != 0 {
                            // Face morph base record: ends in ret with no MSCAL, r1 == base_len, 23 morph chains
                            assert_eq!(r1, base_len, "{path} {} rec {k_idx}: r1 != base_len", entry.name);
                            assert_eq!(raw.morph_count, 23);
                            assert_eq!(unpacks.len(), 4);
                            let n_verts = unpacks[3].1;
                            assert_eq!(u32_le(unpacks[0].3, 28) as usize, n_verts);
                            for t in 0..raw.morph_count {
                                let ma = r0 + r1 + t * r2;
                                let lo = u32_le(raw.data, ma);
                                let qwc = (lo & 0xffff) as usize;
                                let id = (lo >> 28) & 7;
                                assert_eq!(id, 6, "morph chain must end with ret");
                                let body = &raw.data[ma + 8..ma + 16 + qwc * 16];
                                let mut saw_unpack = false;
                                let mut saw_mscal2 = false;
                                let mut i = 0;
                                while i + 4 <= body.len() {
                                    let w = u32_le(body, i);
                                    i += 4;
                                    let cmd = (w >> 24) & 0x7f;
                                    let num = ((w >> 16) & 0xff) as usize;
                                    let imm = w & 0xffff;
                                    match cmd {
                                        0x6a | 0x7a => {
                                            assert_eq!(imm, 0x80b6);
                                            assert_eq!(num, n_verts);
                                            saw_unpack = true;
                                            i += ((num * 3 + 3) / 4) * 4;
                                        }
                                        0x14 => {
                                            assert_eq!(imm, 0x2);
                                            saw_mscal2 = true;
                                        }
                                        0x00 | 0x01 | 0x10 => {}
                                        other => panic!("unexpected morph VIF {other:#x}"),
                                    }
                                }
                                assert!(saw_unpack && saw_mscal2);
                                morph_chains += 1;
                            }
                            assert_eq!(part.meshes[m_idx].morphs[l_idx][k_idx].len(), raw.morph_count);
                        }
                    }
                }
            }
        }
    }

    eprintln!(
        "vu1mpf audit: flags={flag_counts:?}, multi={multi_bone_batches}, single={single_bone_batches}, \
         shadow={shadow_batches} ({shadow_verts} verts), morph_chains={morph_chains}"
    );
    assert_eq!(flag_counts.get(&0x1), Some(&246));
    assert_eq!(flag_counts.get(&0x11), Some(&39));
    assert_eq!(flag_counts.get(&0x100), Some(&16));
    assert_eq!(shadow_batches, 109);
    assert_eq!(shadow_verts, 14_768);
    assert_eq!(morph_chains, 5_796);
    assert_eq!(multi_bone_batches, 1_373);
    assert_eq!(single_bone_batches, 289);
}

/// Check VU1 `XGKICK` output of a regular/face batch (`MSCAL 0x0`, `0xa`, or `0x96`) against decoded vertices
/// (optionally morphed by `morphed_pos`).
fn verify_regular_kicks(
    vu: &Vu1,
    kick_indices: &[usize],
    strips: &[Vec<mpf::MpfVertex>],
    morphed_pos: Option<&[[f32; 3]]>,
    pal: &[usize],
    label: &str,
) -> usize {
    let mut kicked_verts: Vec<([u32; 4], [u32; 4], [u32; 4])> = Vec::new();
    for &ki in kick_indices {
        let start = vu.kicks[ki].1;
        let end = vu.kicks.get(ki + 1).map_or(vu.gs.len(), |k| k.1);
        let tag = vu.gs[start];
        let nloop = (tag[0] & 0x7fff) as usize;
        let nreg = ((tag[1] >> 28) & 0xf) as usize;
        assert_eq!(nreg, 3, "{label}: expected NREG=3 (ST/RGBAQ/XYZ2)");
        assert_eq!(tag[2], 0x412, "{label}: expected REGS=ST/RGBAQ/XYZ2");
        assert_eq!(end - start, 1 + nloop * 3, "{label}: GS packet length");
        for v in 0..nloop {
            kicked_verts.push((vu.gs[start + 1 + v * 3], vu.gs[start + 1 + v * 3 + 1], vu.gs[start + 1 + v * 3 + 2]));
        }
    }

    let expected_total: usize = strips.iter().map(Vec::len).sum();
    assert_eq!(kicked_verts.len(), expected_total, "{label}: total kicked vertices");

    let mut flat = 0usize;
    for (s_idx, strip) in strips.iter().enumerate() {
        for (vi, v) in strip.iter().enumerate() {
            let (st, rgba, xyz2) = kicked_verts[flat];
            let slot = pal.iter().position(|&w| w == v.weights).unwrap();
            let [tx, ty, tz] = slot_offset(slot);
            let pos = morphed_pos.map_or(v.pos, |mp| mp[flat]);

            // ST: UV * Q where Q = 1.0 (orthographic w = 1)
            let got_u = f32::from_bits(st[0]);
            let got_v = f32::from_bits(st[1]);
            assert!((got_u - v.uv[0]).abs() < 1e-5, "{label} s{s_idx} v{vi}: u {got_u} != {}", v.uv[0]);
            assert!((got_v - v.uv[1]).abs() < 1e-5, "{label} s{s_idx} v{vi}: v {got_v} != {}", v.uv[1]);

            // RGBAQ: clamp(nrm, 0) * 100 + ambient(40, 50, 60), alpha = 128
            let exp_r = (v.nrm[0].max(0.0) * 100.0 + 40.0).min(255.0) as u32;
            let exp_g = (v.nrm[1].max(0.0) * 100.0 + 50.0).min(255.0) as u32;
            let exp_b = (v.nrm[2].max(0.0) * 100.0 + 60.0).min(255.0) as u32;
            assert!((rgba[0] as i32 - exp_r as i32).abs() <= 1, "{label} s{s_idx} v{vi}: R {} != {exp_r}", rgba[0]);
            assert!((rgba[1] as i32 - exp_g as i32).abs() <= 1, "{label} s{s_idx} v{vi}: G {} != {exp_g}", rgba[1]);
            assert!((rgba[2] as i32 - exp_b as i32).abs() <= 1, "{label} s{s_idx} v{vi}: B {} != {exp_b}", rgba[2]);
            assert_eq!(rgba[3], 128, "{label} s{s_idx} v{vi}: A");

            // XYZ2: ftoi4(2048.0 + pos + slot_offset)
            let exp_x = ((2048.0 + pos[0] + tx) * 16.0) as i32;
            let exp_y = ((2048.0 + pos[1] + ty) * 16.0) as i32;
            let exp_z = ((2048.0 + pos[2] + tz) * 16.0) as i32;
            assert!((xyz2[0] as i32 - exp_x).abs() <= 2, "{label} s{s_idx} v{vi}: X {} != {exp_x}", xyz2[0] as i32);
            assert!((xyz2[1] as i32 - exp_y).abs() <= 2, "{label} s{s_idx} v{vi}: Y {} != {exp_y}", xyz2[1] as i32);
            assert!((xyz2[2] as i32 - exp_z).abs() <= 2, "{label} s{s_idx} v{vi}: Z {} != {exp_z}", xyz2[2] as i32);

            // ADC bit: 0xffff (kick suppressed) on strip vertices 0 and 1, 0 (draw triangle) from vertex 2 onward
            let adc = xyz2[3] & 0x8000;
            if vi < 2 {
                assert_ne!(adc, 0, "{label} s{s_idx} v{vi}: expected ADC=1 on strip start");
            } else {
                assert_eq!(adc, 0, "{label} s{s_idx} v{vi}: expected ADC=0 inside strip");
            }
            flat += 1;
        }
    }
    flat
}

/// Run real `.mpf` regular (multi-bone `0x000` & single-bone `0x00a`), shadow (`0x004`), and morphed face
/// (`0x002` + `0x096`) chains through `SLUS_203.26`'s VU1 chain `0x318080`, proving every decoded field.
#[test]
fn vu1_program_8_and_9_match_mpf_decoder() {
    let Some(mut r) = Runner::from_env() else { return };
    let Ok(disc_path) = std::env::var("TRICKY_DISC").or_else(|_| std::env::var("TRICKY_ISO")) else { return };

    // Load VU1 chain 0x318080 from SLUS_203.26
    let mut base_vu = Vu1::default();
    base_vu.run_chain(&mut r.mem, CHAIN_MPF).expect("load 0x318080");
    assert_eq!(base_vu.base, 0xbe);
    assert_eq!(base_vu.offset, 0x16a);

    let mut files = Files::new(Disc::open(&disc_path).unwrap());
    files.mount_game_archives().unwrap();

    // 1. Regular meshes (multi-bone MSCAL 0x0 and single-bone MSCAL 0xa): eddie_body Body3000 + board Bx
    let mut reg_verts_checked = 0usize;
    for (path, part_name) in [("|data/char/eddie_body.mpf", "Body3000"), ("|data/char/board.mpf", "Bx")] {
        let bytes = files.load(path).unwrap().unwrap();
        let mf = MpfFile::open(&bytes).unwrap();
        let entry = mf.find(part_name).unwrap();
        let part: MpfPart = mf.part(entry).unwrap();
        let raw = parse_raw_part(&bytes, entry);

        for (m_idx, (flags, chunk0, lists)) in raw.materials.iter().enumerate() {
            if *flags != 0x1 { continue; }
            for (l_idx, recs) in lists.iter().enumerate() {
                let pal = &raw.palettes[chunk0 + l_idx];
                for (k_idx, &(r0, _, _)) in recs.iter().enumerate() {
                    let mut vu = setup_vu1(&base_vu, pal.len());
                    let len = chain_len(raw.data, r0);
                    run_called_chain(&mut r, &mut vu, &raw.data[r0..r0 + len]);
                    // Collect kicks that came from main draw (MSCAL 0x0 -> 0x000 or MSCAL 0xa -> 0x050), skipping gloss (0x030) and GS setup (0x040)
                    let mut main_kicks = Vec::new();
                    let mut ki = 0usize;
                    for &(start_pc, _) in &vu.calls {
                        // Every MSCAL in regular chains kicks once: 0x000 (0x0), 0x050 (0xa), 0x030 (0x6), 0x040 (0x8)
                        if start_pc == 0x0 || start_pc == 0xa * 8 {
                            main_kicks.push(ki);
                        }
                        ki += 1;
                    }
                    assert_eq!(ki, vu.kicks.len(), "{part_name} m{m_idx} l{l_idx}");
                    reg_verts_checked += verify_regular_kicks(
                        &vu,
                        &main_kicks,
                        &part.meshes[m_idx].lists[l_idx][k_idx],
                        None,
                        pal,
                        &format!("{part_name} m{m_idx} l{l_idx}"),
                    );
                }
            }
        }
    }
    assert!(reg_verts_checked > 1500);

    // 2. Shadow meshes (MSCAL 0x4 -> 0x278): eddie_body BodyShdw750, eddie_head HeadShdw750, board ShdwBx
    let mut shadow_verts_checked = 0usize;
    for (path, part_name) in [
        ("|data/char/eddie_body.mpf", "BodyShdw750"),
        ("|data/char/eddie_head.mpf", "HeadShdw750"),
        ("|data/char/board.mpf", "ShdwBx"),
    ] {
        let bytes = files.load(path).unwrap().unwrap();
        let mf = MpfFile::open(&bytes).unwrap();
        let entry = mf.find(part_name).unwrap();
        let part = mf.part(entry).unwrap();
        let raw = parse_raw_part(&bytes, entry);

        for (m_idx, (flags, chunk0, lists)) in raw.materials.iter().enumerate() {
            assert_ne!(*flags & 0x10, 0);
            for (l_idx, recs) in lists.iter().enumerate() {
                let pal = &raw.palettes[chunk0 + l_idx];
                for (k_idx, &(r0, _, _)) in recs.iter().enumerate() {
                    let mut vu = setup_vu1(&base_vu, pal.len());
                    let len = chain_len(raw.data, r0);
                    run_called_chain(&mut r, &mut vu, &raw.data[r0..r0 + len]);
                    assert!(!vu.kicks.is_empty());
                    assert!(vu.calls.iter().all(|&(pc, _)| pc == 0x4 * 8));

                    let mut kicked = Vec::new();
                    for ki in 0..vu.kicks.len() {
                        let start = vu.kicks[ki].1;
                        let end = vu.kicks.get(ki + 1).map_or(vu.gs.len(), |k| k.1);
                        let tag = vu.gs[start];
                        let nloop = (tag[0] & 0x7fff) as usize;
                        assert_eq!((tag[1] >> 28) & 0xf, 1, "{part_name}: shadow NREG=1");
                        assert_eq!(tag[2], 0x4, "{part_name}: shadow REGS=XYZ2");
                        assert_eq!(end - start, 1 + nloop);
                        kicked.extend_from_slice(&vu.gs[start + 1..end]);
                    }

                    let strips = &part.meshes[m_idx].shadow_lists[l_idx][k_idx];
                    let expected: usize = strips.iter().map(Vec::len).sum();
                    assert_eq!(kicked.len(), expected, "{part_name} shadow verts");

                    let mut flat = 0usize;
                    for (s_idx, strip) in strips.iter().enumerate() {
                        for (vi, v) in strip.iter().enumerate() {
                            let xyz2 = kicked[flat];
                            let slot = pal.iter().position(|&w| w == v.weights).unwrap();
                            let [tx, ty, tz] = slot_offset(slot);
                            let exp_x = ((2048.0 + v.pos[0] + tx) * 16.0) as i32;
                            let exp_y = ((2048.0 + v.pos[1] + ty) * 16.0) as i32;
                            let exp_z = ((2048.0 + v.pos[2] + tz) * 16.0) as i32;
                            assert!((xyz2[0] as i32 - exp_x).abs() <= 2, "{part_name} s{s_idx} v{vi}: X");
                            assert!((xyz2[1] as i32 - exp_y).abs() <= 2, "{part_name} s{s_idx} v{vi}: Y");
                            assert!((xyz2[2] as i32 - exp_z).abs() <= 2, "{part_name} s{s_idx} v{vi}: Z");
                            let adc = xyz2[3] & 0x8000;
                            if vi < 2 {
                                assert_ne!(adc, 0, "{part_name} s{s_idx} v{vi}: shadow ADC=1");
                            } else {
                                assert_eq!(adc, 0, "{part_name} s{s_idx} v{vi}: shadow ADC=0");
                            }
                            flat += 1;
                        }
                    }
                    shadow_verts_checked += flat;
                }
            }
        }
    }
    assert!(shadow_verts_checked > 700);

    // 3. Face morph expressions (MSCAL 0x2 -> 0x06b + MSCAL 0x96): all 18 records of eddie_head Face3000,
    // driven in pairs exactly as cPS2GraphicsMan::vf91_1df478 (0x1df478) submits them!
    let head_bytes = files.load("|data/char/eddie_head.mpf").unwrap().unwrap();
    let head_mf = MpfFile::open(&head_bytes).unwrap();
    let face_entry = head_mf.find("Face3000").unwrap();
    let face_part = head_mf.part(face_entry).unwrap();
    let face_raw = parse_raw_part(&head_bytes, face_entry);

    let part_scale = 1.0f32;
    let mut weights = [0.0f32; 23];
    weights[0] = 0.6;
    weights[1] = 0.4;
    weights[12] = 0.85;
    weights[19] = 0.5;

    let (flags, chunk0, lists) = &face_raw.materials[0];
    assert_eq!(*flags, 0x100);
    let pal = &face_raw.palettes[*chunk0];
    let recs = &lists[0];
    assert_eq!(recs.len(), 18);

    // Place the whole Face3000 part data into EE scratch RAM so DMA `call` can jump directly to any chain!
    let data_base = 0x0130_0000u32;
    r.mem.write(data_base, face_raw.data);

    let mut face_verts_checked = 0usize;
    for pair in (0..recs.len()).step_by(2) {
        let (r0_a, r1_a, r2_a) = recs[pair];
        let (r0_b, r1_b, r2_b) = recs[pair + 1];
        let mut vu = setup_vu1(&base_vu, pal.len());

        // Build exact vf91_1df478 morph submission DMA chain for pair (A, B)
        let mut pkt: Vec<u8> = Vec::new();
        let push_u64 = |buf: &mut Vec<u8>, v: u64| buf.extend_from_slice(&v.to_le_bytes());
        let push_call = |buf: &mut Vec<u8>, target: u32, qwc: u16| {
            push_u64(buf, ((target as u64) << 32) | 0x5000_0000 | (qwc as u64));
            push_u64(buf, 0);
        };
        let push_weight = |buf: &mut Vec<u8>, w: f32| {
            // UNPACK V2-32 at 0x3ff = (w, part_scale * 2560.0)
            push_u64(buf, 0x6401_03ff_0000_0000);
            let lo = w.to_bits() as u64;
            let hi = (part_scale * 2560.0f32).to_bits() as u64;
            push_u64(buf, (hi << 32) | lo);
        };

        // Call A base, then call A morph[0] (qwc=1 inline weight packet)
        push_call(&mut pkt, data_base + r0_a as u32, 0);
        push_call(&mut pkt, data_base + (r0_a + r1_a) as u32, 1);
        push_weight(&mut pkt, weights[0]);
        // Call B base, then call B morph[0] (uses same VU1[0x3ff] weight[0]), then cnt(1) nop pad
        push_call(&mut pkt, data_base + r0_b as u32, 0);
        push_call(&mut pkt, data_base + (r0_b + r1_b) as u32, 0);
        push_u64(&mut pkt, 0x1000_0001);
        push_u64(&mut pkt, 0);
        push_u64(&mut pkt, 0);
        push_u64(&mut pkt, 0);
        // Active morph targets 1..23 (weights[t] > 0.01)
        for t in 1..23 {
            if weights[t] > 0.01 {
                push_call(&mut pkt, data_base + (r0_a + r1_a + t * r2_a) as u32, 1);
                push_weight(&mut pkt, weights[t]);
                push_call(&mut pkt, data_base + (r0_b + r1_b + t * r2_b) as u32, 0);
                push_u64(&mut pkt, 0x1000_0001);
                push_u64(&mut pkt, 0);
                push_u64(&mut pkt, 0);
                push_u64(&mut pkt, 0);
            }
        }
        // Final cnt(1) with MSCAL 0x96 (draws A) and FLUSH + MSCAL 0x96 (draws B), then DMA end
        push_u64(&mut pkt, 0x1000_0001);
        push_u64(&mut pkt, 0);
        push_u64(&mut pkt, 0x1400_0096);
        push_u64(&mut pkt, 0x1100_0000_1400_0096);
        push_u64(&mut pkt, 0x7000_0000);
        push_u64(&mut pkt, 0);

        let pkt_addr = 0x011e_0000u32;
        r.mem.write(pkt_addr, &pkt);
        vu.run_chain(&mut r.mem, pkt_addr).expect("face morph pair chain");
        assert_eq!(vu.kicks.len(), 2, "pair {pair}: expected 2 kicks (A and B)");

        for (slot_in_pair, rec_idx) in [pair, pair + 1].into_iter().enumerate() {
            let strips = &face_part.meshes[0].lists[0][rec_idx];
            let targets = &face_part.meshes[0].morphs[0][rec_idx];
            let mut morphed = Vec::new();
            let mut flat = 0usize;
            for strip in strips {
                for v in strip {
                    let mut p = v.pos;
                    for t in 0..23 {
                        if t == 0 || weights[t] > 0.01 {
                            let d = targets[t][flat];
                            p[0] += d[0] * weights[t] * part_scale;
                            p[1] += d[1] * weights[t] * part_scale;
                            p[2] += d[2] * weights[t] * part_scale;
                        }
                    }
                    morphed.push(p);
                    flat += 1;
                }
            }
            face_verts_checked += verify_regular_kicks(
                &vu,
                &[slot_in_pair],
                strips,
                Some(&morphed),
                pal,
                &format!("Face3000 rec {rec_idx}"),
            );
        }
    }
    assert!(face_verts_checked > 900);
    eprintln!(
        "vu1mpf execution: verified {reg_verts_checked} regular verts, {shadow_verts_checked} shadow verts, \
         {face_verts_checked} morphed face verts through SLUS_203.26 VU1 0x318080"
    );
}
