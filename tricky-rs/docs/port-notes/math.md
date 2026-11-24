# Math library: how the original works

Part of the port notes (index: [README.md](README.md)); units and wording as described there.

## Sine, cosine, arcsine, arctangent (E14; ported exactly in `tricky-rs/game/src/math.rs`, checked bit for bit)

- `Math_Sin` 0x250d60, `Math_Cos` 0x250e98: quadrant q = trunc(x·0.63661975 ± 0.5) (sign of x), r = x − q·1.5707964;
  sine polynomial (((r²·2.7557319e-6 − 0.0001984127)r² + 0.008333334)r² − 0.16666667)·r²·r + r, cosine polynomial
  (((r²·2.4801588e-5 − 0.0013888889)r² + 0.041666668)r² − 0.5)r² + 1; odd q swaps them; sin negated when q & 2,
  cos when (q + 1) & 2.
- `Math_SinCos` 0x250c10: one sine polynomial ((… − 0.16666667)r² + 1)·r and cos = √(1 − s²), placed by q & 3.
- `Math_Asin` 0x250fd0: |x| ≤ 0.70710677: x + x·P(x²); beyond, ±(π/2 − √t·(1 + P(t))), t = 1 − x². P(t) =
  t(A1 + t(A2 + t(A3 + t·A4))), A = 0x3e2bba25, 0x3d8a908b, 0x3d55033a, 0x3d6137ab ($gp − 0x7ff0..).
- `Math_Atan` 0x2510f8: the same P on t = x²/(x² + 1) inside ±1 (√t(1 + P(t)), sign of x), and on t = 1/(x² + 1)
  outside (±(π/2 − √t(1 + P(t)))).
- `Math_AngleDiff` 0x1741d0: d = a − b, n = floor(d·0.15915494 + 0.5) (truncate, then − 1 if above), d − n·6.2831855.
- `Math_Hermite` 0x1704b0: cubic Hermite in the order ((2t³ − 3t²)p0 + p0) + (−2t³ + 3t²)p1 + ((t³ − 2t²) + t)m0 +
  (t³ − t²)m1.

## The header's constant vectors (row X2a, proven)

Forty static initializers (`StaticInit_*`, 124 bytes each, from `StaticInit_PrewindControl` 0x105b10 to
`StaticInit_Spline` 0x251b20) do one thing: write a header's three constant vectors, x y z only, 0x10 apart, at the
including file's own copy: (1, 0.966, 0), (1, 0.474, 0), (1, 0.0757, 0.002) (bits 0x3f76e979, 0x3ef2fec5,
0x3d9b3d08, 0x3b03126f). `tricky_game::statics` keeps the forty places (`HEADER_INITS`) and writes them
(`init_header_vectors`); `tests/statics.rs` runs each original on random memory around its place. The 284-byte ones
(`StaticInit_PostAI`, `_Boarder`, `_Lesson`, `_HUD`, row X2b) write six more words-groups first, in one of two
layouts (`WIDE_A` for PostAI and HUD, `WIDE_B` for Boarder and Lesson; 0x60 bytes, then the vectors at +0x60:
`init_wide`), checked by the same test.
