//! The course's own collision for the port (board row E7c3): the world the casts run on ([`World::from_course`],
//! from the course's `.pbd`, `.ltg` and `.ssf`) and its patch cache (`World_LookupCollCache`: a patch tessellated
//! the first time a query needs it, `terrain::PatchGrid`). `World_RayCast` 0x25aff8 on it is what the ground
//! probe and the other casts ask, in the game's units (cm, z up).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::terrain::{LastHit, PatchGrid};
use crate::world::{CastHit, Shape, World, WorldEnv};
use crate::worldline::Hit;

type V4 = [f32; 4];

/// The course's world and its patch cache.
pub struct CourseGround {
    /// the world, whose objects' flags +0xe8 bit 0x20 (solid) the port switches (see [`CourseGround::ray_cast`])
    world: Mutex<World>,
    /// each object's flags (+0xe8): as the course loaded them, then as its controllers set them
    /// ([`CourseGround::set_flags`])
    flags: Mutex<Vec<u32>>,
    /// initial `.ssf` / `.pbd` flags per object, restored by `World_ResetInstances` 0x115fe8 (`CourseGround::reset_cells`)
    init_flags: Vec<u32>,
    /// each patch's coefficients (+0x50)
    coeffs: Vec<[V4; 16]>,
    /// STANDIN: every patch tessellated stays (about 7 KB each), for the game's fixed-size cache and its eviction
    /// (`World_LookupCollCache`), which only change how fast a cast is, not what it hits
    cache: Mutex<HashMap<usize, PatchGrid>>,
    /// how a patch's points are made: `terrain::tessellate` (to nearest) unless [`CourseGround::tessellate_with`]
    /// set another rule
    points: Mutex<fn(&[V4; 16]) -> [V4; 100]>,
    /// the keyframed objects' animated parts' frames as their clocks stand ([`CourseGround::pose`])
    poses: Mutex<HashMap<usize, Vec<[V4; 4]>>>,
    /// the course's `.ssf` (its scripts, physics records and sphere trees)
    pub ssf: tricky_data::ssf::Ssf,
    /// the knockable objects that are moving and the script threads the riders' contacts started (row E8c3)
    pub knock: Mutex<crate::knockables::Knockables>,
    /// live world cells (`AiCells` 0x115e28), object controllers (`cAnimObjectNode`, `cAnimDeltaNode`,
    /// `controllers::Controller`), and persistent/contact scripts (row E7b2)
    pub cells: Mutex<crate::cellhost::CellHost>,
    /// the world grid as `Rail_ClosestPoint` 0x259330 walks it (row G20c2c1a, [`rail_world`]); its rails' on bits
    /// as the course's scripts leave them ([`CourseGround::set_rail_live`])
    pub rails: Mutex<crate::rider::ground_tick::RailWorld>,
    /// Keyframed animated objects (`cAnimObjectNode` 0x100/0x102/0x103, `cAnimDeltaNode` 0x101) whose persistent
    /// script spawns an animation controller: `World_QueryInstances` skips them only while unposed (`!poses.contains_key`),
    /// whereas non-keyframed controllers (`cUVScrollNode` 10 on water/river OOB boxes, `cDebounceNode` 2, …) stay
    /// active in `World_QueryInstances` whenever `flags & 0x20 != 0`.
    driven: Vec<bool>,
    /// gWorld (0x346488) +0xc: the `.ltg` header's box, its lowest z (cm; `ResolveTGroupHeader` 0x252470 leaves it
    /// as the file has it), the floor `cBoarder::vf7` 0x117198 resets riders 100 m under
    pub floor: f32,
}

struct Env<'a> {
    coeffs: &'a [[V4; 16]],
    cache: &'a mut HashMap<usize, PatchGrid>,
    poses: &'a HashMap<usize, Vec<[V4; 4]>>,
    points: fn(&[V4; 16]) -> [V4; 100],
}

impl WorldEnv for Env<'_> {
    /// The shape's +0x48 on the patch's cache entry; `last` notes the patch as its index + 1 (the game notes the
    /// patch's address, never 0, which a cleared record holds).
    fn patch(&mut self, q: &dyn Shape, i: usize, last: Option<&mut LastHit>) -> Option<Hit> {
        let (coeffs, points) = (self.coeffs, self.points);
        let grid = self.cache.entry(i).or_insert_with(|| PatchGrid::with_points(&coeffs[i], points(&coeffs[i])));
        q.patch(grid, i as u32 + 1, last).map(|h| h.hit)
    }
    /// `MeshAnimNode_GetFrameMatrix` 0x1998d0: the frame [`CourseGround::pose`] last gave (the identity for none).
    fn frame(&mut self, object: usize, n: usize) -> [V4; 4] {
        self.poses.get(&object).and_then(|f| f.get(n)).copied().unwrap_or([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }
}

impl CourseGround {
    /// From the course's files (`data/models/<course>.pbd`, `.ltg`, `.ssf`); `None` when one does not parse.
    pub fn new(pbd_bytes: &[u8], ltg_bytes: &[u8], ssf_bytes: &[u8]) -> Option<CourseGround> {
        let pbd = tricky_data::pbd::Pbd::parse(pbd_bytes)?;
        let ltg = tricky_data::ltg::Ltg::parse(ltg_bytes)?;
        let ssf = tricky_data::ssf::Ssf::parse(ssf_bytes, pbd.instances.len() as u32).ok()?;
        let coeffs = pbd.patches.iter().map(|r| tricky_data::pbd::PatchRec::parse(r).map_or([[0.0; 4]; 16], |p| p.coeffs)).collect();
        let world = World::from_course(pbd_bytes, &pbd, ltg_bytes, &ltg, &ssf);
        let init_flags: Vec<u32> = world.objects.iter().map(|o| o.flags).collect();
        let flags = Mutex::new(init_flags.clone());
        let knock = Mutex::new(crate::knockables::Knockables::default());
        let cells = Mutex::new(crate::cellhost::CellHost::new(pbd_bytes, &pbd, world.objects.len()));
        let driven = (0..world.objects.len())
            .map(|o| {
                ssf.effects
                    .get(ssf.event_script(o, 0) as usize)
                    .is_some_and(|ops| ops.iter().any(|op| op.op == 0 && op.args.first().is_some_and(|&t| matches!(t, 0x100..=0x103))))
            })
            .collect();
        // STANDIN: every rail on at load, as the port's `Rails` are (the game's are off in the file and their
        // `cRailNode`s switch them on, row E7b2); the event's scripts switch some off ([`CourseGround::set_rail_live`])
        let mut rails = rail_world(&world, ltg_bytes, &ltg, &pbd);
        for k in 0..pbd.splines.len() as u32 {
            rails.set_live(k, true);
        }
        let rails = Mutex::new(rails);
        let floor = ltg.bbox.min[2];
        Some(CourseGround { world: Mutex::new(world), flags, init_flags, coeffs, cache: Mutex::new(HashMap::new()), points: Mutex::new(crate::terrain::tessellate), poses: Mutex::new(HashMap::new()), ssf, knock, cells, driven, rails, floor })
    }

    /// The patches' points made by `points` from now on (the cache emptied). In PCSX2 the patch program
    /// (`Vu0_RunPatchProgram` 0x2576a8) rounds toward zero (F4h5m): tests replaying a PCSX2 recording ask for that
    /// (`tests/replay.rs`, F4b3d4); which rule the app's courses take waits on F4c (row F4h5n).
    pub fn tessellate_with(&self, points: fn(&[V4; 16]) -> [V4; 100]) {
        *self.points.lock().unwrap_or_else(|e| e.into_inner()) = points;
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// A rail (the course's spline `spline`) switched on or off (`RailWorld::set_live`)
    pub fn set_rail_live(&self, spline: u32, on: bool) {
        self.rails.lock().unwrap_or_else(|e| e.into_inner()).set_live(spline, on);
    }

    /// `World_RayCast` 0x25aff8 on the course: the distance along the query (−1 for nothing) and what was hit.
    /// `show_off` for the show-off modes (3, 5); `last` the caller's last-patch record.
    ///
    /// STANDIN: `skip(instance)` clears a solid object's bit 0x20 for this cast (the port's switched-off, broken
    /// and opened objects), for the game's own switching of an instance (scripts, glass, doors; row E9b2).
    pub fn ray_cast(&self, q: &mut dyn Shape, show_off: bool, last: Option<&mut LastHit>, skip: &dyn Fn(u32) -> bool) -> (f32, Option<CastHit>) {
        let mut world = self.world.lock().unwrap_or_else(|e| e.into_inner());
        for (i, (o, f)) in world.objects.iter_mut().zip(self.flags.lock().unwrap_or_else(|e| e.into_inner()).iter()).enumerate() {
            o.flags = if f & 0x20 != 0 && skip(i as u32) { f & !0x20 } else { *f };
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let poses = self.poses.lock().unwrap_or_else(|e| e.into_inner());
        let mut env = Env { coeffs: &self.coeffs, cache: &mut cache, poses: &poses, points: *self.points.lock().unwrap_or_else(|e| e.into_inner()) };
        world.ray_cast(q, show_off, last, &mut env)
    }

    /// `World_QueryInstances` 0x25b878 on the course: the objects only, the deepest touch (−1 for nothing).
    /// `skip` as [`CourseGround::ray_cast`]'s.
    pub fn query_instances(&self, q: &mut dyn Shape, show_off: bool, skip: &dyn Fn(u32) -> bool) -> (f32, Option<CastHit>) {
        let mut world = self.world.lock().unwrap_or_else(|e| e.into_inner());
        let poses = self.poses.lock().unwrap_or_else(|e| e.into_inner());
        for (i, (o, f)) in world.objects.iter_mut().zip(self.flags.lock().unwrap_or_else(|e| e.into_inner()).iter()).enumerate() {
            let skip = skip(i as u32) || self.driven[i] && !poses.contains_key(&i);
            o.flags = if f & 0x20 != 0 && skip { f & !0x20 } else { *f };
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut env = Env { coeffs: &self.coeffs, cache: &mut cache, poses: &poses, points: *self.points.lock().unwrap_or_else(|e| e.into_inner()) };
        world.query_instances(q, show_off, &mut env)
    }

    /// An object's flags (+0xe8) as they stand (0 for none).
    pub fn object_flags(&self, object: usize) -> u32 {
        self.flags.lock().unwrap_or_else(|e| e.into_inner()).get(object).copied().unwrap_or(0)
    }

    /// An object's flags (+0xe8) set (by its controller).
    pub fn set_flags(&self, object: usize, flags: u32) {
        if let Some(f) = self.flags.lock().unwrap_or_else(|e| e.into_inner()).get_mut(object) {
            *f = flags;
        }
    }

    /// Copy of all object flags (+0xe8), used by `CellHost` without holding nested locks.
    pub fn snapshot_flags(&self) -> Vec<u32> {
        self.flags.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Replace all object flags (+0xe8) after a `CellHost` step.
    pub fn replace_flags(&self, flags: &[u32]) {
        let mut f = self.flags.lock().unwrap_or_else(|e| e.into_inner());
        let n = f.len().min(flags.len());
        f[..n].copy_from_slice(&flags[..n]);
    }

    /// Attach or detach a controller type (`+0x14`) on `world.objects[object]` without changing poses.
    pub fn set_controller(&self, object: u32, kind: Option<u32>) {
        let mut world = self.world.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(obj) = world.objects.get_mut(object as usize) {
            obj.controller = kind;
        }
    }

    /// Controller type (`+0x14`) currently attached to `object` (roller `0` or `CellHost` controller).
    pub fn controller_type(&self, object: usize) -> Option<u32> {
        if self.knock.lock().unwrap_or_else(|e| e.into_inner()).rollers.iter().any(|r| r.object as usize == object) {
            return Some(0);
        }
        self.cells.lock().unwrap_or_else(|e| e.into_inner()).controller(object)
    }

    /// Active clip time (seconds) for a keyframed object driven by `CellHost` (`cAnimObjectNode` / `cAnimDeltaNode`).
    pub fn anim_time(&self, object: usize) -> Option<f32> {
        self.cells.lock().unwrap_or_else(|e| e.into_inner()).anim_time(object)
    }

    /// Whether `object` currently has a controller blocking contact scripts (`Instance_OnPlayerContact` 0x13bd40).
    pub fn blocks_contact(&self, object: usize) -> bool {
        self.cells.lock().unwrap_or_else(|e| e.into_inner()).blocks_contact(object)
    }

    /// `Instance_OnPlayerContact` 0x13bd40 + controller `vf +0x94` (`cAnimObjectNode::OnContact` 0x198eb8,
    /// `cUVScrollNode::OnContact` 0x142690, `cTexFlipNode::OnContact` 0x143378): returns `true` when the object's
    /// event-1 script should run and arms the controller's contact cooldown.
    pub fn try_contact(&self, object: usize) -> bool {
        self.cells.lock().unwrap_or_else(|e| e.into_inner()).try_contact(object)
    }

    /// Queue a `TriggerScript_ExecOp` 0x13bfd0 op 13 (`0x13c3b4`: `Boarder_RequestReset`) for `rider` (`slot + 1`).
    pub fn queue_rider_reset(&self, rider: u32) {
        if rider != 0 {
            self.knock.lock().unwrap_or_else(|e| e.into_inner()).resets.push(rider);
        }
    }

    /// Consume any pending `TriggerScript_ExecOp` 0x13bfd0 op 13 (`0x13c3b4`: `Boarder_RequestReset`) queued for
    /// `rider` by a synchronous or delayed `.ssf` contact script thread.
    pub fn take_rider_reset(&self, rider: u32) -> bool {
        let mut k = self.knock.lock().unwrap_or_else(|e| e.into_inner());
        let before = k.resets.len();
        k.resets.retain(|&id| id != rider);
        k.resets.len() != before
    }

    /// `World_FindScript` 0x260dd8: case-insensitive function index lookup in `self.ssf`.
    pub fn find_script(&self, name: &str) -> Option<u32> {
        crate::cellhost::find_script(&self.ssf, name)
    }

    /// `World_DeleteScriptObject` 0x13bbc0: delete the first live List-3 `cScriptNode` running function `fn_idx`.
    pub fn delete_script_object(&self, fn_idx: u32) -> bool {
        self.cells.lock().unwrap_or_else(|e| e.into_inner()).delete_script_object(fn_idx)
    }

    /// `World_RunScript` 0x13bad8: run a named `.ssf` script function (`RaceMode`, `ShowoffMode`, `FreerideMode`,
    /// `StartCountdown`, `EndCountdown`, `NoCountdown`, …) as a live List-3 `cScriptNode` (row E9b2).
    pub fn run_script_named(&self, name: &str, rand_game: &mut dyn FnMut() -> u32) -> crate::cellhost::CellEvents {
        self.cells
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .run_script_named(self, name, rand_game)
    }

    /// Run an object's `.ssf` event-slot script (`0` persistent, `1` contact, `3` finish, `4` trigger goal) on
    /// `CellHost`'s live `cScriptNode` runner (row E9b2).
    pub fn run_event_script(
        &self,
        object: usize,
        event: usize,
        rider: u32,
        vel: V4,
        human: bool,
        rand_game: &mut dyn FnMut() -> u32,
    ) -> crate::cellhost::CellEvents {
        self.cells
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .run_event_script(self, object, event, rider, vel, human, rand_game)
    }

    /// Advance `AiCells::tick` (`cAIWorld::vf1` 0x115e28) and all live List-3 controllers & script threads by one
    /// 60 Hz frame (rows E7b2, E9b2).
    pub fn tick_cells(
        &self,
        riders: &[V4],
        race_1c: i32,
        camera: V4,
        show_off: bool,
        rand_game: &mut dyn FnMut() -> u32,
    ) -> crate::cellhost::CellEvents {
        self.cells
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .tick(self, riders, race_1c, camera, show_off, rand_game)
    }

    /// `AIWorld_ClearSlots` 0x115df0 + `World_ResetInstances` 0x115fe8: drop all active cells, controllers, rollers,
    /// poses, and restore every object's initial `.ssf` flags.
    pub fn reset_cells(&self) {
        self.cells.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *self.knock.lock().unwrap_or_else(|e| e.into_inner()) = crate::knockables::Knockables::default();
        self.replace_flags(&self.init_flags);
        self.poses.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let mut world = self.world.lock().unwrap_or_else(|e| e.into_inner());
        for (obj, &f) in world.objects.iter_mut().zip(&self.init_flags) {
            obj.flags = f;
            obj.controller = None;
        }
    }

    /// `WorldNode_RayCast` 0x13aad0 = `World_RayCastInstance` 0x25bf48 on the course for one object (instance):
    /// the distance along the query (> 0 when it hits) and what was hit.
    pub fn ray_cast_instance(&self, q: &mut dyn Shape, object: usize) -> (f32, Option<CastHit>) {
        let world = self.world.lock().unwrap_or_else(|e| e.into_inner());
        if object >= world.objects.len() {
            return (-1.0, None);
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let poses = self.poses.lock().unwrap_or_else(|e| e.into_inner());
        let mut env = Env { coeffs: &self.coeffs, cache: &mut cache, poses: &poses, points: *self.points.lock().unwrap_or_else(|e| e.into_inner()) };
        world.ray_cast_instance(q, object, &mut env)
    }

    /// The world as it stands (for tests and tools).
    pub fn with_world<R>(&self, f: impl FnOnce(&World) -> R) -> R {
        f(&self.world.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// A keyframed object's (`cAnimObjectNode` / `cAnimDeltaNode`) animated parts posed at `frames` with controller
    /// type `kind` (`0x100`..`0x103`, `MeshAnimNode_GetFrameMatrix` 0x1998d0).
    /// The object's box (+0xcc) stays the file's: the game never moves it (row E7a6, PINE on gari).
    pub fn pose_with_controller(&self, object: u32, kind: u32, frames: Vec<[V4; 4]>) {
        let o = object as usize;
        let mut world = self.world.lock().unwrap_or_else(|e| e.into_inner());
        let Some(obj) = world.objects.get_mut(o) else { return };
        obj.controller = Some(kind);
        self.poses.lock().unwrap_or_else(|e| e.into_inner()).insert(o, frames);
    }

    /// Pose animated parts with controller type `0x100` (`cAnimObjectNode`).
    pub fn pose(&self, object: u32, frames: Vec<[V4; 4]>) {
        self.pose_with_controller(object, 0x100, frames);
    }

    /// The object's controller gone (killed when its cell turns off, `World_CellDeactivate` 0x260108): its parts
    /// at their own matrices again.
    pub fn unpose(&self, object: u32) {
        let o = object as usize;
        let mut world = self.world.lock().unwrap_or_else(|e| e.into_inner());
        let Some(obj) = world.objects.get_mut(o) else { return };
        obj.controller = None;
        self.poses.lock().unwrap_or_else(|e| e.into_inner()).remove(&o);
    }
}

/// The world grid `Rail_ClosestPoint` 0x259330 walks (row G20c2c1a), from the course as the game resolves it: the same
/// grid as `World_RayCast`'s (the world's origin, cell side and size), each cell a `.ltg` TGroup (its box) whose 16
/// INodes are the patch records (their box; the in-use words +0x24, +0x28, the halfword +0x2c and the word +0x48 as the
/// file has them, nonzero where the game's are), each INode's spline list (+0x2a count, +0x3c offset from its TGroup)
/// the `.pbd` spline segments it holds (checked on gari: every entry a segment index whose box meets the INode's).
/// A curve is the segment's four rows (+0x00..+0x3c: the cubic's coefficients), its box (+0x5c) and its spline (+0x58)
/// as the owner: the spline record's +0x18 (bit 0, the rail switched on: clear in the file, set when the rail's
/// `cRailNode` comes on, [`crate::rider::ground_tick::RailWorld::set_live`]) and +0x24. The game turns the indices
/// into pointers after loading; the port keeps the records.
pub fn rail_world(world: &World, ltg_bytes: &[u8], ltg: &tricky_data::ltg::Ltg, pbd: &tricky_data::pbd::Pbd) -> crate::rider::ground_tick::RailWorld {
    use crate::rider::ground_tick::{RailCell, RailCurve, RailOwner, RailPatch, RailWorld};
    let f = |w: u32| f32::from_bits(w);
    let curve = |k: u32| -> Option<RailCurve> {
        let s = pbd.segments.get(k as usize)?;
        let row = |r: usize| [f(s[4 * r]), f(s[4 * r + 1]), f(s[4 * r + 2]), f(s[4 * r + 3])];
        let spline = tricky_data::pbd::SegmentRec(*s).spline();
        let owner = pbd.splines.get(spline as usize).map(|sp| RailOwner { flags: sp[6], tag: sp[9] });
        Some(RailCurve { m: [row(0), row(1), row(2), row(3)], owner, bbox: [f(s[0x17]), f(s[0x18]), f(s[0x19]), f(s[0x1a]), f(s[0x1b]), f(s[0x1c])], spline: Some(spline) })
    };
    let word = |at: usize| ltg_bytes.get(at..at + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let cells = ltg
        .tgroups
        .iter()
        .map(|g| {
            g.as_ref().map(|(off, g)| RailCell {
                bbox: [g.bbox.min[0], g.bbox.min[1], g.bbox.min[2], g.bbox.max[0], g.bbox.max[1], g.bbox.max[2]],
                patches: g
                    .inodes
                    .iter()
                    .map(|n| {
                        let at = *off as usize + n.offsets[2] as usize;
                        let curves = (0..n.counts[3].max(0) as usize).filter_map(|k| curve(word(at + 4 * k))).collect();
                        RailPatch {
                            bbox: [n.bbox.min[0], n.bbox.min[1], n.bbox.min[2], n.bbox.max[0], n.bbox.max[1], n.bbox.max[2]],
                            x24: (n.counts[0] as u16 as u32) | (n.counts[1] as u16 as u32) << 16,
                            x28: n.counts[2] as u16,
                            x2c: n.counts[4],
                            x48: n.offsets[5],
                            curves,
                        }
                    })
                    .collect(),
            })
        })
        .collect();
    RailWorld { x0: world.origin[0], y0: world.origin[1], cell: world.size, nx: world.width, ny: world.height, cells }
}
