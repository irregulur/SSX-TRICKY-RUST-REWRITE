//! Runner checks for board row E1m (all 26 remaining `not started` `game` functions):
//! - all 25 `MockupEditor*` / `MockupItem*` / `MockupEditorEntry_*` functions (`0x1d3b88..0x1d4de8`)
//! - `SnowProgram_Destruct` (`0x21e738`)
//! (TRICKY_ELF; skipped without it).

use std::cell::RefCell;
use std::rc::Rc;

use r5900::Runner;
use tricky_game::copies::{snow_program_destruct, MEM_DELETE as COPIES_MEM_DELETE};
use tricky_game::fe3dstate::{FeEnv, GAPP};
use tricky_game::mockup::{
    add_item, add_loc_text, add_sprite, close, delete, draw, draw_item_info, draw_items, entry_construct,
    entry_destruct, for_each_a, for_each_b, init, item_decrease, item_increase, item_init, item_set_axis,
    item_stub_a, item_stub_b, next_item, open, prev_item, reset, set_target, update, MockupEnv, BLINK_COUNTER,
    FONT_COLOR, FONT_MEASURE_ASCII, FONT_PRINTF_SHADOWED, FONT_SET_COLOR_V, INPUT_GET_DEVICE, ITEM_STRIDE,
    LOC_GET_STRING, MEM_DELETE, SPRITE_DRAW_RECT_SCALED, SPRITE_SET_GET_RECT, STRCPY, WSTR_TO_ASCII,
};
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
const REN_1D4: u32 = 0x7e41d4;

struct RecMockup {
    log: Log,
    rect_ret: u32,
    loc_ret: u32,
    meas_ret: u32,
    dev_ret: u32,
}

impl MockupEnv for RecMockup {
    fn call(&mut self, m: &mut Mem, func: u32, ints: &[u32], floats: &[f32]) -> u32 {
        let mut v = ints.to_vec();
        v.extend(floats.iter().map(|f| f.to_bits()));
        match func {
            FONT_SET_COLOR_V => {
                // Copy 4 floats from src (ints[1]) to dst (ints[0]) as Font_SetColorV does
                for k in 0..4u32 {
                    let w = m.r32(ints[1] + 4 * k);
                    m.w32(ints[0] + 4 * k, w);
                }
                self.log.borrow_mut().push((func, v));
                0
            }
            SPRITE_SET_GET_RECT => {
                self.log.borrow_mut().push((func, v));
                self.rect_ret
            }
            LOC_GET_STRING => {
                self.log.borrow_mut().push((func, v));
                self.loc_ret
            }
            WSTR_TO_ASCII => {
                // Write a short sentinel ASCII string at buf (ints[0])
                m.w8(ints[0], b'L');
                m.w8(ints[0] + 1, 0);
                self.log.borrow_mut().push((func, v));
                0
            }
            STRCPY => {
                // Log only addresses + write sentinel byte so buf mutation is observed identically
                m.w8(ints[0], b'S');
                m.w8(ints[0] + 1, 0);
                self.log.borrow_mut().push((func, v));
                ints[0]
            }
            FONT_MEASURE_ASCII => {
                v.push(m.r8(ints[1]) as u32);
                self.log.borrow_mut().push((func, v));
                self.meas_ret
            }
            FONT_PRINTF_SHADOWED => {
                if ints[1] >= 0x01fe_0000 {
                    v.push(m.r8(ints[1]) as u32);
                }
                self.log.borrow_mut().push((func, v));
                0
            }
            INPUT_GET_DEVICE => {
                self.log.borrow_mut().push((func, v));
                self.dev_ret
            }
            _ => {
                self.log.borrow_mut().push((func, v));
                0
            }
        }
    }
}

struct RecFe(Log);
impl FeEnv for RecFe {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.0.borrow_mut().push((func, args.to_vec()));
        0
    }
}

#[test]
fn mockup_editor_and_snow_program_destruct_match_the_original() {
    let Some(mut r) = Runner::from_env() else { return };
    r.float_rules = r5900::ps2float::Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    // Allow internal MockupEditor functions to run for real when called by each other
    for f in [
        0x1d3b88u32,
        0x1d3b90,
        0x1d3bb0,
        0x1d3c00,
        0x1d3e40,
        0x1d3f30,
        0x1d41c0,
        0x1d41c8,
        0x1d41d0,
        0x1d42f0,
        0x1d4410,
        0x1d4538,
        0x1d4660,
        0x1d4790,
        0x1d47c0,
        0x1d4ce0,
        0x1d4cf0,
        0x1d4d00,
        0x1d4d08,
        0x1d4d78,
    ] {
        r.allow(f);
    }

    let base = r.mem.alloc(0x2400);
    let (app, ren, vt, dev, ed, col_buf, snow_p) = (
        base,
        base + 0x800,
        base + 0x900,
        base + 0xb00,
        base + 0xc00,
        base + 0x2000,
        base + 0x2100,
    );
    let log: Log = Rc::new(RefCell::new(vec![]));
    let rect_ret: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let loc_ret: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let meas_ret: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

    {
        let l2 = log.clone();
        r.stub(MEM_DELETE, move |c, _| {
            l2.borrow_mut().push((MEM_DELETE, vec![c.gpr(4) as u32]));
        });
    }
    {
        let l2 = log.clone();
        r.stub(REN_1D4, move |c, _| {
            l2.borrow_mut().push((REN_1D4, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
        });
    }
    {
        let l2 = log.clone();
        r.stub(FONT_SET_COLOR_V, move |c, mem| {
            let (dst, src) = (c.gpr(4) as u32, c.gpr(5) as u32);
            for k in 0..4u32 {
                let w = mem.read_u32(src + 4 * k);
                mem.write_u32(dst + 4 * k, w);
            }
            l2.borrow_mut().push((FONT_SET_COLOR_V, vec![dst, src]));
        });
    }
    {
        let l2 = log.clone();
        let rr = rect_ret.clone();
        r.stub(SPRITE_SET_GET_RECT, move |c, _| {
            l2.borrow_mut().push((SPRITE_SET_GET_RECT, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, *rr.borrow() as u64);
        });
    }
    {
        let l2 = log.clone();
        r.stub(SPRITE_DRAW_RECT_SCALED, move |c, _| {
            l2.borrow_mut().push((
                SPRITE_DRAW_RECT_SCALED,
                vec![
                    c.gpr(4) as u32,
                    c.gpr(5) as u32,
                    c.f(12).to_bits(),
                    c.f(13).to_bits(),
                    c.f(14).to_bits(),
                    c.f(15).to_bits(),
                ],
            ));
        });
    }
    {
        let l2 = log.clone();
        let lr = loc_ret.clone();
        r.stub(LOC_GET_STRING, move |c, _| {
            l2.borrow_mut().push((LOC_GET_STRING, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, *lr.borrow() as u64);
        });
    }
    {
        let l2 = log.clone();
        r.stub(WSTR_TO_ASCII, move |c, mem| {
            let (dst, src) = (c.gpr(4) as u32, c.gpr(5) as u32);
            mem.write_u8(dst, b'L');
            mem.write_u8(dst + 1, 0);
            l2.borrow_mut().push((WSTR_TO_ASCII, vec![dst, src]));
        });
    }
    {
        let l2 = log.clone();
        r.stub(STRCPY, move |c, mem| {
            let (dst, src) = (c.gpr(4) as u32, c.gpr(5) as u32);
            mem.write_u8(dst, b'S');
            mem.write_u8(dst + 1, 0);
            l2.borrow_mut().push((STRCPY, vec![dst, src]));
            c.set_gpr(2, dst as u64);
        });
    }
    {
        let l2 = log.clone();
        let mr = meas_ret.clone();
        r.stub(FONT_MEASURE_ASCII, move |c, mem| {
            let mut b = [0u8; 1];
            mem.read(c.gpr(5) as u32, &mut b).unwrap();
            let b0 = b[0] as u32;
            l2.borrow_mut().push((
                FONT_MEASURE_ASCII,
                vec![c.gpr(4) as u32, c.gpr(5) as u32, c.gpr(6) as u32, c.f(12).to_bits(), b0],
            ));
            c.set_gpr(2, *mr.borrow() as i32 as i64 as u64);
        });
    }
    {
        let l2 = log.clone();
        r.stub(FONT_PRINTF_SHADOWED, move |c, mem| {
            let mut v = vec![
                c.gpr(4) as u32,
                c.gpr(5) as u32,
                c.f(12).to_bits(),
                c.f(13).to_bits(),
                c.f(14).to_bits(),
            ];
            if (c.gpr(5) as u32) >= 0x01fe_0000 {
                let mut b = [0u8; 1];
                mem.read(c.gpr(5) as u32, &mut b).unwrap();
                v.push(b[0] as u32);
            }
            l2.borrow_mut().push((FONT_PRINTF_SHADOWED, v));
        });
    }
    {
        let l2 = log.clone();
        r.stub(INPUT_GET_DEVICE, move |c, _| {
            l2.borrow_mut().push((INPUT_GET_DEVICE, vec![c.gpr(4) as u32, c.gpr(5) as u32]));
            c.set_gpr(2, dev as u64);
        });
    }

    let mut g = Rng(0x1d47_f0cc_cafe_0001);
    for case in 0..2_600u32 {
        let mut init_mem = vec![0u8; 0x2400];
        for b in &mut init_mem {
            *b = (g.next() & 0xff) as u8;
        }
        r.mem.write(base, &init_mem).unwrap();
        r.mem.write_u32(GAPP, app);
        r.mem.write_u32(app + 0x24, g.next());
        r.mem.write_u32(app + 0x50, g.next());
        r.mem.write_u32(app + 0x5c, g.next());
        r.mem.write_u32(app + 0x60, g.next());
        r.mem.write_u32(app + 0x724, ren);
        r.mem.write_u32(app + 0x728, g.next());
        r.mem.write_u32(ren + 0x10, vt);
        r.mem.write_u16(vt + 0x1d0, ((g.below(11) as i16) - 5) as u16);
        r.mem.write_u32(vt + 0x1d4, REN_1D4);

        // Reasonable editor state (count 0..=4, cur 0..count, valid item floats)
        let count = g.below(5);
        let cur_idx = if count == 0 { 0 } else { g.below(count) };
        r.mem.write_u32(ed, cur_idx);
        r.mem.write_u32(ed + 4, count);
        r.mem.write_u32(ed + 0x1098, g.below(2));
        for i in 0..5u32 {
            let it = ed + 8 + i * ITEM_STRIDE;
            r.mem.write_u32(it + 0x14, g.below(5));
            r.mem.write_u32(it + 0x18, g.f(-200.0, 640.0).to_bits());
            r.mem.write_u32(it + 0x1c, g.f(-200.0, 480.0).to_bits());
            r.mem.write_u32(it + 0x20, g.f(0.1, 3.0).to_bits());
            r.mem.write_u32(it + 0x28, g.below(4));
            for c_i in 0..4u32 {
                r.mem.write_u32(it + 0x2c + 4 * c_i, g.f(0.0, 1.0).to_bits());
            }
            let sx = if g.below(4) == 0 { -1.0 } else { g.f(0.2, 4.0) };
            let sy = if g.below(4) == 0 { -1.0 } else { g.f(0.2, 4.0) };
            r.mem.write_u32(it + 0xc8, sx.to_bits());
            r.mem.write_u32(it + 0xcc, sy.to_bits());
            r.mem.write_u32(it + 0xd0, g.below(8));
        }

        // Pad prev & cur covering all combinations of 0x1..0x2400
        let prev_pad = g.next() & 0x3fff;
        let cur_pad = g.next() & 0x3fff;
        r.mem.write_u32(dev + 0xc, prev_pad);
        r.mem.write_u32(dev + 0x10, cur_pad);

        // Globals FONT_COLOR (16 bytes) and BLINK_COUNTER (4 bytes)
        for k in 0..4u32 {
            r.mem.write_u32(FONT_COLOR + 4 * k, g.f(0.0, 1.0).to_bits());
        }
        r.mem.write_u32(BLINK_COUNTER, g.below(45));

        let rr = g.next();
        let lr = g.next();
        let mr = g.below(500);
        *rect_ret.borrow_mut() = rr;
        *loc_ret.borrow_mut() = lr;
        *meas_ret.borrow_mut() = mr;

        // Snapshot into our Mem
        let mut m = Mem::default();
        let mut snap = vec![0u8; 0x2400];
        r.mem.read(base, &mut snap).unwrap();
        m.regions.push((base, snap));
        let mut gb = vec![0u8; 4];
        r.mem.read(GAPP, &mut gb).unwrap();
        m.regions.push((GAPP, gb));
        let mut fc = vec![0u8; 16];
        r.mem.read(FONT_COLOR, &mut fc).unwrap();
        m.regions.push((FONT_COLOR, fc));
        let mut bc = vec![0u8; 4];
        r.mem.read(BLINK_COUNTER, &mut bc).unwrap();
        m.regions.push((BLINK_COUNTER, bc));

        let which = case % 26;
        let how = g.below(4);
        let item0 = ed + 8;
        let col_ptr = col_buf + g.below(8); // exercise unaligned colour pointers
        let name = g.next();
        let text = g.next();
        let sprite_idx = g.below(100);
        let loc_id = g.below(1000);
        let font = g.next();
        let flags = g.next();
        let align = g.below(4);
        let axis = g.below(8);
        let sel = g.below(2);
        let mode = g.below(3);
        let (x, y, sx, sy) = (g.f(-50.0, 500.0), g.f(-50.0, 500.0), g.f(0.1, 4.0), g.f(0.1, 4.0));

        log.borrow_mut().clear();
        let ret_theirs = match which {
            0 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.call(0x1d3b88).unwrap();
                r.cpu.gpr(2) as u32
            }
            1 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x1d3b90).unwrap();
                0
            }
            2 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.call(0x1d3bb0).unwrap();
                0
            }
            3 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.cpu.set_gpr(5, sel as u64);
                r.call(0x1d3c00).unwrap();
                0
            }
            4 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.call(0x1d3e40).unwrap();
                0
            }
            5 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.call(0x1d3f30).unwrap();
                0
            }
            6 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.cpu.set_gpr(5, axis as u64);
                r.call(0x1d41c0).unwrap();
                0
            }
            7 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.call(0x1d41c8).unwrap();
                0
            }
            8 => {
                r.cpu.set_gpr(4, item0 as u64);
                r.call(0x1d41d0).unwrap();
                0
            }
            9 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d41d8).unwrap();
                r.cpu.gpr(2) as u32
            }
            10 => {
                r.cpu.set_gpr(4, ed as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x1d4258).unwrap();
                0
            }
            11 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d42f0).unwrap();
                0
            }
            12 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d4338).unwrap();
                0
            }
            13 => {
                r.cpu.set_gpr(4, ed as u64);
                r.cpu.set_gpr(5, name as u64);
                r.cpu.set_gpr(6, sprite_idx as u64);
                r.cpu.set_gpr(7, col_ptr as u64);
                r.cpu.set_gpr(8, flags as u64);
                r.cpu.set_f(12, x);
                r.cpu.set_f(13, y);
                r.cpu.set_f(14, sx);
                r.cpu.set_f(15, sy);
                r.call(0x1d4410).unwrap();
                0
            }
            14 => {
                r.cpu.set_gpr(4, ed as u64);
                r.cpu.set_gpr(5, name as u64);
                r.cpu.set_gpr(6, loc_id as u64);
                r.cpu.set_gpr(7, font as u64);
                r.cpu.set_gpr(8, col_ptr as u64);
                r.cpu.set_gpr(9, align as u64);
                r.cpu.set_f(12, x);
                r.cpu.set_f(13, y);
                r.cpu.set_f(14, sx);
                r.call(0x1d4538).unwrap();
                0
            }
            15 => {
                r.cpu.set_gpr(4, ed as u64);
                r.cpu.set_gpr(5, name as u64);
                r.cpu.set_gpr(6, text as u64);
                r.cpu.set_gpr(7, font as u64);
                r.cpu.set_gpr(8, col_ptr as u64);
                r.cpu.set_gpr(9, align as u64);
                r.cpu.set_f(12, x);
                r.cpu.set_f(13, y);
                r.cpu.set_f(14, sx);
                r.call(0x1d4660).unwrap();
                0
            }
            16 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d4790).unwrap();
                0
            }
            17 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d47c0).unwrap();
                0
            }
            18 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d47f0).unwrap();
                0
            }
            19 => {
                r.cpu.set_gpr(4, ed as u64);
                r.cpu.set_f(12, x);
                r.cpu.set_f(13, y);
                r.call(0x1d4ce0).unwrap();
                0
            }
            20 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d4cf0).unwrap();
                0
            }
            21 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d4d00).unwrap();
                0
            }
            22 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d4d08).unwrap();
                0
            }
            23 => {
                r.cpu.set_gpr(4, ed as u64);
                r.call(0x1d4d78).unwrap();
                0
            }
            24 => {
                r.cpu.set_gpr(4, ed as u64);
                r.cpu.set_gpr(5, mode as u64);
                r.call(0x1d4de8).unwrap();
                0
            }
            _ => {
                r.cpu.set_gpr(4, snow_p as u64);
                r.cpu.set_gpr(5, how as u64);
                r.call(0x21e738).unwrap();
                0
            }
        };
        let sp = r.cpu.gpr(29) as u32;
        let theirs_log = std::mem::take(&mut *log.borrow_mut());
        let mut theirs_mem = vec![0u8; 0x2400];
        r.mem.read(base, &mut theirs_mem).unwrap();
        let mut theirs_fc = vec![0u8; 16];
        r.mem.read(FONT_COLOR, &mut theirs_fc).unwrap();
        let theirs_bc = r.mem.read_u32(BLINK_COUNTER);

        m.regions.push((sp.wrapping_sub(0x200), vec![0u8; 0x200]));
        let mut env = RecMockup { log: log.clone(), rect_ret: rr, loc_ret: lr, meas_ret: mr, dev_ret: dev };
        let ret_mine = match which {
            0 => entry_construct(item0),
            1 => {
                entry_destruct(&mut m, &mut env, item0, how);
                0
            }
            2 => {
                item_init(&mut m, item0);
                0
            }
            3 => {
                draw_item_info(&mut m, &mut env, item0, sel, sp);
                0
            }
            4 => {
                item_increase(&mut m, item0);
                0
            }
            5 => {
                item_decrease(&mut m, item0);
                0
            }
            6 => {
                item_set_axis(&mut m, item0, axis);
                0
            }
            7 => {
                item_stub_a();
                0
            }
            8 => {
                item_stub_b();
                0
            }
            9 => init(&mut m, ed),
            10 => {
                delete(&mut m, &mut env, ed, how);
                0
            }
            11 => {
                reset(&mut m, ed);
                0
            }
            12 => {
                draw(&mut m, &mut env, ed, sp);
                0
            }
            13 => {
                add_sprite(&mut m, &mut env, ed, name, sprite_idx, col_ptr, flags, x, y, sx, sy);
                0
            }
            14 => {
                add_loc_text(&mut m, &mut env, ed, name, loc_id, font, col_ptr, align, x, y, sx);
                0
            }
            15 => {
                add_item(&mut m, &mut env, ed, name, text, font, col_ptr, align, x, y, sx);
                0
            }
            16 => {
                next_item(&mut m, ed);
                0
            }
            17 => {
                prev_item(&mut m, ed);
                0
            }
            18 => {
                update(&mut m, &mut env, ed);
                0
            }
            19 => {
                set_target(&mut m, ed, x, y);
                0
            }
            20 => {
                open(&mut m, ed);
                0
            }
            21 => {
                close(&mut m, ed);
                0
            }
            22 => {
                for_each_a(&m, ed);
                0
            }
            23 => {
                for_each_b(&m, ed);
                0
            }
            24 => {
                draw_items(&mut m, &mut env, ed, mode, sp);
                0
            }
            _ => {
                assert_eq!(COPIES_MEM_DELETE, MEM_DELETE);
                let mut fe = RecFe(log.clone());
                snow_program_destruct(&mut m, &mut fe, snow_p, how);
                0
            }
        };
        let mine_log = std::mem::take(&mut *log.borrow_mut());

        assert_eq!(ret_theirs, ret_mine, "case {case} which {which}: return mismatch");
        assert_eq!(theirs_log, mine_log, "case {case} which {which}: call log mismatch");
        assert_eq!(&theirs_mem, &m.regions[0].1, "case {case} which {which}: memory mismatch");
        assert_eq!(&theirs_fc, &m.regions[2].1, "case {case} which {which}: FONT_COLOR mismatch");
        assert_eq!(theirs_bc, m.r32(BLINK_COUNTER), "case {case} which {which}: BLINK_COUNTER mismatch");
    }
}
