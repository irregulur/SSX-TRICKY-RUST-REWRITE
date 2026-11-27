//! `tricky_game::fileload::host_write_file` (board row E2y) against `Host_WriteFile` 0x2c8708 and
//! `Host_WriteFileChecked` 0x2c87e8 in the function runner (TRICKY_ELF; skipped without it): random paths (with and
//! without "host0:"), sizes around the 0x1000-byte pieces, and the kernel's open, write and close answering
//! success or failure the same on both sides (logged). Memory (the stack aside), answers and calls are compared.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::fileload::{self as fl, LoadEnv};
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
}

const STACK_TOP: u32 = 0x01ff_0000;
const STACK: u32 = STACK_TOP - 0x1000;
const GLOBALS: (u32, u32) = (0x3bb900, 0x200);
const PATHS: u32 = 0x0104_0000;
const PATH_LIST: &[&[u8]] = &[b"host0:out.bin", b"save\\a.dat", b"", b"host0:", b"hos", b"x"];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn answer(seed: u32, n: u32, f: u32, a: &[u32]) -> u32 {
    let h = a.iter().fold(seed ^ f ^ n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
    match f {
        fl::SCE_OPEN | fl::SCE_CLOSE => {
            if h % 5 == 0 {
                -(1 + (h >> 4) as i32 % 20) as u32
            } else {
                (h >> 4) % 0x40
            }
        }
        _ => {
            if h % 7 == 0 {
                -5i32 as u32
            } else {
                a[2]
            }
        }
    }
}

struct Env {
    seed: u32,
    log: Vec<Call>,
}

impl LoadEnv for Env {
    fn call(&mut self, _m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        self.log.push(Call(f, args.to_vec()));
        answer(self.seed, self.log.len() as u32, f, args)
    }
}

#[test]
fn the_host_writes_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x2c8708, 0x2c87e8, 0x2fd260, 0x2fd478, 0x2fd14c, 0x2fcd4c] {
        r.allow(f);
    }
    let seed = Rc::new(RefCell::new(0u32));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for (f, n) in [(fl::SCE_OPEN, 2), (fl::SCE_CLOSE, 1), (fl::SCE_WRITE, 3)] {
        let (seed, log) = (seed.clone(), log.clone());
        r.stub(f, move |c, _| {
            let a: Vec<u32> = (4..4 + n).map(|k| c.gpr(k) as u32).collect();
            log.borrow_mut().push(Call(f, a.clone()));
            let v = answer(*seed.borrow(), log.borrow().len() as u32, f, &a);
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut globals = vec![0u8; GLOBALS.1 as usize];
    r.mem.read(GLOBALS.0, &mut globals).unwrap();
    let mut g = Rng(0x4057_0000_0000_0001);
    let mut ones = 0;
    for run in 0..3000 {
        let mut paths = vec![0u8; 0x100 * PATH_LIST.len()];
        for (k, p) in PATH_LIST.iter().enumerate() {
            paths[0x100 * k..0x100 * k + p.len()].copy_from_slice(p);
        }
        let st: Vec<u8> = (0..0x1000).map(|_| g.next() as u8).collect();
        let mut m = Mem { regions: vec![(GLOBALS.0, globals.clone()), (PATHS, paths), (STACK, st)] };
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let s = g.next();
        *seed.borrow_mut() = s;
        log.borrow_mut().clear();
        let mut env = Env { seed: s, log: vec![] };
        let path = PATHS + 0x100 * g.below(PATH_LIST.len() as u32);
        let buf = 0x0110_0000 + g.below(0x1000);
        let size = match g.below(4) {
            0 => g.below(3) * 0x1000 + [0, 1, 0xfff][g.below(3) as usize],
            1 => g.next() | 0x8000_0000,
            _ => g.below(0x5000),
        };
        let checked = g.below(2) == 0;
        let f = if checked { 0x2c87e8 } else { 0x2c8708 };
        let ours = fl::host_write_file(&mut m, &mut env, STACK_TOP, path, buf, size, checked);
        for (k, a) in [path, buf, size].iter().enumerate() {
            r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
        }
        r.call(f).unwrap_or_else(|e| panic!("run {run}: {f:#x}: {e}"));
        for (a, v) in m.regions.iter().filter(|(a, _)| *a != STACK) {
            let mut t = vec![0u8; v.len()];
            r.mem.read(*a, &mut t).unwrap();
            assert_eq!(&t, v, "run {run}: memory at {a:#x}");
        }
        // the path as made, on the frame
        let at = STACK_TOP - 0x140;
        let mut t = vec![0u8; 0x40];
        r.mem.read(at, &mut t).unwrap();
        let ourpath: Vec<u8> = (0..0x40).map(|k| m.r8(at + k)).collect();
        let end = t.iter().position(|&c| c == 0).unwrap_or(0x40);
        assert_eq!(t[..end], ourpath[..end], "run {run}: the path made");
        assert_eq!(*log.borrow(), env.log, "run {run}: {f:#x} the calls out");
        assert_eq!(r.cpu.gpr(2) as u32, ours, "run {run}: {f:#x} the answer");
        ones += ours;
    }
    assert!(ones > 100);
}
