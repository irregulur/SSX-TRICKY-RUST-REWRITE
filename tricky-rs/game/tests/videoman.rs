//! `tricky_game::videoman` (board row G11a3) against the original in the function runner (TRICKY_ELF; skipped
//! without it): a film manager made, given streams of random sizes, then random runs of opens (the same film again,
//! other films, other sizes, loop and fps of every kind), ticks, uploads, releases of both kinds, texture and
//! finish queries, a stream's own calls, and the destructor; every call out (EA's stream library, the heap, the
//! renderer's virtuals, the base node) answered the same on both sides and logged. Memory, answers and the call
//! logs compared after every call.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::pf::Mem;
use tricky_game::videoman::{self as vm, VideoEnv};

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
const ARENA_SIZE: u32 = 0x10_0000;
const MAN: u32 = 0x0120_0000;
const APP: u32 = 0x0121_0000;
const RENDERER: u32 = 0x0122_0000;
const RVT: u32 = 0x0123_0000;
const NAMES: u32 = 0x0124_0000;
const PKT: u32 = 0x0125_0000;
const UNPACKED: u32 = 0x0126_0000;
const SCRATCH_SIZE: u32 = 0x2000;
const TEX_FN: u32 = 0x0170_0000;
const UPLOAD_FN: u32 = 0x0170_0100;

const OUT: [u32; 15] = [
    vm::OBJNODE_CTOR,
    vm::OBJNODE_DTOR,
    vm::MEM_DELETE,
    vm::SHAPE_PALETTE_FORMAT,
    vm::SHAPE_UNPACK,
    vm::STRM_MEM_SIZE,
    vm::STRM_CREATE,
    vm::STRM_CLOSE,
    vm::STRM_QUEUE_FILE,
    vm::STRM_PURGE,
    vm::STRM_GET_PACKET,
    vm::STRM_RELEASE_PACKET,
    vm::STRM_IS_FINISHED,
    vm::EAMEM_ALLOC,
    vm::EAMEM_FREE,
];

fn arity(f: u32) -> usize {
    match f {
        vm::STRM_CREATE => 5,
        vm::STRM_QUEUE_FILE => 4,
        vm::EAMEM_ALLOC | TEX_FN | UPLOAD_FN => 3,
        vm::STRM_MEM_SIZE => 3,
        vm::OBJNODE_DTOR | vm::STRM_PURGE | vm::STRM_RELEASE_PACKET => 2,
        _ => 1,
    }
}

/// The answers to the calls out: a bump allocator for the heap, a seeded hash for the rest
#[derive(Clone)]
struct World {
    top: u32,
    seed: u32,
    n: u32,
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32]) -> u32 {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        match f {
            vm::EAMEM_ALLOC => {
                let p = self.top;
                self.top = (self.top + a[1].max(1) + 15) & !15;
                assert!(self.top < ARENA + ARENA_SIZE, "the test's heap is full");
                p
            }
            vm::STRM_MEM_SIZE => h & 0xffff,
            vm::STRM_GET_PACKET => {
                if h % 3 == 0 {
                    0
                } else {
                    PKT + 0x10 * (h >> 8 & 0xf)
                }
            }
            vm::STRM_IS_FINISHED => (h % 5 == 0) as u32,
            vm::SHAPE_UNPACK => UNPACKED + 0x10 * (h >> 8 & 0xf),
            vm::SHAPE_PALETTE_FORMAT => h & 0xff,
            _ => h,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

struct Env {
    w: World,
    log: Vec<Call>,
}

impl VideoEnv for Env {
    fn call(&mut self, _m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        let mut a = args.to_vec();
        a.resize(arity(f), 0);
        let v = self.w.answer(f, &a);
        self.log.push(Call(f, a));
        v
    }
}

fn load(r: &mut Runner, m: &Mem) {
    for (a, v) in &m.regions {
        r.mem.write(*a, v).unwrap();
    }
}

fn check(r: &mut Runner, m: &Mem, what: &str) {
    for (a, v) in &m.regions {
        let mut t = vec![0u8; v.len()];
        r.mem.read(*a, &mut t).unwrap();
        if &t != v {
            let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
            panic!("{what}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
        }
    }
}

#[test]
fn the_film_manager_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let funcs = [
        0x1ffd80, 0x1ffde0, 0x1ffe70, 0x1fff30, 0x200098, 0x2000f0, 0x200148, 0x2001a0, 0x2001c8, 0x2001f8, 0x200270, 0x2002e8, 0x1ff6f0, 0x1ff820,
        0x1ff890, 0x1ff928, 0x1ff988, 0x1ff990, 0x1ffa10, 0x1ffa38, 0x1ffa88, 0x1ffb80, 0x1ffbd8,
    ];
    for f in funcs.into_iter().chain([0x2cc808, 0x2ce114, 0x2fd14c, 0x2fd008, 0x2c26d8]) {
        r.allow(f);
    }
    let world = Rc::new(RefCell::new(World { top: 0, seed: 0, n: 0 }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    for f in OUT.into_iter().chain([TEX_FN, UPLOAD_FN]) {
        let (world, log) = (world.clone(), log.clone());
        r.stub(f, move |c, _| {
            let a: Vec<u32> = (4..4 + arity(f)).map(|k| c.gpr(k) as u32).collect();
            let v = world.borrow_mut().answer(f, &a);
            log.borrow_mut().push(Call(f, a));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let mut g = Rng(0x5669_6400_0000_0001);
    let names: [&[u8]; 4] = [b"data/fe/tb_mac1.sss\0", b"data/fe/tb_mac2.sss\0", b"x\0", b"data/fe/course.sss\0"];
    let (mut ops, mut frames, mut opens) = (0u32, 0u32, 0u32);
    for seq in 0..400 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let mut m = Mem {
            regions: vec![
                (ARENA, rnd(&mut g, ARENA_SIZE)),
                (MAN, rnd(&mut g, 0x40)),
                (APP, rnd(&mut g, 0x800)),
                (RENDERER, rnd(&mut g, 0x20)),
                (RVT, rnd(&mut g, 0x200)),
                (NAMES, vec![0; 0x100]),
                (PKT, rnd(&mut g, SCRATCH_SIZE)),
                (UNPACKED, rnd(&mut g, 0x8000)),
                (vm::GAPP, APP.to_le_bytes().to_vec()),
            ],
        };
        m.w32(APP + 0x724, RENDERER);
        m.w32(RENDERER + 0x10, RVT);
        m.w16(RVT + 0x150, (g.below(32) as i16 - 16) as u16);
        m.w32(RVT + 0x154, TEX_FN);
        m.w16(RVT + 0x160, (g.below(32) as i16 - 16) as u16);
        m.w32(RVT + 0x164, UPLOAD_FN);
        for (k, n) in names.iter().enumerate() {
            for (i, &c) in n.iter().enumerate() {
                m.w8(NAMES + 0x40 * k as u32 + i as u32, c);
            }
        }
        load(&mut r, &m);
        let w0 = World { top: ARENA + 0x100, seed: g.next(), n: 0 };
        *world.borrow_mut() = w0.clone();
        let mut env = Env { w: w0, log: vec![] };
        log.borrow_mut().clear();

        let run = |r: &mut Runner, m: &Mem, env: &Env, what: &str, f: u32, args: &[u32], ours: Option<u32>| {
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as i32 as i64 as u64);
            }
            r.call(f).unwrap_or_else(|e| panic!("seq {seq}: {what}: {e}"));
            check(r, m, &format!("seq {seq}: {what}"));
            assert_eq!(*log.borrow(), env.log, "seq {seq}: {what}: the calls out");
            if let Some(o) = ours {
                assert_eq!(r.cpu.gpr(2) as u32, o, "seq {seq}: {what}: the answer");
            }
        };
        let v = vm::new(&mut m, &mut env, MAN);
        run(&mut r, &m, &env, "new", 0x1ffd80, &[MAN], Some(v));
        let sizes = [(32, 32), (16, 24), (g.below(48), g.below(48))];
        for _ in 0..1 + g.below(4) {
            let (w, h) = sizes[g.below(3) as usize];
            let ours = vm::add_stream(&mut m, &mut env, MAN, w, h);
            run(&mut r, &m, &env, &format!("add {w}x{h}"), 0x1ffe70, &[MAN, w, h], Some(ours));
        }
        let count = m.r32(MAN + 0xc);
        for step in 0..120 {
            ops += 1;
            let i = g.below(count.max(1));
            let what = format!("step {step}");
            match g.below(12) {
                0..=2 => {
                    let name = NAMES + 0x40 * g.below(4);
                    let (w, h) = sizes[g.below(3) as usize];
                    let lp = g.below(2);
                    let fps = [10, 15, 30, 0, 40, g.next()][g.below(6) as usize];
                    let ours = vm::player_open(&mut m, &mut env, MAN, name, w, h, lp, fps);
                    opens += (ours >= 0) as u32;
                    run(&mut r, &m, &env, &format!("{what} open"), 0x1fff30, &[MAN, name, w, h, lp, fps], Some(ours as u32));
                }
                3..=5 => {
                    let before = env.log.iter().filter(|c| c.0 == vm::SHAPE_UNPACK).count();
                    for _ in 0..1 + g.below(8) {
                        vm::update(&mut m, &mut env, MAN);
                        run(&mut r, &m, &env, &format!("{what} update"), 0x2001f8, &[MAN], None);
                    }
                    frames += (env.log.iter().filter(|c| c.0 == vm::SHAPE_UNPACK).count() - before) as u32;
                }
                6 => {
                    vm::upload(&mut m, &mut env, MAN);
                    run(&mut r, &m, &env, &format!("{what} upload"), 0x200270, &[MAN], None);
                }
                7 if count > 0 => {
                    let ours = vm::player_release(&mut m, &mut env, MAN, i);
                    run(&mut r, &m, &env, &format!("{what} release {i}"), 0x200098, &[MAN, i], Some(ours));
                }
                8 if count > 0 => {
                    if g.below(4) == 0 {
                        vm::release_all(&mut m, &mut env, MAN);
                        run(&mut r, &m, &env, &format!("{what} release all"), 0x200148, &[MAN], None);
                    } else {
                        let ours = vm::man_release(&mut m, &mut env, MAN, i);
                        run(&mut r, &m, &env, &format!("{what} man release {i}"), 0x2000f0, &[MAN, i], Some(ours));
                    }
                }
                9 if count > 0 => {
                    let (f, ours) = match g.below(3) {
                        0 => (0x2001a0, vm::get_texture(&m, MAN, i)),
                        1 => (0x2001c8, vm::frame_texture(&m, MAN, i)),
                        _ => (0x2002e8, vm::is_finished(&m, MAN, i)),
                    };
                    run(&mut r, &m, &env, &format!("{what} query {f:#x} {i}"), f, &[MAN, i], Some(ours));
                }
                10 if count > 0 => {
                    // a stream's own calls
                    let s = m.r32(MAN + 0x10 + 4 * i);
                    match g.below(4) {
                        0 => {
                            let ours = vm::stream_stop(&mut m, &mut env, s);
                            run(&mut r, &m, &env, &format!("{what} stop {i}"), 0x1ffa10, &[s], Some(ours));
                        }
                        1 => {
                            vm::stream_reopen(&mut m, &mut env, s);
                            run(&mut r, &m, &env, &format!("{what} reopen {i}"), 0x1ffa38, &[s], None);
                        }
                        2 => {
                            let (name, lp, fps) = (NAMES + 0x40 * g.below(4), g.below(2), g.below(40) as i32 - 5);
                            let ours = vm::stream_open(&mut m, &mut env, s, name, lp, fps);
                            run(&mut r, &m, &env, &format!("{what} stream open {i}"), 0x1ff990, &[s, name, lp, fps as u32], Some(ours));
                        }
                        _ => {
                            let kind = 1 + g.below(2);
                            // kind 2 with an odd w · h stores halves at odd addresses (an address error on the PS2)
                            let (w, h) = (g.below(40), g.below(20) * 2);
                            let ours = vm::alloc_shape(&mut m, &mut env, w, h, kind);
                            run(&mut r, &m, &env, &format!("{what} shape {kind} {w}x{h}"), 0x1ffbd8, &[s, w, h, kind], Some(ours));
                        }
                    }
                }
                _ => {}
            }
        }
        if g.below(2) == 0 {
            let flags = g.below(4);
            vm::delete(&mut m, &mut env, MAN, flags);
            run(&mut r, &m, &env, "delete", 0x1ffde0, &[MAN, flags], None);
        }
    }
    println!("{ops} steps, {opens} films opened, {frames} frames decoded");
    assert!(opens > 3_000 && frames > 5_000);
}
