//! Runner checks for board row G8o:
//! - `hudsprites::draw_anchored_gradient` against `Sprite_DrawAnchoredGradient` 0x1fc330
//! - `hudsprites::hud_draw_sprite_anchored` against `HUD_DrawSpriteAnchored` 0x1feaf0
//! - `copies::empty_stub` against `Misc_EmptyStubA` 0x256100
//! (TRICKY_ELF; skipped without it).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::copies::empty_stub;
use tricky_game::hudsprites::{draw_anchored_gradient, hud_draw_sprite_anchored, SpritesEnv};
use tricky_game::pf::Mem;
use tricky_game::replaymgr::GAPP;

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
}

type Log = Rc<RefCell<Vec<(u32, Vec<u32>)>>>;
const R_FN: u32 = 0x7e0000;

struct Ours(Log);
impl SpritesEnv for Ours {
    fn call(&mut self, m: &mut Mem, func: u32, ints: &[u32], floats: &[f32]) -> (u32, f32) {
        let mut v = ints.to_vec();
        if func == 0x19b4d8 || func == 0x19bf90 {
            for &ptr in &ints[1..5] {
                for w in 0..4u32 {
                    v.push(m.r32(ptr.wrapping_add(w * 4)));
                }
            }
        }
        v.extend(floats.iter().map(|f| f.to_bits()));
        self.0.borrow_mut().push((func, v));
        (0, 0.0)
    }
    fn renderer(&mut self, _m: &mut Mem, slot: u32, ints: &[u32], _floats: &[f32]) -> u32 {
        self.0.borrow_mut().push((R_FN + slot, ints.to_vec()));
        0
    }
}

const FVALS: [f32; 14] = [0.0, -0.0, 1.0, 0.5, 2.25, 12.0, -3.5, 100.0, 0.75, 37.5, 0.1, 0.3, 1.3, -42.125];

#[test]
fn sprite_draw_anchored_gradient_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    let base = r.mem.alloc(0x500);
    let (app, ren, vt, rect, colors) = (base, base + 0x100, base + 0x200, base + 0x300, base + 0x340);
    let log: Log = Rc::new(RefCell::new(vec![]));
    for f in [0x19b4d8u32, 0x19bf90] {
        let l2 = log.clone();
        r.stub(f, move |c, mem| {
            let ptrs = [c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32, c.gpr(7) as u32, c.gpr(8) as u32];
            let mut v = ptrs.to_vec();
            for &ptr in &ptrs[1..5] {
                for w in 0..4u32 {
                    v.push(mem.read_u32(ptr.wrapping_add(w * 4)));
                }
            }
            v.extend((0..8).map(|k| c.f(12 + k).to_bits()));
            l2.borrow_mut().push((f, v));
        });
    }
    let l2 = log.clone();
    r.stub(R_FN + 0x1f4, move |c, _| {
        l2.borrow_mut().push((R_FN + 0x1f4, vec![c.gpr(5) as u32, c.gpr(6) as u32]));
    });

    let mut g = Rng(0x1fc3_30aa_babe_0001);
    for case in 0..10_000u32 {
        r.mem.write_u32(GAPP, app);
        r.mem.write_u32(app + 0x724, ren);
        r.mem.write_u32(ren + 0x10, vt);
        r.mem.write_u32(vt + 0x1f4, R_FN + 0x1f4);
        r.mem.write_u16(vt + 0x1f0, 0);
        r.mem.write_u32(rect, g.next());
        for k in 1..7 {
            r.mem.write_u32(rect + 4 * k, FVALS[g.below(FVALS.len() as u32) as usize].to_bits());
        }
        // Exercise both aligned and unaligned 16-byte colour pointers (original uses ldl/ldr).
        let c_ptrs: [u32; 4] = std::array::from_fn(|i| colors + (i as u32) * 0x20 + g.below(8));
        for &cp in &c_ptrs {
            for byte in 0..16u32 {
                r.mem.write_u8(cp + byte, (g.next() & 0xff) as u8);
            }
        }
        let flags = if case < 128 { case } else { g.next() & 0xff };
        let v: [f32; 4] = std::array::from_fn(|_| FVALS[g.below(FVALS.len() as u32) as usize]);

        r.cpu.set_gpr(4, rect as u64);
        r.cpu.set_gpr(5, flags as u64);
        r.cpu.set_gpr(6, c_ptrs[0] as u64);
        r.cpu.set_gpr(7, c_ptrs[1] as u64);
        r.cpu.set_gpr(8, c_ptrs[2] as u64);
        r.cpu.set_gpr(9, c_ptrs[3] as u64);
        for (k, x) in v.iter().enumerate() {
            r.cpu.set_f(12 + k, *x);
        }
        log.borrow_mut().clear();
        r.call(0x1fc330).unwrap_or_else(|e| panic!("case {case}: {e}"));
        let sp = r.cpu.gpr(29) as u32;
        let theirs = std::mem::take(&mut *log.borrow_mut());

        let mut m = Mem::default();
        let mut b = vec![0u8; 0x500];
        r.mem.read(base, &mut b).unwrap();
        m.regions.push((base, b));
        let mut gb = vec![0u8; 4];
        r.mem.read(GAPP, &mut gb).unwrap();
        m.regions.push((GAPP, gb));
        m.regions.push((sp.wrapping_sub(0x100), vec![0u8; 0x100]));

        let env = &mut Ours(log.clone());
        draw_anchored_gradient(
            &mut m,
            env,
            rect,
            flags,
            c_ptrs[0],
            c_ptrs[1],
            c_ptrs[2],
            c_ptrs[3],
            sp,
            v[0],
            v[1],
            v[2],
            v[3],
        );
        let mine = std::mem::take(&mut *log.borrow_mut());
        assert_eq!(theirs, mine, "case {case} (flags {flags:#x})");
    }
}

#[test]
fn hud_draw_sprite_anchored_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    let base = r.mem.alloc(0x400);
    let (app, ren, vt, rect) = (base, base + 0x100, base + 0x200, base + 0x300);
    let log: Log = Rc::new(RefCell::new(vec![]));
    {
        let l2 = log.clone();
        r.stub(0x19bb48, move |c, mem| {
            let mut v = vec![c.gpr(4) as u32];
            v.extend((0..8).map(|k| c.f(12 + k).to_bits()));
            // 9th float arg `d` is passed at 0($sp).
            v.push(mem.read_u32(c.gpr(29) as u32));
            l2.borrow_mut().push((0x19bb48, v));
        });
    }
    let l2 = log.clone();
    r.stub(R_FN + 0x1f4, move |c, _| {
        l2.borrow_mut().push((R_FN + 0x1f4, vec![c.gpr(5) as u32, c.gpr(6) as u32]));
    });

    let mut g = Rng(0x1fea_f0cc_cafe_0002);
    for case in 0..10_000u32 {
        r.mem.write_u32(GAPP, app);
        r.mem.write_u32(app + 0x724, ren);
        r.mem.write_u32(ren + 0x10, vt);
        r.mem.write_u32(vt + 0x1f4, R_FN + 0x1f4);
        r.mem.write_u16(vt + 0x1f0, 0);
        r.mem.write_u32(rect, g.next());
        for k in 1..7 {
            r.mem.write_u32(rect + 4 * k, FVALS[g.below(FVALS.len() as u32) as usize].to_bits());
        }
        let flags = if case < 128 { case } else { g.next() & 0xff };
        let v: [f32; 5] = std::array::from_fn(|_| FVALS[g.below(FVALS.len() as u32) as usize]);

        r.cpu.set_gpr(4, rect as u64);
        r.cpu.set_gpr(5, flags as u64);
        for (k, x) in v.iter().enumerate() {
            r.cpu.set_f(12 + k, *x);
        }
        log.borrow_mut().clear();
        r.call(0x1feaf0).unwrap_or_else(|e| panic!("case {case}: {e}"));
        let theirs = std::mem::take(&mut *log.borrow_mut());

        let mut m = Mem::default();
        let mut b = vec![0u8; 0x400];
        r.mem.read(base, &mut b).unwrap();
        m.regions.push((base, b));
        let mut gb = vec![0u8; 4];
        r.mem.read(GAPP, &mut gb).unwrap();
        m.regions.push((GAPP, gb));

        let env = &mut Ours(log.clone());
        hud_draw_sprite_anchored(&mut m, env, rect, flags, v[0], v[1], v[2], v[3], v[4]);
        let mine = std::mem::take(&mut *log.borrow_mut());
        assert_eq!(theirs, mine, "case {case} (flags {flags:#x})");
    }
}

#[test]
fn misc_empty_stub_a_matches_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    let buf = r.mem.alloc(0x100);
    let mut g = Rng(0x2561_00dd_dead_0003);
    for case in 0..500u32 {
        let mut before = vec![0u8; 0x100];
        for b in &mut before {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(buf, &before).unwrap();
        for reg in 4..12 {
            r.cpu.set_gpr(reg, ((g.next() as u64) << 32) | (g.next() as u64));
        }
        r.call(0x256100).unwrap_or_else(|e| panic!("case {case}: {e}"));
        empty_stub();
        let mut after = vec![0u8; 0x100];
        r.mem.read(buf, &mut after).unwrap();
        assert_eq!(before, after, "case {case}: memory mutated");
        assert_eq!(r.cpu.gpr(29), 0x01ff_0000, "case {case}: sp not restored");
    }
}
