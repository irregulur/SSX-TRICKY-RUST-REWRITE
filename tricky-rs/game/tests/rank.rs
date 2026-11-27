//! `tricky_game::rank` (board row G2e) against the original in the function runner (TRICKY_ELF; skipped without
//! it): the game's rank table and random ones (NaN, unordered and negative thresholds), points of every size and
//! sign, with and without a colour to fill; the colour of every rank and some past the table; the trick name.

use r5900::Runner;
use tricky_game::pf::Mem;
use tricky_game::rank;

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

const OUT: u32 = 0x0100_0000;

#[test]
fn ranks_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let mut game = vec![0u8; 0x100];
    r.mem.read(rank::RANKS, &mut game).unwrap();
    let mut g = Rng(0x5261_6e6b_0000_0001);
    let mut seen = [0u32; 6];
    for seq in 0..3000 {
        let table = if seq % 3 == 0 {
            game.clone()
        } else {
            let mut t = game.clone();
            for k in 0..6 {
                let v: f32 = match g.below(5) {
                    0 => f32::NAN,
                    1 => -(g.below(1000) as f32),
                    _ => g.below(20000) as f32 + 0.5,
                };
                t[0x14 * k..0x14 * k + 4].copy_from_slice(&v.to_bits().to_le_bytes());
            }
            t
        };
        let mut m = Mem { regions: vec![(rank::RANKS, table), (OUT, (0..0x20).map(|_| g.next() as u8).collect())] };
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        for _ in 0..20 {
            let pts = match g.below(4) {
                0 => g.next() as i32,
                1 => g.below(13000) as i32,
                2 => [0, 1000, 1001, 2500, 2501, 4001, 7500, 7501, 11500, 11501, -1][g.below(11) as usize],
                _ => -(g.below(100) as i32),
            };
            let out = if g.below(3) == 0 { 0 } else { OUT };
            let ours = rank::rank_for_points(&mut m, pts, out);
            seen[ours as usize] += 1;
            r.cpu.set_gpr(4, pts as i64 as u64);
            r.cpu.set_gpr(5, out as u64);
            r.call(0x19e7a0).unwrap();
            assert_eq!(r.cpu.gpr(2) as u32, ours, "seq {seq}: {pts}");
            let k = g.below(8);
            rank::rank_colour(&mut m, k, OUT + 0x10);
            r.cpu.set_gpr(4, k as u64);
            r.cpu.set_gpr(5, (OUT + 0x10) as u64);
            r.call(0x19e840).unwrap();
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                assert_eq!(&t, v, "seq {seq}: {pts} {out:#x} colour {k}");
            }
        }
        let s = g.next();
        r.cpu.set_gpr(4, s as u64);
        r.call(0x157978).unwrap();
        assert_eq!(r.cpu.gpr(2) as u32, rank::trick_name(s));
    }
    println!("ranks {seen:?}");
    assert!(seen.iter().all(|&n| n > 1000));
}
