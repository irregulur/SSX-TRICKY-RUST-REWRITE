//! `tricky_game::lesson_draw` (board row C11d1) against the original in the function runner (TRICKY_ELF; skipped
//! without it). Every case starts from the game's own data (0x31d000..0x340000 and 0x36e000..0x370000, as the ELF
//! loads them) with the app, the renderer, the race, the pad config, the lesson manager and its recording made
//! random in an arena, and runs one of the row's functions on both sides from the same stack pointer. Every callee
//! is a stand-in on both sides answering from the case's seed; the stand-ins log their arguments, the strings and
//! colours they are given, and the font colour where they draw. Strings are written, copied, measured and compared
//! for real by the stand-ins. The calls, the answers and the memory (not the stack) are compared.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use r5900::Runner;
use tricky_game::lesson_draw as ld;
use tricky_game::pf::Mem;
use tricky_game::ps2f::F;
use tricky_game::textbox::TextBoxEnv;

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
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
}

const DATA: (u32, u32) = (0x31d000, 0x23000);
const RODATA: (u32, u32) = (0x36e000, 0x2000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x4000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const LM: u32 = ARENA;
const APPOBJ: u32 = ARENA + 0x800;
const RENDER: u32 = ARENA + 0x900;
const RVT: u32 = ARENA + 0xa00;
const RACE: u32 = ARENA + 0xc00;
const R1: u32 = ARENA + 0xc80;
const OWNER: u32 = ARENA + 0xd00;
const LO: u32 = ARENA + 0x1000;
const PM: u32 = ARENA + 0x1400;
const PMX: u32 = ARENA + 0x1480;
const FONTS: u32 = ARENA + 0x1500;
const CFG: u32 = ARENA + 0x1800;
const REC: u32 = ARENA + 0x2000;
const STRS: u32 = ARENA + 0x3a00;
fn rv(off: u32) -> u32 {
    0x01f0_0000 + off
}
const ROFFS: [u32; 10] = [0x88, 0x90, 0xb0, 0x108, 0x1c0, 0x1c8, 0x1d0, 0x1d8, 0x1e0, 0x1f0];
/// The trick book page's frame as `DrawTrickBookPage` itself or `DrawPage` called it: its counts' buffer
const COUNTS_BUF: [u32; 2] = [TOP - 0x380 + 0xe0, TOP - 0x50 - 0x380 + 0xe0];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

trait Io {
    fn w32(&mut self, a: u32, v: u32);
    fn r32(&mut self, a: u32) -> u32;
    fn r8(&mut self, a: u32) -> u8;
    fn w8(&mut self, a: u32, v: u8);
}
impl Io for Mem {
    fn w32(&mut self, a: u32, v: u32) {
        Mem::w32(self, a, v)
    }
    fn r32(&mut self, a: u32) -> u32 {
        Mem::r32(self, a)
    }
    fn r8(&mut self, a: u32) -> u8 {
        Mem::r8(self, a)
    }
    fn w8(&mut self, a: u32, v: u8) {
        Mem::w8(self, a, v)
    }
}
impl Io for r5900::Mem {
    fn w32(&mut self, a: u32, v: u32) {
        self.write(a, &v.to_le_bytes()).unwrap()
    }
    fn r32(&mut self, a: u32) -> u32 {
        self.read_u32(a)
    }
    fn r8(&mut self, a: u32) -> u8 {
        let mut b = [0u8];
        self.read(a, &mut b).unwrap();
        b[0]
    }
    fn w8(&mut self, a: u32, v: u8) {
        self.write(a, &[v]).unwrap()
    }
}

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

/// How many int and float arguments a callee has (`this` counted for a virtual)
fn arity(func: u32, w: &[u32]) -> (usize, usize) {
    use ld::*;
    match func {
        f if f == rv(0xb0) => (1, 6),
        f if f == rv(0x1c8) => (3, 1),
        f if f == rv(0x1f0) => (3, 0),
        f if f == rv(0x1c0) || f == rv(0x1d0) || f == rv(0x1e0) => (2, 0),
        f if f & 0xfff0_0000 == 0x01f0_0000 => (1, 0),
        WSTR_FORMAT if COUNTS_BUF.contains(&w[0]) => (4, 0),
        WSTR_FORMAT => (3, 0),
        TRICK_INFO_VF2 => (4, 0),
        SPRITE_DRAW | FILLED_RECT if func == FILLED_RECT => (1, 4),
        SPRITE_DRAW => (2, 4),
        SET_COLOUR | TEXTBOX_DESTRUCT | TRICK_GET_INFO | FORMAT_TRICK_NAME | GET_TRICK_BOOK | COUNT_DONE_IN_PAGE
        | IS_TRICK_DONE | GET_PLAYER_ID | TEXTBOX_SET_FONT | TEXTBOX_APPEND | TEXTBOX_WRAP | SPRITE_RECT | LOC_STRING
        | STRCMP | STRCPY => (2, 0),
        GAME_TRICK_INFO_VF5 => (2, 2),
        GET_TRICK_NAME | COUNT_COMPLETE_PAGES | TRICKS_PER_PAGE | MENU_IS_ACTIVE | TEXTBOX_INIT | TEXTBOX_RESET
        | GET_LESSON | PAUSE_MENU_VF2 | DIALOG_DRAW | STRLEN | MEM_DELETE => (1, 0),
        TEXTBOX_DRAW => (3, 0),
        TEXTBOX_SET_RECT => (1, 4),
        TEXTBOX_LINE_HEIGHT => (1, 1),
        TEXTBOX_SET_SCALE => (1, 2),
        PRINTF_SHADOWED | PRINTF_CENTRED | PRINT_CENTRED => (2, 3),
        MEASURE_WIDE => (3, 1),
        PRINT_WRAPPED => (3, 4),
        BACKDROP => (2, 1),
        MENU_LIST => (10, 3),
        HUD_SPRITE => (3, 4),
        _ => panic!("no arity for {func:#x}"),
    }
}

fn string(io: &mut dyn Io, a: u32, log: &mut Vec<u32>) {
    for k in 0..0x80 {
        let c = io.r8(a + k);
        log.push(c as u32);
        if c == 0 {
            break;
        }
    }
}
fn words(io: &mut dyn Io, a: u32, n: u32, log: &mut Vec<u32>) {
    log.extend((0..n).map(|k| io.r32(a + 4 * k)));
}
fn len(io: &mut dyn Io, a: u32) -> u32 {
    (0..0x100).find(|&k| io.r8(a + k) == 0).unwrap_or(0x100)
}
fn write_str(io: &mut dyn Io, a: u32, n: u32, r: u32) {
    for k in 0..n {
        io.w8(a + k, b'a' + ((r >> (k % 24)) as u8 + k as u8) % 26);
    }
    io.w8(a + n, 0);
}

fn standin(io: &mut dyn Io, func: u32, w: &[u32], f: &[u32], ans: &Answers) -> (u32, Call) {
    use ld::*;
    let (ni, nf) = arity(func, w);
    let mut log: Vec<u32> = w[..ni].to_vec();
    log.extend(&f[..nf]);
    match func {
        PRINT_CENTRED | PRINTF_CENTRED | PRINTF_SHADOWED | MEASURE_WIDE | TEXTBOX_APPEND | STRCPY => {
            string(io, w[1], &mut log)
        }
        PRINT_WRAPPED => {
            string(io, w[0], &mut log);
            words(io, w[1], 4, &mut log);
        }
        STRCMP => {
            string(io, w[0], &mut log);
            string(io, w[1], &mut log);
        }
        STRLEN => string(io, w[0], &mut log),
        SET_COLOUR => words(io, w[1], 4, &mut log),
        TEXTBOX_DRAW => {
            words(io, w[0] + 0x20, 2, &mut log);
            words(io, w[0] + 0x3c, 4, &mut log);
        }
        _ => {}
    }
    if matches!(func, SPRITE_DRAW | HUD_SPRITE | FILLED_RECT | DIALOG_DRAW | MENU_LIST) || func == rv(0x1c0) {
        words(io, COLOUR, 5, &mut log);
    }
    let r = ans.next();
    let answer = match func {
        LOC_STRING | GET_TRICK_NAME => STRS + 0x10 * (r % 32),
        GET_LESSON => LO,
        PAUSE_MENU_VF2 => PM,
        MENU_IS_ACTIVE => [0, 0, 0, 1][(r % 4) as usize],
        COUNT_COMPLETE_PAGES => r % 7,
        IS_TRICK_DONE => r & 1,
        COUNT_DONE_IN_PAGE | TRICKS_PER_PAGE => r % 6,
        MEASURE_WIDE => r % 300,
        STRLEN => len(io, w[0]),
        STRCMP => {
            let (a, b) = (w[0], w[1]);
            let mut k = 0;
            loop {
                let (x, y) = (io.r8(a + k), io.r8(b + k));
                if x != y || x == 0 {
                    break (x as i32 - y as i32) as u32;
                }
                k += 1;
            }
        }
        STRCPY => {
            let n = len(io, w[1]).min(0x60);
            for k in 0..n {
                let c = io.r8(w[1] + k);
                io.w8(w[0] + k, c);
            }
            io.w8(w[0] + n, 0);
            w[0]
        }
        WSTR_FORMAT => {
            write_str(io, w[0], r % 20, r);
            r
        }
        FORMAT_TRICK_NAME => {
            write_str(io, w[0], [r % 0x34, 0x34, 0x35, r % 0x60][(r >> 28) as usize % 4], r);
            r
        }
        TEXTBOX_SET_SCALE => {
            let v = 5.0 + (r >> 8) as f32 / (1 << 20) as f32;
            io.w32(w[0] + 0x14, v.to_bits());
            r
        }
        _ => r,
    };
    (answer, Call(func, log))
}

struct Env<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl TextBoxEnv for Env<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32], fargs: &[F]) -> u32 {
        let mut w = args.to_vec();
        w.resize(10, 0);
        let mut f: Vec<u32> = fargs.iter().map(|x| x.0.to_bits()).collect();
        f.resize(8, 0);
        let (a, c) = standin(m, func, &w, &f, self.ans);
        self.log.push(c);
        assert!(self.log.len() < 100_000, "the port calls on and on");
        a
    }
}

fn wf(m: &mut Mem, at: u32, v: f32) {
    m.w32(at, v.to_bits());
}

const FUNCS: [u32; 19] = [
    0x14bbe8, 0x14bd98, 0x14c098, 0x14c2d0, 0x14cf38, 0x14cfb0, 0x14d138, 0x14d320, 0x14d5e0, 0x14ed88, 0x1550f8,
    0x1550d0, 0x1550c8, 0x1550e0, 0x1550e8, 0x17ded8, 0x14c2c0, 0x14c2c8, 0x14c098,
];

#[test]
fn lesson_drawing_matches_the_original() {
    use ld::*;
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS {
        r.allow(f);
    }
    let mut stubs = vec![
        SPRITE_DRAW, SET_COLOUR, TEXTBOX_DESTRUCT, TRICK_GET_INFO, TRICK_INFO_VF2, GAME_TRICK_INFO_VF5,
        FORMAT_TRICK_NAME, GET_TRICK_NAME, GET_TRICK_BOOK, COUNT_COMPLETE_PAGES, TRICKS_PER_PAGE, COUNT_DONE_IN_PAGE,
        IS_TRICK_DONE, GET_PLAYER_ID, WSTR_FORMAT, MENU_IS_ACTIVE, TEXTBOX_INIT, TEXTBOX_DRAW, TEXTBOX_RESET,
        TEXTBOX_SET_FONT, TEXTBOX_SET_RECT, TEXTBOX_APPEND, TEXTBOX_LINE_HEIGHT, TEXTBOX_SET_SCALE, TEXTBOX_WRAP,
        FILLED_RECT, PRINTF_SHADOWED, PRINTF_CENTRED, PRINT_CENTRED, MEASURE_WIDE, PRINT_WRAPPED, GET_LESSON,
        PAUSE_MENU_VF2, BACKDROP, DIALOG_DRAW, MENU_LIST, HUD_SPRITE, SPRITE_RECT, LOC_STRING, STRCMP, STRCPY, STRLEN,
        MEM_DELETE,
    ];
    stubs.extend(ROFFS.map(rv));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    for func in stubs {
        let (l, a) = (log.clone(), ans.clone());
        r.stub(func, move |c, mem| {
            let mut w: Vec<u32> = (4..12).map(|k| c.gpr(k) as u32).collect();
            let sp = c.gpr(29) as u32;
            w.push(mem.read_u32(sp));
            w.push(mem.read_u32(sp + 8));
            let f: Vec<u32> = (12..20).map(|k| c.f(k).to_bits()).collect();
            let (v, call) = standin(mem, func, &w, &f, &a);
            l.borrow_mut().push(call);
            assert!(l.borrow().len() < 100_000, "the original calls on and on");
            c.set_gpr(2, v as i32 as i64 as u64);
            c.set_f(0, f32::from_bits(v));
        });
    }
    let read = |r: &mut Runner, (at, n): (u32, u32)| {
        let mut b = vec![0u8; n as usize];
        r.mem.read(at, &mut b).unwrap();
        b
    };
    let data = read(&mut r, DATA);
    let rodata = read(&mut r, RODATA);
    let mut g = Rng(0x1e55_0000_0000_0001);
    let (mut lit, mut boxes, mut menus) = (0usize, 0usize, 0usize);
    let per = 400u32;
    for case in 0..FUNCS.len() as u32 * per {
        let func = FUNCS[(case % FUNCS.len() as u32) as usize];
        let mut m = Mem {
            regions: vec![
                (DATA.0, data.clone()),
                (RODATA.0, rodata.clone()),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        // the app, its renderer and table, the race, the pad config's owner and config
        m.w32(ld::APP, APPOBJ);
        m.w32(APPOBJ + 0x724, RENDER);
        m.w32(APPOBJ + 0x730, RACE);
        m.w32(RENDER + 0x10, RVT);
        for off in ROFFS {
            m.w16(RVT + off, g.pick(&[0u16, 0x10]));
            m.w32(RVT + off + 4, rv(off));
        }
        m.w32(RACE + 0x1c, R1);
        m.w32(R1 + 0x8c, OWNER);
        m.w32(OWNER + 0x870, CFG);
        for k in 0..0x40 {
            let v = g.next();
            let bit = 1u32 << g.below(16);
            m.w32(CFG + 4 * k, g.pick(&[v, u32::MAX, 0, bit, bit]));
        }
        let mask = g.next();
        m.w32(CFG + 0x84, g.pick(&[0xffff, mask, 0x3c0, 0]));
        m.w32(LO + 0x328, g.pick(&[0, 0, 1]));
        m.w32(PM + 0x42c, PMX);
        m.w32(PMX + 0xc, g.pick(&[0, 0, 0, 1]));
        // the pulse, the font colour
        let p = g.f(0.9, 1.1);
        wf(&mut m, ld::PULSE, g.pick(&[1.0, 1.06, 0.94, 1.05, 0.95, p]));
        wf(&mut m, ld::PULSE_STEP, g.pick(&[0.004, -0.004]));
        // strings (some empty)
        for i in 0..32 {
            let n = g.pick(&[0, 3, 9, 15]);
            let r0 = g.next();
            write_str(&mut m, STRS + 0x10 * i, n, r0);
        }
        // the lesson manager
        m.w32(LM + 0xc, g.below(13));
        let bit = 1u32 << g.below(32);
        for at in [0x58u32, 0x5c, 0x64, 0xac] {
            let v = g.next();
            m.w32(LM + at, g.pick(&[0, 1, 3, v]));
        }
        wf(&mut m, LM + 0xa0, g.f(-50.0, 400.0));
        wf(&mut m, LM + 0xa4, g.f(-50.0, 400.0));
        let rec_n = g.pick(&[0x78u32, 0x79, 0x80, 0x90, 5, 0, u32::MAX]);
        m.w32(LM + 0x46c, REC);
        m.w32(REC, rec_n);
        if rec_n == 0x90 || rec_n == 0x80 {
            let last = REC + rec_n * 0x2c - 0x14;
            let (v1, v2) = (g.next(), g.next());
            m.w32(last + 4, g.pick(&[0, bit, 0x3fff, v1]));
            m.w32(last + 8, g.pick(&[0, bit, 0x3ff_ffff, v2]));
        }
        m.w32(LM + 0xb0, g.pick(&[0, 1, 1, 2, 3]));
        m.w32(LM + 0xb4, g.below(5));
        m.w32(LM + 0xb8, g.below(20));
        m.w32(LM + 0x4e8, g.below(40));
        m.w32(LM + 0x50c, g.pick(&[0, 1, 4, 5, 6, u32::MAX]));
        m.w32(LM + 0x510, g.below(17));
        let y = g.f(-100.0, 300.0);
        wf(&mut m, LM + 0x53c, g.pick(&[0.0, 40.0, y]));
        let (a1, f1, f2) = match func {
            0x14c098 | 0x14c2d0 | 0x14cfb0 | 0x14d138 | 0x14d320 | 0x14c2c0 | 0x14c2c8 => (FONTS, 0.0, 0.0),
            0x14cf38 => (0, g.f(-10.0, 700.0), g.f(-10.0, 500.0)),
            0x14d5e0 => (g.pick(&[0, 1, 2]), 0.0, 0.0),
            0x1550f8 => (g.pick(&[0, 1, 2, 3]), 0.0, 0.0),
            0x14ed88 => (g.next() % 0x1000, 0.0, 0.0),
            _ => (g.next(), 0.0, 0.0),
        };
        let a0 = match func {
            _ => LM,
        };
        let seed = g.next() as u64 | (g.next() as u64) << 32;
        ans.seed.set(seed);
        ans.n.set(0);
        let before = m.regions.clone();
        // ours
        let mut env = Env { log: vec![], ans: &ans };
        let none = 0x5eed_5eed;
        let (e, mm) = (&mut env, &mut m);
        let ours = match func {
            0x14bbe8 => { draw(mm, e, a0); none }
            0x14bd98 => { draw_buttons(mm, e, a0); none }
            0x14c098 => { draw_page(mm, e, a0, a1, TOP); none }
            0x14c2d0 => { draw_trick_book_page(mm, e, a0, a1, TOP); none }
            0x14cf38 => { draw_trick_info(mm, e, a0, F(f1), F(f2)); none }
            0x14cfb0 => { draw_trick_page(mm, e, a0, a1, TOP); none }
            0x14d138 => { draw_tip_page(mm, e, a0, a1, TOP); none }
            0x14d320 => { draw_instructions(mm, e, a0, a1, TOP); none }
            0x14d5e0 => { draw_menu(mm, e, a0, a1); none }
            0x14ed88 => get_string(mm, e, a1 % 0x1000),
            0x1550f8 => { trick_info_destruct(mm, e, a0, a1); none }
            0x1550d0 => { assert_eq!(trick_info_vf4().0.to_bits(), 0); none }
            _ => { nothing(); none }
        };
        lit += env.log.iter().filter(|c| c.0 == SPRITE_DRAW).count();
        boxes += env.log.iter().filter(|c| c.0 == FILLED_RECT).count();
        menus += env.log.iter().filter(|c| c.0 == MENU_LIST).count();
        // theirs
        ans.n.set(0);
        log.borrow_mut().clear();
        r.mem.write(STACK.0, &vec![0u8; STACK.1 as usize]).unwrap();
        for (at, b) in &before {
            if *at != STACK.0 {
                r.mem.write(*at, b).unwrap();
            }
        }
        r.cpu.set_gpr(4, a0 as i32 as i64 as u64);
        r.cpu.set_gpr(5, a1 as i32 as i64 as u64);
        r.cpu.set_f(12, f1);
        r.cpu.set_f(13, f2);
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if ours != none {
            assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x}: the answer");
        }
        if func == 0x1550d0 {
            assert_eq!(r.cpu.f(0).to_bits(), 0, "case {case}: vf4's answer");
        }
        let theirs = log.borrow().clone();
        if theirs != env.log {
            let i = theirs.iter().zip(&env.log).position(|(a, b)| a != b).unwrap_or(theirs.len().min(env.log.len()));
            panic!(
                "case {case} {func:#x}: the calls part at {i} of {}/{}: theirs {:x?} ours {:x?}",
                theirs.len(),
                env.log.len(),
                theirs.get(i),
                env.log.get(i)
            );
        }
        for (at, b) in &m.regions {
            if *at == STACK.0 {
                continue;
            }
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    println!("{lit} sprites, {boxes} rects, {menus} menus");
    assert!(lit > 300 && boxes > 1000 && menus > 300);
}
