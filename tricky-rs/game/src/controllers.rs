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

// ---- Roller, debounce, timer, boost & node destructors (row E9m, on engine memory) ----

use crate::ps2f::F;

pub const MEM_DELETE: u32 = 0x114f80;
pub const CONTROLLER_NODE_CTOR: u32 = 0x13a840;
pub const CONTROLLER_RUN_FINISH_SCRIPT: u32 = 0x13a9f8;
pub const CONTROLLER_NODE_ON_MESSAGE: u32 = 0x13aa48;
pub const WORLD_NODE_RAY_CAST: u32 = 0x13aad0;
pub const SCRIPT_NODE_CTOR: u32 = 0x13b8c0;
pub const BOOST_NODE_CTOR: u32 = 0x140498;
pub const UV_SCROLL_NODE_DTOR: u32 = 0x142128;
pub const TEX_FLIP_DTOR: u32 = 0x1499e8;
pub const CONTACT_REMOVE_ALL_FOR_NODE: u32 = 0x14fb38;
pub const RIGID_BODY_CONTACT_IMPULSE: u32 = 0x153ee8;
pub const BX_SPHERE_TREE_DTOR: u32 = 0x2391d0;
pub const MEMPOOL_ALLOC: u32 = 0x23d3b0;
pub const INSTANCE_GET_EVENT_SCRIPT: u32 = 0x25fe78;
pub const WORLD_SPH_TREE_CTOR: u32 = 0x261638;
pub const WORLD_SPH_TREE_VF10: u32 = 0x261a48;

pub const G_APP: u32 = 0x337c58;
pub const G_SCRIPT_POOL: u32 = 0x345e48;
pub const NAME_SCRIPT_NODE: u32 = 0x36c390;

pub const VT_CROWD_BOX_NODE: u32 = 0x36ca18;
pub const VT_DEBOUNCE_NODE: u32 = 0x36caf8;
pub const VT_CRACKED_NODE: u32 = 0x36cbd8;
pub const VT_UV_SCROLL_TEX_FLIP_NODE: u32 = 0x36ce78;
pub const VT_TEX_FLIP_NODE: u32 = 0x36cf58;
pub const VT_Z_BOOST_NODE: u32 = 0x36d128;
pub const VT_TUBE_END_BOOST_NODE: u32 = 0x36d208;
pub const VT_RANDOM_BOOST_NODE: u32 = 0x36d300;
pub const VT_LAP_BOOST_NODE: u32 = 0x36d3f8;
pub const VT_BOOST_NODE: u32 = 0x36d4d8;
pub const VT_TIMER_NODE: u32 = 0x36d6b0;
pub const VT_ROLLER_NODE: u32 = 0x36d790;
pub const HUGE_1E10_BITS: u32 = 0x5015_02f9;

fn call_node_vf(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, off: u32, extra: &[u32]) -> u32 {
    let vt = m.r32(this + 8);
    let adj = m.r16(vt + off) as i16 as i32 as u32;
    let f = m.r32(vt + off + 4);
    let mut args = Vec::with_capacity(1 + extra.len());
    args.push(this.wrapping_add(adj));
    args.extend_from_slice(extra);
    env.call(m, f, &args)
}

fn contact_mgr(m: &Mem) -> u32 {
    let app = m.r32(G_APP);
    let game = m.r32(app + 0x730);
    m.r32(game + 0x30)
}

/// `cRollerNode::~cRollerNode` 0x13e0b8(this, flags): restore vtable `0x36d790`, remove contacts for `this+0x2c0`,
/// restore instance flags with mask `0xffff_0100`, destruct embedded `bxSphereTree` at `this+0x150` (flags 2), and
/// tail-call `cControllerNode::~cControllerNode`.
pub fn roller_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_ROLLER_NODE);
    let cm = contact_mgr(m);
    env.call(m, CONTACT_REMOVE_ALL_FOR_NODE, &[cm, this.wrapping_add(0x2c0)]);
    let inst = m.r32(this + 0x28);
    let fl = restore_flags(m.r32(inst + 0xe8), 0xffff_0100);
    m.w32(inst + 0xe8, fl);
    env.call(m, BX_SPHERE_TREE_DTOR, &[this.wrapping_add(0x150), 2]);
    controller_node_dtor(m, env, this, flags);
}

/// `RollerNode_HitNode` 0x13f330(this, other, sp): test `other` against `cWorldSphTree(this+0x150, 1)` via
/// `WorldNode_RayCast`; on hit (`> 0.0`), notify `other` via virtuals `+0x60`/`+0x98` if responsive (passing 4D
/// length of `this+0xa0`), apply `RigidBody_ContactImpulse(this+0x30, ..., 0.9, 0.0)`, and tear down the sphere tree.
pub fn roller_node_hit_node(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, other: u32, sp: u32) {
    let tree = sp.wrapping_add(0x40);
    env.call(m, WORLD_SPH_TREE_CTOR, &[tree, this.wrapping_add(0x150), 1]);
    let hit = f32::from_bits(env.call(m, WORLD_NODE_RAY_CAST, &[other, tree, sp]));
    if 0.0f32 < F(hit).ee() {
        if call_node_vf(m, env, other, 0x60, &[]) != 0 {
            let mut sq = [F(0.0); 4];
            for (k, s) in sq.iter_mut().enumerate() {
                let v = F(f32::from_bits(m.r32(this + 0xa0 + (k as u32) * 4)));
                *s = v * v;
            }
            let len = (((sq[0] + sq[1]) + sq[2]) + sq[3]).sqrt();
            for k in (0..16u32).step_by(4) {
                let v10 = m.r32(sp + 0x10 + k);
                let v20 = m.r32(sp + 0x20 + k);
                m.w32(sp + 0x1f0 + k, v10);
                m.w32(sp + 0x200 + k, v20);
                m.w32(sp + 0x210 + k, v20);
            }
            m.w32(sp + 0x220, len.0.to_bits());
            call_node_vf(m, env, other, 0x98, &[sp.wrapping_add(0x1f0), sp]);
        }
        env.call(
            m,
            RIGID_BODY_CONTACT_IMPULSE,
            &[
                this.wrapping_add(0x30),
                sp.wrapping_add(0x10),
                sp.wrapping_add(0x20),
                0x3f66_6666,
                0,
            ],
        );
    }
    env.call(m, WORLD_SPH_TREE_VF10, &[tree, 2]);
}

/// `cRollerNode::vf12_149e18` 0x149e18(this): set `*(this+0x140) = 1`.
pub fn roller_node_vf12(m: &mut Mem, this: u32) {
    m.w32(this + 0x140, 1);
}

/// `cRollerNode::Kill` 0x149e48(this): if `this != 0`, invoke deleting destructor virtual `+8/+0xc(3)`.
pub fn roller_node_kill(m: &mut Mem, env: &mut dyn NodeEnv, this: u32) {
    if this != 0 {
        call_node_vf(m, env, this, 8, &[3]);
    }
}

/// `cDebounceNode::cDebounceNode` 0x13fbf0(this, a1, inst, params): construct `cControllerNode(type 2)`, install
/// vtable `0x36caf8`, set `+0x2c = (int)(params->+0xc * 60.0)`, and return `this`.
pub fn debounce_node_ctor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, a1: u32, inst: u32, params: u32) -> u32 {
    env.call(m, CONTROLLER_NODE_CTOR, &[this, a1, 2, inst]);
    m.w32(this + 8, VT_DEBOUNCE_NODE);
    let secs = F(f32::from_bits(m.r32(params + 0xc)));
    let ticks = (secs * F(60.0)).ee() as i32;
    m.w32(this + 0x2c, ticks as u32);
    this
}

/// `cDebounceNode::Update` 0x13fca8(this): if `*(this+0x2c) > 0`, decrement it and call `Finish(1)` when it hits 0.
pub fn debounce_node_update(m: &mut Mem, env: &mut dyn NodeEnv, this: u32) {
    let t = m.r32(this + 0x2c) as i32;
    if t > 0 {
        let nt = t.wrapping_sub(1);
        m.w32(this + 0x2c, nt as u32);
        if nt == 0 {
            call_node_vf(m, env, this, 0xb0, &[1]);
        }
    }
}

/// `cDebounceNode::Kill` 0x13fcf0(this): if `*(this+0x2c) >= 0 && this != 0`, invoke deleting destructor `+8/+0xc(3)`.
pub fn debounce_node_kill(m: &mut Mem, env: &mut dyn NodeEnv, this: u32) {
    if (m.r32(this + 0x2c) as i32) >= 0 && this != 0 {
        call_node_vf(m, env, this, 8, &[3]);
    }
}

/// `cDebounceNode::Finish` 0x13fd30(this, flag): if `flag != 0` and `Controller_RunFinishScript` returns non-zero,
/// return; else if `*(this+0x2c) >= 0 && this != 0`, invoke deleting destructor `+8/+0xc(3)`.
pub fn debounce_node_finish(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flag: u32) {
    if flag != 0 && env.call(m, CONTROLLER_RUN_FINISH_SCRIPT, &[this]) != 0 {
        return;
    }
    if (m.r32(this + 0x2c) as i32) >= 0 && this != 0 {
        call_node_vf(m, env, this, 8, &[3]);
    }
}

/// `cDebounceNode::~cDebounceNode` 0x149688(this, flags): restore vtable `0x36caf8` and tail-call
/// `cControllerNode::~cControllerNode`.
pub fn debounce_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_DEBOUNCE_NODE);
    controller_node_dtor(m, env, this, flags);
}

/// `cTimerNode::cTimerNode` 0x13fdd0(this, a1, inst, params): construct `cControllerNode(type 8)`, install vtable
/// `0x36d6b0`, set `+0x34 = (int)(p3 * 60)` and `+0x38 = (int)(p4 * 60)`, and return `this`.
pub fn timer_node_ctor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, a1: u32, inst: u32, params: u32) -> u32 {
    env.call(m, CONTROLLER_NODE_CTOR, &[this, a1, 8, inst]);
    m.w32(this + 8, VT_TIMER_NODE);
    let p3 = F(f32::from_bits(m.r32(params + 0xc)));
    let p4 = F(f32::from_bits(m.r32(params + 0x10)));
    m.w32(this + 0x34, ((p3 * F(60.0)).ee() as i32) as u32);
    m.w32(this + 0x38, ((p4 * F(60.0)).ee() as i32) as u32);
    this
}

/// `cTimerNode::Update` 0x13fea0(this): decrement `+0x38` — when `<= 0`, spawn event-4 script if present and call
/// `Finish(0)`; then if `+0x34 > 0`, decrement it and call `Finish(1)` when it reaches 0.
pub fn timer_node_update(m: &mut Mem, env: &mut dyn NodeEnv, this: u32) {
    let life = (m.r32(this + 0x38) as i32).wrapping_sub(1);
    m.w32(this + 0x38, life as u32);
    if life <= 0 {
        let inst = m.r32(this + 0x28);
        let ev = env.call(m, INSTANCE_GET_EVENT_SCRIPT, &[G_WORLD_GRID, inst, 4]) as i32;
        if ev != -1 {
            let s = env.call(m, MEMPOOL_ALLOC, &[G_SCRIPT_POOL, 0xf0, 0x4000_0000, NAME_SCRIPT_NODE, 0, 0]);
            if s != 0 {
                eamem::set(m, s, 0xdead_c0de, 0xf0);
            }
            let inst2 = m.r32(this + 0x28);
            env.call(m, SCRIPT_NODE_CTOR, &[s, 3, ev as u32, inst2, 0, 0, 0]);
        }
        call_node_vf(m, env, this, 0xb0, &[0]);
    }
    let fin = m.r32(this + 0x34) as i32;
    if fin > 0 {
        let nfin = fin.wrapping_sub(1);
        m.w32(this + 0x34, nfin as u32);
        if nfin == 0 {
            call_node_vf(m, env, this, 0xb0, &[1]);
        }
    }
}

/// `cTimerNode::Kill` 0x13ffa8(this): if `*(this+0x34) >= 0 && this != 0`, invoke deleting destructor `+8/+0xc(3)`.
pub fn timer_node_kill(m: &mut Mem, env: &mut dyn NodeEnv, this: u32) {
    if (m.r32(this + 0x34) as i32) >= 0 && this != 0 {
        call_node_vf(m, env, this, 8, &[3]);
    }
}

/// `cTimerNode::Finish` 0x13ffe8(this, flag): if `flag != 0` and `Controller_RunFinishScript` returns non-zero,
/// return; else if `*(this+0x34) >= 0 && this != 0`, invoke deleting destructor `+8/+0xc(3)`.
pub fn timer_node_finish(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flag: u32) {
    if flag != 0 && env.call(m, CONTROLLER_RUN_FINISH_SCRIPT, &[this]) != 0 {
        return;
    }
    if (m.r32(this + 0x34) as i32) >= 0 && this != 0 {
        call_node_vf(m, env, this, 8, &[3]);
    }
}

/// `cTimerNode::OnMessage` 0x140048(this, kind, val_f): for `kind == 1`, set `+0x38 = (int)(val_f * 60.0)`; else
/// tail-call `cControllerNode::OnMessage`.
pub fn timer_node_on_message(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, kind: u32, val: f32) {
    if kind == 1 {
        m.w32(this + 0x38, ((F(val) * F(60.0)).ee() as i32) as u32);
    } else {
        env.call(m, CONTROLLER_NODE_ON_MESSAGE, &[this, kind, val.to_bits()]);
    }
}

/// `cTimerNode::~cTimerNode` 0x149d98(this, flags): restore vtable `0x36d6b0` and tail-call
/// `cControllerNode::~cControllerNode`.
pub fn timer_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_TIMER_NODE);
    controller_node_dtor(m, env, this, flags);
}

/// `cBoostNode::~cBoostNode` 0x140630(this, flags): restore vtable `0x36d4d8`, remove contacts for `this+0x70`, and
/// tail-call `cControllerNode::~cControllerNode`.
pub fn boost_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_BOOST_NODE);
    let cm = contact_mgr(m);
    env.call(m, CONTACT_REMOVE_ALL_FOR_NODE, &[cm, this.wrapping_add(0x70)]);
    controller_node_dtor(m, env, this, flags);
}

/// `cLapBoostNode::~cLapBoostNode` 0x140b38(this, flags): restore vtable `0x36d3f8`, remove contacts for `this+0x70`,
/// and tail-call `cControllerNode::~cControllerNode`.
pub fn lap_boost_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_LAP_BOOST_NODE);
    let cm = contact_mgr(m);
    env.call(m, CONTACT_REMOVE_ALL_FOR_NODE, &[cm, this.wrapping_add(0x70)]);
    controller_node_dtor(m, env, this, flags);
}

/// `cLapBoostNode::Finish` 0x141198(this, flag): when `flag != 0`, tail-call `Controller_RunFinishScript(this)`.
pub fn lap_boost_node_finish(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flag: u32) {
    if flag != 0 {
        env.call(m, CONTROLLER_RUN_FINISH_SCRIPT, &[this]);
    }
}

/// `cRandomBoostNode::cRandomBoostNode` 0x141208(this, a1, inst, params): construct `cBoostNode(type 0x10)`, set
/// vtable `0x36d300`, copy `(params+0x28..0x34, 0)` into `+0x110`, zero 6 bytes at `+0x180`, initialize 6 vectors
/// at `+0x120` to `(1e10, 1e10, 1e10, 0)`, and return `this`.
pub fn random_boost_node_ctor(
    m: &mut Mem,
    env: &mut dyn NodeEnv,
    this: u32,
    a1: u32,
    inst: u32,
    params: u32,
) -> u32 {
    env.call(m, BOOST_NODE_CTOR, &[this, a1, 0x10, inst, params]);
    let x = m.r32(params + 0x28);
    let y = m.r32(params + 0x2c);
    let z = m.r32(params + 0x30);
    m.w32(this + 8, VT_RANDOM_BOOST_NODE);
    m.w32(this + 0x110, x);
    m.w32(this + 0x114, y);
    m.w32(this + 0x118, z);
    m.w32(this + 0x11c, 0);
    eamem::set(m, this + 0x180, 0, 6);
    for i in 0..6u32 {
        let v = this + 0x120 + i * 0x10;
        m.w32(v, HUGE_1E10_BITS);
        m.w32(v + 4, HUGE_1E10_BITS);
        m.w32(v + 8, HUGE_1E10_BITS);
        m.w32(v + 0xc, 0);
    }
    this
}

/// `cRandomBoostNode::vf28_1415a0` 0x1415a0(this): zero 6 bytes at `this+0x180`.
pub fn random_boost_node_vf28(m: &mut Mem, this: u32) {
    eamem::set(m, this + 0x180, 0, 6);
}

fn boost_reset_unused_riders(m: &mut Mem, this: u32, flags_off: u32, vecs_off: u32) {
    if (m.r32(this + 0x5c) as i32) > 0 {
        return;
    }
    let mut i = 0i32;
    loop {
        let app = m.r32(G_APP);
        let game = m.r32(app + 0x730);
        let mgr = m.r32(game + 0x1c);
        let n = m.r32(mgr + 0x88) as i32;
        if i >= n {
            break;
        }
        if m.r8(this + flags_off + i as u32) == 0 {
            m.w32(this + vecs_off + ((i as u32) << 4), HUGE_1E10_BITS);
        }
        i += 1;
    }
}

/// `cRandomBoostNode::vf29_1415b0` 0x1415b0(this): when `*(this+0x5c) <= 0`, set `+0x120[i].x = 1e10` for every
/// rider `i` whose `+0x180[i]` flag byte is 0.
pub fn random_boost_node_vf29(m: &mut Mem, this: u32) {
    boost_reset_unused_riders(m, this, 0x180, 0x120);
}

/// `cRandomBoostNode::~cRandomBoostNode` 0x149bf0(this, flags): restore vtable `0x36d300` and tail-call
/// `cBoostNode::~cBoostNode`.
pub fn random_boost_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_RANDOM_BOOST_NODE);
    boost_node_dtor(m, env, this, flags);
}

/// `cTubeEndBoostNode::cTubeEndBoostNode` 0x141688(this, a1, inst, params): construct `cBoostNode(type 0x18)`, set
/// vtable `0x36d208`, copy 3 vectors (`params+0x28/+0x34/+0x40`) into `+0x110/+0x120/+0x130`, scale
/// `params+0x4c..+0x54` by `100.0` into `+0x1a0..+0x1a8`, zero 6 bytes at `+0x1ac`, initialize 6 vectors at `+0x140`
/// to `(1e10, 1e10, 1e10, 0)`, and return `this`.
pub fn tube_end_boost_node_ctor(
    m: &mut Mem,
    env: &mut dyn NodeEnv,
    this: u32,
    a1: u32,
    inst: u32,
    params: u32,
) -> u32 {
    env.call(m, BOOST_NODE_CTOR, &[this, a1, 0x18, inst, params]);
    m.w32(this + 8, VT_TUBE_END_BOOST_NODE);
    for (k, (src, dst)) in [(0x28u32, 0x110u32), (0x34, 0x120), (0x40, 0x130)].into_iter().enumerate() {
        let _ = k;
        m.w32(this + dst, m.r32(params + src));
        m.w32(this + dst + 4, m.r32(params + src + 4));
        m.w32(this + dst + 8, m.r32(params + src + 8));
        m.w32(this + dst + 0xc, 0);
    }
    let hundred = F(100.0);
    for k in 0..3u32 {
        let v = F(f32::from_bits(m.r32(params + 0x4c + k * 4))) * hundred;
        m.w32(this + 0x1a0 + k * 4, v.0.to_bits());
    }
    eamem::set(m, this + 0x1ac, 0, 6);
    for i in 0..6u32 {
        let v = this + 0x140 + i * 0x10;
        m.w32(v, HUGE_1E10_BITS);
        m.w32(v + 4, HUGE_1E10_BITS);
        m.w32(v + 8, HUGE_1E10_BITS);
        m.w32(v + 0xc, 0);
    }
    this
}

/// `cTubeEndBoostNode::vf28_1419f8` 0x1419f8(this): zero 6 bytes at `this+0x1ac`.
pub fn tube_end_boost_node_vf28(m: &mut Mem, this: u32) {
    eamem::set(m, this + 0x1ac, 0, 6);
}

/// `cTubeEndBoostNode::vf29_141a08` 0x141a08(this): when `*(this+0x5c) <= 0`, set `+0x140[i].x = 1e10` for every
/// rider `i` whose `+0x1ac[i]` flag byte is 0.
pub fn tube_end_boost_node_vf29(m: &mut Mem, this: u32) {
    boost_reset_unused_riders(m, this, 0x1ac, 0x140);
}

/// `cTubeEndBoostNode::~cTubeEndBoostNode` 0x149b78(this, flags): restore vtable `0x36d208` and tail-call
/// `cBoostNode::~cBoostNode`.
pub fn tube_end_boost_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_TUBE_END_BOOST_NODE);
    boost_node_dtor(m, env, this, flags);
}

/// `cZBoostNode::~cZBoostNode` 0x141c80(this, flags): restore vtable `0x36d128`, remove contacts for `this+0x70`,
/// and tail-call `cControllerNode::~cControllerNode`.
pub fn z_boost_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_Z_BOOST_NODE);
    let cm = contact_mgr(m);
    env.call(m, CONTACT_REMOVE_ALL_FOR_NODE, &[cm, this.wrapping_add(0x70)]);
    controller_node_dtor(m, env, this, flags);
}

/// `cCrowdBoxNode::~cCrowdBoxNode` 0x149610(this, flags): restore vtable `0x36ca18` and tail-call
/// `cControllerNode::~cControllerNode`.
pub fn crowd_box_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_CROWD_BOX_NODE);
    controller_node_dtor(m, env, this, flags);
}

/// `cCrackedNode::~cCrackedNode` 0x1496f8(this, flags): restore vtable `0x36cbd8`, destruct `cTexFlip` at `+0x40`
/// (flags 2), and tail-call `cControllerNode::~cControllerNode`.
pub fn cracked_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_CRACKED_NODE);
    env.call(m, TEX_FLIP_DTOR, &[this.wrapping_add(0x40), 2]);
    controller_node_dtor(m, env, this, flags);
}

/// `cUVScrollTexFlipNode::~cUVScrollTexFlipNode` 0x149868(this, flags): restore vtable `0x36ce78` at `+0x2c`,
/// destruct `cTexFlip` at `+0x90` (flags 2), destruct `cUVScrollNode` at `+0x24` (flags 0), and `Mem_Delete(this)`
/// when `flags & 1 != 0`.
pub fn uv_scroll_tex_flip_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 0x2c, VT_UV_SCROLL_TEX_FLIP_NODE);
    env.call(m, TEX_FLIP_DTOR, &[this.wrapping_add(0x90), 2]);
    env.call(m, UV_SCROLL_NODE_DTOR, &[this.wrapping_add(0x24), 0]);
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `cTexFlipNode::~cTexFlipNode` 0x149938(this, flags): restore vtable `0x36cf58`, destruct `cTexFlip` at `+0x58`
/// (flags 2), and tail-call `cControllerNode::~cControllerNode`.
pub fn tex_flip_node_dtor(m: &mut Mem, env: &mut dyn NodeEnv, this: u32, flags: u32) {
    m.w32(this + 8, VT_TEX_FLIP_NODE);
    env.call(m, TEX_FLIP_DTOR, &[this.wrapping_add(0x58), 2]);
    controller_node_dtor(m, env, this, flags);
}

/// Additional no-op controller virtuals: `cLapBoostNode::Draw` 0x141188, `cLapBoostNode::Kill` 0x141190,
/// `cBoostNode::vf29_149cc8` 0x149cc8, `cBoostNode::vf28_149cd0` 0x149cd0.
pub fn boost_noop() {}
