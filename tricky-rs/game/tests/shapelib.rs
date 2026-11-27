//! `tricky_game::shapelib` (board row E2r) against the original in the function runner (TRICKY_ELF; skipped
//! without it): shape files made here (names with and without long names, repeated and cut-short names, chains
//! of blocks, indexes past the count) through the directory and name functions; first blocks of every kind
//! (plain, the colour-block types at any size, packed RefPack and colour-block streams, mip levels of 0x18fb and
//! 0x28fb, a next offset to clear) through `Shape_Unpack` into a heap made by the memory library, the memory and
//! the answer compared after each call; `Shape_PixelFormatIndex` on every format up to 0x3000.

use r5900::Runner;
use tricky_game::eamem;
use tricky_game::pf::Mem;
use tricky_game::shapelib as sl;

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
const ARENA_SIZE: u32 = 0x6_0000;
const FILE: u32 = 0x0140_0000;
const FILE_SIZE: u32 = 0x2_0000;
const NAME: u32 = 0x0150_0000;
const STACK: u32 = 0x01ff_0000;
const HEAP_ID: u32 = 3;

const COUNT: u32 = 0x2c2660;
const ENTRY: u32 = 0x2c266c;
const ENTRY_NAME: u32 = 0x2c2694;
const LONG_NAME: u32 = 0x2c2610;
const FIND_BY_NAME: u32 = 0x2c2528;
const FIND: u32 = 0x2c25f0;
const FORMAT: u32 = 0x2c26d8;
const UNPACK: u32 = 0x2c2318;
const UNPACK_BLOCKS: u32 = 0x2c2088;

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    for f in [
        COUNT, ENTRY, ENTRY_NAME, LONG_NAME, FIND_BY_NAME, FORMAT, UNPACK_BLOCKS, // the shapes
        0x2c2f90, 0x2c3190, 0x2c2de0, 0x2c2fb0, 0x2c31b0, 0x2c3730, 0x2c3540, 0x2c34d8, 0x2c2d18, 0x2c2bb8, 0x2c2bf8, 0x2c27c8, // the codecs
        0x2cd278, 0x2ccf70, 0x2cca00, 0x2cc9a0, 0x2cd168, 0x2cc808, 0x2ce114, 0x2ce228, 0x2cd990, // the memory library
        0x2fd008, 0x2fd260, 0x2fd14c,
    ] {
        r.allow(f);
    }
    // the heap's end blocks' names ("%s LOW" / "%s HIGH")
    r.stub(0x2fca28, |c, m| {
        let (buf, fmt, arg) = (c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32);
        let (fmt, arg) = (cstr(m, fmt), cstr(m, arg));
        assert_eq!(&fmt[..2], b"%s");
        let mut out = [&arg[..], &fmt[2..]].concat();
        out.push(0);
        m.write(buf, &out).unwrap();
    });
    Some(r)
}

fn cstr(m: &mut r5900::Mem, mut a: u32) -> Vec<u8> {
    let mut s = vec![];
    loop {
        let mut b = [0u8];
        m.read(a, &mut b).unwrap();
        if b[0] == 0 {
            return s;
        }
        s.push(b[0]);
        a += 1;
    }
}

/// Copies our memory into the runner's.
fn load(r: &mut Runner, m: &Mem) {
    for (a, v) in &m.regions {
        r.mem.write(*a, v).unwrap();
    }
}

fn check(r: &mut Runner, m: &Mem, what: &str, ours: u32) {
    assert_eq!(r.cpu.gpr(2) as u32, ours, "{what}: the answer");
    for (a, v) in &m.regions {
        let mut t = vec![0u8; v.len()];
        r.mem.read(*a, &mut t).unwrap();
        if &t != v {
            let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
            panic!("{what}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
        }
    }
}

fn call(r: &mut Runner, f: u32, args: &[u32]) {
    for (k, a) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
    }
    r.call(f).unwrap_or_else(|e| panic!("{f:#x} {args:x?}: {e}"));
}

/// A fresh memory: the arena (random bytes), the file area, the heap table and the library's words, with heap
/// `HEAP_ID` made over the arena on both sides.
fn fresh(r: &mut Runner, g: &mut Rng) -> Mem {
    let mut table = vec![0u8; 0x40];
    r.mem.read(eamem::HEAPS, &mut table).unwrap();
    // the allocation names (0x3bb918, 0x3bb920) as the game has them
    let mut names = vec![0u8; 0x60];
    r.mem.read(0x3bb918, &mut names).unwrap();
    let mut m = Mem {
        regions: vec![
            (0x3bb918, names),
            (ARENA, (0..ARENA_SIZE).map(|_| g.next() as u8).collect()),
            (FILE, (0..FILE_SIZE).map(|_| g.next() as u8).collect()),
            (NAME, vec![0; 0x100]),
            (eamem::HEAPS, table),
            (eamem::ALLOC_FLAGS, HEAP_ID.to_le_bytes().to_vec()),
            (eamem::OUT_OF_MEMORY, vec![0; 4]),
        ],
    };
    load(r, &m);
    let n = eamem::init_heap(&mut m, HEAP_ID, NAME, ARENA, ARENA_SIZE, 0x10, 0x40, 0, false, false, None, 0);
    for (k, a) in [HEAP_ID, NAME, ARENA, ARENA_SIZE, 0x10, 0x40, 0, 0].iter().enumerate() {
        r.cpu.set_gpr(4 + k, *a as u64);
    }
    for k in 0..3 {
        r.mem.write_u32(STACK + 8 + 8 * k, 0);
    }
    r.call(0x2cd278).unwrap();
    check(r, &m, "the heap", n);
    m
}

fn wbytes(m: &mut Mem, a: u32, b: &[u8]) {
    for (k, &x) in b.iter().enumerate() {
        m.w8(a + k as u32, x);
    }
}

fn no_oom() -> impl FnMut(&mut Mem, u32, u32, u32, u32) -> u32 {
    |_, _, _, _, _| panic!("out of memory")
}

#[test]
fn directory_and_names_match_the_original() {
    let Some(mut r) = runner() else { return };
    let mut g = Rng(0x5348_5053_0000_0001);
    let mut found = 0u32;
    for seq in 0..300 {
        let mut m = fresh(&mut r, &mut g);
        let f = FILE + 0x10 * g.below(0x10);
        let count = g.below(40);
        wbytes(&mut m, f, b"SHPS");
        m.w32(f + 8, count);
        let alpha = b"ab0_\0";
        let mut names: Vec<Vec<u8>> = vec![];
        let mut at = 0x10 + 8 * count + 0x10;
        for i in 0..count {
            let mut short = [0u8; 4];
            for c in short.iter_mut() {
                *c = alpha[g.below(alpha.len() as u32) as usize];
            }
            m.w32(f + 0x10 + 8 * i, u32::from_le_bytes(short));
            m.w32(f + 0x14 + 8 * i, at);
            names.push(short.iter().take_while(|&&c| c != 0).copied().collect());
            // the chain: the first block, then maybe others and a long name
            let mut b = f + at;
            let blocks = 1 + g.below(4);
            let long_at = if g.below(2) == 0 { Some(g.below(blocks)) } else { None };
            for k in 0..blocks {
                let last = k + 1 == blocks;
                let ty = if Some(k) == long_at { b'p' } else { [2u8, 5, 0x21, 0x60, 0x82][g.below(5) as usize] };
                let len = 0x10 + 0x10 * g.below(4);
                m.w32(b, ty as u32 | if last { 0 } else { len << 8 });
                if ty == b'p' {
                    let n: Vec<u8> = (0..1 + g.below(6)).map(|_| alpha[g.below(4) as usize]).collect();
                    wbytes(&mut m, b + 4, &n);
                    m.w8(b + 4 + n.len() as u32, 0);
                    names.push(n);
                }
                b += len;
            }
            at = b - f + 0x10;
        }
        load(&mut r, &m);
        let what = |s: &str| format!("seq {seq}: {s}");
        call(&mut r, COUNT, &[f]);
        check(&mut r, &m, &what("count"), sl::get_count(&m, f));
        for _ in 0..30 {
            let i = if g.below(4) == 0 { g.next() } else { g.below(count + 2) };
            call(&mut r, ENTRY, &[f, i]);
            check(&mut r, &m, &what("entry"), sl::get_entry(&m, f, i));
            let out = NAME + 0x80;
            let ours = sl::get_entry_name(&mut m, f, i, out);
            call(&mut r, ENTRY_NAME, &[f, i, out]);
            check(&mut r, &m, &what("entry name"), ours);
            if i < count {
                let e = sl::get_entry(&m, f, i);
                call(&mut r, LONG_NAME, &[e]);
                check(&mut r, &m, &what("long name"), sl::get_long_name(&m, e));
            }
        }
        call(&mut r, LONG_NAME, &[0]);
        check(&mut r, &m, &what("long name of 0"), 0);
        for _ in 0..30 {
            let mut n = if !names.is_empty() && g.below(4) != 0 {
                names[g.below(names.len() as u32) as usize].clone()
            } else {
                (0..g.below(6)).map(|_| alpha[g.below(4) as usize]).collect()
            };
            n.push(0);
            wbytes(&mut m, NAME, &n);
            load(&mut r, &m);
            let ours = sl::find_by_name(&m, f, NAME);
            found += (ours != 0) as u32;
            call(&mut r, if g.below(2) == 0 { FIND_BY_NAME } else { FIND }, &[f, NAME]);
            check(&mut r, &m, &what(&format!("find {n:?}")), ours);
        }
    }
    println!("{found} names found");
    assert!(found > 3000);
}

#[test]
fn pixel_formats_match_the_original() {
    let Some(mut r) = runner() else { return };
    for v in (-0x10..0x3000).chain([0x7fff_ffff, i32::MIN]) {
        call(&mut r, FORMAT, &[v as u32]);
        assert_eq!(r.cpu.gpr(2) as u32, sl::pixel_format_index(v), "{v:#x}");
    }
}

/// A RefPack stream unpacking to about `n` bytes: literal runs and copies.
fn refpack(g: &mut Rng, ty: u8, n: u32) -> Vec<u8> {
    let mut s = vec![ty, 0xfb];
    s.extend(&n.to_be_bytes()[1..]);
    let mut have = 0u32;
    while have < n {
        if have > 0 && g.below(2) == 0 {
            let back = 1 + g.below(have.min(0x400));
            let len = 3 + g.below(8);
            s.push(((back - 1) >> 3 & 0x60) as u8 | ((len - 3) << 2) as u8);
            s.push((back - 1) as u8);
            have += len;
        } else {
            let k = g.below(0x1c);
            s.push(0xe0 | k as u8);
            for _ in 0..(k + 1) * 4 {
                s.push(g.next() as u8);
            }
            have += (k + 1) * 4;
        }
    }
    s.push(0xfc);
    s
}

#[test]
fn unpack_matches_the_original() {
    let Some(mut r) = runner() else { return };
    let mut g = Rng(0x556e_7061_0000_0001);
    let mut kinds = [0u32; 6];
    for seq in 0..1500 {
        let mut m = fresh(&mut r, &mut g);
        let p = FILE + 0x10 * g.below(0x100);
        let kind = g.below(6);
        kinds[kind as usize] += 1;
        let next = if g.below(3) == 0 { (0x10 + 0x10 * g.below(8)) << 8 } else { 0 };
        let (w, h) = (1 + g.below(40), 1 + g.below(40));
        m.w16(p + 4, w as u16);
        m.w16(p + 6, h as u16);
        match kind {
            0 => m.w8(p, [2u8, 5, 0x21, 0x70, 0x63, 0x5f][g.below(6) as usize]),
            1 => m.w8(p, 0x60 + g.below(3) as u8),
            2 => {
                m.w8(p, 0x80 | g.below(0x80) as u8);
                let (ty, n) = ([0x10u8, 0x90][g.below(2) as usize], g.below(0x2000));
                let s = refpack(&mut g, ty, n);
                let s = if s[0] == 0x90 { [&[0x90, 0xfb, 0][..], &s[2..]].concat() } else { s };
                wbytes(&mut m, p + 0x10, &s);
            }
            3 => {
                // colour blocks packed: 0x1afb, 0x18fb, 0x1cfb (no size: nothing unpacked), with no levels
                m.w8(p, 0x80 | g.below(0x80) as u8);
                let ty = [0x1au8, 0x18, 0x1c, 0x1b][g.below(4) as usize];
                let size = w * h * 2;
                wbytes(&mut m, p + 0x10, &[ty, 0xfb, (size >> 16) as u8, (size >> 8) as u8, size as u8, g.next() as u8, (w >> 8) as u8, w as u8]);
                m.w32(p + 0xc, m.r32(p + 0xc) & 0x0fff_ffff);
            }
            _ => {
                // mip levels: the width must stay above 0 for every level (the game loops forever otherwise)
                m.w8(p, 0x80 | g.below(0x80) as u8);
                let levels = 1 + g.below(4);
                let w = (1 << levels) + g.below(40);
                let ty = if kind == 4 { 0x18u8 } else { 0x28 };
                let size = if g.below(4) == 0 { g.below(0x3000) } else { w * h * 2 };
                wbytes(&mut m, p + 0x10, &[ty, 0xfb, (size >> 16) as u8, (size >> 8) as u8, size as u8, g.next() as u8, (w >> 8) as u8, w as u8]);
                m.w32(p + 0xc, m.r32(p + 0xc) & 0x0fff_ffff | levels << 28);
            }
        }
        m.w32(p, m.r32(p) & 0xff | next);
        load(&mut r, &m);
        let head: Vec<u8> = (0..0x18).map(|k| m.r8(p + k)).collect();
        let ours = sl::unpack(&mut m, p, &mut no_oom());
        call(&mut r, UNPACK, &[p]);
        check(&mut r, &m, &format!("seq {seq}: unpack kind {kind} {head:02x?}"), ours);
    }
    println!("kinds {kinds:?}");
}
