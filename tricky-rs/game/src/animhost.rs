//! Live rider animation host (board row G5d1): runs the game's ported and runner-checked animation bank
//! (`AnimData` 0x3211a8, `AnimIds_Init` 0x15aec0, `AnimData_OpenBig` 0x158ea0), per-rider animation controllers
//! (`AnimCtl_*` at `rider+0x46c0`, `cBoarderSequence`, `AnimMode_*` 4..11, `AnimClip_ResolveAnmIndex` 0x15ada8,
//! `SkelAnim_CreateTree` 0x158dc0), and skeleton pose blender (`AnimCtl_BlendPose` 0x15f7c8 / `PoseBlend_*`
//! 0x202868..0x202b00) in engine memory (`pf::Mem`) directly on `/SLUS_203.26`'s tables, the disc's `.afl` files
//! (`data/char/anm.big`), and the `.mpf` parts' verbatim `0x54`-byte skeleton bones.
//!
//! Per-tick order in `SLUS_203.26`:
//! 1. `cPreAI::vf1` 0x1146f8 -> `Boarder_FlushEndedClips` 0x117190 -> `AnimCtl_FlushEndedClips` 0x15f700
//! 2. `cBoarder::vf7` 0x117198 -> control states play/query clips (`AnimCtl_PlayClip`, `GetClip`, `GetClipType`,
//!    `IsClipAtEnd`, `TestClipFlag`, `TestClipBit`, `TurnAllLayers`, `SetAllLayersStance`), `Boarder_PickIdleClip`
//! 3. `cPostAI::vf1` 0x114780 -> `Boarder_UpdateAnimLayers` 0x117180 -> `AnimCtl_UpdateLayers(ctl, 1.0)` 0x15f390
//!    (advances all 3 layers, runs `AnimMode_*`, fires marker sounds, updates weights, then runs `AnimCtl_BlendPose`
//!    0x15f7c8 into each rider's `PoseBlend` objects at `rider+0x7c0`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::aictl;
use crate::animctl::{self as ctl, CLIP_STARTED, CLIP_TABLE, GET_CLIP_ID, GET_MARKER_FRAME, GET_MARKER_PHASE, IDS_INIT, MARKER_SOUND, MODES, NONE, RESOLVE, SET_MARKER};
use crate::animdata::{self as data, ALLOC, BANK, BIG_CTOR, BIG_DTOR, BIG_RESET, MEM_FREE};
use crate::animids;
use crate::animload::{self as load, LoadEnv};
use crate::animmode;
use crate::animpose::{self as pose, MATRIX};
use crate::animseq::{self as seq, DELETE, POOL_ALLOC, SKEL_ANIM_CREATE, SeqEnv};
use crate::pf::Mem;
use crate::ps2f::F;
use crate::spin;

/// Static ELF data slice mapped verbatim from `/SLUS_203.26` (`file_off = addr - 0xff000`):
/// covers `BIG_PATH` (0x321190), `AnimData` bank (0x3211a8), clip/file name tables (0x3242a0/0x3258b0),
/// `CLIP_TABLE` (0x325fa8..0x32b38c), `PoseBlend` guards & identity matrix (0x341f9c, 0x346440), strings &
/// vtables & `AnimMode_*` tables (0x371790..0x379620), and `NO_KEY`/`SKEL_VTABLE` (0x39e468..0x39e4a0).
pub const ELF_DATA_START: u32 = 0x321190;
pub const ELF_DATA_END: u32 = 0x39e500;
const ELF_LOAD_BIAS: usize = 0xff000;

/// Zeroed BSS slice covering `KEY_TABLE` (0x3bc530), `HASHES` (0x3bdb40), `AnimIds_Init` index tables
/// (0x3bf180..0x3d2170), and `PoseBlend_BlendSkeleton` scratch (`W0..LERP_T`, 0x3d2170..0x3d2960).
const BSS_START: u32 = 0x3bc530;
const BSS_SIZE: usize = 0x16ad0;

/// Career roster slice (`CAREER_BASE + 0x27dc`, 0x84 bytes per rider, up to 12 riders) read by `AnimData_LoadClipTable`.
const CAREER_SLICE_START: u32 = load::CAREER + 0x27dc;
const CAREER_SLICE_SIZE: usize = 0x800;

/// Heap backing `AnimMemBank` (in-game `cGame::Init` 0x17e2a4 passes `0x16e360`; we give 4 MiB inside real
/// PS2 EE RAM `< 0x0200_0000` so `AnimHost` memory maps straight into `r5900::Runner`).
const BANK_MEM_BASE: u32 = 0x0080_0000;
const BANK_MEM_ALLOC_SIZE: u32 = 0x003f_ff00;
const BANK_MEM_REGION_SIZE: usize = 0x0040_0000;

/// Per-rider memory (`cBoarder` offsets: `+0x1b4..+0x464`, `+0x7bc/+0x7c0` `PoseBlend` array, `+0x46c0` `AnimCtl`,
/// plus `+0x5000` raw `.mpf` bones and `+0x5c00` stats stub).
const RIDER_BASE: u32 = 0x00c0_0000;
const RIDER_STRIDE: u32 = 0x0000_6000;
const MAX_RIDERS: usize = 12;
const RIDER_BONES_OFF: u32 = 0x5000;
const RIDER_STATS_OFF: u32 = 0x5c00;
const RIDER_DISP_SCRATCH: u32 = 0x5e00;
/// Maximum bones per `PoseBlend` object (`0x108 / 0xc = 22`).
pub const MAX_POSE_BONES: usize = 22;

/// Recycling pool for `cBoarderSequence` (`0x90` bytes) and `SkelAnim` (`0x2ac` bytes) nodes.
const POOL_BASE: u32 = 0x00d0_0000;
const POOL_SIZE: u32 = 0x0020_0000;
const SEQ_SIZE: u32 = 0x90;
const TREE_SIZE: u32 = 0x2ac;

/// Roster entry handed to `AnimHost::new` so `AnimData_LoadClipTable` 0x159150 loads the right uber `.afl` files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RosterEntry {
    /// Character index (`0..=11`: Eddie, Elise, Mac, JP, Zoe, Moby, Luther, Kaori, Psymon, Seeiah, Brodi, Marisol).
    pub character: u32,
    /// Board class (`0` = BX, `1` = freestyle, `2` = alpine/EX).
    pub board_class: u32,
    /// True loads this rider's uber `.afl` file in `AnimData_LoadClipTable` (`+0x69 >= 0`).
    pub load_uber: bool,
}

/// Rider state fields read by `AnimMode_*` (4..11) during `AnimCtl_UpdateLayers` 0x15f390.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimRiderInputs {
    /// `rider+0x1b4`: riding switch (mirrors `AnimMode_BrakeBlend` and `AnimMode_DirectionBlend` / `WindUpBlend`)
    pub switch: bool,
    /// `rider+0x1dc`: row index for `AnimMode_WindUpBlend` and `AnimMode_CrouchSteerBlend` (rail kind / jump row)
    pub row: u32,
    /// `rider+0x1f0`: air crouch-out fraction (`AnimMode_BoardClassFrame`)
    pub x1f0: f32,
    /// `rider+0x1fc`: brake ease value (`AnimMode_BrakeBlend`)
    pub brake: f32,
    /// `rider+0x208`: crouch ease value (`AnimMode_CrouchBlend`, `_BrakeBlend`, `_WindUpBlend`, `_CrouchSteerBlend`, `_CrouchFrame`)
    pub crouch: f32,
    /// `rider+0x214`: steer ease value (`AnimMode_CrouchSteerBlend`)
    pub steer: f32,
    /// `rider+0x220`: spin wind-up ease value (`AnimMode_WindUpBlend`)
    pub spin_windup: f32,
    /// `rider+0x22c`: flip wind-up ease value (`AnimMode_WindUpBlend`)
    pub flip_windup: f32,
    /// `rider+0x238`: start-gate lean (`AnimMode_GateLeanFrame`)
    pub gate_lean: f32,
    /// `rider+0x268`: side lean ease value (`AnimMode_BrakeBlend`)
    pub side: f32,
    /// `rider+0x27c`, `rider+0x280`: direction blend x/y (`AnimMode_DirectionBlend`)
    pub dir_x: f32,
    pub dir_y: f32,
    /// `rider+0x420`: board class (`0` BX, `1` FR, `2` EX)
    pub board_class: u32,
}

/// Scoring / sound callbacks emitted by `SpinState_UpdateTrickClips` 0x1272e8 (`crate::spin::update_trick_clips`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrickClipCall {
    /// `Score_StartGrab` 0x155ee8(score, clip)
    StartGrab(u32),
    /// `Scoring_OnTrickEvent` 0x156148(score, clip, duration_secs)
    TrickEvent { clip: u32, duration: f32 },
    /// `SpinState_SetTrickClipRate` 0x156050(score, clip)
    SetTrickClipRate(u32),
}

/// `Takeoff_PlayLaunchClip` 0x126fc0: jump table at `0x368bf0` mapping layer-0 prewind clip (`0x219..=0x26d`) to
/// its launch clip (`0x226..=0x22d`, `0x270..=0x277`); `None` for any other clip.
pub fn takeoff_launch_clip(clip: u32) -> Option<u32> {
    Some(match clip {
        0x219 | 0x265 => 0x226,
        0x21a | 0x264 => 0x227,
        0x21b => 0x228,
        0x21c => 0x229,
        0x21d => 0x22a,
        0x21e => 0x22b,
        0x21f | 0x263 => 0x22c,
        0x220 | 0x262 => 0x22d,
        0x266 => 0x272,
        0x267 => 0x273,
        0x268 => 0x270,
        0x269 => 0x271,
        0x26a => 0x276,
        0x26b => 0x277,
        0x26c => 0x274,
        0x26d => 0x275,
        _ => return None,
    })
}

/// `Boarder_PlayTakeoffClip` 0x127298: chooses takeoff clip from `rider+0x1dc` (`3 => 0x26e`, `4 => 0x26f`, else `0x223`).
pub fn takeoff_clip_for_row(row_1dc: u32) -> u32 {
    match row_1dc {
        3 => 0x26e,
        4 => 0x26f,
        _ => 0x223,
    }
}

/// Sound / clip events emitted by `AnimCtl_FireClipSounds` 0x15f300 and `AnimCtl_PlayClip` 0x15fcf8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimEvent {
    /// `0x235780(audio, rider, sound)`: clip marker sound crossed on this tick.
    MarkerSound { rider_slot: usize, sound: u16 },
    /// `0x235778(audio, clip_type)`: new clip started on a layer.
    ClipStarted { clip_type: u32 },
}

/// Blended bone output read out of a `PoseBlend` object (`rider+0x7c0 + part_idx * 0x394`) after `AnimCtl_BlendPose`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlendedPose {
    /// Part slot (`0` = body, `1` = board).
    pub part: i32,
    /// Per-bone Euler rotations (radians, `+0x000 + 0xc * b`), as `PoseBlend_BlendSkeleton` leaves them.
    pub rot: Vec<[f32; 3]>,
    /// Per-bone translations (cm, `+0x108 + 0xc * b`), scaled by `stats+0x60`.
    pub trans: Vec<[f32; 3]>,
}

/// `SkelAnim_Create` 0x158868 + `SkelAnim_Init` 0x159880 + `SkelAnim_SetSkeleton` 0x1598e8: allocates one
/// `0x2ac`-byte `SkelAnim` node from the node pool, zeroes it, binds its clip header (`file+8 + clip_idx * 0x24`),
/// and sets `+0x2a4/+0x2a8` (curve offsets and data) on leaf nodes (`hdr+9 == 0`).
pub fn skel_anim_create(m: &mut Mem, env: &mut dyn SeqEnv, file_entry: u32, clip_idx: u32) -> u32 {
    let node = env.call(m, POOL_ALLOC, &[seq::POOL, TREE_SIZE, 0xc000_0000, 0x378ff0, 0, 0]);
    if node != 0 {
        for k in 0..TREE_SIZE {
            m.w8(node + k, 0);
        }
    }
    let hdr = m.r32(file_entry + 8).wrapping_add(clip_idx.wrapping_mul(0x24));
    let offs = m.r32(file_entry + 0xc);
    let data_ptr = m.r32(file_entry + 0x10);
    m.w32(node + 8, hdr);
    if m.r8(hdr + 9) == 0 {
        m.w32(node + 0x2a8, data_ptr);
        m.w32(node + 0x2a4, offs);
    } else {
        m.w32(node + 0x2a8, 0);
        m.w32(node + 0x2a4, 0);
    }
    node
}

/// `SkelAnim_CreateTree` 0x158dc0(bank, key): creates the root `SkelAnim` node for `(file = key & 0xffff, clip =
/// key >> 16)` and one child node per group (`hdr+9`), linked at `root+0xc + 4*i`.
pub fn skel_anim_create_tree(m: &mut Mem, env: &mut dyn SeqEnv, bank: u32, key: u32) -> u32 {
    let file_no = key & 0xffff;
    let clip_idx = key >> 16;
    let file_entry = bank.wrapping_add(data::FILES).wrapping_add(file_no.wrapping_mul(0x18));
    let root = skel_anim_create(m, env, file_entry, clip_idx);
    m.w32(root + 4, file_no);
    m.w32(root, clip_idx);
    let n_children = m.r8(m.r32(root + 8) + 9) as u32;
    for i in 0..n_children {
        let child_idx = clip_idx.wrapping_add(i).wrapping_add(1);
        let child = skel_anim_create(m, env, file_entry, child_idx);
        m.w32(root + 0xc + 4 * i, child);
        m.w32(child, child_idx);
        m.w32(child + 4, file_no);
    }
    root
}

#[derive(Default)]
struct AllocState {
    pool_bump: u32,
    free_seq: Vec<u32>,
    free_tree: Vec<u32>,
    alloc_kinds: HashMap<u32, u32>,
    events: Vec<AnimEvent>,
}

impl AllocState {
    fn pool_alloc(&mut self, size: u32) -> u32 {
        let aligned = (size + 0xf) & !0xf;
        if size == SEQ_SIZE {
            if let Some(p) = self.free_seq.pop() {
                return p;
            }
        } else if size == TREE_SIZE {
            if let Some(p) = self.free_tree.pop() {
                return p;
            }
        }
        assert!(self.pool_bump + aligned <= POOL_SIZE, "animhost: pool exhausted ({:#x} + {:#x})", self.pool_bump, aligned);
        let p = POOL_BASE + self.pool_bump;
        self.pool_bump += aligned;
        self.alloc_kinds.insert(p, size);
        p
    }

    fn pool_free(&mut self, ptr: u32) {
        if let Some(&sz) = self.alloc_kinds.get(&ptr) {
            if sz == SEQ_SIZE {
                self.free_seq.push(ptr);
            } else if sz == TREE_SIZE {
                self.free_tree.push(ptr);
            }
        }
    }
}

struct RuntimeEnv<'a, R: FnMut() -> u32> {
    alloc: &'a mut AllocState,
    rand_game: &'a mut R,
}

impl<R: FnMut() -> u32> SeqEnv for RuntimeEnv<'_, R> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        match func {
            IDS_INIT => {
                animids::init(m);
                0
            }
            RESOLVE => {
                let (clip, rider) = (args[0], args[1]);
                let class = m.r32(rider + 0x420);
                let stats_ptr = m.r32(rider + 0x464);
                let stats_3c = if stats_ptr != 0 { m.r32(stats_ptr + 0x3c) as i32 } else { 0 };
                aictl::resolve_anm_index(&|a| m.r32(a), clip, class, stats_3c, self.rand_game)
            }
            GET_CLIP_ID => data::clip_id(m, args[1]),
            GET_MARKER_FRAME => data::marker_frame(m, args[0], args[1], args[2]) as u32,
            SET_MARKER => {
                data::set_marker(m, args[0], args[1], args[2], args[3]);
                0
            }
            GET_MARKER_PHASE => data::marker_phase(m, args[0], args[1], args[2]).0.to_bits(),
            ctl::BLEND_POSE => {
                pose::blend_pose(m, args[0]);
                0
            }
            SKEL_ANIM_CREATE => skel_anim_create_tree(m, self, args[0], args[1]),
            POOL_ALLOC => self.alloc.pool_alloc(args[1]),
            DELETE => {
                self.alloc.pool_free(args[0]);
                0
            }
            ALLOC => {
                assert!(args[1] <= BANK_MEM_REGION_SIZE as u32, "animhost: bank alloc {:#x} exceeds region", args[1]);
                BANK_MEM_BASE
            }
            MEM_FREE | BIG_CTOR | BIG_DTOR | BIG_RESET => 0,
            MARKER_SOUND => {
                let rider_addr = args[1];
                let slot = if rider_addr >= RIDER_BASE { ((rider_addr - RIDER_BASE) / RIDER_STRIDE) as usize } else { 0 };
                self.alloc.events.push(AnimEvent::MarkerSound { rider_slot: slot, sound: args[2] as u16 });
                0
            }
            CLIP_STARTED => {
                self.alloc.events.push(AnimEvent::ClipStarted { clip_type: args[1] });
                0
            }
            f if MODES[4..12].contains(&f) => {
                let dt = F(f32::from_bits(args[2]));
                animmode::run(m, self, f, args[0], args[1], dt);
                0
            }
            _ => panic!("animhost: unexpected SeqEnv call {func:#x} args={args:x?}"),
        }
    }
}

struct TrickEnv<'a, R: FnMut() -> u32> {
    inner: RuntimeEnv<'a, R>,
    calls: Vec<TrickClipCall>,
}

impl<R: FnMut() -> u32> SeqEnv for TrickEnv<'_, R> {
    fn call(&mut self, m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        match func {
            spin::PLAY_CLIP => ctl::play_clip(m, &mut self.inner, args[0], args[1], args[2], args[3]),
            spin::START_GRAB => {
                self.calls.push(TrickClipCall::StartGrab(args[1]));
                0
            }
            spin::TRICK_EVENT => {
                self.calls.push(TrickClipCall::TrickEvent { clip: args[1], duration: f32::from_bits(args[2]) });
                0
            }
            spin::SET_TRICK_CLIP_RATE => {
                self.calls.push(TrickClipCall::SetTrickClipRate(args[1]));
                0
            }
            spin::SFX_GRAB_HELD | spin::SFX_TRICK_STOP | spin::SFX_TRICK_START => 0,
            _ => self.inner.call(m, func, args),
        }
    }
}

struct SyncLoadEnv<'a, L: FnMut(&str) -> Option<Vec<u8>>> {
    alloc: &'a mut AllocState,
    loader: &'a mut L,
    open_files: HashMap<u32, Vec<u8>>,
    next_handle: u32,
}

impl<L: FnMut(&str) -> Option<Vec<u8>>> SeqEnv for SyncLoadEnv<'_, L> {
    fn call(&mut self, _m: &mut Mem, func: u32, args: &[u32]) -> u32 {
        match func {
            ALLOC => {
                assert!(args[1] <= BANK_MEM_REGION_SIZE as u32, "animhost: bank alloc {:#x} exceeds region", args[1]);
                BANK_MEM_BASE
            }
            MEM_FREE | BIG_CTOR | BIG_DTOR | BIG_RESET => 0,
            DELETE => {
                self.alloc.pool_free(args[0]);
                0
            }
            _ => panic!("animhost: unexpected SyncLoadEnv SeqEnv call {func:#x}"),
        }
    }
}

impl<L: FnMut(&str) -> Option<Vec<u8>>> LoadEnv for SyncLoadEnv<'_, L> {
    fn open(&mut self, path: &[u8]) -> u32 {
        let s = String::from_utf8_lossy(path);
        let bytes = (self.loader)(&s).unwrap_or_else(|| panic!("animhost: missing animation file {s}"));
        self.next_handle += 1;
        let h = self.next_handle;
        self.open_files.insert(h, bytes);
        h
    }
    fn size(&mut self, h: u32) -> u32 {
        self.open_files.get(&h).map_or(0, |v| v.len() as u32)
    }
    fn read(&mut self, m: &mut Mem, h: u32, dst: u32) {
        if let Some(bytes) = self.open_files.get(&h) {
            for (k, &b) in bytes.iter().enumerate() {
                m.w8(dst + k as u32, b);
            }
        }
    }
    fn free(&mut self, h: u32) {
        self.open_files.remove(&h);
    }
    fn big_request(&mut self, _m: &mut Mem, _big: u32, _path: &[u8]) -> u32 { 0 }
    fn big_ready(&mut self, _m: &mut Mem, _big: u32, _h: u32) -> bool { true }
    fn big_size(&mut self, _m: &mut Mem, _big: u32, _h: u32) -> u32 { 0 }
    fn big_read(&mut self, _m: &mut Mem, _big: u32, _h: u32, _dst: u32) {}
    fn big_close(&mut self, _m: &mut Mem, _big: u32, _h: u32) {}
    fn big_open(&mut self, _m: &mut Mem, _big: u32) {}
    fn big_reset(&mut self, _m: &mut Mem, _big: u32) {}
}

/// Live animation host holding the game's `AnimData` bank (`0x3211a8`) and up to [`MAX_RIDERS`] rider
/// controllers (`rider+0x46c0`) + `PoseBlend` objects (`rider+0x7c0`) in `pf::Mem`.
pub struct AnimHost {
    mem: Mem,
    alloc: AllocState,
    riders_n: usize,
    part_bones: Vec<Vec<(i32, usize)>>,
}

impl AnimHost {
    /// Construct the host from `/SLUS_203.26` bytes, an `.afl` file loader (taking paths like
    /// `"|data/char/bxanim.afl"`), and the race roster (`AnimData_OpenBig(mode=1, now=true)`).
    pub fn new(elf: &[u8], roster: &[RosterEntry], mut load_file: impl FnMut(&str) -> Option<Vec<u8>>) -> Result<Self, String> {
        let start = (ELF_DATA_START as usize).checked_sub(ELF_LOAD_BIAS).ok_or("ELF start underflow")?;
        let end = (ELF_DATA_END as usize).checked_sub(ELF_LOAD_BIAS).ok_or("ELF end underflow")?;
        let elf_slice = elf.get(start..end).ok_or_else(|| format!("SLUS_203.26 too short ({:#x} < {:#x})", elf.len(), end))?;

        let mut mem = Mem {
            regions: vec![
                (ELF_DATA_START, elf_slice.to_vec()),
                (BSS_START, vec![0u8; BSS_SIZE]),
                (CAREER_SLICE_START, vec![0u8; CAREER_SLICE_SIZE]),
                (BANK_MEM_BASE, vec![0u8; BANK_MEM_REGION_SIZE]),
                (RIDER_BASE, vec![0u8; RIDER_STRIDE as usize * MAX_RIDERS]),
                (POOL_BASE, vec![0u8; POOL_SIZE as usize]),
            ],
        };

        // `StaticInit_Spline2` 0x251238 initializes `0x346440` (`animpose::MATRIX`) to the 4×4 identity matrix.
        for r in 0..4u32 {
            for c in 0..4u32 {
                let v = if r == c { 1.0f32 } else { 0.0f32 };
                mem.w32(MATRIX + 0x10 * r + 4 * c, v.to_bits());
            }
        }
        // `IDS_DONE` (0x32b38c) starts cleared so the first `AnimCtl_Construct` runs `AnimIds_Init`.
        mem.w32(ctl::IDS_DONE, 0);

        let mut alloc = AllocState::default();
        {
            let mut env = SyncLoadEnv { alloc: &mut alloc, loader: &mut load_file, open_files: HashMap::new(), next_handle: 0 };
            data::construct(&mut mem, &mut env, BANK);
            data::init(&mut mem, &mut env, BANK, BANK_MEM_ALLOC_SIZE);
            // Populate the career roster at `CAREER + 0x27dc` for `AnimData_LoadClipTable`.
            let n = roster.len().min(12);
            mem.w32(CAREER_SLICE_START, n as u32);
            for (i, entry) in roster.iter().take(n).enumerate() {
                let row = CAREER_SLICE_START + 0x84 * i as u32;
                mem.w32(row + 0x3c, entry.character);
                mem.w32(row + 0x48, entry.board_class);
                mem.w8(row + 0x69, if entry.load_uber { 0 } else { 0xff });
            }
            load::open_big(&mut mem, &mut env, BANK, 1, true, 0, 0);
        }

        Ok(Self { mem, alloc, riders_n: 0, part_bones: Vec::new() })
    }

    /// Construct from an open disc (`tricky_data::big::Files` with `/SLUS_203.26` read from the disc).
    pub fn from_disc(elf: &[u8], files: &mut tricky_data::big::Files, roster: &[RosterEntry]) -> Result<Self, String> {
        Self::new(elf, roster, |p| files.load(p).ok().flatten())
    }

    /// Load an additional `.afl` file by its game file index (`0..0x1be`, e.g. another rider's uber file)
    /// into the live bank and re-index all clip keys.
    pub fn load_extra_file(&mut self, file_idx: u32, mut load_file: impl FnMut(&str) -> Option<Vec<u8>>) {
        let mut env = SyncLoadEnv { alloc: &mut self.alloc, loader: &mut load_file, open_files: HashMap::new(), next_handle: 0 };
        load::load_file(&mut self.mem, &mut env, BANK, file_idx, true);
        load::index_all(&mut self.mem, BANK);
    }

    /// Address of rider slot `slot` in `pf::Mem`.
    pub fn rider_addr(slot: usize) -> u32 {
        assert!(slot < MAX_RIDERS, "animhost: rider slot {slot} >= {MAX_RIDERS}");
        RIDER_BASE + (slot as u32) * RIDER_STRIDE
    }

    /// Address of rider slot `slot`'s `AnimCtl` (`rider + 0x46c0`).
    pub fn ctl_addr(slot: usize) -> u32 {
        Self::rider_addr(slot) + 0x46c0
    }

    /// Direct read-only view of the hosted engine memory (for tests and runner comparisons).
    pub fn mem(&self) -> &Mem {
        &self.mem
    }

    /// Register a rider slot with its character id (`0..11`), board class (`0..2`), body scale (`stats+0x60`),
    /// and skeleton parts `[(part_slot, raw_bones)]` (typically `[(0, body_bones), (1, board_bones)]`), running
    /// `AnimCtl_Construct` 0x15ed68 (and `AnimIds_Init` 0x15aec0 on the first rider). Returns the slot index.
    pub fn add_rider(&mut self, character: u32, board_class: u32, scale: f32, parts: &[(i32, &[[u8; 0x54]])]) -> usize {
        let slot = self.riders_n;
        assert!(slot < MAX_RIDERS, "animhost: too many riders (max {MAX_RIDERS})");
        self.riders_n += 1;
        let base = Self::rider_addr(slot);
        let ctl_ptr = Self::ctl_addr(slot);
        for k in 0..RIDER_STRIDE {
            self.mem.w8(base + k, 0);
        }
        // Stats stub (`+0x3c` character id, `+0x44` board class, `+0x5c` character id, `+0x60` body scale).
        let stats_ptr = base + RIDER_STATS_OFF;
        self.mem.w32(stats_ptr + 0x3c, character);
        self.mem.w32(stats_ptr + 0x44, board_class);
        self.mem.w32(stats_ptr + 0x5c, character);
        self.mem.w32(stats_ptr + 0x60, scale.to_bits());
        self.mem.w32(base + 0x464, stats_ptr);
        self.mem.w32(base + 0x420, board_class);
        self.mem.w32(base + 0x1e4, 0x1a);
        self.mem.w32(ctl_ptr + 0x24, base);

        // Bind `PoseBlend` objects at `rider+0x7c0 + k * 0x394` (`RiderSkin_Init` 0x206f80 + `RiderSkin_Bind`
        // 0x207374 + `RiderSkin_Scale` 0x208250).
        let mut bone_cursor = base + RIDER_BONES_OFF;
        let mut mask_bit = 0u32;
        let mut meta = Vec::with_capacity(parts.len());
        self.mem.w32(base + 0x7bc, parts.len() as u32);
        for (k, &(part_id, raw_bones)) in parts.iter().enumerate() {
            assert!(raw_bones.len() <= MAX_POSE_BONES, "animhost: part {part_id} has {} bones > {MAX_POSE_BONES}", raw_bones.len());
            let skel_ptr = bone_cursor;
            for bone in raw_bones {
                for (o, &b) in bone.iter().enumerate() {
                    self.mem.w8(bone_cursor + o as u32, b);
                }
                bone_cursor += 0x54;
            }
            let p = base + 0x7c0 + 0x394 * k as u32;
            self.mem.w32(p + 0x380, scale.to_bits());
            self.mem.w32(p + 0x384, part_id as u32);
            self.mem.w32(p + 0x388, if raw_bones.is_empty() { 0 } else { skel_ptr });
            self.mem.w32(p + 0x38c, raw_bones.len() as u32);
            self.mem.w32(p + 0x390, mask_bit);
            pose::begin(&mut self.mem, p);
            mask_bit += raw_bones.len() as u32;
            meta.push((part_id, raw_bones.len()));
        }
        self.part_bones.push(meta);

        let mut rng = || crate::rand::rand_game();
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut rng };
        ctl::construct(&mut self.mem, &mut env, ctl_ptr);
        slot
    }

    /// `Boarder_ResetAnim` 0x117170 -> `AnimCtl_Reset` 0x15ee30 on `slot`.
    pub fn reset_rider(&mut self, slot: usize) {
        let base = Self::rider_addr(slot);
        let ctl_ptr = Self::ctl_addr(slot);
        self.mem.w32(ctl_ptr + 0x24, base);
        let mut rng = || crate::rand::rand_game();
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut rng };
        ctl::reset(&mut self.mem, &mut env, ctl_ptr);
    }

    /// Write the rider fields read by `AnimMode_*` (4..11) into `slot`'s `cBoarder` memory.
    pub fn sync_rider(&mut self, slot: usize, inp: &AnimRiderInputs) {
        let r = Self::rider_addr(slot);
        self.mem.w32(r + 0x1b4, inp.switch as u32);
        self.mem.w32(r + 0x1dc, inp.row);
        self.mem.w32(r + 0x1f0, inp.x1f0.to_bits());
        self.mem.w32(r + 0x1fc, inp.brake.to_bits());
        self.mem.w32(r + 0x208, inp.crouch.to_bits());
        self.mem.w32(r + 0x214, inp.steer.to_bits());
        self.mem.w32(r + 0x220, inp.spin_windup.to_bits());
        self.mem.w32(r + 0x22c, inp.flip_windup.to_bits());
        self.mem.w32(r + 0x238, inp.gate_lean.to_bits());
        self.mem.w32(r + 0x268, inp.side.to_bits());
        self.mem.w32(r + 0x27c, inp.dir_x.to_bits());
        self.mem.w32(r + 0x280, inp.dir_y.to_bits());
        self.mem.w32(r + 0x420, inp.board_class);
    }

    /// `Boarder_FlushEndedClips` 0x117190 -> `AnimCtl_FlushEndedClips` 0x15f700 (called in `cPreAI::vf1` 0x1146f8).
    pub fn flush_ended(&mut self, slot: usize, rand_game: &mut dyn FnMut() -> u32) {
        let ctl_ptr = Self::ctl_addr(slot);
        let mut r = || rand_game();
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut r };
        ctl::flush_ended(&mut self.mem, &mut env, ctl_ptr);
    }

    /// `AnimCtl_PlayClip` 0x15fcf8(clip, frame, marker).
    pub fn play_clip(&mut self, slot: usize, clip: u32, frame: u32, marker: u32, rand_game: &mut dyn FnMut() -> u32) -> u32 {
        if clip >= 0x2ff {
            return NONE;
        }
        let ctl_ptr = Self::ctl_addr(slot);
        let mut r = || rand_game();
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut r };
        ctl::play_clip(&mut self.mem, &mut env, ctl_ptr, clip, frame, marker)
    }

    /// `Boarder_UpdateAnimLayers` 0x117180 -> `AnimCtl_UpdateLayers` 0x15f390(ctl, dt) (game passes `dt = 1.0`,
    /// which advances clips by `1.0 * speed * 30 / 60 = 0.5 * speed` frames per 60 Hz tick, then calls
    /// `AnimCtl_BlendPose` 0x15f7c8).
    pub fn update_layers(&mut self, slot: usize, dt: f32, rand_game: &mut dyn FnMut() -> u32) {
        let ctl_ptr = Self::ctl_addr(slot);
        let mut r = || rand_game();
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut r };
        ctl::update_layers(&mut self.mem, &mut env, ctl_ptr, F(dt));
    }

    /// `AnimCtl_BlendPose` 0x15f7c8 on `slot` (already invoked at the end of `update_layers`).
    pub fn blend_pose(&mut self, slot: usize) {
        pose::blend_pose(&mut self.mem, Self::ctl_addr(slot));
    }

    /// `AnimCtl_GetClip` 0x15fdd0(layer).
    pub fn clip(&self, slot: usize, layer: u32) -> u32 {
        ctl::clip(&self.mem, Self::ctl_addr(slot), layer)
    }

    /// `AnimCtl_GetClipType` 0x15eec0(layer).
    pub fn clip_type(&self, slot: usize, layer: u32) -> u32 {
        ctl::clip_type(&self.mem, Self::ctl_addr(slot), layer)
    }

    /// `AnimCtl_IsClipAtEnd` 0x15ff40(layer).
    pub fn at_end(&self, slot: usize, layer: u32) -> bool {
        ctl::at_end(&self.mem, Self::ctl_addr(slot), layer)
    }

    /// `AnimCtl_GetLayerPhase` 0x15fe70(layer).
    pub fn layer_phase(&self, slot: usize, layer: u32) -> f32 {
        ctl::layer_phase(&self.mem, Self::ctl_addr(slot), layer).0
    }

    /// `AnimCtl_GetClipDuration` 0x15fec8(layer).
    pub fn clip_duration(&self, slot: usize, layer: u32) -> f32 {
        ctl::clip_duration(&self.mem, Self::ctl_addr(slot), layer).0
    }

    /// `AnimCtl_TestClipFlag` 0x15ffb8(flag, layer) (`flag` is 1-based, e.g. `4` for the grab/uber gate).
    pub fn test_flag(&self, slot: usize, flag: u32, layer: u32) -> bool {
        ctl::test_flag(&self.mem, Self::ctl_addr(slot), flag, layer)
    }

    /// `AnimCtl_TestClipBit` 0x160008(flag, layer) (`flag` is 1-based, crossed on this tick).
    pub fn test_bit(&self, slot: usize, flag: u32, layer: u32) -> bool {
        ctl::test_bit(&self.mem, Self::ctl_addr(slot), flag, layer)
    }

    /// `AnimCtl_GetMarkerPhase` 0x15fde0(n, layer).
    pub fn marker_phase(&mut self, slot: usize, n: u32, layer: u32, rand_game: &mut dyn FnMut() -> u32) -> f32 {
        let ctl_ptr = Self::ctl_addr(slot);
        let mut r = || rand_game();
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut r };
        ctl::marker_phase(&mut self.mem, &mut env, ctl_ptr, n, layer).0
    }

    /// Set the playback speed (`ctl + 0x18`) that subsequent `play_clip` calls inherit.
    pub fn set_speed(&mut self, slot: usize, speed: f32) {
        self.mem.w32(Self::ctl_addr(slot) + 0x18, speed.to_bits());
    }

    /// Set the bone mask (`ctl + 0x1c`, `-1` = all bones) that subsequent `play_clip` calls inherit.
    pub fn set_bone_mask(&mut self, slot: usize, mask: u32) {
        self.mem.w32(Self::ctl_addr(slot) + 0x1c, mask);
    }

    /// Set the facing (`ctl + 0x20`) that subsequent `play_clip` calls inherit.
    pub fn set_facing(&mut self, slot: usize, facing: f32) {
        self.mem.w32(Self::ctl_addr(slot) + 0x20, facing.to_bits());
    }

    /// Set the stance (`ctl + 0x14`, `rider + 0x46d4`) that subsequent `play_clip` calls inherit.
    pub fn set_stance(&mut self, slot: usize, stance: u32) {
        self.mem.w32(Self::ctl_addr(slot) + 0x14, stance);
    }

    /// `AnimCtl_SetAllLayersStance` 0x15ef48(stance): updates all active sequences across layers 0..2.
    pub fn set_all_layers_stance(&mut self, slot: usize, stance: u32) {
        ctl::set_stance_all(&mut self.mem, Self::ctl_addr(slot), stance);
    }

    /// `AnimCtl_TurnAllLayers` 0x15eef8(angle): turns all active sequences across layers 0..2.
    pub fn turn_all_layers(&mut self, slot: usize, angle: f32) {
        ctl::turn_all(&mut self.mem, Self::ctl_addr(slot), F(angle));
    }

    /// `AnimCtl_StopClipsInCategory` 0x15ef98(category): clears the stop frame on every matching clip (`0x1c` = all).
    pub fn stop_category(&mut self, slot: usize, category: u32) {
        ctl::stop_category(&mut self.mem, Self::ctl_addr(slot), category);
    }

    /// `AnimCtl_ClearClips` 0x15f020: empties layer 1.
    pub fn clear_clips(&mut self, slot: usize) {
        let ctl_ptr = Self::ctl_addr(slot);
        let mut rng = || 0u32;
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut rng };
        ctl::clear_clips(&mut self.mem, &mut env, ctl_ptr);
    }

    /// `AnimCtl_BlendOutAll` 0x15f050: fades out layer 1 over 10 frames.
    pub fn blend_out_all(&mut self, slot: usize) {
        ctl::blend_out_all(&mut self.mem, Self::ctl_addr(slot));
    }

    /// `AnimCtl_GetClipDisplacement` 0x15fa40(out, layer): returns the clip's root displacement scaled by `stats+0x60`.
    pub fn clip_displacement(&mut self, slot: usize, layer: u32, rand_game: &mut dyn FnMut() -> u32) -> [f32; 4] {
        let out = Self::rider_addr(slot) + RIDER_DISP_SCRATCH;
        let ctl_ptr = Self::ctl_addr(slot);
        let mut r = || rand_game();
        let mut env = RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut r };
        ctl::clip_displacement(&mut self.mem, &mut env, out, ctl_ptr, layer);
        std::array::from_fn(|i| f32::from_bits(self.mem.r32(out + 4 * i as u32)))
    }

    /// Read the blended Euler rotations and translations out of `PoseBlend` object `part_idx` (`0` = body, `1` = board).
    pub fn read_pose(&self, slot: usize, part_idx: usize) -> Option<BlendedPose> {
        let &(part, nbones) = self.part_bones.get(slot)?.get(part_idx)?;
        let p = Self::rider_addr(slot) + 0x7c0 + 0x394 * part_idx as u32;
        let mut rot = Vec::with_capacity(nbones);
        let mut trans = Vec::with_capacity(nbones);
        for b in 0..nbones as u32 {
            let r = [0, 1, 2].map(|k| f32::from_bits(self.mem.r32(p + 0xc * b + 4 * k)));
            let t = [0, 1, 2].map(|k| f32::from_bits(self.mem.r32(p + 0x108 + 0xc * b + 4 * k)));
            rot.push(r);
            trans.push(t);
        }
        Some(BlendedPose { part, rot, trans })
    }

    /// Drain sound / clip-start events accumulated during `play_clip` and `update_layers`.
    pub fn drain_events(&mut self) -> Vec<AnimEvent> {
        std::mem::take(&mut self.alloc.events)
    }

    /// Number of clips indexed into `KEY_TABLE` (non-`NO_KEY` entries across `0..0x584`).
    pub fn indexed_clip_count(&self) -> usize {
        let none = self.mem.r32(seq::NO_KEY);
        (0..data::NCLIP).filter(|&i| self.mem.r32(seq::KEY_TABLE + 4 * i) != none).count()
    }

    /// Bytes currently allocated inside `AnimMemBank` (`bank + 8`).
    pub fn bank_bytes_used(&self) -> u32 {
        self.mem.r32(BANK + 8)
    }

    /// Write the rider's 16 stat bytes (`stats+0x0e..+0x1d`), character id (`+0x3c`/`+0x5c`), board class (`+0x44`),
    /// and body scale (`+0x60`) into `slot`'s stats stub (used by `SpinState_UpdateTrickClips` 0x1272e8 and
    /// `SpinState_PlayExitClip` 0x102800).
    pub fn set_stats(&mut self, slot: usize, character: u32, board_class: u32, scale: f32, bytes: [u8; 16]) {
        let base = Self::rider_addr(slot);
        let stats_ptr = base + RIDER_STATS_OFF;
        for (i, &b) in bytes.iter().enumerate() {
            self.mem.w8(stats_ptr + 0x0e + i as u32, b);
        }
        self.mem.w32(stats_ptr + 0x3c, character);
        self.mem.w32(stats_ptr + 0x44, board_class);
        self.mem.w32(stats_ptr + 0x5c, character);
        self.mem.w32(stats_ptr + 0x60, scale.to_bits());
        self.mem.w32(base + 0x420, board_class);
    }

    /// Reset `SpinState_UpdateTrickClips` rider state (`+0x1e4 = 0x1a`, `+0x1e8 = 0`, `+0x1ec = 0`).
    pub fn reset_trick_clips(&mut self, slot: usize) {
        let base = Self::rider_addr(slot);
        self.mem.w32(base + 0x1e4, 0x1a);
        self.mem.w32(base + 0x1e8, 0);
        self.mem.w32(base + 0x1ec, 0);
    }

    /// `Takeoff_PlayLaunchClip` 0x126fc0: plays the launch clip corresponding to layer 0's current prewind clip.
    pub fn play_launch_clip(&mut self, slot: usize, rand_game: &mut dyn FnMut() -> u32) -> Option<u32> {
        let cur = self.clip(slot, 0);
        let next = takeoff_launch_clip(cur)?;
        Some(self.play_clip(slot, next, 0, 5, rand_game))
    }

    /// `Boarder_PlayTakeoffClip` 0x127298: plays `0x26e` (row 3), `0x26f` (row 4), or `0x223`.
    pub fn play_takeoff_clip(&mut self, slot: usize, row_1dc: u32, rand_game: &mut dyn FnMut() -> u32) -> u32 {
        self.play_clip(slot, takeoff_clip_for_row(row_1dc), 0, 5, rand_game)
    }

    /// `SpinState_PlayExitClip` 0x102800 via `crate::spin::play_exit_clip`.
    pub fn play_exit_clip(&mut self, slot: usize, rand_game: &mut dyn FnMut() -> u32) {
        let base = Self::rider_addr(slot);
        let mut r = || rand_game();
        let mut env = TrickEnv { inner: RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut r }, calls: Vec::new() };
        spin::play_exit_clip(&mut self.mem, &mut env, base);
    }

    /// `SpinState_UpdateTrickClips` 0x1272e8 via `crate::spin::update_trick_clips` directly on `AnimHost`'s
    /// live `AnimCtl` and `/SLUS_203.26` `TRICK_TABLE` (`0x333098`).
    pub fn update_trick_clips(
        &mut self,
        slot: usize,
        held: u32,
        trick: u32,
        uber_timer: f32,
        vel_z: f32,
        rand_game: &mut dyn FnMut() -> u32,
    ) -> (u32, Vec<TrickClipCall>) {
        let base = Self::rider_addr(slot);
        self.mem.w32(base + 0x24, uber_timer.to_bits());
        self.mem.w32(base + 0x158, vel_z.to_bits());
        let mut r = || rand_game();
        let mut env = TrickEnv { inner: RuntimeEnv { alloc: &mut self.alloc, rand_game: &mut r }, calls: Vec::new() };
        let ans = spin::update_trick_clips(&mut self.mem, &mut env, base, held, trick);
        (ans, env.calls)
    }

    /// Active trick-clip state `(trick_id +0x1e4, stage +0x1e8, hold_ticks +0x1ec)`.
    pub fn trick_state(&self, slot: usize) -> (u32, u32, u32) {
        let base = Self::rider_addr(slot);
        (self.mem.r32(base + 0x1e4), self.mem.r32(base + 0x1e8), self.mem.r32(base + 0x1ec))
    }

    /// Row `clip` of `CLIP_TABLE` 0x325fa8 (`[category, lookup_mode, update_mode, end_action, layer, blend_in, blend_out]`).
    pub fn clip_table_row(&self, clip: u32) -> [u32; 7] {
        let r = CLIP_TABLE + 0x1c * clip;
        std::array::from_fn(|k| self.mem.r32(r + 4 * k as u32))
    }
}

/// Shared handle to one rider's live `AnimCtl` + `PoseBlend` slot inside an [`AnimHost`].
/// `Clone + Send + Sync` so [`crate::rider::Rider`] stays cheaply cloneable and usable inside Bevy resources.
#[derive(Clone)]
pub struct RiderAnim {
    pub host: Arc<Mutex<AnimHost>>,
    pub slot: usize,
}

impl RiderAnim {
    pub fn new(host: Arc<Mutex<AnimHost>>, slot: usize) -> Self {
        Self { host, slot }
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, AnimHost> {
        self.host.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn set_stats(&self, character: u32, board_class: u32, scale: f32, bytes: [u8; 16]) {
        self.lock().set_stats(self.slot, character, board_class, scale, bytes);
    }
    pub fn reset(&self) {
        self.lock().reset_rider(self.slot);
    }
    pub fn reset_trick_clips(&self) {
        self.lock().reset_trick_clips(self.slot);
    }
    pub fn sync_rider(&self, inp: &AnimRiderInputs) {
        self.lock().sync_rider(self.slot, inp);
    }
    pub fn flush_ended(&self, rand_game: &mut dyn FnMut() -> u32) {
        self.lock().flush_ended(self.slot, rand_game);
    }
    pub fn play_clip(&self, clip: u32, frame: u32, marker: u32, rand_game: &mut dyn FnMut() -> u32) -> u32 {
        self.lock().play_clip(self.slot, clip, frame, marker, rand_game)
    }
    pub fn update_layers(&self, dt: f32, rand_game: &mut dyn FnMut() -> u32) {
        self.lock().update_layers(self.slot, dt, rand_game);
    }
    pub fn clip(&self, layer: u32) -> u32 {
        self.lock().clip(self.slot, layer)
    }
    pub fn clip_type(&self, layer: u32) -> u32 {
        self.lock().clip_type(self.slot, layer)
    }
    pub fn at_end(&self, layer: u32) -> bool {
        self.lock().at_end(self.slot, layer)
    }
    pub fn layer_phase(&self, layer: u32) -> f32 {
        self.lock().layer_phase(self.slot, layer)
    }
    pub fn clip_duration(&self, layer: u32) -> f32 {
        self.lock().clip_duration(self.slot, layer)
    }
    pub fn test_flag(&self, flag: u32, layer: u32) -> bool {
        self.lock().test_flag(self.slot, flag, layer)
    }
    pub fn test_bit(&self, flag: u32, layer: u32) -> bool {
        self.lock().test_bit(self.slot, flag, layer)
    }
    pub fn set_stance(&self, stance: u32) {
        self.lock().set_stance(self.slot, stance);
    }
    pub fn set_all_layers_stance(&self, stance: u32) {
        self.lock().set_all_layers_stance(self.slot, stance);
    }
    pub fn set_bone_mask(&self, mask: u32) {
        self.lock().set_bone_mask(self.slot, mask);
    }
    pub fn turn_all_layers(&self, angle: f32) {
        self.lock().turn_all_layers(self.slot, angle);
    }
    pub fn blend_out_all(&self) {
        self.lock().blend_out_all(self.slot);
    }
    pub fn stop_category(&self, cat: u32) {
        self.lock().stop_category(self.slot, cat);
    }
    pub fn clip_displacement(&self, layer: u32, rand_game: &mut dyn FnMut() -> u32) -> [f32; 4] {
        self.lock().clip_displacement(self.slot, layer, rand_game)
    }
    pub fn play_launch_clip(&self, rand_game: &mut dyn FnMut() -> u32) -> Option<u32> {
        self.lock().play_launch_clip(self.slot, rand_game)
    }
    pub fn play_takeoff_clip(&self, row_1dc: u32, rand_game: &mut dyn FnMut() -> u32) -> u32 {
        self.lock().play_takeoff_clip(self.slot, row_1dc, rand_game)
    }
    pub fn play_exit_clip(&self, rand_game: &mut dyn FnMut() -> u32) {
        self.lock().play_exit_clip(self.slot, rand_game);
    }
    pub fn update_trick_clips(
        &self,
        held: u32,
        trick: u32,
        uber_timer: f32,
        vel_z: f32,
        rand_game: &mut dyn FnMut() -> u32,
    ) -> (u32, Vec<TrickClipCall>) {
        self.lock().update_trick_clips(self.slot, held, trick, uber_timer, vel_z, rand_game)
    }
    pub fn trick_state(&self) -> (u32, u32, u32) {
        self.lock().trick_state(self.slot)
    }
    pub fn read_pose(&self, part_idx: usize) -> Option<BlendedPose> {
        self.lock().read_pose(self.slot, part_idx)
    }
}
