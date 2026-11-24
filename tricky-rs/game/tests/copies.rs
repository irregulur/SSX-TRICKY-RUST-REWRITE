//! `tricky_game::copies` (board row X2c) against the original in the function runner (TRICKY_ELF; skipped
//! without it). Each case fills an arena with random words (floats of every size for the vector helpers), runs one
//! of the row's functions on both sides with the same arguments, the frees and deletes stand-ins that log their
//! argument (`Mem_Free` sometimes shrinking the array's count or making it negative, to show it is read again, signed), `memcpy` the game's own.
//! The calls, the answers and the memory are compared.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use r5900::Runner;
use tricky_game::copies as cp;
use tricky_game::fe3dstate::FeEnv;
use tricky_game::pf::Mem;
use tricky_game::ps2f::F;

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
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
    /// A float: small, around one, huge, tiny or any bits
    fn float(&mut self) -> u32 {
        let x = (self.next() >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        let any = self.next();
        self.pick(&[
            (x * 1.5).to_bits(),
            (x * 1000.0).to_bits(),
            (x * 3.0e38).to_bits(),
            (x * 1.0e-38).to_bits(),
            any,
            0,
            0x8000_0000,
        ])
    }
}

const DATA: (u32, u32) = (0x3a4000, 0x1000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x1000;

const FUNCS: [u32; 12] = [
    0x110860, 0x110898, 0x1108d0, 0x198720, 0x198738, 0x198750, 0x2401a0, 0x1e7da0, 0x2085e8, 0x260fa0, 0x21edf0,
    0x105b90, // (a stand-in for "one of the scale copies": the case picks which)
];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, u32);

struct Answers {
    seed: Cell<u64>,
    n: Cell<u64>,
}
impl Answers {
    fn next(&self) -> u32 {
        self.n.set(self.n.get() + 1);
        Rng(self.seed.get() ^ self.n.get().wrapping_mul(0x1234_5678_9abc_def1)).next()
    }
}

struct Env<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
    arr: u32,
}
impl FeEnv for Env<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.log.push(Call(func, args[0]));
        let r = self.ans.next();
        if func == cp::MEM_FREE && r % 4 == 0 {
            let n = m.r32(self.arr);
            m.w32(self.arr, n.wrapping_sub(1));
        }
        if func == cp::MEM_FREE && r % 8 == 1 {
            m.w32(self.arr, u32::MAX);
        }
        r
    }
}

#[test]
fn copied_helpers_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS.iter().chain(cp::SCALE_COPIES.iter()) {
        r.allow(*f);
    }
    r.allow(0x2ce114); // memcpy, the game's
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    let arr_at = Rc::new(Cell::new(0u32));
    for func in [cp::MEM_FREE, cp::MEM_DELETE] {
        let (l, a, arr) = (log.clone(), ans.clone(), arr_at.clone());
        r.stub(func, move |c, mem| {
            l.borrow_mut().push(Call(func, c.gpr(4) as u32));
            let v = a.next();
            if func == cp::MEM_FREE && v % 4 == 0 {
                let n = mem.read_u32(arr.get());
                mem.write(arr.get(), &n.wrapping_sub(1).to_le_bytes()).unwrap();
            }
            if func == cp::MEM_FREE && v % 8 == 1 {
                mem.write(arr.get(), &u32::MAX.to_le_bytes()).unwrap();
            }
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut data = vec![0u8; DATA.1 as usize];
    r.mem.read(DATA.0, &mut data).unwrap();
    let mut g = Rng(0xc0_9135_0000_0001);
    let mut freed = 0usize;
    for case in 0..12_000u32 {
        let mut func = FUNCS[(case % FUNCS.len() as u32) as usize];
        if func == 0x105b90 {
            func = g.pick(&cp::SCALE_COPIES);
        }
        let mut m = Mem { regions: vec![(DATA.0, data.clone()), (ARENA, vec![0; SIZE as usize])] };
        for k in 0..SIZE / 4 {
            let v = if g.below(2) == 0 { g.float() } else { g.next() };
            m.w32(ARENA + 4 * k, v);
        }
        let q = |g: &mut Rng| ARENA + 0x10 * g.below(SIZE / 0x10 - 4);
        let w = |g: &mut Rng| ARENA + 4 * g.below(SIZE / 4 - 8);
        let (mut a0, mut a1, mut a2, mut a3) = (q(&mut g), q(&mut g), q(&mut g), q(&mut g));
        let f12 = f32::from_bits(g.float());
        match func {
            0x198720 | 0x198738 | 0x198750 => {
                a0 = w(&mut g);
                a1 = if g.below(4) == 0 { a0 } else { w(&mut g) };
            }
            0x1e7da0 => {
                a1 = ARENA + 2 * g.below(0x200);
                a2 = g.below(0x100);
                a3 = w(&mut g);
            }
            0x260fa0 => a1 = g.pick(&[0, 1, 2, 3, 0xffff_ffff]),
            0x21edf0 => {
                a0 = w(&mut g);
                let n = g.pick(&[0u32, 1, 3, 8, 0xffff_ffff, 0x8000_0000]);
                m.w32(a0, n);
                if (n as i32) > 0 {
                    for i in 0..n {
                        let p = g.next();
                        m.w32(a0 + 4 + 4 * i, g.pick(&[0, p, p]));
                    }
                }
            }
            0x2401a0 => a0 = ARENA + g.below(SIZE - 0x20),
            _ => {}
        }
        arr_at.set(a0);
        let seed = g.next() as u64 | (g.next() as u64) << 32;
        ans.seed.set(seed);
        ans.n.set(0);
        let before = m.regions.clone();
        let mut env = Env { log: vec![], ans: &ans, arr: a0 };
        let none = 0x5eed_5eed;
        let (mm, e) = (&mut m, &mut env);
        let ours = match func {
            0x110860 | 0x110898 => cp::construct(a0),
            0x1108d0 => cp::vec4_add_to(mm, a0, a1),
            0x198720 => { cp::assign(mm, a0, a1, 12); none }
            0x198738 | 0x198750 => { cp::assign(mm, a0, a1, 4); none }
            0x2401a0 => { cp::rand_init(mm, a0); none }
            0x1e7da0 => { cp::vec3_pack_short(mm, a1, a2, a3); none }
            0x2085e8 => cp::mat4_transform(mm, a0, a1, a2),
            0x260fa0 => { cp::delete_if(mm, e, a0, a1); none }
            0x21edf0 => { cp::ptr_array_free_all(mm, e, a0); none }
            _ => cp::vec4_scale(mm, a0, a1, F(f12)),
        };
        freed += env.log.len();
        // theirs
        ans.n.set(0);
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        for (k, v) in [a0, a1, a2, a3].iter().enumerate() {
            r.cpu.set_gpr(4 + k, *v as i32 as i64 as u64);
        }
        r.cpu.set_f(12, f12);
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if ours != none {
            assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x}: the answer");
        }
        assert_eq!(*log.borrow(), env.log, "case {case} {func:#x}: the calls");
        for (at, b) in &m.regions {
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    println!("{freed} frees/deletes");
    assert!(freed > 1000);
}
