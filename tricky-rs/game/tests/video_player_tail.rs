//! Function-runner check for row G11a4 (the last 8 `not started` `video` functions, bringing `video` to 0):
//! `VideoStream_FlushFrames` 0x2ba240, `VideoPlayer_DrawFrame` 0x2ba290, `VideoPlayer_OnMpegNoData` 0x2ba408,
//! `VideoPlayer_FeedMpeg` 0x2ba438, `VideoPlayer_Close` 0x2ba998, `cVideoPlayer::~cVideoPlayer` 0x2bab58,
//! `StaticInit_VideoPlayer` 0x2bac00, `StaticDtor_VideoPlayer` 0x2bac40.

use r5900::Runner;
use std::cell::RefCell;
use std::rc::Rc;
use tricky_game::pf::Mem;
use tricky_game::videoman::{self as vm, VideoEnv};

const BASE: u32 = 0x0140_0000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
}

fn runner() -> Option<Runner> {
    let mut r = Runner::from_env()?;
    for a in [
        0x2ba438, // VideoPlayer_FeedMpeg
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
    let z = vec![0u8; 0x30000];
    r.mem.write(BASE, &z);
    Mem {
        regions: vec![
            (0x0033_7000, vec![0u8; 0x1000]),
            (BASE, vec![0u8; 0x30000]),
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

#[derive(Default, Clone)]
struct EnvState {
    calls: Vec<(u32, Vec<u32>)>,
    packets: Vec<u32>,
    finished_after: usize,
    finished_polls: usize,
    frame_snaps: Vec<Vec<u8>>,
}

impl VideoEnv for EnvState {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        self.calls.push((func, args.to_vec()));
        match func {
            vm::STRM_GET_PACKET => {
                if self.packets.is_empty() {
                    0
                } else {
                    self.packets.remove(0)
                }
            }
            vm::STRM_IS_FINISHED => {
                self.finished_polls += 1;
                u32::from(self.finished_polls >= self.finished_after)
            }
            vm::GFX_LOAD_IMAGE_TO_FRAME => {
                let h = args[4] as usize;
                let buf = args[5];
                self.frame_snaps.push(mem_slice(m, buf, h * 16 * 3));
                0
            }
            _ => 0,
        }
    }
}

fn install_stubs(r: &mut Runner, st: Rc<RefCell<EnvState>>) {
    let stubs: &[(u32, usize)] = &[
        (vm::MEM_DELETE, 1),
        (vm::STRM_CLOSE, 1),
        (vm::STRM_GET_PACKET, 1),
        (vm::STRM_RELEASE_PACKET, 2),
        (vm::STRM_IS_FINISHED, 1),
        (vm::EAMEM_FREE, 1),
        (vm::GFX_LOAD_IMAGE_TO_FRAME, 6),
        (vm::SYS_RUN_POLL_TASKS, 1),
        (vm::SND_STREAM_CLOSE, 1),
        (vm::SCE_MPEG_DELETE, 1),
        (vm::MPEG_FEED_DATA, 3),
        (vm::REGISTER_FRAME_INFO_TABLE, 2),
        (vm::DEREGISTER_FRAME_INFO, 1),
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
            let rv = match addr {
                vm::STRM_GET_PACKET => {
                    if e.packets.is_empty() {
                        0
                    } else {
                        e.packets.remove(0)
                    }
                }
                vm::STRM_IS_FINISHED => {
                    e.finished_polls += 1;
                    u32::from(e.finished_polls >= e.finished_after)
                }
                vm::GFX_LOAD_IMAGE_TO_FRAME => {
                    let h = a[4] as usize;
                    let mut buf = vec![0u8; h * 16 * 3];
                    mem.read(a[5], &mut buf).unwrap();
                    e.frame_snaps.push(buf);
                    0
                }
                _ => 0,
            };
            cpu.set_gpr(2, rv as i32 as i64 as u64);
        });
    }
}

#[test]
fn video_player_tail_functions_match() {
    let Some(mut r) = runner() else { return };
    let mut rng = Rng(0x611a_4001);

    // 1. VideoStream_FlushFrames 0x2ba240
    for count in 0..5usize {
        let mut m = fresh_mem(&mut r);
        let pkts: Vec<u32> = (0..count).map(|k| BASE + 0x1000 + k as u32 * 0x40).collect();
        let st_ps = Rc::new(RefCell::new(EnvState { packets: pkts.clone(), ..Default::default() }));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState { packets: pkts, ..Default::default() };
        vm::flush_frames(&mut m, &mut st_rs, BASE + 0x500);
        call_fn(&mut r, 0x2ba240, &[BASE, BASE + 0x500]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }

    // 2. VideoPlayer_DrawFrame 0x2ba290
    for &(fw, fh, mw, mh) in &[
        (0u32, 32, 64, 32),
        (32, 16, 64, 32),
        (64, 32, 48, 16),
        (48, 32, 64, 32),
        (32, 0, 64, 32),
    ] {
        let mut m = fresh_mem(&mut r);
        let app = BASE + 0x100;
        let gfx = BASE + 0x900;
        let this = BASE + 0x200;
        let info = BASE + 0x300;
        let scratch = BASE + 0x1000;
        let src = BASE + 0x8000;
        for &(a, v) in &[
            (vm::GAPP, app),
            (app + 0x724, gfx),
            (this + 0x18, mw),
            (this + 0x1c, mh),
            (this + 0x20, scratch),
            (info, fw),
            (info + 4, fh),
        ] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        let src_bytes: Vec<u8> = (0..0x8000).map(|_| rng.next() as u8).collect();
        for (i, &b) in src_bytes.iter().enumerate() {
            m.w8(src + i as u32, b);
        }
        r.mem.write(src, &src_bytes);

        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();
        vm::player_draw_frame(&mut m, &mut st_rs, this, info, src);
        call_fn(&mut r, 0x2ba290, &[this, info, src]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls, "draw ({fw},{fh},{mw},{mh})");
        assert_eq!(st_rs.frame_snaps, st_ps.borrow().frame_snaps, "snaps ({fw},{fh},{mw},{mh})");
    }

    // 3. VideoPlayer_FeedMpeg 0x2ba438 & VideoPlayer_OnMpegNoData 0x2ba408
    for &(pkt_sz, delay_polls, eof) in &[
        (0x28u32, 0usize, false),
        (0x34, 2, false),
        (0x4c, 1, false),
        (0, 3, true),
    ] {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x200;
        let mpeg = BASE + 0x400;
        let cb = BASE + 0x480;
        let strm = BASE + 0x500;
        let pkt = BASE + 0x1000;
        let feed_buf = BASE + 0x2000;
        let dst = BASE + 0x2000;
        for &(a, v) in &[
            (this + 0x38, strm),
            (this + 0xb8, feed_buf),
            (this + 0xbc, dst),
        ] {
            m.w32(a, v);
            r.mem.write_u32(a, v);
        }
        // Fill dst with sentinel bytes to verify padding zeros
        for i in 0..0x100u32 {
            m.w8(dst + i, 0xaa);
            r.mem.write(dst + i, &[0xaa]);
        }
        let mut packets = vec![0u32; delay_polls];
        if !eof {
            m.w32(pkt + 4, pkt_sz);
            r.mem.write_u32(pkt + 4, pkt_sz);
            for i in 8..pkt_sz {
                let b = rng.next() as u8;
                m.w8(pkt + i, b);
                r.mem.write(pkt + i, &[b]);
            }
            packets.push(pkt);
        }
        let init = EnvState {
            packets,
            finished_after: if eof { delay_polls.max(1) } else { 99 },
            ..Default::default()
        };
        let st_ps = Rc::new(RefCell::new(init.clone()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = init;

        let ret_rs = vm::player_on_mpeg_no_data(&mut m, &mut st_rs, mpeg, cb, this);
        let ret_ps = call_fn(&mut r, 0x2ba408, &[mpeg, cb, this]);
        assert_eq!(ret_rs, ret_ps);
        assert_eq!(st_rs.calls, st_ps.borrow().calls, "feed sz={pkt_sz} eof={eof}");
        assert_eq!(mem_slice(&m, dst, 0x100), copy_out(&mut r, dst, 0x100), "feed dst sz={pkt_sz} eof={eof}");
    }

    // 4. VideoPlayer_Close 0x2ba998, cVideoPlayer::~cVideoPlayer 0x2bab58, StaticInit/Dtor 0x2bac00/0x2bac40
    for flags in 0..4u32 {
        let mut m = fresh_mem(&mut r);
        let this = BASE + 0x200;
        for &(off, v) in &[
            (0x24u32, BASE + 0x1000),
            (0x28, BASE + 0x2000),
            (0x2c, BASE + 0x3000),
            (0x30, BASE + 0x4000),
            (0x38, BASE + 0x5000),
            (0xb0, BASE + 0x6000),
            (0xb8, BASE + 0x7000),
        ] {
            m.w32(this + off, v);
            r.mem.write_u32(this + off, v);
        }
        let st_ps = Rc::new(RefCell::new(EnvState::default()));
        install_stubs(&mut r, st_ps.clone());
        let mut st_rs = EnvState::default();

        vm::player_close(&mut m, &mut st_rs, this);
        call_fn(&mut r, 0x2ba998, &[this]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);

        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        vm::player_dtor(&mut m, &mut st_rs, this, flags);
        call_fn(&mut r, 0x2bab58, &[this, flags]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
        assert_eq!(m.r32(this), r.mem.read_u32(this));

        st_rs.calls.clear();
        st_ps.borrow_mut().calls.clear();
        vm::static_init_video_player(&mut m, &mut st_rs);
        call_fn(&mut r, 0x2bac00, &[]);
        vm::static_dtor_video_player(&mut m, &mut st_rs);
        call_fn(&mut r, 0x2bac40, &[]);
        assert_eq!(st_rs.calls, st_ps.borrow().calls);
    }
}
