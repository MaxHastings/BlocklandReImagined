# 2026-10-01 Unified lighting no longer crashes the software renderer

The fixtures lane found that lavapipe (Mesa 25.2.8, the software Vulkan
driver) crashed with SIGSEGV whenever the default Unified lighting drew a
player or item lit by map lights. To work around it, six synthetic client
tests pinned Classic lighting (repro:
`/mnt/project-files/synthetic-fixtures/lavapipe-unified-crash.md`).

- Cause, from gdb on the crash: the faulting instruction is the JIT-compiled
  `lamp_slot` in `scene.wgsl`. That function looped over the lamp slots
  (`for s < lamp_params.x`) comparing `shadows.lamp_lights[s]` against the
  light, once for every map light at every pixel, inside the divergent
  `map_light_sum` loop. llvmpipe kept the `shadows` uniform-buffer
  descriptor per lane, under the mask of the lanes active when it loaded it.
  In that inner loop it read the descriptor for a lane that had been
  inactive at load time, got a null pointer, and crashed (heap corruption
  with 128-bit vectors). Making `lamp_slot` return -1 alone stopped the
  crash.
- Fix: `shadow.rs` now fills a per-map-light table each frame
  (`light_slots`, 24 entries, replacing the 4-entry `lamp_lights`).
  `lamp_slot` is a single read of that table. This is also cheaper on every
  GPU: there is no slot search per light per pixel. There is no driver
  special case, and the lighting is the same.
- The six tests (`held_items_render`, `distant_render` (since deleted), `mirror_render`,
  `player_types_render`, `vehicle_first_person`, `view_jitter`) run their
  synthetic variants on Unified again. `support::gpu::pin_classic_lighting`
  is removed. Before the fix, `held_items_render` died with SIGSEGV here on
  lavapipe. After it, all six pass.
