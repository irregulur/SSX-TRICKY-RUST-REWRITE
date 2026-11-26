//! `tricky_data::disc` (board row E2k) against the original in the function runner (TRICKY_ELF; skipped without
//! it): made-up ISO 9660 images (random directory trees, names with and without an extension, records that fill
//! a sector to its end, paths whose hashes collide, one disc with more files than the game takes) mounted by
//! `CdFs_Mount` 0x2c9970 with the drive's sectors answered from the image, and our `Disc::open` on the same
//! image: the table (hash, sector, size, path, in order) and the switch to searching by path must be equal; then
//! `CdFs_FindFile` 0x2c9cc0 and `Disc::find` on every file in every spelling, on folders and on missing paths.
//! The C library's `qsort` 0x2fb5c8 is also run on its own with a comparison that is no true order.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_data::disc::{self, Disc, DiscError, SECTOR};

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

const CD_INIT: u32 = 0x2e9188;
const CD_MMODE: u32 = 0x2e9660;
const CD_DISK_READY: u32 = 0x2e9468;
const CD_GET_DISK_TYPE: u32 = 0x2e9c48;
const CD_GET_TOC: u32 = 0x2e9908;
const CD_STOP: u32 = 0x2e9b98;
const CD_INIT_EE_CB: u32 = 0x2e8538;
const CD_CALLBACK: u32 = 0x2e8380;
const FLUSH_CACHE: u32 = 0x306a60;
const CREATE_SEMA: u32 = 0x3067e0;
const CREATE_THREAD: u32 = 0x3065e0;
const START_THREAD: u32 = 0x306600;
const ADD_POLL_TASK: u32 = 0x2cebe8;
const READ_SECTOR: u32 = 0x2c90c8;
const MOUNT: u32 = 0x2c9970;
const FIND_FILE: u32 = 0x2c9cc0;
const QSORT: u32 = 0x2fb5c8;
/// The sector `CdFs_ReadSector` reads, and its buffer.
const SECTOR_AT: u32 = 0x349d80;
const SECTOR_BUF: u32 = 0x349dc0;
/// The comparison `CdFs_FindFile` searches with.
const SEARCH_CMP: u32 = 0x349d6c;
const COMPARE_NAME: u32 = 0x2c9440;
const COMPARE_KEY_HASH: u32 = 0x2c9488;

const TABLE: u32 = 0x0100_0000;
const ENTRY: u32 = 0x10c;
const SCRATCH: u32 = 0x0180_0000;

/// One folder of a made-up disc.
struct Dir {
    files: Vec<(Vec<u8>, u32, u32)>,
    dirs: Vec<(Vec<u8>, Dir)>,
}

/// 1 to `max` characters.
fn name(g: &mut Rng, max: u32) -> Vec<u8> {
    let len = 1 + g.below(max);
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_";
    (0..len).map(|_| A[g.below(A.len() as u32) as usize]).collect()
}

fn make_dir(g: &mut Rng, depth: u32, files_left: &mut u32) -> Dir {
    let mut d = Dir { files: vec![], dirs: vec![] };
    let n = g.below(if depth == 0 { 60 } else { 25 }).min(*files_left);
    *files_left -= n;
    for _ in 0..n {
        let mut f = name(g, 8);
        match g.below(4) {
            0 => f.push(b'.'),
            1 => {}
            _ => {
                f.push(b'.');
                f.extend(name(g, 3));
            }
        }
        f.extend(b";1");
        d.files.push((f, g.next() >> 8, g.next() >> g.below(32)));
    }
    if g.below(4) == 0 {
        // "/…/AB" and "/…/QA" hash the same
        for f in [&b"AB;1"[..], b"QA;1"] {
            d.files.push((f.to_vec(), g.next() >> 8, g.next() >> 4));
        }
    }
    if depth < 3 {
        for _ in 0..g.below(if depth == 0 { 6 } else { 3 }) {
            let n = name(g, 8);
            d.dirs.push((n, make_dir(g, depth + 1, files_left)));
        }
    }
    d
}

fn count(d: &Dir) -> u32 {
    d.files.len() as u32 + d.dirs.iter().map(|(_, s)| count(s)).sum::<u32>()
}

fn record(lba: u32, size: u32, dir: bool, name: &[u8]) -> Vec<u8> {
    let len = 33 + name.len() + (name.len() + 1) % 2;
    let mut r = vec![0u8; len];
    r[0] = len as u8;
    r[2..6].copy_from_slice(&lba.to_le_bytes());
    r[6..10].copy_from_slice(&lba.to_be_bytes());
    r[10..14].copy_from_slice(&size.to_le_bytes());
    r[14..18].copy_from_slice(&size.to_be_bytes());
    // bit 1 marks a folder; some records are also hidden (bit 0)
    r[0x19] = (dir as u8) << 1 | (name[0] % 3 == 0) as u8;
    r[0x20] = name.len() as u8;
    r[0x21..0x21 + name.len()].copy_from_slice(name);
    r
}

/// The folder's records packed into sectors (none crosses a sector's end), with the folders' sectors given.
fn pack(records: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0u8; SECTOR];
    let mut at = 0;
    for r in records {
        if at + r.len() > SECTOR {
            out.resize(out.len() + SECTOR, 0);
            at = 0;
        }
        let base = out.len() - SECTOR;
        out[base + at..base + at + r.len()].copy_from_slice(r);
        at += r.len();
    }
    out
}

/// Lays a folder out from sector `lba` on (subfolders after it), writing into `img`; returns its own sectors
/// and those of it and everything under it.
fn lay_out(d: &Dir, lba: u32, parent: (u32, u32), img: &mut Vec<u8>, g: &mut Rng) -> (u32, u32) {
    let with = |subs: &[(u32, u32)]| {
        let mut rs = vec![record(lba, 0, true, &[0]), record(parent.0, parent.1, true, &[1])];
        let mut items: Vec<Vec<u8>> = d.files.iter().map(|(n, l, s)| record(*l, *s, false, n)).collect();
        items.extend(d.dirs.iter().zip(subs).map(|((n, _), (l, s))| record(*l, *s, true, n)));
        rs.extend(items);
        rs
    };
    // the size does not depend on the sectors' numbers
    let own = (pack(&with(&vec![(0, 0); d.dirs.len()])).len() / SECTOR) as u32;
    let mut next = lba + own;
    let mut subs = vec![];
    for (_, sub) in &d.dirs {
        let (o, n) = lay_out(sub, next, (lba, own * SECTOR as u32), img, g);
        subs.push((next, o * SECTOR as u32));
        next += n;
    }
    let mut rs = with(&subs);
    // the "." record carries the folder's own size
    rs[0] = record(lba, own * SECTOR as u32, true, &[0]);
    let bytes = pack(&rs);
    let at = lba as usize * SECTOR;
    if img.len() < at + bytes.len() {
        img.resize(at + bytes.len(), 0);
    }
    img[at..at + bytes.len()].copy_from_slice(&bytes);
    (own, next - lba)
}

fn paths(d: &Dir, prefix: &str, out: &mut Vec<String>, dirs: &mut Vec<String>) {
    for (n, _, _) in &d.files {
        out.push(format!("{prefix}{}", String::from_utf8_lossy(&n[..n.len() - 2]).trim_end_matches('.')));
    }
    for (n, sub) in &d.dirs {
        let p = format!("{prefix}{}", String::from_utf8_lossy(n));
        dirs.push(p.clone());
        paths(sub, &format!("{p}/"), out, dirs);
    }
}

fn cstr(r: &mut Runner, a: u32) -> String {
    let mut s = vec![];
    for i in 0.. {
        let mut b = [0u8];
        r.mem.read(a + i, &mut b).unwrap();
        if b[0] == 0 {
            break;
        }
        s.push(b[0]);
    }
    String::from_utf8(s).unwrap()
}

fn stub_drive(r: &mut Runner, img: Rc<RefCell<Vec<u8>>>) {
    r.allow(0x2cc808);
    r.allow(0x2ce114);
    for f in [0x2c9200, 0x2c9190, 0x2fb2b4, 0x2fcd4c, 0x2fd14c, 0x2fd260, 0x2fd008, 0x2fd478, QSORT, 0x2fa258, 0x2c9420, 0x2c9440, 0x2c9460, COMPARE_KEY_HASH] {
        r.allow(f);
    }
    for f in [CD_INIT, CREATE_SEMA, CREATE_THREAD] {
        r.stub(f, |c, _| c.set_gpr(2, 1));
    }
    r.stub(CD_GET_DISK_TYPE, |c, _| c.set_gpr(2, 0x12));
    for f in [CD_MMODE, CD_DISK_READY, CD_GET_TOC, CD_STOP, CD_INIT_EE_CB, CD_CALLBACK, FLUSH_CACHE, START_THREAD, ADD_POLL_TASK] {
        r.stub(f, |c, _| c.set_gpr(2, 1));
    }
    r.stub(READ_SECTOR, move |c, m| {
        let mut at = [0u8; 4];
        m.read(SECTOR_AT, &mut at).unwrap();
        let s = u32::from_le_bytes(at) as usize * SECTOR;
        let img = img.borrow();
        let mut sec = vec![0u8; SECTOR];
        if s < img.len() {
            let n = (img.len() - s).min(SECTOR);
            sec[..n].copy_from_slice(&img[s..s + n]);
        }
        m.write(SECTOR_BUF, &sec).unwrap();
        c.set_gpr(2, SECTOR_BUF as u64);
    });
}

#[test]
fn the_file_table_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let img = Rc::new(RefCell::new(vec![]));
    stub_drive(&mut r, img.clone());
    // sorting 1,000 entries of 0x10c bytes
    r.step_limit = 400_000_000;
    let iso = std::env::temp_dir().join(format!("tricky-disc-table-{}.iso", std::process::id()));
    let mut g = Rng(0xe2b0_0000_0000_0001);
    let (mut found, mut by_name, mut full_sectors) = (0u32, 0u32, 0u32);
    for seq in 0..200 {
        // exactly as many files as the game takes, one more, and many more
        let target = match seq {
            7 => 1001,
            8 => 1000,
            9 => 1250,
            _ => 0,
        };
        let too_many = target > disc::MAX_FILES as u32;
        let mut left = 1 + g.below(400);
        let mut root = make_dir(&mut g, 0, &mut left);
        if target > 0 {
            left = target - count(&root);
            while left > 0 {
                let n = left.min(200);
                left -= n;
                let mut d = Dir { files: vec![], dirs: vec![] };
                for k in 0..n {
                    d.files.push((format!("F{k}.BIN;1").into_bytes(), k, k));
                }
                root.dirs.push((format!("MORE{left}").into_bytes(), d));
            }
        }
        let mut image = vec![0u8; 0x14 * SECTOR];
        let root_lba = 0x14 + g.below(4);
        let (n, _) = lay_out(&root, root_lba, (root_lba, 0), &mut image, &mut g);
        // the root's ".." is itself
        let pvd = 0x10 * SECTOR;
        image[pvd] = 1;
        image[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        let rr = record(root_lba, n * SECTOR as u32, true, &[0]);
        image[pvd + 0x9c..pvd + 0x9c + rr.len()].copy_from_slice(&rr);
        full_sectors += image.chunks(SECTOR).filter(|s| s[0] != 0 && {
            let mut at = 0;
            while at < SECTOR && s[at] != 0 {
                at += s[at] as usize;
            }
            at == SECTOR
        }).count() as u32;
        std::fs::write(&iso, &image).unwrap();
        *img.borrow_mut() = image;

        let ours = Disc::open(&iso);
        r.mem.write(TABLE, &vec![0x55u8; (1300 * ENTRY) as usize]).unwrap();
        r.mem.write(0x349d50, &[0xaa; 0x44]).unwrap();
        r.mem.write(SECTOR_BUF, &[0x77; 0x800]).unwrap();
        r.cpu.set_gpr(4, 1000);
        r.cpu.set_gpr(5, TABLE as u64);
        r.cpu.set_gpr(6, 0);
        r.call(MOUNT).unwrap_or_else(|e| panic!("seq {seq}: mount: {e}"));
        let count = r.cpu.gpr(2) as u32;
        let disc = match ours {
            Err(DiscError::TooManyFiles(n)) => {
                assert_eq!(count, 0, "seq {seq}: the game mounts {n} files");
                assert!(too_many, "seq {seq}: {n} files, the game {count}");
                continue;
            }
            Err(e) => panic!("seq {seq}: {e}"),
            Ok(d) => d,
        };
        assert!(!too_many, "seq {seq}: {} files mounted", disc.files().len());
        assert_eq!(count as usize, disc.files().len(), "seq {seq}: the count");
        for (i, f) in disc.files().iter().enumerate() {
            let e = TABLE + ENTRY * i as u32;
            let t = (r.mem.read_u32(e), r.mem.read_u32(e + 4), r.mem.read_u32(e + 8), cstr(&mut r, e + 0xc));
            assert_eq!(t, (f.hash, f.lba, f.size, f.path.clone()), "seq {seq}: entry {i}");
        }
        let names = r.mem.read_u32(SEARCH_CMP);
        assert!(names == COMPARE_NAME || names == COMPARE_KEY_HASH, "seq {seq}: {names:#x}");
        by_name += (names == COMPARE_NAME) as u32;

        let (mut files, mut dirs) = (vec![], vec![]);
        paths(&root, "/", &mut files, &mut dirs);
        let mut tries: Vec<String> = vec![];
        for p in files.iter().chain(&dirs) {
            tries.push(p.clone());
            let mut q = p[1..].to_string();
            if g.below(2) == 0 {
                q = q.to_ascii_lowercase();
            }
            if g.below(2) == 0 {
                q = q.replace('/', "\\");
            }
            if g.below(3) == 0 {
                q.insert(0, '/');
            }
            if g.below(8) == 0 {
                q.push('X');
            }
            tries.push(q);
        }
        for _ in 0..20 {
            tries.push(String::from_utf8(name(&mut g, 12)).unwrap());
        }
        tries.push(String::new());
        tries.push("/".into());
        tries.push("//AB".into());
        for t in &tries {
            let ours = disc.find(t).map(|f| disc.files().iter().position(|x| std::ptr::eq(x, f)).unwrap() as u32 + 1);
            let mut s = t.clone().into_bytes();
            s.push(0);
            r.mem.write(SCRATCH, &s).unwrap();
            r.mem.write_u32(SCRATCH + 0x200, 0xdead);
            r.cpu.set_gpr(4, SCRATCH as u64);
            r.cpu.set_gpr(6, (SCRATCH + 0x200) as u64);
            r.call(FIND_FILE).unwrap_or_else(|e| panic!("seq {seq}: find {t:?}: {e}"));
            let theirs = (r.cpu.gpr(2) as u32, r.mem.read_u32(SCRATCH + 0x200));
            assert_eq!(theirs, (ours.is_some() as u32, ours.unwrap_or(0)), "seq {seq}: find {t:?}");
            found += ours.is_some() as u32;
        }
    }
    let _ = std::fs::remove_file(&iso);
    println!("{found} files found, {by_name} discs searched by path, {full_sectors} sectors filled to the end");
    assert!(found > 20_000 && by_name > 20 && full_sectors > 0);
}

#[test]
fn qsort_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.allow(COMPARE_KEY_HASH);
    let mut g = Rng(0x9507_0000_0000_0001);
    for seq in 0..3000 {
        let n = match g.below(4) {
            0 => g.below(8),
            1 => g.below(45),
            _ => g.below(400),
        };
        let mut pool: Vec<u32> = (0..1 + g.below(12)).map(|_| g.next()).collect();
        // keys 0x80000000 apart compare below each other both ways round
        pool.push(pool[0] ^ 0x8000_0000);
        // (key, place): only the key is compared, so where equal keys end up shows
        let mut v: Vec<(u32, u32)> = (0..n)
            .map(|i| match g.below(3) {
                0 => (pool[g.below(pool.len() as u32) as usize], i),
                1 => (g.below(64), i),
                _ => (g.next(), i),
            })
            .collect();
        let bytes: Vec<u8> = v.iter().flat_map(|x| [x.0.to_le_bytes(), x.1.to_le_bytes()]).flatten().collect();
        r.mem.write(TABLE, &bytes).unwrap();
        disc::qsort(&mut v, |a, b| a.0.wrapping_sub(b.0) as i32);
        for (k, a) in [TABLE, n, 8, COMPARE_KEY_HASH].iter().enumerate() {
            r.cpu.set_gpr(4 + k, *a as u64);
        }
        r.call(QSORT).unwrap_or_else(|e| panic!("seq {seq}: {e}"));
        let theirs: Vec<(u32, u32)> = (0..n).map(|i| (r.mem.read_u32(TABLE + 8 * i), r.mem.read_u32(TABLE + 8 * i + 4))).collect();
        assert_eq!(theirs, v, "seq {seq}: n {n}");
    }
}
