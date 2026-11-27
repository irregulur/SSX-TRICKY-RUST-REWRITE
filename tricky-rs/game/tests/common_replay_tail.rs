//! The last 3 `common` functions and last 1 `replay` function (board row E1l) against `SLUS_203.26` in the
//! function runner (`TRICKY_ELF`; skipped without it):
//! - `Quat_RotateByVecToVec` 0x250728 (`tricky_game::orient::rotate_by_vec_to_vec`)
//! - `Vu0_CopyOutData` 0x257708 (`tricky_game::terrain::vu0_copy_out_data`)
//! - `Vu0_GetVi1` 0x257780 (`tricky_game::terrain::vu0_get_vi1`)
//! - `Huff_CreateHeader` 0x241bb8 (`tricky_game::replay::huff_create_header`)

use std::cell::Cell;
use std::rc::Rc;

use r5900::ps2float::Rules;
use r5900::{Mem as RMem, Runner, VU0_DATA};
use tricky_game::orient::rotate_by_vec_to_vec;
use tricky_game::pf::Mem;
use tricky_game::replay::{huff_create_header, HUFF_HEADER_SIZE};
use tricky_game::terrain::{vu0_copy_out_data, vu0_get_vi1, VU0_OUT_ADDR, VU0_OUT_BYTES};

type V4 = [f32; 4];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
    fn unit(&mut self) -> V4 {
        let v = [self.f(-1.0, 1.0), self.f(-1.0, 1.0), self.f(-1.0, 1.0) + 1e-4];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l, 0.0]
    }
    fn quat(&mut self) -> V4 {
        let v = [self.f(-1.0, 1.0), self.f(-1.0, 1.0), self.f(-1.0, 1.0), self.f(-1.0, 1.0) + 1e-4];
        let l = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.map(|x| x / l)
    }
}

fn w4(m: &mut RMem, a: u32, v: V4) {
    for k in 0..4u32 {
        m.write_f32(a + 4 * k, v[k as usize]);
    }
}
fn r4(m: &mut RMem, a: u32) -> V4 {
    [0, 1, 2, 3].map(|k| m.read_f32(a + 4 * k))
}
fn b4(v: V4) -> [u32; 4] {
    v.map(f32::to_bits)
}

#[test]
fn quat_rotate_by_vec_to_vec_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    for a in [0x250728u32, 0x250fd0, 0x250c10, 0x102e80] {
        r.allow(a);
    }
    let pq = r.mem.alloc(0x10);
    let pa = r.mem.alloc(0x10);
    let pb = r.mem.alloc(0x10);
    let mut g = Rng(0x2507_2800_0001);
    let (mut n_zero, mut n_neg, mut n_clamp) = (0usize, 0, 0);
    for case in 0..5_000u32 {
        let q = g.quat();
        let a = g.unit();
        let b = match case % 20 {
            0 => a,                                // parallel (s == 0)
            1 => [-a[0], -a[1], -a[2], 0.0],       // antiparallel (s == 0, dot < 0)
            2 => {
                // near-perpendicular (s > 0.99999 clamp)
                let up = if a[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
                let c = [a[1] * up[2] - a[2] * up[1], a[2] * up[0] - a[0] * up[2], a[0] * up[1] - a[1] * up[0]];
                let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
                [c[0] / l, c[1] / l, c[2] / l, 0.0]
            }
            _ => g.unit(),
        };
        let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let s = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        if s == 0.0 {
            n_zero += 1;
        } else {
            if s > 0.99999 {
                n_clamp += 1;
            }
            if a[0] * b[0] + a[1] * b[1] + a[2] * b[2] < 0.0 {
                n_neg += 1;
            }
        }
        w4(&mut r.mem, pq, q);
        w4(&mut r.mem, pa, a);
        w4(&mut r.mem, pb, b);
        r.cpu.set_gpr(4, pq as u64);
        r.cpu.set_gpr(5, pa as u64);
        r.cpu.set_gpr(6, pb as u64);
        r.call(0x250728).unwrap();
        let got = r4(&mut r.mem, pq);
        let ours = rotate_by_vec_to_vec(q, a, b);
        assert_eq!(b4(ours), b4(got), "Quat_RotateByVecToVec case {case}");
    }
    assert!(n_zero > 200 && n_neg > 1000 && n_clamp > 200);
}

#[test]
fn vu0_copy_out_data_and_get_vi1_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.allow(0x257708);
    r.allow(0x257780);
    let mut g = Rng(0x2577_0800_0002);

    // 1. Vu0_CopyOutData 0x257708: 500 cases with guard bytes before/after dst
    let base = 0x0100_0000u32;
    let dst = base + 0x20;
    for case in 0..500u32 {
        let mut vu0 = vec![0u8; 0x1000];
        for b in &mut vu0 {
            *b = g.next() as u8;
        }
        let mut dst_init = vec![0u8; VU0_OUT_BYTES + 0x40];
        for b in &mut dst_init {
            *b = g.next() as u8;
        }
        let mut m = Mem { regions: vec![(VU0_DATA, vu0.clone()), (base, dst_init.clone())] };
        vu0_copy_out_data(&mut m, dst);

        r.mem.write(VU0_DATA, &vu0).unwrap();
        r.mem.write(base, &dst_init).unwrap();
        r.cpu.set_gpr(4, g.next() as u64);
        r.cpu.set_gpr(5, dst as u64);
        r.call(0x257708).unwrap();
        let mut orig = vec![0u8; dst_init.len()];
        r.mem.read(base, &mut orig).unwrap();
        assert_eq!(m.regions[1].1, orig, "Vu0_CopyOutData case {case}");
        assert_eq!(&orig[0x20..0x20 + VU0_OUT_BYTES], &vu0[(VU0_OUT_ADDR - VU0_DATA) as usize..][..VU0_OUT_BYTES]);
    }

    // 2. Vu0_GetVi1 0x257780: edges + 2,000 random u16 values
    for (case, vi1) in [0u16, 1, 0x7fff, 0x8000, 0xffff]
        .into_iter()
        .chain((0..2_000).map(|_| g.next() as u16))
        .enumerate()
    {
        r.cpu.vi[1] = vi1;
        r.call(0x257780).unwrap();
        let orig = r.cpu.gpr(2) as u32;
        assert_eq!(vu0_get_vi1(vi1), orig, "Vu0_GetVi1 case {case} vi1={vi1:#x}");
    }
}

#[test]
fn huff_create_header_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.allow(0x241bb8);
    r.allow(0x2fb468); // real ELF memset
    let alloc_ret = Rc::new(Cell::new(0u32));
    let alloc_size = Rc::new(Cell::new(0u32));
    {
        let ret = alloc_ret.clone();
        let sz = alloc_size.clone();
        r.stub(0x2cc958, move |c, _| {
            sz.set(c.gpr(4) as u32);
            c.set_gpr(2, ret.get() as u64);
        });
    }

    // 1. Null allocation path
    let base = 0x0100_0000u32;
    let mut m = Mem { regions: vec![(base, vec![0xaa; 0x200])] };
    alloc_ret.set(0);
    alloc_size.set(0);
    r.call(0x241bb8).unwrap();
    let orig_null = r.cpu.gpr(2) as u32;
    let mut our_sz = 0u32;
    let our_null = huff_create_header(&mut m, &mut |_, sz| {
        our_sz = sz;
        0
    });
    assert_eq!(our_sz, alloc_size.get());
    assert_eq!(our_null, 0);
    assert_eq!(orig_null, 0);

    // 2. 200 cases at various alignments with dirty initial memory & guard bytes
    let mut g = Rng(0x241b_b800_0003);
    for case in 0..200u32 {
        let off = 0x10 + (case % 16);
        let ptr = base + off;
        let mut init = vec![0u8; 0x200];
        for b in &mut init {
            *b = g.next() as u8;
        }
        let mut m = Mem { regions: vec![(base, init.clone())] };
        let mut our_sz = 0u32;
        let ours_ret = huff_create_header(&mut m, &mut |_, sz| {
            our_sz = sz;
            ptr
        });

        r.mem.write(base, &init).unwrap();
        alloc_ret.set(ptr);
        alloc_size.set(0);
        r.call(0x241bb8).unwrap();
        let orig_ret = r.cpu.gpr(2) as u32;

        assert_eq!(our_sz, HUFF_HEADER_SIZE);
        assert_eq!(our_sz, alloc_size.get(), "Huff_CreateHeader alloc size case {case}");
        assert_eq!(ours_ret, orig_ret, "Huff_CreateHeader ret case {case}");
        let mut orig = vec![0u8; init.len()];
        r.mem.read(base, &mut orig).unwrap();
        assert_eq!(m.regions[0].1, orig, "Huff_CreateHeader mem case {case} off={off}");
    }
}
