//! Function-runner check for row G10r (17 `audio` `AudioTGroups` & `WorldTriggerMgr` functions):
//! `AudioTGroups_Init` 0x22a470, `AudioTGroups_Delete` 0x22a4c8, `AudioTGroups_Load` 0x22a518,
//! `AudioTGroups_Free` 0x22a588, `AudioTGroups_BuildGrid` 0x22a6d8, `AudioTGroup_GetShapeBounds` 0x22acc0,
//! `WorldInstance_GetSoundKeys` 0x22baf8, `WorldTriggerMgr_Destroy` 0x22be88, `WorldEmitter_Construct` 0x22bec0,
//! `WorldTriggerMgr_Init` 0x22d098, `WorldTriggerMgr_Destruct` 0x22d168, `WorldEmitter_Acquire` 0x22d260,
//! `WorldTriggerMgr_StopKey` 0x22d480, `WorldEmitter_MarkPlayed` 0x22e2d0, `WorldTriggerMgr_Reset` 0x22e360,
//! `SoundId_IsAmbient` 0x22e388, `SoundId_IsAmbientExtra` 0x22e3f0.

use r5900::ps2float::Rules;
use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::worldemitter::{self as we, WorldTriggerEnv};

const BASE: u32 = 0x0140_0000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        let u = (self.next() >> 8) as f32 / (1u32 << 24) as f32;
        lo + (hi - lo) * u
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    r.float_rules = Rules::parse("add=n,mul=n,div=n,cvt=n").unwrap();
    for a in [
        0x137140, // Vec3_Dot
        0x22a588, // AudioTGroups_Free
        0x22a6d8, // AudioTGroups_BuildGrid
        0x22bec0, // WorldEmitter_Construct
        0x22bf30, // WorldEmitter_Matches
        0x22d168, // WorldTriggerMgr_Destruct
        0x22d188, // WorldEmitter_FindFree
        0x22d1b8, // WorldEmitter_Find
        0x22e270, // WorldEmitter_WasPlayed
        0x22e3f0, // SoundId_IsAmbientExtra
        0x2cc808, // EAMem_Set
        0x2ce114, // EAMem_Copy
    ] {
        r.allow(a);
    }
    Some(r)
}

fn call_fn(r: &mut Runner, addr: u32, args: &[u32]) -> u32 {
    for (k, &v) in args.iter().enumerate() {
        r.cpu.set_gpr(4 + k, v as i32 as i64 as u64);
    }
    r.call(addr).unwrap();
    r.cpu.gpr(2) as u32
}

fn fresh_mem(r: &mut Runner) -> Mem {
    let z = vec![0u8; 0x40000];
    r.mem.write(BASE, &z);
    Mem {
        regions: vec![
            (0x0034_0000, vec![0u8; 0x0001_0000]),
            (BASE, vec![0u8; 0x40000]),
        ],
    }
}

fn mem_slice(m: &Mem, at: u32, len: usize) -> Vec<u8> {
    (0..len).map(|i| m.r8(at + i as u32)).collect()
}

fn copy_out(r: &mut Runner, at: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    r.mem.read(at, &mut b).unwrap();
    b
}

#[derive(Default)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
    alloc_cursor: u32,
    alloc_returns_null_on: Option<usize>,
    alloc_count: usize,
    file_exists: u32,
    file_size: u32,
    file_data: u32,
    add_nodes_per_call: u32,
}

impl EnvState {
    fn new(alloc_start: u32) -> Self {
        Self {
            alloc_cursor: alloc_start,
            ..Default::default()
        }
    }

    fn do_alloc(&mut self, sz: u32) -> u32 {
        let idx = self.alloc_count;
        self.alloc_count += 1;
        if self.alloc_returns_null_on == Some(idx) {
            return 0;
        }
        let p = self.alloc_cursor;
        let step = (sz.max(16) + 15) & !15;
        self.alloc_cursor = self.alloc_cursor.wrapping_add(step);
        p
    }
}

impl WorldTriggerEnv for EnvState {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        self.calls.push((target, args.to_vec()));
        match target {
            we::EAMEM_ALLOC => self.do_alloc(args[1]),
            we::FILESYS_EXISTS => self.file_exists,
            we::FILE_GET_SIZE => self.file_size,
            we::FILE_LOAD => self.file_data,
            we::AUDIO_TGROUPS_ADD_TO_GRID => {
                // Simulate inserting nodes into the temp pool and linking cell heads
                let this = args[0];
                let entry = args[1];
                let tmp = m.r32(this + 0x2c);
                let nx = m.r32(this + 0x10);
                let ny = m.r32(this + 0x14);
                let cells = m.r32(this + 0x18);
                let cap = m.r32(this + 0x28);
                for k in 0..self.add_nodes_per_call {
                    let used = m.r32(this + 0x30);
                    if used >= cap || nx == 0 || ny == 0 {
                        break;
                    }
                    let node = tmp.wrapping_add(used << 3);
                    let cell_idx = (used.wrapping_add(k)) % (nx * ny);
                    let cell = cells.wrapping_add(cell_idx * 0x14);
                    let prev_head = m.r32(cell + 0x10);
                    m.w32(node, entry);
                    m.w32(node + 4, prev_head);
                    m.w32(cell + 0x10, node);
                    m.w32(this + 0x30, used + 1);
                }
                0
            }
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    let stubs: &[(u32, usize)] = &[
        (we::MEM_DELETE, 1),
        (we::MEM_FREE, 1),
        (we::EAMEM_ALLOC, 3),
        (we::EAMEM_FREE, 1),
        (we::FILESYS_EXISTS, 1),
        (we::FILE_GET_SIZE, 1),
        (we::FILE_LOAD, 2),
        (we::AUDIO_TGROUPS_BIND, 1),
        (we::AUDIO_TGROUPS_ADD_TO_GRID, 2),
        (we::WORLD_EMITTER_STOP, 1),
    ];
    for &(addr, nargs) in stubs {
        let s = st.clone();
        r.stub(addr, move |cpu, mem| {
            let mut a = Vec::with_capacity(nargs);
            for k in 0..nargs {
                a.push(cpu.gpr(4 + k) as u32);
            }
            let mut e = s.borrow_mut();
            e.calls.push((addr, a.clone()));
            let ret = match addr {
                we::EAMEM_ALLOC => e.do_alloc(a[1]),
                we::FILESYS_EXISTS => e.file_exists,
                we::FILE_GET_SIZE => e.file_size,
                we::FILE_LOAD => e.file_data,
                we::AUDIO_TGROUPS_ADD_TO_GRID => {
                    let this = a[0];
                    let entry = a[1];
                    let tmp = mem.read_u32(this + 0x2c);
                    let nx = mem.read_u32(this + 0x10);
                    let ny = mem.read_u32(this + 0x14);
                    let cells = mem.read_u32(this + 0x18);
                    let cap = mem.read_u32(this + 0x28);
                    for k in 0..e.add_nodes_per_call {
                        let used = mem.read_u32(this + 0x30);
                        if used >= cap || nx == 0 || ny == 0 {
                            break;
                        }
                        let node = tmp.wrapping_add(used << 3);
                        let cell_idx = (used.wrapping_add(k)) % (nx * ny);
                        let cell = cells.wrapping_add(cell_idx * 0x14);
                        let prev_head = mem.read_u32(cell + 0x10);
                        mem.write_u32(node, entry);
                        mem.write_u32(node + 4, prev_head);
                        mem.write_u32(cell + 0x10, node);
                        mem.write_u32(this + 0x30, used + 1);
                    }
                    0
                }
                _ => 0,
            };
            cpu.set_gpr(2, ret as i32 as i64 as u64);
        });
    }
}

#[test]
fn audio_tgroups_functions_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x610a_7601);

    // 1. AudioTGroups_Init 0x22a470
    for _ in 0..100 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        for off in (0..0x40).step_by(4) {
            let v = rng.next();
            m.w32(this + off, v);
            r.mem.write_u32(this + off, v);
        }
        let ret_rs = we::tgroups_init(&mut m, this);
        let ret_ps = call_fn(&mut r, 0x22a470, &[this]);
        assert_eq!(ret_rs, ret_ps);
        assert_eq!(mem_slice(&m, this, 0x40), copy_out(&mut r, this, 0x40));
    }

    // 2. AudioTGroups_Free 0x22a588 & AudioTGroups_Delete 0x22a4c8
    for mask in 0..8u32 {
        for flags in 0..4u32 {
            let mut m = fresh_mem(&mut r);
            let this = BASE + 0x100;
            let p0 = if (mask & 1) != 0 { BASE + 0x1000 } else { 0 };
            let p1 = if (mask & 2) != 0 { BASE + 0x2000 } else { 0 };
            let p2 = if (mask & 4) != 0 { BASE + 0x3000 } else { 0 };
            for &(off, v) in &[(0, p0), (0x18, p1), (0x2c, p2)] {
                m.w32(this + off, v);
                r.mem.write_u32(this + off, v);
            }
            let st_ps = Rc::new(RefCell::new(EnvState::new(BASE + 0x4000)));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState::new(BASE + 0x4000);
            we::tgroups_delete(&mut m, &mut st_rs, this, flags);
            call_fn(&mut r, 0x22a4c8, &[this, flags]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls, "delete mask={mask} flags={flags}");
            assert_eq!(mem_slice(&m, this, 0x34), copy_out(&mut r, this, 0x34));
        }
    }

    // 3. AudioTGroups_BuildGrid 0x22a6d8 & AudioTGroups_Load 0x22a518
    for case in 0..120 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x100;
        let grid = BASE + 0x200;
        let entries = BASE + 0x300;
        let nx = (rng.next() % 5) as u32;
        let ny = (rng.next() % 5) as u32;
        let ox = rng.f(-5000.0, 5000.0).to_bits();
        let oy = rng.f(-5000.0, 5000.0).to_bits();
        let csz = rng.f(50.0, 1000.0).to_bits();
        let count = if case % 4 == 0 { 0 } else { (rng.next() % 8) + 1 };

        let setup = |mem_w: &mut dyn FnMut(u32, u32)| {
            mem_w(we::G_WORLD_GRID, grid);
            mem_w(grid + 4, ox);
            mem_w(grid + 8, oy);
            mem_w(grid + 0x28, csz);
            mem_w(grid + 0x2c, nx);
            mem_w(grid + 0x30, ny);
            mem_w(this + 8, count);
            mem_w(this + 0xc, entries);
        };
        setup(&mut |a, v| m.w32(a, v));
        setup(&mut |a, v| r.mem.write_u32(a, v));

        let alloc_base = BASE + 0x1000;
        let mut st_rs = EnvState::new(alloc_base);
        st_rs.add_nodes_per_call = (rng.next() % 3) as u32;
        st_rs.file_exists = (case & 1) as u32;
        st_rs.file_size = 0x1234 + case;
        st_rs.file_data = BASE + 0x30000;
        if case % 11 == 0 && nx == 0 && ny == 0 {
            st_rs.alloc_returns_null_on = Some(0);
        }

        let st_ps = Rc::new(RefCell::new(EnvState {
            calls: vec![],
            alloc_cursor: alloc_base,
            alloc_returns_null_on: st_rs.alloc_returns_null_on,
            alloc_count: 0,
            file_exists: st_rs.file_exists,
            file_size: st_rs.file_size,
            file_data: st_rs.file_data,
            add_nodes_per_call: st_rs.add_nodes_per_call,
        }));
        install_stubs(&mut r, st_ps.clone());

        if case % 2 == 0 {
            let ret_rs = we::tgroups_build_grid(&mut m, &mut st_rs, this);
            let ret_ps = call_fn(&mut r, 0x22a6d8, &[this]);
            assert_eq!(ret_rs, ret_ps);
        } else {
            let path = BASE + 0x80;
            let ret_rs = we::tgroups_load(&mut m, &mut st_rs, this, path);
            let ret_ps = call_fn(&mut r, 0x22a518, &[this, path]);
            assert_eq!(ret_rs, ret_ps);
        }
        assert_eq!(st_rs.calls, st_ps.borrow().calls, "grid case {case}");
        assert_eq!(mem_slice(&m, this, 0x34), copy_out(&mut r, this, 0x34), "this case {case}");
        let used_alloc = (st_rs.alloc_cursor - alloc_base) as usize;
        assert_eq!(
            mem_slice(&m, alloc_base, used_alloc),
            copy_out(&mut r, alloc_base, used_alloc),
            "alloc heap case {case}"
        );
    }

    // 4. AudioTGroup_GetShapeBounds 0x22acc0
    for case in 0..1000 {
        let mut m = fresh_mem(&mut r);
        let shape = BASE + 0x100;
        let out_min = BASE + 0x200;
        let out_max = BASE + 0x220;
        let kind = match case % 6 {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 1,
            _ => 4 + (rng.next() % 5),
        };
        let mut vals = vec![(0u32, kind)];
        for off in [8, 0xc, 0x10, 0x14, 0x18, 0x1c] {
            vals.push((off, rng.f(-1000.0, 1000.0).to_bits()));
        }
        // Non-vertical u vector for kind 1 so (0,0,1) x u != 0
        let mut ux = rng.f(-1.0, 1.0);
        let uy = rng.f(-1.0, 1.0);
        if ux.abs() < 0.05 && uy.abs() < 0.05 {
            ux = 0.5;
        }
        let uz = rng.f(-1.0, 1.0);
        vals.push((0x20, ux.to_bits()));
        vals.push((0x24, uy.to_bits()));
        vals.push((0x28, uz.to_bits()));

        for &(off, v) in &vals {
            m.w32(shape + off, v);
            r.mem.write_u32(shape + off, v);
        }
        for off in [0, 4, 8] {
            let sentinel = rng.next();
            m.w32(out_min + off, sentinel);
            r.mem.write_u32(out_min + off, sentinel);
            let sentinel2 = rng.next();
            m.w32(out_max + off, sentinel2);
            r.mem.write_u32(out_max + off, sentinel2);
        }
        we::tgroup_get_shape_bounds(&mut m, shape, out_min, out_max);
        call_fn(&mut r, 0x22acc0, &[BASE, shape, out_min, out_max]);
        assert_eq!(mem_slice(&m, out_min, 12), copy_out(&mut r, out_min, 12), "min case {case} kind {kind}");
        assert_eq!(mem_slice(&m, out_max, 12), copy_out(&mut r, out_max, 12), "max case {case} kind {kind}");
    }

    // 5. WorldInstance_GetSoundKeys 0x22baf8
    for case in 0..500 {
        let mut m = fresh_mem(&mut r);
        let inst = BASE + 0x100;
        let p1 = BASE + 0x300;
        let p2 = BASE + 0x400;
        let outs = [BASE + 0x800, BASE + 0x804, BASE + 0x808, BASE + 0x80c];
        let n = (rng.next() % 5) as u32;
        m.w32(inst + 0xf4, p1);
        r.mem.write_u32(inst + 0xf4, p1);
        m.w32(p1 + 4, p2);
        r.mem.write_u32(p1 + 4, p2);
        m.w32(p2 + 4, n);
        r.mem.write_u32(p2 + 4, n);

        let mut cur = p2 + 8;
        for i in 0..n {
            // Keep first shape kind in 0..=3 so $t2 is always initialized before store
            let kind = if i == 0 { rng.next() % 4 } else { rng.next() % 5 };
            let key = rng.next();
            m.w32(cur, kind);
            m.w32(cur + 4, key);
            r.mem.write_u32(cur, kind);
            r.mem.write_u32(cur + 4, key);
            cur += match kind {
                0 => 0x1c,
                1 | 2 => 0x30,
                3 => 0x18,
                _ => 0,
            };
        }
        for &o in &outs {
            m.w32(o, 0xdead_beef);
            r.mem.write_u32(o, 0xdead_beef);
        }
        we::world_instance_get_sound_keys(&mut m, inst, outs[0], outs[1], outs[2], outs[3]);
        call_fn(&mut r, 0x22baf8, &[BASE, inst, outs[0], outs[1], outs[2], outs[3]]);
        assert_eq!(mem_slice(&m, outs[0], 16), copy_out(&mut r, outs[0], 16), "keys case {case}");
    }
}

#[test]
fn world_trigger_mgr_and_emitter_functions_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x610a_7602);

    // 6. WorldTriggerMgr_Destruct 0x22d168 & WorldTriggerMgr_Destroy 0x22be88
    for has_ptr in [false, true] {
        for flags in 0..4u32 {
            let mut m = fresh_mem(&mut r);
            let this = BASE + 0x100;
            let st_ps = Rc::new(RefCell::new(EnvState::new(BASE + 0x4000)));
            install_stubs(&mut r, st_ps.clone());
            let mut st_rs = EnvState::new(BASE + 0x4000);
            we::mgr_destruct(&mut m, &mut st_rs, this, flags);
            call_fn(&mut r, 0x22d168, &[this, flags]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);

            let gval = if has_ptr { this } else { 0 };
            m.w32(we::G_WORLD_TRIGGER_MGR, gval);
            r.mem.write_u32(we::G_WORLD_TRIGGER_MGR, gval);
            st_rs.calls.clear();
            st_ps.borrow_mut().calls.clear();
            we::mgr_destroy(&mut m, &mut st_rs);
            call_fn(&mut r, 0x22be88, &[]);
            assert_eq!(st_rs.calls, st_ps.borrow().calls);
            assert_eq!(m.r32(we::G_WORLD_TRIGGER_MGR), r.mem.read_u32(we::G_WORLD_TRIGGER_MGR));
        }
    }

    // 7. WorldEmitter_Construct 0x22bec0
    for _ in 0..200 {
        let mut m = fresh_mem(&mut r);
        let rec = BASE + 0x100;
        for off in (0..0x50).step_by(4) {
            let v = rng.next();
            m.w32(rec + off, v);
            r.mem.write_u32(rec + off, v);
        }
        let ret_rs = we::world_emitter_construct(&mut m, rec);
        let ret_ps = call_fn(&mut r, 0x22bec0, &[rec]);
        assert_eq!(ret_rs, ret_ps);
        assert_eq!(mem_slice(&m, rec, 0x50), copy_out(&mut r, rec, 0x50));
    }

    // 8. WorldTriggerMgr_Init 0x22d098 & WorldTriggerMgr_Reset 0x22e360
    {
        let mut m = fresh_mem(&mut r);
        let mgr = BASE + 0x100;
        let alloc_base = BASE + 0x8000;
        let st_ps = Rc::new(RefCell::new(EnvState::new(alloc_base)));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::new(alloc_base);
        let ret_rs = we::mgr_init(&mut m, &mut st_rs, mgr);
        let ret_ps = call_fn(&mut r, 0x22d098, &[mgr]);
        assert_eq!(ret_rs, ret_ps);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(mem_slice(&m, mgr, 0x4448), copy_out(&mut r, mgr, 0x4448));
        assert_eq!(mem_slice(&m, alloc_base, 0xc90), copy_out(&mut r, alloc_base, 0xc90));

        // Corrupt +0x442c..+0x4448 and check WorldTriggerMgr_Reset 0x22e360
        for off in (0x442c..0x4448).step_by(4) {
            let v = rng.next();
            m.w32(mgr + off, v);
            r.mem.write_u32(mgr + off, v);
        }
        we::mgr_reset(&mut m, mgr);
        call_fn(&mut r, 0x22e360, &[mgr]);
        assert_eq!(mem_slice(&m, mgr + 0x442c, 0x1c), copy_out(&mut r, mgr + 0x442c, 0x1c));
    }

    // 9. WorldEmitter_Acquire 0x22d260 & WorldTriggerMgr_StopKey 0x22d480
    {
        let mut m = fresh_mem(&mut r);
        let mgr = BASE + 0x100;
        let alloc_base = BASE + 0x8000;
        let pos_ptr = BASE + 0x40;
        let st_ps = Rc::new(RefCell::new(EnvState::new(alloc_base)));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::new(alloc_base);
        we::mgr_init(&mut m, &mut st_rs, mgr);
        call_fn(&mut r, 0x22d098, &[mgr]);

        for step in 0..300 {
            let enabled = if step % 20 == 0 { 0 } else { 1 };
            m.w32(mgr + 0x443c, enabled);
            r.mem.write_u32(mgr + 0x443c, enabled);

            let k1 = rng.next() % 12;
            let k2 = rng.next() % 3;
            let k3 = rng.next() % 2;
            let sound_key = rng.next() % 8;
            let is_3d = rng.next() % 2;
            let vol = rng.f(0.0, 1.0);
            for c in 0..3 {
                let f = rng.f(-2000.0, 2000.0).to_bits();
                m.w32(pos_ptr + c * 4, f);
                r.mem.write_u32(pos_ptr + c * 4, f);
            }

            we::world_emitter_acquire(&mut m, mgr, k1, k2, k3, sound_key, is_3d, pos_ptr, vol);
            r.cpu.set_gpr(4, mgr as i32 as i64 as u64);
            r.cpu.set_gpr(5, k1 as i32 as i64 as u64);
            r.cpu.set_gpr(6, k2 as i32 as i64 as u64);
            r.cpu.set_gpr(7, k3 as i32 as i64 as u64);
            r.cpu.set_gpr(8, sound_key as i32 as i64 as u64);
            r.cpu.set_gpr(9, is_3d as i32 as i64 as u64);
            r.cpu.set_gpr(10, pos_ptr as i32 as i64 as u64);
            r.cpu.fpr[12] = vol.to_bits();
            r.call(0x22d260).unwrap();

            let recs = m.r32(mgr);
            assert_eq!(mem_slice(&m, recs, 40 * 0x50), copy_out(&mut r, recs, 40 * 0x50), "acquire step {step}");

            if step % 5 == 4 {
                let stop_k = rng.next() % 8;
                st_rs.calls.clear();
                st_ps.borrow_mut().calls.clear();
                we::mgr_stop_key(&mut m, &mut st_rs, mgr, stop_k);
                call_fn(&mut r, 0x22d480, &[mgr, stop_k]);
                assert_eq!(st_rs.calls, st_ps.borrow().calls, "stop_key step {step}");
                assert_eq!(mem_slice(&m, recs, 40 * 0x50), copy_out(&mut r, recs, 40 * 0x50), "stop_key mem {step}");
            }
        }
    }

    // 10. WorldEmitter_MarkPlayed 0x22e2d0 (including cache hit/miss and 200-entry saturation)
    {
        let mut m = fresh_mem(&mut r);
        let mgr = BASE + 0x100;
        we::mgr_reset(&mut m, mgr);
        call_fn(&mut r, 0x22e360, &[mgr]);

        for step in 0..350 {
            let id = if step < 260 { rng.next() % 230 } else { rng.next() % 50 };
            we::world_emitter_mark_played(&mut m, mgr, id);
            call_fn(&mut r, 0x22e2d0, &[mgr, id]);
            assert_eq!(
                mem_slice(&m, mgr + 0x410c, 0x33c),
                copy_out(&mut r, mgr + 0x410c, 0x33c),
                "mark_played step {step}"
            );
        }
    }

    // 11. SoundId_IsAmbient 0x22e388 & SoundId_IsAmbientExtra 0x22e3f0 (exhaustive 0..0x200 + random)
    for id in (0..0x200u32).chain((0..500).map(|_| rng.next())) {
        assert_eq!(we::sound_id_is_ambient_extra(id), call_fn(&mut r, 0x22e3f0, &[BASE, id]), "extra id={id:#x}");
        assert_eq!(we::sound_id_is_ambient(id), call_fn(&mut r, 0x22e388, &[BASE, id]), "ambient id={id:#x}");
    }
}
