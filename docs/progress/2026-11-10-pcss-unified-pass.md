# 2026-11-10 PCSS unified pass (one filter for caster and map shadows)

Follow-up to the early-outs note (`2026-11-10-pcss-dynamic-sun-early-outs.md`)
on `codex/pcss_fix`. The branch's map-cascade PCSS work fixed the visible
edge-softness mismatch between player/object shadows and map-geometry
shadows, but it did so by running **two** full PCSS filters per shadowed
receiver: the live-caster filter (`pcss_lit`, ~41 texture ops) and the map
filter (old `map_pcss_lit`, ~49), for ~90 in the general case. This change
replaces both with a single pass over both layers, so every sun shadow is
softened by exactly one mechanism - same taps, same radius law, same bilinear
behaviour - and costs less than either pair of filters combined.

## What changed (`scene_original.wgsl` + `scene.wgsl`, kept in lockstep)

- New `combined_pcss_tap()`: one bilinear comparison at a tap position against
  the receiver's plane in **both** layers at once - caster layer with its
  occluder rule (a brick that does not cast stops a shadow from passing), map
  layer as a plain comparison at its own rescaled depth. A texel is lit only
  where neither layer blocks; the bilinear runs over the per-texel product,
  not over two separate interpolations.
- New `combined_pcss_lit()`: one blocker search in both layers (each keeps
  its own bias, slope and occluder rules) accumulating every blocker's
  distance back along the sun into **one weighted sum in world units**
  (`inv_scale = 1/scale`, `inv_scale_map = 1/(scale*map_scale)`), so a single
  radius serves both shadows. When only one layer has blockers nearby the
  weighted sum reduces exactly to that layer's own distance estimate, as its
  filter computed it alone. The filter then averages `combined_pcss_tap` over
  `PCSS_FILTER_TAPS` (16) taps at that radius.
- All three PCSS consumers route through the combined pass when Soft Sun
  Shadows is on: `object_sun`, `dynamic_sun`, and - discovered while doing
  this - `relit_sun` (lightmapped texels), which previously only got map-layer
  PCSS. Past the cascades (and while the map layer is not drawn) each keeps
  its coarse fallback (`far_map_lit`, whose own far-map PCSS is unchanged).
- Old `map_pcss_lit()` removed (no caller left after the routing change), and
  `map_cascade_lit()` simplified to the fixed four-tap filter it was before
  the branch, used only when Soft Sun Shadows is off. The
  `shadows.map_params.x > 0.0` "map drawn this frame" gate is kept before any
  map-layer read in every consumer.

## Cost per shadowed receiver (texture ops)

| Case | Branch as committed | Early-outs (prev note) | Unified pass |
|---|---|---|---|
| General (both layers active) | ~90 (41 + 49) | ~90 | **~82** (2 core gathers + 32 ring loads + 16 taps x 3 gathers) |
| Cascade blend band (two cascades) | ~180 | ~180 | **~164** |
| Fully covered by a live caster | ~90 | ~41 (map filter skipped) | ~82 (no short-circuit) |

The fully-covered case loses the early-out's short-circuit: the combined pass
cannot know the result is zero until it has filtered. Accepted trade-off for
one mechanism, but it is a real regression *for that specific case* - if
in-game profiling shows live-object-shadow-heavy scenes regressing, restoring
a caster-only pre-check there is the follow-up to make (it would keep both
quality and cost).

## The grain fix: fixed filter layout across pixels

First attempt kept the branch's per-pixel tap rotation (`pcss_noise()` angle)
in the combined filter. The Dynamic window-shadow regression failed on edge
grain in the Bedroom-scale case (max adjacent-pixel step 29 vs a threshold of
0.8 x the hard filter's 15): with only 16 taps, each pixel's random rotation
made neighbouring pixels' averages jitter against each other at the penumbra
edge. The fix is to keep **one fixed tap layout across all pixels** in the
filter loop (angle 0), while the search ring keeps its per-pixel angle for
the radius estimate. Every pixel then averages the same directions, so
neighbouring pixels differ only by position; both regression cases pass with
this at unchanged tap counts, which is what keeps the cost at ~82 instead of
escalating to 32 taps (~130 ops).

Known trade-off to watch in-game: a fixed pattern can show moiré/shimmer when
a penumbra edge aliases against the tap lattice (the old per-pixel rotation
averaged that away). The regression scene has one straight edge and cannot see
it. If it shows up on brick-heavy shadows, the options are more taps at the
cost above, or a two-ring fixed layout with offset angles.

## Behaviour notes

- The cascade fade band (`0 < strength < 1`) now blends as
  `mix(far_map_lit(...), combined, s)` instead of the old
  `min(mix(far, lit_map, s), mix(1, lit_cast, s))`. Endpoints (s = 0, s = 1)
  are identical; mid-band values differ slightly. The regression cases sit at
  s = 1 or beyond range and do not exercise the mid-band.
- PCSS-off paths (`params.z <= 0`) are untouched: fixed 3x3 caster filter via
  `shadow_lit`/`cascade_lit`, four-tap map filter, coarse far-map fallback.

## Validation (release profile only; GPU suites serial on target hardware)

- Headless naga parse+validate of both scene shaders (`shader_validation`) 3/3.
- `cargo fmt --all --check`; clippy `-D warnings` on bri-render clean.
- Full `cargo build --release` clean. (Unrelated to this change: the repo had
  been moved from `/run/media/surdavow/sub/Documents/github/...`, and stale
  build-script artifacts for `bri-net` and `cranelift-assembler-x64` carried
  absolute paths of the old location; deleting those two
  `target/release/build/*` directories fixed it.)
- GPU regression on target hardware, small offscreen renders:
  `pcss_sun_shadows` 3/3; `unified_lighting` 3/3 including
  `dynamic_window_shadow_softens_when_pcss_is_enabled` in **both** its
  Bedroom-scale cascade and far-map cases (penumbra width >2x the hard filter,
  edge grain <0.8x); `shadow_occluders::cascade_splits_do_not_cut_a_shadow`.

## Remaining

- No before/after GPU frame-time numbers exist; per the branch's own
  requirement, profile representative terrain and interior scenes with Soft
  Sun Shadows on and off (and note live-object-shadow-heavy scenes for the
  lost short-circuit) before calling this performance-validated.
- Visual check of the fixed filter pattern for moiré/shimmer while panning
  across brick-cast penumbrae; any sampling change must keep the Dynamic
  window-shadow regression green in both cases.
