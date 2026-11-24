//! `tricky_game::aiobj` (board row G4k) against the original in the function runner (TRICKY_ELF; skipped without
//! it): the race object's vf10 0x1108f0 (reset), vf11 0x110bf8 (the riders made), vf13 0x110fa0 (tear-down) and vf16
//! 0x111560 (start of a race), (row G4k2) its constructor 0x114c00 and destructor 0x114d30, and (G4k3) vf14 0x111108
//! (the race reset) and vf15 0x1114d0 (the restart), and (G4k4) the start-race handler, the countdown's exit, the
//! pre- and post-AI nodes' ticks and destructors, and (G4k5) the mode machine: vf1 0x1126f0, vf9 0x1125a0 and
//! `Race_CreateHandlers` 0x1125c8 on every mode, the handlers stood in for, and (G4k6) the race handler's Enter /
//! Update, the riders' streaming and the race's small helpers. Every case builds a random race object, career roster (0..6 riders, computers and
//! players mixed), app and controllers in the arena; one function then runs on both sides. Every callee is a
//! stand-in answering from the case's seed: the controllers' constructors answer their object and give it a boarder
//! (or none, at times) at +0; the list's first node is a random word or none. The calls are compared with their
//! arguments, and the memory after (the heap included).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use r5900::Runner;
use tricky_game::aiobj as ao;
use tricky_game::fe3dstate::{FeEnv, ALLOC, FILL, GAPP};
use tricky_game::pf::Mem;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        // splitmix64: successive answers unrelated
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

const DATA: (u32, u32) = (0x337c00, 0x400);
const CAREER: (u32, u32) = (0x32b000, 0x4000);
const ARENA: u32 = 0x0100_0000;
const SIZE: u32 = 0x8000;
const APPOBJ: u32 = ARENA + 0x100;
const AI: u32 = ARENA + 0x1000;
/// the stand-in tables: one for each place an object keeps one (+0, +8), each with its own adjust
fn vt2(k: u32) -> u32 {
    ARENA + 0x3000 + 0x100 * k
}
/// the arena's objects (boarders, controllers, the +0x64 object)
fn obj(k: u32) -> u32 {
    ARENA + 0x4000 + 0x100 * k
}
/// a boarder whose +0x5ae0 lands in the arena (vf16 reads only that of it)
fn far_boarder(k: u32) -> u32 {
    ARENA + 0x6000 + 0x80 * k - 0x5ae0
}
const HEAP: (u32, u32) = (0x0110_0000, 0x50000);
fn slot(off: u32) -> u32 {
    0x0182_0000 + off
}
/// the objects' virtuals: the boarder's destructor +8, the +0x64 object's +0x10, the race's +0x60 and +0x78
const SLOTS: [u32; 9] = [8, 0x10, 0x60, 0x78, 0xa0, 0x48, 0x50, 0x68, 0x18];
const FUNCS: [u32; 27] = [
    0x1108f0, 0x110bf8, 0x110fa0, 0x111560, 0x114c00, 0x114d30, 0x111108, 0x1114d0, 0x113478, 0x1134a0, 0x1134d0,
    0x113738, 0x1146b0, 0x114730, 0x114b08, 0x114a68, 0x1126f0, 0x1125a0, 0x1125c8, 0x1117f0, 0x1128b8, 0x1148d0,
    0x1148e8, 0x114f70, 0x113740, 0x113820, 0x112920,
];
/// the courses' records (read from the ELF) and the speech system's pointer
const COURSES: (u32, u32) = (ao::COURSE_TABLE, 13 * 0x1c);
const SPEECH_PTR: (u32, u32) = (ao::SPEECH, 4);
/// the riders `Race_GetRider` answers (big objects: +0x46b0 is read)
const RIDERS: (u32, u32) = (0x0170_0000, 0x4800);
/// our side's stack words (the original's are on its stack): not compared
const SCRATCH: u32 = 0x0190_0000;
const CTORS: [u32; 2] = [ao::COMPUTER_CTOR, ao::PLAYER_CTOR];

#[derive(Clone, Debug, PartialEq)]
struct Call(u32, Vec<u32>);

fn arity(f: u32) -> usize {
    use ao::*;
    match f {
        LOADER_MANAGERS | REQUEST_RIDER => 4,
        OPEN_BIG => 5,
        LOAD_RIDER | RESTART_POINT => 3,
        RESET_SKIN | GET_PATH | FOCUS_RIDER | SCENE_SETUP => 2,
        HIDE_OVERLAYS | SEED_GAME | MAIN_PATH | GET_CAMERA | IS_TUTORIAL | RESET_RENDERS | SCENE_ANIMS => 1,
        DEFAULT_REQUEST | VALIDATE_ALL => 0,
        GET_RIDER => 2,
        BDR_REQUEST => 4,
        HUD_SHOW | HUD_HIDE | STORE_ATTITUDES | MUSIC_STATE => 3,
        SLOT_READY | BIND_TEXTURES | FREE_SLOT | FINISH | PAIR_DISTANCES | PLACINGS => 2,
        ANIM_POLL | SPEECH_READY | RESET_RENDER | LAUNCH | DRAIN_METER | FINISHED | TIME_UP => 1,
        FLUSH_CLIPS | ANIM_LAYERS | UPDATE_POSE | BROADPHASE | LOOK_BACK | REPLAY_MODE | HAS_START_GATE | RUN_SCRIPT => 1,
        f if f == slot(0x50) => 2,
        NODE_CTOR | NODE_DTOR | ARCHIVE_DTOR | LOADER_DTOR => 2,
        ARCHIVE_CTOR | LOADER_CTOR => 1,
        ALLOC | FILL | COMPUTER_CTOR | PLAYER_CTOR | RIDER_INFO | ARCHIVE_OPEN | LIST_ADD => 3,
        f if f == slot(0x78) => 3,
        SKIN_MEM | INPUT_DEFAULTS | VIBRATION | LIST_DELETE_ALL | LIST_FIRST | LIST_UNLINK => 2,
        f if f == slot(8) || f == slot(0x10) || f == slot(0x18) => 2,
        RAND_FX => 0,
        _ => 1,
    }
}

struct Answers {
    seed: Cell<u64>,
    n: Cell<u64>,
    heap: Cell<u32>,
}
impl Answers {
    fn next(&self) -> u32 {
        self.n.set(self.n.get() + 1);
        Rng(self.seed.get() ^ self.n.get().wrapping_mul(0x1234_5678_9abc_def1)).next()
    }
}

/// A stand-in's answer, its logged call and the words it writes
fn standin(f: u32, w: &[u32], ans: &Answers) -> (u32, Call, Vec<(u32, u32)>) {
    // (the race's +0xa0 is called with -1; a rider's with nothing)
    let n = if f == slot(0xa0) && w[0] == AI + 4 { 2 } else { arity(f) };
    let log = w[..n].to_vec();
    let r = ans.next();
    let mut g = Rng(r as u64 | 1);
    let mut writes = Vec::new();
    let answer = match f {
        ALLOC => {
            let at = ans.heap.get();
            ans.heap.set(at + ((w[1] + 0xf) & !0xf));
            assert!(ans.heap.get() <= HEAP.0 + HEAP.1, "the heap ran out");
            at
        }
        f if CTORS.contains(&f) => {
            writes.push((w[0], if g.below(4) == 0 { 0 } else { obj(g.below(16)) }));
            w[0]
        }
        ao::LIST_FIRST => g.pick(&[0, 0, r & !0x0100_0000]),
        ao::GET_CAMERA => g.pick(&[0, r & !0x0100_0000]),
        ao::IS_TUTORIAL | ao::HAS_START_GATE | ao::SLOT_READY => g.below(2),
        ao::ANIM_POLL | ao::SPEECH_READY | ao::FINISHED => g.pick(&[0, 1, 1, 1]),
        ao::BDR_REQUEST => { let x = g.below(4); g.pick(&[u32::MAX, x]) },
        ao::GET_RIDER => RIDERS.0 + RIDERS.1 * w[1],
        ao::DEFAULT_REQUEST => {
            writes.push((w[0], g.next()));
            0
        }
        ao::SCENE_ANIMS => {
            writes.push((w[1], g.next()));
            r & !0x0100_0000
        }
        _ => r & !0x0100_0000,
    };
    (answer, Call(f, log), writes)
}

struct Env<'a> {
    log: Vec<Call>,
    ans: &'a Answers,
}
impl FeEnv for Env<'_> {
    fn call(&mut self, m: &mut Mem, f: u32, args: &[u32]) -> u32 {
        let mut w: Vec<u32> = args.to_vec();
        w.resize(8, 0);
        let (a, c, writes) = standin(f, &w, self.ans);
        for (at, v) in writes {
            m.w32(at, v);
        }
        self.log.push(c);
        assert!(self.log.len() < 100_000, "the port calls on and on");
        a
    }
}

fn ours(m: &mut Mem, e: &mut dyn FeEnv, f: u32, a1: u32, a2: u32) -> u32 {
    match f {
        0x114c00 => return ao::construct(m, e, AI, a1),
        0x114d30 => ao::destruct(m, e, AI, a1),
        0x111108 => ao::race_reset(m, e, AI, a1, a2, SCRATCH),
        0x1114d0 => ao::restart(m, e, AI),
        0x113478 => ao::start_race_enter(m, e),
        0x1134a0 => ao::start_race_update(m, e, AI),
        0x1134d0 => ao::start_race_exit(m, e, AI),
        0x113738 => ao::countdown_exit(),
        0x1146b0 => ao::pre_ai_tick(m, e),
        0x114730 => ao::post_ai_tick(m, e),
        0x114b08 => ao::ai_node_destruct(m, e, AI, ao::PRE_AI_VT, a1),
        0x114a68 => ao::ai_node_destruct(m, e, AI, ao::POST_AI_VT, a1),
        0x1126f0 => ao::tick(m, e, AI),
        0x1125a0 => ao::set_mode(m, AI, a1),
        0x1125c8 => ao::create_handlers(m, AI, a1),
        0x1117f0 => ao::go_to_end_lesson(m, e, AI),
        0x1128b8 => ao::reset_rider_renders(m, e, AI),
        0x1148d0 => return ao::is_tutorial(m) as u32,
        0x1148e8 => return ao::find_rider_by_player(m, AI, a1),
        0x114f70 => ao::store_circuit_attitudes(m, e, ao::CAREER_AT),
        0x113740 => ao::race_enter(m, e, AI),
        0x113820 => ao::race_update(m, e, ARENA + 0x300, AI),
        0x112920 => ao::update_rider_streaming(m, e, AI, SCRATCH),
        0x1108f0 => ao::reset(m, e, AI),
        0x110bf8 => ao::make_riders(m, e, AI),
        0x110fa0 => ao::tear_down(m, e, AI),
        0x111560 => ao::start_race(m, e, AI),
        _ => unreachable!(),
    }
    0
}

#[test]
fn race_object_glue_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    // (the floats as IEEE, the port's rule until F4c: the clock's int-to-float and its steps)
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    for f in FUNCS {
        r.allow(f);
    }
    let log: Rc<RefCell<Vec<Call>>> = Rc::default();
    let testing: Rc<Cell<u32>> = Rc::default();
    let ans = Rc::new(Answers { seed: Cell::new(0), n: Cell::new(0), heap: Cell::new(HEAP.0) });
    use ao::*;
    let mut stubs = vec![
        ALLOC, FILL, COMPUTER_CTOR, PLAYER_CTOR, SKIN_MEM, RIDER_INFO, GET_DEVICE, INPUT_DEFAULTS, VIBRATION,
        ARCHIVE_OPEN, ARCHIVE_CLOSE, LOADER_MANAGERS, DELETE, ANIM_RESET, LIST_DELETE_ALL, LIST_FIRST, LIST_UNLINK,
        LIST_ADD, RAND_FX, NODE_CTOR, NODE_DTOR, ARCHIVE_CTOR, ARCHIVE_DTOR, LOADER_CTOR, LOADER_DTOR,
        HIDE_OVERLAYS, RESET_SKIN, LOAD_RIDER, DEFAULT_REQUEST, REQUEST_RIDER, SEED_GAME, MAIN_PATH, GET_PATH,
        RESTART_POINT, GET_CAMERA, FOCUS_RIDER, IS_TUTORIAL, VALIDATE_ALL, OPEN_BIG, SCENE_SETUP, SCENE_ANIMS,
        RESET_RENDERS, REPLAY_MODE, HAS_START_GATE, RUN_SCRIPT, GET_RIDER, FLUSH_CLIPS, ANIM_LAYERS, UPDATE_POSE,
        BROADPHASE, LOOK_BACK, RESET_RENDER, LAUNCH, DRAIN_METER, FINISH, FINISHED, PAIR_DISTANCES, PLACINGS, TIME_UP,
        MUSIC_STATE, STORE_ATTITUDES, BDR_REQUEST, SLOT_READY, BIND_TEXTURES, FREE_SLOT, ANIM_POLL, SPEECH_READY,
        HUD_HIDE, HUD_SHOW,
    ];
    stubs.extend(SLOTS.map(slot));
    for f in stubs {
        let (l, a) = (log.clone(), ans.clone());
        let t = testing.clone();
        r.hook(f, move |c, mem| {
            // (a stood-in function under test itself runs)
            if t.get() == f {
                return false;
            }
            let mut w: Vec<u32> = (4..12).map(|k| c.gpr(k) as u32).collect();
            if f == ao::MUSIC_STATE {
                w[2] = c.f(12).to_bits();
            }
            let (v, call, writes) = standin(f, &w, &a);
            for (at, x) in writes {
                mem.write_u32(at, x);
            }
            l.borrow_mut().push(call);
            assert!(l.borrow().len() < 100_000, "the original calls on and on");
            c.set_gpr(2, v as i32 as i64 as u64);
            true
        });
    }
    let mut data = vec![0u8; DATA.1 as usize];
    r.mem.read(DATA.0, &mut data).unwrap();
    let mut courses = vec![0u8; COURSES.1 as usize];
    r.mem.read(COURSES.0, &mut courses).unwrap();
    let mut g = Rng(0x64b0_0000_0000_0001);
    let mut riders = [0u32; 7];
    for case in 0..27000u32 {
        let func = FUNCS[(case % 27) as usize];
        testing.set(func);
        let heap = if func == 0x110bf8 { HEAP.1 } else { 0x100 };
        let mut regions = vec![
            (DATA.0, data.clone()),
            (CAREER.0, vec![0; CAREER.1 as usize]),
            (ARENA, vec![0; SIZE as usize]),
            (HEAP.0, vec![0xa5; heap as usize]),
            (SCRATCH, vec![0; 8]),
            // (the courses' records as the game has them, or any times)
            (COURSES.0, if g.below(2) == 0 { courses.clone() } else { (0..COURSES.1).map(|_| g.next() as u8).collect() }),
            (SPEECH_PTR.0, (g.next() & !0x0100_0000).to_le_bytes().to_vec()),
        ];
        let ticks = matches!(func, 0x1146b0 | 0x114730);
        if ticks {
            regions.push((RIDERS.0, vec![0; 6 * RIDERS.1 as usize]));
        }
        let mut m = Mem { regions };
        let riders_n = if ticks { 6 * RIDERS.1 } else { 0 };
        for (at, n) in [(ARENA, SIZE), (CAREER.0, CAREER.1), (RIDERS.0, riders_n)] {
            for k in 0..n / 4 {
                m.w32(at + 4 * k, g.next() & !0x0100_0000);
            }
        }
        m.w32(GAPP, APPOBJ);
        m.w32(APPOBJ + 0x730, ARENA + 0x200);
        // (the game's race object is the one under test; its +0x40 replay at times)
        m.w32(ARENA + 0x200 + 0x1c, AI);
        m.w32(ARENA + 0x200 + 0x40, g.pick(&[0, ARENA + 0x40]));
        if ticks {
            m.w32(AI + 0x88, g.below(7));
            for k in 0..6 {
                let o = g.pick(&[0, 1]);
                m.w32(RIDERS.0 + RIDERS.1 * k + 0x46b0, o);
            }
        }
        for k in 0..2 {
            for off in SLOTS {
                m.w16(vt2(k) + off, 4 * k as u16);
                m.w32(vt2(k) + off + 4, slot(off));
            }
        }
        for k in 0..16 {
            m.w32(obj(k), vt2(0));
            m.w32(obj(k) + 8, vt2(1));
        }
        m.w32(AI + 8, vt2(1));
        // (a race has up to six riders, up to four of them players: the lists' sizes)
        let n = g.below(7);
        riders[n as usize] += 1;
        m.w32(ao::ROSTER_COUNT, n);
        let players = g.below(n.min(4) + 1);
        let mut comps = 0;
        let mut left = (players, n - players);
        for k in 0..n {
            let c = left.0 == 0 || (left.1 != 0 && g.below(2) == 0);
            if c { left.1 -= 1 } else { left.0 -= 1 }
            m.w8(ao::ROSTER + ao::ENTRY_SIZE * k + 0x65, if c { 0x80 | g.below(0x80) as u8 } else { g.below(0x80) as u8 });
            comps += c as u32;
        }
        match func {
            0x110fa0 => {
                m.w32(AI + 0x64, g.pick(&[0, obj(15)]));
                for k in 0..4 {
                    let o = g.next() & !0x0100_0000;
                    m.w32(AI + 0x6c + 4 * k, g.pick(&[0, o]));
                }
                m.w32(AI + 0x88, g.below(7));
                for k in 0..4 {
                    m.w32(AI + 0xc4 + 4 * k, g.pick(&[0, obj(k)]));
                }
                // (and two more past the four, read when the count says so)
                for k in 4..6 {
                    m.w32(AI + 0xc4 + 4 * k, g.pick(&[0, obj(k)]));
                }
            }
            0x1128b8 | 0x113740 | 0x113820 | 0x112920 => {
                m.w32(AI + 0x88, g.below(7));
                for k in 0..6 {
                    m.w32(AI + 0xc4 + 4 * k, obj(k));
                    m.w32(AI + 0x8c + 4 * k, obj(k + 6));
                    let late = g.next() & !0x0100_0000;
                    m.w32(obj(k) + 0x418, g.pick(&[0, u32::MAX, 1, late]));
                }
                m.w32(AI + 0x7c, { let x = g.below(5); g.pick(&[0, 0xffff_fffe, x]) });
                m.w32(ao::GAME_MODE, { let x = g.below(12); g.pick(&[3, 5, 6, x]) });
                m.w32(ao::GAME_COURSE, g.below(13));
                let clock = [0.5f32 / 60.0, 1.0 / 60.0, 2.0 / 60.0, 10.0, -1.0, 0.0, 95.5];
                m.w32(AI + 0x14, g.pick(&clock).to_bits());
                m.w32(ARENA + 0x200 + 0x34, g.pick(&[0, 0, 0x1234]));
                m.w32(AI + 0x4f8, g.pick(&[0, 1, 1, 1]));
                m.w32(AI + 0x4fc, { let x = g.below(6); g.pick(&[0xffff_fffe, 0xffff_fffe, u32::MAX, x]) });
                m.w32(AI + 0x500, { let x = g.below(4); g.pick(&[u32::MAX, x]) });
            }
            0x1148d0 => m.w32(ao::GAME_MODE, { let x = g.below(12); g.pick(&[6, x]) }),
            0x1148e8 => {
                m.w32(AI + 0x7c, { let x = g.below(5); g.pick(&[0, 0xffff_fffe, x]) });
                for k in 0..4 {
                    let (c, b, info) = (obj(k), far_boarder(k), ARENA + 0x7000 + 0x80 * k);
                    m.w32(AI + 0x8c + 4 * k, c);
                    m.w32(c, b);
                    m.w32(b + 0x464, info);
                    m.w8(info + 0x65, g.pick(&[0, 1, 2, 3, 0x80, 0xff]));
                }
            }
            0x1126f0 | 0x1125a0 | 0x1125c8 => {
                // (the modes 0..12, the handlers: the race's own or none before)
                let cur = g.below(13);
                m.w32(AI + 0x1c, cur);
                let other = g.below(13);
                m.w32(AI + 0x20, g.pick(&[cur, other]));
                let old = obj(g.below(16));
                m.w32(AI + 0x60, g.pick(&[0, old]));
                m.w32(AI + 0x64, obj(g.below(16)));
            }
            0x111108 => {
                m.w32(AI + 0x88, g.below(7));
                for k in 0..6 {
                    m.w32(AI + 0xc4 + 4 * k, obj(k));
                }
                for at in [0xdc, 0xe0] {
                    m.w32(AI + at, g.below(2));
                }
            }
            0x111560 => {
                // (each rider's controller, its boarder, the boarder's +0x5ae0 object or none)
                for (j, k) in (0..comps).chain(0..n - comps).enumerate() {
                    let list = if j < comps as usize { 0x9c } else { 0x8c };
                    let c = obj(j as u32);
                    let b = far_boarder(j as u32);
                    m.w32(AI + list + 4 * k, c);
                    m.w32(c, b);
                    let o = g.next() & !0x0100_0000;
                    m.w32(b + 0x5ae0, g.pick(&[0, o]));
                }
            }
            _ => {}
        }
        for (at, b) in m.regions.iter().filter(|(at, _)| *at != SCRATCH) {
            r.mem.write(*at, b).unwrap();
        }
        ans.seed.set(((g.next() as u64) << 32) | g.next() as u64);
        ans.n.set(0);
        ans.heap.set(HEAP.0);
        let (a1, a2) = match func {
            0x111108 => (g.below(2), g.below(2)),
            0x1125a0 | 0x1125c8 => {
                let v = [m.r32(AI + 0x1c), g.below(13), g.next()];
                (g.pick(&v), 0)
            }
            0x1148e8 => (g.pick(&[0, 1, 2, 3, u32::MAX, 0x80]), 0),
            _ => (g.next(), 0),
        };
        // (the handlers: a0 the handler, a1 the race)
        let handler = matches!(func, 0x113478 | 0x1134a0 | 0x1134d0 | 0x113738 | 0x113740 | 0x113820);
        let mut env = Env { log: Vec::new(), ans: &ans };
        let v = ours(&mut m, &mut env, func, a1, a2);
        ans.n.set(0);
        ans.heap.set(HEAP.0);
        log.borrow_mut().clear();
        let a0 = match func {
            _ if handler => ARENA + 0x300,
            0x114f70 => ao::CAREER_AT,
            _ => AI,
        };
        r.cpu.set_gpr(4, a0 as i32 as i64 as u64);
        r.cpu.set_gpr(5, if handler { AI } else { a1 } as i32 as i64 as u64);
        r.cpu.set_gpr(6, a2 as i32 as i64 as u64);
        r.call(func).unwrap_or_else(|e| panic!("case {case} {func:#x}: {e}"));
        assert_eq!(*log.borrow(), env.log, "case {case} {func:#x}: the calls");
        if matches!(func, 0x114c00 | 0x1148d0 | 0x1148e8) {
            assert_eq!(r.cpu.gpr(2) as u32, v, "case {case}: the answer");
        }
        for (at, b) in m.regions.iter().filter(|(at, _)| *at != SCRATCH) {
            let mut th = vec![0u8; b.len()];
            r.mem.read(*at, &mut th).unwrap();
            if &th != b {
                let i = th.iter().zip(b).position(|(x, y)| x != y).unwrap() & !3;
                let w = |v: &[u8]| u32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                panic!("case {case} {func:#x}: memory at {:#x}: theirs {:#x} ours {:#x}", at + i as u32, w(&th), w(b));
            }
        }
    }
    assert!(riders.iter().all(|&n| n > 100), "every roster size: {riders:?}");
}

#[test]
fn default_virtuals_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    // (the race object's table, 0x364738: vf2..vf6)
    let slots: Vec<u32> = (3..8).map(|k| r.mem.read_u32(ao::AI_VT + 8 * k + 4)).collect();
    assert_eq!(slots, [0x17df00, 0x17def8, 0x17def0, 0x17dee8, 0x17dee0]);
    for (k, f) in slots.into_iter().enumerate() {
        r.allow(f);
        r.cpu.set_gpr(2, 0x1234_5678);
        r.call(f).unwrap();
        if k > 0 {
            assert_eq!(r.cpu.gpr(2) as u32, ao::default_virtual(k as u32 + 2), "vf{}", k + 2);
        }
    }
}
