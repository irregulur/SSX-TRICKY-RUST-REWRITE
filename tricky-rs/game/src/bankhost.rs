//! The bank manager and sound playback hosted (board row E1b2g7d5d): runs the ported, runner-checked
//! [`crate::banksys`] (`cAsyncSys`, `cBankManager`, `cBankSys`, 14 bank slots as `Audio_CreateSystem` 0x20daac
//! sets up `cBXAudio`) and [`crate::sndplay`] (`SndSys_PlayPending` 0x20c948 -> `SND_PlayBankSound` 0x2d1718)
//! in engine memory (`pf::Mem`), loading and decoding `.bnk` files directly from the disc (`tricky_data::bnk`).
//!
//! Slot layout proven from `Audio_CreateSystem` 0x20daac and `Audio_LoadInGameConfig` 0x211098:
//! - 14 slots (`0..=13`), queue depth 4 (`BankSys_Init(this, 14, 4)` 0x20c560).
//! - Slots `0..=5`: `MAIN` (0, `zbxsfx.bnk`), `BOARD` (1, `zboard.bnk`), `BANK` (2, course bank),
//!   `CROWD` (3, `Crowd.bnk`), `AUX` (4), `TRICKY` (5) loaded synchronously via `BankManager_LoadIfChanged` 0x20a460.
//! - Slot `6`: ambient swap bank, kind 1 fixed buffer (`BankManager_AllocStatic(this, 6, 0x19000, 0, 0)`),
//!   loaded asynchronously via `BankManager_LoadBank` 0x20a520 / `BankManager_UnloadBank` 0x20a508 and
//!   pumped each frame by `AsyncSys_Update` 0x209408 (`Audio_UpdateSystems` 0x21eb70).
//! - Slot `8`: kind 1 fixed buffer (`0x89800`), linked with slots `9..=11` (`BankManager_LinkSlots` 0x20a608).

use crate::banksys::{
    self as bs, ASYNC_CANCEL, ASYNC_FINISH, ASYNC_IS_DONE, ASYNC_LOAD, ASYNC_LOAD_INTO, BANK_IS_PLAYING,
    BANK_REGISTER, BANK_RELOCATE, BANK_RESIDENT_SIZE, BANK_UNREGISTER, BANKSYS_VTABLE, EAMEM_ALLOC, EAMEM_COPY,
    EAMEM_FREE, EAMEM_GET_SIZE, EAMEM_RESIZE, EAMEM_SET, FILE_LOAD_ALLOC, FILE_LOAD_INTO, MANAGER_VTABLE,
    MEM_DELETE, MEM_FREE, MONITOR_DTOR, MONITOR_INIT, NAME_STATIC, SND_LOCK, SND_UNLOCK, TIMER_WAIT, VF_ALLOC,
    VF_CONFIGURE, VF_RELEASE,
};
use crate::musicsong::SongEnv;
use crate::pf::Mem;
use crate::sndplay::{self as sp, ADD_INSTANCE, EV_ECHO, INSTANCE_SET, SCHEDULE, SND_PLAY_BANK_SOUND};
use crate::sndqueue::{self, Queue};
use std::collections::HashMap;
use tricky_data::audiocfg::IngameBanks;
use tricky_data::bnk::Bank;

const SYS_AT: u32 = 0x0108_0000;
const SYS_SIZE: usize = 0x4000;
const OWNER_AT: u32 = 0x0107_0000;
const OWNER_VTABLE: u32 = 0x0107_0100;
const TABLES_AT: u32 = 0x010c_0000;
const TABLES_SIZE: usize = 0x4000;
const HEAP_AT: u32 = 0x010d_0000;
const HEAP_SIZE: usize = 0x8000;
const STR_SCRATCH: u32 = TABLES_AT + 0x1000;
/// Synthetic virtual token base for static bank buffers and loaded `.bnk` blobs (never dereferenced in `pf::Mem`).
const DATA_TOKEN_BASE: u32 = 0x0200_0000;

const Q_RECS: u32 = TABLES_AT;
const Q_38: u32 = TABLES_AT + 0x100;
const Q_3C: u32 = TABLES_AT + 0x140;
const Q_40: u32 = TABLES_AT + 0x180;
const Q_64: u32 = TABLES_AT + 0x1c0;
const Q_68: u32 = TABLES_AT + 0x200;
const Q_6C: u32 = TABLES_AT + 0x240;
const Q_70: u32 = TABLES_AT + 0x280;
const Q_74: u32 = TABLES_AT + 0x2c0;
const Q_78: u32 = TABLES_AT + 0x340;
const Q_7C: u32 = TABLES_AT + 0x380;

/// Exact 8 bytes at `0x39e870` in `SLUS_203.26` (`SndSys_EchoCallback` 0x20c680 member pointer).
const EV_ECHO_BYTES: [u8; 8] = [0x00, 0x00, 0xff, 0xff, 0x80, 0xc6, 0x20, 0x00];

/// Number of bank slots created by `Audio_CreateSystem` 0x20daac (`cBXAudio::cBXAudio(this, 0xe, 4, ...)`).
pub const SLOT_COUNT: u32 = 14;

/// A voice start emitted by `SndSys_PlayPending` 0x20c948 -> `SND_PlayBankSound` 0x2d1718.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BankPlay {
    pub voice_handle: u32,
    pub bank_handle: u32,
    pub program: u32,
    pub volume: u8,
    pub bend: u8,
    pub note: u8,
    pub velocity: u8,
    pub pan: u16,
    pub pitch: u16,
}

struct PendingAsync {
    data_token: u32,
    size: u32,
    bank: Option<Bank>,
}

#[derive(Default)]
struct HostState {
    heap_bump: u32,
    next_token: u32,
    next_async: u32,
    next_bank_handle: u32,
    next_voice_handle: u32,
    alloc_sizes: HashMap<u32, u32>,
    pending_async: HashMap<u32, PendingAsync>,
    pending_data: HashMap<u32, (u32, Bank)>,
    banks: HashMap<u32, Bank>,
    last_play: Option<BankPlay>,
}

struct Env<'a, L: FnMut(&str) -> Option<Vec<u8>>> {
    st: &'a mut HostState,
    loader: L,
}

fn cstr(m: &Mem, at: u32) -> String {
    let bytes: Vec<u8> = (0..0x200).map(|k| m.r8(at + k)).take_while(|&b| b != 0).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn put_cstr(m: &mut Mem, at: u32, s: &[u8]) {
    for (k, &b) in s.iter().enumerate() {
        m.w8(at + k as u32, b);
    }
    m.w8(at + s.len() as u32, 0);
}

impl<L: FnMut(&str) -> Option<Vec<u8>>> SongEnv for Env<'_, L> {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32 {
        match target {
            MONITOR_INIT | MONITOR_DTOR | SND_LOCK | SND_UNLOCK | TIMER_WAIT | BANK_RELOCATE | INSTANCE_SET
            | SCHEDULE => 0,
            // Sound memory owner virtuals (`owner_call` +0x20/+0x30/+0x38): no SPU RAM limit on host.
            0x0020_0020 | 0x0020_0030 | 0x0020_0038 => 0,
            ADD_INSTANCE => 0,
            EAMEM_ALLOC => {
                let (name, size) = (args[0], args[1]);
                if name == NAME_STATIC || size > 0x2000 {
                    self.st.next_token += 1;
                    let tok = DATA_TOKEN_BASE + self.st.next_token * 0x1000;
                    self.st.alloc_sizes.insert(tok, size);
                    tok
                } else {
                    let aligned = (self.st.heap_bump + 0xf) & !0xf;
                    let ptr = HEAP_AT + aligned;
                    self.st.heap_bump = aligned + size.max(16);
                    assert!((self.st.heap_bump as usize) <= HEAP_SIZE, "bankhost heap overflow");
                    self.st.alloc_sizes.insert(ptr, size);
                    ptr
                }
            }
            EAMEM_SET => {
                let (dst, val, len) = (args[0], args[1] as u8, args[2]);
                if dst >= HEAP_AT && dst < HEAP_AT + HEAP_SIZE as u32 {
                    for k in 0..len {
                        m.w8(dst.wrapping_add(k), val);
                    }
                }
                dst
            }
            EAMEM_COPY => {
                let (dst, src, len) = (args[0], args[1], args[2]);
                if let Some(entry) = self.st.pending_data.remove(&src) {
                    self.st.pending_data.insert(dst, entry);
                } else if dst >= HEAP_AT && src >= HEAP_AT {
                    let bytes: Vec<u8> = (0..len).map(|k| m.r8(src.wrapping_add(k))).collect();
                    for (k, b) in bytes.into_iter().enumerate() {
                        m.w8(dst.wrapping_add(k as u32), b);
                    }
                }
                dst
            }
            EAMEM_FREE | MEM_FREE | MEM_DELETE => {
                let ptr = args[0];
                self.st.pending_data.remove(&ptr);
                self.st.alloc_sizes.remove(&ptr);
                0
            }
            EAMEM_GET_SIZE => self.st.alloc_sizes.get(&args[0]).copied().unwrap_or(0),
            EAMEM_RESIZE => {
                self.st.alloc_sizes.insert(args[0], args[1]);
                args[0]
            }
            FILE_LOAD_ALLOC => {
                let path = cstr(m, args[0]);
                let Some(raw) = (self.loader)(&path) else { return 0 };
                let size = raw.len() as u32;
                let Some(bank) = tricky_data::bnk::parse(&raw) else { return 0 };
                self.st.next_token += 1;
                let tok = DATA_TOKEN_BASE + self.st.next_token * 0x1000;
                self.st.alloc_sizes.insert(tok, size);
                self.st.pending_data.insert(tok, (size, bank));
                tok
            }
            FILE_LOAD_INTO => {
                let (path, at) = (cstr(m, args[0]), args[1]);
                let Some(raw) = (self.loader)(&path) else { return 0 };
                let size = raw.len() as u32;
                let Some(bank) = tricky_data::bnk::parse(&raw) else { return 0 };
                self.st.alloc_sizes.insert(at, size);
                self.st.pending_data.insert(at, (size, bank));
                size
            }
            ASYNC_LOAD => {
                let path = cstr(m, args[0]);
                let loaded = (self.loader)(&path).and_then(|raw| {
                    let size = raw.len() as u32;
                    tricky_data::bnk::parse(&raw).map(|b| (size, b))
                });
                self.st.next_async += 1;
                let h = self.st.next_async;
                self.st.next_token += 1;
                let tok = DATA_TOKEN_BASE + self.st.next_token * 0x1000;
                let (size, bank) = match loaded {
                    Some((sz, b)) => (sz, Some(b)),
                    None => (0, None),
                };
                self.st.pending_async.insert(h, PendingAsync { data_token: tok, size, bank });
                h
            }
            ASYNC_LOAD_INTO => {
                let (path, at) = (cstr(m, args[0]), args[1]);
                let loaded = (self.loader)(&path).and_then(|raw| {
                    let size = raw.len() as u32;
                    tricky_data::bnk::parse(&raw).map(|b| (size, b))
                });
                self.st.next_async += 1;
                let h = self.st.next_async;
                let (size, bank) = match loaded {
                    Some((sz, b)) => (sz, Some(b)),
                    None => (0, None),
                };
                self.st.pending_async.insert(h, PendingAsync { data_token: at, size, bank });
                h
            }
            ASYNC_IS_DONE => {
                if self.st.pending_async.contains_key(&args[0]) {
                    1
                } else {
                    2
                }
            }
            ASYNC_CANCEL => {
                if self.st.pending_async.remove(&args[0]).is_some() {
                    1
                } else {
                    0
                }
            }
            ASYNC_FINISH => {
                self.st.pending_async.remove(&args[0]);
                0
            }
            BANK_UNREGISTER => {
                self.st.banks.remove(&args[0]);
                0
            }
            BANK_RESIDENT_SIZE => 0x20,
            BANK_IS_PLAYING => self.st.banks.contains_key(&args[0]) as u32,
            SND_PLAY_BANK_SOUND => {
                let (bank_handle, program, opts) = (args[0], args[1], args[2]);
                if bank_handle == u32::MAX || !self.st.banks.contains_key(&bank_handle) {
                    return u32::MAX;
                }
                let volume = m.r8(opts);
                let bend = m.r8(opts + 1);
                let note = m.r8(opts + 3);
                let velocity = m.r8(opts + 4);
                let pan = m.r16(opts + 8);
                let pitch = m.r16(opts + 0x0c);
                self.st.next_voice_handle += 1;
                let voice_handle = self.st.next_voice_handle;
                self.st.last_play = Some(BankPlay {
                    voice_handle,
                    bank_handle,
                    program,
                    volume,
                    bend,
                    note,
                    velocity,
                    pan,
                    pitch,
                });
                voice_handle
            }
            other => panic!("bankhost: unexpected call {other:#x}"),
        }
    }

    fn call_f(&mut self, _: &mut Mem, target: u32, _: &[u32]) -> f32 {
        panic!("bankhost: unexpected call_f {target:#x}")
    }

    fn call_out(&mut self, _: &mut Mem, target: u32, args: &[u32], _: usize) -> Vec<u32> {
        match target {
            ASYNC_FINISH => {
                let Some(p) = self.st.pending_async.remove(&args[0]) else {
                    return vec![0, 0];
                };
                if let Some(bank) = p.bank {
                    self.st.alloc_sizes.insert(p.data_token, p.size);
                    self.st.pending_data.insert(p.data_token, (p.size, bank));
                    vec![p.data_token, p.size]
                } else {
                    vec![0, 0]
                }
            }
            BANK_REGISTER => {
                let tok = args[0];
                if let Some((_, bank)) = self.st.pending_data.remove(&tok) {
                    self.st.next_bank_handle += 1;
                    let h = self.st.next_bank_handle;
                    self.st.banks.insert(h, bank);
                    // Return 8 (non-resident/SPU bank registered cleanly in `slot_load` 0x209cf8).
                    vec![8, h]
                } else {
                    vec![0, u32::MAX]
                }
            }
            other => panic!("bankhost: unexpected call_out {other:#x}"),
        }
    }
}

/// Hosted `cBankSys` (`banksys`) + `SndSys_PlayPending` (`sndplay`) with `cBXAudio`'s 14 bank slots and 7
/// channel volume slots (`SndMgr_GetSlot` 0x211f38).
pub struct BankHost {
    mem: Mem,
    st: HostState,
}

impl Default for BankHost {
    fn default() -> Self {
        Self::new()
    }
}

impl BankHost {
    /// Construct `cBankSys` with `SLOT_COUNT` (14) slots and queue depth 4 (`Audio_CreateSystem` 0x20daac),
    /// plus `Audio_LoadInGameConfig` 0x211098's static slots (6 = `0x19000`, 8 = `0x89800`, linked `9..=11`).
    pub fn new() -> Self {
        let mut mem = Mem {
            regions: vec![
                (0x0034_2c00, vec![0; 0x100]),
                (0x0039_e600, vec![0; 0x300]),
                (OWNER_AT, vec![0; 0x200]),
                (SYS_AT, vec![0; SYS_SIZE]),
                (TABLES_AT, vec![0; TABLES_SIZE]),
                (HEAP_AT, vec![0; HEAP_SIZE]),
            ],
        };
        // Sound-memory owner object at `SYS+0` with vtable at `OWNER_VTABLE`
        mem.w32(OWNER_AT, OWNER_VTABLE);
        for (off, target) in [(0x20u32, 0x0020_0020u32), (0x30, 0x0020_0030), (0x38, 0x0020_0038)] {
            mem.w16(OWNER_VTABLE + off, 0);
            mem.w32(OWNER_VTABLE + off + 4, target);
        }
        mem.w32(SYS_AT, OWNER_AT);
        // `cBankManager` / `cBankSys` vtables: virtual slots +0x50/+0x58/+0x60 -> ported `VF_RELEASE`/`ALLOC`/`CONFIGURE`
        for vt in [MANAGER_VTABLE, BANKSYS_VTABLE] {
            for (off, f) in [(0x50u32, VF_RELEASE), (0x58, VF_ALLOC), (0x60, VF_CONFIGURE)] {
                mem.w16(vt + off, 0);
                mem.w32(vt + off + 4, f);
            }
        }
        for (k, &b) in EV_ECHO_BYTES.iter().enumerate() {
            mem.w8(EV_ECHO + k as u32, b);
        }
        // `cSndPlayOpts` header at `SYS+8..+0x20` and pending-sound queue tables
        let hdr = sndqueue::play_opts_header();
        for (k, &b) in hdr.iter().enumerate() {
            mem.w8(SYS_AT + 8 + k as u32, b);
        }
        mem.w32(SYS_AT + 0x20, u32::MAX);
        for (off, ptr) in [
            (0x24u32, Q_RECS), (0x38, Q_38), (0x3c, Q_3C), (0x40, Q_40), (0x64, Q_64),
            (0x68, Q_68), (0x6c, Q_6C), (0x70, Q_70), (0x74, Q_74), (0x78, Q_78), (0x7c, Q_7C),
        ] {
            mem.w32(SYS_AT + off, ptr);
        }
        // Default all 7 sound manager channel slots (`SYS+0x3848 + 12*ch + 8`) to 1.0
        for ch in 0..7u32 {
            mem.w32(Self::slot_addr(ch), 1.0f32.to_bits());
        }

        let mut st = HostState::default();
        {
            let mut env = Env { st: &mut st, loader: |_| None };
            bs::bank_sys_init(&mut mem, &mut env, SYS_AT, SLOT_COUNT, 4);
            bs::async_alloc_buffer(&mut mem, &mut env, SYS_AT, 0, 0);
            // `Audio_LoadInGameConfig` 0x2115e4..0x211648:
            // slot 6 (ambient swap) static 0x19000, slot 8 static 0x89800, slots 9..=11 linked to 8.
            bs::manager_alloc_static(&mut mem, &mut env, SYS_AT, 6, 0x19000, 0, 0);
            bs::manager_alloc_static(&mut mem, &mut env, SYS_AT, 8, 0x89800, 0, 0);
            for linked in 9..=11u32 {
                bs::manager_link(&mut mem, SYS_AT, linked, 8);
            }
        }
        BankHost { mem, st }
    }

    /// Address of `SndMgr_GetSlot(ch)` (`0x211f38`): `audio + 0x3848 + 0xc * ch + 8` (`ch` in `0..7`).
    pub const fn slot_addr(ch: u32) -> u32 {
        SYS_AT + 0x3848 + ch * 0x0c + 8
    }

    /// `Audio_LoadInGameConfig` 0x211098's `BANKS.INF` loads (`MAIN`..`TRICKY` -> slots `0..=5`) through
    /// `BankManager_LoadIfChanged` 0x20a460 (`banksys::manager_load_now`).
    pub fn load_ingame_banks(&mut self, cfg: &IngameBanks, mut loader: impl FnMut(&str) -> Option<Vec<u8>>) {
        for b in &cfg.loads {
            if b.slot < SLOT_COUNT {
                self.load_if_changed(b.slot, &b.path, 0, &mut loader);
            }
        }
    }

    /// `BankManager_LoadIfChanged` 0x20a460: synchronously load `path` into `slot` unless already loaded.
    pub fn load_if_changed(&mut self, slot: u32, path: &[u8], flags: u32, loader: impl FnMut(&str) -> Option<Vec<u8>>) {
        if slot >= SLOT_COUNT {
            return;
        }
        put_cstr(&mut self.mem, STR_SCRATCH, path);
        let mut env = Env { st: &mut self.st, loader };
        bs::manager_load_now(&mut self.mem, &mut env, SYS_AT, slot, STR_SCRATCH, flags);
    }

    /// `BankManager_LoadBank` 0x20a520: queue async load of `path` into `slot` (e.g. slot 6 ambient swap bank).
    pub fn load_bank(&mut self, slot: u32, path: &[u8], flags: u32, loader: impl FnMut(&str) -> Option<Vec<u8>>) {
        if slot >= SLOT_COUNT {
            return;
        }
        put_cstr(&mut self.mem, STR_SCRATCH, path);
        let mut env = Env { st: &mut self.st, loader };
        bs::manager_load(&mut self.mem, &mut env, SYS_AT, slot, STR_SCRATCH, flags);
    }

    /// `BankManager_UnloadBank` 0x20a508: unload the bank in `slot`.
    pub fn unload_bank(&mut self, slot: u32) {
        if slot >= SLOT_COUNT {
            return;
        }
        let mut env = Env { st: &mut self.st, loader: |_| None };
        bs::manager_unload(&mut self.mem, &mut env, SYS_AT, slot);
    }

    /// `AsyncSys_Update` 0x209408 (called from `Audio_UpdateSystems` 0x21eb70 each frame).
    pub fn update(&mut self, loader: impl FnMut(&str) -> Option<Vec<u8>>) {
        let mut env = Env { st: &mut self.st, loader };
        bs::async_update(&mut self.mem, &mut env, SYS_AT);
    }

    /// State of `slot` (`0` unloaded, `1` loaded, `2` loading), read directly from `cBankManager`'s slot table.
    pub fn slot_state(&self, slot: u32) -> i32 {
        if slot >= SLOT_COUNT {
            return 0;
        }
        let base = self.mem.r32(SYS_AT + 0xa3c);
        self.mem.r32(base + slot * 0x60) as i32
    }

    /// `BankManager_GetHandle` 0x20a6b0: handle of the bank loaded in `slot`, or `None`.
    pub fn slot_handle(&self, slot: u32) -> Option<u32> {
        if slot >= SLOT_COUNT {
            return None;
        }
        let h = bs::manager_handle(&self.mem, SYS_AT, slot);
        (h != u32::MAX).then_some(h)
    }

    /// Decoded `BNKl` bank currently registered under `bank_handle`.
    pub fn bank(&self, bank_handle: u32) -> Option<&Bank> {
        self.st.banks.get(&bank_handle)
    }

    /// Decoded `BNKl` bank currently loaded in `slot`.
    pub fn bank_in_slot(&self, slot: u32) -> Option<&Bank> {
        let h = self.slot_handle(slot)?;
        self.bank(h)
    }

    /// Next voice handle that `SND_PlayBankSound` 0x2d1718 will allocate on a successful play.
    pub fn peek_next_voice_handle(&self) -> u32 {
        self.st.next_voice_handle + 1
    }

    /// `SndSys_PlayPending` 0x20c948 -> `SND_PlayBankSound` 0x2d1718 on the top entry of `q` (which is popped).
    /// `channel_slots` holds the 7 `SndMgr_GetSlot(0..7)` volume floats from `ui::AudioLevels`.
    pub fn play_from_queue(&mut self, q: &mut Queue, channel_slots: &[f32; 7]) -> (u32, Option<BankPlay>) {
        if q.count < 0 {
            return (u32::MAX, None);
        }
        for (ch, &v) in channel_slots.iter().enumerate() {
            self.mem.w32(Self::slot_addr(ch as u32), v.to_bits());
        }
        let i = q.count as usize;
        q.count -= 1;
        // Stage this entry at index 0 of `cBankSys`'s queue and run `sndplay::play_pending` 0x20c948.
        self.mem.w32(SYS_AT + 0x20, 0);
        for (k, &b) in q.record[i].iter().enumerate() {
            self.mem.w8(Q_RECS + k as u32, b);
        }
        self.mem.w32(SYS_AT + 0x28, q.emitter[i]);
        self.mem.w32(Q_38, q.a38[i]);
        self.mem.w32(Q_3C, q.a3c[i]);
        self.mem.w32(Q_40, q.a40[i]);
        self.mem.w32(Q_64, q.a64[i]);
        self.mem.w32(Q_68, q.a68[i]);
        self.mem.w32(Q_6C, q.a6c[i]);
        self.mem.w32(Q_70, q.a70[i]);
        for k in 0..4u32 {
            self.mem.w32(Q_74 + k * 4, q.a74[i][k as usize]);
        }
        self.mem.w32(Q_78, q.a78[i]);
        self.mem.w32(Q_7C, q.a7c[i]);
        let kinds = self.mem.r32(SYS_AT + 0xa40);
        let ids = self.mem.r32(SYS_AT + 0xa44);
        self.mem.w32(kinds, q.kind[i]);
        self.mem.w32(ids, q.id[i]);

        self.st.last_play = None;
        let mut env = Env { st: &mut self.st, loader: |_| None };
        let handle = sp::play_pending(&mut self.mem, &mut env, SYS_AT);
        (handle, self.st.last_play.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_host_loads_ingame_banks_swaps_slot6_and_plays_from_disc() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let disc = tricky_data::disc::Disc::open(&disc_path).expect("open disc");
        let mut files = tricky_data::big::Files::new(disc);
        files.mount_game_archives().expect("mount");
        let banks_inf = files.load("data/config/banks.inf").expect("load").expect("banks.inf missing");
        let cfg = tricky_data::audiocfg::ingame_banks(&banks_inf, b"GARI");

        let mut host = BankHost::new();
        host.load_ingame_banks(&cfg, |p| files.load(p).ok().flatten());
        // Slots 0..=3 (MAIN zbxsfx, BOARD zboard, BANK garibaldi1, CROWD Crowd.bnk) and 5 (tricky.bnk) loaded!
        for s in [0u32, 1, 2, 3, 5] {
            assert_eq!(host.slot_state(s), 1, "slot {s} should be loaded");
            assert!(host.bank_in_slot(s).is_some(), "slot {s} decoded bank missing");
        }

        // Slot 6 async load (as `Audio_LoadAmbientBank` 0x21a548 -> `BankManager_LoadBank` 0x20a520 does).
        assert_eq!(host.slot_state(6), 0);
        let first_swap = cfg.swaps.first().expect("GARI has SWAP banks");
        host.load_bank(6, first_swap, 0x10, |p| files.load(p).ok().flatten());
        // Pumping `AsyncSys_Update` 0x209408 finishes the queued async load into slot 6.
        host.update(|p| files.load(p).ok().flatten());
        assert_eq!(host.slot_state(6), 1, "slot 6 should finish loading after AsyncSys_Update");
        let swap_bank = host.bank_in_slot(6).expect("slot 6 bank decoded");
        assert!(!swap_bank.programs.is_empty());

        // `SndSys_PlayPending` 0x20c948 on slot 0 (zbxsfx.bnk, program 0x6d as `Sfx_OnPlacingChange` 0x219a48
        // queues) with slot 4 scaling = 0.5 -> volume 127 * 0.5 = 63.
        let mut q = Queue { header: sndqueue::play_opts_header(), ..Queue::with_capacity(8) };
        let i = sndqueue::queue_begin(&mut q, 0, 0x6d, Some(127));
        q.a40[i] = BankHost::slot_addr(4);
        let slots = [1.0, 1.0, 1.0, 1.0, 0.5, 1.0, 1.0];
        let (vh, play) = host.play_from_queue(&mut q, &slots);
        assert_ne!(vh, u32::MAX);
        let play = play.expect("expected SND_PlayBankSound");
        assert_eq!(play.program, 0x6d);
        assert_eq!(play.volume, 63);
        assert!(host.bank(play.bank_handle).unwrap().programs.contains_key(&0x6d));

        // `SndSys_PlayPending` 0x20c948 on slot 6 (ambient swap bank, program 0 at volume 0x23 as
        // `Audio_PlayEventSound` 0x21d0c0 queues) with slot 4 scaling = 1.0 -> volume 0x23.
        let i2 = sndqueue::queue_begin(&mut q, 6, 0, Some(0x23));
        q.a40[i2] = BankHost::slot_addr(4);
        let slots_full = [1.0; 7];
        let (vh2, play2) = host.play_from_queue(&mut q, &slots_full);
        assert_ne!(vh2, u32::MAX);
        let play2 = play2.expect("expected SND_PlayBankSound for slot 6");
        assert_eq!(play2.program, 0);
        assert_eq!(play2.volume, 0x23);
        assert!(host.bank(play2.bank_handle).unwrap().programs.contains_key(&0));

        // `BankManager_UnloadBank(6)` empties slot 6 cleanly.
        host.unload_bank(6);
        assert_eq!(host.slot_state(6), 0);
        assert!(host.bank_in_slot(6).is_none());
    }
}
