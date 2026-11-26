//! `tricky_game::libc` (board row E2l) against the original in the function runner (TRICKY_ELF; skipped without
//! it): the C library's string and memory functions on random bytes (text, bytes from 0x80 up, long runs with no
//! 0), at every alignment but mostly 8- and 16-byte aligned so the block paths run, with copies that overlap and
//! second strings that are the first one changed in one place, in case, or cut short. Every byte of the area and
//! the answer must be equal.

use r5900::Runner;
use tricky_game::libc;
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
const SIZE: u32 = 0x400;
/// Around newlib's `_ctype_` (0x3ba520), the bytes before it included.
const CTYPE_AT: u32 = 0x3ba400;
const CTYPE_LEN: u32 = 0x300;

const STRLEN: u32 = 0x2fd260;
const STRCPY: u32 = 0x2fd14c;
const STRNCPY: u32 = 0x2fd630;
const MEMCPY: u32 = 0x2fb2b4;
const MEMSET: u32 = 0x2fb468;
const STRCMP: u32 = 0x2fd008;
const STRNCMP: u32 = 0x2fd478;
const MEMCMP: u32 = 0x2fb220;
const STRICMP: u32 = 0x2fcc98;
const STRNICMP: u32 = 0x2fd398;
const FUNCS: [u32; 10] = [STRLEN, STRCPY, STRNCPY, MEMCPY, MEMSET, STRCMP, STRNCMP, MEMCMP, STRICMP, STRNICMP];

/// A byte of one of the fills.
fn byte(g: &mut Rng, kind: u32) -> u8 {
    const TEXT: &[u8] = b"AaBbCcXxYyZz019_./\\ @[`{";
    match kind {
        // text, now and then a byte from 0x80 up or a 0
        0 => match g.below(40) {
            0 => 0,
            1..=3 => 0x80 | g.below(0x80) as u8,
            _ => TEXT[g.below(TEXT.len() as u32) as usize],
        },
        // long runs, no 0 but rarely
        1 => match g.below(200) {
            0 => 0,
            _ => 1 + g.below(255) as u8,
        },
        // 0x7f and up: `strncpy`'s first quadword goes through whole
        2 => 0x7f + g.below(0x81) as u8,
        _ => g.next() as u8,
    }
}

/// An offset in the first half of the area: mostly 16- or 8-byte aligned.
fn place(g: &mut Rng) -> u32 {
    let p = g.below(0x200);
    match g.below(10) {
        0..=4 => p & !0xf,
        5..=6 => p & !7,
        _ => p,
    }
}

fn count(g: &mut Rng) -> u32 {
    match g.below(3) {
        0 => [0, 1, 2, 7, 8, 9, 15, 16, 17, 23, 24, 31, 32, 33, 48, 64][g.below(16) as usize],
        1 => g.below(40),
        _ => g.below(0x120),
    }
}

fn case_swap(c: u8) -> u8 {
    if c.is_ascii_alphabetic() {
        c ^ 0x20
    } else {
        c
    }
}

#[test]
fn the_c_library_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let mut ctype = vec![0u8; CTYPE_LEN as usize];
    r.mem.read(CTYPE_AT, &mut ctype).unwrap();
    let mut g = Rng(0xe2d0_0000_0000_0001);
    let (mut equal, mut overlaps, mut nonzero) = (0u32, 0u32, 0u32);
    for case in 0..FUNCS.len() as u32 * 8000 {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let kind = g.below(4);
        let mut area: Vec<u8> = (0..SIZE).map(|_| byte(&mut g, kind)).collect();
        // every string ends within 0x100 bytes, so a copy stays inside the area
        for k in (0xff..SIZE as usize).step_by(0x100) {
            area[k] = 0;
        }
        let (a, mut b) = (place(&mut g), place(&mut g));
        // the second string: the first one, changed a little
        if g.below(3) != 0 && func != MEMSET {
            let len = g.below(0x100) as usize;
            let (a, b) = (a as usize, b as usize);
            let copy: Vec<u8> = area[a..a + len].to_vec();
            for (k, &c) in copy.iter().enumerate() {
                if (b + k) % 0x100 != 0xff {
                    area[b + k] = if g.below(6) == 0 { case_swap(c) } else { c };
                }
            }
            let k = g.below(len as u32 + 1) as usize;
            match g.below(5) {
                0 => area[b + k] = byte(&mut g, 3),
                1 => area[b + k] = 0,
                // the first string ends where the second has 0xe0, which `strnicmp`'s walk lowers to 0
                2 if (b + k) % 0x100 != 0xff => {
                    area[a + k] = 0;
                    area[b + k] = 0xe0;
                }
                _ => {}
            }
        }
        // copies now and then onto or from a few bytes away, often a whole number of doublewords away
        if g.below(3) == 0 {
            b = match g.below(2) {
                0 => (a + g.below(0x20)).saturating_sub(0x10),
                _ => (a + 8 * g.below(5)).saturating_sub(0x10),
            };
        }
        let (mut a, mut b) = (ARENA + a, ARENA + b);
        let n = count(&mut g);
        let c = g.next();

        let mut m = Mem { regions: vec![(ARENA, area.clone()), (CTYPE_AT, ctype.clone())] };
        let (args, ours): (Vec<u32>, u32) = match func {
            STRLEN => (vec![b], libc::strlen(&m, b)),
            STRCPY => {
                // onto its own string ahead it would copy on for ever: only from ahead onto behind
                let len = libc::strlen(&m, b);
                if b < a && a <= b + len {
                    (a, b) = (b, a);
                }
                overlaps += (a < b && b <= a + libc::strlen(&m, b)) as u32;
                (vec![a, b], libc::strcpy(&mut m, a, b))
            }
            STRNCPY => {
                overlaps += (b < a && a < b + n) as u32;
                (vec![a, b, n], libc::strncpy(&mut m, a, b, n))
            }
            MEMCPY => {
                overlaps += (b < a && a < b + n) as u32;
                (vec![a, b, n], libc::memcpy(&mut m, a, b, n))
            }
            MEMSET => (vec![a, c, n], libc::memset(&mut m, a, c, n)),
            STRCMP => (vec![a, b], libc::strcmp(&m, a, b) as u32),
            STRNCMP => (vec![a, b, n], libc::strncmp(&m, a, b, n) as u32),
            MEMCMP => (vec![a, b, n], libc::memcmp(&m, a, b, n) as u32),
            STRICMP => (vec![a, b], libc::stricmp(&m, a, b) as u32),
            _ => (vec![a, b, n], libc::strnicmp(&m, a, b, n) as u32),
        };
        equal += (ours == 0) as u32;
        nonzero += (ours != 0 && func != STRLEN) as u32;

        r.mem.write(ARENA, &area).unwrap();
        for (k, x) in args.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *x as i32 as i64 as u64);
        }
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x} {args:x?}: {e}"));
        assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x} {args:x?}: the answer");
        let mut th = vec![0u8; SIZE as usize];
        r.mem.read(ARENA, &mut th).unwrap();
        let ourm = &m.regions[0].1;
        if &th != ourm {
            let i = th.iter().zip(ourm).position(|(x, y)| x != y).unwrap();
            panic!("case {case} {func:#x} {args:x?}: byte at {:#x}: theirs {:#x} ours {:#x}", ARENA + i as u32, th[i], ourm[i]);
        }
    }
    println!("{equal} answers 0, {nonzero} not, {overlaps} copies onto their own source");
    assert!(equal > 5000 && nonzero > 5000 && overlaps > 300);
}
