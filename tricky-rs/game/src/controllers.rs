//! The level scripts' logic controllers (board row E9e): what op 0 puts on an object and how each behaves, as
//! the game's classes do. A controller is a node on the object (object+0xe4; the node's +0x28 is the object, +0x14
//! its type). Its virtuals: Update (once a frame), Kill (+0xbc), Finish(flag) (+0xb4: flag 1 runs the object's
//! event-3 script, `Controller_RunFinishScript` 0x13a9f8), OnMessage(value, kind) (+0xc4, ops 3 and 9), and the
//! deleting destructor (+0xc, 3). Checked in the function runner (`tests/controllers.rs`).
//!
//! - `cDebounceNode` (2): counts down `(int)(p3 · 60)` frames, then finishes.
//! - `cCounterNode` (6): a count of hits (p3, a half) taken from messages; at 0 it runs event 4 and ends; it also
//!   gives up after `(int)(p4 · 60)` frames.
//! - `cTimerNode` (8): runs event 4 and ends after `(int)(p4 · 60)` frames (message kind 1 sets it again), and
//!   finishes after `(int)(p3 · 60)`.
//! - `cTrickTriggerNode` (21, needs a rider): while its rider is inside the object's box, waits for it to land with
//!   `p4` more trick points than when it started; then runs event 4. Leaving or landing short finishes it.
//! - Type 5 (`TriggerScript_ApplyControllerOp` 0x13ac88), by p3: 0 restore the object's flags, 1 finish the
//!   controller, 2 / 3 a `cDeadNode` (hidden, no collision; 3 also sets flag 0x800), 4 a `cRestoreNode` (hidden until
//!   it is killed, which restores the flags).
//!
//! The object's flags (+0xe8): the upper half holds the file's defaults (the `.ssf` physics record), the lower the
//! live state; restoring copies the upper half down (`m = flags & mask; m | (m >> 16) | 2`).

/// What a controller calls outside itself, in the game's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtrlCall {
    /// a new script thread on the object (`TriggerScript_Spawn` 0x13d538 or `cScriptNode` 0x13b8c0): effect, object
    Start(i32, u32),
    /// the controller's deleting destructor (vf 0xc, 3); the object loses its controller
    Delete,
}

/// The world as a controller sees it.
pub trait CtrlEnv {
    /// the object's flags word (+0xe8)
    fn flags(&mut self, object: u32) -> &mut u32;
    /// `Instance_GetEventScript` 0x25fe78(world, object, event)
    fn event_script(&self, object: u32, event: u32) -> i32;
    /// for a trick trigger: whether `rider` is among the objects overlapping the trigger's box, and its +0x424 and
    /// +0x5910 (trick points) if so
    fn rider_in_box(&self, rider: u32) -> Option<(i32, i32)>;
    fn call(&mut self, c: CtrlCall);
}

/// The controller kinds this row ports, with their fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// +0x2c frames left
    Debounce { ticks: i32 },
    /// +0x34 hits still needed (a half), +0x36 the message bits taken, +0x38 frames left
    Counter { count: i16, bits: u16, life: i32 },
    /// +0x34 frames to finish, +0x38 frames to run event 4
    Timer { finish_in: i32, life: i32 },
    /// +0x34 (p3), +0x38 points needed, +0x3c the rider's points at the start (+0x5910), +0x40 the rider's +0x460,
    /// +0x48 frames left (once the rider is gone), +0x4c the rider (0: gone)
    TrickTrigger { x34: i32, need: i32, start: i32, x40: u32, ticks: i32, rider: u32 },
    /// type 5 (`cDeadNode`, flag 0x800 when p3 was 3)
    Dead,
    /// type 0x3ee
    Restore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controller {
    pub kind: Kind,
    pub object: u32,
}

/// `(int)(v · 60)` as the constructors work it out (`cvt.w.s`).
fn frames(v: f32) -> i32 {
    let p = v * 60.0;
    if p.is_nan() {
        i32::MAX
    } else {
        p as i32
    }
}

/// The flags reset the game writes: the defaults in the upper half copied down, and 2.
pub fn restore_flags(flags: u32, mask: u32) -> u32 {
    let m = flags & mask;
    m | ((m as i32) >> 16) as u32 | 2
}

impl Controller {
    /// The type the node carries (+0x14).
    pub fn type_id(&self) -> u32 {
        match self.kind {
            Kind::Debounce { .. } => 2,
            Kind::Counter { .. } => 6,
            Kind::Timer { .. } => 8,
            Kind::TrickTrigger { .. } => 0x15,
            Kind::Dead => 5,
            Kind::Restore => 0x3ee,
        }
    }

    /// The constructors from op 0's words (`args[0]` the type, then p3, p4, p5 at op+0xc, +0x10, +0x14); for a trick
    /// trigger `rider` with its +0x460 and +0x5910.
    pub fn new(object: u32, args: &[u32], rider: Option<(u32, u32, i32)>) -> Option<Controller> {
        let a = |k: usize| args.get(k).copied().unwrap_or(0);
        let f = |k: usize| f32::from_bits(a(k));
        let kind = match a(0) {
            2 => Kind::Debounce { ticks: frames(f(1)) },
            6 => Kind::Counter { count: a(1) as u16 as i16, bits: 0, life: frames(f(2)) },
            8 => Kind::Timer { finish_in: frames(f(1)), life: frames(f(2)) },
            0x15 => {
                let (r, x460, points) = rider?;
                Kind::TrickTrigger { x34: a(1) as u16 as i16 as i32, need: a(2) as i32, start: points, x40: x460, ticks: frames(f(3)), rider: r }
            }
            _ => return None,
        };
        Some(Controller { kind, object })
    }

    /// `cDeadNode::cDeadNode` 0x13af38: hidden and without collision (flags & 0xffffff0d | 4), | 0x800 when `stop`.
    pub fn dead(object: u32, stop: bool, env: &mut impl CtrlEnv) -> Controller {
        let f = env.flags(object);
        let v = *f & 0xffff_ff0d;
        *f = v | 4 | if stop { 0x800 } else { 0 };
        Controller { kind: Kind::Dead, object }
    }

    /// `cRestoreNode::cRestoreNode` 0x13aff8: hidden the same way, until killed.
    pub fn restore(object: u32, env: &mut impl CtrlEnv) -> Controller {
        let f = env.flags(object);
        *f = *f & 0xffff_ff0d | 4;
        Controller { kind: Kind::Restore, object }
    }

    /// The deleting destructor: `~cDeadNode` 0x13afb8 puts the dead flags back first; all detach from the object.
    pub fn delete(&mut self, env: &mut impl CtrlEnv) {
        if self.kind == Kind::Dead {
            let f = env.flags(self.object);
            *f = *f & 0xffff_ff0d | 4;
        }
        env.call(CtrlCall::Delete);
    }

    /// `Controller_RunFinishScript` 0x13a9f8: the object's event-3 script, if any; whether it ran one.
    fn finish_script(&self, env: &mut impl CtrlEnv) -> bool {
        let s = env.event_script(self.object, 3);
        if s >= 0 {
            env.call(CtrlCall::Start(s, self.object));
        }
        s >= 0
    }

    /// Update (vtable +0x10).
    pub fn update(&mut self, env: &mut impl CtrlEnv) {
        match self.kind {
            Kind::Debounce { ticks } => {
                if ticks > 0 {
                    self.kind = Kind::Debounce { ticks: ticks - 1 };
                    if ticks - 1 == 0 {
                        self.finish(1, env);
                    }
                }
            }
            Kind::Counter { count, bits, life } => {
                if count < 1 {
                    let s = env.event_script(self.object, 4);
                    if s >= 0 {
                        env.call(CtrlCall::Start(s, self.object));
                    }
                    self.finish(0, env);
                } else if life > 0 {
                    self.kind = Kind::Counter { count, bits, life: life - 1 };
                    if life - 1 == 0 {
                        self.finish(1, env);
                    }
                }
            }
            Kind::Timer { life, .. } => {
                let life = life.wrapping_sub(1);
                if let Kind::Timer { finish_in, .. } = self.kind {
                    self.kind = Kind::Timer { finish_in, life };
                }
                if life < 1 {
                    let s = env.event_script(self.object, 4);
                    if s != -1 {
                        env.call(CtrlCall::Start(s, self.object));
                    }
                    self.finish(0, env);
                }
                // the game goes on with the node it may just have deleted
                if let Kind::Timer { finish_in, life } = self.kind {
                    if finish_in > 0 {
                        self.kind = Kind::Timer { finish_in: finish_in - 1, life };
                        if finish_in - 1 == 0 {
                            self.finish(1, env);
                        }
                    }
                }
            }
            Kind::TrickTrigger { x34, need, start, x40, ticks, rider } => {
                if rider == 0 {
                    if ticks > 0 {
                        self.kind = Kind::TrickTrigger { x34, need, start, x40, ticks: ticks - 1, rider };
                        if ticks - 1 == 0 {
                            self.finish(0, env);
                        }
                    }
                    return;
                }
                if let Some((x424, points)) = env.rider_in_box(rider) {
                    if x424 == 1 {
                        return;
                    }
                    if need <= points.wrapping_sub(start) {
                        let s = env.event_script(self.object, 4);
                        if s != -1 {
                            env.call(CtrlCall::Start(s, self.object));
                            self.kind = Kind::TrickTrigger { x34, need, start, x40, ticks, rider: 0 };
                            return;
                        }
                    }
                }
                self.finish(1, env);
            }
            Kind::Dead | Kind::Restore => {}
        }
    }

    /// Kill (vtable +0xb8).
    pub fn kill(&mut self, env: &mut impl CtrlEnv) {
        match self.kind {
            Kind::Debounce { ticks: n } | Kind::Timer { finish_in: n, .. } => {
                if n >= 0 {
                    self.delete(env);
                }
            }
            Kind::Counter { life, .. } => {
                if life >= 0 {
                    let f = env.flags(self.object);
                    *f = restore_flags(*f, 0xffff_0000);
                    self.delete(env);
                }
            }
            // `cRestoreNode::Kill` 0x13a928, also the trick trigger's
            Kind::TrickTrigger { .. } | Kind::Restore => {
                let f = env.flags(self.object);
                *f = restore_flags(*f, 0xffff_0000);
                self.delete(env);
            }
            Kind::Dead => {}
        }
    }

    /// Finish(flag) (vtable +0xb0).
    pub fn finish(&mut self, flag: u32, env: &mut impl CtrlEnv) {
        match self.kind {
            Kind::Debounce { ticks: n } | Kind::Timer { finish_in: n, .. } => {
                if (flag == 0 || !self.finish_script(env)) && n >= 0 {
                    self.delete(env);
                }
            }
            // `cCounterNode::Finish` and `cControllerNode::Finish` 0x13a980 (the trick trigger's) are the same
            Kind::Counter { .. } | Kind::TrickTrigger { .. } => {
                if flag == 0 || !self.finish_script(env) {
                    let f = env.flags(self.object);
                    *f = restore_flags(*f, 0xffff_0100);
                    self.delete(env);
                }
            }
            Kind::Dead | Kind::Restore => {}
        }
    }

    /// OnMessage(value, kind) (vtable +0xc0): ops 3 and 9 send (p3 as a float, p2).
    pub fn on_message(&mut self, value: f32, kind: u32, env: &mut impl CtrlEnv) {
        match self.kind {
            Kind::Counter { count, bits, life } => {
                let bit: u16 = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0].iter().position(|v| *v == value).map_or(0, |i| 1 << i);
                let mut hit = false;
                let mut nb = bits;
                match kind {
                    1 => {
                        if bits & bit == 0 {
                            nb = bits | bit;
                            hit = true;
                        }
                    }
                    2 => {
                        if bits & bit == 0 {
                            if value == 1.0 {
                                nb = bits | 1;
                                hit = true;
                            } else if bits & ((bit as i16) >> 1) as u16 != 0 {
                                nb = bits | bit;
                                hit = true;
                            }
                        }
                    }
                    3 => hit = true,
                    _ => base_message(self.object, kind, env),
                }
                let count = if hit && count > 0 { count - 1 } else { count };
                self.kind = Kind::Counter { count, bits: nb, life };
            }
            Kind::Timer { finish_in, .. } if kind == 1 => self.kind = Kind::Timer { finish_in, life: frames(value) },
            Kind::Debounce { .. } | Kind::Timer { .. } | Kind::TrickTrigger { .. } => base_message(self.object, kind, env),
            Kind::Dead | Kind::Restore => {}
        }
    }
}

/// `cControllerNode::OnMessage` 0x13aa48: kind 7 clears the object's flag 0x800, kind 8 sets it.
fn base_message(object: u32, kind: u32, env: &mut impl CtrlEnv) {
    let f = env.flags(object);
    match kind {
        7 => *f &= 0xffff_f7ff,
        8 => *f |= 0x800,
        _ => {}
    }
}

/// What op 0 does (`TriggerScript_Op0_SpawnController` 0x13c5d0), in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spawn {
    /// the running controller's vf 0x74 (restart)
    Restart,
    /// delete the running controller
    DeleteOld,
    /// a `cMeshAnimNode` that takes over the old controller (type 20 replacing one), then the old is deleted
    MeshAnimReplacing,
    /// construct a controller of this type (5: `TriggerScript_ApplyControllerOp`)
    Create(u32),
}

/// `TriggerScript_Op0_SpawnController` 0x13c5d0: `old` is the object's controller (its type, and its vf 0x64 —
/// 0 for every class of this row), `new` the op's type, `rider` whether the thread has one.
pub fn spawn(old: Option<(u32, bool)>, new: u32, rider: bool) -> Vec<Spawn> {
    let create = |t: u32| -> Vec<Spawn> {
        match t {
            0 | 2 | 5..=0x14 | 0x16..=0x18 | 0x100..=0x103 => vec![Spawn::Create(t)],
            0x15 if rider => vec![Spawn::Create(t)],
            _ => vec![],
        }
    };
    let Some((ty, running)) = old else { return create(new) };
    if ty == 5 {
        return vec![];
    }
    if ty == new {
        return if running { vec![Spawn::Restart] } else { vec![] };
    }
    if new == 0x14 {
        return vec![Spawn::MeshAnimReplacing, Spawn::DeleteOld];
    }
    let mut v = vec![Spawn::DeleteOld];
    v.extend(create(new));
    v
}

/// What type 5 does (`TriggerScript_ApplyControllerOp` 0x13ac88) for mode p3, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Apply {
    DeleteOld,
    /// the object's flags restored (mask 0xffff0100)
    RestoreFlags,
    /// the controller's Finish(1)
    FinishOld,
    Dead { stop: bool },
    Restore,
}

/// `TriggerScript_ApplyControllerOp` 0x13ac88: `old` the object's controller type.
pub fn apply(old: Option<u32>, mode: u32) -> Vec<Apply> {
    let guard = |extra: u32| -> Option<Vec<Apply>> {
        match old {
            Some(t) if t == 5 || t == extra => None,
            Some(_) => Some(vec![Apply::DeleteOld]),
            None => Some(vec![]),
        }
    };
    match mode {
        0 => guard(5).map(|mut v| {
            v.push(Apply::RestoreFlags);
            v
        }),
        1 => Some(if old.is_some() { vec![Apply::FinishOld] } else { vec![] }),
        2 | 3 => guard(5).map(|mut v| {
            v.push(Apply::Dead { stop: mode == 3 });
            v
        }),
        4 => guard(0x3ee).map(|mut v| {
            v.push(Apply::Restore);
            v
        }),
        _ => Some(vec![]),
    }
    .unwrap_or_default()
}

/// `cRailNode` (type 9, row E9h): switches a rail (a spline segment's flags word +0x18, bit 0) on or off for a
/// time. Constructor 0x1400d8: +0x34 the segment (p3, a half; −1 none), +0x38 `(int)(p5 · 60)` frames, +0x3c on
/// (p4 ≠ 0); the object's flags `& 0xffffffdd | 0x44`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailNode {
    pub object: u32,
    pub segment: i32,
    pub ticks: i32,
    pub on: u32,
}

/// What a rail node needs: the object's flags and the segment's flags word.
pub trait RailEnv: CtrlEnv {
    fn segment_flags(&mut self, segment: i32) -> &mut u32;
}

impl RailNode {
    pub fn new(object: u32, args: &[u32], env: &mut impl RailEnv) -> RailNode {
        let a = |k: usize| args.get(k).copied().unwrap_or(0);
        let n = RailNode { object, segment: a(1) as u16 as i16 as i32, ticks: frames(f32::from_bits(a(3))), on: (a(2) != 0) as u32 };
        let f = env.flags(object);
        *f = *f & 0xffff_ffdd | 0x44;
        n.set(n.on != 0, env);
        n
    }

    /// `RailNode_Enable` 0x1403c0 / `RailNode_Disable` 0x140400.
    fn set(&self, on: bool, env: &mut impl RailEnv) {
        if self.segment != -1 {
            let f = env.segment_flags(self.segment);
            *f = if on { *f | 1 } else { *f & !1 };
        }
    }

    /// Update 0x1402b0: after the frames, Finish(1).
    pub fn update(&mut self, env: &mut impl RailEnv) {
        if self.ticks > 0 {
            self.ticks -= 1;
            if self.ticks == 0 {
                self.finish(1, env);
            }
        }
    }

    /// Kill 0x1402f8.
    pub fn kill(&mut self, env: &mut impl RailEnv) {
        if self.ticks >= 0 {
            self.delete(env);
        }
    }

    /// Finish 0x140338.
    pub fn finish(&mut self, flag: u32, env: &mut impl RailEnv) {
        let ran = flag != 0 && {
            let s = env.event_script(self.object, 3);
            if s >= 0 {
                env.call(CtrlCall::Start(s, self.object));
            }
            s >= 0
        };
        if !ran && self.ticks >= 0 {
            self.delete(env);
        }
    }

    /// OnMessage 0x140398: kind 0 off, 1 on, others the base's.
    pub fn on_message(&mut self, kind: u32, env: &mut impl RailEnv) {
        match kind {
            0 => self.set(false, env),
            1 => self.set(true, env),
            _ => base_message(self.object, kind, env),
        }
    }

    /// `~cRailNode` 0x140218: the object's flags restored (mask 0xffff0100), the segment's defaults (upper half)
    /// copied into its lower half; then it detaches.
    pub fn delete(&mut self, env: &mut impl RailEnv) {
        let f = env.flags(self.object);
        *f = restore_flags(*f, 0xffff_0100);
        if self.segment != -1 {
            let s = env.segment_flags(self.segment);
            *s = *s & 0xffff_0000 | *s >> 16;
        }
        env.call(CtrlCall::Delete);
    }
}

/// The controller layer of `cRollerNode` (type 0, the knockable objects; row E9h): when it sleeps and the forces it
/// hands to the rigid body each frame. The body itself (`RigidBody_Integrate` 0x153c50, the transform, the sphere
/// tree, `RollerNode_Collide`) is `roller::RollerBody` (row E8c2). Fields: the rigid body at +0x30 (+0x38 inverse mass, +0x3c the 3×3
/// inverse inertia, +0x80 momentum, +0x90 angular momentum, +0xa0 velocity, +0xb0 angular velocity), +0xc8 the
/// energy average, +0xe4 seconds awake, +0xe8 a knock that decays, +0x140 asleep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roller {
    pub object: u32,
    pub inv_mass: f32,
    pub inv_inertia: [f32; 9],
    pub momentum: [f32; 4],
    pub ang_momentum: [f32; 4],
    pub vel: [f32; 4],
    pub ang_vel: [f32; 4],
    pub energy: f32,
    pub awake: f32,
    pub knock: f32,
    pub asleep: u32,
}

impl Roller {
    /// `cRollerNode::Update` 0x13e8a8 up to the integration: `None` while asleep, or when it goes to sleep this frame
    /// (momenta and velocities zeroed, Finish(1)); else the force (gravity −980·mass on z, less the momentum × a
    /// damping that grows with the knock and the time awake) and the torque (the angular momentum × the same).
    pub fn update(&mut self, env: &mut impl CtrlEnv) -> Option<([f32; 4], [f32; 4])> {
        if self.asleep != 0 {
            return None;
        }
        let dot = |a: [f32; 4], b: [f32; 4]| {
            let m = [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]];
            ((m[0] + m[1]) + 1.0 * m[2]) + 1.0 * m[3]
        };
        let e = dot(self.vel, self.momentum) + dot(self.ang_vel, self.ang_momentum);
        let mass = if self.inv_mass == 0.0 { -1.0 } else { 1.0 / self.inv_mass };
        self.awake += f32::from_bits(0x3c88_8889);
        self.energy = self.energy * f32::from_bits(0x3f73_3333) + e * f32::from_bits(0x3d4c_cccd) / mass;
        if self.energy < 10000.0 || 10.0 <= self.awake {
            self.awake = 0.0;
            self.momentum = [0.0; 4];
            self.vel = [0.0; 4].map(|v: f32| v * self.inv_mass);
            self.ang_momentum = [0.0; 4];
            let l = self.ang_momentum;
            let i = self.inv_inertia;
            self.ang_vel = [
                i[0] * l[0] + i[3] * l[1] + i[6] * l[2],
                i[1] * l[0] + i[4] * l[1] + i[7] * l[2],
                i[2] * l[0] + i[5] * l[1] + i[8] * l[2],
                0.0,
            ];
            self.finish(1, env);
            return None;
        }
        self.knock *= f32::from_bits(0x3f55_5555);
        let t = self.knock * 10.0;
        let s = if t < 0.0 {
            0.0
        } else if 2.0 < t {
            2.0
        } else {
            t
        };
        let k = (self.awake + self.awake) * s + s;
        // the FPU's division by 0 gives the largest float, signed
        let g = f32::from_bits(0xc475_0000);
        let gz = if self.inv_mass == 0.0 { if g.is_sign_negative() != self.inv_mass.is_sign_negative() { -f32::MAX } else { f32::MAX } } else { g / self.inv_mass };
        let grav = [0.0, 0.0, gz, 0.0];
        let force = [0, 1, 2, 3].map(|j| grav[j] - self.momentum[j] * k);
        let torque = [0, 1, 2, 3].map(|j| 0.0 - self.ang_momentum[j] * k);
        Some((force, torque))
    }

    /// Finish 0x13e860: flag 1 tries the object's event-3 script; otherwise (or without one) it goes to sleep
    /// (+0x140 = 1). It is never deleted by finishing; Kill deletes it.
    pub fn finish(&mut self, flag: u32, env: &mut impl CtrlEnv) {
        if flag != 0 {
            let s = env.event_script(self.object, 3);
            if s >= 0 {
                env.call(CtrlCall::Start(s, self.object));
                return;
            }
        }
        self.asleep = 1;
    }
}

// ---- Base world node / controller / counter / script virtuals & dtors (row E9l, on engine memory) ----

use crate::eamem;
use crate::pf::Mem;

pub const BX_OBJ_NODE_DTOR: u32 = 0x17cfe0;
pub const INSTANCE_DRAW_VISIBLE_CELLS: u32 = 0x200390;
pub const G_WORLD_GRID: u32 = 0x346488;

pub const VT_COUNTER_NODE: u32 = 0x36b0e8;
pub const VT_RESTORE_NODE: u32 = 0x36b1c8;
pub const VT_CONTROLLER_NODE: u32 = 0x36b388;
pub const VT_SCRIPT_NODE: u32 = 0x36bc20;
pub const VT_BX_SORT_OBJ_NODE: u32 = 0x386460;
pub const VT_BX_TYPE_OBJ_NODE: u32 = 0x3864c0;
pub const VT_MODELS_OBJ_NODE: u32 = 0x390c48;
pub const VT_TERRAIN_OBJ_NODE: u32 = 0x390c98;

/// Calls out for world node virtuals and destructors on engine memory.
pub trait NodeEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32;
}

/// `Instance_DetachController` 0x13aab0(this, inst): if `inst != 0 && *(inst+0xe4) == this`, clear `*(inst+0xe4)`.
pub fn instance_detach_controller(m: &mut Mem, this: u32, inst: u32) {
    if inst != 0 && m.r32(inst + 0xe4) == this {
        m.w32(inst + 0xe4, 0);
    }
}

/// `cBxTypeObjNode::~cBxTypeObjNode` 0x17ddc0(this, flags): restore vtable `0x3864c0` and tail-call
/// `cBxObjNode::~cBxObjNode`.
pub fn bx_type_obj_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_BX_TYPE_OBJ_NODE);
    env.call(m, BX_OBJ_NODE_DTOR, &[this, flags]);
}

/// `cBxSortObjNode::~cBxSortObjNode` 0x17dd00(this, flags): restore vtable `0x386460` and tail-call
/// `cBxTypeObjNode::~cBxTypeObjNode`.
pub fn bx_sort_obj_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_BX_SORT_OBJ_NODE);
    bx_type_obj_node_dtor(m, env, this, flags);
}

/// `cControllerNode::~cControllerNode` 0x13a8e0(this, flags): restore vtable `0x36b388`, detach from `*(this+0x28)`,
/// and tail-call `cBxSortObjNode::~cBxSortObjNode`.
pub fn controller_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_CONTROLLER_NODE);
    let inst = m.r32(this + 0x28);
    instance_detach_controller(m, this, inst);
    bx_sort_obj_node_dtor(m, env, this, flags);
}

/// `cRestoreNode::~cRestoreNode` 0x13b060(this, flags): restore vtable `0x36b1c8` and tail-call
/// `cControllerNode::~cControllerNode`.
pub fn restore_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_RESTORE_NODE);
    controller_node_dtor(m, env, this, flags);
}

/// `cCounterNode::~cCounterNode` 0x13b5f8(this, flags): restore vtable `0x36b0e8` and tail-call
/// `cControllerNode::~cControllerNode`.
pub fn counter_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_COUNTER_NODE);
    controller_node_dtor(m, env, this, flags);
}

/// `cCounterNode::Kill` 0x13b218(this): when `*(this+0x38) >= 0`, restore `*(this+0x28)->+0xe8` flags via
/// `restore_flags(..., 0xffff_0000)` and invoke deleting destructor virtual `+8/+0xc(3)`.
pub fn counter_node_kill(m: &mut Mem, env: &mut dyn NodeEnv, this: u32) {
    if (m.r32(this + 0x38) as i32) < 0 {
        return;
    }
    let inst = m.r32(this + 0x28);
    let fl = restore_flags(m.r32(inst + 0xe8), 0xffff_0000);
    m.w32(inst + 0xe8, fl);
    if this != 0 {
        let vt = m.r32(this + 8);
        let adj = m.r16(vt + 8) as i16 as i32 as u32;
        let f = m.r32(vt + 0xc);
        env.call(m, f, &[this.wrapping_add(adj), 3]);
    }
}

/// `cScriptNode::~cScriptNode` 0x13bd20(this, flags): restore vtable `0x36bc20` and tail-call
/// `cBxSortObjNode::~cBxSortObjNode`.
pub fn script_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_SCRIPT_NODE);
    bx_sort_obj_node_dtor(m, env, this, flags);
}

/// `TriggerScript_SetContext` 0x13ba30(this, a1, mat1, mat2): set `+0xe8 = a1`, clear `+0xd0`/`+0xd4`, copy or
/// zero 64-byte matrices at `+0x50` and `+0x90`, and set `+0xd0`/`+0xd4 = 1` when each source pointer is non-null.
pub fn trigger_script_set_context(m: &mut Mem, this: u32, a1: u32, mat1: u32, mat2: u32) {
    m.w32(this + 0xe8, a1);
    m.w32(this + 0xd0, 0);
    m.w32(this + 0xd4, 0);
    if mat1 != 0 {
        eamem::copy(m, this + 0x50, mat1, 0x40);
        m.w32(this + 0xd0, 1);
    } else {
        eamem::set(m, this + 0x50, 0, 0x40);
    }
    if mat2 != 0 {
        eamem::copy(m, this + 0x90, mat2, 0x40);
        m.w32(this + 0xd4, 1);
    } else {
        eamem::set(m, this + 0x90, 0, 0x40);
    }
}

/// `cCounterNode::vf18_13b770` 0x13b770(this): forward to virtual `+0x70/+0x74` on `*(this+8)`.
pub fn counter_node_vf18(m: &mut Mem, env: &mut dyn NodeEnv, this: u32) -> u32 {
    let vt = m.r32(this + 8);
    let adj = m.r16(vt + 0x70) as i16 as i32 as u32;
    let f = m.r32(vt + 0x74);
    env.call(m, f, &[this.wrapping_add(adj)])
}

/// `cCounterNode::vf10_13b7a8` 0x13b7a8(this): return `*(this+0x28)`.
pub fn counter_node_vf10(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x28)
}

/// `cCounterNode::vf6_17dc60` 0x17dc60(this): return `*(this+0x24)`.
pub fn counter_node_vf6(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x24)
}

/// `cCounterNode::vf5_17dc68` 0x17dc68(this): return `*(this+0x20)`.
pub fn counter_node_vf5(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x20)
}

/// `cCounterNode::vf4_17dc70` 0x17dc70(this, other): return 1 if `other->vf5() == *(this+0x20)` (via `other`'s
/// vtable `+0x30/+0x34`), else 0.
pub fn counter_node_vf4(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, other: u32) -> u32 {
    let vt = m.r32(other + 8);
    let adj = m.r16(vt + 0x30) as i16 as i32 as u32;
    let f = m.r32(vt + 0x34);
    let v = env.call(m, f, &[other.wrapping_add(adj)]);
    u32::from(v == m.r32(this + 0x20))
}

/// `cCounterNode::vf3_17dcb0` 0x17dcb0(this, other): return 1 if `other->vf5() >= *(this+0x20)` (unsigned), else 0.
pub fn counter_node_vf3(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, other: u32) -> u32 {
    let vt = m.r32(other + 8);
    let adj = m.r16(vt + 0x30) as i16 as i32 as u32;
    let f = m.r32(vt + 0x34);
    let v = env.call(m, f, &[other.wrapping_add(adj)]);
    u32::from(v >= m.r32(this + 0x20))
}

/// `cModelsObjNode::Draw` 0x1c78f0(): tail-call `Instance_DrawVisibleCells(0x346488)`.
pub fn models_obj_node_draw(m: &mut Mem, env: &mut dyn NodeEnv) -> u32 {
    env.call(m, INSTANCE_DRAW_VISIBLE_CELLS, &[G_WORLD_GRID])
}

/// `cModelsObjNode::~cModelsObjNode` 0x1c7d50(this, flags): restore vtable `0x390c48` and tail-call
/// `cBxObjNode::~cBxObjNode`.
pub fn models_obj_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_MODELS_OBJ_NODE);
    env.call(m, BX_OBJ_NODE_DTOR, &[this, flags]);
}

/// `cTerrainObjNode::~cTerrainObjNode` 0x1c7df0(this, flags): restore vtable `0x390c98` and tail-call
/// `cBxObjNode::~cBxObjNode`.
pub fn terrain_obj_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_TERRAIN_OBJ_NODE);
    env.call(m, BX_OBJ_NODE_DTOR, &[this, flags]);
}

/// Empty no-op virtual stubs:
/// `cRestoreNode::OnMessage` 0x13b670, `cRestoreNode::Finish` 0x13b678,
/// `cDeadNode::OnMessage` 0x13b6d8, `cDeadNode::Kill` 0x13b6e0, `cDeadNode::Finish` 0x13b6e8,
/// `cCounterNode::vf25_13b750` 0x13b750, `cCounterNode::vf24_13b758` 0x13b758,
/// `cCounterNode::vf20_13b760` 0x13b760, `cCounterNode::vf19_13b768` 0x13b768,
/// `cCounterNode::vf17_13b798` 0x13b798, `cCounterNode::vf16_13b7a0` 0x13b7a0,
/// `cCounterNode::vf13_13b7c0` 0x13b7c0, `cCounterNode::vf12_13b7c8` 0x13b7c8,
/// `cCounterNode::vf2_17dcf0` 0x17dcf0, `cRestoreNode::vf1_17dcf8` 0x17dcf8,
/// `cBxTypeObjNode::Draw` 0x17ddb0, `cBxTypeObjNode::Update` 0x17ddb8,
/// `cCounterNode::vf8_17ded0` 0x17ded0, `cBxObjNode::Update` 0x17df08.
pub fn node_noop() {}

/// Zero-returning virtual stubs:
/// `cCounterNode::vf26_13b748` 0x13b748, `cCounterNode::vf15_13b7b0` 0x13b7b0,
/// `cCounterNode::vf11_13b7d0` 0x13b7d0, `cScriptNode::vf10_17dd98` 0x17dd98.
pub fn node_ret_zero() -> u32 {
    0
}

/// One-returning virtual stub: `cCounterNode::vf14_13b7b8` 0x13b7b8.
pub fn node_ret_one() -> u32 {
    1
}
