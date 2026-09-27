# Brick visuals audit against v20 (in progress, 2026-09-27)

Evidence: read-only disassembly of `blocklandv20.exe` (capstone, image base
0x400000), authored v20 BLBs under `base/data/bricks`, v20 surface PNGs, and
`.research/bl-decompiled/v20/client/defaults.cs`. No fixes have landed yet;
side-by-side renders are still to do.

## v20 render state recovered from the exe

- `fxBrickBatcher` texture slots (init at 0x531f40, loader 0x52cde0):
  0 TOP, 1 BOTTOMLOOP, 2 BOTTOMEDGE, 4 RAMP use `GL_REPEAT` + linear/trilinear;
  **3 SIDE uses `GL_CLAMP` + `GL_NEAREST` magnification**
  (`GL_NEAREST_MIPMAP_LINEAR` min). Prints (slot 5+) repeat + trilinear.
- Batch begin (0x52d120): depth test, `GL_COLOR_MATERIAL`, cull face,
  texture env **`GL_DECAL` (0x2101)**; fixed-function lighting enabled in
  0x531860 (all 8 lights, quadratic attenuation 0.1).
  DECAL means `final = mix(clamp(paint * lighting), tex.rgb, tex.a)`,
  alpha = vertex (paint) alpha only.
- Opaque pass (0x531d20) draws per-texture buckets; translucent pass
  (0x531de0) enables blend `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` and draws buckets
  in order prints, RAMP, TOP, BOTTOMLOOP, BOTTOMEDGE, SIDE (no depth sort).
- Ghost (temp) brick prefs, `updateTempBrickSettings` 0x52dcc0: flash time
  800 ms (clamped 10..1000 in engine), flash offset 0.3, range 0.3, outside
  colour = paint colour, inside colour = black (defaults.cs lines 55-65).

## Ranked findings

1. **Surface overlay is lit (wrong order).** `scene.wgsl` mixes the overlay
   into paint and then multiplies by illumination. v20 DECAL applies the
   white/black bevel overlay after lighting, so the bevel highlights keep
   constant strength in shadow. Fix: light the paint, clamp to 1, then mix.
2. **SIDE sampled with repeat and linear filtering.** v20 clamps and uses
   nearest magnification. Authored side UVs run past 0..1 (-0.09..1.09 on
   plates, -0.02..1.02 on bricks); with repeat the rim moves inward and the
   opposite edge wraps in. Fix: a clamped, nearest-mag sampler for SIDE only.
3. **Ghost brick look differs.** Ours is the full textured brick at a static
   45% alpha. v20 pulses the temp brick (offset 0.3 +/- range 0.3 over 800 ms)
   and draws an inside colour (black) separate from the paint-coloured outside.
   The exact geometry for the inside/outside passes still needs locating
   (readers of 0x76dd44..0x76dd5c).
4. **Colour FX are unverified approximations** (pearl, chrome, glow, blink,
   swirl, rainbow, undulo, water); see `docs/research/brick-fx/README.md`.
   Their exe implementation is not located yet.
5. Generated `BRICK` UVs in `crates/convert/src/brick.rs` match the authored
   pattern (SIDE stretched 0..1 per face, TOP one tile per stud, BOTTOMEDGE /
   BOTTOMLOOP per stud). Still to confirm against the exe's generator.

## Still to audit

Ramp/slope, corner, wedge and round faces; UV scale across sizes; print
direction and mirroring; transparency sorting; colour and shade per face;
side-by-side renders against the v20 look.
