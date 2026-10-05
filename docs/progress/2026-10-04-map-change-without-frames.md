# 2026-10-04 A map change finishes without frames; icon tests write where the build is

## Map change waited for a frame (multiplayer test timed out on Slate)

b55f4508 made finishing a map change wait for `scene_pipelines_ready()`,
which is false while `gpu.gpu_restart` is set. A map change
(`take_prepared_scene`) dropped the renderers and set `gpu_restart`; only
`prepare_render`, at the start of a drawn frame, rebuilt them and cleared it.
So a client that drew no frame after the change never finished it: the
multiplayer test's host (it never draws) stayed on the loading screen
("Timed out waiting for both clients on Slate"), and so would a real
client whose window is minimized or covered (the platform loop skips
`render` when occluded or zero-sized) until it was shown again.

Fix: `gpu_ready` keeps the device, queue and format it was given
(`GpuState::device`, cleared by `gpu_stopped`), and a map change rebuilds the
renderers on it at once, after the new map is set up. `gpu_restart` remains
only for a change made while no device is open. b55f4508's intent is kept:
the compile still runs on a worker and the loading screen still waits for
it, drawing and responsive.

Noted, not changed: there is no wgpu pipeline cache, so every map change
recompiles the scene pipelines (about 20 s with FXC on Windows, measured at
launch); the loading screen now covers that time on a map change.

## Icon tests failed when the target folder is elsewhere

`the_gravity_gun_icon_is_drawn_from_its_model_like_the_printers` and
`an_add_on_tool_icon_is_drawn_from_its_model_like_the_hammers` save their
pictures to `<checkout>/target/`, which does not exist when
`CARGO_TARGET_DIR` points elsewhere (agent worktrees), so `save_buffer`
failed with "No such file or directory". Not from a recent merge. They now
use `testing::look_path`, which writes into `CARGO_TARGET_DIR` when set
(else `target/`) and makes the folder.

## Evidence

Before (tip 2b9fea6c): the multiplayer test failed after 136 s with the
timeout; both icon tests failed with os error 2. After:
`cargo test -p bri-client --test multiplayer` 1 passed (37.7 s);
`cargo test -p bri-client --lib` 471 passed, 0 failed;
`cargo clippy -p bri-client --lib --tests -- -D warnings` clean.
