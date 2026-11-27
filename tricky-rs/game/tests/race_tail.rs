//! Runner checks for board row G11f (last 15 `not started` `race` functions):
//! `Race_FindLineAhead` 0x11a020, `Race_CreateBoarders` 0x150e38, `Career_Destruct` 0x161760,
//! `Career_InitShortRecord` 0x165660, `Career_SetupSoloRider` 0x165688, `StaticDtor_Career` 0x167170,
//! `PreRaceSel_Destruct` 0x168688, `PreRaceSel_GetPicks` 0x168b80, `RaceScene_Destruct` 0x169668,
//! `StaticDtor_RaceScene` 0x169d28, `Circuit_Destruct` 0x16a978, `Circuit_ResetHeat` 0x16bca8,
//! `Circuit_SetupRider` 0x16cfd8, `Circuit_OnEventOver` 0x16d0a8, `Circuit_SetupRace` 0x16d230.
//! (TRICKY_ELF; skipped without it).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::aiobj::{
    career_destruct, career_init_short_record, career_setup_solo_rider, circuit_destruct, circuit_on_event_over,
    circuit_reset_heat, circuit_setup_race, circuit_setup_rider, player_construct, pre_race_sel_destruct,
    pre_race_sel_get_picks, race_find_line_ahead, race_scene_destruct, static_dtor_career, static_dtor_race_scene,
    BOARDER_CTOR, BOARDER_FIND_NEAREST_LINE, BOARDER_HANDLER_CTOR, CAREER_FINALIZE_ROSTER, CAREER_SETUP_AI_RIDER,
    CAREER_SETUP_PLAYER_RIDER, CIRCUIT_GET_MEDAL_THRESHOLDS, DELETE, G_REL_TABLE_PTR, PLAYER_VBASE_VT,
    PRERACE_SEL_CHOOSE, PRERACE_SEL_CLEAR, RACE_GET_RIDER, REPLAY_ADD_INPUT_STREAM, REPLAY_ADD_PIN, RIVALS_INIT_PAIRS,
};
use tricky_game::fe3dstate::{FeEnv, GAPP};
use tricky_game::pf::Mem;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        let u = (self.next() as f32) / (u32::MAX as f32);
        lo + u * (hi - lo)
    }
}

type Log = Rc<RefCell<Vec<(u32, Vec<u32>)>>>;

struct RecEnv {
    log: Log,
    rider_ret: u32,
    line_ret: u32,
    inp_ret: u32,
    pin_ret: u32,
    medal_idx: u32,
}

impl FeEnv for RecEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut v = args.to_vec();
        if func == REPLAY_ADD_INPUT_STREAM || func == BOARDER_HANDLER_CTOR {
            // For player_construct, log the vtable pointer at *b when checking post-install state
            if func == REPLAY_ADD_INPUT_STREAM {
                self.log.borrow_mut().push((func, v));
                return self.inp_ret;
            }
        }
        match func {
            RACE_GET_RIDER => {
                self.log.borrow_mut().push((func, v));
                self.rider_ret
            }
            BOARDER_FIND_NEAREST_LINE => {
                self.log.borrow_mut().push((func, v));
                self.line_ret
            }
            REPLAY_ADD_PIN => {
                self.log.borrow_mut().push((func, v));
                self.pin_ret
            }
            CIRCUIT_GET_MEDAL_THRESHOLDS => {
                m.w32(args[1], self.medal_idx);
                v.push(self.medal_idx);
                self.log.borrow_mut().push((func, v));
                0
            }
            _ => {
                self.log.borrow_mut().push((func, v));
                0
            }
        }
    }
}

#[test]
fn player_construct_and_find_line_ahead_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    let base = r.mem.alloc(0x2000);
    let (app, game, ai, rider, player) = (base, base + 0x800, base + 0x900, base + 0xa00, base + 0xf00);
    let log: Log = Rc::new(RefCell::new(vec![]));
    let line_ret: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let inp_ret: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let pin_ret: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

    {
        let l2 = log.clone();
        r.stub(RACE_GET_RIDER, move |c, _| {
            l2.borrow_mut().push((RACE_GET_RIDER, vec![c.gpr(4) as u32]));
            c.set_gpr(2, rider as u64);
        });
    }
    {
        let l2 = log.clone();
        let lr = line_ret.clone();
        r.stub(BOARDER_FIND_NEAREST_LINE, move |c, _| {
            l2.borrow_mut().push((
                BOARDER_FIND_NEAREST_LINE,
                vec![c.gpr(4) as u32, c.gpr(5) as u32, c.f(12).to_bits(), c.f(13).to_bits()],
            ));
            c.set_gpr(2, *lr.borrow() as u64);
        });
    }
    {
        let l2 = log.clone();
        r.stub(BOARDER_CTOR, move |c, _| {
            l2.borrow_mut().push((BOARDER_CTOR, vec![c.gpr(4) as u32]));
        });
    }
    {
        let l2 = log.clone();
        r.stub(BOARDER_HANDLER_CTOR, move |c, _| {
            l2.borrow_mut().push((
                BOARDER_HANDLER_CTOR,
                vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32],
            ));
        });
    }
    {
        let l2 = log.clone();
        let ir = inp_ret.clone();
        r.stub(REPLAY_ADD_INPUT_STREAM, move |c, _| {
            l2.borrow_mut().push((REPLAY_ADD_INPUT_STREAM, vec![c.gpr(4) as u32]));
            c.set_gpr(2, *ir.borrow() as u64);
        });
    }
    {
        let l2 = log.clone();
        let pr = pin_ret.clone();
        r.stub(REPLAY_ADD_PIN, move |c, _| {
            l2.borrow_mut().push((REPLAY_ADD_PIN, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, *pr.borrow() as u64);
        });
    }

    let mut vt_bytes = vec![0u8; 0xb0];
    r.mem.read(PLAYER_VBASE_VT, &mut vt_bytes).unwrap();

    let mut g = Rng(0x150e_38aa_11a0_0001);
    for case in 0..1_000u32 {
        let mut init = vec![0u8; 0x2000];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(base, &init).unwrap();
        r.mem.write_u32(GAPP, app);
        r.mem.write_u32(app + 0x730, game);
        r.mem.write_u32(game + 0x1c, ai);
        let replay = if g.below(2) == 0 { 0 } else { g.next() | 4 };
        r.mem.write_u32(game + 0x40, replay);
        r.mem.write_u32(ai + 0x7c, g.below(8));

        let b_alt = base + 0x1900 + ((g.below(16)) << 2);
        r.mem.write_u32(player, b_alt);

        let dist = g.f(-500.0, 500.0);
        let delta = g.f(-500.0, 500.0);
        let f13 = g.f(-100.0, 100.0);
        r.mem.write_u32(rider + 0x374, dist.to_bits());

        let lr = g.next();
        let ir = g.next();
        let pr = g.next();
        *line_ret.borrow_mut() = lr;
        *inp_ret.borrow_mut() = ir;
        *pin_ret.borrow_mut() = pr;

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x2000];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut gb = vec![0u8; 4];
        r.mem.read(GAPP, &mut gb).unwrap();
        m.regions.push((GAPP, gb));
        m.regions.push((PLAYER_VBASE_VT, vt_bytes.clone()));

        let which = case % 2;
        let out = g.next();
        let in_charge = g.below(2);
        let rider_idx = (g.next() & 0xffff) as i16;

        log.borrow_mut().clear();
        let ret_theirs = if which == 0 {
            r.cpu.set_gpr(4, out as u64);
            r.cpu.set_f(12, delta);
            r.cpu.set_f(13, f13);
            r.call(0x11a020).unwrap_or_else(|e| panic!("FindLineAhead {case}: {e}"));
            r.cpu.gpr(2) as u32
        } else {
            r.cpu.set_gpr(4, player as u64);
            r.cpu.set_gpr(5, in_charge as u64);
            r.cpu.set_gpr(6, rider_idx as i32 as i64 as u64);
            r.call(0x150e38).unwrap_or_else(|e| panic!("Race_CreateBoarders {case}: {e}"));
            r.cpu.gpr(2) as u32
        };
        let sp = r.cpu.gpr(29) as u32;
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x2000];
        r.mem.read(base, &mut theirs_mem).unwrap();
        // Also compare the 0xb0-byte stack vtable copy when in_charge == 0!
        let mut theirs_vt_copy = vec![0u8; 0xb0];
        if which == 1 && in_charge == 0 {
            r.mem.read(sp.wrapping_sub(0x120), &mut theirs_vt_copy).unwrap();
        }

        m.regions.push((sp.wrapping_sub(0x140), vec![0u8; 0x140]));
        let mut env = RecEnv {
            log: log.clone(),
            rider_ret: rider,
            line_ret: lr,
            inp_ret: ir,
            pin_ret: pr,
            medal_idx: 0,
        };
        let ret_mine = if which == 0 {
            race_find_line_ahead(&mut m, &mut env, out, delta, f13)
        } else {
            player_construct(&mut m, &mut env, player, in_charge, rider_idx, sp)
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
        if which == 1 && in_charge == 0 {
            let mine_vt_copy: Vec<u8> = (0..0xb0u32).map(|k| m.r8(sp.wrapping_sub(0x120) + k)).collect();
            assert_eq!(theirs_vt_copy, mine_vt_copy, "case {case}: stack vtable copy mismatch");
        }
    }
}

#[test]
fn career_prerace_scene_and_circuit_tail_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.allow(0x161760);
    r.allow(0x168688);
    r.allow(0x169668);
    r.allow(0x16a978);
    r.allow(0x16bb98);

    let base = r.mem.alloc(0x5000);
    let (career, circuit, roster, ps, ps_rec, src, dst) = (
        base,
        base + 0x3a00,
        base + 0x3b00,
        base + 0x4300,
        base + 0x4400,
        base + 0x4500,
        base + 0x4600,
    );
    let log: Log = Rc::new(RefCell::new(vec![]));
    let medal_idx: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

    for (f, arity) in [
        (DELETE, 1usize),
        (CAREER_SETUP_PLAYER_RIDER, 3),
        (CAREER_SETUP_AI_RIDER, 4),
        (RIVALS_INIT_PAIRS, 4),
        (PRERACE_SEL_CLEAR, 1),
        (PRERACE_SEL_CHOOSE, 5),
        (CAREER_FINALIZE_ROSTER, 2),
    ] {
        let l2 = log.clone();
        r.stub(f, move |c, _| {
            let v: Vec<u32> = (0..arity).map(|i| c.gpr(4 + i) as u32).collect();
            l2.borrow_mut().push((f, v));
        });
    }
    {
        let l2 = log.clone();
        let mi = medal_idx.clone();
        r.stub(CIRCUIT_GET_MEDAL_THRESHOLDS, move |c, mem| {
            let idx = *mi.borrow();
            let a0 = c.gpr(4) as u32;
            let a1 = c.gpr(5) as u32;
            mem.write_u32(a1, idx);
            l2.borrow_mut().push((CIRCUIT_GET_MEDAL_THRESHOLDS, vec![a0, a1, idx]));
        });
    }

    let mut g = Rng(0x1656_88dd_c1cc_0002);
    for case in 0..1_300u32 {
        let mut init = vec![0u8; 0x5000];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(base, &init).unwrap();
        r.mem.write_u32(G_REL_TABLE_PTR, g.next());

        // Configure player slots in career (+0x30fc, 4 * 0x44)
        for s in 0..4u32 {
            let ch: i8 = if g.below(3) == 0 { g.below(12) as i8 } else { -1 };
            r.mem.write_u8(career + 0x30fc + s * 0x44, ch as u8);
        }
        // Configure circuit object
        r.mem.write_u32(circuit + 4, career);
        r.mem.write_u32(circuit + 8, roster);
        r.mem.write_u32(circuit + 0x10, ps);
        r.mem.write_u32(circuit + 0x14, g.below(5));
        r.mem.write_u32(circuit + 0x18, g.next());
        let p_char = g.below(12);
        r.mem.write_u8(ps, p_char as u8);
        r.mem.write_u32(ps + 0x14, ps_rec);
        r.mem.write_u32(ps + 0x18, base + 0x4700);
        r.mem.write_u32(ps_rec, p_char);
        r.mem.write_u32(roster + 0x31c, g.below(12));
        r.mem.write_u32(roster + 0x320, g.below(8));

        let mi = g.below(16);
        *medal_idx.borrow_mut() = mi;

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x5000];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut rel_b = vec![0u8; 4];
        r.mem.read(G_REL_TABLE_PTR, &mut rel_b).unwrap();
        m.regions.push((G_REL_TABLE_PTR, rel_b));

        let which = case % 13;
        let how = g.below(4);
        let obj = g.next();
        let slot = g.below(6);
        let rider = if g.below(2) == 0 { p_char } else { g.below(12) };

        log.borrow_mut().clear();
        let ret_theirs = match which {
            0 => {
                r.cpu.set_gpr(4, career as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x161760).unwrap_or_else(|e| panic!("Career_Destruct {case}: {e}"));
                0
            }
            1 => {
                r.cpu.set_gpr(4, career as u64);
                r.cpu.set_gpr(5, src as u64);
                r.cpu.set_gpr(6, dst as u64);
                r.call(0x165660).unwrap_or_else(|e| panic!("Career_InitShortRecord {case}: {e}"));
                0
            }
            2 => {
                r.cpu.set_gpr(4, career as u64);
                r.call(0x165688).unwrap_or_else(|e| panic!("Career_SetupSoloRider {case}: {e}"));
                0
            }
            3 => {
                r.call(0x167170).unwrap_or_else(|e| panic!("StaticDtor_Career {case}: {e}"));
                0
            }
            4 => {
                r.cpu.set_gpr(4, obj as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x168688).unwrap_or_else(|e| panic!("PreRaceSel_Destruct {case}: {e}"));
                0
            }
            5 => {
                r.cpu.set_gpr(4, obj as u64);
                r.call(0x168b80).unwrap_or_else(|e| panic!("PreRaceSel_GetPicks {case}: {e}"));
                r.cpu.gpr(2) as u32
            }
            6 => {
                r.cpu.set_gpr(4, obj as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x169668).unwrap_or_else(|e| panic!("RaceScene_Destruct {case}: {e}"));
                0
            }
            7 => {
                r.call(0x169d28).unwrap_or_else(|e| panic!("StaticDtor_RaceScene {case}: {e}"));
                0
            }
            8 => {
                r.cpu.set_gpr(4, obj as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x16a978).unwrap_or_else(|e| panic!("Circuit_Destruct {case}: {e}"));
                0
            }
            9 => {
                r.cpu.set_gpr(4, circuit as u64);
                r.call(0x16bca8).unwrap_or_else(|e| panic!("Circuit_ResetHeat {case}: {e}"));
                0
            }
            10 => {
                r.cpu.set_gpr(4, circuit as u64);
                r.cpu.set_gpr(5, slot as u64);
                r.cpu.set_gpr(6, rider as u64);
                r.call(0x16cfd8).unwrap_or_else(|e| panic!("Circuit_SetupRider {case}: {e}"));
                0
            }
            11 => {
                r.call(0x16d0a8).unwrap_or_else(|e| panic!("Circuit_OnEventOver {case}: {e}"));
                0
            }
            _ => {
                r.cpu.set_gpr(4, circuit as u64);
                r.call(0x16d230).unwrap_or_else(|e| panic!("Circuit_SetupRace {case}: {e}"));
                0
            }
        };
        let sp = r.cpu.gpr(29) as u32;
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x5000];
        r.mem.read(base, &mut theirs_mem).unwrap();

        m.regions.push((sp.wrapping_sub(0x80), vec![0u8; 0x80]));
        let mut env = RecEnv {
            log: log.clone(),
            rider_ret: 0,
            line_ret: 0,
            inp_ret: 0,
            pin_ret: 0,
            medal_idx: mi,
        };
        let ret_mine = match which {
            0 => {
                career_destruct(&mut m, &mut env, career, how);
                0
            }
            1 => {
                career_init_short_record(&mut m, src, dst);
                0
            }
            2 => {
                career_setup_solo_rider(&mut m, &mut env, career);
                0
            }
            3 => {
                static_dtor_career(&mut m, &mut env);
                0
            }
            4 => {
                pre_race_sel_destruct(&mut m, &mut env, obj, how);
                0
            }
            5 => pre_race_sel_get_picks(obj),
            6 => {
                race_scene_destruct(&mut m, &mut env, obj, how);
                0
            }
            7 => {
                static_dtor_race_scene(&mut m, &mut env);
                0
            }
            8 => {
                circuit_destruct(&mut m, &mut env, obj, how);
                0
            }
            9 => {
                circuit_reset_heat(&mut m, circuit);
                0
            }
            10 => {
                circuit_setup_rider(&mut m, &mut env, circuit, slot, rider);
                0
            }
            11 => {
                circuit_on_event_over();
                0
            }
            _ => {
                circuit_setup_race(&mut m, &mut env, circuit, sp);
                0
            }
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
    }
}
