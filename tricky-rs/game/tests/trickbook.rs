//! The trick book's functions in `tricky_game::career` (board row E12h) against the original in the function
//! runner (TRICKY_ELF; skipped without it): career blocks with four player slots pointing at records whose book
//! words (+0x14: +4 done, +8 forced) are random, sparse, full pages and runs of pages; every function on every
//! player, trick and page (negative ones included).

use r5900::Runner;
use tricky_game::career::{self, Career, CAREER_BASE, CAREER_SIZE};

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

/// A book word: random, sparse, the first n pages full, or all but a few bits.
fn word(g: &mut Rng) -> u32 {
    match g.below(5) {
        0 => g.next(),
        1 => g.next() & g.next() & g.next(),
        2 => (1u64 << (5 * g.below(7))) as u32 - 1,
        3 => !(1 << g.below(32)),
        _ => 0,
    }
}

#[test]
fn the_book_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x164020, 0x164060, 0x1640a8, 0x164108, 0x164110, 0x164178, 0x1641f8, 0x164258] {
        r.allow(f);
    }
    let mut g = Rng(0x4274_6b00_0000_0001);
    let mut pages = [0u32; 8];
    for seq in 0..3000 {
        let mut c = Career::default();
        for (k, b) in c.bytes.iter_mut().enumerate() {
            *b = (k as u32).wrapping_mul(0x9e37).wrapping_add(seq) as u8;
        }
        let mut books = [0u32; 4];
        for k in 0..4 {
            c.bytes[0x30fc + 0x44 * k] = k as u8;
            let rec = 0x100 + 0x40 * k as u32;
            c.w32(0x3114 + 0x44 * k, CAREER_BASE + rec);
            let (done, forced) = (word(&mut g), if g.below(3) == 0 { word(&mut g) } else { 0 });
            // all 32 bits set never ends (past page 6 the pages are page 0 again)
            let hole = !(1u32 << g.below(32));
            let (done, forced) = if done | forced == u32::MAX { (done & hole, forced & hole) } else { (done, forced) };
            c.w32(rec as usize + 0x14 + 4, done);
            c.w32(rec as usize + 0x14 + 8, forced);
            books[k] = CAREER_BASE + rec + 0x14;
        }
        r.mem.write(CAREER_BASE, &c.bytes[..CAREER_SIZE]).unwrap();
        let mut run = |f: u32, args: &[u32]| {
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
            }
            r.call(f).unwrap_or_else(|e| panic!("seq {seq}: {f:#x} {args:x?}: {e}"));
            r.cpu.gpr(2) as u32
        };
        for p in 0..4i8 {
            let b = books[p as usize];
            let n = c.complete_pages(p);
            pages[n.min(7) as usize] += 1;
            assert_eq!(run(0x164060, &[b]), n, "seq {seq}: complete pages");
            assert_eq!(run(0x1641f8, &[b]), c.first_undone_trick(p) as u32, "seq {seq}: first undone");
            for t in (0..40).chain([u32::MAX, 0x7fff_ffff, g.next()]) {
                assert_eq!(run(0x164258, &[b, t]), c.trick_done(p, t) as u32, "seq {seq}: done {t:#x}");
                assert_eq!(run(0x1640a8, &[t]), career::trick_page(t), "seq {seq}: tier {t:#x}");
            }
            for page in (0..10).chain([u32::MAX, 0x8000_0000, g.next()]) {
                assert_eq!(run(0x164178, &[b, page]), c.tricks_done_in_page(p, page), "seq {seq}: done in {page:#x}");
                assert_eq!(run(0x164020, &[b, page]), c.page_complete(p, page) as u32, "seq {seq}: complete {page:#x}");
                assert_eq!(run(0x164110, &[page]), career::book_page_first_trick(page), "seq {seq}: first {page:#x}");
                assert_eq!(run(0x164108, &[page]), career::book_tricks_per_page(page), "seq {seq}: per page");
            }
        }
    }
    println!("complete pages 0..7: {pages:?}");
    assert!(pages.iter().take(7).all(|&n| n > 50));
}
