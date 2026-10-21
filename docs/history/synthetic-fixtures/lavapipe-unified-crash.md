> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `synthetic-fixtures/lavapipe-unified-crash.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# lavapipe crash in the Unified lighting path

Environment: Linux container, Mesa 25.2.8-0ubuntu0.24.04.2 (mesa-vulkan-drivers,
lavapipe/llvmpipe, the only adapter), wgpu Headless via crates/client/tests/support/gpu.rs.
Debug test build: `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`.

## Repro
1. Remove the `$pref::Video::Lighting=0` pin from the synthetic variant of
   `crates/client/tests/held_items_render.rs` (or `distant_render.rs`).
2. `cargo test -p bri-client --test held_items_render`
3. The process dies with SIGSEGV (signal 11) in a JIT-compiled function on an
   `llvmpipe-N` rasterizer thread (gdb: `0x00007fff... in ?? ()`, no symbols).
   With `LP_NATIVE_VECTOR_WIDTH=128` it aborts with `malloc(): invalid size (unsorted)`,
   which points to heap corruption, not a plain bad read. `LP_NUM_THREADS=0/1`
   still crashes, on the test thread.

## Which settings trigger it
held_items_render (synthetic root, 640x480, single player):
- The first capture (first person, ball in hand) is fine.
- The second capture crashes. It is taken after ToggleFirstPerson: third person, so the avatar plus the held ball.
- `$pref::Video::Lighting` 1 (Unified), 2 (the default) and 3 (Dynamic) crash. 0 (Classic) does not.
- `$pref::Video::AntiAliasing=0` (no 4x MSAA) also avoids it with Lighting=2.
- `$pref::ShadowQuality=4` (no shadows) and `$pref::Video::Reflections=0` do NOT avoid it.

distant_render (synthetic root: slopes mirrors and a jeep, 60-120 units):
- Crashes with Lighting=2 even when AntiAliasing=0.
- Lighting=0 passes (both tests, about 26 s).

mirror_render and shadow_render crashed once the fixture room got an open
skylight (sky environment, no ceiling). With the ceiling back, mirror_render
passes on the defaults.

## Notes
- The difference between Classic and Unified is the `lighting_mode()>=1` branches of
  crates/render/src/scene.wgsl: map_light_sum / light_seen / lamp_reach / lamp_taps
  (textureSampleCompareLevel on the shadow_map array with computed layers),
  cube_seen, and the light_grid storage reads.
- I found no disabled naga bounds checks in bri-render.
- Not settled: whether this is a lavapipe bug or an out-of-bounds index the renderer
  produces that real GPUs tolerate (e.g. the robustBufferAccess path, where wgpu
  leaves buffer checks to the driver).
- Real-GPU behaviour was not tested here.
