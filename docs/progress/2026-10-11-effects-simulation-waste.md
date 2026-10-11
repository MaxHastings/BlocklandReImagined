# 2026-10-11 Unseen brick emitters sleep; the effects follower reads only changed bricks

Thread "Effects simulation waste" (branch `claude/v0210-perf-effects-njxcy7`,
PR #69). Findings 1 and 2 of the v0.2.10 performance waste audit
(`/mnt/project-files/perf/waste-audit-v0.2.10.md`).

## What was wrong

- Every emitter brick on the map was stepped every frame and every live
  particle integrated, sampled and frustum-tested whether or not any view
  could see it: on the Bedroom stack 43,916 particles simulated for 320
  drawn at spawn.
- `WorldEffects::sync` walked all ~200,000 bricks to rebuild the effect
  attachments on every world update that changed any brick (up to 20 a
  second in a fight or while a build streams in).

## What changed

- `EffectsWorld::cull_sources` (fx-runtime), called in `render.rs` with
  every view the frame draws (player, mirror and portal planes, probe faces)
  before any snapshot. A still emitter whose conservative reach (top speed
  and largest acceleration over its particles' longest life, plus
  placement, offset and largest sprite) misses every view for
  `SLEEP_UNSEEN` (1 s) goes dormant: its particles are dropped and it emits
  nothing, keeping its clock. When any view can reach it again it wakes in
  the same frame and back-fills what it would have emitted over that
  longest life, each particle aged to now, so the visible cloud is a full
  one. Lights, moving or paused sources, emitter-sized sources and worlds
  with portals never sleep.
- Exact smaller cuts on the same path: drag decay once per particle
  definition per step, no second key search for particles without emitter
  keys, `combine_effect_frames` moves the single non-empty list through.
- `WorldEffects::sync` follows an imbl diff from the last world it saw
  (shared structure skipped); a new map or palette, or a failed follow,
  rebuilds whole. Warnings are kept per brick.
- Frame-trace spans "effects sync" and "effects cull"; `entity_counts`
  reports dormant emitters.

## Evidence

`large_build_perf`, run D setup on Max's PC (RTX 4070 SUPER, DX12,
3440x1440, Max's settings, Bedroom with the four stacked saves, 200 frames,
`BRI_PERF_PROFILE=inside`), main `cf9c761` -> `3ce3f94`:

| View | frame p50 (p95) ms | record | update | GPU | particles live | drawn |
|---|---|---|---|---|---|---|
| spawn | 6.99 (7.49) -> 5.84 (6.29) | 2.50 -> 2.05 | 0.91 -> 0.40 | 3.29 -> 3.06 | 43,916 -> 550 | 320 -> 316 |
| overview | 11.39 (12.57) -> 11.10 (12.78) | 5.88 -> 5.65 | 1.14 -> 1.13 | 3.94 -> 3.92 | 46,586 -> 47,199 | 43,489 -> 47,088 |
| inside | 11.25 (12.52) -> 10.26 (10.94) | 5.63 -> 5.06 | 1.26 -> 1.04 | 3.92 -> 3.66 | 47,855 -> 38,660 | 35,781 -> 36,251 |

The overview draws 8% more sprites after: before, the benchmark's clouds
were still filling (live particles rose 43.9k -> 46.6k -> 47.9k across the
three views), while a woken emitter starts from its full steady cloud. The
overview sees nearly every emitter, so it gains little; the rest of its
cost is drawing ~45k visible sprites. In the inside profile
`WorldEffects::advance` (8.9%) and `EffectsWorld::integrate` (4.1%) left
the top 30, as did `queue.write_buffer` and `RtlAllocateHeap`.

`one_change_in_a_big_world` (200k bricks, 488 emitters, 151 lights, one
changed brick per update) on Max's PC: whole walk p50 3.58 ms (max 7.65)
-> followed p50 0.004 ms (max 0.04). Cloud container: 5.3 ms -> 0.011 ms.

Content-free `many_rooms_perf` (488 emitters, ~49k particles, cloud
container): spawn view 2.17 -> 0.07 ms effects CPU; overview 4.16 -> ~3.9.

Work-count and correctness tests: `unseen_emitters_sleep_and_what_is_seen_stays_identical`
(identical sprite list in view with culling on), `a_waking_emitter_refills_its_cloud_before_it_is_seen`,
`lights_moving_sources_and_portal_worlds_never_sleep`,
`a_world_update_looks_only_at_the_bricks_it_changed` (one change, one
brick looked at; attachments equal a whole rebuild).

## Not done

The audit appendix's plausible effects items (re-sampling per mirror or
probe view, flare draw-call splits, per-frame flare line-of-sight casts)
were left: unmeasured and not glaring.
