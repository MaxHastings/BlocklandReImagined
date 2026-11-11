# 2026-11-10 PCSS redundant sun work (exact-equivalence follow-up)

Follow-up to `2026-11-10-pcss-unified-pass.md` on `codex/pcss_fix`. That note's
remaining item asked for GPU profiling before calling the branch
performance-validated. Reading the shaders for that turned up sun-shadow work
that was computed and then discarded, or computed twice for the same receiver.
This change removes it. No sampling changed: tap counts, radius law, fixed
filter layout, fast paths and `combined_pcss_lit` are untouched, and no PCF or
other approximation was introduced. Edits are in `scene_original.wgsl` and
`scene.wgsl`, kept in lockstep (the two `shadow_coord` bodies differ, so that
patch differs between them, see below).

## What changed

- **`decomposed_lightmap`**: a relit texel ran `sun_visibility` (the caster-only
  PCSS pass) and then overwrote the result with `relit_sun`. When the baked sun
  direction was kept, `relit_sun` called `sun_visibility` again. Relit texels
  now take `relit_sun` alone; non-relit texels still use `sun_visibility`.
- **`object_sun`**: `shadow_coord` ran before the Dynamic-mode hand-off to
  `dynamic_sun`, which recomputes it. The `lighting_mode()==3` check now comes
  first.
- **`shadow_coord`**: the far cascade (a mat4 multiply plus the PCSS slope
  math) was built for every receiver but only read when `blend>0`. It is now
  built only where blend can be above zero. `scene_original.wgsl`: the last
  `CASCADE_BLEND` share of the cascade's range. `scene.wgsl`: the same, plus
  within `2*PCSS_MAX_TEXELS` of the near map's edge when PCSS is on, because
  that file also blends on edge distance. The guard is conservative (a
  degenerate zero-width band still builds it), and every `c.far` read in both
  files was already gated on `c.blend>0.0`.
- **Glint pass (`fs_main`, decal/surface specular)**: the sun share was
  evaluated for the diffuse light and then again, with identical inputs, for
  the glint. The first result is now kept in `sun_cached` and reused. This
  needed `dynamic_illumination` split into a wrapper and
  `dynamic_illumination_lit(position, normal, sun)` so Dynamic mode can supply
  the sun it already has; the wrapper keeps every other caller unchanged.

## Equivalence

Classic and Unified results are bit-identical to before. Dynamic mode's diffuse
path is bit-identical. The Dynamic glint on normal-mapped surfaces now reuses
the diffuse path's normalized normal instead of re-normalizing the already
normalized one, which can differ at float-rounding level. Nothing else about
the shadow value changes.

## What this does not address

The blocker search in `combined_pcss_lit` runs for every sun-facing receiver
inside the cascades, shadowed or not: 2 core gathers plus 16 ring loads in each
of two layers, 34 fetches, against roughly 13 before the branch (9 caster PCSS
plus the 4-tap map filter). The cost table in the unified-pass note counts
shadowed receivers only, so this is likely the main reason Soft Sun Shadows
costs more than 0.4.5 in open sunlit scenes. It was left alone because no
change to the ring is exact except skipping it where no texel can block.

Options for a later change:

- **Min-depth skip (exact).** Build a 1/16-resolution min-depth texture for the
  caster and map layers after the shadow render. In the shader, two gathers
  over the receiver's tile give the nearest depth across the whole search
  footprint; if it is at or beyond `depth_minus_bias` minus the slope bound
  (`(abs(slope.x)+abs(slope.y))*10.0/size`), return 1.0, which is what the
  search would have returned. Needs a reduction pass and bind-group changes in
  Rust.
- **Gather-based ring (not exact).** Eight `textureGather` positions per layer
  instead of 16 point loads: half the fetches and better coverage, but the
  radius estimate shifts, so the window-shadow regression must be re-checked.

To confirm the search dominates before building either: temporarily set
`PCSS_MAP_SEARCH_TAPS` to 4 and compare GPU time with Soft Sun Shadows on in an
open sunlit scene.

## Validation

- Headless naga parse+validate of both patched shaders, using the same
  front-end, validator flags and shader prefix as `shader_validation`
  (naga 30.0.1, standalone harness): both OK. The unpatched shaders also pass
  the same harness.
- Not run: `cargo test` GPU suites (`pcss_sun_shadows`, `unified_lighting`,
  `shadow_occluders`), fmt and clippy. The change is WGSL only and these need
  the target hardware.
- No before/after frame times exist for this change.

## Remaining

- Run the GPU suites above on target hardware.
- Profile representative terrain, interior and lightmapped-relit scenes with
  Soft Sun Shadows on and off. The savings land where the redundancy was
  (relit lightmaps, Dynamic receivers, glossy decals and surfaces, receivers
  outside cascade blend zones) and will be small in scenes without them.
- Decide on the search-cost options above from that profile.
