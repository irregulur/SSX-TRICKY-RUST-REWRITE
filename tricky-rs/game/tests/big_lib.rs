//! The BIG library in `tricky_data::big` (board row E2h) against the original in the function runner
//! (TRICKY_ELF; skipped without it): random `C0FB`, `BIGF` and neither directories (near misses among them) (0 to 20 entries, names of mixed
//! case, directory sizes that end early, mid-entry or late), each looked up by number (in and past the count) and
//! by name (present in another case, absent, or prefixes), with and without the offset and size words. The
//! original's `stricmp` and `strlen` run as they are. Its answers and the words it writes are compared with ours.

use r5900::Runner;
use tricky_data::big;

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
const SIZE: usize = 0x1000;
const NAME: u32 = 0x0100_2000;
const OUT: [u32; 2] = [0x0100_3000, 0x0100_3004];
/// `BIG_FindEntry`'s size word
const SIZE_WORD: u32 = 0x3bb960;

fn random_name(g: &mut Rng) -> Vec<u8> {
    let n = 1 + g.below(12);
    (0..n)
        .map(|_| {
            let c = b"abcdefgxyz_./0123"[g.below(17) as usize];
            if g.below(3) == 0 { c.to_ascii_uppercase() } else { c }
        })
        .collect()
}

/// A random archive's first `SIZE` bytes and its names
fn archive(g: &mut Rng) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut b: Vec<u8> = (0..SIZE).map(|_| g.next() as u8).collect();
    let kind = g.below(5);
    let width = if kind < 2 { 3 } else { 4 };
    let mut p = if kind < 2 { 6 } else { 0x10 };
    let count = g.below(21);
    let mut names = vec![];
    for _ in 0..count {
        let name = random_name(g);
        for k in 0..2 * width {
            b[p + k] = g.next() as u8;
        }
        if g.below(8) == 0 {
            for k in 0..width {
                b[p + k] = 0;
            }
        }
        let at = p + 2 * width;
        b[at..at + name.len()].copy_from_slice(&name);
        b[at + name.len()] = 0;
        p = at + name.len() + 1;
        names.push(name);
    }
    let end = match g.below(4) {
        0 => p.saturating_sub(g.below(12) as usize),
        1 => p + g.below(8) as usize,
        _ => p,
    } as u32;
    match kind {
        0 | 1 => {
            b[0] = 0xc0;
            b[1] = 0xfb;
            let e = end.saturating_sub(4);
            b[2] = (e >> 8) as u8;
            b[3] = e as u8;
            b[4] = (count >> 8) as u8;
            b[5] = count as u8;
        }
        2 | 3 => {
            b[..4].copy_from_slice(b"BIGF");
            b[8..12].copy_from_slice(&count.to_be_bytes());
            b[0xc..0x10].copy_from_slice(&end.to_be_bytes());
        }
        _ => {
            if b[0] == 0xc0 {
                b[0] = 0;
            }
            // near misses: `BIG` and another letter, `c0 fa`
            match g.below(3) {
                0 => b[..4].copy_from_slice(b"BIGf"),
                1 => b[..2].copy_from_slice(&[0xc0, 0xfa]),
                _ => {}
            }
        }
    }
    // the rest past the directory stays random but every name there ends
    b[SIZE - 1] = 0;
    (b, names)
}

fn set(r: &mut Runner, args: &[u32]) {
    for (k, v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, *v as i32 as i64 as u64);
    }
}
fn ret(r: &Runner) -> u32 {
    r.cpu.gpr(2) as u32
}

#[test]
fn big_library_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x2cbc88, 0x2cbd08, 0x2cbdc0, 0x2cc0d8, 0x2cc1b0, 0x2fcc98, 0x2fd260] {
        r.allow(f);
    }
    let mut g = Rng(0xe2b0_0000_0000_0001);
    let mut found = [0u32; 2];
    for case in 0..3000 {
        let (b, names) = archive(&mut g);
        r.mem.write(ARENA, &b).unwrap();
        set(&mut r, &[ARENA]);
        r.call(0x2cbc88).unwrap();
        assert_eq!(ret(&r), big::identify(&b), "case {case}: BIG_Identify");
        set(&mut r, &[ARENA]);
        r.call(0x2cbd08).unwrap();
        assert_eq!(ret(&r), big::header_size(&b) as u32, "case {case}: BIG_HeaderSize");
        set(&mut r, &[ARENA]);
        r.call(0x2cc1b0).unwrap();
        assert_eq!(ret(&r), big::file_count(&b), "case {case}: BIG_GetFileCount");
        for _ in 0..6 {
            // a name: one of the archive's (case changed), a prefix of one, a new one, or none
            let name: Option<Vec<u8>> = match g.below(5) {
                0 if !names.is_empty() => {
                    let n = &names[g.below(names.len() as u32) as usize];
                    Some(n.iter().map(|c| if g.below(2) == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }).collect())
                }
                1 if !names.is_empty() => {
                    let n = &names[g.below(names.len() as u32) as usize];
                    Some(n[..g.below(n.len() as u32) as usize].to_vec())
                }
                2 | 3 => Some(random_name(&mut g)),
                _ => None,
            };
            let index = g.below(names.len() as u32 + 3);
            let outs = [if g.below(4) != 0 { OUT[0] } else { 0 }, if g.below(4) != 0 { OUT[1] } else { 0 }];
            let mut z = vec![0u8; 0x20];
            if let Some(n) = &name {
                z[..n.len()].copy_from_slice(n);
            }
            r.mem.write(NAME, &z).unwrap();
            r.mem.write_u32(OUT[0], 0xaaaa_aaaa);
            r.mem.write_u32(OUT[1], 0xbbbb_bbbb);
            let ptr = if name.is_some() { NAME } else { 0 };
            set(&mut r, &[ARENA, ptr, index, outs[0], outs[1]]);
            r.call(0x2cbdc0).unwrap();
            let text = name.as_ref().map(|n| String::from_utf8(n.clone()).unwrap());
            let ours = big::lookup(&b, text.as_deref(), index as usize);
            found[ours.is_some() as usize] += 1;
            let (at, off, size) = ours.as_ref().map_or((0, 0, 0), |e| (ARENA + e.at, e.offset, e.size));
            assert_eq!(ret(&r), at, "case {case}: BIG_Lookup {text:?} {index}");
            assert_eq!(r.mem.read_u32(OUT[0]), if outs[0] != 0 { off } else { 0xaaaa_aaaa }, "case {case}: the offset");
            assert_eq!(r.mem.read_u32(OUT[1]), if outs[1] != 0 { size } else { 0xbbbb_bbbb }, "case {case}: the size");
            if let Some(e) = ours.as_ref().filter(|e| e.name.is_ascii()) {
                assert_eq!(e.name.as_bytes(), &b[e.at as usize..e.at as usize + e.name.len()], "case {case}: the name");
            }
            set(&mut r, &[ARENA, ptr]);
            r.call(0x2cc0d8).unwrap();
            let off = big::find_entry(&b, text.as_deref());
            assert_eq!(ret(&r), if off == 0 { 0 } else { ARENA.wrapping_add(off) }, "case {case}: BIG_FindEntry {text:?}");
            if ptr != 0 {
                let size = big::lookup(&b, text.as_deref(), 0).map_or(0, |e| e.size);
                assert_eq!(r.mem.read_u32(SIZE_WORD), size, "case {case}: BIG_FindEntry's size word");
            }
        }
    }
    println!("lookups not found / found: {found:?}");
    assert!(found.iter().all(|&n| n > 2000));
}
