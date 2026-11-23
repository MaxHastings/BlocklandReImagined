# 2026-11-15 Distant sun shadows: casters persist at range (whole-map layer redraws)

On `codex/pcss_fix`, the final state of the distant sun shadow work (earlier attempts in this
chain were superseded and their notes removed; what shipped is recorded here).
Report (Maxwell): the smooth fade is liked, but "shadows smoothly disable from a distance" - he wants
distant shadows **visible at all times**, low quality at range, never disappearing.

## Diagnosis: the stable layer only ever held map surfaces

The far layer (`far_map_lit`) was drawn once per (map key, sun) with `map_only=true` - just the map's
own surfaces. Live casters were faded out over `[0.75d -> d]` by design in `shadow_lit`. Everything a
player builds in Blockland is bricks, and bricks live in the *caster* layer - so their shadows beyond
the shadow distance had no stable representation at all: they simply stopped existing past the fade.

## Change (Rust only; no shader edits)

The far layer now holds **casters on top of map surfaces**, redrawn whenever its content actually moves:

- `shadow.rs` - `FarMap.drawn` gains the casters' signature (`caster_signature`: scene geometry
  revisions + instance set revisions, which bump only on real changes). `far_map(queue, key, bounds,
  casters_sig)` returns a draw when the sun, map key or caster content changed since the last draw;
  receiver fields still written only while they change. A still frame redraws nothing.
- `scene.rs` - the far target becomes two passes on one view: pass 1 clears and draws all casters
  (static + moving scenes and instances), pass 2 loads that depth and adds the map surfaces
  (`map_only=true`). Occluders stay out of both - they stop shadows, never cast them. The load-op
  rule for a view already cleared this frame now keeps its content for *untiled* targets too (previously
  only tiled ones), so pass 2 loads over pass 1 instead of re-clearing; no existing target pair shared
  an untiled view, so behavior elsewhere is unchanged.

`far_map_lit` itself is untouched: it reads the one far layer exactly as before - that layer just now
contains everything. Because the fit depends only on map bounds and sun (view-independent), zoom and
camera movement still can't perturb distant shadows; the `[0.75d -> d]` hand off from the previous fix
stands, but past it casters are present at coarse texel size instead of gone.

## Test (`tests/distant_shadow_reach.rs`)

Assertion (c) flipped: with a live caster in the scene its shadow **must** now shade the distant face
(`both[0] < open[0] - 30`), was "must not". Zoom-stability checks unchanged and still pass - the far
face reads are entirely the stable layer, which no refit touches.

## Verification (each suite in its own process, sleeps between - VRAM)

- `distant_shadow_reach` 1/1 (with the flipped expectation), `shadow_occluders` 5/5 (still frames draw
  zero shadow layers: the signature gate keeps them free), `pcss_sun_shadows` 3/3, `unified_lighting`
  3/3, `shader_validation` 3/3.
- Release build of `bri-client` only; `--check` smoke RC=0 (pre-existing content fallbacks).

## Cost note

While something casts shadows is moving (a player almost always is), the far layer redraws: one extra
whole-map orthographic pass over caster + map geometry at the coarse far resolution, depth-only. Still
frames and sun-locked scenes pay nothing new; static maps pay only what they paid before. Flagged for
playtest judgment.

## Low quality at range (`FAR_TEXEL_SCALE`)

Report (Maxwell): persistence works, but "distant casted shadows should be low quality" - very distant
shadows deliberately coarse, no regression to the earlier states.

Mechanism: **sub-region fit** in `far_fit`. The whole-map box now fits into a central sub-region of its
layer instead of filling it: ortho x/y half-range and margin are multiplied by `FAR_TEXEL_SCALE` (2.0),
so the same world spans twice as many pixels per axis - i.e. **half the texels**, one coarse texel = two
hardware texels wide. The returned `texel` is that coarse texel's *world* size, so the receiver bias push
(`n*z*1.5`) and all PCSS math stay consistent with what the layer actually stores.

- `shadow.rs` - new `const FAR_TEXEL_SCALE: f32 = 2.0;`; `far_fit` scales margin + view half-range and
  returns the coarse world texel; unit test bound updated to `texel <= span * FAR_TEXEL_SCALE / 2048`
  (corners sit further inside NDC, so the corner asserts still hold).
- **`.w` deliberately unscaled** (`1.0/resolution`, hardware texel): it feeds only the non-PCSS 4-tap
  filter step, which must keep its usual width - scaling it widened hard edges and broke
  `unified_lighting`'s "soft > 2x hard" invariant.
- Both WGSL files (lockstep) - new `const PCSS_FAR_MAX_TEXELS:f32=4.0;`; only the radius clamp in
  `far_map_pcss_lit` uses it. With coarse texels an uncapped search reaches across shadow edges and
  smears false partial shade onto lit faces (`distant_shadow_reach` saw 255 to 231 on an unshaded point).

Value choice: F=4 passed everything except `unified_lighting`
(`dynamic_window_shadow_softens_when_pcss_is_enabled`: the Poisson profile steps at one coarse texel were
grainer than the hard edge, "12 vs 9"). F=2 halves that step; all suites green.

## Verification (each suite in its own process, sleeps between - VRAM)

- lib 85/85, `distant_shadow_reach` 1/1, `pcss_sun_shadows` 3/3, `shadow_occluders` 5/5,
  `unified_lighting` 3/3 (grain assertion now passes), `shader_validation` 3/3.
- Release build of `bri-client` only; `--check` smoke RC=0 (pre-existing content fallbacks).

## Cost note (additive)

The far layer's own resolution is unchanged - it stores the same number of texels, just covering the map
at half density. The redraw cost from the persistence change above stands as described; this adds no new
draws.

## Hand off crossfade + far layer caster reach

Report (Maxwell): persistence and low quality at range are in, but two artifacts remained - a lighter band
of shadow strength that moved with the camera when flying away from tall builds, and sun-side casters (tall
builds, flyers) whose shadows faded out at the hand off and never came back. Dark should stay dark through
the whole transition: one crossfade, not two competing fades.

### The lit band: two separate fades taken with `min()`

Past 75% of the shadow distance the old code ran two independent ramps: the map part faded toward the far
layer (`far_map_lit`) while the caster part (`shadow_lit`) faded out to unshadowed sun, and `min()` combined
them. Where both held a real shadow mid-ramp the min picked the weaker - about half strength at the
crossing. That was the lit band; it sat on the ramp so it moved with the camera as you flew away.

Fix (both WGSL files, lockstep): compute the near result once - map and casters combined exactly as before,
`min(map_cascade_lit(c), shadow_lit_near(c))` - then crossfade that whole result onto the far layer, which
already holds both map surfaces and casters since the persistence change. A shadow present in both layers
stays dark through the entire transition instead of dipping at the crossing; old and new results are
identical at both ends of the ramp (`far_blend` 0 and 1), only the middle changes. New helper
`shadow_lit_near`: the cascade's own caster shadow without the hand off fade - callers that crossfade onto
the far layer blend it themselves. Applied at all three hand off sites: `object_sun`, `relit_sun`,
`dynamic_sun`. The PCSS path is untouched: it already blended whole results (`combined_pcss_lit`), so the
dip fix changes nothing there.

### Casters clipped out of the far layer (`far_fit`)

The far layer's depth range was fitted to the map's own bounds only, but casters are not part of those
bounds: a tall build or flyer can stand well toward the sun of the map's highest surface - inside the
cascades (which reach `CASTER_REACH`, 400 units, for their casters) yet outside the far layer. Their shadow
faded out over the hand off ramp and never came back past it. `far_fit` now extends the ortho near plane by
the same `CASTER_REACH` toward the sun, so those casters stay in the layer at coarse texel size like
everything else past the distance. The returned `texel` is unchanged - it depends only on the x/y extent,
which this doesn't touch; only the depth range grows (slightly coarser depth precision).

### Unchanged

- Classic lighting and baked lightmapped surfaces still fade caster shadows out at the shadow distance.
- Very small casters can still vanish from the far layer's coarse texels - their shadows now fade out
  smoothly across the hand off band rather than showing a light streak.

## Verification (each suite in its own process, sleeps between - VRAM)

- lib 85/85, `shader_validation` 3/3, `distant_shadow_reach` 1/1, `pcss_sun_shadows` 3/3,
  `shadow_occluders` 5/5 (individually - batch runs OOMed on VRAM), `unified_lighting` 3/3.
- Release build of `bri-client` only; `--check` smoke RC=0 ("21 maps").
- Playtest: fly away from a tall build in Dynamic and Unified, PCSS on and off - the hand off should read
  as one crossfade with no lit band, and sun-side casters keep their distant shadows.

## Cost note

None new: same layers, same redraw cadence; `far_fit` just reaches deeper toward the sun (one extra
near-plane offset), at no resolution or texel change.
