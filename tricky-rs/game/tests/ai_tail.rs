//! Runner checks for board row G4m (last 9 `not started` `ai` functions):
//! - `aiobj::computer_construct` (`cComputer::cComputer` 0x138210)
//! - `aiobj::computer_vf8` (`cComputer::vf8_138438` 0x138438)
//! - `aiobj::computer_destruct` (`cComputer::~cComputer` 0x13a628)
//! - `snapshot::spline_path_place_all` (`SplinePathNode_PlaceAll` 0x1fa838)
//! - `snapshot::spline_path_place_one` (`SplinePathNode_PlaceOne` 0x1fa8d8)
//! - `snapshot::spline_path_free_mem` (`SplinePathNode_FreeMem` 0x1fb3c8)
//! - `snapshot::spline_model_init` (`SplineModel_Init` 0x2514c0)
//! - `snapshot::spline_path_draw_path` (`SplinePathNode_DrawPath` 0x251748)
//! - `snapshot::spline_path_is_visible` (`SplinePathNode_IsVisible` 0x251a98)
//! (TRICKY_ELF; skipped without it).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::aiobj::{
    computer_construct, computer_destruct, computer_vf8, BOARDER_CTOR, BOARDER_DTOR, BOARDER_HANDLER_CTOR,
    BOARDER_HANDLER_DTOR, BOARDER_VF8, COMPUTER_RESET, COMPUTER_VBASE_VT, DELETE,
};
use tricky_game::fe3dstate::{FeEnv, GAPP};
use tricky_game::pf::Mem;
use tricky_game::snapshot::{
    spline_model_init, spline_path_draw_path, spline_path_free_mem, spline_path_is_visible, spline_path_place_all,
    spline_path_place_one, LoadEnv,
};

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
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        let mut v = args.to_vec();
        // When cComputer::cComputer (in_charge=0) or ~cComputer (how=0) passes a stack vtable copy,
        // verify the vtable pointer at *b and all 0xb0 bytes of that vtable copy!
        if func == COMPUTER_RESET || func == BOARDER_HANDLER_DTOR {
            let b = m.r32(args[0]);
            let vt = m.r32(b);
            v.push(vt);
            for k in 0..0xb0 / 4 {
                v.push(m.r32(vt + 4 * k));
            }
        }
        self.0.borrow_mut().push((func, v));
        0
    }
}

#[test]
fn ccomputer_lifecycle_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let base = r.mem.alloc(0x1000);
    let log: Log = Rc::new(RefCell::new(vec![]));

    for f in [BOARDER_CTOR, BOARDER_HANDLER_CTOR, COMPUTER_RESET, BOARDER_VF8, BOARDER_HANDLER_DTOR, BOARDER_DTOR, DELETE]
    {
        let l2 = log.clone();
        let arity = match f {
            BOARDER_CTOR | COMPUTER_RESET | BOARDER_VF8 | DELETE => 1,
            BOARDER_HANDLER_DTOR | BOARDER_DTOR => 2,
            BOARDER_HANDLER_CTOR => 3,
            _ => unreachable!(),
        };
        r.stub(f, move |c, mem| {
            let mut v: Vec<u32> = (0..arity).map(|i| c.gpr(4 + i) as u32).collect();
            if f == COMPUTER_RESET || f == BOARDER_HANDLER_DTOR {
                let b = mem.read_u32(v[0]);
                let vt = mem.read_u32(b);
                v.push(vt);
                for k in 0..0xb0 / 4 {
                    v.push(mem.read_u32(vt + 4 * k));
                }
            }
            l2.borrow_mut().push((f, v));
        });
    }

    let mut g = Rng(0x1382_10aa_c001_0001);
    for case in 0..1_000u32 {
        let mut init = vec![0u8; 0x1000];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        // Keep a valid boarder pointer at +0 (either inside base+0x8b0 or offset)
        let o = base;
        let b_alt = base + 0x900 + ((g.below(16)) << 2);
        r.mem.write(base, &init).unwrap();
        r.mem.write_u32(o, b_alt);
        r.mem.write_u32(b_alt, COMPUTER_VBASE_VT);

        let which = case % 3;
        let in_charge = g.below(2);
        let rider_idx = (g.next() & 0xffff) as i16;
        let how = g.below(4);

        let mut vt_bytes = vec![0u8; 0xb0];
        r.mem.read(COMPUTER_VBASE_VT, &mut vt_bytes).unwrap();

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x1000];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        m.regions.push((COMPUTER_VBASE_VT, vt_bytes));

        log.borrow_mut().clear();
        let ret_theirs = match which {
            0 => {
                r.cpu.set_gpr(4, o as u64);
                r.cpu.set_gpr(5, in_charge as u64);
                r.cpu.set_gpr(6, rider_idx as i32 as i64 as u64);
                r.call(0x138210).unwrap_or_else(|e| panic!("ctor case {case}: {e}"));
                r.cpu.gpr(2) as u32
            }
            1 => {
                r.cpu.set_gpr(4, o as u64);
                r.call(0x138438).unwrap_or_else(|e| panic!("vf8 case {case}: {e}"));
                0
            }
            _ => {
                r.cpu.set_gpr(4, o as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x13a628).unwrap_or_else(|e| panic!("dtor case {case}: {e}"));
                0
            }
        };
        let sp = r.cpu.gpr(29) as u32;
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x1000];
        r.mem.read(base, &mut theirs_mem).unwrap();

        m.regions.push((sp.wrapping_sub(0x100), vec![0u8; 0x100]));
        let env = &mut RecFe(log.clone());
        let ret_mine = match which {
            0 => computer_construct(&mut m, env, o, in_charge, rider_idx, sp),
            1 => {
                computer_vf8(&mut m, env, o);
                0
            }
            _ => {
                computer_destruct(&mut m, env, o, how, sp);
                0
            }
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());
        let mine_mem = &m.regions[0].1;

        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, mine_mem, "case {case} which {which}: memory mismatch");
    }
}

struct SplineEnv {
    log: Log,
    tan: [f32; 3],
    seg_ret: u32,
    len_ret: u32,
    cull_rets: Vec<u32>,
    cull_idx: usize,
}

impl LoadEnv for SplineEnv {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        match func {
            0x2515c8 => {
                // Spline_Evaluate(spline, pos_dst, tan_buf, dist_bits)
                let (pos_dst, tan_buf, dist_bits) = (args[1], args[2], args[3]);
                m.w32(pos_dst, dist_bits);
                m.w32(pos_dst + 4, dist_bits ^ 0x1234_5678);
                m.w32(pos_dst + 8, dist_bits.wrapping_add(1));
                for k in 0..3u32 {
                    m.w32(tan_buf + 4 * k, self.tan[k as usize].to_bits());
                }
                self.log.borrow_mut().push((func, args.to_vec()));
                0
            }
            0x254b20 => {
                self.log.borrow_mut().push((func, args.to_vec()));
                self.seg_ret
            }
            0x251968 => {
                self.log.borrow_mut().push((func, args.to_vec()));
                self.len_ret
            }
            0x114f80 => {
                self.log.borrow_mut().push((func, args.to_vec()));
                0
            }
            0x7e0284 => {
                // Renderer CullBox: log args + both 16-byte stack boxes
                let mut v = args.to_vec();
                for &p in &[args[1], args[2]] {
                    for k in 0..4u32 {
                        v.push(m.r32(p + 4 * k));
                    }
                }
                self.log.borrow_mut().push((func, v));
                let ret = self.cull_rets[self.cull_idx % self.cull_rets.len()];
                self.cull_idx += 1;
                ret
            }
            0x7e023c => {
                // Renderer DrawPath: log args + all 11 written words of each of the 10 vertices
                let mut v = args.to_vec();
                let base = args[2];
                for k in 0..10u32 {
                    let vtx = base + k * 0x30;
                    for w in [0u32, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11] {
                        v.push(m.r32(vtx + 4 * w));
                    }
                }
                self.log.borrow_mut().push((func, v));
                0
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
fn spline_path_place_and_free_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    r.allow(0x1fa8d8);
    r.allow(0x2510f8);
    let base = r.mem.alloc(0x600);
    let (node, arr, tan_buf) = (base, base + 0x100, base + 0x400);
    let log: Log = Rc::new(RefCell::new(vec![]));
    let cur_tan: Rc<RefCell<[f32; 3]>> = Rc::new(RefCell::new([0.0; 3]));

    {
        let l2 = log.clone();
        let t2 = cur_tan.clone();
        r.stub(0x2515c8, move |c, mem| {
            let (a0, a1, a2, d) = (c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32, c.f(12).to_bits());
            mem.write_u32(a1, d);
            mem.write_u32(a1 + 4, d ^ 0x1234_5678);
            mem.write_u32(a1 + 8, d.wrapping_add(1));
            let tan = *t2.borrow();
            for k in 0..3u32 {
                mem.write_u32(a2 + 4 * k, tan[k as usize].to_bits());
            }
            l2.borrow_mut().push((0x2515c8, vec![a0, a1, a2, d]));
        });
    }
    {
        let l2 = log.clone();
        r.stub(0x114f80, move |c, _| {
            l2.borrow_mut().push((0x114f80, vec![c.gpr(4) as u32]));
        });
    }

    let edge_tans: [[f32; 3]; 10] = [
        [0.0, 0.0, 0.0],
        [0.0, -0.0, -0.0],
        [0.0, 1.0, 0.5],
        [0.0, -1.0, -0.5],
        [1.0, 0.0, 0.25],
        [-1.0, 0.0, -0.25],
        [-1.0, 2.0, 0.75],
        [-1.0, -2.0, -0.75],
        [1.0, -2.0, 0.125],
        [1.0, 2.0, -0.125],
    ];

    let mut g = Rng(0x1fa8_38bb_beef_0002);
    for case in 0..4_000u32 {
        let mut init = vec![0u8; 0x600];
        for b in &mut init {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(base, &init).unwrap();

        let count: i32 = (g.below(10) as i32) - 2; // -2..=7
        let mode34 = g.below(4);
        let mode38 = g.below(5);
        let base_yaw = g.f(-6.28, 6.28);
        let step = g.f(0.1, 50.0);
        let speed = if g.below(3) == 0 { -g.f(0.01, 20.0) } else { g.f(0.0, 20.0) };
        let start_dist = g.f(0.0, 100.0);
        let total_len = g.f(10.0, 80.0);

        r.mem.write_u32(node + 0x30, count as u32);
        r.mem.write_u32(node + 0x34, mode34);
        r.mem.write_u32(node + 0x38, mode38);
        r.mem.write_u32(node + 0x44, base_yaw.to_bits());
        r.mem.write_u32(node + 0x48, step.to_bits());
        r.mem.write_u32(node + 0x4c, speed.to_bits());
        r.mem.write_u32(node + 0x50, start_dist.to_bits());
        r.mem.write_u32(node + 0x74, total_len.to_bits());
        r.mem.write_u32(node + 0x7c, arr);

        let tan = if (case as usize) < edge_tans.len() * 20 {
            edge_tans[(case as usize) % edge_tans.len()]
        } else {
            [g.f(-5.0, 5.0), g.f(-5.0, 5.0), g.f(-2.0, 2.0)]
        };
        *cur_tan.borrow_mut() = tan;

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x600];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));

        let which = case % 3;
        let idx = g.below(8);
        let dist = g.f(0.0, 100.0);
        let how = g.below(4);

        log.borrow_mut().clear();
        match which {
            0 => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, tan_buf as u64);
                r.call(0x1fa838).unwrap_or_else(|e| panic!("PlaceAll case {case}: {e}"));
            }
            1 => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, idx as u64);
                r.cpu.set_gpr(6, tan_buf as u64);
                r.cpu.set_f(12, dist);
                r.call(0x1fa8d8).unwrap_or_else(|e| panic!("PlaceOne case {case}: {e}"));
            }
            _ => {
                r.cpu.set_gpr(4, node as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x1fb3c8).unwrap_or_else(|e| panic!("FreeMem case {case}: {e}"));
            }
        }
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x600];
        r.mem.read(base, &mut theirs_mem).unwrap();

        let mut env = SplineEnv {
            log: log.clone(),
            tan,
            seg_ret: 0,
            len_ret: 0,
            cull_rets: vec![0],
            cull_idx: 0,
        };
        match which {
            0 => spline_path_place_all(&mut m, &mut env, node, tan_buf),
            1 => spline_path_place_one(&mut m, &mut env, node, idx, tan_buf, dist),
            _ => spline_path_free_mem(&mut m, &mut env, node, how),
        }
        let mine_log = std::mem::take(&mut *log.borrow_mut());
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
    }
}

#[test]
fn spline_model_init_draw_and_visibility_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    r.allow(0x251a98);
    let base = r.mem.alloc(0x1800);
    let (app, ren, vt, sm, seg_obj, seg_hdr, color, segs_base, seg_ret_obj) = (
        base,
        base + 0x800,
        base + 0x900,
        base + 0xc00,
        base + 0xc40,
        base + 0xcc0,
        base + 0xd00,
        base + 0xd40,
        base + 0x1300,
    );
    let log: Log = Rc::new(RefCell::new(vec![]));
    let len_bits: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let cull_rets: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(vec![0]));
    let cull_idx: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

    {
        let l2 = log.clone();
        r.stub(0x254b20, move |c, _| {
            l2.borrow_mut().push((0x254b20, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, seg_ret_obj as u64);
        });
    }
    {
        let l2 = log.clone();
        let lb = len_bits.clone();
        r.stub(0x251968, move |c, _| {
            l2.borrow_mut().push((0x251968, vec![c.gpr(4) as u32]));
            c.set_f(0, f32::from_bits(*lb.borrow()));
        });
    }
    {
        let l2 = log.clone();
        r.stub(0x7e01f4, move |c, _| {
            l2.borrow_mut().push((0x7e01f4, vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32]));
        });
    }
    {
        let l2 = log.clone();
        r.stub(0x7e01e4, move |c, _| {
            l2.borrow_mut().push((0x7e01e4, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
        });
    }
    {
        let l2 = log.clone();
        r.stub(0x7e023c, move |c, mem| {
            let mut v = vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32, c.gpr(7) as u32];
            let p = c.gpr(6) as u32;
            for k in 0..10u32 {
                let vtx = p + k * 0x30;
                for w in [0u32, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11] {
                    v.push(mem.read_u32(vtx + 4 * w));
                }
            }
            l2.borrow_mut().push((0x7e023c, v));
        });
    }
    {
        let l2 = log.clone();
        let cr = cull_rets.clone();
        let ci = cull_idx.clone();
        r.stub(0x7e0284, move |c, mem| {
            let mut v = vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32];
            for &p in &[c.gpr(5) as u32, c.gpr(6) as u32] {
                for k in 0..4u32 {
                    v.push(mem.read_u32(p + 4 * k));
                }
            }
            l2.borrow_mut().push((0x7e0284, v));
            let mut idx = ci.borrow_mut();
            let list = cr.borrow();
            let ret = list[*idx % list.len()];
            *idx += 1;
            c.set_gpr(2, ret as i32 as i64 as u64);
        });
    }

    let mut g = Rng(0x2517_48cc_d00d_0003);
    for case in 0..1_500u32 {
        r.mem.write_u32(GAPP, app);
        r.mem.write_u32(app + 0x724, ren);
        r.mem.write_u32(ren + 0x10, vt);
        // Set vtable entries + non-zero this-adjustments
        for (slot, fn_addr, adj) in [
            (0x1f0u32, 0x7e01f4u32, 4i16),
            (0x1e0, 0x7e01e4, -4),
            (0x238, 0x7e023c, 8),
            (0x280, 0x7e0284, 12),
        ] {
            r.mem.write_u16(vt + slot, adj as u16);
            r.mem.write_u32(vt + slot + 4, fn_addr);
        }

        // Setup SplineModel_Init data
        r.mem.write_u32(seg_ret_obj + 0x20, g.next());
        let lb = g.f(0.0, 5000.0).to_bits();
        *len_bits.borrow_mut() = lb;

        // Setup linked segments (up to 6) for DrawPath & IsVisible
        let count: i32 = (g.below(8) as i32) - 1; // -1..=6
        r.mem.write_u32(sm + 0xc, seg_obj);
        r.mem.write_u32(seg_obj + 0x58, seg_hdr);
        r.mem.write_u32(seg_hdr + 0x1c, count as u32);
        r.mem.write_u32(seg_hdr + 0x20, segs_base);
        for k in 0..4u32 {
            r.mem.write_u32(color + 4 * k, g.f(-2.0, 2.0).to_bits());
        }
        for s in 0..6u32 {
            let seg = segs_base + s * 0x80;
            for w in 0..16u32 {
                r.mem.write_u32(seg + 4 * w, g.f(-100.0, 100.0).to_bits());
            }
            r.mem.write_u32(seg + 0x54, segs_base + ((s + 1) % 6) * 0x80);
            for c in 0..3u32 {
                r.mem.write_u32(seg + 0x5c + 4 * c, g.f(-5000.0, 5000.0).to_bits());
            }
        }
        let c_vals = [0u32, 1, 2, 0xffff_ffff];
        let rets: Vec<u32> = (0..6).map(|_| c_vals[g.below(4) as usize]).collect();
        *cull_rets.borrow_mut() = rets.clone();
        *cull_idx.borrow_mut() = 0;

        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x1800];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut gb = vec![0u8; 4];
        r.mem.read(GAPP, &mut gb).unwrap();
        m.regions.push((GAPP, gb));

        let which = case % 3;
        let idx = g.below(32);

        log.borrow_mut().clear();
        let ret_theirs = match which {
            0 => {
                r.cpu.set_gpr(4, sm as u64);
                r.cpu.set_gpr(5, idx as u64);
                r.call(0x2514c0).unwrap_or_else(|e| panic!("SplineModel_Init case {case}: {e}"));
                0
            }
            1 => {
                r.cpu.set_gpr(4, segs_base as u64);
                r.call(0x251a98).unwrap_or_else(|e| panic!("IsVisible case {case}: {e}"));
                r.cpu.gpr(2) as u32
            }
            _ => {
                r.cpu.set_gpr(4, sm as u64);
                r.cpu.set_gpr(5, color as u64);
                r.call(0x251748).unwrap_or_else(|e| panic!("DrawPath case {case}: {e}"));
                0
            }
        };
        let sp = r.cpu.gpr(29) as u32;
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x1800];
        r.mem.read(base, &mut theirs_mem).unwrap();

        m.regions.push((sp.wrapping_sub(0x300), vec![0u8; 0x300]));
        let mut env = SplineEnv {
            log: log.clone(),
            tan: [0.0; 3],
            seg_ret: seg_ret_obj,
            len_ret: lb,
            cull_rets: rets,
            cull_idx: 0,
        };
        let ret_mine = match which {
            0 => {
                spline_model_init(&mut m, &mut env, sm, idx);
                0
            }
            1 => spline_path_is_visible(&mut m, &mut env, segs_base, sp),
            _ => {
                spline_path_draw_path(&mut m, &mut env, sm, color, sp);
                0
            }
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());
        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
    }
}
