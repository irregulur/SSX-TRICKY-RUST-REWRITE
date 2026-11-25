//! `tricky_game::course_lookup` (board row E3f) against the original in the function runner (TRICKY_ELF; skipped
//! without it): a random course (record tables, a hash block with six sorted tables, sometimes missing) behind the
//! world, every lookup run on both sides with keys taken from the tables or not, the answers and memory compared.

use r5900::Runner;
use tricky_game::course_lookup::{self as cl, Hash};
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
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
}

const WORLDS: (u32, u32) = (0x346000, 0x1000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x4000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const COURSE: u32 = ARENA + 0x100;
const BLOCK: u32 = ARENA + 0x300;
const TABLES: u32 = ARENA + 0x400;
const RES: u32 = ARENA + 0x1000;
const OUT: u32 = ARENA + 0x1100;
const RECS: u32 = ARENA + 0x1400;
const KIND: u32 = ARENA + 0x1800;

const KINDS: [(Hash, u32, u32); 6] = [
    (Hash::Patch, 0x254c28, 0x254c60),
    (Hash::Instance, 0x254d08, 0x254d40),
    (Hash::Model, 0x254c98, 0x254cd0),
    (Hash::Light, 0x254bb8, 0x254bf0),
    (Hash::Spline, 0x254de8, 0x254e20),
    (Hash::CamAnim, 0x254d78, 0x254db0),
];
const FINDS: [(Hash, u32); 6] = [
    (Hash::Patch, 0x254fb8),
    (Hash::Instance, 0x2550c8),
    (Hash::Model, 0x255040),
    (Hash::Light, 0x254f30),
    (Hash::Spline, 0x2551d8),
    (Hash::CamAnim, 0x255150),
];

fn any(g: &mut Rng) -> u32 {
    let (a, b) = (g.next(), g.below(40));
    g.pick(&[a, b])
}

#[test]
fn course_lookups_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let mut funcs = vec![0x254a50u32, 0x254a80, 0x254b20, 0x254ba8, 0x254ed8, 0x255260, 0x254e58, 0x254e98, 0x255310];
    funcs.extend(KINDS.iter().flat_map(|k| [k.1, k.2]));
    funcs.extend(FINDS.iter().map(|f| f.1));
    for f in &funcs {
        r.allow(*f);
    }
    let mut g = Rng(0xc0_25e0_0000_0001);
    let mut found = 0;
    for case in 0..funcs.len() as u32 * 500 {
        let func = funcs[(case % funcs.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (WORLDS.0, vec![0; WORLDS.1 as usize]),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        m.w32(cl::WORLD + 4, COURSE);
        for at in [0x10u32, 0x1c, 0x20] {
            let v = g.next();
            m.w32(COURSE + at, g.pick(&[0, 1, 5, 20, u32::MAX, v]));
        }
        m.w32(COURSE + 0x80, g.pick(&[BLOCK, BLOCK, BLOCK, 0]));
        // the six tables: sorted hashes (some repeated), a record word each
        let mut keys = vec![];
        let mut per: Vec<Vec<u32>> = vec![];
        for (k, (h, _, _)) in KINDS.iter().enumerate() {
            let t = TABLES + 0x100 * k as u32;
            let n = g.pick(&[0, 1, 2, 3, 7, 16, 31]);
            let mut hs: Vec<u32> = (0..n).map(|_| any(&mut g)).collect();
            hs.sort();
            for (i, v) in hs.iter().enumerate() {
                m.w32(t + 8 * i as u32, *v);
            }
            keys.extend(&hs);
            per.push(hs.clone());
            m.w32(BLOCK + h.at(), n);
            m.w32(BLOCK + h.at() + 4, g.pick(&[t, t, t, 0]));
        }
        // the resolver's index tables
        m.w32(RES + 4, COURSE);
        m.w32(RES + 8, OUT);
        m.w32(RES + 0xc, OUT + 0x80);
        for (cnt, first) in [(0x18u32, 0x54u32), (0x28, 0x64)] {
            let n = g.pick(&[0, 1, 3, 6, u32::MAX]);
            m.w32(COURSE + cnt, n);
            let base = RECS + if cnt == 0x18 { 0 } else { 0x200 };
            m.w32(COURSE + first, base);
            let mut p = base;
            for _ in 0..6 {
                let len = g.below(6);
                m.w32(p, len);
                p += 4 * len + 4;
            }
        }
        // a find's key mostly from its own table
        let own = FINDS.iter().position(|f| f.1 == func).map(|i| KINDS.iter().position(|k| k.0 == FINDS[i].0).unwrap());
        let pool = match own {
            Some(k) if !per[k].is_empty() => per[k].clone(),
            _ => keys.clone(),
        };
        let key = if pool.is_empty() || g.below(4) == 0 { any(&mut g) } else { g.pick(&pool) };
        let n = { let v = g.next(); g.pick(&[0, 1, 4, 19, 20, 21, u32::MAX, v]) };
        let (lo, hi) = (g.pick(&[0u32, 0, 1, 5, -3i32 as u32, -10i32 as u32]), g.pick(&[u32::MAX, 0, 3, 15, 30]));
        let tbl = TABLES + 0x100 * g.below(6);
        let before = m.regions.clone();
        let w = cl::WORLD;
        let (args, ours): ([u32; 4], Option<u32>) = match func {
            0x254a50 => ([w, n, 0, 0], Some(cl::particle_instance(&m, w, n))),
            0x254a80 => ([w, n, 0, 0], Some(cl::light(&m, w, n))),
            0x254b20 => ([w, n, 0, 0], Some(cl::spline_segment(&m, w, n))),
            0x254ba8 => ([w, 0, 0, 0], Some(cl::hash_tables(&m, w))),
            0x254ed8 => ([lo, hi, key, tbl], Some(cl::hash_bsearch(&m, lo, hi, key, tbl))),
            0x255260 => ([w, key, KIND, 0], Some(cl::find_object(&mut m, w, key, KIND))),
            0x254e58 => ([RES, 0, 0, 0], Some(cl::index_table_08(&mut m, RES))),
            0x254e98 => ([RES, 0, 0, 0], Some(cl::index_table_0c(&mut m, RES))),
            0x255310 => {
                cl::debug_print();
                ([0x3a6f10, 1, 2, 3], None)
            }
            f => {
                if let Some(k) = KINDS.iter().find(|k| k.1 == f) {
                    ([w, 0, 0, 0], Some(cl::hash_count(&m, w, k.0)))
                } else if let Some(k) = KINDS.iter().find(|k| k.2 == f) {
                    ([w, 0, 0, 0], Some(cl::hash_table(&m, w, k.0)))
                } else {
                    let k = FINDS.iter().find(|k| k.1 == f).unwrap();
                    let v = cl::find(&m, w, k.0, key);
                    found += (v != 0) as u32;
                    ([w, key, 0, 0], Some(v))
                }
            }
        };
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        for (k, v) in args.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *v as i32 as i64 as u64);
        }
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if let Some(v) = ours {
            assert_eq!(r.cpu.gpr(2) as u32, v, "case {case} {func:#x} {args:x?}: the answer");
        }
        for (at, b) in &m.regions {
            if *at == STACK.0 {
                continue;
            }
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            assert!(&th == b, "case {case} {func:#x}: memory at {at:#x}");
        }
    }
    println!("{found} found");
    assert!(found > 500);
}
