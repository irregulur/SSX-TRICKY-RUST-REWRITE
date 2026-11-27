//! The path set's small functions (board row E2p) against the original in the function runner (TRICKY_ELF;
//! skipped without it), on random path sets: `AIP_GetPathIndex` 0x198688, `AIP_GetLineIndex` 0x1986d8,
//! `AIPathSet_GetMainPath` 0x197ef0, `AIPathSet_GetPath` 0x197ef8, `AIPath_FindEvent` 0x1977f0 (on the game's bytes
//! and on `aipath::Path`), `AIPath_Free` 0x197e90, `cPath::vf1` 0x197b90 (`Path::fork_between`), `cPath::vf2` /
//! `vf3`, and `AIPathSet_SortByDistance` 0x197f08 (`aipath::sort_set_by_distance`). Memory and answers compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::aipath::{self, Path};
use tricky_game::aipathset;
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
    fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[self.below(of.len() as u32) as usize]
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() >> 8) as f32 / (1 << 24) as f32
    }
}

const SET: u32 = 0x0100_0000;
const AI: u32 = 0x0100_1000;
const EV: u32 = 0x0100_2000;
const EVENTS: u32 = 0x0100_3000;
const POS: u32 = 0x0100_5000;
const LIST: u32 = 0x0100_6000;

#[test]
fn the_path_set_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x198688u32, 0x1986d8, 0x197ef0, 0x197ef8, 0x1977f0, 0x197e90, 0x197b90, 0x197ce8, 0x197ce0, 0x197f08, 0x197f28, 0x197830] {
        r.allow(f);
    }
    let freed = Rc::new(RefCell::new(Vec::new()));
    let fr = freed.clone();
    r.stub(0x14a040, move |c, _| fr.borrow_mut().push(c.gpr(4) as u32));
    let mut g = Rng(0xe2b0_0000_0000_0001);
    let mut tally = std::collections::BTreeMap::<&str, (u32, u32)>::new();
    for case in 0..20000 {
        let mut m = Mem {
            regions: vec![
                (SET, (0..0x60).map(|_| g.next() as u8).collect()),
                (AI, (0..0x200).map(|_| g.next() as u8).collect()),
                (EV, (0..0x1e0).map(|_| g.next() as u8).collect()),
                (EVENTS, (0..0x2000).map(|_| g.next() as u8).collect()),
                (POS, vec![0; 0x200]),
            ],
        };
        let (nai, nev) = (g.below(9), g.below(9));
        m.w32(SET, if g.below(10) == 0 { 0x8000_0000 | g.next() } else { nai });
        m.w32(SET + 4, AI);
        m.w32(SET + 8, if g.below(10) == 0 { 0x8000_0000 | g.next() } else { nev });
        m.w32(SET + 0xc, EV);
        // the paths: their events, kinds from a few
        let mut paths = vec![];
        for k in 0..nai.max(1) {
            let at = AI + 0x40 * k;
            let n = g.below(6);
            let ev = EVENTS + 0x100 * k;
            let mut p = Path { min: [0.0; 3], max: [0.0; 3], ..Default::default() };
            for e in 0..n {
                let kind = g.pick(&[0x1f, 0x1f, 0xd, 0, 3, 0x1e]);
                let (a, b) = (g.f(-100.0, 2000.0), g.f(-100.0, 2000.0));
                let (s, t) = if g.below(5) == 0 { (b, a) } else { (a.min(b), a.max(b)) };
                let v = g.next();
                for (o, w) in [kind, v, s.to_bits(), t.to_bits()].iter().enumerate() {
                    m.w32(ev + 0x10 * e + 4 * o as u32, *w);
                }
                p.events.push((kind, v, s, t));
            }
            m.w32(at, n);
            m.w32(at + 4, ev);
            for i in 0..3 {
                let (a, b) = (g.f(-5000.0, 5000.0), g.f(-5000.0, 5000.0));
                p.min[i] = a.min(b);
                p.max[i] = a.max(b);
                m.w32(at + 0x1c + 4 * i as u32, p.min[i].to_bits());
                m.w32(at + 0x28 + 4 * i as u32, p.max[i].to_bits());
            }
            p.respawnable = g.pick(&[0, 1, 1, 1]);
            m.w32(at + 0x3c, p.respawnable);
            paths.push(p);
        }
        // records at 0 (a path 0 would match the first) and nothing to free
        for off in [4, 0xc, 0x2c] {
            if g.below(6) == 0 {
                m.w32(SET + off, 0);
            }
        }
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let op = g.below(9);
        let (k1, k2, x) = (g.below(10), g.below(10), g.next());
        let (name, args, fl): (&str, Vec<u32>, Option<(f32, f32)>) = match op {
            0 => ("GetPathIndex", vec![SET, g.pick(&[0, EV, EV + 0x3c * k1, EV + 0x3c * k2 + 4, x])], None),
            1 => ("GetLineIndex", vec![SET, g.pick(&[0, AI, AI + 0x40 * k1, AI + 0x40 * k2 + 4, x])], None),
            2 => ("GetMainPath", vec![SET], None),
            3 => ("GetPath", vec![SET, g.below(12)], None),
            4 => ("FindEvent", vec![AI + 0x40 * g.below(nai.max(1)), g.pick(&[0x1f, 0xd, 0, 3, 0x1e, 7])], None),
            5 => ("Free", vec![SET], None),
            6 => {
                let (mut a, mut b) = (g.f(-200.0, 2100.0), g.f(-200.0, 2100.0));
                // sometimes touching an event's ends exactly
                let k = g.below(nai.max(1));
                if let Some(e) = paths[k as usize].events.first() {
                    match g.below(3) {
                        0 => a = e.3,
                        1 => b = e.2,
                        _ => {}
                    }
                }
                ("Fork", vec![AI + 0x40 * k], Some((a, b)))
            }
            7 => ("Vf2Vf3", vec![g.next(), g.next(), g.next()], None),
            _ => {
                let p = [g.f(-6000.0, 6000.0), g.f(-6000.0, 6000.0), g.f(-6000.0, 6000.0), 1.0];
                for (i, v) in p.iter().enumerate() {
                    r.mem.write_f32(POS + 4 * i as u32, *v);
                    m.w32(POS + 4 * i as u32, v.to_bits());
                }
                ("Sort", vec![SET, POS, LIST, 1 + g.below(8)], None)
            }
        };
        let f = match name {
            "GetPathIndex" => 0x198688,
            "GetLineIndex" => 0x1986d8,
            "GetMainPath" => 0x197ef0,
            "GetPath" => 0x197ef8,
            "FindEvent" => 0x1977f0,
            "Free" => 0x197e90,
            "Fork" => 0x197b90,
            "Vf2Vf3" => 0x197ce8,
            _ => 0x197f08,
        };
        for (k, a) in args.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *a as u64);
        }
        if let Some((a, b)) = fl {
            r.cpu.set_f(12, a);
            r.cpu.set_f(13, b);
        }
        freed.borrow_mut().clear();
        if name == "Sort" && (nai == 0 || m.r32(SET) != nai || m.r32(SET + 4) != AI) {
            continue;
        }
        r.call(f).unwrap_or_else(|e| panic!("case {case}: {name} {args:x?}: {e}"));
        if name == "Vf2Vf3" {
            r.call(0x197ce0).unwrap();
        }
        let theirs = r.cpu.gpr(2) as u32;
        let what = format!("case {case}: {name} {args:x?}");
        let mut ours_freed = vec![];
        let ours: Option<u32> = match name {
            "GetPathIndex" => Some(aipathset::path_index(&m, args[0], args[1]) as u32),
            "GetLineIndex" => Some(aipathset::line_index(&m, args[0], args[1]) as u32),
            "GetMainPath" => Some(aipathset::main_path(&m, args[0])),
            "GetPath" => Some(aipathset::get_path(&m, args[0], args[1])),
            "FindEvent" => {
                let e = aipathset::find_event(&m, args[0], args[1]);
                let k = ((args[0] - AI) / 0x40) as usize;
                let on_struct = paths[k].find_event(args[1]).map(|i| m.r32(args[0] + 4) + 0x10 * i as u32).unwrap_or(0);
                assert_eq!(on_struct, e, "{what}: aipath::Path::find_event");
                Some(e)
            }
            "Free" => {
                aipathset::free(&mut m, args[0], &mut |p| ours_freed.push(p));
                None
            }
            "Fork" => {
                let k = ((args[0] - AI) / 0x40) as usize;
                let (a, b) = fl.unwrap();
                Some(paths[k].fork_between(a, b) as u32)
            }
            "Vf2Vf3" => None,
            _ => {
                let p = [0, 1, 2, 3].map(|i| f32::from_bits(m.r32(POS + 4 * i)));
                let got: Vec<usize> = (0..theirs as usize).map(|i| ((r.mem.read_u32(LIST + 4 * i as u32) - AI) / 0x40) as usize).collect();
                assert_eq!(got, aipath::sort_set_by_distance(&paths, p, args[3] as usize), "{what}: the sorted paths");
                None
            }
        };
        if let Some(o) = ours {
            assert_eq!(theirs, o, "{what}: the answer");
            let t = tally.entry(name).or_default();
            if o == 0 || o == u32::MAX { t.0 += 1 } else { t.1 += 1 }
        }
        for (a, v) in &m.regions {
            let mut t = vec![0u8; v.len()];
            r.mem.read(*a, &mut t).unwrap();
            assert!(&t == v, "{what}: memory at {a:#x}");
        }
        assert_eq!(*freed.borrow(), ours_freed, "{what}: what was freed");
    }
    println!("(none, some) per function: {tally:?}");
}
