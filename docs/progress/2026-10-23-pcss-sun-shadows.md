# 2026-10-23 PCSS Sun Shadows (Options > Graphics)

New option **Soft Sun Shadows** (PCSS; `$pref::Video::PcssSunShadows`, off by
default), a checkbox under the radios of the Shadow Quality box. With it on, sun shadow edges are sharp where
the caster touches a surface and soften with the caster's distance above it,
instead of one fixed-width filter.

## What changed

- `crates/render/src/shadow.rs`: `ShadowSettings::pcss` and `PCSS_SUN_TAN`
  (0.03, the tangent of the sun's apparent half angle, several times the real
  sun's so softening shows at build scale). Both reach the shader in the unused
  `params.z` / `params.w` of the shadow uniform; the layout is unchanged. A
  changed setting rebuilds the renderer through the existing Shadow Quality
  path (the uploaded world is kept).
- `scene.wgsl` and `scene_original.wgsl` (identical edits, so Classic lighting
  gets it too): `pcss_lit` replaces the 3x3 filter in `cascade_lit` when the
  flag is on. The four texels under the receiver (one gather) and 12
  `textureLoad` taps out to `PCSS_MAX_TEXELS` (10) over the caster layer find
  blockers, nearer taps counting more (none: lit). Searching as far as the
  filter can reach makes the soft edge spread to both sides of the true edge
  instead of ending in a hard outline, and the four texels under the receiver
  keep thin shadows from being missed; their mean distance along the sun times `PCSS_SUN_TAN`, in texels and
  clamped to 1.5..10, is the radius of a 16-tap rotated disc of bilinear
  comparisons that use the same caster and occluder rules as the 3x3 filter,
  so a player on a brick tower still does not shade the floor under it. The
  disc is turned per pixel by interleaved gradient noise (`sun_pixel`, set at
  the top of `shade_surface` / `fs_main`). Wide discs need the receiver's own
  plane: `cascade_coord` computes its depth slope analytically from the normal
  and the cascade matrix (no derivatives, which the filters avoid), clamped at
  tan 10, and every tap compares against the plane's depth at its offset.
- `scene.wgsl` only: `map_pcss_lit` does the same for the map-layer sun
  shadows (Unified and Dynamic lighting, sun through windows onto builds),
  with a 4-texel blocker search (`PCSS_BASE_SEARCH_TEXELS`), a filter radius
  three times the object one (up to 15 texels) and hardware comparison taps. `scene_original.wgsl` has
  no map layer, so it is unchanged by this. The far-map shadows keep the old
  filter.
- `crates/ui/src/screens/options.rs`, `crates/client/src/graphics.rs`: the
  checkbox (added to the Shadow Quality section under its radios, labeled
  Soft Sun Shadows), the pref, and `Graphics::shadows.pcss`. Only the Ultra
  quality preset turns it on; the others turn it off.
- `crates/render/src/shadow.rs`: `ShadowSettings::pcss` and `PCSS_SUN_TAN`.
  Shadow map sizes are unchanged (the first draft raised Best to 4096; that
  quadrupled shadow memory for everyone, so it was reverted on merge).

## Evidence

Headless lavapipe renders, `crates/render/tests/pcss_sun_shadows.rs`, both
shaders. The figures below were measured with the first version, which
searched 10 texels for blockers; the 4-texel search, the map-layer path and
the new Best size came after and were checked by playing, not by new
numbers:

- Edge across a floor, caster 0.3 and 4 units above it: the ordinary filter is
  0.05 units wide at both heights; PCSS stays 0.05 at 0.3 and is 0.17 at 4; lit
  and umbra levels match.
- A caster 12 units up (the filter at its widest): the edge is 0.35 wide,
  centered on the true edge, and fades without a step (largest jump between
  neighbouring samples 0.04 of the dark-to-lit range; a search shorter than the
  filter gave 0.11, the hard outline this test guards against).
- A ramp tilted 50 and 65 degrees under a roof edge: PCSS gives a wide clean
  penumbra and a fully lit side beside it. With the receiver-plane slope
  zeroed the same test fails (the penumbra collapses to about half width and
  speckles), so the slope term is load-bearing.

Commands: see the PR description for the exact `cargo test`, `fmt` and
`clippy` results.

## Not verified

- The new checkbox sits inside the Shadow Quality section; that box's
  authored size is not in this repo. Run `cargo test -p bri-ui
  native_graphics_panel_offscreen -- --ignored` on a PC with content and check
  the label is not clipped.
- Cost is roughly 4x the sun-shadow texture reads of the 3x3 filter, and wide
  penumbrae show some grain (no temporal accumulation); the knobs are the
  `PCSS_*` consts in the shaders and `PCSS_SUN_TAN`.
- Candidate follow-up: a softness slider.
