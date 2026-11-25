//! `tricky_game::memcard` (board row E12f1) against the original in the function runner (TRICKY_ELF; skipped
//! without it). Each case builds a driver in an arena (random words, NUL-ended strings in its string fields and
//! file names, port results from the values the game tests for), runs one of the row's functions on both sides
//! from the same stack pointer, and compares the calls, the answers and the memory (the stack aside). The `sceMc*`
//! calls, `MemCard_Process`, `sprintf`, the text calls, the frees and the driver's own virtuals are stand-ins that
//! log their arguments (and the strings they are handed) and answer from the case's seed; `sceMcSync` writes its
//! answer to the result word. `memset`, `strcpy` and `strlen` are the game's own.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use r5900::Runner;
use tricky_game::fe3dstate::FeEnv;
use tricky_game::memcard as mc;
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

const APPDATA: (u32, u32) = (0x337000, 0x1000);
const GLOBALS: (u32, u32) = (0x345000, 0x2000);
const RODATA: (u32, u32) = (0x3a4000, 0x1000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x1000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;
const C: u32 = ARENA + 0x100;
const VT: u32 = ARENA + 0x800;
const APPOBJ: u32 = ARENA + 0xc00;
const STRS: u32 = ARENA + 0xd00;
const OUT: u32 = ARENA + 0xe00;
fn vf(off: u32) -> u32 {
    0x01f0_0000 + off
}
const VOFFS: [u32; 12] = [0x88, 0x98, 0xf0, 0x148, 0x158, 0x168, 0x170, 0x178, 0x180, 0x198, 0x1c8, 0x1e8];
const RESULTS: [i32; 17] = [0, -1, -2, -3, -5, -6, -7, -8, -10, -11, -9999, -10000, -10001, -10002, -10003, 1, 5];

const FUNCS: [u32; 71] = [
    0x23d908, 0x23d940, 0x23d998, 0x23db08, 0x23db10, 0x23db40, 0x23dbc8, 0x23dc40, 0x23dc50, 0x23dd10, 0x23dd18,
    0x23dd70, 0x23dd80, 0x23dd88, 0x23dd90, 0x23ddf8, 0x23de08, 0x23de48, 0x23de98, 0x23dea0, 0x23dea8, 0x23df20,
    0x23df80, 0x23df98, 0x23dfa8, 0x23dfb8, 0x23dfc0, 0x23e038, 0x23e090, 0x23e0a8, 0x23e0b8, 0x23e0c8, 0x23e928,
    0x23e988, 0x23e9b0, 0x23ea18, 0x23ea78, 0x23ea98, 0x23eac0, 0x23eb18, 0x23eb28, 0x23eb48, 0x23eb60, 0x23eb80,
    0x23ebd8, 0x23ebf0, 0x23ec08, 0x23ecf0, 0x23ed08, 0x23ed10, 0x23ed18, 0x23ed48, 0x23ed60, 0x23ed78, 0x23edb0,
    0x23fda8, 0x23fe38, 0x23fe48, 0x23fea8, 0x23ff18, 0x23fed8, 0x23ff38, 0x23ff70, 0x240010, 0x240040, 0x240070,
    0x2400f8, 0x23fcb8, 0x23ffe0, 0x23fb60, 0x23edc8,
];
/// The constant and field answers (`memcard::constant`)
const CONSTS: [u32; 11] =
    [0x23fec0, 0x23fec8, 0x23fed0, 0x23ff08, 0x23ff10, 0x23ffa0, 0x23ff68, 0x23ffd0, 0x23ffd8, 0x2400a0, 0x23ffa8];
/// Answers that read the current port
const CURRENT: [u32; 2] = [0x23ffb0, 0x23fff0];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

trait Io {
    fn r8(&mut self, a: u32) -> u8;
    fn w32(&mut self, a: u32, v: u32);
}
impl Io for Mem {
    fn r8(&mut self, a: u32) -> u8 {
        Mem::r8(self, a)
    }
    fn w32(&mut self, a: u32, v: u32) {
        Mem::w32(self, a, v)
    }
}
impl Io for r5900::Mem {
    fn r8(&mut self, a: u32) -> u8 {
        let mut b = [0u8];
        self.read(a, &mut b).unwrap();
        b[0]
    }
    fn w32(&mut self, a: u32, v: u32) {
        self.write(a, &v.to_le_bytes()).unwrap()
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

fn arity(func: u32, w: &[u32]) -> usize {
    match func {
        mc::MC_GET_DIR => 6,
        mc::MC_GET_INFO => 5,
        mc::SPRINTF if w[1] == mc::DIR_FMT => 3,
        mc::SPRINTF => 4,
        mc::MC_SYNC | mc::WSTR_FORMAT | mc::WSTR_TO_ASCII | mc::ASYNC_FINISH => 3,
        mc::WSTR_SET_ASCII => 2,
        mc::PROCESS | mc::MC_FORMAT | mc::LOC_STRING | mc::WSTR_COPY => 2,
        f if f == vf(0xf0) => 4,
        f if f == vf(0x88) || f == vf(0x170) || f == vf(0x178) || f == vf(0x1e8) => 1,
        f if f & 0xfff0_0000 == 0x01f0_0000 => 2,
        _ => 1,
    }
}

fn standin(io: &mut dyn Io, func: u32, w: &[u32], ans: &Answers) -> (u32, Call) {
    let mut log = w[..arity(func, w)].to_vec();
    // the strings handed over
    let s = match func {
        mc::SPRINTF => Some(w[2]),
        f if f == vf(0xf0) => Some(w[1]),
        _ => None,
    };
    if let Some(s) = s {
        for k in 0..0x80 {
            let c = io.r8(s + k);
            log.push(c as u32);
            if c == 0 {
                break;
            }
        }
    }
    let r = ans.next();
    let v = match func {
        mc::MC_GET_DIR | mc::MC_FORMAT => [0, 1, (-1i32) as u32, (-4i32) as u32][(r % 4) as usize],
        mc::MC_GET_INFO => [0, 0, 1, (-1i32) as u32][(r % 4) as usize],
        f if f == vf(0x88) || f == vf(0x98) => r % 40,
        f if f == vf(0x170) || f == vf(0x178) => [0, 1, 1, 1][(r % 4) as usize],
        f if f == vf(0x1c8) => [0, 0, 1][(r % 3) as usize],
        f if f == vf(0x1e8) => r % 2,
        mc::ASYNC_IS_DONE => [0, 1, 1, 2][(r % 4) as usize],
        mc::MC_SYNC => {
            let codes = [
                0, -1, -1, 5, 1, 2, -4, -2, -10, -15, -45, -55, -75, -3, -10001, -20, -11, -19, -40, -49, -50, -59, -70,
                -79, -80, -69, -39, -60,
            ];
            io.w32(w[2], codes[(r >> 8) as usize % codes.len()] as u32);
            if r % 6 == 0 { 0 } else { r }
        }
        _ => r,
    };
    (v, Call(func, log))
}

struct Env<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl FeEnv for Env<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(8, 0);
        let (v, c) = standin(m, func, &w, self.ans);
        self.log.push(c);
        v
    }
}

fn put_str(m: &mut Mem, at: u32, n: u32, r: u32) {
    for k in 0..n {
        m.w8(at + k, b'A' + ((r >> (k % 24)) as u8 + k as u8) % 26);
    }
    m.w8(at + n, 0);
}

#[test]
fn memory_card_driver_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in FUNCS.iter().chain(&CONSTS).chain(&CURRENT) {
        r.allow(*f);
    }
    for f in [0x2cc808u32, 0x2fd14c, 0x2fd260, 0x23d680, 0x23d868] {
        r.allow(f);
    }
    let mut stubs = vec![
        mc::ASYNC_IS_DONE, mc::ASYNC_FINISH,
        mc::PROCESS, mc::MC_GET_DIR, mc::MC_FORMAT, mc::MC_GET_INFO, mc::MC_SYNC, mc::SPRINTF, mc::LOC_STRING,
        mc::WSTR_COPY, mc::WSTR_FORMAT, mc::WSTR_TO_ASCII, mc::EA_FREE, mc::MEM_DELETE, mc::WSTR_SET_ASCII,
    ];
    stubs.extend(VOFFS.map(vf));
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    for func in stubs {
        let (l, a) = (log.clone(), ans.clone());
        r.stub(func, move |c, mem| {
            let w: Vec<u32> = (4..12).map(|k| c.gpr(k) as u32).collect();
            let (v, call) = standin(mem, func, &w, &a);
            l.borrow_mut().push(call);
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    let read = |r: &mut Runner, (at, n): (u32, u32)| {
        let mut b = vec![0u8; n as usize];
        r.mem.read(at, &mut b).unwrap();
        b
    };
    let (appdata, globals, rodata) = (read(&mut r, APPDATA), read(&mut r, GLOBALS), read(&mut r, RODATA));
    let mut g = Rng(0x3c_a4d0_0000_0001);
    let all: Vec<u32> = FUNCS.iter().chain(&CONSTS).chain(&CURRENT).copied().collect();
    let mut processed = 0usize;
    let base = all.len() as u32 * 150;
    // the per-frame step (vf2) gets 8000 more, for its states
    for case in 0..base + 8000 {
        let func = if case < base { all[(case % all.len() as u32) as usize] } else { 0x23edc8 };
        let mut m = Mem {
            regions: vec![
                (APPDATA.0, appdata.clone()),
                (GLOBALS.0, globals.clone()),
                (RODATA.0, rodata.clone()),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        m.w32(mc::APP, APPOBJ);
        m.w32(mc::RESULT, g.pick(&[0, u32::MAX, 3]));
        m.w32(C, VT);
        for off in VOFFS {
            m.w16(VT + off, g.pick(&[0u16, 0, 8]));
            m.w32(VT + off + 4, vf(off));
        }
        m.w32(C + 4, g.pick(&[0, 0x34, 0x35, 7]));
        m.w32(C + 0x10, g.below(2));
        m.w32(C + 0x14, g.pick(&[4, 4, 0, 2, u32::MAX]));
        m.w32(C + 0x1c, g.below(4));
        for at in [0x3c, 0x30, 0x40, 0x44, 0x24] {
            let v = g.next();
            m.w32(C + at, g.pick(&[0, 1, v]));
        }
        for i in 0..2 {
            let p = C + 0x14 * i;
            m.w32(p + 0x17c, g.pick(&RESULTS) as u32);
            let st = g.next();
            m.w32(p + 0x178, g.pick(&[0, 3, (-2i32) as u32, st]));
            for at in [0x180, 0x184] {
                m.w32(p + at, g.pick(&[0, 1]));
            }
        }
        for at in [0x71u32, 0xb1, 0xf1, 0x131, 0x4c0] {
            let (n, v) = (g.below(30), g.next());
            put_str(&mut m, C + at, n, v);
        }
        for i in 0..4 {
            let (n, v) = (g.pick(&[0, 0, 1, 5, 20]), g.next());
            put_str(&mut m, C + 0x70 * i + 0x1a8, n, v);
            for at in [0x19c, 0x1a0, 0x1a4] {
                m.w32(C + 0x70 * i + at, g.pick(&[0, 0, 1]));
            }
        }
        for i in 0..8 {
            let (n, v) = (g.below(25), g.next());
            put_str(&mut m, STRS + 0x20 * i, n, v);
        }
        if func == 0x23edc8 {
            // the per-frame step: every state, the flags mostly clear, a directory of names ending in a file's
            // digit, a title in the icon block
            let st = g.next();
            m.w32(C + 4, if case % 5 == 0 { g.pick(&[st, 0x3c, 0x3d]) } else { g.below(0x3c) });
            for at in [0x3c, 0x40, 0x44] {
                m.w32(C + at, g.pick(&[0, 0, 0, 0, 1]));
            }
            m.w32(C + 0x28, g.below(4));
            let (a, b) = (m.r32(C + 0x24), m.r32(0x3a49e0 + 4 * g.below(3)));
            let v = g.next();
            m.w32(C + 0x390, g.pick(&[a, b, b, v]));
            m.w32(C + 0x20, g.pick(&[0, 1, 2, 5, u32::MAX]));
            m.w32(mc::INFO_TYPE, g.pick(&[2, 2, 1]));
            m.w32(mc::INFO_FORMAT, g.pick(&[0, 1, 1]));
            for j in 0..6 {
                let (n, v) = (g.below(12), g.next());
                let at = C + 0x3e0 + 0x40 * j;
                put_str(&mut m, at, n, v);
                if n > 0 && g.below(2) == 0 {
                    m.w8(at + n - 1, b'1' + g.below(5) as u8);
                }
            }
            let n = g.below(20);
            for k in 0..n {
                let (hi, any) = (g.below(0xc0) + 0x40, g.below(255) + 1);
                let h = hi << 8 | g.pick(&[0x81, 0x82, 0x82, any]);
                m.w16(0x3460d0 + 2 * k, h as u16);
            }
            m.w16(0x3460d0 + 2 * n, 0);
        }
        let name = STRS + 0x20 * g.below(8);
        let (a1, a2, a3) = match func {
            0x23db10 | 0x23de48 | 0x23dea8 | 0x23dfc0 => (name, g.next(), g.next()),
            0x23fea8 => (name, g.pick(&[0, 0, 1]), 0),
            0x23df20 | 0x23e038 => (g.below(4), g.next(), g.next()),
            0x23ecf0 => (g.below(4), OUT, 0),
            0x23ec08 => (g.below(2), OUT, 0),
            0x23ffe0 => (OUT, 0, 0),
            0x23fb60 => (g.below(4), OUT, 0),
            0x23fe48 => (g.pick(&[0, 1, 2, u32::MAX]), OUT, 0),
            0x23fe38 => (g.pick(&RESULTS) as u32, 0, 0),
            0x23ff18 => (g.pick(&[0, 1, 0x3ff, 0x400, 0x401, 0x12345, u32::MAX, 0x8000_0000, 0xffff_fc00]), 0, 0),
            0x23d940 | 0x2400f8 => (g.below(4), 0, 0),
            0x23dc40 | 0x23dd70 => (g.below(4), 0, 0),
            0x23ddf8 => (g.below(2), 0, 0),
            0x23ea18 | 0x23eb28 | 0x23eb48 | 0x23eb80 | 0x23ebd8 | 0x23ebf0 => (g.below(2), 0, 0),
            0x23ed18 | 0x23ed48 | 0x23ed60 | 0x23ed78 | 0x23edb0 => (g.below(4), 0, 0),
            _ => (g.next(), g.next(), g.next()),
        };
        let seed = g.next() as u64 | (g.next() as u64) << 32;
        ans.seed.set(seed);
        ans.n.set(0);
        let before = m.regions.clone();
        let mut env = Env { log: vec![], ans: &ans };
        let none = 0x5eed_5eed;
        let (mm, e) = (&mut m, &mut env);
        let ours = match func {
            0x23d908 => mc::construct(mm, C),
            0x23d940 => { mc::destruct(mm, e, C, a1); none }
            0x23d998 => { mc::reset(mm, C); none }
            0x23db08 | 0x23de98 | 0x23dea0 | 0x23ed10 => none,
            0x23db10 => { mc::set_dir(mm, e, C, a1); none }
            0x23db40 => { mc::read_dir(mm, e, C); none }
            0x23dbc8 => mc::named_files(mm, C),
            0x23dc40 => { mc::pick_22(mm, e, C, a1); none }
            0x23dc50 => { mc::stat_picked(mm, e, C); none }
            0x23dd10 => { mc::state_11(mm, e, C); none }
            0x23dd18 => { mc::format(mm, e, C); none }
            0x23dd70 => { mc::pick_13(mm, e, C, a1); none }
            0x23dd80 => mc::entry_word(mm, C),
            0x23dd88 => { mc::set_24(mm, C, a1); none }
            0x23dd90 => { mc::build_save_path(mm, e, C); none }
            0x23ddf8 => { mc::set_port(mm, e, C, a1); none }
            0x23de08 => { mc::clear_changed(mm, C); none }
            0x23de48 => { mc::open_dir(mm, e, C, a1); none }
            0x23dea8 => { mc::begin_save(mm, e, C, a1, a2, a3); none }
            0x23df20 => { mc::begin_save_slot(mm, e, C, a1, a2, a3, TOP); none }
            0x23df80 => { mc::save_more(mm, e, C, a1, a2); none }
            0x23df98 => mc::saving(mm, C),
            0x23dfa8 => { mc::flag_44(mm, C); none }
            0x23dfb8 => { mc::end_save(mm, e, C); none }
            0x23dfc0 => { mc::begin_load(mm, e, C, a1, a2, a3); none }
            0x23e038 => { mc::begin_load_slot(mm, e, C, a1, a2, a3, TOP); none }
            0x23e090 => { mc::load_more(mm, e, C, a1, a2); none }
            0x23e0a8 => mc::loading(mm, C),
            0x23e0b8 => { mc::flag_40(mm, C); none }
            0x23e0c8 => { mc::end_load(mm, e, C); none }
            0x23e928 => { mc::start_get_info(mm, e, C); none }
            0x23e988 => mc::busy(mm, C),
            0x23e9b0 => mc::result_is_card_error(mm, C),
            0x23ea18 => mc::slot_error(mm, C, a1),
            0x23ea78 => mc::has_error(mm, C),
            0x23ea98 => { mc::clear_current_status(mm, C); none }
            0x23eac0 => { mc::reset_state(mm, C); none }
            0x23eb18 => { mc::clear_name(mm, C); none }
            0x23eb28 => mc::no_card(mm, C, a1),
            0x23eb48 => mc::slot_status(mm, C, a1),
            0x23eb60 => mc::current_status(mm, C),
            0x23eb80 => mc::usable(mm, C, a1),
            0x23ebd8 => mc::formatted(mm, C, a1),
            0x23ebf0 => mc::changed(mm, C, a1),
            0x23ec08 => { mc::slot_text(mm, e, C, a1, a2); none }
            0x23ecf0 => { mc::copy_name(mm, C, a1, a2); none }
            0x23ed08 => mc::vf37(mm, e, C, a1, a2),
            0x23ed18 => mc::has_name(mm, C, a1),
            0x23ed48 => { mc::flag_file(mm, C, a1, 0x19c); none }
            0x23ed60 => { mc::flag_file(mm, C, a1, 0x1a0); none }
            0x23ed78 => mc::file_flagged(mm, C, a1),
            0x23edb0 => mc::file_1a4(mm, C, a1),
            0x23fda8 => { mc::close(mm, e, C); none }
            0x23fe38 => mc::is_error_code(a1),
            0x23fe48 => { mc::vf59(mm, e, C, a1, a2); none }
            0x23fea8 => { mc::set_name(mm, C, a1, a2); none }
            0x23ff18 => mc::blocks_for(a1),
            0x23fed8 => mc::forward(mm, e, C, 0x198, 0x10),
            0x23ff38 => mc::forward(mm, e, C, 0x98, 0x24),
            0x23ff70 => mc::forward(mm, e, C, 0x158, 0x10),
            0x240010 => mc::forward(mm, e, C, 0x180, 0x10),
            0x240040 => mc::forward(mm, e, C, 0x168, 0x10),
            0x240070 => mc::forward(mm, e, C, 0x148, 0x10),
            0x2400f8 => { mc::memcard_destruct(mm, e, C, a1); none }
            0x23fcb8 => { mc::check_room(mm, e, C); none }
            0x23ffe0 => { mc::current_text(mm, e, C, a1); none }
            0x23fb60 => { mc::file_line(mm, e, C, a1, a2, TOP); none }
            0x23edc8 => { mc::update(mm, e, C); none }
            0x23ffb0 => mc::current_error(mm, C),
            0x23fff0 => mc::current_changed(mm, C),
            f => mc::constant(mm, C, f).unwrap(),
        };
        processed += env.log.iter().filter(|c| c.0 == mc::PROCESS).count();
        ans.n.set(0);
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        for (k, v) in [C, a1, a2, a3].iter().enumerate() {
            r.cpu.set_gpr(4 + k, *v as i32 as i64 as u64);
        }
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if ours != none {
            assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x}: the answer");
        }
        assert_eq!(*log.borrow(), env.log, "case {case} {func:#x}: the calls");
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
    println!("{processed} states handed on");
    assert!(processed > 2000);
}

fn p_arity(func: u32) -> usize {
    match func {
        mc::MC_GET_DIR => 6,
        mc::MC_GET_INFO => 5,
        mc::MC_OPEN | mc::SPRINTF => 4,
        mc::MC_MKDIR | mc::MC_READ | mc::MC_WRITE | mc::MC_DELETE | mc::ASCII_TO_SJIS => 3,
        mc::ASYNC_LOAD => 2,
        f if f == vf(0x1c8) => 2,
        _ => 1,
    }
}
fn p_standin(io: &mut dyn Io, func: u32, w: &[u32], ans: &Answers) -> (u32, Call) {
    let mut log = w[..p_arity(func)].to_vec();
    if func == mc::SPRINTF {
        for s in [w[2], w[3]] {
            for k in 0..0x60 {
                let c = io.r8(s + k);
                log.push(c as u32);
                if c == 0 {
                    break;
                }
            }
        }
    }
    let r = ans.next();
    let v = match func {
        mc::MC_OPEN | mc::MC_CLOSE | mc::MC_MKDIR | mc::MC_READ | mc::MC_WRITE | mc::MC_DELETE | mc::MC_GET_DIR
        | mc::MC_GET_INFO => [0, 0, 1, u32::MAX, (-5i32) as u32][(r % 5) as usize],
        f if f == vf(0x1c8) => [0, 0, 0, 1][(r % 4) as usize],
        _ => r,
    };
    (v, Call(func, log))
}
struct PEnv<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl FeEnv for PEnv<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(8, 0);
        let (v, c) = p_standin(m, func, &w, self.ans);
        self.log.push(c);
        v
    }
}

/// `MemCard_Process` (row E12f2) on every state, with the `sceMc*` calls answering 0, 1 or an error.
#[test]
fn memory_card_process_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [mc::PROCESS, 0x23e928, 0x23ed60, 0x2cc808, 0x2ce114, 0x2fd14c, 0x2fd260] {
        r.allow(f);
    }
    let stubs = [
        mc::MC_OPEN, mc::MC_CLOSE, mc::MC_MKDIR, mc::MC_READ, mc::MC_WRITE, mc::MC_DELETE, mc::MC_GET_DIR,
        mc::MC_GET_INFO, mc::SPRINTF, mc::ASYNC_LOAD, mc::ASCII_TO_SJIS, mc::EA_FREE, vf(0x1c8),
    ];
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    for func in stubs {
        let (l, a) = (log.clone(), ans.clone());
        r.stub(func, move |c, mem| {
            let w: Vec<u32> = (4..12).map(|k| c.gpr(k) as u32).collect();
            let (v, call) = p_standin(mem, func, &w, &a);
            l.borrow_mut().push(call);
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    const GLOBALS2: (u32, u32) = (0x345000, 0x2000);
    let read = |r: &mut Runner, (at, n): (u32, u32)| {
        let mut b = vec![0u8; n as usize];
        r.mem.read(at, &mut b).unwrap();
        b
    };
    let (globals, rodata) = (read(&mut r, GLOBALS2), read(&mut r, RODATA));
    let mut g = Rng(0x9e0c_0000_0000_0001);
    let mut calls = 0usize;
    for case in 0..0x40 * 150u32 {
        let state = if case % 0x40 == 0x3f { g.next() } else { case % 0x40 };
        let mut m = Mem {
            regions: vec![
                (GLOBALS2.0, globals.clone()),
                (RODATA.0, rodata.clone()),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        m.w32(mc::RESULT, g.pick(&[0, u32::MAX, 3]));
        m.w32(C, VT);
        m.w16(VT + 0x1c8, g.pick(&[0u16, 8]));
        m.w32(VT + 0x1cc, vf(0x1c8));
        let cnum = g.next();
        m.w32(C + 0xc, g.pick(&[0, 5, 0x10002, cnum]));
        m.w32(C + 0x10, g.below(2));
        m.w32(C + 0x14, g.pick(&[4, 4, 0, 2, u32::MAX]));
        m.w32(C + 0x18, g.pick(&[u32::MAX, 0, 3]));
        m.w32(C + 0x28, g.below(4));
        for at in [0x3c, 0x30] {
            let v = g.next();
            m.w32(C + at, g.pick(&[0, 1, v]));
        }
        for at in [0x50u32, 0x71, 0xb1, 0xf1, 0x131, 0x4c0] {
            let (n, v) = (g.below(30), g.next());
            put_str(&mut m, C + at, n, v);
        }
        for i in 0..4 {
            let (n, v) = (g.pick(&[0, 1, 5, 20]), g.next());
            put_str(&mut m, C + 0x70 * i + 0x1a8, n, v);
        }
        // three different icon names
        for k in 0..3 {
            let v = g.next();
            put_str(&mut m, STRS + 0x20 * k, 3 + k, v);
            m.w32(mc::ICONS + 4 * k, STRS + 0x20 * k);
        }
        let seed = g.next() as u64 | (g.next() as u64) << 32;
        ans.seed.set(seed);
        ans.n.set(0);
        let before = m.regions.clone();
        let mut env = PEnv { log: vec![], ans: &ans };
        mc::process_state(&mut m, &mut env, C, state, TOP);
        calls += env.log.len();
        ans.n.set(0);
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        r.cpu.set_gpr(4, C as u64);
        r.cpu.set_gpr(5, state as i32 as i64 as u64);
        r.call(mc::PROCESS).unwrap_or_else(|e| panic!("case {case} state {state:#x}: {e}"));
        assert_eq!(*log.borrow(), env.log, "case {case} state {state:#x}: the calls");
        for (at, b) in &m.regions {
            if *at == STACK.0 {
                continue;
            }
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} state {state:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    println!("{calls} calls");
    assert!(calls > 5000);
}

/// The SJIS converters (row E12f2b): every byte and random halfwords through the character converters, random
/// strings through the string ones.
#[test]
fn sjis_converters_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x23d628u32, 0x23d680, 0x23d770, 0x23d868] {
        r.allow(f);
    }
    const GLOBALS2: (u32, u32) = (0x345000, 0x2000);
    let mut globals = vec![0u8; GLOBALS2.1 as usize];
    r.mem.read(GLOBALS2.0, &mut globals).unwrap();
    let mut g = Rng(0x5_1150_0000_0001);
    for case in 0..6000u32 {
        let func = [0x23d770u32, 0x23d868, 0x23d628, 0x23d680][(case % 4) as usize];
        let mut m = Mem { regions: vec![(GLOBALS2.0, globals.clone()), (ARENA, vec![0; SIZE as usize])] };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        let (src, dst) = (ARENA + 0x100, ARENA + 0x400);
        let n = g.below(40);
        if func == 0x23d628 {
            for k in 0..n {
                let (x, y) = (g.below(0x60) as u8 + 0x20, (g.below(255) + 1) as u8);
                let c = g.pick(&[x, y]);
                m.w8(src + k, c);
            }
            m.w8(src + n, 0);
        } else if func == 0x23d680 {
            for k in 0..n {
                let (hi, any) = (g.below(0xc0) + 0x40, g.below(255) + 1);
                let h = hi << 8 | g.pick(&[0x81, 0x82, 0x82, any]);
                m.w16(src + 2 * k, h as u16);
            }
            m.w16(src + 2 * n, 0);
        }
        let a1 = match func {
            0x23d770 => {
                let v = g.next();
                g.pick(&[case / 4 % 256, v])
            }
            0x23d868 => {
                let hi = g.below(0xc0) + 0x40;
                let v = g.next();
                g.pick(&[hi << 8 | 0x81, hi << 8 | 0x82, (((hi << 8) | 0x82) as u16 as i16) as i32 as u32, v])
            }
            _ => 0,
        };
        let before = m.regions.clone();
        let ours = match func {
            0x23d770 => mc::ascii_to_sjis(&m, a1),
            0x23d868 => mc::sjis_to_ascii(&m, a1 & 0xffff | if a1 & 0x8000 != 0 { 0xffff_0000 } else { 0 }),
            0x23d628 => { mc::ascii_string_to_sjis(&mut m, dst, src); 0 }
            _ => { mc::sjis_string_to_ascii(&mut m, dst, src); 0 }
        };
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        let a0 = if func >= 0x23d770 {
            if func == 0x23d868 { (a1 & 0xffff) as u16 as i16 as i32 as u32 } else { a1 }
        } else {
            ARENA
        };
        r.cpu.set_gpr(4, a0 as i32 as i64 as u64);
        r.cpu.set_gpr(5, dst as u64);
        r.cpu.set_gpr(6, src as u64);
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        if func >= 0x23d770 {
            assert_eq!(r.cpu.gpr(2) as u32, ours, "case {case} {func:#x}({a0:#x}): the answer");
        }
        for (at, b) in &m.regions {
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            assert!(&th == b, "case {case} {func:#x}: memory");
        }
    }
}

fn t_standin(io: &mut dyn Io, func: u32, w: &[u32], ans: &Answers) -> (u32, Call) {
    let n = match func {
        mc::WSTR_FORMAT => 4,
        _ => 2,
    };
    let mut log = w[..n].to_vec();
    if func == mc::WSTR_FORMAT {
        for k in 0..5 {
            log.push(io.r8(w[3] + k) as u32);
        }
    }
    (ans.next(), Call(func, log))
}
struct TEnv<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl FeEnv for TEnv<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut w = args.to_vec();
        w.resize(8, 0);
        let (v, c) = t_standin(m, func, &w, self.ans);
        self.log.push(c);
        v
    }
}

/// `MemCard_FormatSaveTitle` (row E12f2b): both titles, the course's code from 0 to 8 letters (the library's
/// `strncpy` the game's own), the card's vf and the text calls logged.
#[test]
fn save_title_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [0x1ce8f8u32, mc::STRNCPY] {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    for func in [mc::LOC_STRING, mc::WSTR_COPY, mc::WSTR_FORMAT, vf(0x130)] {
        let (l, a) = (log.clone(), ans.clone());
        r.stub(func, move |c, mem| {
            let w: Vec<u32> = (4..12).map(|k| c.gpr(k) as u32).collect();
            let (v, call) = t_standin(mem, func, &w, &a);
            l.borrow_mut().push(call);
            c.set_gpr(2, v as i32 as i64 as u64);
        });
    }
    const O: u32 = ARENA + 0x100;
    const CARD: u32 = ARENA + 0x400;
    let mut g = Rng(0x717_1e00_0000_0001);
    for case in 0..2000u32 {
        let mut m = Mem {
            regions: vec![
                (0x32d000, vec![0; 0x1000]),
                (0x332000, vec![0; 0x2000]),
                (APPDATA.0, vec![0; APPDATA.1 as usize]),
                (ARENA, vec![0; SIZE as usize]),
                (STACK.0, vec![0; STACK.1 as usize]),
            ],
        };
        for k in 0..SIZE / 4 {
            m.w32(ARENA + 4 * k, g.next());
        }
        m.w32(mc::APP, APPOBJ);
        m.w32(O, g.pick(&[0, 0, 1, 0x8000_0000]));
        m.w32(O + 0x224, CARD);
        m.w32(CARD, VT);
        m.w16(VT + 0x130, g.pick(&[0u16, 8, 0xfff8]));
        m.w32(VT + 0x134, vf(0x130));
        let course = g.below(13);
        m.w32(mc::GAME_COURSE, course);
        let (n, v) = (g.below(9), g.next());
        put_str(&mut m, STRS, n, v);
        m.w32(mc::COURSES + 0x1c * course + 0x10, STRS);
        let slot = g.pick(&[0, 1, 2, 3, u32::MAX]);
        ans.seed.set(g.next() as u64 | (g.next() as u64) << 32);
        ans.n.set(0);
        let before = m.regions.clone();
        let mut env = TEnv { log: vec![], ans: &ans };
        mc::format_save_title(&mut m, &mut env, O, slot, TOP);
        ans.n.set(0);
        log.borrow_mut().clear();
        for (at, b) in &before {
            r.mem.write(*at, b).unwrap();
        }
        r.cpu.set_gpr(4, O as u64);
        r.cpu.set_gpr(5, slot as i32 as i64 as u64);
        r.call(0x1ce8f8).unwrap_or_else(|e| panic!("case {case}: {e}"));
        assert_eq!(*log.borrow(), env.log, "case {case}: the calls");
        for (at, b) in &m.regions {
            if *at == STACK.0 {
                continue;
            }
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            assert!(&th == b, "case {case}: memory at {at:#x}");
        }
    }
}
