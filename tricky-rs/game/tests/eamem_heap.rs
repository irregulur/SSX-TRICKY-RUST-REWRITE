//! `tricky_game::eamem`'s heaps (board row E2j) against the original in the function runner (TRICKY_ELF; skipped
//! without it): a heap made with random settings in a random arena (alignment, large alignment, kept bytes,
//! "BEND", names, a lock, its number), then random allocations (every flag that changes the search or the block,
//! any alignment and offset, sizes that fit and that don't, through `EAMem_AllocEx` and its wrappers), frees and
//! resizes (−1, small, larger than the room), with an out-of-memory function that asks for a retry or not; the
//! arena, the heap table and every answer compared after each step.

use std::cell::{Cell, RefCell};
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
const SIZE: u32 = 0x8000;
const NAMES: u32 = 0x0170_0000;
const STACK: u32 = 0x01ff_0000;
const OOM_FN: u32 = 0x2cd790;

const INIT_HEAP: u32 = 0x2cd278;
const ALLOC_EX: u32 = 0x2cca00;
const ALLOC_ALIGNED: u32 = 0x2ccf50;
const ALLOC: u32 = 0x2ccf70;
const ALLOC0: u32 = 0x2ccf98;
const FREE: u32 = 0x2ccfc0;
const RESIZE: u32 = 0x2cda18;
const GET_SIZE: u32 = 0x2cd9b0;
const GET_NAME: u32 = 0x2cd9b8;
const BLOCK_HEADER_SIZE: u32 = 0x2cc9a0;
const HEAP_HEADER_SIZE: u32 = 0x2cd990;

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

#[test]
fn heaps_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [INIT_HEAP, ALLOC_EX, FREE, RESIZE, GET_NAME, BLOCK_HEADER_SIZE, HEAP_HEADER_SIZE, 0x2cd168, 0x2cc808, 0x2ce114, 0x2ce228, 0x2fd14c, 0x2fd260] {
        r.allow(f);
    }
    r.stub(0x2fca28, |c, m| {
        let (buf, fmt, arg) = (c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32);
        let fmt = read_cstr(m, fmt);
        let arg = read_cstr(m, arg);
        assert_eq!(&fmt[..2], b"%s");
        let mut out = [&arg[..], &fmt[2..]].concat();
        out.push(0);
        m.write(buf, &out).unwrap();
    });
    let locks = Rc::new(Cell::new(0i32));
    r.stub(0x2cf468, |c, _| c.set_gpr(2, 0x77));
    let l = locks.clone();
    r.stub(0x2cf4d0, move |c, _| {
        assert_eq!(c.gpr(4) as u32, 0x77);
        l.set(l.get() + 1)
    });
    let l = locks.clone();
    r.stub(0x2cf550, move |c, _| {
        assert_eq!(c.gpr(4) as u32, 0x77);
        l.set(l.get() - 1)
    });
    // the out-of-memory function: answers 1 (try again) `retries` times, then `last`
    let retries = Rc::new(Cell::new(0u32));
    let last = Rc::new(Cell::new(0u32));
    let theirs_oom = Rc::new(RefCell::new(Vec::new()));
    {
        let (retries, last, log) = (retries.clone(), last.clone(), theirs_oom.clone());
        r.stub(OOM_FN, move |c, _| {
            log.borrow_mut().push((c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32));
            let a = if retries.get() > 0 {
                retries.set(retries.get() - 1);
                1
            } else {
                last.get()
            };
            c.set_gpr(2, a as u64);
        });
    }
    let mut table = vec![0u8; 0x40];
    r.mem.read(eamem::HEAPS, &mut table).unwrap();

    let mut g = Rng(0xe2a0_0000_0000_0001);
    let (mut ops, mut oom_calls, mut fails, mut joins) = (0u32, 0u32, 0u32, 0u32);
    for seq in 0..400 {
        let mut m = Mem {
            regions: vec![
                (ARENA, (0..SIZE).map(|_| g.next() as u8).collect()),
                (NAMES, vec![0; 0x100]),
                (eamem::HEAPS, table.clone()),
                (eamem::OUT_OF_MEMORY, vec![0; 4]),
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
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let id = g.below(16);
        let base = ARENA + 0x10 * g.below(0x40) + g.pick(&[0, 0, 4, 8]);
        let size = 0x400 + 4 * g.below((SIZE - 0x800) / 4);
        let align = g.pick(&[4, 8, 0x10, 0x40]);
        let big = g.pick(&[0x10, 0x40, 0x80]);
        let after = g.pick(&[0, 0, 4, 8, 0x10, 0x20]);
        // "BEND" needs 4 kept bytes and the 0x800 zeros 16, and a block's name a named heap, or they write over
        // the next block's header (as they would in the game; its heaps keep none and ask for neither)
        let bend = after >= 4 && g.below(2) == 1;
        let named = g.below(2) == 1;
        let lock = g.below(3) == 0;
        let word = g.next();
        let heap_name = g.pick(&names);
        let n = eamem::init_heap(&mut m, id, heap_name, base, size, align, big, after, bend, named, lock.then_some(0x77), word);
        for (k, a) in [id, heap_name, base, size, align, big, after, bend as u32].iter().enumerate() {
            r.cpu.set_gpr(4 + k, *a as u64);
        }
        for (k, a) in [named as u32, lock as u32, word].iter().enumerate() {
            r.mem.write_u32(STACK + 8 + 8 * k as u32, *a);
        }
        r.call(INIT_HEAP).unwrap_or_else(|e| panic!("seq {seq}: init: {e}"));
        let check = |r: &mut Runner, m: &Mem, what: &str, ours: u32| {
            let th = r.cpu.gpr(2) as u32;
            assert_eq!(th, ours, "seq {seq}: {what}: the answer");
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                if &t != v {
                    let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
                    panic!("seq {seq}: {what}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
                }
            }
            assert_eq!(locks.get(), 0, "seq {seq}: {what}: the lock");
        };
        check(&mut r, &m, "init", n);
        if g.below(2) == 0 {
            m.w32(eamem::OUT_OF_MEMORY, OOM_FN);
            r.mem.write_u32(eamem::OUT_OF_MEMORY, OOM_FN);
        }
        let mut live: Vec<u32> = vec![];
        for step in 0..120 {
            ops += 1;
            let what = format!("step {step}");
            match g.below(10) {
                0..=4 => {
                    let name = if g.below(3) == 0 { 0 } else { g.pick(&names) };
                    let sz = match g.below(8) {
                        0 => g.below(8),
                        1 => g.below(0x40),
                        2 => if g.below(2) == 0 { u32::MAX } else { g.next() | 0x8000_0000 },
                        3 => 0x400 + g.below(0x2000),
                        _ => g.below(0x300),
                    };
                    let mut flags = id;
                    for bit in [0x10, 0x20, 0x40, 0x100, 0x200, 0x800] {
                        let fits = match bit {
                            0x200 => after >= 4,
                            0x800 => after >= 0x10,
                            // a name only fits where the heap counts it (a named heap)
                            0x100 => named,
                            _ => true,
                        };
                        if fits && g.below(4) == 0 {
                            flags |= bit;
                        }
                    }
                    let entry = g.pick(&[ALLOC_EX, ALLOC_EX, ALLOC_ALIGNED, ALLOC, ALLOC0]);
                    let (al, off) = if entry == ALLOC || entry == ALLOC0 {
                        (0, 0)
                    } else {
                        (g.pick(&[0, 0, 4, 8, 0x10, 0x40, 0x100]), g.pick(&[0, 0, 4, 8, 0xc, 0x40]))
                    };
                    retries.set(g.pick(&[0, 0, 1, 2]));
                    last.set(g.pick(&[0, 2]));
                    let (k, l) = (retries.get(), last.get());
                    theirs_oom.borrow_mut().clear();
                    let mut ours_oom = vec![];
                    let mut left = k;
                    let p = eamem::alloc_ex(&mut m, name, sz, al, off, flags, &mut |_, f, a, b, c| {
                        assert_eq!(f, OOM_FN);
                        ours_oom.push((a, b, c));
                        if left > 0 {
                            left -= 1;
                            1
                        } else {
                            l
                        }
                    });
                    let args = if entry == ALLOC || entry == ALLOC0 { vec![name, sz, flags] } else { vec![name, sz, al, off, flags] };
                    for (k, a) in args.iter().enumerate() {
                        r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
                    }
                    r.call(entry).unwrap_or_else(|e| panic!("seq {seq} {what}: alloc {entry:#x} {args:x?}: {e}"));
                    check(&mut r, &m, &format!("{what} alloc {entry:#x} {args:x?}"), p);
                    assert_eq!(*theirs_oom.borrow(), ours_oom, "seq {seq} {what}: the out-of-memory calls");
                    oom_calls += ours_oom.len() as u32;
                    if p != 0 {
                        live.push(p);
                    } else {
                        fails += 1;
                    }
                }
                5..=7 => {
                    let p = if live.is_empty() || g.below(20) == 0 { 0 } else { live.swap_remove(g.below(live.len() as u32) as usize) };
                    if p != 0 {
                        let (b, n) = (m.r32(p - 4), m.r32(p - 8));
                        if m.r16(b + 2) & 0x4000 != 0 || m.r16(n + 2) & 0x4000 != 0 {
                            joins += 1;
                        }
                    }
                    let ours = eamem::free(&mut m, p);
                    r.cpu.set_gpr(4, p as u64);
                    r.call(FREE).unwrap_or_else(|e| panic!("seq {seq} {what}: free {p:#x}: {e}"));
                    check(&mut r, &m, &format!("{what} free {p:#x}"), ours);
                }
                8 if !live.is_empty() => {
                    let p = g.pick(&live);
                    let sz = match g.below(5) {
                        0 => u32::MAX,
                        1 => g.below(8),
                        2 => g.below(0x2000),
                        _ => g.below(0x200),
                    };
                    let ours = eamem::resize(&mut m, p, sz);
                    r.cpu.set_gpr(4, p as u64);
                    r.cpu.set_gpr(5, sz as i32 as i64 as u64);
                    r.call(RESIZE).unwrap_or_else(|e| panic!("seq {seq} {what}: resize {p:#x} {sz:#x}: {e}"));
                    check(&mut r, &m, &format!("{what} resize {p:#x} {sz:#x}"), ours);
                }
                _ if !live.is_empty() => {
                    let p = g.pick(&live);
                    let fl = m.r16(p - 0xe) as u32;
                    let name = g.pick(&names);
                    let (f, args, ours) = match g.below(4) {
                        0 => (GET_SIZE, vec![p], eamem::get_size(&m, p)),
                        1 => (GET_NAME, vec![p], eamem::get_name(&m, p)),
                        2 => (BLOCK_HEADER_SIZE, vec![name, fl], eamem::block_header_size(&m, name, fl)),
                        _ => (HEAP_HEADER_SIZE, vec![fl], eamem::heap_header_size(&m, fl)),
                    };
                    for (k, a) in args.iter().enumerate() {
                        r.cpu.set_gpr(4 + k, *a as u64);
                    }
                    r.call(f).unwrap_or_else(|e| panic!("seq {seq} {what}: {f:#x}: {e}"));
                    check(&mut r, &m, &format!("{what} {f:#x} {args:x?}"), ours);
                }
                _ => {}
            }
        }
        // the heap's own record of its table entry
        assert_eq!(m.r32(eamem::HEAPS + 4 * id), base + 0x10);
        // the next sequence starts from the table as the game has it
        m.w32(eamem::HEAPS + 4 * id, u32::from_le_bytes(table[4 * id as usize..4 * id as usize + 4].try_into().unwrap()));
        r.mem.write(eamem::HEAPS, &table).unwrap();
        r.mem.write_u32(eamem::OUT_OF_MEMORY, 0);
    }
    println!("{ops} steps, {fails} failed allocations, {oom_calls} out-of-memory calls, {joins} frees joining a neighbour");
    assert!(fails > 500 && oom_calls > 300 && joins > 1000);
}
