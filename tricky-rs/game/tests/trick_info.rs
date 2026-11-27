//! `cGameTrickInfo`, `cTrickInfo::vf2_154688`, `Trick_GetInfo`, and `TrickInfo_BuildText`
//! (board row E1j) against the original in the function runner (`TRICKY_ELF`; skipped without it).
//!
//! Covers all 8 functions in `0x154638..0x15506c`:
//! - `Trick_GetInfo` 0x154638
//! - `cTrickInfo::vf2_154688` 0x154688
//! - `TrickInfo_BuildText` 0x1546c8 (both stubbed callees on 2,000 random cases and real ELF
//!   `EAMem_Set` + `sprintf` across all 360 real tricks in `DAT_0031e210` in regular and goofy stance)
//! - `cGameTrickInfo::vf5_154b18` 0x154b18
//! - `cGameTrickInfo::cGameTrickInfo` 0x154d20
//! - `cGameTrickInfo::vf3_154d60` 0x154d60
//! - `cGameTrickInfo::vf4_154e20` 0x154e20
//! - `cGameTrickInfo::~cGameTrickInfo` 0x155058

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
}

const DATA: (u32, u32) = (0x31d000, 0x23000);
const RODATA: (u32, u32) = (0x36e000, 0x2000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x8000;
const STACK: (u32, u32) = (0x01fe_0000, 0x1_0000);
const TOP: u32 = 0x01ff_0000;

const INFO: u32 = ARENA + 0x0040;
const APPOBJ: u32 = ARENA + 0x0100;
const GAME: u32 = ARENA + 0x0900;
const RACE: u32 = ARENA + 0x0a00;
const PLAYERS: u32 = ARENA + 0x1000;
const BOARDERS: u32 = ARENA + 0x3800;
const RINFO: u32 = ARENA + 0x5000;
const PADCFG: u32 = ARENA + 0x5800;
const ENTRY: u32 = ARENA + 0x6000;
const OUTBUF: u32 = ARENA + 0x6200;
const FONT: u32 = ARENA + 0x6400;
const XPTR: u32 = ARENA + 0x6600;
const CUSTOM_VT: u32 = ARENA + 0x6800;

const STUB_VF1: u32 = 0x01f0_0010;
const STUB_VF3: u32 = 0x01f0_0020;
const STUB_VF4: u32 = 0x01f0_0030;

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

trait Io {
    fn r8(&mut self, a: u32) -> u8;
    fn w8(&mut self, a: u32, v: u8);
    fn r32(&mut self, a: u32) -> u32;
    fn w32(&mut self, a: u32, v: u32);
}
impl Io for Mem {
    fn r8(&mut self, a: u32) -> u8 {
        Mem::r8(self, a)
    }
    fn w8(&mut self, a: u32, v: u8) {
        Mem::w8(self, a, v)
    }
    fn r32(&mut self, a: u32) -> u32 {
        Mem::r32(self, a)
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
    fn w8(&mut self, a: u32, v: u8) {
        self.write(a, &[v]).unwrap()
    }
    fn r32(&mut self, a: u32) -> u32 {
        self.read_u32(a)
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

fn write_bytes(io: &mut dyn Io, dst: u32, s: &[u8]) -> u32 {
    for (i, &b) in s.iter().enumerate() {
        io.w8(dst + i as u32, b);
    }
    io.w8(dst + s.len() as u32, 0);
    s.len() as u32
}

fn read_cstr(io: &mut dyn Io, a: u32, out: &mut Vec<u32>) {
    for k in 0..0x40u32 {
        let b = io.r8(a + k);
        out.push(b as u32);
        if b == 0 {
            break;
        }
    }
}

fn do_sprintf(io: &mut dyn Io, dst: u32, fmt: u32, arg: u32) -> u32 {
    use ld::*;
    match fmt {
        FMT_INT => write_bytes(io, dst, format!("{}", arg as i32).as_bytes()),
        FMT_RAIL => write_bytes(io, dst, b" RAIL"),
        FMT_SPACE_INT => write_bytes(io, dst, format!(" {}", arg as i32).as_bytes()),
        FMT_PLUS => write_bytes(io, dst, b"+"),
        FMT_OR => write_bytes(io, dst, b"OR"),
        FMT_TO => write_bytes(io, dst, b" TO "),
        _ => panic!("unexpected sprintf fmt {fmt:#x}"),
    }
}

fn standin(io: &mut dyn Io, func: u32, w: &[u32], f: &[u32], ans: &Answers) -> (u32, Call) {
    use ld::*;
    let mut log = Vec::new();
    match func {
        MEM_SET => {
            log.extend(&w[..3]);
            for k in 0..w[2] {
                io.w8(w[0] + k, w[1] as u8);
            }
            (w[0], Call(func, log))
        }
        SPRINTF => {
            let has_int = matches!(w[1], FMT_INT | FMT_SPACE_INT);
            log.extend(&w[..if has_int { 3 } else { 2 }]);
            let n = do_sprintf(io, w[0], w[1], w[2]);
            (n, Call(func, log))
        }
        SPRITE_RECT => {
            log.extend(&w[..2]);
            (ans.next(), Call(func, log))
        }
        SPRITE_DRAW => {
            log.extend(&w[..2]);
            log.extend(&f[..4]);
            (0, Call(func, log))
        }
        FONT_PRINTF_SHADOWED => {
            log.extend(&w[..2]);
            log.extend(&f[..3]);
            read_cstr(io, w[1], &mut log);
            (0, Call(func, log))
        }
        FONT_GLYPH_WIDTH => {
            log.extend(&w[..2]);
            (ans.next() % 24, Call(func, log))
        }
        MEM_DELETE => {
            log.push(w[0]);
            (0, Call(func, log))
        }
        STUB_VF1 => {
            log.push(w[0]);
            (0, Call(func, log))
        }
        STUB_VF3 => {
            log.extend(&w[..3]);
            log.push(f[0]);
            log.push(io.r32(w[1]));
            let cur = f32::from_bits(io.r32(w[1]));
            let adv = (ans.next() % 40) as f32;
            io.w32(w[1], (cur + adv).to_bits());
            (0, Call(func, log))
        }
        STUB_VF4 => {
            log.extend(&w[..2]);
            let v = ((ans.next() % 400) as f32 + 0.25).to_bits();
            (v, Call(func, log))
        }
        _ => panic!("unexpected callee {func:#x}"),
    }
}

struct Env<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl TextBoxEnv for Env<'_> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32], fargs: &[F]) -> u32 {
        let mut w = args.to_vec();
        w.resize(6, 0);
        let mut f: Vec<u32> = fargs.iter().map(|x| x.0.to_bits()).collect();
        f.resize(6, 0);
        let (a, c) = standin(m, func, &w, &f, self.ans);
        self.log.push(c);
        a
    }
}

fn load_base(r: &mut Runner) -> (Vec<u8>, Vec<u8>) {
    let mut data = vec![0u8; DATA.1 as usize];
    r.mem.read(DATA.0, &mut data).unwrap();
    let mut rodata = vec![0u8; RODATA.1 as usize];
    r.mem.read(RODATA.0, &mut rodata).unwrap();
    (data, rodata)
}

fn new_mem(data: &[u8], rodata: &[u8]) -> Mem {
    Mem {
        regions: vec![
            (DATA.0, data.to_vec()),
            (RODATA.0, rodata.to_vec()),
            (ARENA, vec![0u8; SIZE as usize]),
            (STACK.0, vec![0u8; STACK.1 as usize]),
        ],
    }
}

fn sync_to_runner(r: &mut Runner, regions: &[(u32, Vec<u8>)]) {
    r.mem.write(STACK.0, &vec![0u8; STACK.1 as usize]).unwrap();
    for (at, bytes) in regions {
        if *at != STACK.0 {
            r.mem.write(*at, bytes).unwrap();
        }
    }
}

fn assert_mem_eq(r: &mut Runner, m: &Mem, ctx: &str) {
    for (at, b) in &m.regions {
        if *at == STACK.0 {
            continue;
        }
        let mut th = vec![0u8; b.len()];
        r.mem.read(*at, &mut th).unwrap();
        if &th != b {
            let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
            let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
            panic!("{ctx}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
        }
    }
}

fn setup_world(m: &mut Mem, rng: &mut Rng, force_goofy: Option<bool>) {
    m.w32(ld::APP, APPOBJ);
    m.w32(APPOBJ + 0x728, 0x0100_3300);
    m.w32(APPOBJ + 0x730, GAME);
    m.w32(GAME + 0x1c, RACE);
    let n_players: i32 = match force_goofy {
        Some(_) => 2,
        None => [-1, 0, 1, 2, 4][(rng.below(5)) as usize],
    };
    m.w32(RACE + 0x7c, n_players as u32);
    let match_slot = if force_goofy.is_some() { 1 } else { rng.below(5) };
    let cnt = n_players.max(0) as u32;
    for i in 0..cnt {
        let p = PLAYERS + 0x900 * i;
        let b = BOARDERS + 0x500 * i;
        let ri = RINFO + 0x80 * i;
        m.w32(RACE + 0x8c + 4 * i, p);
        m.w32(p, b);
        m.w32(b + 0x464, ri);
        let cfg = if i == match_slot { PADCFG } else { PADCFG + 0x300 };
        m.w32(p + 0x870, cfg);
        let goofy = match force_goofy {
            Some(g) => u32::from(g),
            None => rng.below(2),
        };
        m.w32(ri + 0x40, goofy);
    }
}

fn populate_pad_cfg(m: &mut Mem, rng: &mut Rng, stack_safe: bool) {
    let bits: [u32; 12] = [
        0x200, 0x400, 0x1000, 0x2000, 0x10, 0x20, 0x40, 0x800, 0x8, 0x4, 0x1, 0x2,
    ];
    for slot in 0..26u32 {
        let v = if stack_safe {
            match rng.below(5) {
                0 => u32::MAX,
                1 => 0,
                2 | 3 => bits[rng.below(12) as usize],
                _ => bits[rng.below(12) as usize] | bits[rng.below(12) as usize],
            }
        } else {
            match rng.below(6) {
                0 => u32::MAX,
                1 => 0,
                2 => bits[rng.below(12) as usize],
                3 => bits[rng.below(12) as usize] | bits[rng.below(12) as usize],
                4 => bits[rng.below(12) as usize] | bits[rng.below(12) as usize] | bits[rng.below(12) as usize],
                _ => rng.next() & 0x3fff,
            }
        };
        m.w32(PADCFG + 0x1cc + 4 * slot, v);
    }
    let tweak_mask = if stack_safe {
        match rng.below(4) {
            0 => 0,
            1 => bits[rng.below(12) as usize],
            2 => bits[rng.below(12) as usize] | bits[rng.below(12) as usize],
            _ => bits[rng.below(12) as usize] | bits[rng.below(12) as usize] | bits[rng.below(12) as usize],
        }
    } else {
        match rng.below(6) {
            0 => 0,
            1 => bits[rng.below(12) as usize],
            2 => bits[rng.below(12) as usize] | bits[rng.below(12) as usize],
            3 => bits[rng.below(12) as usize] | bits[rng.below(12) as usize] | bits[rng.below(12) as usize],
            4 => 0x3fff,
            _ => rng.next() & 0x3fff,
        }
    };
    m.w32(PADCFG + 0x24c, tweak_mask);
}

fn populate_synthetic_entry(m: &mut Mem, at: u32, rng: &mut Rng) {
    let flags = match rng.below(4) {
        0 => 0,
        1 => (rng.below(16) << 4) | (rng.next() & !0xf0),
        _ => rng.next(),
    };
    let flip = [-3i16, -2, -1, 0, 0, 1, 2, 3][rng.below(8) as usize];
    let spin = [-4i16, -2, -1, 0, 0, 1, 2, 4][rng.below(8) as usize];
    let rail = [0i16, 0, 1, -1][rng.below(4) as usize];
    let tweak1 = [0i16, 0, 1, -1][rng.below(4) as usize];
    let tweak2 = [0i16, 0, 1, -1][rng.below(4) as usize];
    let grab1 = if rng.below(4) == 0 { 0x1a } else { rng.below(26) };
    let grab2 = if rng.below(3) == 0 { 0x1a } else { rng.below(26) };
    m.w32(at, rng.next());
    m.w32(at + 0x4, flags);
    m.w16(at + 0x8, flip as u16);
    m.w16(at + 0xa, spin as u16);
    m.w16(at + 0xc, rail as u16);
    m.w16(at + 0xe, 0);
    m.w16(at + 0x10, tweak1 as u16);
    m.w16(at + 0x12, tweak2 as u16);
    m.w32(at + 0x14, grab1);
    m.w32(at + 0x18, grab2);
}

fn install_stubs(r: &mut Runner, ans: &Rc<Answers>, log: &Rc<RefCell<Vec<Call>>>) {
    use ld::*;
    for f in [
        0x154638u32, 0x154688, 0x1546c8, 0x154b18, 0x154d20, 0x154d60, 0x154e20, 0x155058, 0x1550d0, 0x1550e8,
        0x1550f8,
    ] {
        r.allow(f);
    }
    let targets = [
        MEM_SET,
        SPRINTF,
        SPRITE_RECT,
        SPRITE_DRAW,
        FONT_PRINTF_SHADOWED,
        FONT_GLYPH_WIDTH,
        MEM_DELETE,
        STUB_VF1,
        STUB_VF3,
        STUB_VF4,
    ];
    for &func in &targets {
        let a = Rc::clone(ans);
        let l = Rc::clone(log);
        r.stub(func, move |c, mem| {
            let w: Vec<u32> = (4..10).map(|i| c.gpr(i) as u32).collect();
            let f: Vec<u32> = (12..18).map(|i| c.f(i).to_bits()).collect();
            let (ret, call) = standin(mem, func, &w, &f, &a);
            l.borrow_mut().push(call);
            c.set_gpr(2, ret as i32 as i64 as u64);
            c.set_f(0, f32::from_bits(ret));
        });
    }
}

#[test]
fn trick_get_info_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.allow(0x154638);
    let mut rng = Rng(0x1546_3800_0001);
    for rider in 0..12u32 {
        for trick in 0..30u32 {
            r.cpu.set_gpr(4, rider as i32 as i64 as u64);
            r.cpu.set_gpr(5, trick as i32 as i64 as u64);
            r.call(0x154638).unwrap();
            let got = r.cpu.gpr(2) as u32;
            let ours = ld::trick_get_info(rider, trick);
            assert_eq!(ours, got, "Trick_GetInfo({rider}, {trick})");
        }
    }
    for case in 0..200u32 {
        let rider = rng.next();
        let trick = rng.next();
        r.cpu.set_gpr(4, rider as i32 as i64 as u64);
        r.cpu.set_gpr(5, trick as i32 as i64 as u64);
        r.call(0x154638).unwrap();
        let got = r.cpu.gpr(2) as u32;
        let ours = ld::trick_get_info(rider, trick);
        assert_eq!(ours, got, "Trick_GetInfo random case {case}");
    }
}

#[test]
fn game_trick_info_ctor_dtor_and_vf2_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let (data, rodata) = load_base(&mut r);
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    let rlog = Rc::new(RefCell::new(Vec::new()));
    install_stubs(&mut r, &ans, &rlog);
    let mut rng = Rng(0x154d_2000_0002);

    // Constructor 0x154d20
    for case in 0..200u32 {
        let mut m = new_mem(&data, &rodata);
        for off in (0..0x28u32).step_by(4) {
            m.w32(INFO + off, rng.next());
        }
        let before = m.regions.clone();
        let our_ret = ld::game_trick_info_construct(&mut m, INFO);

        sync_to_runner(&mut r, &before);
        r.cpu.set_gpr(4, INFO as u64);
        r.call(0x154d20).unwrap();
        let orig_ret = r.cpu.gpr(2) as u32;
        assert_eq!(our_ret, orig_ret, "ctor ret case {case}");
        assert_mem_eq(&mut r, &m, &format!("ctor case {case}"));
    }

    // Destructor 0x155058
    for case in 0..200u32 {
        let how = case & 3;
        let mut m = new_mem(&data, &rodata);
        for off in (0..0x28u32).step_by(4) {
            m.w32(INFO + off, rng.next());
        }
        let before = m.regions.clone();
        ans.seed.set(case as u64 + 1);
        ans.n.set(0);
        let mut env = Env { log: Vec::new(), ans: &ans };
        ld::game_trick_info_destruct(&mut m, &mut env, INFO, how);

        ans.n.set(0);
        rlog.borrow_mut().clear();
        sync_to_runner(&mut r, &before);
        r.cpu.set_gpr(4, INFO as u64);
        r.cpu.set_gpr(5, how as u64);
        r.call(0x155058).unwrap();
        assert_eq!(env.log, *rlog.borrow(), "dtor calls case {case}");
        assert_mem_eq(&mut r, &m, &format!("dtor case {case}"));
    }

    // cTrickInfo::vf2_154688 (SetTrick) with custom vtable + signed this-adjustment,
    // plus real cTrickInfo (0x36fad0 -> vf4 = 0.0) and real cGameTrickInfo (0x36fa98 -> vf4_154e20)
    for case in 0..500u32 {
        let mut m = new_mem(&data, &rodata);
        setup_world(&mut m, &mut rng, None);
        populate_pad_cfg(&mut m, &mut rng, true);
        m.w32(INFO, FONT);
        m.w32(INFO + 0x20, rng.f(0.5, 2.0).to_bits());
        m.w32(FONT + 0x18, rng.f(0.5, 2.0).to_bits());
        let mode = if case < 300 {
            0
        } else if case < 400 {
            1
        } else {
            2
        };
        match mode {
            0 => {
                let adj = [-16i16, -4, 0, 4, 12][(rng.below(5)) as usize];
                m.w32(INFO + 0x24, CUSTOM_VT);
                m.w16(CUSTOM_VT + 0x28, adj as u16);
                m.w32(CUSTOM_VT + 0x2c, STUB_VF4);
            }
            1 => {
                m.w32(INFO + 0x24, ld::TRICK_INFO_VT);
            }
            _ => {
                ld::game_trick_info_construct(&mut m, INFO);
                m.w32(INFO, FONT);
                m.w32(INFO + 0x20, rng.f(0.5, 2.0).to_bits());
            }
        }
        let rider = rng.below(12);
        let trick = rng.below(30);
        populate_synthetic_entry(&mut m, ld::trick_get_info(rider, trick), &mut rng);
        let before = m.regions.clone();

        ans.seed.set(case as u64 + 1000);
        ans.n.set(0);
        struct Vf2Env<'a> {
            inner: Env<'a>,
        }
        impl TextBoxEnv for Vf2Env<'_> {
            fn call(&mut self, m: &mut Mem, func: u32, args: &[u32], fargs: &[F]) -> u32 {
                match func {
                    0x1550d0 => ld::trick_info_vf4().0.to_bits(),
                    0x154e20 => ld::game_trick_info_measure(m, &mut self.inner, args[0], args[1], TOP - 0x20)
                        .0
                        .to_bits(),
                    _ => self.inner.call(m, func, args, fargs),
                }
            }
        }
        let mut env = Vf2Env { inner: Env { log: Vec::new(), ans: &ans } };
        ld::trick_info_set_trick(&mut m, &mut env, INFO, PADCFG, rider, trick);

        ans.n.set(0);
        rlog.borrow_mut().clear();
        sync_to_runner(&mut r, &before);
        r.cpu.set_gpr(4, INFO as u64);
        r.cpu.set_gpr(5, PADCFG as u64);
        r.cpu.set_gpr(6, rider as u64);
        r.cpu.set_gpr(7, trick as u64);
        r.call(0x154688).unwrap();
        assert_eq!(env.inner.log, *rlog.borrow(), "vf2 calls case {case} mode={mode}");
        assert_mem_eq(&mut r, &m, &format!("vf2 case {case} mode={mode}"));
    }
}

#[test]
fn trick_info_build_text_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let (data, rodata) = load_base(&mut r);
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    let rlog = Rc::new(RefCell::new(Vec::new()));
    install_stubs(&mut r, &ans, &rlog);
    let mut rng = Rng(0x1546_c800_0003);

    // 2,000 random synthetic cases (stubbed EAMem_Set + sprintf verifying every call)
    for case in 0..2000u32 {
        let mut m = new_mem(&data, &rodata);
        setup_world(&mut m, &mut rng, None);
        populate_pad_cfg(&mut m, &mut rng, false);
        populate_synthetic_entry(&mut m, ENTRY, &mut rng);
        for k in 0..0x38u32 {
            m.w8(OUTBUF + k, 0xaa);
        }
        let before = m.regions.clone();

        ans.seed.set(case as u64 + 2000);
        ans.n.set(0);
        let mut env = Env { log: Vec::new(), ans: &ans };
        ld::trick_info_build_text(&mut m, &mut env, PADCFG, OUTBUF, ENTRY);

        ans.n.set(0);
        rlog.borrow_mut().clear();
        sync_to_runner(&mut r, &before);
        r.cpu.set_gpr(4, PADCFG as u64);
        r.cpu.set_gpr(5, OUTBUF as u64);
        r.cpu.set_gpr(6, ENTRY as u64);
        r.call(0x1546c8).unwrap();
        assert_eq!(env.log, *rlog.borrow(), "BuildText calls case {case}");
        assert_mem_eq(&mut r, &m, &format!("BuildText case {case}"));
    }

    // Real ELF EAMem_Set + real ELF sprintf on ALL 360 real tricks in DAT_0031e210 (regular + goofy)
    let Some(mut r_real) = Runner::from_env() else { return };
    for &a in &[
        0x1546c8u32, 0x2cc808, 0x2fca28, 0x2ff870, 0x2ff8e8, 0x2ff770, 0x304398, 0x2fb2b4, 0x2fb364, 0x2fb468,
        0x304a4c, 0x2fd260, 0x2fa6b8, 0x2fa4e0, 0x2fa4f0, 0x3047f8, 0x304838, 0x304a10, 0x301d58, 0x2f54e8,
        0x30e4d8, 0x30edb0, 0x30f418,
    ] {
        r_real.allow(a);
    }
    let mut rng2 = Rng(0x1546_c8de_ad01);
    for goofy in [false, true] {
        for rider in 0..12u32 {
            for trick in 0..30u32 {
                let mut m = new_mem(&data, &rodata);
                setup_world(&mut m, &mut rng2, Some(goofy));
                populate_pad_cfg(&mut m, &mut rng2, true);
                for k in 0..0x30u32 {
                    m.w8(OUTBUF + k, 0x55);
                }
                let entry = ld::trick_get_info(rider, trick);
                populate_synthetic_entry(&mut m, entry, &mut rng2);
                let before = m.regions.clone();

                ans.n.set(0);
                let mut env = Env { log: Vec::new(), ans: &ans };
                ld::trick_info_build_text(&mut m, &mut env, PADCFG, OUTBUF, entry);

                sync_to_runner(&mut r_real, &before);
                r_real.cpu.set_gpr(4, PADCFG as u64);
                r_real.cpu.set_gpr(5, OUTBUF as u64);
                r_real.cpu.set_gpr(6, entry as u64);
                r_real.call(0x1546c8).unwrap();
                let mut orig_out = [0u8; 0x30];
                r_real.mem.read(OUTBUF, &mut orig_out).unwrap();
                let ours_out: Vec<u8> = (0..0x30u32).map(|k| m.r8(OUTBUF + k)).collect();
                assert_eq!(
                    ours_out.as_slice(),
                    &orig_out[..],
                    "Real BuildText rider={rider} trick={trick} goofy={goofy}"
                );
            }
        }
    }
}

#[test]
fn game_trick_info_draw_icon_measure_and_draw_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let (data, rodata) = load_base(&mut r);
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0) });
    let rlog = Rc::new(RefCell::new(Vec::new()));
    install_stubs(&mut r, &ans, &rlog);
    let mut rng = Rng(0x154b_1800_0004);

    // vf3_154d60 (DrawIcon): all 12 icon indices × 50 random positions = 600 cases
    for case in 0..600u32 {
        let idx = case % 12;
        let mut m = new_mem(&data, &rodata);
        setup_world(&mut m, &mut rng, None);
        let x0 = rng.f(-200.0, 800.0);
        let y = rng.f(-100.0, 600.0);
        m.w32(XPTR, x0.to_bits());
        let before = m.regions.clone();

        ans.seed.set(case as u64 + 5000);
        ans.n.set(0);
        let mut env = Env { log: Vec::new(), ans: &ans };
        ld::game_trick_info_draw_icon(&mut m, &mut env, INFO, XPTR, idx, F(y));

        ans.n.set(0);
        rlog.borrow_mut().clear();
        sync_to_runner(&mut r, &before);
        r.cpu.set_gpr(4, INFO as u64);
        r.cpu.set_gpr(5, XPTR as u64);
        r.cpu.set_gpr(6, idx as u64);
        r.cpu.set_f(12, y);
        r.call(0x154d60).unwrap();
        assert_eq!(env.log, *rlog.borrow(), "vf3 calls case {case}");
        assert_mem_eq(&mut r, &m, &format!("vf3 case {case}"));
    }

    // vf4_154e20 (MeasureWidth): 1,000 random cases across populated DAT_0031e210 tricks
    for case in 0..1000u32 {
        let mut m = new_mem(&data, &rodata);
        setup_world(&mut m, &mut rng, None);
        populate_pad_cfg(&mut m, &mut rng, true);
        ld::game_trick_info_construct(&mut m, INFO);
        let rider = rng.below(12);
        let trick = rng.below(30);
        populate_synthetic_entry(&mut m, ld::trick_get_info(rider, trick), &mut rng);
        m.w32(INFO, FONT);
        m.w32(INFO + 0x8, rider);
        m.w32(INFO + 0xc, trick);
        m.w32(INFO + 0x20, rng.f(0.25, 2.5).to_bits());
        m.w32(FONT + 0x18, rng.f(0.5, 2.0).to_bits());
        let before = m.regions.clone();

        ans.seed.set(case as u64 + 7000);
        ans.n.set(0);
        let mut env = Env { log: Vec::new(), ans: &ans };
        let ours_f0 = ld::game_trick_info_measure(&mut m, &mut env, INFO, PADCFG, TOP).0.to_bits();

        ans.n.set(0);
        rlog.borrow_mut().clear();
        sync_to_runner(&mut r, &before);
        r.cpu.set_gpr(4, INFO as u64);
        r.cpu.set_gpr(5, PADCFG as u64);
        r.call(0x154e20).unwrap();
        let orig_f0 = r.cpu.f(0).to_bits();
        assert_eq!(ours_f0, orig_f0, "vf4 return bits case {case}");
        assert_eq!(env.log, *rlog.borrow(), "vf4 calls case {case}");
    }

    // vf5_154b18 (Draw): 1,000 cases (half with custom vtable stubs for vf1/vf3, half with real vtable 0x36fa98)
    let (mut saw_icon, mut saw_paren, mut saw_plus, mut saw_other) = (0usize, 0usize, 0usize, 0usize);
    for case in 0..1000u32 {
        let use_real_vt = case >= 500;
        let mut m = new_mem(&data, &rodata);
        setup_world(&mut m, &mut rng, None);
        populate_pad_cfg(&mut m, &mut rng, true);
        ld::game_trick_info_construct(&mut m, INFO);
        let rider = rng.below(12);
        let trick = rng.below(30);
        populate_synthetic_entry(&mut m, ld::trick_get_info(rider, trick), &mut rng);
        m.w32(INFO, FONT);
        m.w32(INFO + 0x8, rider);
        m.w32(INFO + 0xc, trick);
        m.w32(INFO + 0x10, rng.f(0.0, 400.0).to_bits());
        m.w32(INFO + 0x20, rng.f(0.25, 2.5).to_bits());
        m.w32(FONT + 0x18, rng.f(0.5, 2.0).to_bits());
        if !use_real_vt {
            let adj1 = [-8i16, 0, 8][(rng.below(3)) as usize];
            let adj3 = [-4i16, 0, 12][(rng.below(3)) as usize];
            m.w32(INFO + 0x24, CUSTOM_VT);
            m.w16(CUSTOM_VT + 0x10, adj1 as u16);
            m.w32(CUSTOM_VT + 0x14, STUB_VF1);
            m.w16(CUSTOM_VT + 0x20, adj3 as u16);
            m.w32(CUSTOM_VT + 0x24, STUB_VF3);
        }
        let x = rng.f(-100.0, 740.0);
        let y = rng.f(-50.0, 530.0);
        let before = m.regions.clone();

        ans.seed.set(case as u64 + 9000);
        ans.n.set(0);
        struct RealOrStubEnv<'a> {
            inner: Env<'a>,
            real_vt: bool,
        }
        impl TextBoxEnv for RealOrStubEnv<'_> {
            fn call(&mut self, m: &mut Mem, func: u32, args: &[u32], fargs: &[F]) -> u32 {
                if self.real_vt {
                    if func == 0x1550e8 {
                        ld::nothing();
                        return 0;
                    }
                    if func == 0x154d60 {
                        ld::game_trick_info_draw_icon(m, &mut self.inner, args[0], args[1], args[2], fargs[0]);
                        return 0;
                    }
                }
                self.inner.call(m, func, args, fargs)
            }
        }
        let mut env = RealOrStubEnv {
            inner: Env { log: Vec::new(), ans: &ans },
            real_vt: use_real_vt,
        };
        ld::game_trick_info_draw(&mut m, &mut env, INFO, PADCFG, F(x), F(y), TOP);

        ans.n.set(0);
        rlog.borrow_mut().clear();
        sync_to_runner(&mut r, &before);
        r.cpu.set_gpr(4, INFO as u64);
        r.cpu.set_gpr(5, PADCFG as u64);
        r.cpu.set_f(12, x);
        r.cpu.set_f(13, y);
        r.call(0x154b18).unwrap();
        assert_eq!(env.inner.log, *rlog.borrow(), "vf5 calls case {case} real_vt={use_real_vt}");
        for c in &env.inner.log {
            if c.0 == STUB_VF3 || c.0 == ld::SPRITE_DRAW {
                saw_icon += 1;
            } else if c.0 == ld::FONT_PRINTF_SHADOWED {
                match c.1.get(5).copied() {
                    Some(0x28 | 0x29) => saw_paren += 1,
                    Some(0x2b) => saw_plus += 1,
                    _ => saw_other += 1,
                }
            }
        }
        assert_mem_eq(&mut r, &m, &format!("vf5 case {case} real_vt={use_real_vt}"));
    }
    assert!(
        saw_icon > 500 && saw_paren > 50 && saw_plus > 50 && saw_other > 500,
        "coverage: icon={saw_icon} paren={saw_paren} plus={saw_plus} other={saw_other}"
    );
}
