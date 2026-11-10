# 2026-11-10 PCSS dynamic sun early-outs

Follow-up to the map-cascade PCSS work on `codex/pcss_fix` (34bdc0b36). The
branch's own commit message flagged that Dynamic lighting can evaluate both
the live-caster shadow filter and the new map-shadow PCSS for the same
receiver, so combined cost could exceed either path alone. `dynamic_sun()` in
both scene shaders was the only PCSS consumer missing the early-outs this
branch had already added to `object_sun()`.

`dynamic_sun()` in `scene_original.wgsl` and `scene.wgsl` now mirrors that
structure: compute the dynamic (player/object) shadow first and return 0 when
the receiver is fully covered, so a solid live-caster shadow no longer pays
the map PCSS search/filter (~49 texture reads); receivers fading out of the
last cascade (`strength<=0`) skip the discarded cascade PCSS; and a zero map
result returns before the final `min`. No tap counts or quality constants
changed. The change is exact-equivalent for all reachable inputs: it relies
only on `mix(x, y, 0) == x`, `min(map, 0) == 0` with non-negative visibility
values, and `shadow_lit`'s own guards (it returns 1 when the cascade or its
strength is absent).

Validation, all release-profile per the session constraint:

- Headless naga parse+validate of both scene shaders (`shader_validation`).
- `cargo fmt --all --check`, clippy `-D warnings` on bri-render, and a clean
  `cargo build --release -p bri-render`.
- GPU regression suite on target hardware (small offscreen renders, serial):
  `pcss_sun_shadows` 3/3, `unified_lighting` 3/3 including the Dynamic
  window-shadow PCSS regression in both its Bedroom-scale cascade and far-map
  cases (penumbra width >2x and edge grain <0.8x of the hard filter), and
  `shadow_occluders` 5/5 including `cascade_splits_do_not_cut_a_shadow`.

Remaining: no before/after GPU frame-time numbers exist; profile
representative terrain and interior scenes in-game with Soft Sun Shadows on
and off before considering the branch performance-validated.

The unified-pass follow-up mentioned above was implemented afterwards; see
`2026-11-10-pcss-unified-pass.md`. Note its interaction with this note's
dynamic_sun early-outs: when Soft Sun Shadows is on, the combined pass
subsumes the caster filter. That initially removed the fully-covered
short-circuit (a covered receiver paid ~82 ops instead of ~41); the unified
note's exact coverage fast path restores it (~66 ops, bit-identical result).
The faded-out (`strength<=0`) and zero-map early-outs are preserved.
