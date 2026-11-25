//! `tricky_game::eamem` and `career::crc16` (board row E2i) against the original in the function runner
//! (TRICKY_ELF; skipped without it): fills, copies and moves of 0 to 0x300 bytes (and negative counts for the
//! fill and the copy) between random places of one random arena, at every alignment and overlapping either way;
//! checksums of 0 to 0x80 random bytes. The arena (and the checksum) compared after each.

use r5900::Runner;
use tricky_game::career;
use tricky_game::eamem;
use tricky_game::pf::Mem;

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
}

const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x1000;
const FUNCS: [u32; 4] = [0x2cc808, 0x2ce114, 0x2ce228, 0x2ce04c];

/// A place: on a 16-byte boundary or off it by anything
fn place(g: &mut Rng) -> u32 {
    let base = 0x20 + 0x10 * g.below(0x70);
    base + if g.below(3) == 0 { 0 } else { g.below(16) }
}
fn count(g: &mut Rng, negative: bool) -> u32 {
    match g.below(6) {
        0 => g.below(0x12),
        1 => 0x12 + g.below(0x20),
        2 => 0x40 + g.below(0x100),
        3 => g.below(0x300),
        4 if negative => g.next() | 0x8000_0000,
        _ => [0, 1, 2, 3, 4, 15, 16, 17, 18, 63, 64, 127, 128, 0x88][g.below(14) as usize],
    }
}

#[test]
fn memory_library_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    // `EAMem_Copy` runs as it is from `Mem_Move`
    for f in FUNCS {
        r.allow(f);
    }
    let mut g = Rng(0xe2c0_0000_0000_0001);
    let mut overlaps = 0;
    for case in 0..FUNCS.len() as u32 * 5000 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem { regions: vec![(ARENA, vec![0; SIZE as usize])] };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        let (d, s) = (ARENA + place(&mut g), ARENA + place(&mut g));
        // a source just before or after the destination now and then
        let s = if g.below(4) == 0 { d.wrapping_add(g.below(0x20)).wrapping_sub(0x10) } else { s };
        let v = g.next();
        let before = m.regions[0].1.clone();
        let (args, ours): (Vec<u32>, Option<u32>) = match func {
            0x2cc808 => {
                let n = count(&mut g, true);
                eamem::set(&mut m, d, v, n);
                (vec![d, v, n], None)
            }
            0x2ce114 => {
                let n = count(&mut g, true);
                eamem::copy(&mut m, d, s, n);
                (vec![d, s, n], None)
            }
            0x2ce228 => {
                let n = count(&mut g, false);
                if s < d && d < s + n {
                    overlaps += 1;
                }
                eamem::mem_move(&mut m, d, s, n);
                (vec![d, s, n], None)
            }
            _ => {
                let n = g.below(0x81);
                let data = &before[(d - ARENA) as usize..(d - ARENA + n) as usize];
                (vec![d, n], Some(career::crc16(data)))
            }
        };
        r.mem.write(ARENA, &before).unwrap();
        for (k, a) in args.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
        }
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x} {args:x?}: {e}"));
        if let Some(v) = ours {
            assert_eq!(r.cpu.gpr(2) as u32, v, "case {case} {func:#x} {args:x?}: the answer");
        }
        let mut th = vec![0u8; SIZE as usize];
        r.mem.read(ARENA, &mut th).unwrap();
        let b = &m.regions[0].1;
        if &th != b {
            let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap();
            panic!("case {case} {func:#x} {args:x?}: byte at {:#x}: theirs {:#x} ours {:#x}", ARENA + i as u32, th[i], b[i]);
        }
    }
    println!("{overlaps} backward moves");
    assert!(overlaps > 500);
}
