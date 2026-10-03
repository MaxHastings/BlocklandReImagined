# 2026-10-03 Dynamic cube refresh preserves runtime illumination

Maxwell reported occasional black screen flicker with Dynamic lighting during
the candidate playtest. This entry records a concrete source-level blackout
mechanism and a narrow correction; it does not yet establish that this caused
the exact playtest symptom.

Previously any map geometry cache-key change cleared the global cube readiness
flag until all recovered lights' six faces had refreshed. Dynamic's shader
converted the unavailable `-1` sentinel to zero illumination. At 24 lights and
24 faces per frame this removes all recovered lamp illumination for the first
five refresh frames. Batch hiding and terrain transform changes can invalidate
that key. The existing one-lamp offscreen test fits its six faces in one frame
and could not expose this gap.

Cube freshness and availability are now separate. Changed geometry retains
previous runtime geometry shadows only when all projector positions/radii and
indices are identical, and refreshes replacement faces under the unchanged
24-face budget. Changed light projectors, explicit forgetting, a missing map or
a new renderer discard that availability. New sources render current lamps
temporarily unshadowed until a complete geometry cube cohort exists. This can
temporarily admit light during initial warmup; no old baked illumination,
visibility, residual or cleanup data is used. Geometry changes can retain up to
five frames of old geometry shadow faces while the bounded refresh completes.
Classic and Unified's illumination/shadow branches remain unchanged.

Authored regressions:

- `shadow::tests::geometry_cube_refresh_preserves_available_lighting_and_bounded_work`:
  six initial frames, six changed-geometry frames, unchanged work budget,
  changed-projector rejection and missing-map reset.
- `dynamic_cube_refresh_never_blacks_out_current_lamps` in `unified_lighting`:
  128px offscreen floor lit by 24 descriptor-only lamps, no ambient/sun;
  warms the whole cache, hides an unrelated batch to change the exact geometry
  key, checks all six refresh frames against the warmed pixel, then checks six
  first-use frames after a source rebind. The pre-fix code should fail on the
  first warmed-cache invalidation frame, independently of initial warmup.

Source validation: direct `rustfmt --edition 2024 --config skip_children=true`
on the three changed Rust files and `git diff --check` passed. No Cargo or GPU
test has run for this patch in this worktree. Root integration owns serialized
validation in the already cached primary target; requested commands are:

```sh
cargo test -p bri-render --lib geometry_cube_refresh_preserves_available_lighting_and_bounded_work
cargo test -p bri-render --test unified_lighting dynamic_cube_refresh_never_blacks_out_current_lamps -- --exact --test-threads=1 --nocapture
cargo test -p bri-render --test unified_lighting -- --test-threads=1
cargo clippy -p bri-render --all-targets -- -D warnings
```

Next: root runs focused validation and the existing lighting regressions,
records frame evidence and fixes any Dynamic failure without a baked fallback.
Interactive confirmation remains Maxwell's. No visible game was launched and
the source installation was not modified.
