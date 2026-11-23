# 2026-11-23 Scene sun shadow handoff

PR #449 identified the missing soft map shadow in Dynamic lighting and the
loss of live caster shadows at the end of the fitted cascades. The scene
variants now share cascade selection and sun filtering in `sun_shadows.wgsl`.
Dynamic and Unified combine map and caster visibility before the handoff.
Classic and baked receivers whose original sun direction is retained use
the existing fitted caster fade.

The PCSS blocker search estimates a common penumbra in world units. The
filter still checks caster depth at its own taps. A thin caster may land
between the sparse search samples, so the search alone cannot justify a
map-only filter. A tap with four blocked caster texels can skip the map
gather. Near maps keep their original fitted depth range.

The far map uses the map bounds and sun, with extra depth toward the sun
for tall casters. Caster culling uses this same fit. The final quarter of
the fitted range blends to the fixed map, and the cache tracks its actual
map and caster geometry revisions. Static frames reuse its depth array
layer. A caster change redraws the coarse layer from both geometry sets.
The single distant depth layer approximates visibility at low resolution.
The separate noncasting occluder rule applies inside the fitted cascades.

The offscreen tests cover Dynamic map softness, the handoff, zoom stability
and removal of cached caster depth. Windows passed the renderer library,
`shader_validation`, `distant_shadow_reach`, `unified_lighting`,
`pcss_sun_shadows` and `shadow_occluders`, plus all-target Clippy for
`bri-render` and `bri-client`. The repeated-terrain extent follows the
active tiles, so it can refit when the tile set changes. Long-range visual
and performance acceptance remains a player check.
