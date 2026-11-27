//! Runner checks for board row G1y (24 `boarder` functions):
//! - `boardfx`: `AirPredict_Init`/`Reset` (`0x123a88`/`0x123ab8`), `Fx_Splash_Init`/`Reset` (`0x12c0e8`/`0x12c178`),
//!   `Fx_BrakeSpray_Init`/`Reset` (`0x12df58`/`0x12dfd0`), `Fx_BoardPuff_Init`/`Destruct`/`Reset`
//!   (`0x12ec08`/`0x12ec78`/`0x12ec98`), `Fx_SpeedDust_Init`/`Destruct`/`Reset` (`0x12f420`/`0x12f490`/`0x12f4b0`),
//!   `Fx_BoardTrail_Init`/`Reset` (`0x12fe48`/`0x12fed8`), `Fx_SurfaceSpray_Init`/`Reset` (`0x131110`/`0x131160`),
//!   `Fx_BoardTrack_Init`/`Reset` (`0x1324c8`/`0x132518`)
//! - `bdrload`: `BdrLoad_AddPart` (`0x201118`), `StaticInit_BdrLoad` (`0x208540`)
//! - `snapshot`: `Spline_FindSegmentForward` (`0x251508`), `Spline_FindSegmentBackward` (`0x251568`),
//!   `Spline_Evaluate` (`0x2515c8`) + `0x2516a8` (`Spline_IsVisible`), `RailNode_GetSpline` (`0x2609b0`)
//! (TRICKY_ELF; skipped without it).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::bdrload::{add_part, static_init_bdr_load, BDR_LOAD_COLORS, MPF_ADD_PART};
use tricky_game::boardfx::{
    air_predict_init, air_predict_reset, fx_board_puff_init, fx_board_puff_reset, fx_board_track_init,
    fx_board_track_reset, fx_board_trail_init, fx_board_trail_reset, fx_brake_spray_init, fx_brake_spray_reset,
    fx_delete_if, fx_speed_dust_init, fx_speed_dust_reset, fx_splash_init, fx_splash_reset, fx_surface_spray_init,
    fx_surface_spray_reset,
};
use tricky_game::fe3dstate::{FeEnv, GAPP};
use tricky_game::pf::Mem;
use tricky_game::snapshot::{
    rail_node_get_spline, spline_evaluate, spline_find_segment_backward, spline_find_segment_forward,
    spline_is_visible, LoadEnv,
};
use tricky_game::textbox::TextBoxEnv;

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

struct RecFe(Log);
impl FeEnv for RecFe {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.0.borrow_mut().push((func, args.to_vec()));
        0
    }
}

struct RecTb(Log);
impl TextBoxEnv for RecTb {
    fn call(&mut self, _m: &mut Mem, func: u32, ints: &[u32], _floats: &[tricky_game::ps2f::F]) -> u32 {
        self.0.borrow_mut().push((func, ints.to_vec()));
        0
    }
}

struct RecLoad {
    log: Log,
    seg_ret: u32,
    cull_ret: u32,
}
impl LoadEnv for RecLoad {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        match func {
            0x254b20 => {
                self.log.borrow_mut().push((func, args.to_vec()));
                self.seg_ret
            }
            0x7e5284 => {
                let mut v = args.to_vec();
                for &p in &[args[1], args[2]] {
                    for k in 0..4u32 {
                        v.push(m.r32(p + 4 * k));
                    }
                }
                self.log.borrow_mut().push((func, v));
                self.cull_ret
            }
            _ => {
                self.log.borrow_mut().push((func, args.to_vec()));
                0
            }
        }
    }
    fn pool_alloc(&mut self, _m: &mut Mem, _size: u32, _name: u32) -> u32 {
        0
    }
}

#[test]
fn boarder_fx_airpredict_and_bdrload_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    for f in [
        0x123ab8u32,
        0x12c0e8,
        0x12c178,
        0x12dfd0,
        0x12ec98,
        0x12f4b0,
        0x12fed8,
        0x131160,
        0x132518,
    ] {
        r.allow(f);
    }

    let log: Log = Rc::new(RefCell::new(vec![]));
    {
        let l2 = log.clone();
        r.stub(0x114f80, move |c, _| l2.borrow_mut().push((0x114f80, vec![c.gpr(4) as u32])));
    }
    {
        let l2 = log.clone();
        r.stub(MPF_ADD_PART, move |c, _| {
            l2.borrow_mut().push((MPF_ADD_PART, vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32]));
        });
    }

    let buf = r.mem.alloc(0x2000);
    let mut g = Rng(0x12c0_e8aa_2085_0001);

    for case in 0..400u32 {
        let mut init = vec![0u8; 0x2000];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(buf, &init).unwrap();
        let mut cinit = vec![0u8; 0x30];
        for b in &mut cinit {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(BDR_LOAD_COLORS, &cinit).unwrap();

        let p = buf + ((g.below(4)) << 2); // test both 16-byte aligned and misaligned pointers!
        let how = g.below(4);
        let flag = if g.below(2) == 0 { 0 } else { g.next() | 1 };
        let a2 = g.next();
        let mpf = if g.below(2) == 0 { 0 } else { g.next() | 4 };

        let mut m = Mem::default();
        m.regions.push((buf, init));
        m.regions.push((BDR_LOAD_COLORS, cinit));

        let which = case % 20;
        log.borrow_mut().clear();
        let ret_theirs = match which {
            0 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x123a88).unwrap();
                r.cpu.gpr(2) as u32
            }
            1 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x123ab8).unwrap();
                0
            }
            2 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12c0e8).unwrap();
                r.cpu.gpr(2) as u32
            }
            3 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12c178).unwrap();
                0
            }
            4 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12df58).unwrap();
                r.cpu.gpr(2) as u32
            }
            5 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12dfd0).unwrap();
                0
            }
            6 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12ec08).unwrap();
                r.cpu.gpr(2) as u32
            }
            7 => {
                r.cpu.set_gpr(4, p as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x12ec78).unwrap();
                0
            }
            8 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12ec98).unwrap();
                0
            }
            9 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12f420).unwrap();
                r.cpu.gpr(2) as u32
            }
            10 => {
                r.cpu.set_gpr(4, p as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x12f490).unwrap();
                0
            }
            11 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12f4b0).unwrap();
                0
            }
            12 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12fe48).unwrap();
                r.cpu.gpr(2) as u32
            }
            13 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x12fed8).unwrap();
                0
            }
            14 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x131110).unwrap();
                r.cpu.gpr(2) as u32
            }
            15 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x131160).unwrap();
                0
            }
            16 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x1324c8).unwrap();
                r.cpu.gpr(2) as u32
            }
            17 => {
                r.cpu.set_gpr(4, p as u64);
                r.call(0x132518).unwrap();
                0
            }
            18 => {
                r.cpu.set_gpr(4, p as u64);
                r.cpu.set_gpr(5, flag as u64);
                r.cpu.set_gpr(6, a2 as u64);
                r.cpu.set_gpr(7, mpf as u64);
                r.call(0x201118).unwrap();
                0
            }
            _ => {
                r.call(0x208540).unwrap();
                0
            }
        };
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x2000];
        r.mem.read(buf, &mut theirs_mem).unwrap();
        let mut theirs_col = vec![0u8; 0x30];
        r.mem.read(BDR_LOAD_COLORS, &mut theirs_col).unwrap();

        let ret_mine = match which {
            0 => air_predict_init(&mut m, p),
            1 => {
                air_predict_reset(&mut m, p);
                0
            }
            2 => fx_splash_init(&mut m, p),
            3 => {
                fx_splash_reset(&mut m, p);
                0
            }
            4 => fx_brake_spray_init(&mut m, p),
            5 => {
                fx_brake_spray_reset(&mut m, p);
                0
            }
            6 => fx_board_puff_init(&mut m, p),
            7 => {
                let mut fe = RecFe(log.clone());
                fx_delete_if(&mut m, &mut fe, p, how);
                0
            }
            8 => {
                fx_board_puff_reset(&mut m, p);
                0
            }
            9 => fx_speed_dust_init(&mut m, p),
            10 => {
                let mut fe = RecFe(log.clone());
                fx_delete_if(&mut m, &mut fe, p, how);
                0
            }
            11 => {
                fx_speed_dust_reset(&mut m, p);
                0
            }
            12 => fx_board_trail_init(&mut m, p),
            13 => {
                fx_board_trail_reset(&mut m, p);
                0
            }
            14 => fx_surface_spray_init(&mut m, p),
            15 => {
                fx_surface_spray_reset(&mut m, p);
                0
            }
            16 => fx_board_track_init(&mut m, p),
            17 => {
                fx_board_track_reset(&mut m, p);
                0
            }
            18 => {
                let mut tb = RecTb(log.clone());
                add_part(&mut m, &mut tb, flag, a2, mpf);
                0
            }
            _ => {
                static_init_bdr_load(&mut m);
                0
            }
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
        assert_eq!(&theirs_col, &m.regions[1].1, "case {case} which {which}: BDR_LOAD_COLORS mismatch");
    }
}

#[test]
fn spline_seek_evaluate_visible_and_rail_get_spline_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    r.allow(0x251508);
    r.allow(0x251568);

    let base = r.mem.alloc(0x1400);
    let (app, ren, vt, sm, segs_base, seg_bounds) =
        (base, base + 0x800, base + 0x900, base + 0xc00, base + 0xc40, base + 0x1100);
    let log: Log = Rc::new(RefCell::new(vec![]));
    let cull_ret: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

    {
        let l2 = log.clone();
        r.stub(0x251390, move |c, _| {
            l2.borrow_mut().push((
                0x251390,
                vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32, c.f(12).to_bits()],
            ));
        });
    }
    {
        let l2 = log.clone();
        r.stub(0x254b20, move |c, _| {
            l2.borrow_mut().push((0x254b20, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, seg_bounds as u64);
        });
    }
    {
        let l2 = log.clone();
        let cr = cull_ret.clone();
        r.stub(0x7e5284, move |c, mem| {
            let mut v = vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32];
            for &p in &[c.gpr(5) as u32, c.gpr(6) as u32] {
                for k in 0..4u32 {
                    v.push(mem.read_u32(p + 4 * k));
                }
            }
            l2.borrow_mut().push((0x7e5284, v));
            c.set_gpr(2, *cr.borrow() as i32 as i64 as u64);
        });
    }

    let mut g = Rng(0x2515_c8bb_2609_0002);
    for case in 0..1_500u32 {
        r.mem.write_u32(GAPP, app);
        r.mem.write_u32(app + 0x724, ren);
        r.mem.write_u32(ren + 0x10, vt);
        r.mem.write_u16(vt + 0x280, ((g.below(11) as i16) - 5) as u16);
        r.mem.write_u32(vt + 0x284, 0x7e5284);

        // Build a doubly-linked chain of 6 segments with monotonically increasing [start, start+len)
        let n_segs = 1 + g.below(6);
        let mut cursor = 0.0f32;
        for i in 0..n_segs {
            let seg = segs_base + i * 0x80;
            let len = g.f(5.0, 50.0);
            let prev = if i == 0 { 0 } else { segs_base + (i - 1) * 0x80 };
            let next = if i + 1 == n_segs { 0 } else { segs_base + (i + 1) * 0x80 };
            r.mem.write_u32(seg + 0x50, prev);
            r.mem.write_u32(seg + 0x54, next);
            r.mem.write_u32(seg + 0x74, len.to_bits());
            r.mem.write_u32(seg + 0x78, cursor.to_bits());
            cursor += len;
        }
        let start_seg = segs_base + g.below(n_segs) * 0x80;
        r.mem.write_u32(sm + 8, g.below(32));
        r.mem.write_u32(sm + 0xc, start_seg);
        r.mem.write_u32(sm + 0x10, cursor.to_bits());

        for k in 0..6u32 {
            r.mem.write_u32(seg_bounds + 4 * k, g.f(-5000.0, 5000.0).to_bits());
        }
        let c_vals = [0u32, 1, 2, 0xffff_ffff];
        let cr = c_vals[g.below(4) as usize];
        *cull_ret.borrow_mut() = cr;

        let dist = g.f(-20.0, cursor * 3.2);
        let out_pos = g.next();
        let out_tan = g.next();
        let w = g.next();
        let idx = g.below(64);

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x1400];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut gb = vec![0u8; 4];
        r.mem.read(GAPP, &mut gb).unwrap();
        m.regions.push((GAPP, gb));

        let which = case % 5;
        log.borrow_mut().clear();
        let ret_theirs = match which {
            0 => {
                r.cpu.set_gpr(4, sm as u64);
                r.cpu.set_f(12, dist);
                r.call(0x251508).unwrap();
                r.cpu.gpr(2) as u32
            }
            1 => {
                r.cpu.set_gpr(4, sm as u64);
                r.cpu.set_f(12, dist);
                r.call(0x251568).unwrap();
                r.cpu.gpr(2) as u32
            }
            2 => {
                r.cpu.set_gpr(4, sm as u64);
                r.cpu.set_gpr(5, out_pos as u64);
                r.cpu.set_gpr(6, out_tan as u64);
                r.cpu.set_f(12, dist);
                r.call(0x2515c8).unwrap();
                0
            }
            3 => {
                r.cpu.set_gpr(4, sm as u64);
                r.call(0x2516a8).unwrap();
                r.cpu.gpr(2) as u32
            }
            _ => {
                r.cpu.set_gpr(4, w as u64);
                r.cpu.set_gpr(5, idx as u64);
                r.call(0x2609b0).unwrap();
                r.cpu.gpr(2) as u32
            }
        };
        let sp = r.cpu.gpr(29) as u32;
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x1400];
        r.mem.read(base, &mut theirs_mem).unwrap();

        m.regions.push((sp.wrapping_sub(0x80), vec![0u8; 0x80]));
        let mut env = RecLoad { log: log.clone(), seg_ret: seg_bounds, cull_ret: cr };
        let ret_mine = match which {
            0 => spline_find_segment_forward(&mut m, sm, dist),
            1 => spline_find_segment_backward(&mut m, sm, dist),
            2 => {
                spline_evaluate(&mut m, &mut env, sm, out_pos, out_tan, dist);
                0
            }
            3 => spline_is_visible(&mut m, &mut env, sm, sp),
            _ => rail_node_get_spline(&mut m, &mut env, w, idx),
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
    }
}
