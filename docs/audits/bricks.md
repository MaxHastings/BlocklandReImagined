# Brick visuals audit against v20 (2026-09-27)

Ground truth: read-only disassembly and emulation of `blocklandv20.exe`
(image base 0x400000; capstone for reading, unicorn for running the brick
generator), the authored v20 BLBs and surface PNGs, and
`.research/bl-decompiled/v20/client/defaults.cs`. Nothing in the original
install was modified.

## How v20 draws bricks (recovered)

- **Textures** (`fxBrickBatcher` init 0x531f40, loader 0x52cde0). Slot 0 TOP,
  1 BOTTOMLOOP, 2 BOTTOMEDGE, 4 RAMP and prints (5+) use `GL_REPEAT` with
  linear/trilinear filtering. **Slot 3 SIDE uses `GL_CLAMP` with nearest
  magnification** (`GL_NEAREST_MIPMAP_LINEAR` minification).
- **Combine** (batch begin 0x52d120): `GL_COLOR_MATERIAL`, fixed-function
  per-vertex lighting (0x531860, 8 lights, quadratic attenuation 0.1) and
  texture env **`GL_DECAL`**: `final = mix(clamp(paint * light), tex.rgb,
  tex.a)`, alpha is the vertex (paint) alpha. The overlay is not lit.
- **Passes**: opaque buckets per texture (0x531d20), then translucent buckets
  (0x531de0) with `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` in the order prints, RAMP,
  TOP, BOTTOMLOOP, BOTTOMEDGE, SIDE. No depth sort; depth writes stay on.
  Blending happens on display values (no sRGB), which our non-sRGB swapchain
  also does.
- **Generated `BRICK` geometry** (datablock loader 0x53ad25, emulated for
  1x1x1, 1x4x3, 4x4x1, 2x3x5, 16x32x1; output kept in
  `crates/convert/data/v20-generated-bricks.jsonl`): outer extents grow by
  0.0012; TOP tiles once per stud with U toward -X and V toward +Y (Torque
  axes); SIDE UVs are centred, `0.5 ± (245/512)·n/(n-0.084)` for a side of
  `n` studs (height in studs is plates·0.4); four BOTTOMEDGE quads and a
  BOTTOMLOOP quad only when both footprint sides exceed one stud.
- **Temp (ghost) brick** (0x52e370 outside, 0x52e860 inside, prefs in
  `updateTempBrickSettings` 0x52dcc0): every quad pushed 0.02 along its
  normals and drawn twice, reversed-winding inside copy in the inside colour
  (default black) and the forward outside copy in paint ×1.5 (default
  `tempBrickOutsideUsePaintColor 1`). Alpha is a triangle wave
  `0.3 + 0.3·t/400`, t = ms mod 800 folded at 400; paint alpha is ignored.
- **Colour and shape FX** are computed per vertex on the CPU every frame in the
  quad emitter 0x52ed70 (colour switch table 0x531840). Only quads whose
  authored colour alpha is negative (paint) get FX; literal-colour quads keep
  their colour with alpha `min(authored, paint)`. Times are milliseconds.
  - Pearl 1: gradient between `clamp(1.6·paint)` and `0.9·paint`,
    `t = clamp(dot(v - c, normalize(c - eye)) / (0.125·depthStuds) + 0.5)`,
    c = brick centre; near side bright, far side dark.
  - Chrome 2: same axis with scale `0.5·depthStuds`; near half blends paint to
    white, far half blends paint to `0.5·paint`; normals doubled (lighting ×2).
  - Glow 3: normals point at the sun with length `1/min(1, sun rgb)`, so
    `light = ambient + sun/min(sun)`: fully lit whatever the facing.
  - Blink 4: rgb × `0.7 + 0.6·tri`, tri = (ms mod 1000 folded at 500)/500.
    Alpha does not change.
  - Swirl 5: like blink with factor `0.4 + 0.6·tri`, each quad corner k
    phase-shifted by 250·k ms, so light runs around every face.
  - Rainbow 6: `phase = 2π·(ms mod 1000)/1000`, per vertex (Torque x,y,z):
    `r = sin(y+z-x+phase+2π/3)·paint.r`, `g = sin(x-y+z+phase+4π/3)·paint.g`,
    `b = sin(x+y-z+phase+2π)·paint.b` (negative clamps to black).
  - Undulo shape 1: only within 100 units of the camera, amplitude
    `clamp(100 - dist, 0, 10)·0.1·0.08`, `s = 1.2566·(x+y+z) + 2π·(ms mod
    1000)/1000`, offsets `sin(s + 2π/3)`, `sin(s + 4π/3)`, `sin(s + 2π)` on
    x, y, z.
  - Water shape 2: `a = 1.2566·x + 2π·(ms mod 2000)/2000`; vertex z lowered by
    `0.1·(sin(a)+1)`; normal z raised by `0.25·(sin(a + 2π/3)+1)`.
  - A paint quad whose colour offset exceeds 0.9 in any channel is forced to
    glow.

## Ranked findings and status

1. **Overlay lit with the paint (fixed).** We multiplied the bevel overlay by
   lighting; v20 decals it on after lighting. Fixed in `scene.wgsl`.
2. **TOP studs rotated 180° on every generated brick (fixed).** The converter
   ran U toward +X/V toward +Z; v20 runs them the other way, which flips which
   bevel edges are highlighted. The converter now reproduces the exe's
   generator exactly (unit test against the emulated output) and
   `maps-pass-007` carries the 80 regenerated meshes.
3. **Generated SIDE UVs 0..1 instead of v20's centred rim scaling (fixed).**
   Rims were too wide on short faces and too narrow on long ones. Fixed with
   finding 2.
4. **SIDE sampled with repeat and linear filtering (fixed).** v20 clamps and
   magnifies with nearest; authored BLBs rely on it (UVs run to -0.09..1.09 on
   plates). New `Material::clamp_nearest`, set on the SIDE surface.
5. **Ghost brick (fixed).** Was the full textured brick at a flat 45% alpha;
   now v20's two-shell pulsing look (`world_scene::v20_temp_brick`). The
   non-rendering bricks shown while building use the same look; v20's exact
   treatment of those was not traced.
6. **Colour/shape FX were invented approximations (fixed).** Blink changed
   alpha (v20 changes brightness), pearl/chrome used a fake sheen, swirl and
   rainbow used unrelated waves, undulo/water used wrong axes and amplitudes.
   The brick shader now evaluates the equations above per vertex. FX data moved
   out of the lightmap UV into a dedicated `SceneVertex::fx` attribute (brick
   centre plus packed colour, shape, corner and depth). Not ported: the forced
   glow for colour offsets above 0.9, glow's fallback for bricks carrying
   their own light, and FX displacement in the shadow pass.
7. **Pumpkin face colour explained (open).** Its literal RGB (200,150,0) goes
   through the literal-colour path and GL clamps it, so it renders as
   full-bright yellow (1,1,0) under any light brighter than 1/150.
8. **Translucent draw order (noted, not changed).** v20 draws translucent
   bricks unsorted by texture bucket with depth writes on, so overlapping
   translucent bricks hide each other. We sort back to front, which looks
   better; left as an intentional modern improvement.
9. **Extents grow by 0.0012 (fixed with 2).** Tiny, but part of the geometry.

Prints, ramps, corners, wedges, crests and rounds use authored BLB UVs; the
side-by-side below shows them matching (letters read correctly, not mirrored).

## Side-by-side verification

`cargo test -p bri-client --test brick_audit -- --ignored` renders sixteen
bricks (generated bricks, plates, ramps, corner ramps, crest, rounds, cone,
three print bricks, a translucent brick) offscreen to
`artifacts/brick-audit/<scene>/ours.png`.
`python tools/brick_reference.py <v20> artifacts/brick-audit/<scene>`
renders the same layout with an independent software rasterizer that reads
the original BLBs and PNGs and applies only the exe-derived rules above, then
writes `v20-reference.png` and `side-by-side.png` in `families/` and `fx/`
(every colour FX and both shape FX frozen at 0.37 s). After the fixes the mean
absolute difference is 1.1/255 for the families and 1.3/255 for the FX; what
remains is edge antialiasing and texture-filter noise on the ramp surface.
