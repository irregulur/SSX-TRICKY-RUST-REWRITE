//! The particle emitter's setters (board row G7f6): `ParticleSys_InitEmitter` 0x1d50b0, `_SetSizeRange` 0x1d52e8,
//! `_SetLifeRange` 0x1d5300, `_SetSpawnLine` 0x1d5380, `_SetSpawnQuad` 0x1d5418, `_ClearForce` 0x1d5548,
//! `_SetVelocityBasis` 0x1d5790, `_SetColors` 0x1d58e0, `_SetTime` 0x1d59e0, `_AdvanceTime` 0x1d59f0 and `_StepEmitter`
//! 0x1d5a30 (row E1b2g1). An emitter is 0x140 bytes of words (the functions take it as a word slice); +0x3c is its
//! time scale. `_RandomizeFactors` 0x1d5180 is `fxrand::emitter_factors`.

pub type V4 = [f32; 4];

/// The PS2 never makes an infinity: an overflow stops at the largest float.
pub fn cl(x: f32) -> f32 {
    if x.is_infinite() {
        f32::MAX.copysign(x)
    } else {
        x
    }
}

pub fn scale(v: V4, k: f32) -> V4 {
    v.map(|x| cl(x * k))
}

pub fn add(a: V4, b: V4) -> V4 {
    [cl(a[0] + b[0]), cl(a[1] + b[1]), cl(a[2] + b[2]), cl(a[3] + b[3])]
}

pub fn sub(a: V4, b: V4) -> V4 {
    [cl(a[0] - b[0]), cl(a[1] - b[1]), cl(a[2] - b[2]), cl(a[3] - b[3])]
}

/// The vector unit's four-lane dot: `((a0b0 + a1b1) + 1·a2b2) + 1·a3b3`.
pub fn dot4(a: V4, b: V4) -> f32 {
    let m = [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]];
    ((m[0] + m[1]) + 1.0 * m[2]) + 1.0 * m[3]
}

/// A division that, like the PS2's, answers the largest float (signed) for a zero divisor.
pub fn ps2_div(a: f32, b: f32) -> f32 {
    if b == 0.0 {
        return if a.is_sign_negative() != b.is_sign_negative() { -f32::MAX } else { f32::MAX };
    }
    cl(a / b)
}

fn f(e: &[u32], b: usize) -> f32 {
    f32::from_bits(e[b / 4])
}

fn set(e: &mut [u32], b: usize, v: f32) {
    e[b / 4] = v.to_bits();
}

fn v(e: &[u32], b: usize) -> V4 {
    [0, 1, 2, 3].map(|k| f(e, b + 4 * k))
}

fn setv(e: &mut [u32], b: usize, x: V4) {
    for k in 0..4 {
        set(e, b + 4 * k, x[k]);
    }
}

/// `ParticleSys_InitEmitter` 0x1d50b0(f12 = rate, f13, f14 = time scale, e, n, a2, a3 = a vector).
#[allow(clippy::too_many_arguments)]
pub fn init(e: &mut [u32], f12: f32, f13: f32, f14: f32, n: i32, a2: i32, a3: V4) {
    let f12 = f12 * f14;
    let f13 = f13 * f14;
    e[0] = n as u32;
    set(e, 0xc, ps2_div(f12, n as f32));
    set(e, 0x20, ps2_div(1.0, a2 as f32));
    let q = ps2_div(1.0, f14);
    let v1 = scale(a3, q);
    set(e, 0x24, f13);
    set(e, 0x3c, f14);
    let v2 = scale(v1, q);
    setv(e, 0x50, v2);
    e[4 / 4] = a2 as u32;
    setv(e, 0x120, [0.0; 4]);
    setv(e, 0x130, [1.0, 1.0, 0.0, 0.0]);
}

/// `ParticleSys_SetSizeRange` 0x1d52e8.
pub fn size_range(e: &mut [u32], min: f32, max: f32) {
    let d = max - min;
    set(e, 0x10, d);
    set(e, 0x18, min - d);
}

/// `ParticleSys_SetLifeRange` 0x1d5300.
pub fn life_range(e: &mut [u32], min: f32, max: f32) {
    let d = max - min;
    let k = f(e, 0x3c);
    let lo = min * k;
    let d = d * k;
    set(e, 0x14, d);
    set(e, 0x1c, lo - d);
}

/// `ParticleSys_SetSpawnLine` 0x1d5380(e, p0, p1).
pub fn spawn_line(e: &mut [u32], p0: V4, p1: V4) {
    setv(e, 0xa0, sub(scale(p0, 2.0), p1));
    setv(e, 0xb0, sub(p1, p0));
    setv(e, 0xc0, [0.0; 4]);
}

/// `ParticleSys_SetSpawnQuad` 0x1d5418(e, p, a, b).
pub fn spawn_quad(e: &mut [u32], p: V4, a: V4, b: V4) {
    let s = scale(add(a, b), 3.0);
    setv(e, 0xa0, sub(p, s));
    setv(e, 0xb0, scale(a, 2.0));
    setv(e, 0xc0, scale(b, 2.0));
}

/// `ParticleSys_ClearForce` 0x1d5548.
pub fn clear_force(e: &mut [u32]) {
    setv(e, 0xd0, [0.0; 4]);
}

/// `ParticleSys_SetVelocityBasis` 0x1d5790(e, v, u1, u2, u3).
pub fn velocity_basis(e: &mut [u32], vel: V4, u1: V4, u2: V4, u3: V4) {
    let q = ps2_div(1.0, f(e, 0x3c));
    let a = scale(vel, q);
    setv(e, 0x60, a);
    setv(e, 0x70, scale(u1, q));
    setv(e, 0x80, scale(u2, q));
    setv(e, 0x90, scale(u3, q));
    let sum = add(add(v(e, 0x70), v(e, 0x80)), v(e, 0x90));
    setv(e, 0x60, sub(a, scale(sum, 1.5)));
}

/// `ParticleSys_SetColors` 0x1d58e0(f12 = life, e, c0, c1, c2, c3).
pub fn colors(e: &mut [u32], life: f32, c0: V4, c1: V4, c2: V4, c3: V4) {
    let g = scale(add(c2, c3), 1.5);
    setv(e, 0xe0, sub(c0, g));
    let life = life * f(e, 0x3c);
    let d = sub(c1, c0);
    let q = ps2_div(1.0, life);
    setv(e, 0x110, scale(d, q));
    setv(e, 0xf0, c2);
    setv(e, 0x100, c3);
}

/// `ParticleSys_SetTime` 0x1d59e0.
pub fn set_time(e: &mut [u32], t: f32) {
    let k = f(e, 0x3c);
    set(e, 0x8, t * k);
}

/// `ParticleSys_AdvanceTime` 0x1d59f0.
pub fn advance_time(e: &mut [u32], dt: f32) {
    let k = f(e, 0x3c);
    let t = f(e, 0x8);
    set(e, 0x8, t + dt * k);
}

/// `ParticleSys_StepEmitter` 0x1d5a30(e, dt): [`advance_time`], then while the emitted count (+0, an integer) times
/// the interval (+0xc) is below the time (+0x8): the time less one interval, the spawn point (+0xa0) moved by its
/// step (+0xd0, the vector unit's `vadd`), and each random factor ([`crate::fxrand::EMITTER_FACTORS`]) stepped by
/// `Rand_VuNext` 0x1d5a08 ([`crate::fxrand::vu_next`]). The count is not changed here, so the loop ends on the time.
pub fn step(e: &mut [u32], dt: f32) {
    advance_time(e, dt);
    let owed = |e: &[u32]| cl(e[0] as i32 as f32 * f(e, 0xc)) < f(e, 0x8);
    while owed(e) {
        set(e, 0x8, cl(f(e, 0x8) - f(e, 0xc)));
        setv(e, 0xa0, add(v(e, 0xa0), v(e, 0xd0)));
        for o in crate::fxrand::EMITTER_FACTORS {
            e[o / 4] = crate::fxrand::vu_next(e[o / 4]);
        }
    }
}

/// Words in a `ParticleSys` block (0x170 bytes: +0 lifetime, +4 texture, +8 blend kind, +0xc pad,
/// +0x10..+0x150 emitter block, +0x150..+0x170 AABB min/max).
pub const SYS_WORDS: usize = 0x170 / 4;

fn expand_box(lo: &mut V4, hi: &mut V4, p: V4) {
    for k in 0..3 {
        if p[k] < lo[k] {
            lo[k] = p[k];
        }
    }
    for k in 0..3 {
        if hi[k] < p[k] {
            hi[k] = p[k];
        }
    }
}

fn mat_mul_v4(m: &[V4; 4], x: V4) -> V4 {
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        let mut acc = cl(m[0][c] * x[0]);
        acc = cl(acc + cl(m[1][c] * x[1]));
        acc = cl(acc + cl(m[2][c] * x[2]));
        out[c] = cl(acc + cl(m[3][c] * x[3]));
    }
    out
}

/// `ParticleSys_CalcBounds` 0x1d5b60(e, out_min, out_max): analytical AABB over the emitter's spawn corners
/// (four with no force + three with `count * force`, matching the original's omitted `(1,1)` force corner) and
/// VU1 program 6 trajectories at `t_max` and at each positive axis extremum `t = (W_c / (V_ext - W_c) + 0.73) * 4.424779`
/// clamped to `2.7`.
pub fn calc_bounds(e: &[u32]) -> (V4, V4) {
    let dlife = f(e, 0x14);
    let base_life = f(e, 0x1c);
    let mut t_max = base_life + dlife;
    // (the original doubles the spread first: one rounding, `add.s f2, f0, f0` at 0x1d5bb4)
    let hi_life = base_life + (dlife + dlife);
    if t_max <= hi_life {
        t_max = hi_life;
    }

    let start = v(e, 0xa0);
    let a = v(e, 0xb0);
    let b = v(e, 0xc0);
    let force = v(e, 0xd0);
    let n = e[0] as i32 as f32;

    let p00 = add(add(start, a), b);
    let mut lo = p00;
    let mut hi = p00;
    let a2 = scale(a, 2.0);
    let b2 = scale(b, 2.0);
    expand_box(&mut lo, &mut hi, add(add(start, a2), b));
    expand_box(&mut lo, &mut hi, add(add(start, a), b2));
    expand_box(&mut lo, &mut hi, add(add(start, a2), b2));
    let fn_v = scale(force, n);
    expand_box(&mut lo, &mut hi, add(add(add(start, a2), b), fn_v));
    expand_box(&mut lo, &mut hi, add(add(add(start, a), b2), fn_v));
    expand_box(&mut lo, &mut hi, add(add(add(start, a2), b2), fn_v));

    let smin = lo;
    let smax = hi;

    let vel = v(e, 0x60);
    let u1 = v(e, 0x70);
    let u2 = v(e, 0x80);
    let u3 = v(e, 0x90);
    let u1_2 = scale(u1, 2.0);
    let u2_2 = scale(u2, 2.0);
    let u3_2 = scale(u3, 2.0);

    let v000 = add(add(add(vel, u1), u2), u3);
    let mut vmin = v000;
    let mut vmax = v000;
    expand_box(&mut vmin, &mut vmax, add(add(add(vel, u1_2), u2), u3));
    expand_box(&mut vmin, &mut vmax, add(add(add(vel, u1), u2_2), u3));
    expand_box(&mut vmin, &mut vmax, add(add(add(vel, u1), u2), u3_2));
    expand_box(&mut vmin, &mut vmax, add(add(add(vel, u1_2), u2_2), u3));
    expand_box(&mut vmin, &mut vmax, add(add(add(vel, u1_2), u2), u3_2));
    expand_box(&mut vmin, &mut vmax, add(add(add(vel, u1), u2_2), u3_2));
    expand_box(&mut vmin, &mut vmax, add(add(add(vel, u1_2), u2_2), u3_2));

    let w = v(e, 0x50);
    let dw_max = sub(w, vmax);
    let dw_min = sub(w, vmin);

    let k27 = f32::from_bits(0x402c_cccd);
    let k0113 = f32::from_bits(0x3de7_6c8b);
    let km073 = f32::from_bits(0xbf3a_e148);
    let kinv = f32::from_bits(0x408d_97ca);

    let eval_at = |lo: &mut V4, hi: &mut V4, t_lin: f32, t_bend: f32| {
        let bend = t_bend * (t_bend * k0113 + km073);
        let wt = scale(w, t_lin);
        for (s, dw) in [(smin, dw_max), (smin, dw_min), (smax, dw_max), (smax, dw_min)] {
            let p = add(add(s, wt), scale(dw, bend));
            expand_box(lo, hi, p);
        }
    };

    let m = if k27 <= t_max { k27 } else { t_max };
    eval_at(&mut lo, &mut hi, t_max, m);

    let ext = [
        (ps2_div(w[0], vmin[0] - w[0]) - km073) * kinv,
        (ps2_div(w[0], vmax[0] - w[0]) - km073) * kinv,
        (ps2_div(w[1], vmin[1] - w[1]) - km073) * kinv,
        (ps2_div(w[1], vmax[1] - w[1]) - km073) * kinv,
        (ps2_div(w[2], vmin[2] - w[2]) - km073) * kinv,
        (ps2_div(w[2], vmax[2] - w[2]) - km073) * kinv,
    ];
    for mut t in ext {
        if 0.0 < t {
            if k27 <= t {
                t = k27;
            }
            eval_at(&mut lo, &mut hi, t, t);
        }
    }
    (lo, hi)
}

/// `ParticleSys_Create` 0x1d8490(ps, mat, rec): builds a 0x170-byte particle system (`92` words) from an
/// instance's 4×4 matrix `mat` (rows in game cm) and 51-word level `type2Sub0` record `rec[0..51]`, drawing
/// the nine random factors from `Rand_Fx` (`ParticleSys_RandomizeFactors` 0x1d5180 at 0x1d85c4).
pub fn create(ps: &mut [u32], mat: &[V4; 4], rec: &[u32; 51], rand_fx: &mut dyn FnMut() -> u32) {
    let rf = |i: usize| f32::from_bits(rec[i]);
    let dur = rf(2);
    ps[1] = rec[0x31];
    ps[2] = rec[0x32];
    let (lifetime, init_dur) = if dur < 0.0 {
        (dur, rf(5) + rf(7) * 0.5)
    } else {
        ((dur + rf(5)) + rf(7) * 0.5, dur)
    };
    ps[0] = lifetime.to_bits();

    let e = &mut ps[4..84];
    init(e, init_dur, rf(8), rf(3), rec[0] as i32, rec[1] as i32, [rf(30), rf(31), rf(32), 0.0]);
    let h_sz = rf(6) * 0.5;
    size_range(e, rf(4) - h_sz, rf(4) + h_sz);
    let h_life = rf(7) * 0.5;
    life_range(e, rf(5) - h_life, rf(5) + h_life);
    for (o, w) in crate::fxrand::EMITTER_FACTORS.iter().zip(crate::fxrand::emitter_factors(rand_fx)) {
        e[o / 4] = w;
    }
    clear_force(e);

    let o = mat_mul_v4(mat, [rf(9), rf(10), rf(11), 1.0]);
    let a = mat_mul_v4(mat, [rf(12), rf(13), rf(14), 0.0]);
    let b = mat_mul_v4(mat, [rf(15), rf(16), rf(17), 0.0]);
    setv(e, 0xb0, a);
    setv(e, 0xc0, b);
    setv(e, 0xa0, sub(o, scale(add(a, b), 1.5)));

    let q = ps2_div(1.0, rf(3));
    let v0 = scale(mat_mul_v4(mat, [rf(18), rf(19), rf(20), 0.0]), q);
    let vs0 = scale(mat_mul_v4(mat, [rf(21), rf(22), rf(23), 0.0]), q);
    let vs1 = scale(mat_mul_v4(mat, [rf(24), rf(25), rf(26), 0.0]), q);
    let vs2 = scale(mat_mul_v4(mat, [rf(27), rf(28), rf(29), 0.0]), q);
    setv(e, 0x70, vs0);
    setv(e, 0x80, vs1);
    setv(e, 0x90, vs2);
    setv(e, 0x60, sub(v0, scale(add(add(vs0, vs1), vs2), 1.5)));

    let col = |base: usize| scale([rf(base + 1), rf(base + 2), rf(base + 3), rf(base)], 128.0);
    colors(e, rf(5) + rf(7) * 0.5, col(33), col(37), col(41), col(45));

    if lifetime < 0.0 {
        set_time(e, init_dur);
    } else {
        set_time(e, 0.0);
    }

    let (lo, hi) = calc_bounds(e);
    for k in 0..4 {
        ps[84 + k] = lo[k].to_bits();
        ps[88 + k] = hi[k].to_bits();
    }
}

/// `ParticleNode_SetLifetime` 0x1d8ac8(ps, life): sets lifetime and time to `life` if nonzero, else resets time to 0.
pub fn set_lifetime(ps: &mut [u32], life: f32) {
    if life != 0.0 {
        ps[0] = life.to_bits();
        set_time(&mut ps[4..84], life);
    } else {
        set_time(&mut ps[4..84], 0.0);
    }
}

/// `ParticleNode_Update` 0x1d8b20(ps): one 60 Hz tick. Continuous systems (`lifetime < 0`) run
/// `ParticleSys_StepEmitter` 0x1d5a30 (advancing time and stepping VU random factors on wrap); one-shot systems
/// (`lifetime > 0`) run `ParticleSys_AdvanceTime` 0x1d59f0 and count `lifetime` down by `1/60` to `0`.
pub fn node_update(ps: &mut [u32]) {
    let tick = f32::from_bits(0x3c88_8889);
    let life = f32::from_bits(ps[0]);
    if life != 0.0 {
        if life < 0.0 {
            step(&mut ps[4..84], tick);
        } else {
            advance_time(&mut ps[4..84], tick);
            let mut rem = life - tick;
            if rem < 0.0 {
                rem = 0.0;
            }
            ps[0] = rem.to_bits();
        }
    }
}
