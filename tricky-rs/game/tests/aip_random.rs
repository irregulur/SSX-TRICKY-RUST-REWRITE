//! The `.aip` / `.sop` loaders (board row E2o) on random files: `AIP_LoadPathsFromFile` 0x197da0 with
//! `AIP_LoadAIPaths` 0x198518, `AIP_LoadEventPaths` 0x1983d0, `AIPath_Load` 0x196f28, `AIPath_LoadPoint` 0x197130,
//! `AIPath_LoadEvent` 0x197170 and the path classes' parameter methods run in the function runner (TRICKY_ELF;
//! skipped without it) against `tricky_data::aip::read`. The files: sections of every kind in any order (AI paths
//! once, event paths up to twice, unknown kinds, sizes longer than the data), paths with no parameters or several
//! (repeated ids, unknown ids, shorter than a word), any point and event counts, every event type and unknown
//! ones. Compared: the set's counts, the start positions, the main path, and each path's whole record.

use r5900::Runner;
use tricky_data::aip;

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
}

/// A path; with `tail`, its last event's start and end are a section header (kind 2, no bytes), for a section
/// whose size stops 8 bytes short of its data
fn path(g: &mut Rng, out: &mut Vec<u8>, tail: bool) {
    let w = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let params = g.pick(&[0, 0, 1, 1, 2, 3, 5]);
    w(out, params);
    for _ in 0..params {
        w(out, g.pick(&[0, 0, 100, 100, 101, 101, 1, 99, 102]));
        let size = g.pick(&[4, 4, 4, 0, 2, 8]);
        w(out, size);
        for _ in 0..size {
            out.push(g.next() as u8);
        }
    }
    let (points, mut events) = (g.pick(&[0, 1, 2, 3, 7]), g.pick(&[0, 0, 1, 2, 5]));
    if tail {
        events = events.max(1);
    }
    w(out, points);
    w(out, events);
    for _ in 0..9 + 4 * points {
        w(out, g.next());
    }
    for k in 0..events {
        let code = if g.below(4) == 0 { g.pick(&[-2, 24, 99, 106, 200, 301]) } else { g.pick(&aip::EVENT_TYPES) };
        w(out, code as u32);
        w(out, g.next());
        if tail && k == events - 1 {
            w(out, 2);
            w(out, 0);
        } else {
            w(out, g.next());
            w(out, g.next());
        }
    }
}

fn file(g: &mut Rng) -> Vec<u8> {
    let w = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let mut kinds = vec![];
    if g.below(6) != 0 {
        kinds.push(0);
    }
    for _ in 0..g.pick(&[0, 1, 1, 1, 2]) {
        kinds.push(1);
    }
    for _ in 0..g.pick(&[0, 0, 0, 1, 2]) {
        kinds.push(g.pick(&[2, 3, 7, 0x100]));
    }
    for k in (1..kinds.len()).rev() {
        kinds.swap(k, g.below(k as u32 + 1) as usize);
    }
    let mut out = vec![];
    w(&mut out, 0x0a0a_0a0a);
    let count_at = out.len();
    w(&mut out, kinds.len() as u32);
    let mut count = kinds.len() as u32;
    for kind in kinds {
        let mut d = vec![];
        let short = kind < 2 && g.below(5) == 0;
        match kind {
            0 => {
                let n = g.pick(&[0, 1, 2, 3, 6]);
                let starts = g.below(8);
                w(&mut d, n);
                w(&mut d, starts);
                for _ in 0..starts {
                    let s = g.below(n.max(1) + 1);
                    w(&mut d, s);
                }
                for k in 0..n {
                    path(g, &mut d, short && k == n - 1);
                }
            }
            1 => {
                let n = g.pick(&[0, 1, 2, 3, 6]);
                w(&mut d, n);
                let main = if g.below(8) == 0 { g.next() } else { g.below(n.max(1) + 1) };
                w(&mut d, main);
                for k in 0..n {
                    path(g, &mut d, short && k == n - 1);
                }
            }
            _ => {
                for _ in 0..g.below(12) {
                    d.push(g.next() as u8);
                }
            }
        }
        let mut size = d.len() as u32;
        if short && d.len() > 16 && d[d.len() - 8..] == [2, 0, 0, 0, 0, 0, 0, 0] {
            // the next section is read from inside this one: an empty one of kind 2
            size -= 8;
            count += 1;
        } else {
            for _ in 0..g.pick(&[0, 0, 0, 4, 9]) {
                d.push(g.next() as u8);
            }
            size = d.len() as u32;
        }
        w(&mut out, kind);
        w(&mut out, size);
        out.extend(d);
    }
    out[count_at..count_at + 4].copy_from_slice(&count.to_le_bytes());
    out
}

fn f3(m: &mut r5900::Mem, at: u32) -> [u32; 3] {
    [m.read_u32(at), m.read_u32(at + 4), m.read_u32(at + 8)]
}

/// One loaded path's record (`cPath`: +0 events, +4 their address, +8 points, +0xc start, +0x18 the points'
/// address, +0x1c / +0x28 the box, +0x34 the class's table, +0x38 / +0x3c the class's parameters) against ours
fn check_path(m: &mut r5900::Mem, at: u32, p: &aip::AipPath, ai: bool, what: &str) {
    let ev = m.read_u32(at) as usize;
    let pts = m.read_u32(at + 8) as usize;
    assert_eq!((pts, ev), (p.points.len(), p.events.len()), "{what}: counts");
    assert_eq!(f3(m, at + 0xc), p.start.map(f32::to_bits), "{what}: start");
    assert_eq!(f3(m, at + 0x1c), p.v1.map(f32::to_bits), "{what}: v1");
    assert_eq!(f3(m, at + 0x28), p.v2.map(f32::to_bits), "{what}: v2");
    if ai {
        assert_eq!(m.read_u32(at + 0x34), 0x38abd8, "{what}: cAIPath's table");
        assert_eq!((m.read_u32(at + 0x38), m.read_u32(at + 0x3c)), (p.u3(), p.respawnable()), "{what}: U3, respawnable");
    } else {
        assert_eq!(m.read_u32(at + 0x38), p.param(0).unwrap_or(0), "{what}: distance parameter");
    }
    let pp = m.read_u32(at + 0x18);
    for (k, (o, d)) in p.points.iter().enumerate() {
        let a = pp + 16 * k as u32;
        assert_eq!((f3(m, a), m.read_u32(a + 12)), (o.map(f32::to_bits), d.to_bits()), "{what}: point {k}");
    }
    let ep = m.read_u32(at + 4);
    for (k, e) in p.events.iter().enumerate() {
        let a = ep + 16 * k as u32;
        let got = (m.read_u32(a), m.read_u32(a + 4), m.read_u32(a + 8), m.read_u32(a + 12));
        assert_eq!(got, (e.kind, e.value, e.start.to_bits(), e.end.to_bits()), "{what}: event {k} (code {})", e.code);
    }
}

#[test]
fn random_ai_files_load_as_the_game_loads_them() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x197da0u32, 0x198518, 0x1983d0, 0x196f28, 0x198738, 0x198720, 0x198750, 0x197130, 0x197170, 0x2cc808, 0x2ce114, 0x196700, 0x196718, 0x196990, 0x196998] {
        r.allow(f);
    }
    let buf = r.mem.alloc(0x10000);
    let heap = r.mem.alloc(0x10_0000);
    let set = r.mem.alloc(0x200);
    r.stub(0x2c8a70, move |c, _| c.set_gpr(2, buf as u64)); // File_Load
    let next = std::rc::Rc::new(std::cell::Cell::new(heap));
    let n = next.clone();
    r.stub(0x2ccf70, move |c, _| {
        // EAMem_Alloc(name, size, flags): a simple heap, 16-byte aligned
        let size = (c.gpr(5) as u32 + 15) & !15;
        let at = n.get();
        n.set(at + size);
        c.set_gpr(2, at as u64);
    });
    let mut g = Rng(0xa1b0_0000_0000_0e2f);
    let (mut paths, mut events, mut repeated, mut short) = (0, 0, 0, 0);
    for case in 0..3000 {
        let bytes = file(&mut g);
        assert!(bytes.len() < 0x10000);
        let tail: Vec<u8> = (0..16).map(|_| g.next() as u8).collect();
        r.mem.write(buf, &bytes).unwrap();
        r.mem.write(buf + bytes.len() as u32, &tail).unwrap();
        let mut full = bytes.clone();
        full.extend(&tail);
        let ours = aip::read(&full).unwrap_or_else(|e| panic!("case {case}: {e}"));
        next.set(heap);
        r.mem.write(set, &[0xcd; 0x200]).unwrap();
        r.cpu.set_gpr(4, set as u64);
        r.cpu.set_gpr(5, 0);
        r.call(0x197da0).unwrap_or_else(|e| panic!("case {case}: {e}"));
        let what = format!("case {case}");
        let m = &mut r.mem;
        // the start positions run on into the file's pointer (+0x2c) from the seventh
        if ours.start_positions.len() <= 6 {
            assert_eq!(m.read_u32(set + 0x2c), buf, "{what}: the file");
        }
        assert_eq!(m.read_u32(set) as usize, ours.ai_paths.len(), "{what}: AI paths");
        assert_eq!(m.read_u32(set + 8) as usize, ours.event_paths.len(), "{what}: event paths");
        let (ai, evp) = (m.read_u32(set + 4), m.read_u32(set + 0xc));
        for (k, s) in ours.start_positions.iter().enumerate() {
            assert_eq!(m.read_u32(set + 0x14 + 4 * k as u32), ai.wrapping_add(0x40 * s), "{what}: start position {k}");
        }
        if ours.event_paths.is_empty() && m.read_u32(set + 0x10) == 0xcdcd_cdcd {
            // no event section: the main path is never set
        } else {
            assert_eq!(m.read_u32(set + 0x10), evp.wrapping_add(ours.main_event_path.wrapping_mul(0x3c)), "{what}: main event path");
        }
        for (k, p) in ours.ai_paths.iter().enumerate() {
            check_path(m, ai + 0x40 * k as u32, p, true, &format!("{what}: AI path {k}"));
        }
        for (k, p) in ours.event_paths.iter().enumerate() {
            assert_eq!(m.read_u32(evp + 0x3c * k as u32 + 0x34), 0x38ac10, "{what}: event path {k}: cEventPath's table");
            check_path(m, evp + 0x3c * k as u32, p, false, &format!("{what}: event path {k}"));
        }
        for p in ours.ai_paths.iter().chain(&ours.event_paths) {
            paths += 1;
            events += p.events.len();
            let mut ids: Vec<u32> = p.params.iter().map(|q| q.0).collect();
            ids.sort();
            ids.dedup();
            repeated += (ids.len() < p.params.len()) as u32;
            short += p.params.iter().filter(|q| q.1.len() < 4).count();
        }
    }
    println!("{paths} paths, {events} events, {repeated} with a repeated parameter, {short} short parameters");
    assert!(paths > 5000 && repeated > 300 && short > 300);
}
