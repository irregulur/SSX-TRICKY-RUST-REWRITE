//! `tricky_game::mempool` (board row E2s) against the original in the function runner (TRICKY_ELF; skipped without
//! it): the game's block manager (0x345e48) made with random classes (counts 0.., sizes of any rounding, negative
//! ones) over a heap of our memory library, then random block allocations (sizes that fit one class, several or
//! none, with the three header words), frees through `MemPool_Free`, `Mem_Free` and `Mem_Delete` (pool blocks, heap
//! blocks, 0), `Object_Delete` on objects with this-adjusting vtables, the destroy; the two seeders. Memory and
//! answers compared after every call.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::eamem;
use tricky_game::mempool as mp;
use tricky_game::pf::Mem;
use tricky_game::rand::Streams;

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
const ARENA_SIZE: u32 = 0x4_0000;
const TABLE: u32 = 0x0150_0000;
const STACK: u32 = 0x01ff_0000;
/// A function the objects' vtables point at: the test answers it
const DTOR: u32 = 0x0160_0000;

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    for f in [
        0x23d160, 0x23d368, 0x23d3b0, 0x23d420, 0x14a040, 0x114f80, 0x23d738, 0x23d498, 0x23d4c8, 0x2402c0, 0x23d5d0,
        0x2cd278, 0x2ccf70, 0x2cca00, 0x2ccfc0, 0x2cc9a0, 0x2cd168, 0x2cc808, 0x2ce114, 0x2ce228, 0x2cd990, 0x2fd260, 0x2fd14c,
    ] {
        r.allow(f);
    }
    r.stub(0x2fca28, |c, m| {
        let (buf, arg) = (c.gpr(4) as u32, c.gpr(6) as u32);
        let mut s = vec![];
        let mut a = arg;
        loop {
            let mut b = [0u8];
            m.read(a, &mut b).unwrap();
            if b[0] == 0 {
                break;
            }
            s.push(b[0]);
            a += 1;
        }
        s.extend(b" X\0");
        m.write(buf, &s).unwrap();
    });
    Some(r)
}

fn load(r: &mut Runner, m: &Mem) {
    for (a, v) in &m.regions {
        r.mem.write(*a, v).unwrap();
    }
}

fn check(r: &mut Runner, m: &Mem, what: &str, ours: Option<u32>) {
    for (a, v) in &m.regions {
        let mut t = vec![0u8; v.len()];
        r.mem.read(*a, &mut t).unwrap();
        if &t != v {
            let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
            panic!("{what}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
        }
    }
    if let Some(o) = ours {
        assert_eq!(r.cpu.gpr(2) as u32, o, "{what}: the answer");
    }
}

fn call(r: &mut Runner, f: u32, args: &[u32]) {
    for (k, a) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
    }
    r.call(f).unwrap_or_else(|e| panic!("{f:#x} {args:x?}: {e}"));
}

fn no_oom() -> impl FnMut(&mut Mem, u32, u32, u32, u32) -> u32 {
    |_, _, _, _, _| 0
}

#[test]
fn the_block_manager_matches_the_original() {
    let Some(mut r) = runner() else { return };
    let calls = Rc::new(RefCell::new(vec![]));
    {
        let calls = calls.clone();
        r.stub(DTOR, move |c, _| calls.borrow_mut().push((c.gpr(4) as u32, c.gpr(5) as u32)));
    }
    let mut heaps = vec![0u8; 0x40];
    r.mem.read(eamem::HEAPS, &mut heaps).unwrap();
    let mut g = Rng(0x4d50_6f6f_0000_0001);
    let (mut taken, mut full, mut to_heap) = (0u32, 0u32, 0u32);
    for seq in 0..1000 {
        let mut name = vec![0u8; 0x10];
        r.mem.read(mp::NAME, &mut name).unwrap();
        let mut m = Mem {
            regions: vec![
                (ARENA, (0..ARENA_SIZE).map(|_| g.next() as u8).collect()),
                (TABLE, (0..0x200).map(|_| g.next() as u8).collect()),
                (mp::BLOCKS, (0..0x10).map(|_| g.next() as u8).collect()),
                (0x345e58, vec![0; 0x30]),
                (mp::NAME, name),
                (eamem::HEAPS, heaps.clone()),
                (eamem::OUT_OF_MEMORY, vec![0; 4]),
            ],
        };
        load(&mut r, &m);
        // heap 0 over the arena (what flags 0 allocate from)
        let n = eamem::init_heap(&mut m, 0, mp::NAME, ARENA, ARENA_SIZE, 0x10, 0x40, 0, false, false, None, 0);
        for (k, a) in [0, mp::NAME, ARENA, ARENA_SIZE, 0x10, 0x40, 0, 0].iter().enumerate() {
            r.cpu.set_gpr(4 + k, *a as u64);
        }
        for k in 0..3 {
            r.mem.write_u32(STACK + 8 + 8 * k, 0);
        }
        r.call(0x2cd278).unwrap();
        check(&mut r, &m, &format!("seq {seq}: heap"), Some(n));

        let classes = g.below(6) as i32;
        for k in 0..classes as u32 {
            // a class of 0 blocks points its free list at the next class's first block (or past the end): the game's
            // tables never have one, and the test can't follow it into random memory
            let count = [1, 1 + g.below(8), 1 + g.below(40)][g.below(3) as usize];
            // sizes that round to 0 or below make blocks whose data is the next block (or, for the last, the heap
            // block after the pool, which `Mem_Free` then hands to the heaps): the game's never do
            let size = match g.below(6) {
                0 => 1 + g.below(16),
                1 => 16 * (1 + g.below(20)),
                _ => 1 + g.below(0x200),
            };
            m.w32(TABLE + 8 * k, count);
            m.w32(TABLE + 8 * k + 4, size);
        }
        load(&mut r, &m);
        mp::init(&mut m, mp::BLOCKS, classes, TABLE, &mut no_oom());
        call(&mut r, 0x23d160, &[mp::BLOCKS, classes as u32, TABLE]);
        check(&mut r, &m, &format!("seq {seq}: init {classes}"), None);
        if m.r32(mp::BLOCKS) == 0 {
            continue;
        }
        let mut pool_live = vec![];
        let mut heap_live = vec![];
        for step in 0..150 {
            let what = format!("seq {seq} step {step}");
            match g.below(10) {
                0..=4 => {
                    let size = [g.below(0x40), g.below(0x220), g.next(), 0][g.below(4) as usize];
                    let w = [g.next(), g.next(), g.next()];
                    let ours = mp::alloc(&mut m, mp::BLOCKS, size, w[0], w[1], w[2]);
                    call(&mut r, 0x23d3b0, &[mp::BLOCKS, size, g.next(), w[0], w[1], w[2]]);
                    check(&mut r, &m, &format!("{what}: alloc {size:#x}"), Some(ours));
                    if ours != 0 {
                        pool_live.push(ours);
                        taken += 1;
                    } else {
                        full += 1;
                    }
                }
                5 => {
                    let size = g.below(0x100);
                    let ours = eamem::alloc(&mut m, 0, size, 0, &mut no_oom());
                    call(&mut r, 0x2ccf70, &[0, size, 0]);
                    check(&mut r, &m, &format!("{what}: heap alloc"), Some(ours));
                    if ours != 0 {
                        heap_live.push(ours);
                    }
                }
                6..=8 if !pool_live.is_empty() || !heap_live.is_empty() => {
                    let from_pool = heap_live.is_empty() || (!pool_live.is_empty() && g.below(3) != 0);
                    let p = if from_pool {
                        pool_live.swap_remove(g.below(pool_live.len() as u32) as usize)
                    } else {
                        to_heap += 1;
                        heap_live.swap_remove(g.below(heap_live.len() as u32) as usize)
                    };
                    let f = if from_pool { [0x23d420, 0x14a040, 0x114f80][g.below(3) as usize] } else { [0x14a040, 0x114f80][g.below(2) as usize] };
                    let ours = if f == 0x23d420 { mp::free(&mut m, mp::BLOCKS, p) } else { mp::mem_free(&mut m, p) };
                    if f == 0x23d420 {
                        call(&mut r, f, &[mp::BLOCKS, p]);
                    } else {
                        call(&mut r, f, &[p]);
                    }
                    check(&mut r, &m, &format!("{what}: free {f:#x} {p:#x}"), Some(ours));
                }
                _ => {
                    // an object in a heap block: its vtable in the table area, entry 1 adjusting `this`
                    let obj = if g.below(5) == 0 { 0 } else { TABLE + 0x100 + 4 * g.below(16) };
                    let vt = TABLE + 0x180;
                    let adjust = (g.below(64) as i16 - 32) as u16;
                    m.w32(vt + 0xc, DTOR);
                    m.w16(vt + 8, adjust);
                    if obj != 0 {
                        m.w32(obj, vt);
                    }
                    load(&mut r, &m);
                    let mut ours = vec![];
                    mp::object_delete(&mut m, obj, &mut |_, f, this, how| {
                        assert_eq!(f, DTOR);
                        ours.push((this, how));
                    });
                    calls.borrow_mut().clear();
                    call(&mut r, 0x23d738, &[obj]);
                    check(&mut r, &m, &format!("{what}: delete {obj:#x}"), None);
                    assert_eq!(*calls.borrow(), ours, "{what}: the destructor's calls");
                }
            }
        }
        mp::destroy(&mut m, mp::BLOCKS);
        call(&mut r, 0x23d368, &[mp::BLOCKS]);
        check(&mut r, &m, &format!("seq {seq}: destroy"), None);
        // the seeders
        let (a, b) = (g.next(), g.next());
        let mut s = Streams::default();
        s.seed_fx(a);
        s.seed_game(b);
        call(&mut r, 0x23d498, &[a]);
        call(&mut r, 0x23d4c8, &[b]);
        for (k, w) in s.fx.s.iter().chain(s.game.s.iter()).enumerate() {
            assert_eq!(r.mem.read_u32(0x345e58 + 4 * k as u32), *w, "seq {seq}: seed word {k}");
        }
        mp::debug_printf();
        call(&mut r, 0x23d5d0, &[0x1234, 1, 2, 3]);
    }
    println!("{taken} blocks taken, {full} asks with none, {to_heap} frees to the heap");
    assert!(taken > 10_000 && full > 5_000 && to_heap > 1_000);
}
