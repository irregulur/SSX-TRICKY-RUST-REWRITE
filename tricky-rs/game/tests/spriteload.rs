//! `tricky_game::spriteload` (board row G8z) against the original in the function runner (TRICKY_ELF; skipped
//! without it): shape files made here (shapes named from the binders' tables, some missing, some found only by a
//! long name, some by a 4-character name hidden behind a long one), then random runs of the seven loaders (at once
//! and started), polls (pending, not done, done, nothing pending), the binders called directly, record copies and
//! the dummy textures. The file system, the heap and the renderer's virtuals are answered the same on both sides and
//! logged; memory, answers and the call logs compared after every call.

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::pf::Mem;
use tricky_game::spriteload::{self as sl, SpriteEnv};

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

const SET: u32 = 0x0100_0000;
const RUN: u32 = 0x0101_0000;
const FILE: u32 = 0x0102_0000;
const RECT: u32 = 0x0103_0000;
const APP: u32 = 0x0104_0000;
const RENDERER: u32 = 0x0105_0000;
const RVT: u32 = 0x0106_0000;
const BUF: u32 = 0x0107_0000;
const TEX_FN: u32 = 0x0170_0000;
const EMPTY_FN: u32 = 0x0170_0100;
const LOAD_FN: u32 = 0x0170_0200;

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

#[derive(Clone)]
struct World {
    seed: u32,
    n: u32,
    done: u32,
}

impl World {
    fn answer(&mut self, f: u32, a: &[u32]) -> u32 {
        self.n += 1;
        let h = a.iter().fold(self.seed ^ f ^ self.n.wrapping_mul(0x9e37_79b9), |h, &x| (h ^ x).wrapping_mul(0x0100_0193).rotate_left(5));
        match f {
            sl::FILE_LOAD => FILE,
            sl::ASYNC_LOAD => {
                if h % 6 == 0 {
                    0
                } else {
                    h | 1
                }
            }
            sl::ASYNC_IS_DONE => [1, 1, 0, 2][(h % 4) as usize] & self.done,
            sl::EAMEM_ALLOC => {
                if h % 5 == 0 {
                    0
                } else {
                    BUF
                }
            }
            _ => h,
        }
    }
}

struct Env {
    w: World,
    log: Vec<Call>,
}

impl SpriteEnv for Env {
    fn call(&mut self, _m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        let v = self.w.answer(f, args);
        self.log.push(Call(f, args.to_vec()));
        v
    }
    fn async_finish(&mut self, _m: &mut Mem, h: u32) -> u32 {
        self.w.answer(sl::ASYNC_FINISH, &[h, 0]);
        self.log.push(Call(sl::ASYNC_FINISH, vec![h, 0]));
        FILE
    }
}

fn cstr(r: &mut Runner, mut a: u32) -> Vec<u8> {
    let mut s = vec![];
    loop {
        let mut b = [0u8];
        r.mem.read(a, &mut b).unwrap();
        if b[0] == 0 {
            return s;
        }
        s.push(b[0]);
        a += 1;
    }
}

/// A shape file whose shapes are named from `pool`: (4-character name, long name or none)
fn shape_file(g: &mut Rng, pool: &[Vec<u8>]) -> Vec<u8> {
    let n = g.below(150);
    let mut f = vec![0u8; 0x10 + 8 * n as usize];
    f[..4].copy_from_slice(b"SHPS");
    f[8..12].copy_from_slice(&n.to_le_bytes());
    for i in 0..n as usize {
        let pick = |g: &mut Rng| pool[g.below(pool.len() as u32) as usize].clone();
        let short = if g.below(6) == 0 { b"zzzz".to_vec() } else { pick(g) };
        let mut s4 = [0u8; 4];
        s4[..short.len().min(4)].copy_from_slice(&short[..short.len().min(4)]);
        f[0x10 + 8 * i..0x14 + 8 * i].copy_from_slice(&s4);
        let at = f.len() as u32;
        f[0x14 + 8 * i..0x18 + 8 * i].copy_from_slice(&at.to_le_bytes());
        let long = (g.below(5) == 0).then(|| pick(g));
        let mut b = vec![0u8; 0x10];
        b[0] = 2;
        if let Some(l) = long {
            b[1..4].copy_from_slice(&[0x10, 0, 0]);
            b.extend([b'p', 0, 0, 0]);
            b.extend(&l);
            b.push(0);
            while b.len() % 16 != 0 {
                b.push(0);
            }
        }
        f.extend(b);
    }
    f
}

#[test]
fn the_sprite_loaders_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let funcs = [
        0x1f9970, 0x1f99f8, 0x1f9a80, 0x1f9b08, 0x1f9b90, 0x1f9c18, 0x1f9ca0, 0x1f9d28, 0x1f8c98, 0x1f9080, 0x1f93c8, 0x1f9550, 0x1f9608, 0x1f9660,
        0x1f9798, 0x1f9ee8, 0x1f9f38,
    ];
    for f in funcs.into_iter().chain([0x2c2528, 0x2c2660, 0x2c266c, 0x2c2610, 0x2c2694, 0x2fd008, 0x2cc808]) {
        r.allow(f);
    }
    let world = Rc::new(RefCell::new(World { seed: 0, n: 0, done: 3 }));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let regs = |f: u32| match f {
        sl::FILE_LOAD | sl::ASYNC_LOAD => 2,
        sl::EAMEM_ALLOC | LOAD_FN => 3,
        TEX_FN => 4,
        EMPTY_FN => 8,
        _ => 1,
    };
    for f in [sl::FILE_LOAD, sl::ASYNC_LOAD, sl::ASYNC_IS_DONE, sl::EAMEM_ALLOC, sl::EAMEM_FREE, sl::MEM_FREE, TEX_FN, EMPTY_FN, LOAD_FN] {
        let (world, log) = (world.clone(), log.clone());
        r.stub(f, move |c, m| {
            let mut a: Vec<u32> = (4..4 + regs(f)).map(|k| c.gpr(k) as u32).collect();
            if f == EMPTY_FN {
                let sp = c.gpr(29) as u32;
                let mut w = [0u8; 4];
                for o in [0, 8] {
                    m.read(sp + o, &mut w).unwrap();
                    a.push(u32::from_le_bytes(w));
                }
            }
            let v = world.borrow_mut().answer(f, &a);
            log.borrow_mut().push(Call(f, a));
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    {
        let (world, log) = (world.clone(), log.clone());
        r.stub(sl::ASYNC_FINISH, move |c, m| {
            let (h, out, z) = (c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32);
            world.borrow_mut().answer(sl::ASYNC_FINISH, &[h, z]);
            log.borrow_mut().push(Call(sl::ASYNC_FINISH, vec![h, z]));
            m.write(out, &FILE.to_le_bytes()).unwrap();
        });
    }
    let mut data = vec![0u8; 0x1000];
    r.mem.read(0x39a000, &mut data).unwrap();
    let mut tables = vec![0u8; 0x1c00];
    r.mem.read(0x33f000, &mut tables).unwrap();
    // the names the binders look for
    let mut pool = vec![];
    for k in 0..13 {
        pool.push(cstr(&mut r, sl::ICON_NAMES + 8 * k));
    }
    for (t, n) in [(sl::NAMED, 30), (sl::RIDER_PROFILE, 4), (sl::LOAD_SCREEN, 19)] {
        for i in 0..n {
            let a = r.mem.read_u32(t + 8 * i + 4);
            pool.push(cstr(&mut r, a));
        }
    }
    let mut g = Rng(0x5370_7200_0000_0001);
    let (mut binds, mut found) = (0u32, 0u32);
    for seq in 0..1500 {
        let rnd = |g: &mut Rng, n: u32| -> Vec<u8> { (0..n).map(|_| g.next() as u8).collect() };
        let file = shape_file(&mut g, &pool);
        let mut m = Mem {
            regions: vec![
                (SET, rnd(&mut g, 0x1620)),
                (RUN, rnd(&mut g, 0x200)),
                (FILE, file),
                (RECT, rnd(&mut g, 0x20)),
                (APP, rnd(&mut g, 0x800)),
                (RENDERER, rnd(&mut g, 0x20)),
                (RVT, rnd(&mut g, 0x200)),
                (BUF, rnd(&mut g, 0x4000)),
                (0x39a000, data.clone()),
                (0x33f000, tables.clone()),
                (sl::GAPP, APP.to_le_bytes().to_vec()),
            ],
        };
        m.w32(SET + 0x1610, RUN);
        m.w32(APP + 0x724, RENDERER);
        m.w32(RENDERER + 0x10, RVT);
        for (o, f) in [(0x150, TEX_FN), (0x158, EMPTY_FN), (0x160, LOAD_FN)] {
            m.w16(RVT + o, (g.below(32) as i16 - 16) as u16);
            m.w32(RVT + o + 4, f);
        }
        if g.below(3) == 0 {
            m.w32(SET + 4, 0);
        }
        for (a, v) in &m.regions {
            r.mem.write(*a, v).unwrap();
        }
        let w0 = World { seed: g.next(), n: 0, done: if g.below(4) == 0 { 0 } else { 3 } };
        *world.borrow_mut() = w0.clone();
        log.borrow_mut().clear();
        let mut env = Env { w: w0, log: vec![] };
        let run = |r: &mut Runner, m: &Mem, env: &Env, what: &str, f: u32, args: &[u32], ours: Option<u32>| {
            for (k, a) in args.iter().enumerate() {
                r.cpu.set_gpr(4 + k, *a as u64);
            }
            r.call(f).unwrap_or_else(|e| panic!("seq {seq}: {what}: {e}"));
            for (a, v) in &m.regions {
                let mut t = vec![0u8; v.len()];
                r.mem.read(*a, &mut t).unwrap();
                if &t != v {
                    let i = t.iter().zip(v).position(|(x, y)| x != y).unwrap();
                    panic!("seq {seq}: {what}: byte at {:#x}: theirs {:#x} ours {:#x}", a + i as u32, t[i], v[i]);
                }
            }
            assert_eq!(*log.borrow(), env.log, "seq {seq}: {what}: the calls out");
            if let Some(o) = ours {
                assert_eq!(r.cpu.gpr(2) as u32, o, "seq {seq}: {what}: the answer");
            }
        };
        let loaders = [0x1f9970, 0x1f99f8, 0x1f9a80, 0x1f9b08, 0x1f9b90, 0x1f9c18, 0x1f9ca0];
        let binders = [0x1f8c98, 0x1f9080, 0x1f93c8, 0x1f9550, 0x1f9608, 0x1f9660, 0x1f9798];
        for step in 0..12 {
            let what = format!("step {step}");
            match g.below(6) {
                0 | 1 => {
                    let kind = g.below(7);
                    let (n, wait) = (g.below(12), g.below(2));
                    sl::load(&mut m, &mut env, kind, SET, n, wait);
                    binds += (wait == 0) as u32;
                    run(&mut r, &m, &env, &format!("{what}: load {kind} {n} {wait}"), loaders[kind as usize], &[SET, n, wait], None);
                }
                2 => {
                    if g.below(2) == 0 {
                        m.w32(SET, g.below(9));
                        r.mem.write_u32(SET, m.r32(SET));
                    }
                    let ours = sl::poll(&mut m, &mut env, SET);
                    run(&mut r, &m, &env, &format!("{what}: poll"), 0x1f9d28, &[SET], Some(ours));
                }
                3 => {
                    let k = g.below(7);
                    let before = env.log.len();
                    match k {
                        0 => sl::icon_shapes(&mut m, &mut env, SET, FILE),
                        1 => sl::hud_game(&mut m, &mut env, SET, FILE),
                        2 => sl::loading_icons(&mut m, &mut env, SET, FILE),
                        3 => sl::named_shapes(&mut m, &mut env, SET, FILE),
                        4 => sl::first_shape(&mut m, &mut env, SET, FILE),
                        5 => sl::rider_profile_shapes(&mut m, &mut env, SET, FILE),
                        _ => sl::load_screen_shapes(&mut m, &mut env, SET, FILE),
                    }
                    found += (env.log.len() - before) as u32;
                    binds += 1;
                    run(&mut r, &m, &env, &format!("{what}: binder {k}"), binders[k as usize], &[SET, FILE], None);
                }
                4 => {
                    let k = g.below(200);
                    sl::set_shape(&mut m, SET, k, RECT);
                    run(&mut r, &m, &env, &format!("{what}: set shape {k}"), 0x1f9ee8, &[SET, k, RECT], None);
                }
                _ => {
                    sl::dummy_textures(&mut m, &mut env, SET);
                    run(&mut r, &m, &env, &format!("{what}: dummy"), 0x1f9f38, &[SET], None);
                }
            }
        }
    }
    println!("{binds} binds, {found} textures made or loaded by the binders");
    assert!(binds > 5000 && found > 12_000);
}
