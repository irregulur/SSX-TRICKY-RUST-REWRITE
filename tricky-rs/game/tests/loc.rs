//! `tricky_data::loc` (board row E5g) against the original in the function runner (TRICKY_ELF; skipped without
//! it): `.loc` files made here (with and without an id map, several blocks, sorted maps with repeated keys and
//! keys from 0x8000 up, strings of any 16-bit characters) and every kind of id (mapped, unmapped, past the count,
//! 0x80000000 and up) through `Loc_GetString` 0x2c5160 and `LocFile::get_raw`: the string found, or none.

use r5900::Runner;
use tricky_data::loc::LocFile;

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

const FILE: u32 = 0x0100_0000;

fn put32(f: &mut [u8], at: usize, v: u32) {
    f[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// A file: header, block offsets, blocks of strings, maybe an id map; and the ids worth asking for.
fn file(g: &mut Rng) -> (Vec<u8>, Vec<u32>) {
    let blocks = 1 + g.below(3) as usize;
    let mut f = vec![0u8; 0x10 + 4 * blocks];
    for b in f.iter_mut().take(0x10) {
        *b = g.next() as u8;
    }
    let mapped = g.below(2) == 0;
    let flags = (g.next() & !1) | mapped as u32;
    put32(&mut f, 8, flags);
    let which = g.below(blocks as u32) as u16;
    f[0xe..0x10].copy_from_slice(&which.to_le_bytes());
    let mut counts = vec![];
    for k in 0..blocks {
        let at = f.len();
        put32(&mut f, 0x10 + 4 * k, at as u32);
        let n = g.below(40) as usize;
        counts.push(n);
        f.extend(vec![0u8; 0x10 + 4 * n]);
        put32(&mut f, at + 0xc, n as u32);
        for i in 0..n {
            let s = f.len() - at;
            put32(&mut f, at + 0x10 + 4 * i, s as u32);
            for _ in 0..g.below(12) {
                let c = [g.next() as u16 | 1, b'a' as u16, 0x3042][g.below(3) as usize];
                f.extend(c.to_le_bytes());
            }
            f.extend([0, 0]);
        }
        while f.len() % 4 != 0 {
            f.push(0);
        }
    }
    let n = counts[which as usize] as u32;
    let mut ids = vec![0, 1, n, n.wrapping_sub(1), n + 1, 0x8000_0000, 0xffff_ffff, g.next()];
    for _ in 0..10 {
        ids.push(g.below(n + 3));
    }
    if mapped {
        let at = f.len();
        put32(&mut f, 4, at as u32);
        let pairs = g.below(30);
        f.extend(vec![0u8; 0x10 + 4 * pairs as usize]);
        put32(&mut f, at + 8, pairs);
        let mut keys: Vec<u16> = (0..pairs).map(|_| if g.below(3) == 0 { 0x8000 | g.next() as u16 } else { g.below(60) as u16 }).collect();
        keys.sort();
        for (i, k) in keys.iter().enumerate() {
            let v = if g.below(5) == 0 { g.next() as u16 } else { g.below(n + 2) as u16 };
            f[at + 0x10 + 4 * i..at + 0x12 + 4 * i].copy_from_slice(&k.to_le_bytes());
            f[at + 0x12 + 4 * i..at + 0x14 + 4 * i].copy_from_slice(&v.to_le_bytes());
            ids.push(*k as u32 | if g.below(4) == 0 { g.next() << 16 } else { 0 });
        }
        for _ in 0..5 {
            ids.push(g.below(0x10000));
        }
    }
    (f, ids)
}

#[test]
fn lookups_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x2c50b0, 0x2fa258] {
        r.allow(f);
    }
    r.allow(0x2c5070);
    let mut g = Rng(0x4c6f_6300_0000_0001);
    let (mut found, mut none) = (0u32, 0u32);
    for seq in 0..5000 {
        let (bytes, ids) = file(&mut g);
        r.mem.write(FILE, &bytes).unwrap();
        // past the file: zeros, so a read past it is the same on both sides (ours finds nothing there)
        r.mem.write(FILE + bytes.len() as u32, &[0; 64]).unwrap();
        let loc = LocFile::new(bytes.clone());
        for id in ids {
            r.cpu.set_gpr(4, FILE as u64);
            r.cpu.set_gpr(5, id as u64);
            r.call(0x2c5160).unwrap_or_else(|e| panic!("seq {seq}: {id:#x}: {e}"));
            let p = r.cpu.gpr(2) as u32;
            let theirs = (p != 0).then(|| {
                let mut s = vec![];
                let mut a = p;
                loop {
                    let mut c = [0u8; 2];
                    r.mem.read(a, &mut c).unwrap();
                    let c = u16::from_le_bytes(c);
                    if c == 0 {
                        break s;
                    }
                    s.push(c);
                    a += 2;
                }
            });
            assert_eq!(theirs, loc.get_raw(id), "seq {seq}: id {id:#x}");
            if theirs.is_some() {
                found += 1;
            } else {
                none += 1;
            }
        }
    }
    println!("{found} strings found, {none} ids with none");
    assert!(found > 30_000 && none > 20_000);
}
