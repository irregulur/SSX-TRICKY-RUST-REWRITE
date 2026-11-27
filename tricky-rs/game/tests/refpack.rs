//! RefPack and the codecs' front door (board row E2q) against the game in the function runner (TRICKY_ELF;
//! skipped without it): RefPack streams written here (every command, the longest copies and offsets, copies that
//! overlap what they write, literal runs, every end, both size widths with and without the extra size field)
//! unpacked by the game's `Compress_DecodeApi` 0x2c2f90 → `Compress_Decode` 0x2c2de0 → `RefPack_Decode` 0x2c31b0
//! and by `tricky_data::compress::refpack_decode`; `Compress_UnpackedSizeApi` 0x2c3190 → `Compress_UnpackedSize`
//! 0x2c2fb0 and `Compress_Decode` on every type byte, with and without the 0xfb mark.

use r5900::Runner;
use tricky_data::compress::{decode, refpack_decode, unpacked_size};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    /// 1..=n, often small or the largest.
    fn up_to(&mut self, n: u32) -> u32 {
        match self.below(4) {
            0 => n,
            1 => 1 + self.below(n.min(8)),
            _ => 1 + self.below(n),
        }
    }
}

const DECODE_API: u32 = 0x2c2f90;
const SIZE_API: u32 = 0x2c3190;
const DECODE: u32 = 0x2c2de0;

/// A RefPack stream: the header for `ty`, then random commands; returns it and what it unpacks to.
fn refpack(g: &mut Rng, ty: u8, size: u32, commands: u32) -> (Vec<u8>, Vec<u8>) {
    let mut s = vec![ty, 0xfb];
    let width = if ty & 0x80 != 0 { 4 } else { 3 };
    if ty & 1 != 0 {
        // the extra size field, skipped
        for _ in 0..width {
            s.push(g.next() as u8);
        }
    }
    s.extend(&size.to_be_bytes()[4 - width..]);
    let mut out: Vec<u8> = vec![];
    let lits = |g: &mut Rng, s: &mut Vec<u8>, out: &mut Vec<u8>, n: u32| {
        for _ in 0..n {
            let b = if g.below(2) == 0 { g.below(4) as u8 } else { g.next() as u8 };
            s.push(b);
            out.push(b);
        }
    };
    let copy = |out: &mut Vec<u8>, back: u32, n: u32| {
        let from = out.len() - back as usize;
        for i in 0..n as usize {
            out.push(out[from + i]);
        }
    };
    for _ in 0..g.below(commands) {
        let have = out.len() as u32;
        let lit = g.below(4);
        let can = have + lit;
        match g.below(5) {
            0 if can >= 1 => {
                let back = g.up_to(can.min(0x400));
                let n = 3 + g.below(8);
                s.push(((back - 1) >> 3 & 0x60) as u8 | ((n - 3) << 2) as u8 | lit as u8);
                s.push((back - 1) as u8);
                lits(g, &mut s, &mut out, lit);
                copy(&mut out, back, n);
            }
            1 if can >= 1 => {
                let back = g.up_to(can.min(0x4000));
                let n = 4 + g.below(64);
                s.push(0x80 | (n - 4) as u8);
                s.push((lit << 6 | (back - 1) >> 8) as u8);
                s.push((back - 1) as u8);
                lits(g, &mut s, &mut out, lit);
                copy(&mut out, back, n);
            }
            2 if can >= 1 => {
                let back = g.up_to(can.min(0x20000));
                let n = 5 + if g.below(3) == 0 { 1023 } else { g.below(1024) };
                let (b, m) = (back - 1, n - 5);
                s.push(0xc0 | (b >> 12 & 0x10) as u8 | (m >> 6 & 0xc) as u8 | lit as u8);
                s.push((b >> 8) as u8);
                s.push(b as u8);
                s.push(m as u8);
                lits(g, &mut s, &mut out, lit);
                copy(&mut out, back, n);
            }
            _ => {
                let k = g.below(0x1c);
                s.push(0xe0 | k as u8);
                lits(g, &mut s, &mut out, (k + 1) * 4);
            }
        }
    }
    let lit = g.below(4);
    s.push(0xfc | lit as u8);
    lits(g, &mut s, &mut out, lit);
    (s, out)
}

fn runner() -> Option<(Runner, u32, u32)> {
    let mut r = Runner::from_env()?;
    for a in [DECODE, 0x2c2fb0, 0x2c31b0, 0x2c3730, 0x2c3540, 0x2c34d8, 0x2c2d18, 0x2c2bb8, 0x2c2bf8, 0x2c27c8] {
        r.allow(a);
    }
    let src = r.mem.alloc(0x8_0000);
    let dst = r.mem.alloc(0x40_0000);
    Some((r, src, dst))
}

#[test]
fn refpack_matches_the_original() {
    let Some((mut r, src, dst)) = runner() else { return };
    let mut g = Rng(0x5265_6650_0000_0001);
    let (mut bytes, mut far) = (0usize, 0u32);
    for seq in 0..4000 {
        let ty = [0x10u8, 0x11, 0x90, 0x91][g.below(4) as usize];
        let size = if g.below(2) == 0 { g.next() } else { g.below(0x10000) };
        let size = if ty & 0x80 == 0 { size & 0xff_ffff } else { size };
        let (s, want) = refpack(&mut g, ty, size, if seq % 8 == 0 { 2000 } else { 300 });
        let mut ours = vec![];
        let n = refpack_decode(&s, &mut ours);
        assert_eq!(ours, want, "seq {seq}: ours against the stream's writer");
        assert_eq!(n, size, "seq {seq}");
        far += (want.len() > 0x20000) as u32;
        r.mem.write(src, &s).unwrap();
        r.mem.write(dst, &vec![0xcdu8; ours.len() + 16]).unwrap();
        r.cpu.set_gpr(4, src as u64);
        r.cpu.set_gpr(5, dst as u64);
        r.call(DECODE_API).unwrap_or_else(|e| panic!("seq {seq}: {e}"));
        let ret = r.cpu.gpr(2) as u32;
        let mut theirs = vec![0u8; ours.len() + 16];
        r.mem.read(dst, &mut theirs).unwrap();
        assert_eq!(ret, n, "seq {seq}: the answer");
        assert_eq!(&theirs[..ours.len()], &ours[..], "seq {seq}: the bytes");
        assert!(theirs[ours.len()..].iter().all(|&b| b == 0xcd), "seq {seq}: written past the end");
        r.cpu.set_gpr(4, src as u64);
        r.call(SIZE_API).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, unpacked_size(&s), "seq {seq}: the size");
        bytes += ours.len();
    }
    println!("{bytes} bytes unpacked, {far} streams past 128 KiB");
    assert!(far > 100);
}

#[test]
fn every_type_matches_the_original() {
    let Some((mut r, src, dst)) = runner() else { return };
    let mut g = Rng(0x5479_7065_0000_0001);
    for seq in 0..20_000u32 {
        let ty = (seq % 256) as u8;
        let mark = if g.below(4) == 0 { g.next() as u8 } else { 0xfb };
        let mut s = vec![ty, mark];
        for _ in 0..6 {
            s.push(g.next() as u8);
        }
        r.mem.write(src, &s).unwrap();
        r.cpu.set_gpr(4, src as u64);
        r.call(SIZE_API).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, unpacked_size(&s), "seq {seq}: the size of {s:02x?}");
        // the types the front door does not take (0x1e / 0x9e: the codec never registered): nothing decoded,
        // nothing written
        let taken = mark == 0xfb
            && matches!(ty & 0xfe, 0x10 | 0x90 | 0x30 | 0x32 | 0x34 | 0xb0 | 0xb2 | 0xb4 | 0x46 | 0xc6 | 0x18 | 0x1a | 0x1c | 0x98 | 0x9a | 0x9c);
        if !taken {
            assert_eq!(decode(&s), None, "seq {seq}: {s:02x?}");
            r.mem.write(dst, &[0xcd; 64]).unwrap();
            r.cpu.set_gpr(4, src as u64);
            r.cpu.set_gpr(5, dst as u64);
            r.call(DECODE).unwrap();
            assert_eq!(r.cpu.gpr(2), 0, "seq {seq}: {s:02x?}");
            let mut b = [0u8; 64];
            r.mem.read(dst, &mut b).unwrap();
            assert!(b.iter().all(|&x| x == 0xcd), "seq {seq}");
        }
    }
}
