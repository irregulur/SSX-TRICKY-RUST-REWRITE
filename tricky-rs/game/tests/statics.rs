//! The forty header-vector static initializers (row X2a, `tricky_game::statics`) against the game in the function
//! runner (TRICKY_ELF; skipped without it): each run on random memory around its place, every byte compared.

use r5900::Runner;
use tricky_game::pf::Mem;
use tricky_game::statics::{init_header_vectors, HEADER_INITS};

#[test]
fn header_vector_inits_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for (f, base) in HEADER_INITS {
        r.allow(f);
        let at = base - 0x10;
        let bytes: Vec<u8> = (0..0x50)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        let mut m = Mem { regions: vec![(at, bytes.clone())] };
        init_header_vectors(&mut m, base);
        r.mem.write(at, &bytes).unwrap();
        r.call(f).unwrap_or_else(|e| panic!("{f:#x}: {e}"));
        let mut theirs = vec![0u8; 0x50];
        r.mem.read(at, &mut theirs).unwrap();
        assert_eq!(theirs, m.regions[0].1, "{f:#x} at {base:#x}");
    }
}
