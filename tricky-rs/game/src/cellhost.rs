//! Live world-cell and controller host (board row E7b2): drives `AiCells::tick` (`cAIWorld::vf1` 0x115e28) each
//! 60 Hz frame on a course's `.pbd`, `.ltg`, and `.ssf` (`CourseGround`), spawning and stepping `cAnimObjectNode`
//! (0x100, `objanim::AnimObject` 0x198810 / 0x199058), `cAnimDeltaNode` (0x101, `objanim::AnimDelta` 0x199d10 /
//! 0x199db8), and logic controllers (`cDebounceNode` 2, `cDeadNode` 5, `cCounterNode` 6, `cTimerNode` 8,
//! `cRestoreNode` 0x3ee, `controllers::Controller`) via `TriggerScript_Op0_SpawnController` 0x13c5d0 /
//! `TriggerScript_ApplyControllerOp` 0x13ac88, and posing animated collision parts on `CourseGround` through
//! `meshanim::mesh_frame` (`MeshAnim_ApplyFrames` 0x199258 / `MeshAnimNode_GetFrameMatrix` 0x1998d0).

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

/// Events produced by one 60 Hz [`CellHost::tick`] for the app's visual animation layer (`WorldAnim`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellEvents {
    /// `(object, effect_id)` scripts started by cell activation, deactivation, or controller finish.
    pub activated_scripts: Vec<(usize, i32)>,
    /// Objects whose controller was killed or finished this tick (so visual controllers on them stop).
    pub killed_objects: Vec<usize>,
}

/// Live cell activation, script controller state, and keyframed animation clocks for one course (`CourseGround`).
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
    /// Nodes spawned this frame (`BxList_MergePending`): posed at start time for this frame, stepped from next frame.
    pending: HashSet<usize>,
    chans: HashMap<usize, Vec<[f32; 16]>>,
    threads: Vec<Thread>,
    events: CellEvents,
}

impl CellHost {
    /// Build a fresh cell/controller host from a course's parsed `.pbd` and instance count.
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
            pending: HashSet::new(),
            chans: HashMap::new(),
            threads: Vec::new(),
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

    /// Drain queued visual events since the last call.
    pub fn drain_events(&mut self) -> CellEvents {
        std::mem::take(&mut self.events)
    }

    /// `AIWorld_ClearSlots` 0x115df0 + `World_ResetInstances` 0x115fe8: clear all live cell slots, controllers,
    /// animation clocks, threads, and pending poses. Caller also resets `CourseGround` flags and poses.
    pub fn clear(&mut self) {
        self.ai_cells = AiCells::default();
        self.rider_cells.clear();
        self.ctrl.fill(None);
        self.anim_nodes.clear();
        self.delta_nodes.clear();
        self.logic_nodes.clear();
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
    /// Deferred script thread execution state.
    waiting: Vec<Thread>,
    cur: Option<(ScriptRef, u32, Option<script::Context>, Option<script::Context>)>,
    depth: u32,
}

impl<'a> TickCtx<'a> {
    fn detach_controller(&mut self, o: usize) {
        self.host.ctrl[o] = None;
        self.host.anim_nodes.remove(&o);
        self.host.delta_nodes.remove(&o);
        self.host.logic_nodes.remove(&o);
        self.host.pending.remove(&o);
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
            l.kill(self);
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
                            }
                            Apply::FinishOld => self.finish_controller(o, 1),
                            Apply::Dead { stop } => {
                                let c = Controller::dead(o as u32, stop, self);
                                self.host.ctrl[o] = Some(5);
                                self.host.logic_nodes.insert(o, c);
                                self.batch.ctrl.push((o, Some(5)));
                            }
                            Apply::Restore => {
                                let c = Controller::restore(o as u32, self);
                                self.host.ctrl[o] = Some(0x3ee);
                                self.host.logic_nodes.insert(o, c);
                                self.batch.ctrl.push((o, Some(0x3ee)));
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
                    if let Some(frames) = self.host.pose_frames(o, start_t) {
                        self.batch.pose.push((o, t, frames));
                    } else {
                        self.batch.ctrl.push((o, Some(t)));
                    }
                }
                Spawn::Create(0x101) => {
                    let model_end = self.host.model_end(o);
                    let d = AnimDelta::new(o as u32, &op.args, model_end, armed, self);
                    let start_t = d.anim.time;
                    self.host.ctrl[o] = Some(0x101);
                    self.host.chans.remove(&o);
                    self.host.delta_nodes.insert(o, d);
                    self.host.pending.insert(o);
                    if let Some(frames) = self.host.pose_frames(o, start_t) {
                        self.batch.pose.push((o, 0x101, frames));
                    } else {
                        self.batch.ctrl.push((o, Some(0x101)));
                    }
                }
                Spawn::Create(t @ (2 | 6 | 8)) => {
                    if let Some(c) = Controller::new(o as u32, &op.args, None) {
                        self.host.ctrl[o] = Some(t);
                        self.host.logic_nodes.insert(o, c);
                        self.host.pending.insert(o);
                        self.batch.ctrl.push((o, Some(t)));
                    }
                }
                Spawn::Create(t) => {
                    // Other controller types (UVScroll 10, TexFlip 11, Flag 13, Cracked 14, CrowdBox 17, Boost 7, …):
                    // track type in `ctrl` so `World_CellActivate_RunPersistant` / `World_CellDeactivate` manage
                    // lifecycle while `WorldAnim` runs the visual effect from `activated_scripts` / `killed_objects`.
                    self.host.ctrl[o] = Some(t);
                    self.batch.ctrl.push((o, Some(t)));
                }
            }
        }
    }

    fn run_thread(&mut self, mut t: Thread, armed: bool) {
        if self.depth >= 12 {
            return;
        }
        let saved = self.cur;
        self.cur = Some((t.script, t.object, t.ctx_a, t.ctx_b));
        self.depth += 1;
        let done = script::run(&mut t, self);
        self.depth -= 1;
        self.cur = saved;
        let _ = armed;
        if !done {
            self.waiting.push(t);
        }
    }

    fn run_effect(&mut self, effect: i32, o: usize, armed: bool) {
        if effect < 0 || o >= self.host.ctrl.len() {
            return;
        }
        self.host.events.activated_scripts.push((o, effect));
        let t = Thread::new(ScriptRef::Effect(effect as u32), o as u32 + 1, 0, None, None);
        self.run_thread(t, armed);
    }

    fn flush_starts(&mut self) {
        for _ in 0..8 {
            let starts = std::mem::take(&mut self.starts);
            if starts.is_empty() {
                break;
            }
            for (s, o) in starts {
                self.run_effect(s, o, false);
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
        [0.0; 4]
    }
    fn rider_x41c(&self, _: u32) -> i32 {
        0
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
            ScriptCall::StartEffect { effect, object, .. } => {
                if object > 0 {
                    self.run_effect(effect as i32, (object - 1) as usize, false);
                }
            }
            ScriptCall::StartFunction { function, .. } => {
                let t = Thread::new(ScriptRef::Function(function), 0, 0, None, None);
                self.run_thread(t, false);
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
        self.ctx.run_effect(s, o, false);
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
            waiting: Vec::new(),
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

        // List 3: step all controllers created before this frame (`pending` waits until `BxList_MergePending`).
        let mut anim_ids: Vec<usize> = ctx.host.anim_nodes.keys().copied().collect();
        anim_ids.sort_unstable();
        for o in anim_ids {
            if ctx.host.pending.contains(&o) {
                continue;
            }
            let Some(mut a) = ctx.host.anim_nodes.remove(&o) else { continue };
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
        }

        let mut delta_ids: Vec<usize> = ctx.host.delta_nodes.keys().copied().collect();
        delta_ids.sort_unstable();
        for o in delta_ids {
            if ctx.host.pending.contains(&o) {
                continue;
            }
            let Some(mut d) = ctx.host.delta_nodes.remove(&o) else { continue };
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
        }

        let mut logic_ids: Vec<usize> = ctx.host.logic_nodes.keys().copied().collect();
        logic_ids.sort_unstable();
        for o in logic_ids {
            if ctx.host.pending.contains(&o) {
                continue;
            }
            let Some(mut l) = ctx.host.logic_nodes.remove(&o) else { continue };
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

        // Waiting trigger script threads (`cScriptNode::Update` 0x13bd38).
        let old_threads = std::mem::take(&mut ctx.host.threads);
        for t in old_threads {
            ctx.run_thread(t, false);
        }
        ctx.flush_starts();
        let waiting = std::mem::take(&mut ctx.waiting);
        ctx.host.threads.extend(waiting);

        // `BxList_MergePending`: nodes spawned this frame will step starting next frame.
        ctx.host.pending.clear();

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
            waiting: Vec::new(),
            cur: None,
            depth: 0,
        };
        ctx.spawn_op(object, op, true);
        ctx.flush_starts();
        let waiting = std::mem::take(&mut ctx.waiting);
        ctx.host.threads.extend(waiting);
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
            waiting: Vec::new(),
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
            waiting: Vec::new(),
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
