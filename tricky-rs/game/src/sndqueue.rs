//! The audio system's pending-sound queue and the handlers that fill it (board row G10a2), checked in the function
//! runner (`tests/sndqueue.rs`). `Sfx_UpdateAir` 0x217d50 is the first handler.
//!
//! The queue is the audio system's `count` (+0x20) and a set of parallel arrays indexed by it (the entry being
//! `++count`): the 24-byte header copy (+0x24 points to the records), the emitter pointer (inline at +0x28), 0x38,
//! 0x3c (volume), 0x40 (the sound manager's slot), 0x44 (8 bytes each, inline), 0x64, 0x68 (the listener), 0x6c,
//! 0x70, 0x74 (16-byte rows), 0x78, 0x7c, 0xa40 (the kind), 0xa44 (the sound id).

use crate::jump::dot;

/// The queue arrays, each indexed by the entry.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Queue {
    /// +0x08..+0x20 of the audio system, copied into each entry's record.
    pub header: [u8; 24],
    /// +0x20.
    pub count: i32,
    pub record: Vec<[u8; 24]>,
    pub emitter: Vec<u32>,
    pub a38: Vec<u32>,
    pub a3c: Vec<u32>,
    pub a40: Vec<u32>,
    pub a44: Vec<[u32; 2]>,
    pub a64: Vec<u32>,
    pub a68: Vec<u32>,
    pub a6c: Vec<u32>,
    pub a70: Vec<u32>,
    pub a74: Vec<[u32; 4]>,
    pub a78: Vec<u32>,
    pub a7c: Vec<u32>,
    pub kind: Vec<u32>,
    pub id: Vec<u32>,
}

impl Queue {
    pub fn with_capacity(n: usize) -> Queue {
        Queue {
            header: [0; 24],
            count: 0,
            record: vec![[0; 24]; n],
            emitter: vec![0; n],
            a38: vec![0; n],
            a3c: vec![0; n],
            a40: vec![0; n],
            a44: vec![[0; 2]; n],
            a64: vec![0; n],
            a68: vec![0; n],
            a6c: vec![0; n],
            a70: vec![0; n],
            a74: vec![[0; 4]; n],
            a78: vec![0; n],
            a7c: vec![0; n],
            kind: vec![0; n],
            id: vec![0; n],
        }
    }
}

/// `SndSys_SetPendingVolume` 0x226dd8 (`cull`, `far`, the audio system): the top entry's cull distance (+0x70) =
/// `cull`, its fall-off distance (+0x6c) = `far`, or `cull` when `far` is below 0 (metres; `BankMonitor_AddInstance`
/// copies them to the instance's +0x8c and +0x88).
pub fn set_pending_volume(q: &mut Queue, cull: f32, far: f32) {
    let i = q.count as usize;
    q.a70[i] = cull.to_bits();
    q.a6c[i] = if 0.0 <= far { far } else { cull }.to_bits();
}

/// What the queue's users call out to.
pub trait QueueEnv {
    /// `SndMgr_GetSlot` 0x211f38 (the audio system, 4).
    fn get_slot(&mut self, which: u32) -> u32;
    /// `Audio_GetRiderListener` 0x20fc60 (the audio system, the rider).
    fn rider_listener(&mut self) -> u32;
    /// `SndSys_PlayPending` 0x20c948.
    fn play_pending(&mut self) -> u64;
}

/// The entry every handler builds (inlined in each): a default record with the `kind` (+0xa40) and sound `id`
/// (+0xa44); `volume` (clamped to the byte 0..127 into the record and kept in +0x3c) when the handler gives one;
/// `emitter` when the sound is the rider's (its pointer is stored and the listener asked for); then the slot the
/// manager gives. The caller plays the pending sounds (`update_air` and the others do it here, `play`).
pub fn queue_sound(q: &mut Queue, kind: u32, id: u32, volume: Option<i64>, emitter: Option<u32>, env: &mut dyn QueueEnv) {
    let i = queue_begin(q, kind, id, volume);
    let slot = env.get_slot(4);
    if let Some(e) = emitter {
        q.emitter[i] = e;
        q.a40[i] = slot;
        let l = env.rider_listener();
        q.a68[i] = l;
    } else {
        q.a40[i] = slot;
    }
    env.play_pending();
}

/// `SndSys_PushEntry` 0x2210e8: a new default entry (no kind or id yet): the new entry's index.
pub fn queue_push(q: &mut Queue) -> usize {
    q.count += 1;
    let i = q.count as usize;
    q.record[i] = q.header;
    q.emitter[i] = 0;
    q.a38[i] = 0;
    q.a3c[i] = q.record[i][0] as i8 as i32 as u32;
    q.a40[i] = 0;
    q.a44[i] = [0; 2];
    q.a68[i] = 0;
    q.a6c[i] = 0;
    q.a70[i] = 0;
    q.a74[i][1] = 0;
    q.a74[i][0] = 100;
    q.a64[i] = 0;
    q.a74[i][2] = 0x5a;
    q.a74[i][3] = 0x32;
    q.a78[i] = 0;
    q.a7c[i] = 1;
    i
}

/// The entry's volume: kept whole in +0x3c, clamped to the byte 0..127 into the record.
pub fn queue_volume(q: &mut Queue, i: usize, volume: i64) {
    q.a3c[i] = volume as u32;
    let v = volume as i32;
    q.record[i][0] = if v < 0 { 0 } else if v < 0x7f { v as u8 } else { 0x7f };
}

/// The part of the entry before the sound manager is asked for a slot: the new entry's index.
pub fn queue_begin(q: &mut Queue, kind: u32, id: u32, volume: Option<i64>) -> usize {
    let i = queue_push(q);
    q.kind[i] = kind;
    q.id[i] = id;
    if let Some(volume) = volume {
        queue_volume(q, i, volume);
    }
    i
}

/// The rider values `Sfx_UpdateAir` reads.
#[derive(Clone, Copy, Debug)]
pub struct AirRider {
    /// +0x41c: 1 for a rider the player's (the human) own.
    pub flag_41c: u32,
    /// +0x38: the player number.
    pub player: u32,
    /// +0x798: the rider's air speed (as an integer to the curve).
    pub air: f32,
    /// The rider's address + 0x140 (the emitter).
    pub emitter: u32,
}

pub trait AirEnv: QueueEnv {
    fn replay_is_playing(&mut self) -> bool;
    /// `Audio_GetListenerRider` 0x20ed20 (the audio system, the player or -1): the listener's rider.
    fn listener_rider(&mut self, who: u32) -> i64;
    /// 0x2210c8 (the audio system, who).
    fn crowd_off(&mut self, who: u32);
    /// 0x221068 (the audio system, level 0..2, who).
    fn crowd_level(&mut self, level: u32, who: u32);
    /// `Audio_FindRiderNear` 0x20f9b0 (40.0, the audio system, the emitter): negative when none.
    fn find_rider_near(&mut self, radius: f32, emitter: u32) -> i64;
    /// `Sfx_EvalCurve` 0x20d928 (the curve slot 0 or 1, the speed as an integer).
    fn eval_curve(&mut self, slot: u32, x: i32) -> i64;
    /// `Sfx_SurfaceSoundType` 0x20f908 (the audio system, the rider).
    fn surface_sound_type(&mut self) -> u32;
}

/// `Sfx_UpdateAir` 0x217d50: keep the air whoosh of a rider going, and the crowd's level for the listener.
/// `rider` is the rider's address as the listener returns it, `level` the crowd's excitement (f12).
pub fn update_air(q: &mut Queue, level: f32, rider_id: i64, r: &AirRider, env: &mut dyn AirEnv) {
    let mut who = 0xffff_ffffu32;
    if !env.replay_is_playing() {
        if r.flag_41c == 1 {
            who = r.player;
        }
    } else {
        who = 0;
    }
    if env.listener_rider(who) == rider_id {
        if level <= 0.0 {
            env.crowd_off(who);
        } else if level < 0.25 {
            env.crowd_level(0, who);
        } else if level < f32::from_bits(0x3f26_6666) {
            env.crowd_level(1, who);
        } else {
            env.crowd_level(2, who);
        }
    }
    if r.flag_41c != 1 && env.find_rider_near(40.0, r.emitter) < 0 {
        return;
    }
    let own = r.flag_41c == 1;
    let x = r.air as i32;
    let curve = env.eval_curve((!own) as u32, x);
    let mut vol = if curve < 0x80 { curve } else { 0x7f };
    if vol <= -1 {
        vol = 0;
    }
    let surface = env.surface_sound_type();
    let id = surface * 8 + if own { 1 } else { 5 };
    queue_sound(q, 1, id, Some(vol), Some(r.emitter), env);
}

/// The speed fraction of a rider's impact for the volume: `impact * 127 / 1000` through a float.
fn impact_volume(impact: i32) -> i32 {
    let q = (impact << 7).wrapping_sub(impact) / 1000;
    (q as f32) as i32
}

pub trait AirEnterEnv: QueueEnv {
    /// 0x233950 (no arguments), called when the rider's +0x57f4 is above 5.
    fn big_landing(&mut self);
    /// 0x235170 (the audio system, the rider, the caller's third argument).
    fn on_air_enter_prelude(&mut self);
    fn replay_is_playing(&mut self) -> bool;
    fn listener_rider(&mut self, who: u32) -> i64;
    /// 0x2210a8 (the audio system, who), when +0x57f4 is above 2.
    fn crowd_stop(&mut self, who: u32);
    /// 0x225768 (the audio system, 0), when +0x57f4 is above 1.5.
    fn crowd_clear(&mut self);
    /// `Sfx_ImpactSpeed` 0x21a6a8 (the rider).
    fn impact_speed(&mut self) -> i32;
    fn surface_sound_type(&mut self) -> u32;
}

/// `Sfx_OnAirEnter` 0x2174f8: a rider leaves the ground: the crowd reacts to how high it went (`height`, the rider's
/// +0x57f4), and for the listener's rider a landing-out sound is queued whose volume follows the impact speed (at
/// least 64, at most 127: the original evaluates `clamp` as nested conditionals that come to this).
pub fn on_air_enter(q: &mut Queue, height: f32, rider_id: i64, r: &AirRider, env: &mut dyn AirEnterEnv) {
    if 5.0 < height {
        env.big_landing();
    }
    env.on_air_enter_prelude();
    let mut who = 0xffff_ffffu32;
    if !env.replay_is_playing() {
        if r.flag_41c == 1 {
            who = r.player;
        }
    } else {
        who = 0;
    }
    if env.listener_rider(who) != rider_id {
        return;
    }
    if 2.0 < height {
        env.crowd_stop(who);
    }
    if 1.5 < height {
        env.crowd_clear();
    }
    let x = impact_volume(env.impact_speed());
    let vol = if x < 0x40 { 0x40 } else if x < 0x7f { x } else { 0x7f };
    let surface = env.surface_sound_type();
    queue_sound(q, 1, surface * 8 + 2, Some(vol as i64), Some(r.emitter), env);
}

pub trait WipeoutEnv {
    fn replay_is_playing(&mut self) -> bool;
    fn listener_rider(&mut self, who: u32) -> i64;
    /// 0x2210c8 (the audio system, who).
    fn crowd_off(&mut self, who: u32);
    /// `CrowdSys_StartBoxCheer` 0x221088 (the audio system, level 0..2, who).
    fn crowd_level(&mut self, level: u32, who: u32);
    /// 0x233d38 (the audio system, the rider).
    fn wipeout_voice(&mut self);
}

/// `Sfx_OnWipeout` 0x2181e8: the crowd's cheer for the listener's rider after a wipeout, by `level` (f12).
pub fn on_wipeout(level: f32, rider_id: i64, r: &AirRider, env: &mut dyn WipeoutEnv) {
    let mut who = 0xffff_ffffu32;
    if !env.replay_is_playing() {
        if r.flag_41c == 1 {
            who = r.player;
        }
    } else {
        who = 0;
    }
    if env.listener_rider(who) == rider_id {
        if level <= 0.0 {
            env.crowd_off(who);
        } else if level < 0.25 {
            env.crowd_level(0, who);
        } else if level < f32::from_bits(0x3f26_6666) {
            env.crowd_level(1, who);
        } else {
            env.crowd_level(2, who);
        }
        env.wipeout_voice();
    }
}

pub trait WipeoutImpactEnv: QueueEnv {
    /// 0x235090 (no arguments).
    fn prelude(&mut self);
    fn find_rider_near(&mut self, radius: f32, emitter: u32) -> i64;
    fn eval_curve(&mut self, slot: u32, x: i32) -> i64;
    /// The audio system's random function (its object's virtual +0x24).
    fn random(&mut self) -> u32;
}

/// `Sfx_OnWipeoutImpact` 0x218328: the thud of a wipeout, volume from curve 7 of the rider's air speed, the sound one of
/// three picked at random (0x30..0x32) unless the rider's +0x310 is 3 or 4 (then 0x40).
pub fn on_wipeout_impact(q: &mut Queue, state_310: u32, r: &AirRider, env: &mut dyn WipeoutImpactEnv) {
    env.prelude();
    if r.flag_41c != 1 && env.find_rider_near(40.0, r.emitter) < 0 {
        return;
    }
    let curve = env.eval_curve(7, r.air as i32);
    let mut vol = if curve < 0x80 { curve } else { 0x7f };
    if vol <= -1 {
        vol = 0;
    }
    let id = if state_310.wrapping_sub(3) < 2 {
        0x40
    } else {
        let u = env.random();
        ((u & 0x7fff) * 3) / 0x7fff + 0x30
    };
    queue_sound(q, 0, id, Some(vol), Some(r.emitter), env);
}

pub trait RiderResetEnv: QueueEnv {
    /// 0x233a18 (no arguments).
    fn prelude(&mut self);
    /// `StreamSys_StopStream` (the audio system, 3).
    fn stop_stream(&mut self, which: u32);
    /// 0x21c4d0 (the audio system).
    fn after_a(&mut self);
    /// 0x21d5c8 (the audio system).
    fn after_b(&mut self);
}

/// `Sfx_OnRiderReset` 0x218eb0: a rider is put back on the course: the reset sound (id 0x7b, no emitter), then the
/// streams and voices of the old run are stopped.
pub fn on_rider_reset(q: &mut Queue, env: &mut dyn RiderResetEnv) {
    env.prelude();
    queue_sound(q, 0, 0x7b, None, None, env);
    env.stop_stream(3);
    env.after_a();
    env.after_b();
}

pub trait TrickBookEnv {
    /// `SndMgr_GetSlot` (the audio system, 4).
    fn get_slot(&mut self) -> u32;
    /// The global at 0x39ecf0.
    fn global(&mut self) -> u64;
    /// `SndSys_ScheduleEvent` (the audio system, `delay`, `global`, `slot`, 0, 0x5f, 0x7f, 0).
    fn schedule(&mut self, delay: u32, global: u64, slot: u32);
}

/// `Sfx_OnTrickBookDone` 0x2190f0: the trick book fanfare, three events 250, 500 and 750 ms later.
pub fn on_trick_book_done(env: &mut dyn TrickBookEnv) {
    let slot = env.get_slot();
    let g = env.global();
    env.schedule(0xfa, g, slot);
    let slot = env.get_slot();
    env.schedule(500, g, slot);
    let slot = env.get_slot();
    env.schedule(0x2ee, g, slot);
}

pub trait WallHitEnv: QueueEnv {
    /// 0x2154e0 (the audio system, the rider): non-zero while a wall hit sound is already queued.
    fn is_queued(&mut self) -> bool;
    /// `SndSys_SetPendingVolume` 0x226dd8 (40.0, -1.0, the audio system).
    fn set_pending_volume(&mut self, a: f32, b: f32);
    /// `Audio_IsNearerPlayer2` 0x20fb68 (the audio system, the emitter): which player's listener.
    fn is_nearer_player2(&mut self, emitter: u32) -> u32;
    /// `SndSys_QueueSounds` 0x215998 (the audio system, the rider, what `play_pending` returned, `word_80 + 0x32`, 0).
    fn queue_sounds(&mut self, played: u64, id: u32);
}

/// `Sfx_OnWallHit` 0x2162f0: a rider hits a wall: unless a hit sound is already queued, the scrape (id 0x31, no
/// volume of its own, the manager's pending volume set to 40, -1) for whichever player is nearer, played at once,
/// then the follow-up sounds queued after it. `word_80` is the audio system's +0x80.
pub fn on_wall_hit(q: &mut Queue, word_80: u32, emitter: u32, env: &mut dyn WallHitEnv) {
    if env.is_queued() {
        return;
    }
    let i = queue_begin(q, 0, 0x31, None);
    let slot = env.get_slot(4);
    q.a40[i] = slot;
    env.set_pending_volume(40.0, -1.0);
    let l = env.is_nearer_player2(emitter);
    q.a68[i] = l;
    q.emitter[i] = emitter;
    let played = env.play_pending();
    env.queue_sounds(played, word_80.wrapping_add(0x32));
}

pub trait CollisionEnv: QueueEnv {
    /// The audio system's random function (its object's virtual +0x24).
    fn random(&mut self) -> u32;
}

/// `Sfx_OnRiderCollision` 0x216960: two riders bump (one of them the player's): at most one bump sound per 30 frames
/// (`last_bump` is the global at 0x343118, `frame` the app's +0x10), id 0x5a or 0x5b by chance, at the first rider.
pub fn on_rider_collision(q: &mut Queue, last_bump: &mut i32, frame: i32, flag_a: u32, flag_b: u32, emitter: u32, env: &mut dyn CollisionEnv) {
    if (flag_a == 1 || flag_b == 1) && 0x1e < frame.wrapping_sub(*last_bump) {
        let u = env.random();
        queue_sound(q, 0, ((u & 0x7fff) << 1) / 0x7fff + 0x5a, None, Some(emitter), env);
        *last_bump = frame;
    }
}

pub trait WipeoutLoopEnv {
    /// 0x20c2a0 (0.75, the global at 0x343050, the handle, 1): stop a voice with a fade.
    fn stop_voice(&mut self, fade: f32, handle: u32);
}

/// `Sfx_StopWipeoutLoop` 0x218b20: stop the rider's wipeout loop voice (the handle at the rider's +0x5ae8, negative
/// for none) and forget it.
pub fn stop_wipeout_loop(handle: &mut i32, env: &mut dyn WipeoutLoopEnv) {
    if -1 < *handle {
        env.stop_voice(f32::from_bits(0x3f40_0000), *handle as u32);
        *handle = -1;
    }
}

/// `Sfx_PlayMenuSelect` 0x2138f0: the menu click (id 0x70, byte 1 of the record 0x5c); the sound it started is kept in
/// the audio system's +0x6e48.
pub fn play_menu_select(q: &mut Queue, w6e48: &mut u32, env: &mut dyn QueueEnv) {
    let i = queue_begin(q, 0, 0x70, None);
    let slot = env.get_slot(4);
    q.a40[i] = slot;
    q.record[i][1] = 0x5c;
    *w6e48 = env.play_pending() as u32;
}

pub trait BoostEnv: QueueEnv {
    fn replay_is_playing(&mut self) -> bool;
    /// `SndSys_CancelEvents` 0x222df0 (the audio system, the global at 0x39eb58).
    fn cancel_events(&mut self);
}

/// `Sfx_UpdateBoost` 0x2192a8: each tick of a player's boost: the earlier boost events are cancelled, and on the
/// first tick (`w6e3c` 0) the boost start sound (id 0x78 at over two thirds of the boost bar, 0x79 over one third, else
/// 0x7a) is queued and its handle kept in `w6e38`; with `extra` (the caller's third argument) a second, 0x30 lower,
/// sound goes with it every tick. `w6e3c` counts the ticks.
pub fn update_boost(q: &mut Queue, w6e38: &mut u32, w6e3c: &mut u32, flag_41c: u32, boost: f32, extra: bool, env: &mut dyn BoostEnv) {
    if env.replay_is_playing() || flag_41c != 1 {
        return;
    }
    env.cancel_events();
    let mut id = 0x7a;
    if f32::from_bits(0x3f2a_7e78) < boost {
        id = 0x78;
    } else if f32::from_bits(0x3eaa_d18b) < boost {
        id = 0x79;
    }
    if *w6e3c == 0 {
        let i = queue_begin(q, 0, id, None);
        let slot = env.get_slot(4);
        q.a40[i] = slot;
        *w6e38 = env.play_pending() as u32;
    }
    if extra {
        let i = queue_begin(q, 0, id - 0x30, None);
        let slot = env.get_slot(4);
        q.a40[i] = slot;
        env.play_pending();
    }
    *w6e3c = w6e3c.wrapping_add(1);
}

pub trait ScriptSoundEnv: QueueEnv {
    /// 0x2154e0 (the audio system, the node): non-zero while its sound is already queued.
    fn is_queued(&mut self) -> bool;
    /// The audio system's virtual +0x6c (the audio system, 2, 0x7a): the special play for sound 0x7a.
    fn play_special(&mut self) -> u64;
    /// `SndSys_SetPendingVolume` 0x226dd8 (31.0, -1.0, the audio system).
    fn set_pending_volume(&mut self, a: f32, b: f32);
    /// `Audio_IsNearerPlayer2` 0x20fb68 (the audio system, the position): which player's listener.
    fn is_nearer_player2(&mut self, pos: [u32; 4]) -> u32;
    /// `SndSys_QueueSounds` 0x215998 (the audio system, the node, what was played, `word_80 + 0x32`, 0).
    fn queue_sounds(&mut self, played: u64, id: u32);
}

/// `Sfx_ScriptSoundPlay` 0x2165c8: a trigger script plays sound `id` at a node's position `pos` (its +0x30..0x40): unless the
/// node's sound is already queued, either (in game mode 4, for sound 0x7a) a special play, or a queue entry
/// (kind 2, flag +0x38 = 1) whose emitter is a copy of the position on the caller's stack (`local`), with the
/// pending volume set to 31, -1 and the listener of the nearer player; then the follow-up sounds are queued.
pub fn script_sound_play(q: &mut Queue, word_80: u32, mode_is_4: bool, id: i64, pos: [u32; 4], local: u32, env: &mut dyn ScriptSoundEnv) {
    if env.is_queued() {
        return;
    }
    if mode_is_4 && id == 0x7a {
        let played = env.play_special();
        env.queue_sounds(played, word_80.wrapping_add(0x32));
        return;
    }
    let i = queue_begin(q, 2, id as u32, None);
    let slot = env.get_slot(4);
    q.a40[i] = slot;
    q.a38[i] = 1;
    q.emitter[i] = local;
    env.set_pending_volume(f32::from_bits(0x41f0_0000), -1.0);
    let l = env.is_nearer_player2(pos);
    q.a68[i] = l;
    let played = env.play_pending();
    env.queue_sounds(played, word_80.wrapping_add(0x32));
}


pub trait InstanceCollisionEnv {
    /// `Audio_FindRiderNear` 0x20f9b0 (40.0, the audio system, the rider's position): negative when none.
    fn find_rider_near(&mut self, radius: f32) -> i64;
    /// 0x2154e0 (the audio system, the instance): non-zero while its sound is already queued.
    fn is_queued(&mut self) -> bool;
    /// `Sfx_EvalCurve` 0x20d928 (2, the speed as an integer).
    fn eval_curve(&mut self, slot: u32, x: i32) -> i64;
    /// 0x22d508 (the object at 0x34370c, the instance, its sound's word, the rider, the volume): the sound started,
    /// negative for none.
    fn start_instance_sound(&mut self, sound: u32, volume: i64) -> i64;
    /// `SndSys_QueueSounds` 0x215998 (the audio system, the instance, what was started, `word_80 + 0x32`, 0).
    fn queue_sounds(&mut self, started: i64, id: u32);
}

/// `Sfx_InstanceCollision` 0x216c50: a rider hits a world instance that has a sound (`sound`, the word its +0xf4
/// record points to; `None` for no record): the volume follows the rider's speed (curve 2, 0..127), and the sound is
/// started and its follow-ups queued unless a rider is out of earshot or the instance's sound is already queued.
pub fn instance_collision(vel: [f32; 4], sound: Option<u32>, word_80: u32, env: &mut dyn InstanceCollisionEnv) {
    let Some(sound) = sound else { return };
    if env.find_rider_near(40.0) < 0 {
        return;
    }
    if env.is_queued() {
        return;
    }
    let speed = dot(vel, vel).sqrt();
    let curve = env.eval_curve(2, speed as i32);
    let mut vol = if curve < 0x80 { curve } else { 0x7f };
    if vol <= -1 {
        vol = 0;
    }
    let started = env.start_instance_sound(sound, vol);
    if started < 0 {
        return;
    }
    env.queue_sounds(started, word_80.wrapping_add(0x32));
}

pub trait PlacingChangeEnv: QueueEnv {
    /// `Audio_GetListenerRider` 0x20ed20 (the audio system, −1): the rider the sound follows.
    fn listener(&mut self) -> u32;
    /// `SndSys_IsQueued` 0x2154e0 (the audio system, 0).
    fn is_queued(&mut self) -> bool;
    /// `SndSys_QueueSounds` 0x215998 (the audio system, 0, what `play_pending` returned, `word_80 + 200`, 0).
    fn queue_sounds(&mut self, played: u64, id: u32);
    /// `Speech_OnPlacingChange` 0x2347c8 (the audio system, the rider who passed, the rider passed).
    fn speech_placing(&mut self, a: u32, b: u32);
}

/// `Sfx_OnPlacingChange` 0x219a48 (the audio system, the rider, the rider behind (`next`), the rider ahead (`prev`),
/// the old place, the new): for a local player's rider (+0x41c is 1) or the listener's, taking the lead (`new` 0)
/// queues sound 0x6d (no volume of its own) unless one is queued, played at once with the follow-ups queued after
/// it; then the passing line: the rider over the one now behind when it moved up, else the one ahead over it.
pub fn on_placing_change(q: &mut Queue, word_80: u32, rider: u32, flag_41c: u32, next: u32, prev: u32, old: i64, new: i64, env: &mut dyn PlacingChangeEnv) {
    if flag_41c != 1 && env.listener() != rider {
        return;
    }
    if new == 0 && !env.is_queued() {
        let i = queue_begin(q, 0, 0x6d, None);
        q.a40[i] = env.get_slot(4);
        let played = env.play_pending();
        env.queue_sounds(played, word_80.wrapping_add(200));
    }
    if new < old {
        env.speech_placing(rider, next);
    } else {
        env.speech_placing(prev, rider);
    }
}

pub trait CheerEnv {
    /// `Audio_GetListenerRider` 0x20ed20 (the audio system, the rider's player number).
    fn listener_rider(&mut self, who: u32) -> u32;
    /// `Replay_IsPlaying` 0x21bfd0.
    fn replay_is_playing(&mut self) -> bool;
    /// `Audio_PlayCrowdCheer` 0x221068 (`*0x343050`, 2, who): when the crowd (+0x372c) is there,
    /// `CrowdBox_PlayCheer` 0x2203a8 on its box `who` (0x190 bytes each).
    fn play_crowd_cheer(&mut self, level: u32, who: u32);
}

/// `Audio_CheerForRider` 0x21e528 (the audio system, the rider): when the rider is its player's listener, the
/// crowd's level-2 cheer on that player's box (the first, in a replay).
pub fn cheer_for_rider(rider: u32, player: u32, env: &mut dyn CheerEnv) {
    if env.listener_rider(player) == rider {
        let who = if env.replay_is_playing() { 0 } else { player };
        env.play_crowd_cheer(2, who);
    }
}

/// What the ring of queued sounds asks of the audio system.
pub trait RingEnv {
    /// `BankMonitor_Find` 0x20c200 (the audio system, the handle, 1): whether the sound still plays.
    fn find(&mut self, handle: u32) -> bool;
    /// `SndSys_StopBankSound` 0x20c2a0 (0, the audio system, the handle, 1).
    fn stop(&mut self, handle: u32);
}

/// The audio system's ring of queued sounds: records of four words (the owner, the sound's handle, the tick it ends
/// at, whether to stop it then) in a ring of words (+0x6e24 its size, 0x50 from `cBXAudio::cBXAudio` 0x20db18;
/// +0x6e28 the words, filled with 0xdeadc0ed; +0x6e2c the head, +0x6e30 the tail). One word is always left free.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundRing {
    pub words: Vec<u32>,
    pub head: i32,
    pub tail: i32,
}

impl Default for SoundRing {
    fn default() -> Self {
        SoundRing { words: vec![0xdead_c0ed; 0x50], head: 0, tail: 0 }
    }
}

impl SoundRing {
    fn size(&self) -> i32 {
        self.words.len() as i32
    }
    fn used(&self) -> i32 {
        (self.head - self.tail + self.size()) % self.size()
    }
    fn skip(&mut self) {
        if self.head != self.tail {
            self.tail += 1;
            if self.size() <= self.tail {
                self.tail = 0;
            }
        }
    }
    fn pop(&mut self) -> u32 {
        if self.head == self.tail {
            return u32::MAX;
        }
        let v = self.words[self.tail as usize];
        self.skip();
        v
    }
    fn push(&mut self, v: u32) {
        if (self.head + 1) % self.size() != self.tail {
            self.words[self.head as usize] = v;
            self.head += 1;
            if self.size() <= self.head {
                self.head = 0;
            }
        }
    }
    /// The `i`th word from the tail, −1 past the head.
    fn peek(&self, i: i32) -> u32 {
        if self.head == self.tail || self.used() <= i {
            return u32::MAX;
        }
        self.words[((self.tail + i) % self.size()) as usize]
    }

    /// `SndSys_QueueSounds` 0x215998 (the audio system, the owner, the handle, the end tick, the flag): the record
    /// added; a full ring first drops its oldest record, stopping that sound.
    pub fn queue_sounds(&mut self, owner: u32, handle: u32, end: u32, flag: u32, env: &mut dyn RingEnv) {
        if (self.head + 1) % self.size() == self.tail {
            self.skip();
            let h = self.pop();
            env.stop(h);
            self.skip();
            self.skip();
        }
        for v in [owner, handle, end, flag] {
            self.push(v);
        }
    }

    /// `SndSys_IsQueued` 0x2154e0 (the audio system, the owner): whether a record of the owner holds a sound still
    /// playing (or a handle of 0).
    pub fn is_queued(&self, owner: u32, env: &mut dyn RingEnv) -> bool {
        let n = self.used();
        let mut i = 0;
        while i < n {
            if self.peek(i) == owner {
                let h = self.peek(i + 1);
                if h == 0 || env.find(h) {
                    return true;
                }
            }
            i += 4;
        }
        false
    }

    /// `SndSys_UpdateQueuedSounds` 0x215658 (the audio system; `now` its tick, +0x80), from `Audio_InGameUpdate`:
    /// each record is taken off; one whose handle is 0 or still playing goes back while the tick is before its end,
    /// else, with its flag set, its sound is stopped.
    pub fn update(&mut self, now: i32, env: &mut dyn RingEnv) {
        let n = self.used() >> 2;
        for _ in 0..n {
            let owner = self.pop();
            let handle = self.pop() as i32;
            let end = self.pop() as i32;
            let flag = self.pop();
            if handle >= 0 && (handle == 0 || env.find(handle as u32)) {
                if now < end {
                    for v in [owner, handle as u32, end as u32, flag] {
                        self.push(v);
                    }
                } else if flag != 0 {
                    env.stop(handle as u32);
                }
            }
        }
    }
}

/// `DAT_00343998`, the radius (m) the announcer's rider lines are heard within (`Audio_FindRiderNear`'s first argument
/// in `speech`)
pub const SPEECH_NEAR: f32 = 30.0;

/// `Audio_FindRiderNear` 0x20f9b0 (f12 the radius in metres, the audio system, a position): the first view
/// (`Audio_RiderCount` 0x21c078: the race's +0x298, 1 with no race; `Audio_GetRiderPos` 0x21c010: the race's
/// +0xb0 + 0x80 × i) whose position is within the radius (×100 cm, squared) of `at`, −1 for none. `count` and `view`
/// stand for the two calls (asked again each turn, as the original does)
pub fn find_rider_near(radius: f32, count: &mut dyn FnMut() -> i32, view: &mut dyn FnMut(i32) -> [f32; 3], at: [f32; 3]) -> i32 {
    let r = radius * 100.0;
    let lim = r * r;
    let mut i = 0;
    while i < count() {
        let p = view(i);
        let (dx, dy, dz) = (p[0] - at[0], p[1] - at[1], p[2] - at[2]);
        if dx * dx + dy * dy + dz * dz < lim {
            return i;
        }
        i += 1;
    }
    -1
}

/// The audio system's event sound (+0x7420 pending, +0x7424 its handle, +0x7428 its kind): a loop the rider's event
/// bits ask for (`cBoarder::vf10` 0x11a348: bits 0xe, 0x17, 0x18 → kinds 0, 1, 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventSound {
    pub pending: u32,
    pub handle: u32,
    pub kind: u32,
}

impl Default for EventSound {
    /// `cBXAudio::cBXAudio` 0x20db18: the handle −1 (0x20dbdc), none pending (0x20dc18)
    fn default() -> Self {
        EventSound { pending: 0, handle: u32::MAX, kind: 0 }
    }
}

/// The banks the event sound loads: kind 0 (and any kind over 2) the tunnel's (0x39f1c0), kind 1 the river's (0x39f1d8)
pub const EVENT_BANKS: [&str; 2] = ["|data\\audio\\Tunnel1.bnk", "|data\\audio\\RiverSmall.bnk"];

pub trait EventSoundEnv: QueueEnv {
    /// `Audio_FindBankName` 0x21a640 then `Audio_LoadAmbientBank` 0x21a548 (the audio system, the name found): 1 when
    /// the bank is in
    fn load_ambient_bank(&mut self, name: &str) -> i64;
    /// `SndSys_StopBankSound` 0x20c2a0(f12 2.0, the audio object, the handle, 1)
    fn stop_bank_sound(&mut self, handle: u32);
}

impl EventSound {
    /// `Audio_QueueEventSound` 0x21d0b0: the kind kept, the sound pending
    pub fn queue(&mut self, kind: u32) {
        self.kind = kind;
        self.pending = 1;
    }

    /// `Audio_StopEventSound` 0x21d5c8: a handle kept (not negative) is stopped; none kept after
    pub fn stop(&mut self, env: &mut dyn EventSoundEnv) {
        if (self.handle as i32) >= 0 {
            env.stop_bank_sound(self.handle);
        }
        self.handle = u32::MAX;
    }

    /// `Audio_PlayEventSound` 0x21d0c0 (from `Audio_InGameUpdate` 0x20ee90 at 0x20ef24): a pending sound played.
    /// Kinds 0 and 1 load their bank (nothing more when it is not in, still pending) and play its sound 0 as kind 6
    /// at volume 0x23; kind 2 plays sound 9 as kind 2. The handle is kept and nothing is pending after.
    pub fn update(&mut self, q: &mut Queue, env: &mut dyn EventSoundEnv) {
        if self.pending == 0 {
            return;
        }
        if self.kind == 2 {
            queue_begin(q, 2, 9, None);
        } else {
            let name = EVENT_BANKS[(self.kind == 1) as usize];
            if env.load_ambient_bank(name) != 1 {
                return;
            }
            queue_begin(q, 6, 0, Some(0x23));
        }
        let i = q.count as usize;
        q.a40[i] = env.get_slot(4);
        self.handle = env.play_pending() as u32;
        self.pending = 0;
    }
}

/// The audio system's ambient ("swap") banks: the names `Audio_LoadInGameConfig` 0x211098 lists from BANKS.INF's
/// `SWAP` keys (+0x38e0, 0x40 bytes each, at most 200; their count +0x38dc) and a loaded flag for each (+0x6ae0).
/// One of them at a time sits in the bank manager's slot 6.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmbientBanks {
    pub names: Vec<Vec<u8>>,
    pub loaded: [u32; 200],
}

impl Default for AmbientBanks {
    fn default() -> Self {
        AmbientBanks { names: Vec::new(), loaded: [0; 200] }
    }
}

/// The bank manager's slot 6 (the system +0xa3c, 0x60 bytes a slot).
pub trait AmbientEnv {
    /// the slot's state (+0x240: 0 empty, 1 in, 2 loading)
    fn state(&mut self) -> i32;
    /// `BankManager_UnloadBank` 0x20a508 (the system, 6)
    fn unload(&mut self);
    /// `BankManager_LoadBank` 0x20a520 (the system, 6, the name, 0x10)
    fn load(&mut self, name: &[u8]);
}

impl AmbientBanks {
    /// `Audio_FindBankName` 0x21a640: the index of the list entry equal to `name` (`strcmp` over all 200, whatever the
    /// count), −1 when none
    pub fn find(&self, name: &[u8]) -> i32 {
        (0..200).find(|&i| self.names.get(i).map_or(name.is_empty(), |n| n.as_slice() == name)).map_or(-1, |i| i as i32)
    }

    /// `Audio_ClearAmbientLoaded` 0x21a468: every flag 0
    pub fn clear_loaded(&mut self) {
        self.loaded = [0; 200];
    }

    /// `Audio_LoadAmbientBank` 0x21a548 (the system, the index): 1 when that bank is in. Not marked loaded: slot 6
    /// emptied first (flags cleared, unloaded) unless it is loading (−1 then). Then slot 6 in: 1; empty: the bank
    /// marked and its load asked for, −1; loading: −1. (An index of −1 would read +0x6adc, the word before the flags;
    /// every course of the disc's BANKS.INF lists the event sound's banks.)
    pub fn load(&mut self, index: usize, env: &mut dyn AmbientEnv) -> i64 {
        if self.loaded[index] == 0 {
            let s = env.state();
            if (0..2).contains(&s) {
                self.clear_loaded();
                env.unload();
            } else if s == 2 {
                return -1;
            }
        }
        match env.state() {
            1 => 1,
            0 => {
                self.loaded[index] = 1;
                let name = self.names.get(index).cloned().unwrap_or_default();
                env.load(&name);
                -1
            }
            _ => -1,
        }
    }
}

/// `SndPlayOpts_Init` 0x2d4a60: a play record's defaults (volume 0x7f, pan 0x40, 0x7f, root 0x3c, 0x7f, then the
/// half-words 0, 0, 0x1000, 0x1000, 0x1000, 0, 0xffff, 0; bytes 5..7 left as they were). `cSndPlayOpts::cSndPlayOpts`
/// 0x226848 (built under every audio system, from `cMonitor::cMonitor` 0x222b10) runs it on the stack's first record
/// and copies that into the system's header (+8, 0x18 bytes), the record `queue_begin` copies into each sound.
pub fn play_opts_init(r: &mut [u8; 24]) {
    r[..5].copy_from_slice(&[0x7f, 0x40, 0x7f, 0x3c, 0x7f]);
    for (k, h) in [0u16, 0, 0x1000, 0x1000, 0x1000, 0, 0xffff, 0].into_iter().enumerate() {
        r[8 + 2 * k..10 + 2 * k].copy_from_slice(&h.to_le_bytes());
    }
}

/// The system's header (+8) as `cSndPlayOpts::cSndPlayOpts` 0x226848 leaves it: [`play_opts_init`] on a record the
/// allocator filled with 0xdeadc0ed (`EAMem_Set`), so bytes 5..7 are 0xc0, 0xad, 0xde.
pub fn play_opts_header() -> [u8; 24] {
    let mut r = [0u8; 24];
    for (k, b) in r.iter_mut().enumerate() {
        *b = 0xdead_c0ed_u32.to_le_bytes()[k % 4];
    }
    play_opts_init(&mut r);
    r
}

// ---- The rider's sound loops (board row E1b2g7d5g) ----

/// The rider values the loops' starts read and write.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoopRider {
    /// The rider's address (the instance's owner, +0x64; +0x140 is the emitter).
    pub addr: u32,
    /// +0x41c: 1 for the player's own rider.
    pub flag_41c: u32,
    /// +0x424: the rider's motion (1 air, 2 ground, 3 a rail).
    pub motion: u32,
    /// +0x788 / +0x78c: the motion the loops last saw.
    pub w788: u32,
    pub w78c: u32,
    /// +0x790 / +0x794: the surface sound type the loops last saw.
    pub w790: u32,
    pub w794: u32,
    /// +0x5ae4: the sound flag (the loops silent while set).
    pub w5ae4: u32,
    /// +0x5af0: the loops' fade-in count (`Boarder_TickSfxFadeIn` 0x11f468 counts it down).
    pub w5af0: i32,
}

pub trait RiderLoopEnv: QueueEnv {
    /// `Sfx_SurfaceSoundType` 0x20f908 (the audio system, the rider).
    fn surface_sound_type(&mut self, rider: u32) -> u32;
    /// `SndSys_SetPendingVolume` 0x226dd8 (the volume, -1.0, the audio system).
    fn set_pending_volume(&mut self, a: f32, b: f32);
}

/// The update callbacks the loops' entries carry (+0x44: a pointer to member, no this-adjust, no vtable index, the
/// function): `Sfx_UpdateScrapeLoop` 0x21b440, `Sfx_UpdateGlideLoop` 0x21b508, `Sfx_UpdateAiGlideLoop` 0x21b628,
/// `Sfx_UpdateCarveLoop` 0x21b718.
pub const SCRAPE_LOOP_UPDATE: u32 = 0x21b440;
pub const GLIDE_LOOP_UPDATE: u32 = 0x21b508;
pub const AI_GLIDE_LOOP_UPDATE: u32 = 0x21b628;
pub const CARVE_LOOP_UPDATE: u32 = 0x21b718;

/// One loop's entry: kind 1, the id, volume 0, the slot, the rider's emitter and update callback, the rider as the
/// owner, the listener when `listen`, +0x7c cleared, the volume `vol` (-1.0) and played (the starts write these in
/// different orders; only the calls' order shows).
fn start_loop(q: &mut Queue, r: &LoopRider, id: u32, update: u32, listen: bool, vol: f32, env: &mut dyn RiderLoopEnv) {
    let i = queue_begin(q, 1, id, Some(0));
    q.a40[i] = env.get_slot(4);
    q.emitter[i] = r.addr.wrapping_add(0x140);
    q.a44[i] = [0xffff_0000, update];
    q.a64[i] = r.addr;
    if listen {
        q.a68[i] = env.rider_listener();
    }
    q.a7c[i] = 0;
    env.set_pending_volume(vol, -1.0);
    env.play_pending();
}

/// `Sfx_StartScrapeLoop` 0x20fce0 (the audio system, the rider): the player's rider's scrape loop, sound 0 (zboard's slow
/// scrape; it sounds only on the ground, see [`update_scrape_loop`]).
pub fn start_scrape_loop(q: &mut Queue, r: &LoopRider, env: &mut dyn RiderLoopEnv) {
    if r.flag_41c == 1 {
        start_loop(q, r, 0, SCRAPE_LOOP_UPDATE, true, 40.0, env);
    }
}

/// `Sfx_StartGlideLoop` 0x210048 (the audio system, the rider, the surface sound type): the glide loop, sound
/// `surface * 8 + 4` heard by the rider's listener for the player's rider, else `surface * 8 + 6` (the AI's, louder,
/// no listener).
pub fn start_glide_loop(q: &mut Queue, r: &LoopRider, surface: u32, env: &mut dyn RiderLoopEnv) {
    if r.flag_41c == 1 {
        start_loop(q, r, surface.wrapping_mul(8).wrapping_add(4), GLIDE_LOOP_UPDATE, true, 40.0, env);
    } else {
        start_loop(q, r, surface.wrapping_mul(8).wrapping_add(6), AI_GLIDE_LOOP_UPDATE, false, 50.0, env);
    }
}

/// `Sfx_StartCarveLoop` 0x2106c0 (the audio system, the rider, the surface sound type): the player's rider's carve
/// loop, sound `surface * 8 + 3`.
pub fn start_carve_loop(q: &mut Queue, r: &LoopRider, surface: u32, env: &mut dyn RiderLoopEnv) {
    if r.flag_41c == 1 {
        start_loop(q, r, surface.wrapping_mul(8).wrapping_add(3), CARVE_LOOP_UPDATE, true, 40.0, env);
    }
}

/// `Sfx_StartRiderLoops` 0x210ad0 (the audio system, the rider): the motion and surface remembered, the three loops
/// started.
pub fn start_rider_loops(q: &mut Queue, r: &mut LoopRider, env: &mut dyn RiderLoopEnv) {
    let motion = r.motion;
    let surface = env.surface_sound_type(r.addr);
    r.w78c = motion;
    r.w788 = motion;
    r.w790 = surface;
    r.w794 = surface;
    start_scrape_loop(q, r, env);
    start_glide_loop(q, r, surface, env);
    start_carve_loop(q, r, surface, env);
}

pub trait InitRiderSoundsEnv: RiderLoopEnv {
    /// `Audio_CancelRaceEvents` 0x210b50.
    fn cancel_race_events(&mut self);
    /// The race's rider count (`gApp` 0x337c58, +0x730 → +0x1c → +0x88), read before each rider.
    fn rider_count(&mut self) -> i32;
    /// `Race_GetRider` (the race, the index): the rider's address.
    fn get_rider(&mut self, i: i32) -> u32;
}

/// `Audio_InitRiderSounds` 0x210a38 (the audio system): the race's events cancelled, every rider's loops started
/// (`riders` found by address), then the audio system's +0x38a0 cleared.
pub fn init_rider_sounds(q: &mut Queue, w38a0: &mut u32, riders: &mut [LoopRider], env: &mut dyn InitRiderSoundsEnv) {
    env.cancel_race_events();
    let mut i = 0;
    while i < env.rider_count() {
        let a = env.get_rider(i);
        let r = riders.iter_mut().find(|r| r.addr == a).expect("a rider the race gives");
        start_rider_loops(q, r, env);
        i += 1;
    }
    *w38a0 = 0;
}

// ---- The rider's sound loops' updates (board row E1b2g7d5h) ----

/// The snow program tables (`cBXAudio` + table + surface · 0x50, one 0x50-byte record each, `Sfx_RunSnowProgram`
/// 0x21ae90): SNOW.INF's GLIDE, AIGLIDE and CARVE programs.
pub const GLIDE_PROGRAMS: u32 = 0x745c;
pub const AI_GLIDE_PROGRAMS: u32 = 0x777c;
pub const CARVE_PROGRAMS: u32 = 0x7a9c;

/// A loop's bank instance as its update sees it: its address and its volume (+0x64); the rider is the instance's
/// +0x80.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoopInstance {
    pub addr: u32,
    pub volume: i32,
}

pub trait LoopUpdateEnv: RiderLoopEnv {
    /// `Sfx_ImpactSpeed` 0x21a6a8 (the rider).
    fn impact_speed(&mut self, rider: u32) -> i32;
    /// `Sfx_EvalCurve` 0x20d928 (the curve slot, the impact speed).
    fn eval_curve(&mut self, slot: u32, x: i32) -> i64;
    /// `Music_CancelBreak` 0x2257d8 (the audio system, 0).
    fn cancel_break(&mut self);
    /// `BankInstance_Reset` 0x20a9d8 (the instance).
    fn reset_instance(&mut self, inst: u32);
    /// `Sfx_RunSnowProgram` 0x21ae90 (the record at the audio system + `table` + `surface` · 0x50, the rider, the
    /// volume's and the bend's words; `sfxcalc::run_snow_program`).
    fn run_snow_program(&mut self, table: u32, surface: u32, rider: u32, vol: &mut i32, bend: &mut i32);
    /// `BankInstance_SetPitchBend` 0x21e7e0 (the instance, the bend; `bankmon::set_pitch_bend`).
    fn set_pitch_bend(&mut self, inst: u32, bend: i64);
}

/// The end the updates share: silent while the rider's sound flag is set, the volume divided by the fade-in count
/// while it counts down (`Boarder_TickSfxFadeIn` 0x11f468), then the volume stored and the bend sent.
fn finish_loop(inst: &mut LoopInstance, r: &mut LoopRider, mut vol: i32, mut bend: i64, env: &mut dyn LoopUpdateEnv) {
    if r.w5ae4 != 0 {
        vol = 0;
        bend = 0;
    }
    let fade = crate::control::tick_sfx_fade_in(&mut r.w5af0);
    if fade > 0 {
        vol = vol.wrapping_div(fade);
    }
    inst.volume = vol;
    env.set_pitch_bend(inst.addr, bend);
}

/// `Sfx_UpdateScrapeLoop` 0x21b440 (the audio system, the instance): on the ground (+0x424 = 2) the volume and bend are
/// curves 5 and 6 of the impact speed, else silent with bend 64.
pub fn update_scrape_loop(inst: &mut LoopInstance, r: &mut LoopRider, env: &mut dyn LoopUpdateEnv) {
    let (mut vol, mut bend) = (0, 0x40);
    if r.motion == 2 {
        let x = env.impact_speed(r.addr);
        vol = env.eval_curve(5, x) as i32;
        bend = env.eval_curve(6, x);
    }
    finish_loop(inst, r, vol, bend, env);
}

/// `Sfx_UpdateGlideLoop` 0x21b508 (the audio system, the instance): a changed motion is remembered (+0x788) and,
/// unless it is 1, the music's break cancelled; off motion 1, a changed surface restarts the glide loop (the
/// instance reset), else the surface's GLIDE program gives the volume and bend.
pub fn update_glide_loop(q: &mut Queue, inst: &mut LoopInstance, r: &mut LoopRider, env: &mut dyn LoopUpdateEnv) {
    let motion = r.motion;
    if motion != r.w788 {
        r.w788 = motion;
        if motion != 1 {
            env.cancel_break();
        }
    }
    let (mut vol, mut bend) = (0, 0x40);
    if motion != 1 {
        let surface = env.surface_sound_type(r.addr);
        if surface != r.w790 {
            r.w790 = surface;
            env.reset_instance(inst.addr);
            start_glide_loop(q, r, surface, env);
        } else {
            env.run_snow_program(GLIDE_PROGRAMS, surface, r.addr, &mut vol, &mut bend);
        }
    }
    finish_loop(inst, r, vol, bend as i64, env);
}

/// `Sfx_UpdateAiGlideLoop` 0x21b628 (the audio system, the instance): the motion remembered (+0x788); on the
/// ground or a rail (+0x424 2 or 3) a changed surface restarts the glide loop (the instance reset) and the surface's
/// AIGLIDE program gives the volume and bend; no fade-in, no sound flag.
pub fn update_ai_glide_loop(q: &mut Queue, inst: &mut LoopInstance, r: &mut LoopRider, env: &mut dyn LoopUpdateEnv) {
    let motion = r.motion;
    if motion != r.w788 {
        r.w788 = motion;
    }
    let (mut vol, mut bend) = (0, 0x40);
    if (motion as i32) < 4 && 1 < (motion as i32) {
        let surface = env.surface_sound_type(r.addr);
        if surface != r.w790 {
            r.w790 = surface;
            env.reset_instance(inst.addr);
            let surface = env.surface_sound_type(r.addr);
            start_glide_loop(q, r, surface, env);
        }
        let surface = env.surface_sound_type(r.addr);
        env.run_snow_program(AI_GLIDE_PROGRAMS, surface, r.addr, &mut vol, &mut bend);
    }
    inst.volume = vol;
    env.set_pitch_bend(inst.addr, bend as i64);
}

/// `Sfx_UpdateCarveLoop` 0x21b718 (the audio system, the instance): off motion 1, a changed surface (+0x794)
/// restarts the carve loop (the instance reset), else the surface's CARVE program gives the volume and bend.
pub fn update_carve_loop(q: &mut Queue, inst: &mut LoopInstance, r: &mut LoopRider, env: &mut dyn LoopUpdateEnv) {
    let (mut vol, mut bend) = (0, 0x40);
    if r.motion != 1 {
        let surface = env.surface_sound_type(r.addr);
        if surface != r.w794 {
            r.w794 = surface;
            env.reset_instance(inst.addr);
            let surface = env.surface_sound_type(r.addr);
            start_carve_loop(q, r, surface, env);
        } else {
            let surface = env.surface_sound_type(r.addr);
            env.run_snow_program(CARVE_PROGRAMS, surface, r.addr, &mut vol, &mut bend);
        }
    }
    finish_loop(inst, r, vol, bend as i64, env);
}

/// The update an entry's callback word (+0x44) names, run on its instance.
pub fn update_loop(update: u32, q: &mut Queue, inst: &mut LoopInstance, r: &mut LoopRider, env: &mut dyn LoopUpdateEnv) {
    match update {
        SCRAPE_LOOP_UPDATE => update_scrape_loop(inst, r, env),
        GLIDE_LOOP_UPDATE => update_glide_loop(q, inst, r, env),
        AI_GLIDE_LOOP_UPDATE => update_ai_glide_loop(q, inst, r, env),
        CARVE_LOOP_UPDATE => update_carve_loop(q, inst, r, env),
        _ => {}
    }
}

// ---- cPS2SoundManager, cSoundSys, cSndPlayOpts & SndSys tail (row G10s, on engine memory) ----

use crate::eamem;
use crate::pf::Mem;

pub const MEM_DELETE: u32 = 0x114f80;
pub const MEM_FREE: u32 = 0x14a040;
pub const PATHX_GET_SONG_STATE: u32 = 0x224620;
pub const EAMEM_ALLOC: u32 = 0x2ccf70;
pub const SND_GET_CONFIG: u32 = 0x2cfeb0;
pub const SND_SET_CONFIG: u32 = 0x2d0080;
pub const SND_INIT: u32 = 0x2d0260;
pub const SND_SHUTDOWN: u32 = 0x2d0430;
pub const SND_START_STREAM_POLL: u32 = 0x2d0548;
pub const SND_SET_SPU_RANGE: u32 = 0x2d0a80;
pub const SND_GET_LARGEST_SPU_FREE: u32 = 0x2d0aa0;
pub const DTS_INSTALL_ENCODER: u32 = 0x2d6ad0;

pub const G_SOUND_SYS: u32 = 0x3436f8;
pub const NAME_SOUND_HEAP: u32 = 0x3a01d8;
pub const NAME_PS2_SOUND_MANAGER: u32 = 0x3a01e8;
pub const VT_PS2_SOUND_MANAGER: u32 = 0x3a01f8;
pub const VT_SND_PLAY_OPTS: u32 = 0x3a02f8;
pub const VT_SOUND_SYS: u32 = 0x3a0340;
pub const SND_CONFIG_CALLBACK: u32 = 0x225fa0;

/// Calls out for `cPS2SoundManager`, `cSoundSys`, `cSndPlayOpts`, and `Audio_GetSongState` on engine memory.
pub trait SoundMgrEnv {
    fn call(&mut self, m: &mut Mem, target: u32, args: &[u32]) -> u32;
}

/// `Audio_GetSongState` 0x225e98(this): return `PathX_GetSongState(*(this+0x36f0))`, or `-1` when null.
pub fn audio_get_song_state(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) -> u32 {
    let p = m.r32(this + 0x36f0);
    if p == 0 {
        0xffff_ffff
    } else {
        env.call(m, PATHX_GET_SONG_STATE, &[p])
    }
}

/// `cPS2SoundManager::cPS2SoundManager` 0x225fa8(this): set vtable `0x3a01f8`, zero heap/mode/lock, set SPU top and
/// list heads to `-1`, and return `this`.
pub fn ps2_sound_mgr_ctor(m: &mut Mem, this: u32) -> u32 {
    m.w32(this + 0x318, 0xffff_ffff);
    m.w32(this, VT_PS2_SOUND_MANAGER);
    m.w32(this + 4, 0);
    m.w32(this + 8, 0);
    m.w32(this + 0x30c, 0xffff_ffff);
    m.w32(this + 0x310, 0xffff_ffff);
    m.w32(this + 0x31c, 0);
    this
}

/// `cPS2SoundManager::vf7_225fd8` 0x225fd8(this, flags): destructor — restore vtable `0x3a01f8`, `Mem_Free(*(this+4))`
/// when non-null, and `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn ps2_sound_mgr_dtor(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, flags: u32) {
    m.w32(this, VT_PS2_SOUND_MANAGER);
    let heap = m.r32(this + 4);
    if heap != 0 {
        env.call(m, MEM_FREE, &[heap]);
    }
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `cPS2SoundManager::vf0_226040` 0x226040(this, mode, sp): allocate `0x32000` bytes (`"mSoundHeap"`), start stream
/// polling, configure output via virtual `+0x14`, call `SND_Init(heap, 0x32000, 0x21)`, initialize all 32 SPU
/// allocation records at `+0xc`, and set `+0x30c` to `base + SND_GetLargestSpuFree(&base)`.
pub fn ps2_sound_mgr_init(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, mode: u32, sp: u32) {
    let heap = env.call(m, EAMEM_ALLOC, &[NAME_SOUND_HEAP, 0x32000, 0]);
    if heap != 0 {
        eamem::set(m, heap, 0xdead_c0ed, 0x32000);
    }
    m.w32(this + 4, heap);
    env.call(m, SND_START_STREAM_POLL, &[]);
    let vt = m.r32(this);
    let adj = m.r16(vt + 0x10) as i16 as i32 as u32;
    let f = m.r32(vt + 0x14);
    env.call(m, f, &[this.wrapping_add(adj), mode]);
    let heap_now = m.r32(this + 4);
    env.call(m, SND_INIT, &[heap_now, 0x32000, 0x21]);
    m.w32(this + 0x318, 0xffff_ffff);
    m.w32(this + 0x310, 31);
    for i in (0..32i32).rev() {
        let rec = this.wrapping_add(0xc + (i as u32) * 0x18);
        m.w32(rec, i as u32);
        m.w32(rec + 4, 0xffff_ffff);
        m.w32(rec + 8, 0xffff_ffff);
        m.w32(rec + 0xc, 0);
        m.w32(rec + 0x10, 0);
        m.w32(rec + 0x14, (i - 1) as u32);
    }
    m.w32(this + 0x31c, 0);
    m.w32(sp, 0);
    let largest = env.call(m, SND_GET_LARGEST_SPU_FREE, &[sp]);
    let base = m.r32(sp);
    m.w32(this + 0x30c, base.wrapping_add(largest));
}

/// `cPS2SoundManager::vf1_226168` 0x226168(this, mode, cfg_buf): fetch `SND_GetConfig`, apply mode-specific voice
/// and mixer layout (stereo/mono `0`/`1`, surround `2`, DTS `3` via `Dts_InstallEncoder`), call `SND_SetConfig`,
/// and return `2` (or `1` early without calling `SND_SetConfig` when switching between incompatible PCM/DTS modes).
pub fn ps2_sound_mgr_set_mode(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, mode: u32, cfg_buf: u32) -> u32 {
    env.call(m, SND_GET_CONFIG, &[cfg_buf]);
    m.w32(cfg_buf + 0xf8, SND_CONFIG_CALLBACK);
    m.w16(cfg_buf + 0x26, 0x8ca0);
    m.w16(cfg_buf + 0x28, 0x5dc0);
    m.w8(cfg_buf + 0x36, 0x32);
    m.w8(cfg_buf + 0x33, 0);
    m.w8(cfg_buf + 0x35, 0x32);
    match mode as i32 {
        2 => {
            if m.r32(this + 8) == 2 {
                return 1;
            }
            m.w32(this + 8, 1);
            m.w8(cfg_buf + 0x2a, 0x10);
            m.w8(cfg_buf + 0x2b, 8);
            m.w8(cfg_buf + 0x2c, 0x30);
            m.w8(cfg_buf + 0x3d, 1);
            m.w8(cfg_buf + 0x3b, 2);
            m.w8(cfg_buf + 0x3f, 6);
            m.w16(cfg_buf + 0x40, 0x48);
            m.w16(cfg_buf + 0x42, 0x140);
            m.w16(cfg_buf + 0x44, 0x44);
            m.w16(cfg_buf + 0x46, 0x28);
            m.w16(cfg_buf + 0x48, 0x120);
            m.w16(cfg_buf + 0x4a, 0x24);
        }
        0 | 1 => {
            if m.r32(this + 8) == 2 {
                return 1;
            }
            m.w8(cfg_buf + 0x2a, 0x10);
            m.w32(this + 8, 1);
            m.w8(cfg_buf + 0x2b, 8);
            m.w8(cfg_buf + 0x2c, 0x30);
            m.w8(cfg_buf + 0x3d, 1);
            m.w8(cfg_buf + 0x3b, 2);
            m.w8(cfg_buf + 0x3f, 3);
            m.w16(cfg_buf + 0x40, 0x28);
            m.w16(cfg_buf + 0x42, 0x120);
            m.w16(cfg_buf + 0x44, 0x24);
        }
        3 => {
            if m.r32(this + 8) == 1 {
                return 1;
            }
            m.w8(cfg_buf + 0x2a, 0x10);
            m.w8(cfg_buf + 0x2b, 8);
            m.w8(cfg_buf + 0x2c, 0x18);
            m.w8(cfg_buf + 0x3b, 4);
            m.w8(cfg_buf + 0x3f, 3);
            m.w16(cfg_buf + 0x40, 0x28);
            m.w16(cfg_buf + 0x42, 0x120);
            m.w16(cfg_buf + 0x44, 0x24);
            m.w8(cfg_buf + 0x3d, 2);
            env.call(m, DTS_INSTALL_ENCODER, &[]);
            m.w32(this + 8, 2);
        }
        _ => {}
    }
    env.call(m, SND_SET_CONFIG, &[cfg_buf]);
    2
}

/// `cPS2SoundManager::vf2_226320` 0x226320(this): reset SPU range `(-1, -1)`, call `SND_Shutdown`, free and clear
/// `*(this+4)`.
pub fn ps2_sound_mgr_shutdown(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    env.call(m, SND_SET_SPU_RANGE, &[0xffff_ffff, 0xffff_ffff]);
    env.call(m, SND_SHUTDOWN, &[]);
    let heap = m.r32(this + 4);
    if heap != 0 {
        env.call(m, MEM_FREE, &[heap]);
    }
    m.w32(this + 4, 0);
}

/// `Audio_CreateSoundManager` 0x226370(): allocate `0x320` bytes (`"PS2SoundManager"`), fill with `0xdeadc0de`,
/// construct via `cPS2SoundManager::cPS2SoundManager`, and return the pointer.
pub fn audio_create_sound_manager(m: &mut Mem, env: &mut dyn SoundMgrEnv) -> u32 {
    let p = env.call(m, EAMEM_ALLOC, &[NAME_PS2_SOUND_MANAGER, 0x320, 0]);
    if p != 0 {
        eamem::set(m, p, 0xdead_c0de, 0x320);
    }
    ps2_sound_mgr_ctor(m, p)
}

/// `SpuAlloc_PopRecord` 0x2266a8(this): pop one SPU record from the free-record stack at `+0x310` (returning `0` when
/// empty), clear its `+0x14` next link to `-1`, and return its address.
pub fn spu_alloc_pop_record(m: &mut Mem, this: u32) -> u32 {
    let idx = m.r32(this + 0x310) as i32;
    if idx < 0 {
        return 0;
    }
    let rec = this.wrapping_add(0xc + (idx as u32) * 0x18);
    let next = m.r32(rec + 0x14);
    m.w32(this + 0x310, next);
    m.w32(rec + 0x14, 0xffff_ffff);
    rec
}

fn spu_alloc_reclaim_record(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, rec: u32) {
    let hi = m.r32(rec + 8);
    let locked = m.r32(this + 0x31c);
    m.w32(this + 0x30c, hi);
    m.w32(rec + 8, 0xffff_ffff);
    m.w32(rec + 4, 0xffff_ffff);
    m.w32(rec + 0xc, 0);
    let id = m.r32(rec);
    let old_pool = m.r32(this + 0x310);
    m.w32(rec + 0x10, 0);
    m.w32(rec + 0x14, old_pool);
    m.w32(this + 0x310, id);
    if locked == 0 {
        let top = m.r32(this + 0x30c);
        env.call(m, SND_SET_SPU_RANGE, &[0xffff_ffff, top]);
    }
}

/// `cPS2SoundManager::vf3_2263c8` 0x2263c8(this, size, sp): allocate `size` bytes from SPU RAM — reusing an exact or
/// larger free block from `+0x318` (splitting from the high end; preserving the original's un-updated head on exact
/// first-block reuse), or carving downward from `+0x30c` if unlocked and `SND_GetLargestSpuFree` fits. Returns record
/// index, `-1` (out of space), or `-2` (locked).
pub fn ps2_sound_mgr_alloc_spu(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, size: u32, sp: u32) -> u32 {
    let mut idx = m.r32(this + 0x318) as i32;
    let mut prev = 0u32;
    while idx >= 0 {
        let rec = this.wrapping_add(0xc + (idx as u32) * 0x18);
        let rec_sz = m.r32(rec + 0xc) as i32;
        if rec_sz == size as i32 {
            m.w32(rec + 0x10, 2);
            if prev != 0 {
                let next = m.r32(rec + 0x14);
                m.w32(prev + 0x14, next);
            }
            m.w32(rec + 0x14, 0xffff_ffff);
            return m.r32(rec);
        }
        if (size as i32) < rec_sz {
            let new_rec = spu_alloc_pop_record(m, this);
            let hi = m.r32(rec + 8);
            let mid = hi.wrapping_sub(size);
            m.w32(new_rec + 0xc, size);
            m.w32(new_rec + 0x14, 0xffff_ffff);
            m.w32(new_rec + 8, hi);
            m.w32(new_rec + 4, mid);
            m.w32(rec + 8, mid);
            m.w32(new_rec + 0x10, 2);
            m.w32(rec + 0xc, (rec_sz as u32).wrapping_sub(size));
            return m.r32(new_rec);
        }
        prev = rec;
        idx = m.r32(rec + 0x14) as i32;
    }
    m.w32(sp, 0);
    let largest = env.call(m, SND_GET_LARGEST_SPU_FREE, &[sp]);
    if (largest as i32) < (size as i32) {
        return 0xffff_ffff;
    }
    if m.r32(this + 0x31c) != 0 {
        return 0xffff_fffe;
    }
    let new_rec = spu_alloc_pop_record(m, this);
    let top = m.r32(this + 0x30c);
    let bot = top.wrapping_sub(size);
    m.w32(new_rec + 0xc, size);
    m.w32(new_rec + 0x14, 0xffff_ffff);
    m.w32(new_rec + 8, top);
    m.w32(new_rec + 4, bot);
    m.w32(new_rec + 0x10, 2);
    let new_top = m.r32(this + 0x30c).wrapping_sub(size);
    m.w32(this + 0x30c, new_top);
    env.call(m, SND_SET_SPU_RANGE, &[0xffff_ffff, new_top]);
    m.r32(new_rec)
}

/// `cPS2SoundManager::vf4_226520` 0x226520(this, handle): free SPU record `handle`, reclaiming contiguous frontier
/// blocks at `+0x30c` back into the pool stack (`+0x310`) or inserting into the `+4`-sorted free list at `+0x318`.
pub fn ps2_sound_mgr_free_spu(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, handle: u32) {
    let rec = this.wrapping_add(0xc + handle.wrapping_mul(0x18));
    m.w32(rec + 0x10, 1);
    let bot = m.r32(rec + 4);
    if bot == m.r32(this + 0x30c) {
        spu_alloc_reclaim_record(m, env, this, rec);
        loop {
            let head = m.r32(this + 0x318) as i32;
            if head < 0 {
                break;
            }
            let cur = this.wrapping_add(0xc + (head as u32) * 0x18);
            if m.r32(cur + 4) != m.r32(this + 0x30c) {
                break;
            }
            let next = m.r32(cur + 0x14);
            m.w32(this + 0x318, next);
            spu_alloc_reclaim_record(m, env, this, cur);
        }
    } else {
        let head = m.r32(this + 0x318) as i32;
        if head < 0 {
            let id = m.r32(rec);
            m.w32(rec + 0x14, head as u32);
            m.w32(this + 0x318, id);
        } else {
            let rec_id = m.r32(rec);
            let mut prev = 0u32;
            let mut cur_idx = head;
            loop {
                let cur = this.wrapping_add(0xc + (cur_idx as u32) * 0x18);
                if (bot as i32) < (m.r32(cur + 4) as i32) {
                    if prev != 0 {
                        let next = m.r32(prev + 0x14);
                        m.w32(rec + 0x14, next);
                        m.w32(prev + 0x14, rec_id);
                    } else {
                        m.w32(rec + 0x14, head as u32);
                        m.w32(this + 0x318, rec_id);
                    }
                    break;
                }
                let next_idx = m.r32(cur + 0x14) as i32;
                if next_idx < 0 {
                    m.w32(cur + 0x14, rec_id);
                    m.w32(rec + 0x14, 0xffff_ffff);
                    break;
                }
                prev = cur;
                cur_idx = next_idx;
            }
        }
    }
}

/// `cPS2SoundManager::vf5_226650` 0x226650(this, handle): set lock flag `+0x31c = 1` and restrict SPU range to record
/// `handle`'s `[+4, +8]`.
pub fn ps2_sound_mgr_lock_spu(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, handle: u32) {
    m.w32(this + 0x31c, 1);
    let rec = this.wrapping_add(0xc + handle.wrapping_mul(0x18));
    let lo = m.r32(rec + 4);
    let hi = m.r32(rec + 8);
    env.call(m, SND_SET_SPU_RANGE, &[lo, hi]);
}

/// `cPS2SoundManager::vf6_226678` 0x226678(this): restore SPU range `(-1, *(this+0x30c))` and clear lock flag `+0x31c`.
pub fn ps2_sound_mgr_unlock_spu(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    let top = m.r32(this + 0x30c);
    env.call(m, SND_SET_SPU_RANGE, &[0xffff_ffff, top]);
    m.w32(this + 0x31c, 0);
}

/// `cSoundSys::cSoundSys` 0x227960(this, mode): set vtable `0x3a0340`, store `this` in `gSoundSys` (`0x3436f8`),
/// create the `cPS2SoundManager` into `+0`, invoke its virtual `+0xc` with `mode`, and return `this`.
pub fn sound_sys_ctor(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, mode: u32) -> u32 {
    m.w32(this + 4, VT_SOUND_SYS);
    m.w32(G_SOUND_SYS, this);
    let mgr = audio_create_sound_manager(m, env);
    m.w32(this, mgr);
    let vt = m.r32(mgr);
    let adj = m.r16(vt + 8) as i16 as i32 as u32;
    let f = m.r32(vt + 0xc);
    env.call(m, f, &[mgr.wrapping_add(adj), mode]);
    this
}

/// `cSoundSys::~cSoundSys` 0x2279c8(this, flags): restore vtable `0x3a0340`, invoke `mgr`'s shutdown virtual `+0x1c`,
/// `Mem_Delete(mgr)`, clear `gSoundSys` (`0x3436f8`), and `Mem_Delete(this)` when `flags & 1 != 0`.
pub fn sound_sys_dtor(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, flags: u32) {
    m.w32(this + 4, VT_SOUND_SYS);
    let mgr = m.r32(this);
    let vt = m.r32(mgr);
    let adj = m.r16(vt + 0x18) as i16 as i32 as u32;
    let f = m.r32(vt + 0x1c);
    env.call(m, f, &[mgr.wrapping_add(adj)]);
    let mgr2 = m.r32(this);
    env.call(m, MEM_DELETE, &[mgr2]);
    m.w32(G_SOUND_SYS, 0);
    if (flags & 1) != 0 {
        env.call(m, MEM_DELETE, &[this]);
    }
}

/// `cSoundSys::vf1_227a48` 0x227a48(this, mode): forward to `*(this+0)`'s virtual `+0x14` with `mode`.
pub fn sound_sys_vf1(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, mode: u32) -> u32 {
    let mgr = m.r32(this);
    let vt = m.r32(mgr);
    let adj = m.r16(vt + 0x10) as i16 as i32 as u32;
    let f = m.r32(vt + 0x14);
    env.call(m, f, &[mgr.wrapping_add(adj), mode])
}

/// `cSndPlayOpts::~cSndPlayOpts` 0x226cc0(this, flags): restore vtable `0x3a02f8`, `Mem_Free` each non-null queue
/// array pointer at `+0x24, +0x38, +0x3c, +0x40, +0x64..=+0x7c`, and tail-call `cSoundSys::~cSoundSys`.
pub fn snd_play_opts_dtor(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, flags: u32) {
    m.w32(this + 4, VT_SND_PLAY_OPTS);
    for off in [0x24, 0x38, 0x3c, 0x40, 0x64, 0x68, 0x6c, 0x70, 0x74, 0x78, 0x7c] {
        let p = m.r32(this + off);
        if p != 0 {
            env.call(m, MEM_FREE, &[p]);
        }
    }
    sound_sys_dtor(m, env, this, flags);
}

/// `SndSys_SetEntryCallback` 0x226e58(this, cb): store 8-byte member-function callback `cb` into
/// `this + 0x44 + *(this+0x20) * 8`.
pub fn snd_sys_set_entry_callback(m: &mut Mem, this: u32, cb: u64) {
    let idx = m.r32(this + 0x20);
    let dst = this.wrapping_add(idx << 3).wrapping_add(0x44);
    m.w32(dst, cb as u32);
    m.w32(dst + 4, (cb >> 32) as u32);
}

fn snd_sys_init_entry_slot(m: &mut Mem, this: u32) {
    let idx = m.r32(this + 0x20);
    let rec = m.r32(this + 0x24).wrapping_add(idx.wrapping_mul(0x18));
    for k in (0..24u32).step_by(4) {
        let w = m.r32(this + 8 + k);
        m.w32(rec + k, w);
    }
    m.w32(this + 0x28 + (idx << 2), 0);
    let a38 = m.r32(this + 0x38);
    m.w32(a38.wrapping_add(idx << 2), 0);
    let vol0 = m.r8(m.r32(this + 0x24).wrapping_add(idx.wrapping_mul(0x18))) as i8 as i32 as u32;
    let a3c = m.r32(this + 0x3c);
    m.w32(a3c.wrapping_add(idx << 2), vol0);
    let a40 = m.r32(this + 0x40);
    m.w32(a40.wrapping_add(idx << 2), 0);
    let cb = this.wrapping_add(idx << 3).wrapping_add(0x44);
    m.w32(cb, 0);
    m.w32(cb + 4, 0);
    let a68 = m.r32(this + 0x68);
    m.w32(a68.wrapping_add(idx << 2), 0);
    let a74 = m.r32(this + 0x74).wrapping_add(idx << 4);
    m.w32(a74 + 4, 0);
    let a6c = m.r32(this + 0x6c);
    m.w32(a6c.wrapping_add(idx << 2), 0);
    m.w32(a74, 0x64);
    let a64 = m.r32(this + 0x64);
    m.w32(a64.wrapping_add(idx << 2), 0);
    let a70 = m.r32(this + 0x70);
    m.w32(a70.wrapping_add(idx << 2), 0);
    m.w32(a74 + 8, 0x5a);
    m.w32(a74 + 0xc, 0x32);
    let a78 = m.r32(this + 0x78);
    m.w32(a78.wrapping_add(idx << 2), 0);
    let a7c = m.r32(this + 0x7c);
    m.w32(a7c.wrapping_add(idx << 2), 1);
}

/// `SndSys_PushEntryCopy2` 0x227350(this): increment stack index `*(this+0x20)` and initialize the new slot from the
/// default 24-byte header at `+8` with default 3D attenuation `(100, 0, 90, 50)` and flags.
pub fn snd_sys_push_entry_copy2(m: &mut Mem, this: u32) {
    let next = m.r32(this + 0x20).wrapping_add(1);
    m.w32(this + 0x20, next);
    snd_sys_init_entry_slot(m, this);
}

/// `SndSys_ResetEntry` 0x227508(this): reset the current stack slot `*(this+0x20)` from the default 24-byte header at
/// `+8` with default 3D attenuation `(100, 0, 90, 50)` and flags.
pub fn snd_sys_reset_entry(m: &mut Mem, this: u32) {
    snd_sys_init_entry_slot(m, this);
}

/// `SndPlayOpts_InitCurrent` 0x227708(this): run `SndPlayOpts_Init` (`0x2d4a60`) on the current 24-byte play record at
/// `*(this+0x24) + *(this+0x20) * 0x18`.
pub fn snd_play_opts_init_current(m: &mut Mem, this: u32) {
    let idx = m.r32(this + 0x20);
    let rec = m.r32(this + 0x24).wrapping_add(idx.wrapping_mul(0x18));
    let mut buf = [0u8; 24];
    for (k, b) in buf.iter_mut().enumerate() {
        *b = m.r8(rec + k as u32);
    }
    play_opts_init(&mut buf);
    for (k, &b) in buf.iter().enumerate() {
        m.w8(rec + k as u32, b);
    }
}

// ---- cBXAudio, Music & Sfx tail (row G10u, on engine memory) ----

use crate::libc;
use crate::ps2f::F;

pub const RING_QUEUE_DESTROY: u32 = 0x209b40;
pub const SND_SYS_PLAY_PENDING: u32 = 0x20c948;
pub const SND_MGR_GET_SLOT: u32 = 0x211f38;
pub const SNOW_PROGRAM_DESTRUCT: u32 = 0x21e738;
pub const CONTROLLER_DTOR: u32 = 0x21e8d8;
pub const INI_OPEN: u32 = 0x221668;
pub const INI_CLOSE: u32 = 0x221788;
pub const INI_FIND_SECTION: u32 = 0x221af0;
pub const INI_READ_TOKEN: u32 = 0x221c90;
pub const INI_AT_SECTION_END: u32 = 0x221e90;
pub const INI_READ_STRING: u32 = 0x221ef8;
pub const SND_SYS_SCHEDULE_EVENT: u32 = 0x222cc0;
pub const SPEECH_SYS_SET_FOLLOW_STREAM: u32 = 0x228b88;
pub const STREAM_SYS_DESTROY_STREAM: u32 = 0x229d40;
pub const SPEECH_FREE_NARRATION: u32 = 0x230d78;
pub const SPEECH_ON_METER_FULL: u32 = 0x233e10;
pub const SPEECH_FREE_CHAR_SCRIPT_SAMPLES: u32 = 0x2356a0;
pub const EAMEM_FREE: u32 = 0x2ccfc0;
pub const SPRINTF: u32 = 0x2fca28;

pub const VT_BX_AUDIO: u32 = 0x39f200;
pub const STR_GLOBAL: u32 = 0x39ea80;
pub const STR_UNUSED_BANK: u32 = 0x39ed90;
pub const CB_BIG_AIR_WIND: u64 = 0x0021_b438_ffff_0000;
pub const CB_WORLD_TRIGGERS_ON: u64 = 0x0021_e4c0_ffff_0000;

/// `cBXAudio::~cBXAudio` 0x20ddb0(this, flags): restore vtable `0x39f200`, delete 3×10 snow-program buffers at
/// `+0x745c/+0x777c/+0x7a9c`, free narration & char script samples, destruct all 30 `SnowProgram`s in reverse,
/// destroy the ring queue at `+0x6e24` with flags `2`, and tail-call `cController::~cController`.
pub fn bx_audio_dtor(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, flags: u32) {
    m.w32(this + 4, VT_BX_AUDIO);
    for i in 0..10u32 {
        let base = this.wrapping_add(0x745c + i * 0x50);
        for off in [0u32, 0x320, 0x640] {
            let p = m.r32(base + off);
            env.call(m, MEM_DELETE, &[p]);
            m.w32(base + off, 0);
        }
    }
    env.call(m, SPEECH_FREE_NARRATION, &[this]);
    env.call(m, SPEECH_FREE_CHAR_SCRIPT_SAMPLES, &[this]);
    for start_off in [0x7a9cu32, 0x777c, 0x745c] {
        let start = this.wrapping_add(start_off);
        if start != 0 {
            let mut cur = start.wrapping_add(0x320);
            while cur != start {
                cur = cur.wrapping_sub(0x50);
                env.call(m, SNOW_PROGRAM_DESTRUCT, &[cur, 0]);
            }
        }
    }
    env.call(m, RING_QUEUE_DESTROY, &[this.wrapping_add(0x6e24), 2]);
    env.call(m, CONTROLLER_DTOR, &[this, flags]);
}

/// `cBXAudio::PlayFESound` 0x20eaf0(this, kind, id): push a default sound entry, set `(kind, id)`, bind volume slot 4
/// via `SndMgr_GetSlot`, and play via `SndSys_PlayPending`.
pub fn bx_audio_play_fe_sound(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, kind: u32, id: u32) -> u32 {
    snd_sys_push_entry_copy2(m, this);
    let idx = m.r32(this + 0x20);
    let a40 = m.r32(this + 0xa40);
    m.w32(a40.wrapping_add(idx << 2), kind);
    let a44 = m.r32(this + 0xa44);
    m.w32(a44.wrapping_add(idx << 2), id);
    let slot = env.call(m, SND_MGR_GET_SLOT, &[this, 4]);
    let idx2 = m.r32(this + 0x20);
    let v40 = m.r32(this + 0x40);
    m.w32(v40.wrapping_add(idx2 << 2), slot);
    env.call(m, SND_SYS_PLAY_PENDING, &[this])
}

/// `Audio_PlayBigAirWind` 0x20f630(this): if `+0x38a4 == 0`, push sound `(0, 0x20)` pointing its volume slot at
/// `this+0x38a0` with callback `0x0021_b438_ffff_0000`, play via `SndSys_PlayPending`, and set `+0x38a4 = 1`.
pub fn audio_play_big_air_wind(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    if m.r32(this + 0x38a4) != 0 {
        return;
    }
    snd_sys_push_entry_copy2(m, this);
    let idx = m.r32(this + 0x20);
    let a40 = m.r32(this + 0xa40);
    m.w32(a40.wrapping_add(idx << 2), 0);
    let a44 = m.r32(this + 0xa44);
    m.w32(a44.wrapping_add(idx << 2), 0x20);
    let v40 = m.r32(this + 0x40);
    m.w32(v40.wrapping_add(idx << 2), this.wrapping_add(0x38a0));
    let v7c = m.r32(this + 0x7c);
    m.w32(v7c.wrapping_add(idx << 2), 0);
    let cb = this.wrapping_add(idx << 3).wrapping_add(0x44);
    m.w32(cb, CB_BIG_AIR_WIND as u32);
    m.w32(cb + 4, (CB_BIG_AIR_WIND >> 32) as u32);
    env.call(m, SND_SYS_PLAY_PENDING, &[this]);
    m.w32(this + 0x38a4, 1);
}

/// `Audio_CancelMusicDuck` 0x20f8c0(this): dispatch virtual `+0x3c` (this-adjust `+0x38`) with callback
/// `0x0021_b438_ffff_0000`, then clear `+0x38a4` and `+0x38a0`.
pub fn audio_cancel_music_duck(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    let vt = m.r32(this + 4);
    let adj = m.r16(vt + 0x38) as i16 as i32 as u32;
    let f = m.r32(vt + 0x3c);
    env.call(
        m,
        f,
        &[this.wrapping_add(adj), CB_BIG_AIR_WIND as u32, (CB_BIG_AIR_WIND >> 32) as u32],
    );
    m.w32(this + 0x38a4, 0);
    m.w32(this + 0x38a0, 0);
}

/// `Audio_SetMusicMode` 0x20f998(this, mode): store `mode` at `+0x6e34` when changed.
pub fn audio_set_music_mode(m: &mut Mem, this: u32, mode: u32) {
    if m.r32(this + 0x6e34) != mode {
        m.w32(this + 0x6e34, mode);
    }
}

/// `Audio_GetVolumeSlot` 0x211f50(this, idx): return `this + 0x3848 + idx * 0xc`.
pub fn audio_get_volume_slot(this: u32, idx: u32) -> u32 {
    this.wrapping_add(idx.wrapping_mul(0xc)).wrapping_add(0x3848)
}

/// `Audio_FadeMusicTo` 0x211fb8(this, mode, target, secs): start a linear 60 Hz fade from `+0x38d4` to `target` over
/// `secs` seconds (no-op when `secs == 0.0`).
pub fn audio_fade_music_to(m: &mut Mem, this: u32, mode: u32, target: f32, secs: f32) {
    if F(secs).ee() == 0.0 {
        return;
    }
    let cur = F(f32::from_bits(m.r32(this + 0x38d4)));
    let diff = F(target) - cur;
    m.w32(this + 0x38c8, mode);
    m.w32(this + 0x38d0, target.to_bits());
    let step = (diff / F(secs)) / F(60.0);
    m.w32(this + 0x38cc, step.0.to_bits());
    if step.ee() != 0.0 {
        m.w32(this + 0x38c4, 1);
    }
}

/// `Audio_UpdateMusicFade` 0x212018(this): step the active music fade (`+0x38c4`) by `+0x38cc`, clamp at `+0x38d0`,
/// and multiply the channel volumes selected by fade mode `+0x38c8` (`0..=4`).
pub fn audio_update_music_fade(m: &mut Mem, this: u32) {
    if m.r32(this + 0x38c4) == 0 {
        return;
    }
    let cur = F(f32::from_bits(m.r32(this + 0x38d4)));
    let step = F(f32::from_bits(m.r32(this + 0x38cc)));
    let next = cur + step;
    m.w32(this + 0x38d4, next.0.to_bits());
    let target = F(f32::from_bits(m.r32(this + 0x38d0)));
    let done = if step.ee() < 0.0 { next.ee() < target.ee() } else { target.ee() < next.ee() };
    if done {
        m.w32(this + 0x38d4, target.0.to_bits());
        m.w32(this + 0x38c4, 0);
    }
    let mode = m.r32(this + 0x38c8);
    let f = F(f32::from_bits(m.r32(this + 0x38d4)));
    let fb = f.0.to_bits();
    let mut scale = |src: u32, dst: u32| {
        let v = F(f32::from_bits(m.r32(this + src))) * f;
        m.w32(this + dst, v.0.to_bits());
    };
    match mode {
        0 => {
            for (s, d) in [(0x3858, 0x385c), (0x3864, 0x3868), (0x3870, 0x3874), (0x3888, 0x388c)] {
                scale(s, d);
            }
            for o in [0x38ac, 0x38b0, 0x38b4, 0x38bc] {
                m.w32(this + o, fb);
            }
        }
        1 => {
            for (s, d) in [(0x3864, 0x3868), (0x3870, 0x3874), (0x387c, 0x3880), (0x3888, 0x388c)] {
                scale(s, d);
            }
            for o in [0x38b0, 0x38b4, 0x38b8, 0x38bc] {
                m.w32(this + o, fb);
            }
        }
        2 => {
            for (s, d) in [(0x3858, 0x385c), (0x387c, 0x3880), (0x3888, 0x388c), (0x3894, 0x3898)] {
                scale(s, d);
            }
            for o in [0x38ac, 0x38b8, 0x38bc, 0x38c0] {
                m.w32(this + o, fb);
            }
        }
        3 => {
            for (s, d) in [
                (0x3858, 0x385c),
                (0x3870, 0x3874),
                (0x387c, 0x3880),
                (0x3888, 0x388c),
                (0x3894, 0x3898),
            ] {
                scale(s, d);
            }
            for o in [0x38ac, 0x38b4, 0x38b8, 0x38bc, 0x38c0] {
                m.w32(this + o, fb);
            }
        }
        4 => {
            for (s, d) in [
                (0x3858, 0x385c),
                (0x3864, 0x3868),
                (0x387c, 0x3880),
                (0x3888, 0x388c),
                (0x3894, 0x3898),
            ] {
                scale(s, d);
            }
            for o in [0x38ac, 0x38b0, 0x38b8, 0x38bc, 0x38c0] {
                m.w32(this + o, fb);
            }
        }
        _ => {}
    }
}

/// `cBXAudio::vf1_212280` 0x212280(this, mode): forward to `cSoundSys::vf1`, recording `mode` at `+0x3838` when it
/// returns `2`.
pub fn bx_audio_vf1(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, mode: u32) -> u32 {
    let rc = sound_sys_vf1(m, env, this, mode);
    if rc == 2 {
        m.w32(this + 0x3838, mode);
    }
    rc
}

/// `Audio_FreeRaceStream` 0x2151c8(this): destroy stream slot 2, clear follow-stream `(0, 0)`, free `+0x38d8` via
/// `EAMem_Free` when non-null, and zero `+0x7448` and `+0x38d8`.
pub fn audio_free_race_stream(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    env.call(m, STREAM_SYS_DESTROY_STREAM, &[this, 2]);
    env.call(m, SPEECH_SYS_SET_FOLLOW_STREAM, &[this, 0, 0]);
    let p = m.r32(this + 0x38d8);
    if p != 0 {
        env.call(m, EAMEM_FREE, &[p]);
    }
    m.w32(this + 0x7448, 0);
    m.w32(this + 0x38d8, 0);
}

/// `Sfx_InstanceControllerVf26` 0x2168f8(_this, inst): when `*(inst+0xe4)` is non-null, invoke its vtable (`+8`)
/// entry `+0xdc` (this-adjust `+0xd8`).
pub fn sfx_instance_controller_vf26(m: &mut Mem, env: &mut dyn SoundMgrEnv, inst: u32) {
    let ctl = m.r32(inst + 0xe4);
    if ctl != 0 {
        let vt = m.r32(ctl + 8);
        let adj = m.r16(vt + 0xd8) as i16 as i32 as u32;
        let f = m.r32(vt + 0xdc);
        env.call(m, f, &[ctl.wrapping_add(adj)]);
    }
}

/// `Sfx_MeterFullEvent` 0x219258(this, rider): when `+0x6e3c == 0` and `*(rider+0x18)->+0x1c > 0.875`, call
/// `Speech_OnMeterFull(this, *(rider+0x18))`; always returns `1`.
pub fn sfx_meter_full_event(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, rider: u32) -> u32 {
    if m.r32(this + 0x6e3c) == 0 {
        let score_obj = m.r32(rider + 0x18);
        let meter = F(f32::from_bits(m.r32(score_obj + 0x1c))).ee();
        if 0.875f32 < meter {
            env.call(m, SPEECH_ON_METER_FULL, &[this, score_obj]);
        }
    }
    1
}

/// `Audio_ListBankNames` 0x21a498(this, out_buf): copy `*(this+0x38dc)` 64-byte bank names from `+0x38e0` into
/// `out_buf`, then format remaining entries up to `200` via `sprintf(dst, "Unused bank %d", i)`.
pub fn audio_list_bank_names(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, out_buf: u32) {
    let mut n = m.r32(this + 0x38dc) as i32;
    let mut i = 0i32;
    while i < n {
        libc::strcpy(
            m,
            out_buf.wrapping_add((i as u32) << 6),
            this.wrapping_add(0x38e0 + ((i as u32) << 6)),
        );
        i += 1;
        n = m.r32(this + 0x38dc) as i32;
    }
    let mut j = n;
    while j < 0xc8 {
        env.call(m, SPRINTF, &[out_buf.wrapping_add((j as u32) << 6), STR_UNUSED_BANK, j as u32]);
        j += 1;
    }
}

/// `cBXAudio::vf2_21bc60` 0x21bc60(): empty virtual stub.
pub fn bx_audio_vf2() {}

/// `Audio_LoadFEGlobalBankList` 0x21ce78(this, path, sp): open INI `path` with mode `0x10`, find `[GLOBAL]`, copy
/// every string value into `this + 0x6e9c + idx * 0x40` (incrementing `this + 0x6e98`), and close the reader.
pub fn audio_load_fe_global_bank_list(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, path: u32, sp: u32) {
    let reader = sp.wrapping_add(0x40);
    env.call(m, INI_OPEN, &[reader, path, 0x10]);
    if env.call(m, INI_FIND_SECTION, &[reader, STR_GLOBAL]) != 0 {
        while env.call(m, INI_AT_SECTION_END, &[reader]) == 0 {
            let tok = env.call(m, INI_READ_TOKEN, &[reader]);
            env.call(m, INI_READ_STRING, &[reader, tok, sp]);
            let idx = m.r32(this + 0x6e98);
            m.w32(this + 0x6e98, idx.wrapping_add(1));
            libc::strcpy(m, this.wrapping_add(0x6e9c + (idx << 6)), sp);
        }
    }
    env.call(m, INI_CLOSE, &[reader, 2]);
}

fn audio_music_state_to_mode(state: u32) -> u32 {
    match state {
        0 | 1 => 3,
        2 => 1,
        3 => 4,
        _ => 5,
    }
}

/// `Audio_SetMusicState` 0x21d610(this, state, param): for state `4`, if `+0x7444 != 0`, fade music to `1.0` over
/// `1.0s` in the mode mapped from the previous state (`Audio_MusicStateToMode` `0x20d9f0`) and clear `+0x7444`;
/// otherwise record `param` at `+0x7440`; always store `state` at `+0x743c`.
pub fn audio_set_music_state(m: &mut Mem, this: u32, state: u32, param: f32) {
    if state == 4 {
        if m.r32(this + 0x7444) != 0 {
            let prev = m.r32(this + 0x743c);
            let mode = audio_music_state_to_mode(prev);
            audio_fade_music_to(m, this, mode, 1.0, 1.0);
            m.w32(this + 0x7444, 0);
        }
        m.w32(this + 0x743c, state);
    } else {
        m.w32(this + 0x7440, param.to_bits());
        m.w32(this + 0x743c, state);
    }
}

/// `Audio_ScheduleWorldTriggersOn` 0x21e498(this): schedule callback `0x0021_e4c0_ffff_0000` in 250 ms via
/// `SndSys_ScheduleEvent`.
pub fn audio_schedule_world_triggers_on(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) -> u32 {
    env.call(
        m,
        SND_SYS_SCHEDULE_EVENT,
        &[
            this,
            0xfa,
            CB_WORLD_TRIGGERS_ON as u32,
            (CB_WORLD_TRIGGERS_ON >> 32) as u32,
            0,
            0,
            0,
            0,
            0,
        ],
    )
}

/// `Audio_GetMusicFxThunk` 0x21e690(this): forward to `Audio_GetMusicFx` (`0x21e688`), returning `*(this+0x3844)`.
pub fn audio_get_music_fx_thunk(m: &Mem, this: u32) -> u32 {
    m.r32(this + 0x3844)
}

// ---- Final 9 audio functions (row G10v, bringing `audio` to 0 `not started`) ----

pub const SND_SYS_STOP_BANK_SOUND: u32 = 0x20c2a0;
pub const SFX_EVAL_CURVE: u32 = 0x20d928;
pub const SFX_SURFACE_SOUND_TYPE: u32 = 0x20f908;
pub const AUDIO_FIND_RIDER_NEAR: u32 = 0x20f9b0;
pub const AUDIO_ANY_RIDER_NEAR_AHEAD: u32 = 0x20fa80;
pub const AUDIO_IS_NEARER_PLAYER2: u32 = 0x20fb68;
pub const AUDIO_GET_RIDER_LISTENER: u32 = 0x20fc60;
pub const SND_SYS_IS_QUEUED: u32 = 0x2154e0;
pub const SND_SYS_QUEUE_SOUNDS: u32 = 0x215998;
pub const SFX_IMPACT_SPEED: u32 = 0x21a6a8;
pub const MUSIC_CANCEL_TRICKY: u32 = 0x21c6c0;
pub const SND_SYS_SET_PENDING_VOLUME: u32 = 0x226dd8;
pub const SND_LOCK: u32 = 0x2d0908;
pub const SND_UNLOCK: u32 = 0x2d0938;
pub const SND_HANDLE_IS_DEAD: u32 = 0x2d3338;

fn ring_pop_word(m: &mut Mem, q: u32) -> u32 {
    let w = m.r32(q + 8);
    let r = m.r32(q + 0xc);
    if w == r {
        return 0xffff_ffff;
    }
    let buf = m.r32(q + 4);
    let val = m.r32(buf.wrapping_add(r << 2));
    let mut nr = (r as i32).wrapping_add(1);
    m.w32(q + 0xc, nr as u32);
    let cap = m.r32(q) as i32;
    if nr >= cap {
        nr = 0;
        m.w32(q + 0xc, 0);
    }
    let _ = nr;
    val
}

fn ring_push_word(m: &mut Mem, q: u32, val: u32) {
    let w = m.r32(q + 8) as i32;
    let cap = m.r32(q) as i32;
    let r = m.r32(q + 0xc) as i32;
    if (w.wrapping_add(1)) % cap == r {
        return;
    }
    let buf = m.r32(q + 4);
    m.w32(buf.wrapping_add((w as u32) << 2), val);
    let nw = (m.r32(q + 8) as i32).wrapping_add(1);
    m.w32(q + 8, nw as u32);
    let cap2 = m.r32(q) as i32;
    if nw >= cap2 {
        m.w32(q + 8, 0);
    }
}

/// `Audio_StopQueuedSounds` 0x215c28(this, key): drain the `+0x6e24` ring queue for `count = (w - r + cap) % cap`
/// iterations, popping 4 words `(k, handle, expiry, flag)` per iteration; entries with `k == key` stop `handle`
/// (`SndSys_StopBankSound(this, handle, 1, 0.0)`) when `flag != 0`, and non-matching entries are re-enqueued.
pub fn audio_stop_queued_sounds(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, key: u32) {
    let q = this.wrapping_add(0x6e24);
    let w = m.r32(q + 8);
    let r = m.r32(q + 0xc);
    let cap = m.r32(q) as i32;
    let n = (w.wrapping_sub(r).wrapping_add(cap as u32) as i32) % cap;
    if n <= 0 {
        return;
    }
    for _ in 0..n {
        let w0 = ring_pop_word(m, q);
        let w1 = ring_pop_word(m, q);
        let w2 = ring_pop_word(m, q);
        let w3 = ring_pop_word(m, q);
        if w0 == key {
            if w3 != 0 {
                env.call(m, SND_SYS_STOP_BANK_SOUND, &[this, w1, 1, 0.0f32.to_bits()]);
            }
        } else {
            ring_push_word(m, q, w0);
            ring_push_word(m, q, w1);
            ring_push_word(m, q, w2);
            ring_push_word(m, q, w3);
        }
    }
}

fn clamp_slot_vol_byte(m: &mut Mem, this: u32, vol: i32) {
    let idx = m.r32(this + 0x20);
    let v3c = m.r32(this + 0x3c);
    m.w32(v3c.wrapping_add(idx << 2), vol as u32);
    let stored = m.r32(v3c.wrapping_add(idx << 2)) as i32;
    let clamped = stored.clamp(0, 0x7f) as u8;
    let rec = m.r32(this + 0x24).wrapping_add(idx.wrapping_mul(0x18));
    m.w8(rec, clamped);
}

/// `Sfx_PlayRiderBump` 0x215f18(this, rider1, rider2): if `rider1+0x140` is near (<40), ahead, and not already
/// queued, compute the 4D length of `rider1+0x150 + rider2+0x150`, evaluate `Sfx_EvalCurve(3, speed)`, push sound
/// `(0, 0x32)` with 40 m pending volume, play it, and queue `(rider1, handle, tick+0x32, 0)`.
pub fn sfx_play_rider_bump(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, rider1: u32, rider2: u32) {
    let pos1 = rider1.wrapping_add(0x140);
    let forty = 40.0f32.to_bits();
    if (env.call(m, AUDIO_FIND_RIDER_NEAR, &[this, pos1, forty]) as i32) < 0 {
        return;
    }
    if env.call(m, AUDIO_ANY_RIDER_NEAR_AHEAD, &[this, pos1, forty]) == 0 {
        return;
    }
    if env.call(m, SND_SYS_IS_QUEUED, &[this, rider1]) != 0 {
        return;
    }
    let mut sum = [F(0.0); 4];
    for (k, s) in sum.iter_mut().enumerate() {
        let a = F(f32::from_bits(m.r32(rider1 + 0x150 + (k as u32) * 4)));
        let b = F(f32::from_bits(m.r32(rider2 + 0x150 + (k as u32) * 4)));
        *s = a + b;
    }
    let sq0 = sum[0] * sum[0];
    let sq1 = sum[1] * sum[1];
    let sq2 = sum[2] * sum[2];
    let sq3 = sum[3] * sum[3];
    let len = (((sq0 + sq1) + sq2) + sq3).sqrt();
    let speed = len.ee() as i32 as u32;
    let vol = env.call(m, SFX_EVAL_CURVE, &[3, speed]) as i32;

    snd_sys_push_entry_copy2(m, this);
    let idx = m.r32(this + 0x20);
    let a40 = m.r32(this + 0xa40);
    m.w32(a40.wrapping_add(idx << 2), 0);
    let a44 = m.r32(this + 0xa44);
    m.w32(a44.wrapping_add(idx << 2), 0x32);
    clamp_slot_vol_byte(m, this, vol);
    let slot = env.call(m, SND_MGR_GET_SLOT, &[this, 4]);
    let idx2 = m.r32(this + 0x20);
    let v40 = m.r32(this + 0x40);
    m.w32(v40.wrapping_add(idx2 << 2), slot);
    env.call(m, SND_SYS_SET_PENDING_VOLUME, &[this, forty, (-1.0f32).to_bits()]);
    let nearer = env.call(m, AUDIO_IS_NEARER_PLAYER2, &[this, pos1]);
    let idx3 = m.r32(this + 0x20);
    let v68 = m.r32(this + 0x68);
    m.w32(v68.wrapping_add(idx3 << 2), nearer);
    m.w32(this + 0x28 + (idx3 << 2), pos1);
    let handle = env.call(m, SND_SYS_PLAY_PENDING, &[this]);
    let expiry = m.r32(this + 0x80).wrapping_add(0x32);
    env.call(m, SND_SYS_QUEUE_SOUNDS, &[this, rider1, handle, expiry, 0]);
}

fn eval_takeoff_speed(m: &mut Mem, env: &mut dyn SoundMgrEnv, rider: u32) -> i32 {
    let v = env.call(m, SFX_IMPACT_SPEED, &[rider]) as i32;
    let q = v.wrapping_mul(127) / 1000;
    F(q as f32).ee() as i32
}

fn clamp_takeoff_inner(m: &mut Mem, env: &mut dyn SoundMgrEnv, rider: u32) -> i32 {
    if eval_takeoff_speed(m, env, rider) < 0x7f && eval_takeoff_speed(m, env, rider) < 0 {
        return 0;
    }
    if eval_takeoff_speed(m, env, rider) < 0x7f {
        eval_takeoff_speed(m, env, rider)
    } else {
        0x7f
    }
}

/// `Sfx_PlayTakeoff` 0x216d58(this, rider): for the player's rider (`+0x41c == 1`), evaluate the nested clamp of
/// `Sfx_ImpactSpeed(rider) * 127 / 1000` into `[64, 127]` (preserving every macro re-evaluation of
/// `Sfx_ImpactSpeed`), push sound `(1, surface * 8 + 2)` with volume/slot/emitter/listener, and play pending.
pub fn sfx_play_takeoff(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, rider: u32) -> u32 {
    if m.r32(rider + 0x41c) != 1 {
        return 0;
    }
    let vol = if clamp_takeoff_inner(m, env, rider) < 0x7f && clamp_takeoff_inner(m, env, rider) < 0x40 {
        0x40
    } else if clamp_takeoff_inner(m, env, rider) < 0x7f {
        clamp_takeoff_inner(m, env, rider)
    } else {
        0x7f
    };
    let surf = env.call(m, SFX_SURFACE_SOUND_TYPE, &[this, rider]);
    let snd_id = (surf << 3).wrapping_add(2);
    snd_sys_push_entry_copy2(m, this);
    let idx = m.r32(this + 0x20);
    let a40 = m.r32(this + 0xa40);
    m.w32(a40.wrapping_add(idx << 2), 1);
    let a44 = m.r32(this + 0xa44);
    m.w32(a44.wrapping_add(idx << 2), snd_id);
    clamp_slot_vol_byte(m, this, vol);
    let slot = env.call(m, SND_MGR_GET_SLOT, &[this, 4]);
    let idx2 = m.r32(this + 0x20);
    let v40 = m.r32(this + 0x40);
    m.w32(v40.wrapping_add(idx2 << 2), slot);
    m.w32(this + 0x28 + (idx2 << 2), rider.wrapping_add(0x140));
    let listener = env.call(m, AUDIO_GET_RIDER_LISTENER, &[this, rider]);
    let idx3 = m.r32(this + 0x20);
    let v68 = m.r32(this + 0x68);
    m.w32(v68.wrapping_add(idx3 << 2), listener);
    env.call(m, SND_SYS_PLAY_PENDING, &[this])
}

fn sfx_play_clipboard_sound(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, rider: u32, id: u32) -> u32 {
    if m.r32(rider + 0x41c) != 1 {
        return 0;
    }
    snd_sys_push_entry_copy2(m, this);
    let idx = m.r32(this + 0x20);
    let a40 = m.r32(this + 0xa40);
    m.w32(a40.wrapping_add(idx << 2), 1);
    let a44 = m.r32(this + 0xa44);
    m.w32(a44.wrapping_add(idx << 2), id);
    clamp_slot_vol_byte(m, this, 0x7f);
    let slot = env.call(m, SND_MGR_GET_SLOT, &[this, 4]);
    let idx2 = m.r32(this + 0x20);
    let v40 = m.r32(this + 0x40);
    m.w32(v40.wrapping_add(idx2 << 2), slot);
    m.w32(this + 0x28 + (idx2 << 2), rider.wrapping_add(0x140));
    let listener = env.call(m, AUDIO_GET_RIDER_LISTENER, &[this, rider]);
    let idx3 = m.r32(this + 0x20);
    let v68 = m.r32(this + 0x68);
    m.w32(v68.wrapping_add(idx3 << 2), listener);
    env.call(m, SND_SYS_PLAY_PENDING, &[this])
}

/// `Sfx_PlayClipBoardSound51` 0x21d680(this, rider): for player rider (`+0x41c == 1`), play sound `(1, 0x51)` at
/// volume `0x7f` on slot 4 with emitter `rider+0x140` and `Audio_GetRiderListener`.
pub fn sfx_play_clipboard_sound_51(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, rider: u32) -> u32 {
    sfx_play_clipboard_sound(m, env, this, rider, 0x51)
}

/// `Sfx_PlayClipBoardSound50` 0x21d9b0(this, rider): for player rider (`+0x41c == 1`), play sound `(1, 0x50)` at
/// volume `0x7f` on slot 4 with emitter `rider+0x140` and `Audio_GetRiderListener`.
pub fn sfx_play_clipboard_sound_50(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, rider: u32) -> u32 {
    sfx_play_clipboard_sound(m, env, this, rider, 0x50)
}

fn stop_meter_cue_if_alive(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    env.call(m, SND_LOCK, &[]);
    let h = m.r32(this + 0x7458);
    let dead = env.call(m, SND_HANDLE_IS_DEAD, &[h]);
    env.call(m, SND_UNLOCK, &[]);
    if dead == 0 {
        let h2 = m.r32(this + 0x7458);
        env.call(m, SND_SYS_STOP_BANK_SOUND, &[this, h2, 1, 0.0f32.to_bits()]);
    }
}

fn play_meter_cue_entry(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32, id: u32) -> u32 {
    snd_sys_push_entry_copy2(m, this);
    let idx = m.r32(this + 0x20);
    let a40 = m.r32(this + 0xa40);
    m.w32(a40.wrapping_add(idx << 2), 5);
    let a44 = m.r32(this + 0xa44);
    m.w32(a44.wrapping_add(idx << 2), id);
    let slot = env.call(m, SND_MGR_GET_SLOT, &[this, 6]);
    let idx2 = m.r32(this + 0x20);
    let v40 = m.r32(this + 0x40);
    m.w32(v40.wrapping_add(idx2 << 2), slot);
    env.call(m, SND_SYS_PLAY_PENDING, &[this])
}

/// `Music_PlayMeterCue1` 0x21ddc0(this): play bank-5 cue `1` on volume slot 6 and store its handle at `+0x7458`.
pub fn music_play_meter_cue1(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    let h = play_meter_cue_entry(m, env, this, 1);
    m.w32(this + 0x7458, h);
}

/// `Music_PlayMeterCue2` 0x21dfe8(this): stop the active `+0x7458` handle if alive, then play bank-5 cue `2` on slot 6.
pub fn music_play_meter_cue2(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) -> u32 {
    stop_meter_cue_if_alive(m, env, this);
    play_meter_cue_entry(m, env, this, 2)
}

/// `Music_RestartMeterCue` 0x21e248(this): stop the active `+0x7458` handle if alive, then play bank-5 cue `0` on slot 6.
pub fn music_restart_meter_cue(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) -> u32 {
    stop_meter_cue_if_alive(m, env, this);
    play_meter_cue_entry(m, env, this, 0)
}

/// `Music_UpdateMeterCues` 0x21c6d0(this): cancel Tricky music if `+0x3708 != 0` and `*(+0x6e7c)->+0x24 <= 0.0`,
/// then advance the `+0x6e80` rider's meter cue flags (`+0x6e90` restart on `+0x58a8 > +0x6e94`, `+0x6e88` cue 2 on
/// `+0x24 <= 0.0`, `+0x6e8c` cue 1 on `+0x24 < 2.7`).
pub fn music_update_meter_cues(m: &mut Mem, env: &mut dyn SoundMgrEnv, this: u32) {
    if m.r32(this + 0x3708) != 0 {
        let obj = m.r32(this + 0x6e7c);
        if F(f32::from_bits(m.r32(obj + 0x24))).ee() <= 0.0 {
            env.call(m, MUSIC_CANCEL_TRICKY, &[this]);
        }
    }
    let rider = m.r32(this + 0x6e80);
    if rider == 0 {
        return;
    }
    if m.r32(this + 0x6e90) != 0 {
        let cnt = m.r32(rider + 0x58a8) as i32;
        if (m.r32(this + 0x6e94) as i32) < cnt {
            env.call(m, MUSIC_CANCEL_TRICKY, &[this]);
            music_restart_meter_cue(m, env, this);
            let r2 = m.r32(this + 0x6e80);
            m.w32(this + 0x6e8c, 0);
            m.w32(this + 0x6e88, 0);
            m.w32(this + 0x6e94, m.r32(r2 + 0x58a8));
            m.w32(this + 0x6e90, 0);
            m.w32(this + 0x6e80, 0);
        }
    }
    if m.r32(this + 0x6e88) != 0 {
        let r2 = m.r32(this + 0x6e80);
        if F(f32::from_bits(m.r32(r2 + 0x24))).ee() <= 0.0 {
            music_play_meter_cue2(m, env, this);
            m.w32(this + 0x6e8c, 0);
            m.w32(this + 0x6e88, 0);
            m.w32(this + 0x6e80, 0);
        }
    }
    if m.r32(this + 0x6e8c) != 0 {
        let r2 = m.r32(this + 0x6e80);
        if F(f32::from_bits(m.r32(r2 + 0x24))).ee() < f32::from_bits(0x402c_cccd) {
            env.call(m, MUSIC_CANCEL_TRICKY, &[this]);
            music_play_meter_cue1(m, env, this);
            m.w32(this + 0x6e8c, 0);
        }
    }
}
