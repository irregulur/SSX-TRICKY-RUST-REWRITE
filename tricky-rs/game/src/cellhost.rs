//! Live world-cell, controller, and script-thread host (board rows E7b2, E9b2): drives `AiCells::tick`
//! (`cAIWorld::vf1` 0x115e28) and List 3 (`BxList_Add` 0x17d4e8, `BxList_UpdateAll` 0x17d950,
//! `BxList_MergePending` 0x17d548, `cBxSortObjNode` 0x17d0e0) each 60 Hz frame on a course's `.pbd`, `.ltg`, and
//! `.ssf` (`CourseGround`), executing live `cScriptNode` threads (`0x13b8c0`, `0x13b978`, `0x13bc78`,
//! `World_RunScript` 0x13bad8, `World_DeleteScriptObject` 0x13bbc0, `World_FindScript` 0x260dd8), spawning and
//! stepping `cAnimObjectNode` (0x100, `objanim::AnimObject` 0x198810 / 0x199058), `cAnimDeltaNode` (0x101,
//! `objanim::AnimDelta` 0x199d10 / 0x199db8), and logic controllers (`cDebounceNode` 2, `cDeadNode` 5,
//! `cCounterNode` 6, `cTimerNode` 8, `cRestoreNode` 0x3ee, `controllers::Controller`) via
//! `TriggerScript_Op0_SpawnController` 0x13c5d0 / `TriggerScript_ApplyControllerOp` 0x13ac88, and posing animated
//! collision parts on `CourseGround` through `meshanim::mesh_frame` (`MeshAnim_ApplyFrames` 0x199258 /
//! `MeshAnimNode_GetFrameMatrix` 0x1998d0).

use std::collections::{HashMap, HashSet};

use tricky_data::anim::ObjAnim;
use tricky_data::pbd::Pbd;
use tricky_data::ssf::{Op, Ssf};

use crate::cells::{AiCells, CellEnv, RiderCell};
use crate::controllers::{self, restore_flags, Apply, Controller, CtrlCall, CtrlEnv, Kind, Spawn};
use crate::course_ground::CourseGround;
use crate::meshanim;
use crate::objanim::{AnimDelta, AnimObject, ObjAnimCall, ObjAnimEnv};
use crate::script::{self, ScriptCall, ScriptEnv, ScriptRef, Thread};
use crate::texanim::AnimEnv;
use crate::world::World;

type V4 = [f32; 4];

/// Per-op side effect emitted by live `.ssf` script threads (`tricky_game::script`, row E9b2) for the app layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptEvent {
    /// Op 0 spawned a controller on `object` (raw `.ssf` op-0 `args`: `[sub_type, ...]`).
    SpawnController { object: usize, args: Vec<u32> },
    /// Ops 3 / 9 sent a controller message `(p2, p3)` to `object`.
    ControllerMessage { object: usize, p2: u32, p3: u32 },
    /// Op 0 sub-type 5 (`cDeadNode` mode 2..=3 or `cRestoreNode` mode 4) hid `object` (`flags & 0xffffff0d | 4`).
    DeadObject(usize),
    /// Op 0 sub-type 5 (`RestoreFlags` mode 0 or killed `cRestoreNode`) restored `object`.
    RestoreObject(usize),
    /// Op 25 (`RailMan_SetRail` 0x149030): spline rail switched on or off.
    SetRail { spline: u32, on: bool },
    /// Op 8 (`Sfx_ScriptSoundPlay` 0x2165c8).
    ScriptSound { object: usize, sound: u32 },
    /// Op 13 (`Boarder_RequestReset` 0x118f10) for `rider` handle.
    ResetRider(u32),
    /// Op 24 (`Boarder_TeleportToInstance` 0x11a098): teleport `rider` to `object`.
    Teleport { rider: u32, object: usize },
}

/// Events produced by `CellHost` (`tick`, `run_script_named`, `run_event_script`) for the app's visual/gameplay layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellEvents {
    /// `(object, effect_id)` scripts started by cell activation, deactivation, or controller finish.
    pub activated_scripts: Vec<(usize, i32)>,
    /// Objects whose controller was killed or finished this tick (so visual controllers on them stop).
    pub killed_objects: Vec<usize>,
    /// Ordered side effects executed by live `.ssf` script threads (`tricky_game::script`).
    pub script_events: Vec<ScriptEvent>,
}

/// What kind of List 3 node an entry represents (`cBxSortObjNode` 0x17d0e0 on list 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum List3Kind {
    Controller(usize),
    Script(u64),
}

/// One node in List 3 (`0x337c60`, list 3): sorted descending by `sort_id` (`+0x20`, `cBxSortObjNode::vf3` 0x17dcb0),
/// so `BxList_UpdateAll` 0x17d950 walks newest-first (head to tail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct List3Entry {
    sort_id: u32,
    is_dead: bool,
    kind: List3Kind,
}

/// A live/waiting `cScriptNode` (`type 0x3e9`) plus the rider context (`+0x150` velocity, `+0x41c` human flag)
/// visible to `TriggerScript_ExecOp` `0x13bfd0` op 5.
#[derive(Clone, Debug)]
struct ThreadState {
    thread: Thread,
    vel: V4,
    human: bool,
}

/// `World_FindScript` 0x260dd8: case-insensitive (`stricmp`) search of `ssf.functions`.
pub fn find_script(ssf: &Ssf, name: &str) -> Option<u32> {
    ssf.functions.iter().position(|f| f.name.eq_ignore_ascii_case(name)).map(|i| i as u32)
}

/// Live cell activation, List-3 script threads (`cScriptNode`), controller state, and keyframed animation clocks
/// for one course (`CourseGround`).
pub struct CellHost {
    pub ai_cells: AiCells,
    pub rider_cells: Vec<i32>,
    model_ends: Vec<f32>,
    inst_models: Vec<i32>,
    model_anims: Vec<Vec<ObjAnim>>,
    ctrl: Vec<Option<u32>>,
    anim_nodes: HashMap<usize, AnimObject>,
    delta_nodes: HashMap<usize, AnimDelta>,
    logic_nodes: HashMap<usize, Controller>,
    /// Live List 3 nodes (`BxList_First`..`BxList_NextOrNull`), sorted descending by `sort_id` (newest first).
    live_list3: Vec<List3Entry>,
    /// Pending List 3 nodes created this frame (`BxList_Add` 0x17d4e8 on sorted list 3 -> `BxList_MergePending` 0x17d548).
    pending_list3: Vec<List3Entry>,
    /// `COUNTER` 0x337c6c (non-type-5 `cBxSortObjNode` sort IDs).
    next_sort_id: u32,
    /// `COUNTER5` 0x337c68 (type-5 `cDeadNode` sort IDs).
    next_dead_id: u32,
    next_thread_id: u64,
    /// Objects whose controller was spawned this frame (`pending_list3`).
    pending: HashSet<usize>,
    chans: HashMap<usize, Vec<[f32; 16]>>,
    threads: HashMap<u64, ThreadState>,
    events: CellEvents,
}

impl CellHost {
    /// Build a fresh cell/controller/script host from a course's parsed `.pbd` and instance count.
    pub fn new(pbd_bytes: &[u8], pbd: &Pbd, n_objects: usize) -> Self {
        let model_ends = pbd.models.iter().map(|m| f32::from_bits(m.head[5])).collect();
        let inst_models = pbd.instances.iter().map(|r| r[0x30] as i32).collect();
        let model_anims = pbd
            .models
            .iter()
            .map(|m| {
                m.nodes
                    .iter()
                    .filter_map(|n| n.animation.as_ref())
                    .filter_map(|a| tricky_data::level::node_animation(pbd_bytes, a).ok())
                    .collect()
            })
            .collect();
        Self {
            ai_cells: AiCells::default(),
            rider_cells: Vec::new(),
            model_ends,
            inst_models,
            model_anims,
            ctrl: vec![None; n_objects],
            anim_nodes: HashMap::new(),
            delta_nodes: HashMap::new(),
            logic_nodes: HashMap::new(),
            live_list3: Vec::new(),
            pending_list3: Vec::new(),
            next_sort_id: 0,
            next_dead_id: 0,
            next_thread_id: 1,
            pending: HashSet::new(),
            chans: HashMap::new(),
            threads: HashMap::new(),
            events: CellEvents::default(),
        }
    }

    /// Controller type (`+0x14`) currently attached to `object` in `CellHost`, if any.
    pub fn controller(&self, object: usize) -> Option<u32> {
        self.ctrl.get(object).copied().flatten()
    }

    /// Current clip time (seconds) for an active `cAnimObjectNode` (0x100/0x102/0x103) or `cAnimDeltaNode` (0x101).
    pub fn anim_time(&self, object: usize) -> Option<f32> {
        self.anim_nodes
            .get(&object)
            .map(|a| a.time)
            .or_else(|| self.delta_nodes.get(&object).map(|d| d.anim.time))
    }

    /// Whether `object` currently has a controller that blocks a new contact script (`Instance_OnPlayerContact`
    /// 0x13bd40: non-roller controllers return 0 from vf +0x64; an `AnimObject` only re-runs contact when
    /// `cooldown == 0`).
    pub fn blocks_contact(&self, object: usize) -> bool {
        let Some(ty) = self.controller(object) else { return false };
        if let Some(a) = self.anim_nodes.get(&object) {
            return a.cooldown != 0;
        }
        if let Some(d) = self.delta_nodes.get(&object) {
            return d.anim.cooldown != 0;
        }
        ty != 0
    }

    /// Number of currently occupied active cell slots in `AiCells`.
    pub fn active_cell_count(&self) -> usize {
        self.ai_cells.slots.iter().filter(|s| s.cell.is_some()).count()
    }

    /// Number of waiting `cScriptNode` threads (live + pending on List 3).
    pub fn active_thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Record a `ScriptEvent` produced outside `CellHost` (e.g. `Knockables::contact` thread op).
    pub fn record_event(&mut self, ev: ScriptEvent) {
        self.events.script_events.push(ev);
    }

    /// `World_DeleteScriptObject` 0x13bbc0: walks live List 3 head-to-tail (`BxList_First`..`BxList_NextOrNull`)
    /// and deletes the first `cScriptNode` (`type 0x3e9`) running function `fn_idx`.
    pub fn delete_script_object(&mut self, fn_idx: u32) -> bool {
        let target = ScriptRef::Function(fn_idx);
        let pos = self.live_list3.iter().position(|e| match e.kind {
            List3Kind::Script(tid) => self.threads.get(&tid).is_some_and(|st| st.thread.script == target),
            List3Kind::Controller(_) => false,
        });
        if let Some(i) = pos {
            if let List3Kind::Script(tid) = self.live_list3.remove(i).kind {
                self.threads.remove(&tid);
                return true;
            }
        }
        false
    }

    /// Allocate the next `cBxSortObjNode` sort ID (`COUNTER` 0x337c6c, or `COUNTER5` 0x337c68 for type 5) and add
    /// `kind` onto List 3's pending list (`BxList_Add` 0x17d4e8).
    fn add_pending_node(&mut self, is_dead: bool, kind: List3Kind) -> u32 {
        let sort_id = if is_dead {
            let id = self.next_dead_id;
            self.next_dead_id = id.wrapping_add(1);
            id
        } else {
            let id = self.next_sort_id;
            self.next_sort_id = id.wrapping_add(1);
            id
        };
        // `BxList_Add` links new nodes at the head of the pending list (newest first).
        self.pending_list3.insert(0, List3Entry { sort_id, is_dead, kind });
        sort_id
    }

    /// `BxList_MergePending` 0x17d548 on sorted List 3: walks `pending_list3` first-to-last and inserts each node
    /// into `live_list3` in descending `sort_id` order (`cBxSortObjNode::vf3` 0x17dcb0: `new.sort_id <= existing.sort_id`,
    /// walking tail-to-head so higher `sort_id` lands closer to the head).
    fn merge_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending_list3);
        for node in pending {
            // Walk live_list3 from tail to head (`BxList_SortedFirst` / `SortedNextOrNull`) to find the first
            // existing node where `node.sort_id <= existing.sort_id`, and insert immediately AFTER it (or at head).
            let mut insert_at = 0usize;
            for idx in (0..self.live_list3.len()).rev() {
                if node.sort_id <= self.live_list3[idx].sort_id {
                    insert_at = idx + 1;
                    break;
                }
            }
            self.live_list3.insert(insert_at, node);
        }
        self.pending.clear();
    }

    /// Remove any List 3 entries (live or pending) belonging to controller `o`.
    fn unlink_controller_node(&mut self, o: usize) {
        self.live_list3.retain(|e| e.kind != List3Kind::Controller(o));
        self.pending_list3.retain(|e| e.kind != List3Kind::Controller(o));
    }

    /// Drain queued visual/script events since the last call.
    pub fn drain_events(&mut self) -> CellEvents {
        std::mem::take(&mut self.events)
    }

    /// `AIWorld_ClearSlots` 0x115df0 + `World_ResetInstances` 0x115fe8: clear all live cell slots, controllers,
    /// animation clocks, List 3 nodes/threads, and pending poses. Caller also resets `CourseGround` flags and poses.
    pub fn clear(&mut self) {
        self.ai_cells = AiCells::default();
        self.rider_cells.clear();
        self.ctrl.fill(None);
        self.anim_nodes.clear();
        self.delta_nodes.clear();
        self.logic_nodes.clear();
        self.live_list3.clear();
        self.pending_list3.clear();
        self.next_sort_id = 0;
        self.next_dead_id = 0;
        self.next_thread_id = 1;
        self.pending.clear();
        self.chans.clear();
        self.threads.clear();
        self.events = CellEvents::default();
    }

    fn model_end(&self, object: usize) -> f32 {
        let m = self.inst_models.get(object).copied().unwrap_or(-1);
        if m < 0 { 0.0 } else { self.model_ends.get(m as usize).copied().unwrap_or(0.0) }
    }

    fn pose_frames(&mut self, object: usize, time: f32) -> Option<Vec<[V4; 4]>> {
        let m = self.inst_models.get(object).copied().unwrap_or(-1);
        if m < 0 { return None; }
        let anims = self.model_anims.get(m as usize)?;
        if anims.is_empty() { return None; }
        let ch = self.chans.entry(object).or_insert_with(|| {
            anims.iter().map(|a| {
                let mut c = meshanim::key_channels(a);
                let _ = meshanim::mesh_frame(a, time, &mut c);
                c
            }).collect()
        });
        if ch.len() != anims.len() {
            *ch = anims.iter().map(meshanim::key_channels).collect();
        }
        Some(anims.iter().zip(ch.iter_mut()).map(|(a, c)| meshanim::mesh_frame(a, time, c)).collect())
    }
}

/// Changes to `CourseGround` accumulated while `CellHost` executes without holding `CourseGround`'s mutexes.
#[derive(Default)]
struct SyncBatch {
    pose: Vec<(usize, u32, Vec<[V4; 4]>)>,
    unpose: Vec<usize>,
    ctrl: Vec<(usize, Option<u32>)>,
    rails: Vec<(u32, bool)>,
    wake_rollers: Vec<u32>,
}

/// Execution context for `AiCells::tick`, controllers, and trigger scripts on a snapshot of `CourseGround`'s flags.
struct TickCtx<'a> {
    host: &'a mut CellHost,
    ssf: &'a Ssf,
    flags: Vec<u32>,
    /// Rollers currently alive in `CourseGround::knock`: `object -> asleep`.
    rollers: HashMap<usize, bool>,
    rand_game: &'a mut dyn FnMut() -> u32,
    batch: SyncBatch,
    /// Queued `(script_id, object)` calls emitted by controller `Update` / `Finish`.
    starts: Vec<(i32, usize)>,
    /// Currently executing thread state: `(script, object, ctx_a, ctx_b, vel, human)`.
    cur: Option<(ScriptRef, u32, Option<script::Context>, Option<script::Context>, V4, bool)>,
    depth: u32,
}

impl<'a> TickCtx<'a> {
    fn detach_controller(&mut self, o: usize) {
        self.host.ctrl[o] = None;
        self.host.anim_nodes.remove(&o);
        self.host.delta_nodes.remove(&o);
        self.host.logic_nodes.remove(&o);
        self.host.pending.remove(&o);
        self.host.unlink_controller_node(o);
        self.host.chans.remove(&o);
        self.batch.unpose.push(o);
        self.batch.ctrl.push((o, None));
        self.host.events.killed_objects.push(o);
    }

    fn kill_controller(&mut self, o: usize) {
        if let Some(mut a) = self.host.anim_nodes.remove(&o) {
            a.kill(self);
            self.detach_controller(o);
            return;
        }
        if let Some(mut d) = self.host.delta_nodes.remove(&o) {
            d.anim.kill(self);
            self.detach_controller(o);
            return;
        }
        if let Some(mut l) = self.host.logic_nodes.remove(&o) {
            let was_restore = l.kind == Kind::Restore;
            l.kill(self);
            if was_restore {
                self.host.events.script_events.push(ScriptEvent::RestoreObject(o));
            }
            if l.kind != Kind::Dead {
                self.detach_controller(o);
            }
            return;
        }
        if let Some(ty) = self.host.ctrl.get(o).copied().flatten() {
            if ty != 5 {
                self.detach_controller(o);
            }
        }
    }

    fn finish_controller(&mut self, o: usize, flag: u32) {
        if let Some(mut l) = self.host.logic_nodes.remove(&o) {
            let mut deleted = false;
            let mut starts = Vec::new();
            {
                let mut sub = SubCtrlEnv { flags: &mut self.flags, ssf: self.ssf, deleted: &mut deleted, starts: &mut starts };
                l.finish(flag, &mut sub);
            }
            if deleted {
                self.detach_controller(o);
            } else {
                self.host.logic_nodes.insert(o, l);
            }
            self.starts.extend(starts);
            return;
        }
        if self.host.anim_nodes.contains_key(&o) || self.host.delta_nodes.contains_key(&o) {
            if flag != 0 {
                let s = self.ssf.event_script(o, 3);
                if s >= 0 {
                    self.starts.push((s, o));
                    return;
                }
            }
            if let Some(f) = self.flags.get_mut(o) {
                *f = restore_flags(*f, 0xffff_0100);
            }
            self.detach_controller(o);
        }
    }

    fn message_controller(&mut self, o: usize, p2: u32, p3: u32) {
        self.host.events.script_events.push(ScriptEvent::ControllerMessage { object: o, p2, p3 });
        let val = f32::from_bits(p3);
        if let Some(mut a) = self.host.anim_nodes.remove(&o) {
            a.on_message(val, p2, self);
            let t = a.time;
            let ty = self.host.ctrl[o].unwrap_or(0x100);
            self.host.anim_nodes.insert(o, a);
            if let Some(frames) = self.host.pose_frames(o, t) {
                self.batch.pose.push((o, ty, frames));
            }
            return;
        }
        if let Some(mut d) = self.host.delta_nodes.remove(&o) {
            d.on_message(val, p2, self);
            let t = d.anim.time;
            self.host.delta_nodes.insert(o, d);
            if let Some(frames) = self.host.pose_frames(o, t) {
                self.batch.pose.push((o, 0x101, frames));
            }
            return;
        }
        if let Some(mut l) = self.host.logic_nodes.remove(&o) {
            l.on_message(val, p2, self);
            self.host.logic_nodes.insert(o, l);
            return;
        }
        // Base `cControllerNode::OnMessage` (7: clear 0x800, 8: set 0x800) for visual-only controllers.
        if let Some(f) = self.flags.get_mut(o) {
            match p2 {
                7 => *f &= 0xffff_f7ff,
                8 => *f |= 0x800,
                _ => {}
            }
        }
    }

    /// `TriggerScript_Op0_SpawnController` 0x13c5d0 + `TriggerScript_ApplyControllerOp` 0x13ac88.
    fn spawn_op(&mut self, o: usize, op: &Op, armed: bool) {
        if o >= self.host.ctrl.len() {
            return;
        }
        let kind = op.args.first().copied().unwrap_or(0);
        if kind == 0 {
            // `cRollerNode` is owned by `Knockables`.
            return;
        }
        let old_ty = self.host.ctrl[o].or_else(|| self.rollers.contains_key(&o).then_some(0));
        let old = old_ty.map(|t| (t, t == 0 && self.rollers.get(&o).copied().unwrap_or(false)));
        let actions = controllers::spawn(old, kind, false);
        for act in actions {
            match act {
                Spawn::Restart => {
                    if old_ty == Some(0) {
                        self.batch.wake_rollers.push(o as u32);
                    }
                }
                Spawn::DeleteOld => self.kill_controller(o),
                Spawn::MeshAnimReplacing => {}
                Spawn::Create(5) => {
                    let mode = op.args.get(1).copied().unwrap_or(0);
                    let steps = controllers::apply(self.host.ctrl[o], mode);
                    for s in steps {
                        match s {
                            Apply::DeleteOld => self.kill_controller(o),
                            Apply::RestoreFlags => {
                                if let Some(f) = self.flags.get_mut(o) {
                                    *f = restore_flags(*f, 0xffff_0100);
                                }
                                self.host.events.script_events.push(ScriptEvent::RestoreObject(o));
                            }
                            Apply::FinishOld => self.finish_controller(o, 1),
                            Apply::Dead { stop } => {
                                let c = Controller::dead(o as u32, stop, self);
                                self.host.ctrl[o] = Some(5);
                                self.host.logic_nodes.insert(o, c);
                                self.host.pending.insert(o);
                                self.host.add_pending_node(true, List3Kind::Controller(o));
                                self.batch.ctrl.push((o, Some(5)));
                                self.host.events.script_events.push(ScriptEvent::DeadObject(o));
                            }
                            Apply::Restore => {
                                let c = Controller::restore(o as u32, self);
                                self.host.ctrl[o] = Some(0x3ee);
                                self.host.logic_nodes.insert(o, c);
                                self.host.pending.insert(o);
                                self.host.add_pending_node(false, List3Kind::Controller(o));
                                self.batch.ctrl.push((o, Some(0x3ee)));
                                self.host.events.script_events.push(ScriptEvent::DeadObject(o));
                            }
                        }
                    }
                }
                Spawn::Create(t @ (0x100 | 0x102 | 0x103)) => {
                    let model_end = self.host.model_end(o);
                    let a = AnimObject::new(o as u32, &op.args, model_end, armed, true, self);
                    let start_t = a.time;
                    self.host.ctrl[o] = Some(t);
                    self.host.chans.remove(&o);
                    self.host.anim_nodes.insert(o, a);
                    self.host.pending.insert(o);
                    self.host.add_pending_node(false, List3Kind::Controller(o));
                    if let Some(frames) = self.host.pose_frames(o, start_t) {
                        self.batch.pose.push((o, t, frames));
                    } else {
                        self.batch.ctrl.push((o, Some(t)));
                    }
                    self.host.events.script_events.push(ScriptEvent::SpawnController { object: o, args: op.args.clone() });
                }
                Spawn::Create(0x101) => {
                    let model_end = self.host.model_end(o);
                    let d = AnimDelta::new(o as u32, &op.args, model_end, armed, self);
                    let start_t = d.anim.time;
                    self.host.ctrl[o] = Some(0x101);
                    self.host.chans.remove(&o);
                    self.host.delta_nodes.insert(o, d);
                    self.host.pending.insert(o);
                    self.host.add_pending_node(false, List3Kind::Controller(o));
                    if let Some(frames) = self.host.pose_frames(o, start_t) {
                        self.batch.pose.push((o, 0x101, frames));
                    } else {
                        self.batch.ctrl.push((o, Some(0x101)));
                    }
                    self.host.events.script_events.push(ScriptEvent::SpawnController { object: o, args: op.args.clone() });
                }
                Spawn::Create(t @ (2 | 6 | 8)) => {
                    if let Some(c) = Controller::new(o as u32, &op.args, None) {
                        self.host.ctrl[o] = Some(t);
                        self.host.logic_nodes.insert(o, c);
                        self.host.pending.insert(o);
                        self.host.add_pending_node(false, List3Kind::Controller(o));
                        self.batch.ctrl.push((o, Some(t)));
                    }
                }
                Spawn::Create(t) => {
                    // Visual/effect controllers (UVScroll 10, TexFlip 11, Flag 13, Cracked 14, CrowdBox 17,
                    // UVScrollTexFlip 19, Boost 7, …): track type & flags (`& ~2 | 4`) in `CellHost` so
                    // `World_CellActivate_RunPersistant` / `World_CellDeactivate` / `op 3, 9, 10, 11` manage
                    // lifecycle while `WorldAnim` drives the visual effect from `ScriptEvent::SpawnController`.
                    if matches!(t, 10 | 11 | 13 | 14 | 17 | 19) {
                        if let Some(f) = self.flags.get_mut(o) {
                            *f = *f & 0xffff_fffd | 4;
                        }
                    }
                    self.host.ctrl[o] = Some(t);
                    self.batch.ctrl.push((o, Some(t)));
                    self.host.events.script_events.push(ScriptEvent::SpawnController { object: o, args: op.args.clone() });
                }
            }
        }
    }

    /// Construct a new `cScriptNode` (`0x13b8c0` / `0x13b978` / `0x13bc78`) and run `TriggerScript_Run` immediately;
    /// if it hits a wait, queue it on List 3's pending list with its `cBxSortObjNode` sort ID.
    fn spawn_thread(&mut self, mut t: Thread, vel: V4, human: bool) {
        if self.depth >= 12 {
            return;
        }
        // `cBxSortObjNode::cBxSortObjNode(3, 0x3e9)` reserves its sort_id BEFORE `TriggerScript_Run` runs.
        let sort_id = self.host.next_sort_id;
        self.host.next_sort_id = sort_id.wrapping_add(1);
        let saved = self.cur;
        self.cur = Some((t.script, t.object, t.ctx_a, t.ctx_b, vel, human));
        self.depth += 1;
        let done = script::run(&mut t, self);
        self.depth -= 1;
        self.cur = saved;
        if !done {
            let tid = self.host.next_thread_id;
            self.host.next_thread_id += 1;
            self.host.threads.insert(tid, ThreadState { thread: t, vel, human });
            self.host.pending_list3.insert(0, List3Entry { sort_id, is_dead: false, kind: List3Kind::Script(tid) });
        }
    }

    /// Step an existing live `cScriptNode` thread (`cScriptNode::Update` 0x13bd38). Returns true if it ended.
    fn step_live_thread(&mut self, tid: u64) -> bool {
        let Some(mut st) = self.host.threads.remove(&tid) else { return true };
        let saved = self.cur;
        self.cur = Some((st.thread.script, st.thread.object, st.thread.ctx_a, st.thread.ctx_b, st.vel, st.human));
        self.depth += 1;
        let done = script::run(&mut st.thread, self);
        self.depth -= 1;
        self.cur = saved;
        if !done {
            self.host.threads.insert(tid, st);
        }
        done
    }

    fn run_effect(&mut self, effect: i32, o: usize, rider: u32, vel: V4, human: bool) {
        if effect < 0 || o >= self.host.ctrl.len() {
            return;
        }
        self.host.events.activated_scripts.push((o, effect));
        let t = Thread::new(ScriptRef::Effect(effect as u32), o as u32 + 1, rider, None, None);
        self.spawn_thread(t, vel, human);
    }

    fn flush_starts(&mut self) {
        for _ in 0..8 {
            let starts = std::mem::take(&mut self.starts);
            if starts.is_empty() {
                break;
            }
            for (s, o) in starts {
                self.run_effect(s, o, 0, [0.0; 4], false);
            }
        }
    }
}

/// Lightweight `CtrlEnv` capturing `CtrlCall::Delete` and `CtrlCall::Start` during `Controller::update` / `finish`.
struct SubCtrlEnv<'a> {
    flags: &'a mut [u32],
    ssf: &'a Ssf,
    deleted: &'a mut bool,
    starts: &'a mut Vec<(i32, usize)>,
}

impl CtrlEnv for SubCtrlEnv<'_> {
    fn flags(&mut self, object: u32) -> &mut u32 {
        &mut self.flags[object as usize]
    }
    fn event_script(&self, object: u32, event: u32) -> i32 {
        self.ssf.event_script(object as usize, event as usize)
    }
    fn rider_in_box(&self, _: u32) -> Option<(i32, i32)> {
        None
    }
    fn call(&mut self, c: CtrlCall) {
        match c {
            CtrlCall::Delete => *self.deleted = true,
            CtrlCall::Start(s, o) => self.starts.push((s, o as usize)),
        }
    }
}

impl CtrlEnv for TickCtx<'_> {
    fn flags(&mut self, object: u32) -> &mut u32 {
        &mut self.flags[object as usize]
    }
    fn event_script(&self, object: u32, event: u32) -> i32 {
        self.ssf.event_script(object as usize, event as usize)
    }
    fn rider_in_box(&self, _: u32) -> Option<(i32, i32)> {
        None
    }
    fn call(&mut self, c: CtrlCall) {
        if let CtrlCall::Start(s, o) = c {
            self.starts.push((s, o as usize));
        }
    }
}

impl AnimEnv for TickCtx<'_> {
    fn rand_fx(&mut self) -> u32 {
        crate::rand::rand_fx()
    }
    fn contact_script(&mut self) {}
}

impl ObjAnimEnv for TickCtx<'_> {
    fn rand_game(&mut self) -> u32 {
        (self.rand_game)()
    }
    fn anim(&mut self, _: ObjAnimCall) {}
}

impl ScriptEnv for TickCtx<'_> {
    fn ops(&self, s: ScriptRef) -> &[Op] {
        match s {
            ScriptRef::Effect(e) => self.ssf.effects.get(e as usize).map_or(&[], |v| &v[..]),
            ScriptRef::Function(f) => self.ssf.functions.get(f as usize).map_or(&[], |v| &v.ops[..]),
        }
    }
    fn rand01(&mut self) -> f32 {
        let r = (self.rand_game)();
        f32::from_bits(r & 0x7f_ffff | 0x3f80_0000) - 1.0
    }
    fn rider_velocity(&self, _: u32) -> V4 {
        self.cur.map_or([0.0; 4], |c| c.4)
    }
    fn rider_x41c(&self, _: u32) -> i32 {
        self.cur.map_or(0, |c| c.5 as i32)
    }
    fn controller(&self, object: u32) -> u32 {
        match object.checked_sub(1) {
            Some(o) if self.host.ctrl.get(o as usize).copied().flatten().is_some() => 0x2000_0000 | o,
            _ => 0,
        }
    }
    fn instance(&self, index: u32) -> u32 {
        if (index as usize) < self.host.ctrl.len() { index + 1 } else { 0 }
    }
    fn camera(&self, _: u32) -> u32 {
        0
    }
    fn call(&mut self, c: ScriptCall) {
        match c {
            ScriptCall::SpawnController(at) => {
                let Some((s, object, ..)) = self.cur else { return };
                if object == 0 { return; }
                let mut off = 0u32;
                let op = self.ops(s).iter().find(|op| {
                    let here = off == at;
                    off += op.size;
                    here
                });
                if let Some(op) = op.cloned() {
                    self.spawn_op((object - 1) as usize, &op, false);
                }
            }
            ScriptCall::StartEffect { effect, object, rider, ctx_a, ctx_b } => {
                if object > 0 {
                    let (vel, human) = self.cur.map_or(([0.0; 4], false), |c| (c.4, c.5));
                    let o = (object - 1) as usize;
                    self.host.events.activated_scripts.push((o, effect as i32));
                    let t = Thread::new(ScriptRef::Effect(effect), object, rider, ctx_a, ctx_b);
                    self.spawn_thread(t, vel, human);
                }
            }
            ScriptCall::StartFunction { function, rider, ctx_a, ctx_b } => {
                let (vel, human) = if rider == 0 { ([0.0; 4], false) } else { self.cur.map_or(([0.0; 4], false), |c| (c.4, c.5)) };
                let t = Thread::new(ScriptRef::Function(function), 0, rider, ctx_a, ctx_b);
                self.spawn_thread(t, vel, human);
            }
            ScriptCall::ControllerMessage { controller, p2, p3 } => {
                if controller & 0xf000_0000 == 0x2000_0000 {
                    self.message_controller((controller & 0x0fff_ffff) as usize, p2, p3);
                }
            }
            ScriptCall::ControllerSwitch { controller, on } => {
                if controller & 0xf000_0000 == 0x2000_0000 {
                    self.finish_controller((controller & 0x0fff_ffff) as usize, on);
                }
            }
            ScriptCall::ControllerXbc(controller) => {
                if controller & 0xf000_0000 == 0x2000_0000 {
                    self.kill_controller((controller & 0x0fff_ffff) as usize);
                }
            }
            ScriptCall::SetRail(spline, on) => {
                self.batch.rails.push((spline, on != 0));
                self.host.events.script_events.push(ScriptEvent::SetRail { spline, on: on != 0 });
            }
            ScriptCall::ScriptSound { object, sound } => {
                if let Some(o) = object.checked_sub(1) {
                    self.host.events.script_events.push(ScriptEvent::ScriptSound { object: o as usize, sound });
                }
            }
            ScriptCall::ResetRider(rider) => {
                self.host.events.script_events.push(ScriptEvent::ResetRider(rider));
            }
            ScriptCall::Teleport { rider, object } => {
                if let Some(o) = object.checked_sub(1) {
                    self.host.events.script_events.push(ScriptEvent::Teleport { rider, object: o as usize });
                }
            }
            _ => {}
        }
    }
}

/// `CellEnv` bridge driving `AiCells::tick` (`World_CellActivate_RunPersistant` 0x25fed0 / `World_CellDeactivate` 0x260108).
struct AiCellBridge<'a, 'b> {
    ctx: &'a mut TickCtx<'b>,
}

impl CellEnv for AiCellBridge<'_, '_> {
    fn flags(&mut self, o: usize) -> &mut u32 {
        &mut self.ctx.flags[o]
    }
    fn controller(&self, o: usize) -> Option<u32> {
        self.ctx
            .host
            .ctrl
            .get(o)
            .copied()
            .flatten()
            .or_else(|| self.ctx.rollers.contains_key(&o).then_some(0))
    }
    fn event_script(&self, o: usize, e: u32) -> i32 {
        self.ctx.ssf.event_script(o, e as usize)
    }
    fn spawn(&mut self, s: i32, o: usize) {
        self.ctx.run_effect(s, o, 0, [0.0; 4], false);
    }
    fn ctrl_wants(&mut self, o: usize) -> bool {
        self.ctx.rollers.get(&o).copied().unwrap_or(false)
    }
    fn ctrl_7c(&mut self, o: usize) -> bool {
        self.ctx.rollers.get(&o).copied().unwrap_or(false)
    }
    fn ctrl_74(&mut self, o: usize) {
        self.ctx.batch.wake_rollers.push(o as u32);
    }
    fn ctrl_kill(&mut self, o: usize) {
        self.ctx.kill_controller(o);
    }
}

fn apply_batch(course: &CourseGround, flags: &[u32], batch: SyncBatch) {
    course.replace_flags(flags);
    for o in batch.unpose {
        course.unpose(o as u32);
    }
    for (o, ty) in batch.ctrl {
        course.set_controller(o as u32, ty);
    }
    for (o, ty, frames) in batch.pose {
        course.pose_with_controller(o as u32, ty, frames);
    }
    for (spline, on) in batch.rails {
        course.set_rail_live(spline, on);
    }
    if !batch.wake_rollers.is_empty() {
        let mut knock = course.knock.lock().unwrap_or_else(|e| e.into_inner());
        for o in batch.wake_rollers {
            if let Some(r) = knock.rollers.iter_mut().find(|r| r.object == o) {
                r.body.asleep = 0;
            }
        }
    }
}

impl CellHost {
    /// Run one 60 Hz frame of `cAIWorld::vf1` 0x115e28 (`AiCells::tick`), then step all non-pending controllers
    /// (`cAnimObjectNode` 0x199058, `cAnimDeltaNode` 0x199db8, `controllers::Controller`) and waiting script threads,
    /// then merge newly created pending nodes (`BxList_MergePending`) and sync flags & poses onto `course`.
    pub fn tick(
        &mut self,
        course: &CourseGround,
        rider_positions: &[V4],
        race_1c: i32,
        camera: V4,
        show_off: bool,
        rand_game: &mut dyn FnMut() -> u32,
    ) -> CellEvents {
        let flags = course.snapshot_flags();
        let rollers: HashMap<usize, bool> = {
            let k = course.knock.lock().unwrap_or_else(|e| e.into_inner());
            k.rollers.iter().map(|r| (r.object as usize, r.body.asleep != 0)).collect()
        };
        if self.rider_cells.len() != rider_positions.len() {
            self.rider_cells.resize(rider_positions.len(), -1);
        }
        let mut rc: Vec<RiderCell> = rider_positions
            .iter()
            .zip(self.rider_cells.iter().copied())
            .map(|(&pos, prev)| RiderCell { pos, cell: 0, prev })
            .collect();

        // Take `ai_cells` out so `AiCells::tick` borrows `&mut AiCells` independently of `TickCtx`.
        let mut ai_cells = std::mem::take(&mut self.ai_cells);
        let mut ctx = TickCtx {
            host: self,
            ssf: &course.ssf,
            flags,
            rollers,
            rand_game,
            batch: SyncBatch::default(),
            starts: Vec::new(),
            cur: None,
            depth: 0,
        };

        // List 2 (`cAIWorld::vf1` 0x115e28): read `World` grid while running cell activation/deactivation.
        course.with_world(|world: &World| {
            let mut bridge = AiCellBridge { ctx: &mut ctx };
            ai_cells.tick(world, &mut rc, race_1c, camera, show_off, &mut bridge);
        });
        ctx.host.ai_cells = ai_cells;
        for (slot, r) in ctx.host.rider_cells.iter_mut().zip(&rc) {
            *slot = r.cell;
        }
        ctx.flush_starts();

        // List 3 (`BxList_UpdateAll(0x337c60, 3)` 0x17d950): walk live nodes head-to-tail (descending `sort_id`,
        // newest first), taking the next node before each `vf1` call; controllers and waiting `cScriptNode` threads
        // step interleaved in exact List 3 order, while newly spawned nodes wait in `pending_list3` until merge.
        let snapshot: Vec<List3Entry> = ctx.host.live_list3.clone();
        for entry in snapshot {
            match entry.kind {
                List3Kind::Controller(o) => {
                    if let Some(mut a) = ctx.host.anim_nodes.remove(&o) {
                        let gone = a.update(&mut ctx);
                        if gone {
                            ctx.detach_controller(o);
                        } else {
                            let t = a.time;
                            let ty = ctx.host.ctrl[o].unwrap_or(0x100);
                            ctx.host.anim_nodes.insert(o, a);
                            if let Some(frames) = ctx.host.pose_frames(o, t) {
                                ctx.batch.pose.push((o, ty, frames));
                            }
                        }
                    } else if let Some(mut d) = ctx.host.delta_nodes.remove(&o) {
                        let gone = d.update(&mut ctx);
                        if gone {
                            ctx.detach_controller(o);
                        } else {
                            let t = d.anim.time;
                            ctx.host.delta_nodes.insert(o, d);
                            if let Some(frames) = ctx.host.pose_frames(o, t) {
                                ctx.batch.pose.push((o, 0x101, frames));
                            }
                        }
                    } else if let Some(mut l) = ctx.host.logic_nodes.remove(&o) {
                        let mut deleted = false;
                        let mut starts = Vec::new();
                        {
                            let mut sub = SubCtrlEnv { flags: &mut ctx.flags, ssf: ctx.ssf, deleted: &mut deleted, starts: &mut starts };
                            l.update(&mut sub);
                        }
                        if deleted {
                            ctx.detach_controller(o);
                        } else {
                            ctx.host.logic_nodes.insert(o, l);
                        }
                        ctx.starts.extend(starts);
                    }
                    ctx.flush_starts();
                }
                List3Kind::Script(tid) => {
                    if !ctx.host.threads.contains_key(&tid) {
                        continue;
                    }
                    if ctx.step_live_thread(tid) {
                        ctx.host.live_list3.retain(|e| e.kind != List3Kind::Script(tid));
                    }
                    ctx.flush_starts();
                }
            }
        }

        // `BxList_MergePending(0x337c60, 3)` 0x17d548 (`Game_UpdateObjectLists` 0x1814c0): merge pending List 3
        // nodes (new controllers + waiting `cScriptNode` threads) into `live_list3` sorted descending by `sort_id`.
        ctx.host.merge_pending();

        let TickCtx { flags, batch, .. } = ctx;
        apply_batch(course, &flags, batch);
        self.drain_events()
    }

    /// `World_RunScript` 0x13bad8: finds function `name` (`World_FindScript` 0x260dd8, case-insensitive), deletes
    /// any existing live `cScriptNode` running that function (`World_DeleteScriptObject` 0x13bbc0), constructs a new
    /// `cScriptNode` (`0x13b978`), runs `TriggerScript_Run` immediately, and queues it on `pending_list3` if it waits.
    pub fn run_script_named(
        &mut self,
        course: &CourseGround,
        name: &str,
        rand_game: &mut dyn FnMut() -> u32,
    ) -> CellEvents {
        let Some(fn_idx) = find_script(&course.ssf, name) else {
            return CellEvents::default();
        };
        self.delete_script_object(fn_idx);
        let flags = course.snapshot_flags();
        let mut ctx = TickCtx {
            host: self,
            ssf: &course.ssf,
            flags,
            rollers: HashMap::new(),
            rand_game,
            batch: SyncBatch::default(),
            starts: Vec::new(),
            cur: None,
            depth: 0,
        };
        let t = Thread::new(ScriptRef::Function(fn_idx), 0, 0, None, None);
        ctx.spawn_thread(t, [0.0; 4], false);
        ctx.flush_starts();
        let TickCtx { flags, batch, .. } = ctx;
        apply_batch(course, &flags, batch);
        self.drain_events()
    }

    /// Run an object's event-slot script (`Instance_GetEventScript` 0x25fe78: 0 persistent, 1 contact, 3 finish,
    /// 4 controller trigger goal) as a live `cScriptNode` (`0x13b8c0`) on `course.ssf`.
    pub fn run_event_script(
        &mut self,
        course: &CourseGround,
        object: usize,
        event: usize,
        rider: u32,
        vel: V4,
        human: bool,
        rand_game: &mut dyn FnMut() -> u32,
    ) -> CellEvents {
        let s = course.ssf.event_script(object, event);
        if s < 0 {
            return CellEvents::default();
        }
        let flags = course.snapshot_flags();
        let mut ctx = TickCtx {
            host: self,
            ssf: &course.ssf,
            flags,
            rollers: HashMap::new(),
            rand_game,
            batch: SyncBatch::default(),
            starts: Vec::new(),
            cur: None,
            depth: 0,
        };
        ctx.run_effect(s, object, rider, vel, human);
        ctx.flush_starts();
        let TickCtx { flags, batch, .. } = ctx;
        apply_batch(course, &flags, batch);
        self.drain_events()
    }

    /// Spawn a non-roller controller from a contact script (`Knockables::contact` -> `ThreadEnv::spawn`, e.g.
    /// Tokyo Megaplex `Pinball_FlipperL` 75's 300-tick `cAnimObjectNode` or trigger `59`/`41`'s `cDebounceNode`).
    pub fn spawn_from_contact(
        &mut self,
        course: &CourseGround,
        object: usize,
        op: &Op,
        rand_game: &mut dyn FnMut() -> u32,
    ) {
        let flags = course.snapshot_flags();
        let mut ctx = TickCtx {
            host: self,
            ssf: &course.ssf,
            flags,
            rollers: HashMap::new(),
            rand_game,
            batch: SyncBatch::default(),
            starts: Vec::new(),
            cur: None,
            depth: 0,
        };
        ctx.spawn_op(object, op, true);
        ctx.flush_starts();
        let TickCtx { flags, batch, .. } = ctx;
        apply_batch(course, &flags, batch);
    }

    /// Dispatch a controller message (`ops 3, 9`) from a contact script thread.
    pub fn message_from_script(
        &mut self,
        course: &CourseGround,
        object: usize,
        p2: u32,
        p3: u32,
        rand_game: &mut dyn FnMut() -> u32,
    ) {
        let flags = course.snapshot_flags();
        let mut ctx = TickCtx {
            host: self,
            ssf: &course.ssf,
            flags,
            rollers: HashMap::new(),
            rand_game,
            batch: SyncBatch::default(),
            starts: Vec::new(),
            cur: None,
            depth: 0,
        };
        ctx.message_controller(object, p2, p3);
        ctx.flush_starts();
        let TickCtx { flags, batch, .. } = ctx;
        apply_batch(course, &flags, batch);
    }

    /// Dispatch a controller finish (`op 10`) or kill (`op 11`) from a contact script thread.
    pub fn finish_or_kill_from_script(
        &mut self,
        course: &CourseGround,
        object: usize,
        finish_flag: Option<u32>,
        rand_game: &mut dyn FnMut() -> u32,
    ) {
        let flags = course.snapshot_flags();
        let mut ctx = TickCtx {
            host: self,
            ssf: &course.ssf,
            flags,
            rollers: HashMap::new(),
            rand_game,
            batch: SyncBatch::default(),
            starts: Vec::new(),
            cur: None,
            depth: 0,
        };
        match finish_flag {
            Some(flag) => ctx.finish_controller(object, flag),
            None => ctx.kill_controller(object),
        }
        ctx.flush_starts();
        let TickCtx { flags, batch, .. } = ctx;
        apply_batch(course, &flags, batch);
    }
}
