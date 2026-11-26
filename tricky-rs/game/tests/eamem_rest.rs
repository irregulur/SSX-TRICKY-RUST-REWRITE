//! The rest of `tricky_game::eamem` (board row E2n) against the original in the function runner (TRICKY_ELF;
//! skipped without it): one to three heaps with random settings and random use (made by our heap code, checked by
//! `tests/eamem_heap.rs`), the table sometimes holding a heap twice, then one call: the heap checks (on sound
//! heaps and on heaps with broken tags, flags, links, sizes and "BEND"s), `EAMem_FreeAll`, `EAMem_DestroyHeap`,
//! `EAMem_DestroyAll`, `EAMem_ClearHeapTable`, `EAMem_Zero`, `Compress_Alloc` / `Compress_Free`,
//! `EAMem_InitMainHeap`, `EAMem_InitDefaultHeap` / `Max`. Memory, the answer and the host calls compared. Where
//! the original runs into a loop that never ends (a broken free list) or out of memory, ours is not run.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::eamem;
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
    fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[self.below(of.len() as u32) as usize]
    }
}

const ARENA: u32 = 0x0100_0000;
const ARENA_SIZE: u32 = 0x8000;
const NAMES: u32 = 0x0170_0000;
const GLOBALS: u32 = 0x3bb970;
const IMAGE: u32 = 0x10_0000;

const ZERO: u32 = 0x2cc938;
const COMPRESS_ALLOC: u32 = 0x2cc958;
const COMPRESS_FREE: u32 = 0x2cc980;
const CLEAR_HEAP_TABLE: u32 = 0x2cd250;
const DESTROY_HEAP: u32 = 0x2cd500;
const FREE_ALL: u32 = 0x2cd570;
const INIT_MAIN_HEAP: u32 = 0x2cd798;
const INIT_DEFAULT_HEAP: u32 = 0x2cd820;
const INIT_DEFAULT_HEAP_MAX: u32 = 0x2cd890;
const DESTROY_ALL: u32 = 0x2cd8b0;
const IS_BLOCK_HEADER: u32 = 0x2cdc60;
const CHECK_NEIGHBOURS: u32 = 0x2cdcc8;
const CHECK_BACK_LINKS: u32 = 0x2cddc0;
const CHECK_IN_HEAP: u32 = 0x2cde70;
const VALIDATE_HEAP: u32 = 0x2cdf08;
const VALIDATE_ALL: u32 = 0x2cdfd8;

const LOCK: u32 = 0x77;

#[derive(Debug, PartialEq, Clone)]
enum Host {
    Lock(u32),
    Unlock(u32),
    Destroy(u32),
    Exit(u32),
    Malloc(u32),
    Free(u32),
}

fn read_cstr(m: &mut r5900::Mem, mut a: u32) -> Vec<u8> {
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

/// Every block header of a heap along its chain, and its sentinel
fn blocks(m: &Mem, h: u32) -> Vec<u32> {
    let mut v = vec![h + 0x10];
    let mut b = m.r32(h + 8);
    while b != 0 && v.len() < 400 {
        v.push(b);
        b = m.r32(b + 8);
    }
    v
}

#[test]
fn the_rest_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.step_limit = 300_000;
    for f in [
        ZERO, COMPRESS_ALLOC, COMPRESS_FREE, CLEAR_HEAP_TABLE, DESTROY_HEAP, FREE_ALL, INIT_MAIN_HEAP, INIT_DEFAULT_HEAP,
        INIT_DEFAULT_HEAP_MAX, DESTROY_ALL, IS_BLOCK_HEADER, CHECK_NEIGHBOURS, CHECK_BACK_LINKS, CHECK_IN_HEAP,
        VALIDATE_HEAP, VALIDATE_ALL, 0x2ce2c0, 0x2cc9a0, 0x2cd990, 0x2cd278, 0x2cd168, 0x2cca00, 0x2ccf70, 0x2ccfc0, 0x2cc808, 0x2ce114,
        0x2ce228, 0x2fd14c, 0x2fd260,
    ] {
        r.allow(f);
    }
    assert_eq!(r.gp().wrapping_sub(0x7f74), eamem::MAIN_HEAP_BASE);
    assert_eq!(r.gp().wrapping_sub(0x7f78), eamem::ALLOC_FLAGS);
    assert_eq!(r.gp().wrapping_sub(0x7f64), eamem::ALLOC_NAME);
    assert_eq!(r.gp().wrapping_sub(0x7f44), eamem::CHECK_ERROR);
    r.stub(0x2fca28, |c, m| {
        let (buf, fmt, arg) = (c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32);
        let fmt = read_cstr(m, fmt);
        let arg = read_cstr(m, arg);
        assert_eq!(&fmt[..2], b"%s");
        let mut out = [&arg[..], &fmt[2..]].concat();
        out.push(0);
        m.write(buf, &out).unwrap();
    });
    let log = Rc::new(RefCell::new(Vec::new()));
    r.stub(0x2cf468, |c, _| c.set_gpr(2, LOCK as u64));
    for (a, k) in [(0x2cf4d0u32, 0), (0x2cf550, 1), (0x2cf4b0, 2), (0x2ce9d0, 3), (0x2fa840, 5)] {
        let l = log.clone();
        r.stub(a, move |c, _| {
            let v = c.gpr(4) as u32;
            l.borrow_mut().push(match k {
                0 => Host::Lock(v),
                1 => Host::Unlock(v),
                2 => Host::Destroy(v),
                3 => Host::Exit(v),
                _ => Host::Free(v),
            });
        });
    }
    let malloc_answer = Rc::new(RefCell::new(0u32));
    {
        let (l, a) = (log.clone(), malloc_answer.clone());
        r.stub(0x2fa818, move |c, _| {
            l.borrow_mut().push(Host::Malloc(c.gpr(4) as u32));
            c.set_gpr(2, *a.borrow() as u64);
        });
    }
    let mut globals = vec![0u8; 0x40];
    r.mem.read(GLOBALS, &mut globals).unwrap();
    let mut table = vec![0u8; 0x40];
    r.mem.read(eamem::HEAPS, &mut table).unwrap();
    r.mem.write(0, &[0; 0x40]).unwrap();
    // the program's own bytes, for the checks that follow a broken link into them (a name, a size read as an
    // address); after every other region, so the globals inside it are read and written in theirs
    let mut image = vec![0u8; 0x40_0000];
    r.mem.read(IMAGE, &mut image).unwrap();

    let mut g = Rng(0xe2a0_0000_0000_00e2);
    let mut seen = std::collections::BTreeMap::<String, u32>::new();
    let mut skipped = 0;
    for seq in 0..6000 {
        let mut m = Mem {
            regions: vec![
                (0, vec![0; 0x40]),
                (ARENA, (0..4 * ARENA_SIZE).map(|_| g.next() as u8).collect()),
                (NAMES, vec![0; 0x100]),
                (eamem::HEAPS, table.clone()),
                (GLOBALS, globals.clone()),
            ],
        };
        let names: Vec<u32> = (0..4)
            .map(|k| {
                let a = NAMES + 0x40 * k;
                for i in 0..1 + g.below(20) {
                    m.w8(a + i, b'a' + g.below(26) as u8);
                }
                a
            })
            .collect();
        // the heaps, made and used by our code
        let mut heaps = vec![];
        let mut live = vec![];
        for k in 0..1 + g.below(3) {
            let id = loop {
                let id = g.below(16);
                if !heaps.iter().any(|&(i, _)| i == id) {
                    break id;
                }
            };
            let base = ARENA + k * ARENA_SIZE + 0x10 * g.below(0x40);
            let size = 0x400 + 4 * g.below((ARENA_SIZE - 0x800) / 4);
            let after = g.pick(&[0, 0, 4, 8, 0x10]);
            let (bend, named) = (after >= 4 && g.below(2) == 1, g.below(2) == 1);
            let lock = g.below(3) == 0;
            let h = base + 0x10;
            let (al, big) = (g.pick(&[4, 8, 0x10, 0x40]), g.pick(&[0x10, 0x40, 0x80]));
            eamem::init_heap(&mut m, id, g.pick(&names), base, size, al, big, after, bend, named, lock.then_some(LOCK), g.next());
            heaps.push((id, h));
            let mut mine = vec![];
            for _ in 0..g.below(60) {
                match g.below(4) {
                    0 | 1 => {
                        let mut flags = id;
                        for bit in [0x10, 0x20, 0x40] {
                            if g.below(4) == 0 {
                                flags |= bit;
                            }
                        }
                        if bend && g.below(2) == 0 {
                            flags |= 0x200;
                        }
                        let name = if named && g.below(2) == 0 { g.pick(&names) } else { 0 };
                        if named && name != 0 {
                            flags |= 0x100;
                        }
                        let p = eamem::alloc_ex(&mut m, name, g.below(0x300), 0, 0, flags, &mut |_, _, _, _, _| 0);
                        if p != 0 {
                            mine.push(p);
                        }
                    }
                    2 if !mine.is_empty() => {
                        let p = mine.swap_remove(g.below(mine.len() as u32) as usize);
                        eamem::free(&mut m, p);
                    }
                    _ if !mine.is_empty() => {
                        let p = g.pick(&mine);
                        eamem::resize(&mut m, p, g.below(0x200));
                    }
                    _ => {}
                }
            }
            live.extend(mine);
            let x = g.next();
            m.w32(h + 0x40, g.pick(&[0, 0, x]));
        }
        if g.below(4) == 0 {
            let (i, j) = (g.below(16), g.below(16));
            m.w32(eamem::HEAPS + 4 * j, m.r32(eamem::HEAPS + 4 * i));
        }
        m.w32(eamem::MAIN_HEAP_BASE, g.pick(&[0, 0, ARENA + 3 * ARENA_SIZE]));
        m.w32(eamem::CHECK_ERROR, g.pick(&[0, 7]));
        let (hid, h) = g.pick(&heaps);
        let all = blocks(&m, h);
        // broken heaps for the checks
        let op = g.below(19);
        if op < 9 && g.below(3) != 0 {
            for _ in 0..1 + g.below(3) {
                let b = g.pick(&all[1..]);
                match g.below(6) {
                    0 => m.w16(b, g.pick(&[0x424d, 0x4246, 0x424f, 0x4253, 0x1234])),
                    1 => m.w16(b + 2, m.r16(b + 2) ^ g.pick(&[0x8000, 0x4000, 0x200])),
                    2 => {
                        let off = g.pick(&[8, 0xc, 0x10, 0x14]);
                        let to = if g.below(6) == 0 { 0 } else { g.pick(&all) };
                        m.w32(b + off, to);
                    }
                    3 if m.r16(b) == 0x424d => m.w32(b + 4, m.r32(b + 4).wrapping_add(g.pick(&[4, 0u32.wrapping_sub(4)]))),
                    4 if m.r16(b) == 0x424d => {
                        let at = b + 0x10 + m.r32(b + 4) + g.below(4);
                        m.w8(at, m.r8(at) ^ 1);
                    }
                    _ => {
                        let s = h + 0x10;
                        let off = g.pick(&[0x10, 0x14]);
                        m.w32(s + off, g.pick(&all));
                    }
                }
            }
        }
        // a block that looks free must link to blocks (its +0x10 / +0x14 are otherwise data, and the walk along
        // them would leave the memory our side has)
        for &b in &all[1..] {
            if m.r32(b) & 0x4000_ffff == 0x4000_4246 {
                for off in [0x10, 0x14] {
                    let to = m.r32(b + off);
                    if to != 0 && !all.contains(&to) {
                        m.w32(b + off, g.pick(&all));
                    }
                }
            }
        }
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        if op < 9 {
            m.regions.push((IMAGE, image.clone()));
        }
        let args: Vec<u32>;
        let name;
        let mut answer = true;
        match op {
            0 | 1 => {
                name = "ValidateAll";
                args = vec![];
            }
            2 | 3 => {
                name = "ValidateHeap";
                args = vec![hid | (g.below(4) << 4)];
            }
            4 => {
                name = "CheckNeighbours";
                args = vec![g.pick(&all)];
            }
            5 => {
                name = "CheckBackLinks";
                args = vec![g.pick(&all)];
            }
            6 => {
                name = "IsBlockHeader";
                args = vec![if g.below(5) == 0 { 0 } else { g.pick(&all) }];
            }
            7 | 8 => {
                name = "CheckInHeap";
                args = vec![h + 0x10];
            }
            9 => {
                name = "FreeAll";
                args = vec![hid];
                answer = false;
            }
            10 => {
                name = "DestroyHeap";
                args = vec![g.below(32)];
            }
            11 => {
                name = "DestroyAll";
                args = vec![];
                answer = false;
            }
            12 => {
                name = "ClearHeapTable";
                args = vec![];
                answer = false;
            }
            13 => {
                name = "Zero";
                args = vec![ARENA + g.below(ARENA_SIZE), g.below(0x100)];
                answer = false;
            }
            14 => {
                name = "CompressAlloc";
                let (id, _) = g.pick(&heaps);
                m.w32(eamem::ALLOC_FLAGS, id | g.pick(&[0, 0x10, 0x20, 0x40]));
                m.w32(eamem::ALLOC_NAME, g.pick(&[0, names[0]]));
                r.mem.write_u32(eamem::ALLOC_FLAGS, m.r32(eamem::ALLOC_FLAGS));
                r.mem.write_u32(eamem::ALLOC_NAME, m.r32(eamem::ALLOC_NAME));
                args = vec![g.below(0x400)];
            }
            15 if !live.is_empty() => {
                name = "CompressFree";
                args = vec![g.pick(&live)];
            }
            15 | 16 => {
                name = "InitMainHeap";
                args = vec![ARENA + 3 * ARENA_SIZE + 0x10 * g.below(0x40), 0x400 + 4 * g.below((ARENA_SIZE - 0x800) / 4)];
            }
            17 => {
                name = "InitDefaultHeap";
                let x = ARENA + 3 * ARENA_SIZE + 0x10 * g.below(0x40);
                *malloc_answer.borrow_mut() = g.pick(&[0, x]);
                args = vec![0x400 + 4 * g.below((ARENA_SIZE - 0x800) / 4)];
            }
            _ => {
                name = "InitDefaultHeapMax";
                *malloc_answer.borrow_mut() = 0;
                args = vec![];
            }
        }
        let f = match name {
            "ValidateAll" => VALIDATE_ALL,
            "ValidateHeap" => VALIDATE_HEAP,
            "CheckNeighbours" => CHECK_NEIGHBOURS,
            "CheckBackLinks" => CHECK_BACK_LINKS,
            "IsBlockHeader" => IS_BLOCK_HEADER,
            "CheckInHeap" => CHECK_IN_HEAP,
            "FreeAll" => FREE_ALL,
            "DestroyHeap" => DESTROY_HEAP,
            "DestroyAll" => DESTROY_ALL,
            "ClearHeapTable" => CLEAR_HEAP_TABLE,
            "Zero" => ZERO,
            "CompressAlloc" => COMPRESS_ALLOC,
            "CompressFree" => COMPRESS_FREE,
            "InitMainHeap" => INIT_MAIN_HEAP,
            "InitDefaultHeap" => INIT_DEFAULT_HEAP,
            _ => INIT_DEFAULT_HEAP_MAX,
        };
        for (k, a) in args.iter().enumerate() {
            r.cpu.set_gpr(4 + k, *a as u64);
        }
        log.borrow_mut().clear();
        if let Err(e) = r.call(f) {
            assert!(
                matches!(e, r5900::Error::TooLong { .. } | r5900::Error::BadAddress { .. }),
                "seq {seq}: {name} {args:x?}: {e}"
            );
            skipped += 1;
            continue;
        }
        let theirs = r.cpu.gpr(2) as u32;
        let key = if op < 9 { format!("{name} {theirs} e{}", r.mem.read_u32(eamem::CHECK_ERROR)) } else { name.to_string() };
        *seen.entry(key).or_default() += 1;
        let lg = RefCell::new(Vec::new());
        let ours = {
            let mut destroy = |l| lg.borrow_mut().push(Host::Destroy(l));
            let a = |k: usize| args[k];
            match f {
                VALIDATE_ALL => eamem::validate_all(&mut m),
                VALIDATE_HEAP => eamem::validate_heap(&mut m, a(0)),
                CHECK_NEIGHBOURS => eamem::check_neighbours(&mut m, a(0)),
                CHECK_BACK_LINKS => eamem::check_back_links(&mut m, a(0)),
                IS_BLOCK_HEADER => eamem::is_block_header(&mut m, a(0)),
                CHECK_IN_HEAP => eamem::check_in_heap(&mut m, a(0)),
                FREE_ALL => {
                    eamem::free_all(&mut m, a(0));
                    0
                }
                DESTROY_HEAP => eamem::destroy_heap(&mut m, a(0), &mut destroy),
                DESTROY_ALL => {
                    eamem::destroy_all(&mut m, &mut destroy, &mut |p| lg.borrow_mut().push(Host::Free(p)));
                    0
                }
                CLEAR_HEAP_TABLE => {
                    eamem::clear_heap_table(&mut m);
                    0
                }
                ZERO => {
                    eamem::zero(&mut m, a(0), a(1));
                    0
                }
                COMPRESS_ALLOC => eamem::compress_alloc(&mut m, a(0), &mut |_, _, _, _, _| unreachable!()),
                COMPRESS_FREE => eamem::compress_free(&mut m, a(0)),
                INIT_MAIN_HEAP => eamem::init_main_heap(&mut m, a(0), a(1), LOCK),
                INIT_DEFAULT_HEAP | INIT_DEFAULT_HEAP_MAX => {
                    let mut exit = |p| lg.borrow_mut().push(Host::Exit(p));
                    let ans = *malloc_answer.borrow();
                    let mut ml = vec![];
                    let v = if f == INIT_DEFAULT_HEAP {
                        eamem::init_default_heap(&mut m, a(0), &mut exit, &mut |n| {
                            ml.push(n);
                            ans
                        }, LOCK)
                    } else {
                        eamem::init_default_heap_max(&mut m, &mut exit, &mut |n| {
                            ml.push(n);
                            ans
                        }, LOCK)
                    };
                    lg.borrow_mut().extend(ml.into_iter().map(Host::Malloc));
                    v
                }
                _ => unreachable!(),
            }
        };
        let what = format!("seq {seq}: {name} {args:x?}");
        if answer {
            assert_eq!(theirs, ours, "{what}: the answer");
        }
        for (a, v) in m.regions.iter().filter(|(a, _)| *a != IMAGE) {
            let mut t = vec![0u8; v.len()];
            r.mem.read(*a, &mut t).unwrap();
            if &t != v {
                let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
                panic!("{what}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
            }
        }
        // the host calls: ours has no lock around the checks (our host's), so those are left out of the comparison
        let theirs_log: Vec<Host> = log.borrow().iter().filter(|x| !matches!(x, Host::Lock(_) | Host::Unlock(_))).cloned().collect();
        assert_eq!(theirs_log, lg.into_inner(), "{what}: the host calls");
    }
    println!("{seen:?}, {skipped} skipped (the original never returns or leaves memory)");
    assert!(skipped < 300);
}
