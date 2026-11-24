//! The lesson archives `data/tutorial/<rider>.big` from the player's disc (TRICKY_DISC; skipped without it), board
//! row C11c2: what they hold, and `tricky_data::inp` on every recording in them.
//!
//! Each archive is a `C0FB` BIG of 30 RefPack-packed files `data\tutorial\<rider><NN>.inp` (NN 01..30, one per
//! trick of the six pages of five), each unpacking to exactly the lesson manager's 0x4d598-byte buffer. Nothing else
//! (no sounds, no speech) is in them.

use std::collections::BTreeSet;

use tricky_data::{big, compress, disc::Disc, inp};

#[test]
fn lesson_archives_hold_the_recordings() {
    let Ok(iso) = std::env::var("TRICKY_DISC") else { return };
    let mut d = Disc::open(&iso).expect("the disc");
    let bigs: Vec<String> = d
        .files()
        .iter()
        .map(|f| f.path.clone())
        .filter(|p| p.starts_with("/DATA/TUTORIAL/") && p.ends_with(".BIG"))
        .collect();
    assert_eq!(bigs.len(), 12, "one archive a rider: {bigs:?}");
    let (mut fewest, mut most) = (usize::MAX, 0);
    let mut ctl0 = BTreeSet::new();
    let mut ctl2 = BTreeSet::new();
    for path in &bigs {
        let rider = path["/DATA/TUTORIAL/".len()..path.len() - 4].to_ascii_lowercase();
        let b = d.read_path(path).unwrap().unwrap();
        assert_eq!(big::identify(&b), 1, "{path}: a C0FB archive");
        let es = big::entries(&b[..big::header_size(&b)]);
        let names: BTreeSet<String> = es.iter().map(|e| e.name.clone()).collect();
        let want: BTreeSet<String> = (1..=30).map(|k| format!("data\\tutorial\\{rider}{k:02}.inp")).collect();
        assert_eq!(names, want, "{path}");
        for e in &es {
            let raw = &b[e.offset as usize..(e.offset + e.size) as usize];
            let un = compress::load(raw.to_vec()).expect("unpacks");
            // (the whole buffer: the recorder saved it as it stood)
            assert_eq!(un.len(), inp::BUFFER_BYTES, "{}", e.name);
            let p = inp::Inp::parse(&un).expect("parses");
            let pos = u32::from_le_bytes(un[0..4].try_into().unwrap());
            // (word 0, the position, left at the end by the recorder; the game zeroes it when it loads)
            assert_eq!(pos, p.len(), "{}", e.name);
            assert_eq!(p.rest, [0; 4], "{}", e.name);
            fewest = fewest.min(p.frames.len());
            most = most.max(p.frames.len());
            for (k, f) in p.frames.iter().enumerate() {
                // (the pad state at record +12: each frame's previous buttons are the frame before's)
                if k > 0 {
                    assert_eq!(f.pad.prev, p.frames[k - 1].pad.now, "{} frame {k}", e.name);
                }
                // (the sticks: a byte over 255, in -1..1)
                for a in f.pad.axes {
                    assert!(a.abs() <= 1.0 && ((a * 255.0) - (a * 255.0).round()).abs() < 1e-3, "{} {a}", e.name);
                }
                ctl0.insert(f.ctl[0]);
                ctl2.insert(f.ctl[2]);
            }
        }
    }
    eprintln!("frames {fewest}..{most}; control words {ctl0:x?} {ctl2:x?}");
}
