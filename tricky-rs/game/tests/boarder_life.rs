//! `tricky_game::boarder_life` (board row G1x) against the original in the function runner (TRICKY_ELF; skipped
//! without it): the 28 state and motion destructors, the boarder's and its parts' destructors and the boarder's
//! small virtuals, on random objects and `how` words, the calls out logged and the memory compared (with the
//! table's copy a destructor leaves on the stack).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::boarder_life as bl;
use tricky_game::fe3dstate::FeEnv;
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
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
}

const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x8000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const VTS: (u32, u32) = (0x366000, 0x2000);

const EMPTY: [u32; 15] = [
    0x11a7a0, 0x122d60, 0x122d68, 0x122d70, 0x122d78, 0x122d80, 0x122d88, 0x122d90, 0x122e50, 0x122fd8, 0x122fe0,
    0x122fe8, 0x122ff0, 0x122fc8, 0x122fd0,
];
const OTHERS: [u32; 6] = [0x116990, 0x122ff8, 0x1230a0, 0x1230c0, 0x236190, 0x122e20];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);
fn arity(f: u32) -> usize {
    if f == bl::MEM_DELETE { 1 } else { 2 }
}
struct Env {
    log: Vec<Call>,
}
impl FeEnv for Env {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(arity(func), 0);
        self.log.push(Call(func, w));
        0
    }
}

#[test]
fn boarder_destructors_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let mut funcs: Vec<u32> = bl::STATE_DESTRUCTORS.iter().map(|d| d.0).collect();
    funcs.extend(EMPTY);
    funcs.extend(OTHERS);
    for f in &funcs {
        r.allow(*f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for func in [bl::MEM_DELETE, bl::BOARDER_RESET_SKIN, bl::SHAPE_LIST_DESTRUCT, bl::SKIN_DELETE] {
        let l = log.clone();
        r.stub(func, move |c, _| {
            let w: Vec<u32> = (4..4 + arity(func)).map(|k| c.gpr(k) as u32).collect();
            l.borrow_mut().push(Call(func, w));
            c.set_gpr(2, 0);
        });
    }
    let mut vts = vec![0u8; VTS.1 as usize];
    r.mem.read(VTS.0, &mut vts).unwrap();
    let mut g = Rng(0xb0a2_d000_0000_0001);
    let mut copies = 0;
    for case in 0..funcs.len() as u32 * 300 {
        let func = funcs[(case % funcs.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (VTS.0, vts.clone()),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        for k in 0..STACK.1 / 4 {
            m.w32(STACK.0 + 4 * k, g.next());
        }
        // the object and its boarder (before or after it)
        let b = ARENA + 0x100 + 4 * g.below(0x40);
        let far = ARENA + 0x6000 + 4 * g.below(0x100);
        let o = g.pick(&[far, b + 0x10, b]);
        let at = bl::STATE_DESTRUCTORS.iter().find(|d| d.0 == func).map_or(0, |d| d.2);
        m.w32(o + at, b);
        let any = g.next();
        let how = g.pick(&[0, 0, 1, 2, 3, 3, any]);
        let before = m.regions.clone();
        let mut env = Env { log: vec![] };
        let e = &mut env;
        let mut answer = None;
        if let Some(d) = bl::STATE_DESTRUCTORS.iter().find(|d| d.0 == func) {
            bl::state_destruct(&mut m, e, o, how, *d, TOP);
            copies += (how == 0) as u32;
        } else {
            match func {
                0x116990 => bl::boarder_destruct(&mut m, e, b, how),
                0x122ff8 => bl::cache_destruct(&mut m, e, b, how),
                0x1230a0 | 0x1230c0 | 0x236190 => bl::part_destruct(&mut m, e, b, how),
                0x122fc8 | 0x122fd0 => answer = Some(bl::zero(&m, b)),
                0x122e20 => answer = Some(bl::one(&m, b).to_bits()),
                _ => bl::nothing(&mut m, b),
            }
        }
        log.borrow_mut().clear();
        for (at, bytes) in &before {
            r.mem.write(*at, bytes).unwrap();
        }
        let a0 = if bl::STATE_DESTRUCTORS.iter().any(|d| d.0 == func) { o } else { b };
        r.cpu.set_gpr(4, a0 as u64);
        r.cpu.set_gpr(5, how as i32 as i64 as u64);
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if let Some(a) = answer {
            let th = if func == 0x122e20 { r.cpu.f(0).to_bits() } else { r.cpu.gpr(2) as u32 };
            assert_eq!(th, a, "case {case} {func:#x}: the answer");
        }
        assert_eq!(*log.borrow(), env.log, "case {case} {func:#x}: the calls");
        for (at, bytes) in &m.regions {
            // of the stack, only the table's copy below the destructor's frame
            let (at, bytes) = if *at == STACK.0 {
                let s = (TOP - 0xe0 - STACK.0) as usize;
                (TOP - 0xe0, &bytes[s..s + 0xb0])
            } else {
                (*at, &bytes[..])
            };
            let mut th = vec![0u8; bytes.len()];
            r.mem.read(at, &mut th).unwrap();
            if th != bytes {
                let i = th.iter().zip(bytes).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x} how {how:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(bytes));
            }
        }
    }
    println!("{copies} copies");
    assert!(copies > 1000);
}
