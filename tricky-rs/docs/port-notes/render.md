# Render system port notes (`SLUS_203.26`)

## Boarder effects, tex-flip, model/mesh-anim, particles & sphere-tree cache (row G7s, `rendertail.rs`, checked: `tests/render_misc_tail.rs`)

Proven (`SLUS_203.26` disassembly + runner):

- **Boarder effects & skin memory (`0x123060`, `0x135200`, `0x135268`, `0x136148`, `0x136ea0`, `0x136f88`)**:
  - `RiderSkin_InitMem` initializes the two embedded `MdfMemBank`s at `+0` and `+0x10`.
  - `BoarderRender_ResetEffects` resets all 6 embedded effects (`BoardPuff` `+0`, `BoardTrail` `+0x1c20`,
    `SurfaceSpray` `+0x4b60`, `BoardTrack` `+0x8020`, `BrakeSpray` `+0x99b0`, `SpeedDust` `+0xb600`);
    `BoarderRender_SpawnLandingPuffs` tail-calls `Fx_SpawnLandingPuffs(this+0x99b0, ...)`;
    `BoarderRender_DestructEffects` tears down `SpeedDust` (`+0xb600`) and `BoardPuff` (`+0`) with flags `2`.
  - `BoarderRender_RecordStreaks` samples hand-streak points every 3rd race frame (`race+0x18 % 3 == 0`) when
    `rider+0x130 > 0` or `rider+0x134 > 0` (choosing color from `0x3bc230` / `0x3bc240` / `0x3bc250` using
    thresholds `0x3eaa_d18b` and `0x3f2a_7e78`) into a 7-entry ring at `+0x20`, and every frame samples the
    boarder matrix into a 7-entry ring at `+0x200` while `rider+0x428 == 0x10`.
- **TexFlip & model/mesh-anim (`0x142d40`, `0x142da8`, `0x1499e8`, `0x19aa18`, `0x19aab0`)**:
  - `TexFlip_ApplyFrame` copies `u16` frame handles from `inst->+0xf0[i+1]->+0x44[frame+1]` to
    `this + i*0x48 + 0x34`; `TexFlip_Draw` frustum-culls instance bounds via `gfx` `+0x284` and submits via
    `+0x2ac` with `this+0x10`.
  - `Model_IsBoxCulled` returns `(gfx->vf_284(min, max) != 1) as u32` (1 = visible, 0 = culled).
  - `MeshAnim_AllocFrames` counts animated model parts (`part+0x10 != 0`), allocates `count * 0xd0 + 0x10` bytes
    on `gScriptPool` (`0x345e48`, flags `0x5000_0000`, `"MeshAnimFrames"` `0x38c338`, `0xdeadc0ed` fill),
    initializes each `cMeshAnimFrame` (`cAnimFrame` vtable `0x391b10` -> `AnimFrame_Clear` -> `cMeshAnimFrame`
    vtable `0x391af8` -> `AnimFrame_SetIdentity`), and calls `AnimFrame_SetKey` with `*(f32*)(this+0x24)`.
- **Particles (`0x1d5338`, `0x1d54e0`, `0x1d5570`, `0x1d8b00`, `0x1d98f8`)**:
  - `ParticleSys_SetSpawnPoint` copies `+0xa0` and zeros `+0xb0`/`+0xc0`; `ParticleSys_SetForce` scales by
    `*(this+0xc) / (scale * *(this+0x3c))` into `+0xd0`; `ParticleSys_SetVelocityCone` builds an orthonormal
    spread basis (`+0x70`, `+0x80`, `+0x90 = 0`) around `vel / *(this+0x3c)` using `[1,0,0,0]` when `vel.x == 0`
    else `[0,1,0,0]`, and shifts `+0x60` by `-1.5 * (v70 + v80 + v90)`.
- **Mesh scaling, owned buffers, masked draw, sphere-tree cache & RLE (`0x203d40..0x23b520`)**:
  - `MeshChunk_ScalePositions` walks VIF/DMA qword packets (`qwc = low 16 bits`), skips strip headers, UV block
    (`(n/2)*16 + (0x40 if (n*8)&15 else 0x30)`), and normal block (`(n*6/16)*16 + (0x40 if (n*6)&15 else 0x30)`),
    scales `total_verts` 12-byte xyz floats by `scale`, and skips the trailing packet + `num_morphs` morph packets.
  - `MaskedList_Draw` renders the bounding sphere in red `[1,0,0,1]` and each bit-masked sub-sphere in yellow
    `[1,1,0,0]` via `gfx` vtable `+0x26c`.
  - `SphereTreeCache_Init` / `SphereTreeBufCache_FreeAll` manage the two global octree buffer pools at
    `0x3439a0..0x3439c0` (sizes `sum_{k=0..depth} 8^k`, `0xdeadc0ed` fill).
  - `Rle_Encode` packs literal runs (`-lit`, max `0x7f`) and repeat runs (`run - 1`, max `0x7f`) terminated by `0`,
    preserving the original end-of-buffer decrement when `v1 == end && run >= 2`.
- Checked bit-for-bit across all 26 functions in `tests/render_misc_tail.rs` (reducing `render` `not started`
  from 174 to 148).

## `cPS2GraphicsMan`, `cGraphicsMan`, `cPS2LightMan` & `cPS2GridMesh` virtuals & matrix stack (row G7t, `rendertail.rs`, checked: `tests/render_gfxman_tail.rs`)

Proven (`SLUS_203.26` disassembly + runner):

- **Lifecycle (`0x1dbab0`, `0x1ea0c0`, `0x1ea130`, `0x1ea290`)**: `cPS2GraphicsMan::cPS2GraphicsMan` sets
  `+0x10 = 0x393610` and installs `tPS2DrawState` vtable `0x390ad0` on the 4 embedded state slots at
  `+0x3fc + i*0x1c + 0x18`; `~cPS2GraphicsMan` tail-calls `~cGraphicsMan` (`+0x10 = 0x393a20`, `Mem_Delete` if
  `flags & 1`); `~cPS2LightMan` tail-calls `cLightMan::~cLightMan` (`0x1ce110`).
- **Viewport stack, display modes & clear color (`0x1e8928..0x1e9108`, `0x1e9628`, `0x1e9ff0..0x1ea020`)**:
  `vf16_1e8cd8` pushes a `0x34`-byte stack entry at `+0x14 + idx*0x34` (`idx = +0xe4`); `vf23_1e8970` truncates
  its 4 floats `+4..+0x14` via `cvt.w.s`; `vf7..vf11` query the `0x94`-byte display-mode table at `+0x654`
  (index `+0x650`, count `+0x64c`); `vf12_1e8e20` packs `(u32)(rgb * 255.0)` into `+0x1724`; `vf19_1ea020`
  converts four `i32`s to `$f12..$f15` and dispatches virtual `+0x9c`.
- **Matrix stack & fog (`0x1e9128`, `0x1e9880..0x1e9ea8`)**: `vf26`/`vf27` push/pop `0x40`-byte matrices on
  `+0x3f4` (`+0x3f0` depth); `vf29`/`vf31`/`vf32`/`vf33` load, post-multiply, load identity (`0x346440`), and
  translate the top matrix while clearing dirty flag `+0x17d0`. `PS2Render_CalcFog` updates model matrices
  (`0x1e85b0`), computes `clamp((M_5b0 * pt).w * *(+0x63c), 0.0, *(+0x630))`, and returns its truncated `u16`.
- **Trivial virtual stubs (`0x1ea1d8..0x1ea310`)**: 14 no-op, 5 return-0, and 2 return-1 stubs on `cPS2LightMan`
  and `cGraphicsMan`.
- Checked bit-for-bit across all 52 functions in `tests/render_gfxman_tail.rs` (reducing `render` `not started`
  from 148 to 96).
