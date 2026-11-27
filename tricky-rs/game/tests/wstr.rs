//! `tricky_game::wstr` (board row E5f) against the original in the function runner (TRICKY_ELF; skipped without
//! it): random 16-bit strings through the helpers, and random formats (every flag, widths and precisions written
//! and from `*`, every conversion and unknown ones, `%` at the end, null strings, `%n`) with matching arguments
//! through `WStr_VFormat` and `WStr_Sprintf`; the output, the memory and the answers compared.

use r5900::Runner;
use tricky_game::pf::Mem;
use tricky_game::wstr;

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

const DST: u32 = 0x0100_0000;
const FMT: u32 = 0x0110_0000;
const ARGS: u32 = 0x0120_0000;
const STRS: u32 = 0x0130_0000;
const SIZE: u32 = 0x2000;
const HEAP: u32 = 0x0180_0000;

const LEN: u32 = 0x2c52f8;
const FIND: u32 = 0x2c5380;
const CMP: u32 = 0x2c5418;
const NCMP: u32 = 0x2c5438;
const COPY_ALL: u32 = 0x2c5498;
const NCOPY: u32 = 0x2c54b8;
const FROM_ASCII: u32 = 0x2c5510;
const FORMAT_THUNK: u32 = 0x2c5558;
const FORMAT_ARGS: u32 = 0x2c5578;
const VFORMAT: u32 = 0x2c48e8;
const SPRINTF: u32 = 0x2c4e90;

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    for f in [LEN, FIND, CMP, NCMP, COPY_ALL, NCOPY, FROM_ASCII, FORMAT_ARGS, VFORMAT, 0x2c45e0, 0x2c4508, 0x2c4478, 0x2c44d8, 0x2f64b0, 0x2f6500, 0x2f65b8, 0x2f6650, 0x2fa330, 0x2fd260, 0x304fe8, 0x305490, 0x301ec0, 0x305318, 0x304e68, 0x304c08, 0x305218, 0x304fb0, 0x304bd8, 0x2f6230, 0x2f5ec8, 0x2f5480, 0x2f5f88, 0x2f5c88, 0x2f5f20, 0x304ef0, 0x2fb468, 0x306298, 0x304b30, 0x3058a0, 0x3020d8, 0x303658, 0x2f6398, 0x2f5be8, 0x2f5ab8] {
        r.allow(f);
    }
    // the C library's heap, for `fcvt`'s buffer and `_dtoa_r`'s numbers: a bump allocator in memory not compared
    let top = std::rc::Rc::new(std::cell::Cell::new(HEAP));
    for f in [0x2fc020u32, 0x2faaa8, 0x306298] {
        let top = top.clone();
        r.stub(f, move |c, m| {
            // _realloc_r(r, old, n), _malloc_r(r, n), _calloc_r(r, n, size)
            let n = match f {
                0x2fc020 => c.gpr(6) as u32,
                0x2faaa8 => c.gpr(5) as u32,
                _ => (c.gpr(5) as u32) * (c.gpr(6) as u32),
            };
            // with a header like newlib's chunks (the size word just before the memory: `%f` reads the byte
            // before `fcvt`'s digits, the size's top byte)
            let n = (n + 15) & !15;
            let p = top.get() + 16;
            assert!(p + n < HEAP + 0x10_0000, "the test's heap is full");
            m.write(p - 8, &[0, 0, 0, 0]).unwrap();
            m.write(p - 4, &((n + 8) | 1).to_le_bytes()).unwrap();
            m.write(p, &vec![0; n as usize]).unwrap();
            top.set(p + n);
            c.set_gpr(2, p as u64);
        });
    }
    Some(r)
}

fn fresh(g: &mut Rng) -> Mem {
    Mem {
        regions: [DST, FMT, ARGS, STRS].iter().map(|&a| (a, (0..SIZE).map(|_| g.next() as u8).collect())).collect(),
    }
}

fn load(r: &mut Runner, m: &Mem) {
    for (a, v) in &m.regions {
        r.mem.write(*a, v).unwrap();
    }
}

fn check(r: &mut Runner, m: &Mem, what: &str, ours: Option<u32>) {
    let answer = r.cpu.gpr(2) as u32;
    for (a, v) in &m.regions {
        let mut t = vec![0u8; v.len()];
        r.mem.read(*a, &mut t).unwrap();
        if &t != v {
            let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
            let show = |b: &[u8]| -> String {
                let s = i.saturating_sub(40) & !1;
                char::decode_utf16(b[s..(i + 40).min(b.len())].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])))
                    .map(|c| c.unwrap_or('?'))
                    .collect()
            };
            panic!("{what}: byte at {:#x}: theirs {:#x} ours {:#x}\n theirs {:?}\n ours   {:?}", a + i as u32, t[i], v[i], show(&t), show(v));
        }
    }
    if let Some(o) = ours {
        assert_eq!(answer, o, "{what}: the answer");
    }
}

fn call(r: &mut Runner, f: u32, args: &[u64]) {
    for (k, a) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, *a);
    }
    r.call(f).unwrap_or_else(|e| panic!("{f:#x} {args:x?}: {e}"));
}

fn wstr(m: &mut Mem, at: u32, s: &[u16]) {
    for (k, &c) in s.iter().enumerate() {
        m.w16(at + 2 * k as u32, c);
    }
    m.w16(at + 2 * s.len() as u32, 0);
}

fn chars(g: &mut Rng, n: u32) -> Vec<u16> {
    let set = [b'a' as u16, b'b' as u16, b'A' as u16, b'%' as u16, b'1' as u16, b'2' as u16, 0x3042, 0xffff, b' ' as u16];
    (0..g.below(n)).map(|_| set[g.below(set.len() as u32) as usize]).collect()
}

#[test]
fn helpers_match_the_original() {
    let Some(mut r) = runner() else { return };
    let mut g = Rng(0x5753_7472_0000_0001);
    for seq in 0..20_000 {
        let mut m = fresh(&mut g);
        let (a, b) = (chars(&mut g, 12), chars(&mut g, 6));
        // often both empty (the bytes after them random)
        let (a, b) = if g.below(8) == 0 { (vec![], vec![]) } else { (a, b) };
        // b often a piece of a
        let b = if g.below(2) == 0 && !a.is_empty() {
            let i = g.below(a.len() as u32) as usize;
            a[i..(i + g.below(4) as usize).min(a.len())].to_vec()
        } else {
            b
        };
        wstr(&mut m, STRS, &a);
        wstr(&mut m, STRS + 0x100, &b);
        let ascii: Vec<u8> = (0..g.below(12)).map(|_| 1 + g.below(255) as u8).collect();
        for (k, &c) in ascii.iter().enumerate() {
            m.w8(STRS + 0x200 + k as u32, c);
        }
        m.w8(STRS + 0x200 + ascii.len() as u32, 0);
        load(&mut r, &m);
        let (sa, sb) = (STRS as u64, STRS as u64 + 0x100);
        let n = [0i32, -1, 1, 2, 3, 5, 100, i32::MIN][g.below(8) as usize];
        let what = |s: &str| format!("seq {seq}: {s} {a:x?} {b:x?} {n}");
        match g.below(9) {
            0 => {
                call(&mut r, LEN, &[sa]);
                check(&mut r, &m, &what("len"), Some(wstr::len(&m, STRS)));
            }
            1 => {
                call(&mut r, NCMP, &[sa, sb, n as i64 as u64]);
                check(&mut r, &m, &what("ncmp"), Some(wstr::ncmp(&m, STRS, STRS + 0x100, n) as u32));
            }
            2 => {
                call(&mut r, CMP, &[sa, sb]);
                check(&mut r, &m, &what("cmp"), Some(wstr::cmp(&m, STRS, STRS + 0x100) as u32));
            }
            3 => {
                call(&mut r, FIND, &[sa, sb]);
                check(&mut r, &m, &what("find"), Some(wstr::find(&m, STRS, STRS + 0x100) as u32));
            }
            4 => {
                wstr::ncopy(&mut m, DST, STRS, n);
                call(&mut r, NCOPY, &[DST as u64, sa, n as i64 as u64]);
                check(&mut r, &m, &what("ncopy"), None);
            }
            5 => {
                wstr::copy_all(&mut m, DST, STRS);
                call(&mut r, COPY_ALL, &[DST as u64, sa]);
                check(&mut r, &m, &what("copy all"), None);
            }
            6 => {
                wstr::from_ascii(&mut m, DST, STRS + 0x200);
                call(&mut r, FROM_ASCII, &[DST as u64, sa + 0x200]);
                check(&mut r, &m, &what("from ascii"), None);
            }
            7 => {
                // a format with %1 and %2 and other %s; its arguments a and b
                let f = chars(&mut g, 16);
                wstr(&mut m, FMT, &f);
                load(&mut r, &m);
                wstr::format_args(&mut m, DST, FMT, STRS, STRS + 0x100);
                call(&mut r, FORMAT_ARGS, &[DST as u64, FMT as u64, sa, sb]);
                check(&mut r, &m, &format!("{} {f:x?}", what("format args")), None);
            }
            _ => {
                let f: Vec<u16> = chars(&mut g, 16).into_iter().filter(|&c| c != b'2' as u16).collect();
                wstr(&mut m, FMT, &f);
                load(&mut r, &m);
                wstr::format_args(&mut m, DST, FMT, STRS, 0);
                call(&mut r, FORMAT_THUNK, &[DST as u64, FMT as u64, sa]);
                check(&mut r, &m, &format!("{} {f:x?}", what("format thunk")), None);
            }
        }
    }
}

/// A value for `%f` at `prec` places: halves and eighths (ties), thousandths, big ones, small ones, near 1, any
/// bits under 1e6; none so small that the point lies more than one place before the digits end (decpt + prec <
/// −1: the game then never returns).
fn fixed(g: &mut Rng, prec: i32) -> f64 {
    let sign = if g.below(2) == 0 { -1.0 } else { 1.0 };
    let v = match g.below(8) {
        0 => 0.0,
        1 => (g.below(4096) as f64 - 2048.0) / 64.0,
        2 => (g.next() as i32) as f64 / 1000.0,
        3 => (g.next() as i32) as f64 * 37.0,
        4 => g.below(1000) as f64 / 10f64.powi(g.below(9) as i32) * sign,
        5 => (1.0 - g.below(1000) as f64 * 1e-9) * sign,
        6 => (1 + g.below(999)) as f64 * 10f64.powi(-(3 + g.below(6) as i32)) * sign,
        _ => f64::from_bits((g.next() as u64) << 32 | g.next() as u64) % 1e6,
    };
    if v.is_nan() || wstr::fcvt(v, prec).1 + prec < -1 {
        0.0
    } else {
        v
    }
}

/// A random format and its argument slots.
fn format(g: &mut Rng) -> (Vec<u16>, Vec<u64>) {
    let mut f = vec![];
    let mut slots = vec![];
    for _ in 0..1 + g.below(6) {
        for _ in 0..g.below(4) {
            f.push([b'a' as u16, b' ' as u16, b'=' as u16, 0x3042][g.below(4) as usize]);
        }
        if g.below(8) == 0 {
            continue;
        }
        f.push(b'%' as u16);
        for &(c, p) in &[(b'-', 4), (b'+', 4), (b' ', 5), (b'#', 4), (b'0', 3)] {
            if g.below(p) == 0 {
                f.push(c as u16);
            }
        }
        match g.below(4) {
            0 => {
                f.push(b'*' as u16);
                slots.push((g.below(46) as i64 - 6) as u64);
            }
            1 => f.extend(format!("{}", 1 + g.below(30)).bytes().map(|c| c as u16)),
            _ => {}
        }
        let mut prec = 6i32;
        let mut zero = false;
        match g.below(6) {
            0 => f.push(b'.' as u16),
            1 => {
                prec = 1 + g.below(8) as i32;
                f.extend(format!(".{prec}").bytes().map(|c| c as u16));
            }
            2 => {
                prec = g.below(8) as i32;
                f.extend(b".*".iter().map(|&c| c as u16));
                slots.push(prec as u64);
            }
            3 => {
                // not a precision: the 0 is printed as the conversion, the letter after it as text
                f.extend(b".0".iter().map(|&c| c as u16));
                zero = true;
            }
            _ => {}
        }
        let conv = b"cCdiouxXpPeEfgGnsS%DkZ"[g.below(if zero { 18 } else { 22 }) as usize];
        f.push(conv as u16);
        if zero {
            continue;
        }
        match conv {
            b'c' | b'C' => slots.push(g.next() as u64 | (g.next() as u64) << 32),
            b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'p' | b'P' => {
                let v = match g.below(5) {
                    0 => [0u32, 1, 0xffff_ffff, 0x8000_0000, 0x7fff_ffff][g.below(5) as usize],
                    1 => g.below(100),
                    2 => g.below(100).wrapping_neg(),
                    _ => g.next(),
                };
                slots.push(v as u64 | (g.next() as u64) << 32);
            }
            b'e' | b'E' | b'g' | b'G' => {
                let v = match g.below(4) {
                    0 => (g.next() as i32) as f64 * 3.5,
                    1 => -0.5,
                    _ => fixed(g, 6),
                };
                slots.push(v.to_bits());
            }
            b'f' => slots.push(fixed(g, prec).to_bits()),
            b'n' => slots.push((ARGS + 0x400 + 4 * g.below(8)) as u64),
            b's' | b'S' => slots.push(if g.below(5) == 0 { 0 } else { (STRS + if conv == b's' { 0 } else { 0x200 }) as u64 }),
            _ => {}
        }
    }
    // sometimes a % at the very end (the game reads on past the 0)
    if g.below(10) == 0 {
        f.push(b'%' as u16);
    }
    (f, slots)
}

#[test]
fn the_formatter_matches_the_original() {
    let Some(mut r) = runner() else { return };
    let mut g = Rng(0x4669_6d74_0000_0001);
    let mut chars_out = 0u64;
    for seq in 0..100_000 {
        let mut m = fresh(&mut g);
        let (f, mut slots) = format(&mut g);
        // a % at the end reads on into the tail's %d: one more slot, set the same on both sides
        let sprintf = slots.len() <= 5 && g.below(3) == 0;
        while slots.len() < 8 {
            slots.push(g.below(1000) as u64);
        }
        wstr(&mut m, FMT, &f);
        // past the format: a short tail and its 0, for a % at the end
        let tail = FMT + 2 * f.len() as u32 + 2;
        wstr(&mut m, tail, &[b'q' as u16, b'%' as u16, b'd' as u16]);
        for (k, s) in slots.iter().enumerate() {
            m.w32(ARGS + 8 * k as u32, *s as u32);
            m.w32(ARGS + 8 * k as u32 + 4, (*s >> 32) as u32);
        }
        let s16 = chars(&mut g, 10);
        wstr(&mut m, STRS, &s16);
        let s8: Vec<u8> = (0..g.below(10)).map(|_| 1 + g.below(255) as u8).collect();
        for (k, &c) in s8.iter().enumerate() {
            m.w8(STRS + 0x200 + k as u32, c);
        }
        m.w8(STRS + 0x200 + s8.len() as u32, 0);
        let text = String::from_utf16_lossy(&f);
        load(&mut r, &m);
        let ours = wstr::vformat(&mut m, DST, FMT, ARGS);
        chars_out += ((ours - DST) / 2) as u64;
        if sprintf {
            let mut regs = vec![DST as u64, FMT as u64];
            regs.extend(&slots[..6]);
            call(&mut r, SPRINTF, &regs);
        } else {
            call(&mut r, VFORMAT, &[DST as u64, FMT as u64, ARGS as u64]);
        }
        check(&mut r, &m, &format!("seq {seq}: {} {text:?} {slots:x?}", if sprintf { "sprintf" } else { "vformat" }), Some(ours));
    }
    println!("{chars_out} characters written");
}
