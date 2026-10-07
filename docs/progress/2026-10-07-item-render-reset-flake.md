# 2026-10-07 Held-item render test: compare frames once lighting has landed

`app_item_render native_core_tools_render_from_eye_and_original_mounts::content`
failed on the PC gate only while other builds loaded the machine ("GPU
recreation changed static third-person item render"), and passed alone.

## Cause

On the Bedroom the Unified lighting needs the map's compatibility bake,
which runs on a worker. Frames draw Classic lighting until a frame's
`light_volume.upload` polls the finished bake, and then Unified
(`lighting.rs` `mode`, `poll_compatibility`). The test compares frames it
draws seconds apart (first-person before and after a tool, third-person
before and after a GPU reset). Alone, the bake landed before the first
frame. On a loaded machine it could land between two compared frames, so
the frame after the reset was lit differently from the one before it.

## Change

Both tests in `crates/client/tests/app_item_render.rs` draw frames after
`gpu_ready` until `App::map_lighting_settled` (the same wait `app_soak`
uses), before any frame they compare. No tolerance changed; no runtime
change.

## Evidence

- Reasoned from the code, not reproduced: the bake is the work I found
  that lands on a worker and changes a drawn frame with no step between
  the two frames compared (pipelines `wait`; chunks upload in the frame).
  The test needs
  the v20 content, which the cloud checkout does not have, so it was not
  run here. `cargo clippy --workspace --tests -- -D warnings` clean.
- Next: the PC gate under load is the proof; remove nothing from
  `gate-known-failures.toml` (this test is not listed there).
