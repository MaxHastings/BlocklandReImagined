# 2026-10-11 Far map shadows, per-pixel AO and a staged skylight rebuild

Thread "Shadows and shading performance" (branch `claude/project-thread-x4iib6`).
Maxwell asked for two things, industry solutions only: the far side of the
Bedroom no longer lit flat when shadows are below Best, and the frame-rate
cost of Ambient Occlusion and Soft Shading cut where there was low-hanging
fruit. The before/after evidence below was captured headless with
`lighting_probe` on Maxwell's PC at fixed cameras, resolution and settings.

## What was wrong

- Past the cascade distance (Medium: 160 units, Best: 320) the Dynamic sun
  term fell back to fully lit, so a room larger than the cascade reach showed
  its far end at full sun brightness with no shadow at all. Best hid it only
  because its 320-unit reach covers the whole Bedroom.
- The SSAO pass declared `@builtin(sample_index)`, which forces sample-rate
  shading under MSAA 4: the AO kernel ran four times per pixel.
- The AO exclusion mask drew every scene as a full extra geometry pass to
  decide which pixels are emissive, although almost no bricks are.
- Soft Shading rebuilt all nine skylight depth maps in one frame whenever the
  eye left the cached cell, which is the hitch felt while walking or building.

## What changed

- **Far map shadow layer** (`crates/render/src/shadow.rs`, `scene.rs`,
  both scene shaders). One extra depth layer in the shadow array holds a
  whole-map orthographic sun depth map, fitted to the map bounds plus a
  margin and redrawn only when the map, sun or fit changes (`far_fit`,
  `ShadowMaps::far_map`). The receiver samples it where the cascades end
  and inside their 10% fade band (`far_map_lit`, four compare taps, normal
  offset of 1.5 texels); inside the cascades nothing changes. This is the
  standard "static far cascade" every cascaded renderer ships.
- **Per-pixel AO** (`ambient_occlusion.wgsl`): the sample index is gone;
  AO reads sample 0 of the depth and runs once per pixel.
- **Emissive-only AO mask** (`scene.rs`): a scene is marked emissive when it
  has an Unlit or UnlitOverlay material or a Glow colour FX vertex; the mask
  pass draws only those scenes, and nothing when there are none.
- **Staged skylight rebuild** (`sky_exposure.rs`): a cell change draws a
  budgeted number of directions per frame (default one) into pending
  textures and swaps when all nine are done; the first field still draws
  whole so a fresh view is never unshaded. `SceneRenderer::sky_exposure_budget`
  lets tests draw all nine at once.
- `lighting_probe` gained `BRI_SHADOWS=best|high|medium|low` and a
  `save:<path>` scene that loads a `.world.json` save on its own map.

## Evidence (Maxwell's PC, headless)

Setup: BEFORE `d557cd5` (origin/main `b650033` plus only the probe
switches) vs AFTER `59aa99e`, real "Crazy Collage Madness" save in the
Bedroom (about 250k bricks), 1920x1080, MSAA 4, `BRI_FRAMES=400`, two corner
cameras at 90% room height (roomA eye (-40,405,200), roomB eye (180,405,200),
both looking at (95,330,98)), branches alternated per cell. The labelled
sheets (BEFORE left, AFTER right) and the full table stayed on Maxwell's PC:
`C:\Users\Maxwell\Desktop\Games\BlocklandReImagined-worktrees\shading-after\probe-out\sheets\roomA-dynamic-medium-off.png`
(plus `roomB-dynamic-medium-off.png`, `roomA-dynamic-best-off.png`) and
`...\shading-after\probe-out\real-summary.txt`. The `shading-before`
worktree beside it holds the baseline build and its `target/` can be deleted
with `python tools/clean_targets.py --apply --keep shading-after`.

Picture (Dynamic, Medium shadows, roomA): mean luminance of the far half of
the frame 100.2 before, 76.0 after; the near half 100.5 to 90.8. The far wall,
door and poster wall now carry the sun shadow instead of flat brightness.
Best shadows are pixel-identical before and after (mean diff 0.01). Unified
is unchanged by design.

Total GPU p50 ms, BEFORE -> AFTER (roomA / roomB):

| Cell | roomA | roomB |
|---|---|---|
| Unified, Best, AO and soft off | 4.64 -> 4.63 | 4.29 -> 4.30 |
| Dynamic, Best, off | 4.48 -> 4.70 | 4.26 -> 4.09 |
| Dynamic, Medium, off | 3.90 -> 3.92 | 3.77 -> 3.85 |
| Dynamic, Medium, AO and soft on | 5.99 -> 5.14 | 6.28 -> 5.66 |

Stretches with AO and soft on: occlusion 1.15 -> 0.62 (roomA) and
1.30 -> 0.66 (roomB); sky exposure 0.58 -> 0.59; sun shadows 0.22 -> 0.22.
The far map costs nothing measurable once drawn.

Dense sunlit build: "Afghanistan DM" drawn on Skylands, 1080, Unified Best,
four street cameras, total GPU p50 ms BEFORE -> AFTER with AO and soft on:
4.19 -> 2.47, 4.12 -> 2.44, 5.20 -> 2.68, 3.90 -> 1.81. Off: unchanged.

Tests: `cargo test -p bri-render` CPU tests pass here; all GPU render tests
(`lighting_environment` with `sky_exposure_budget(9)` and the rest) passed on
Maxwell's PC. Clippy and fmt clean.

## Not done or not reproduced

- The diagonal banding Maxwell saw in Afghanistan DM on Skylands did not
  appear in the probe's four views with Soft Shading on or off. His exact
  camera and lighting settings are needed to reproduce it. Candidates if it
  does: the sky exposure receiver offset (0.025 units against roughly 0.65-unit
  texels) or the cascade bias.
- A player-height Bedroom camera (eye z 340) at the corner looks into
  furniture, so the floor-level before/after was not captured; a different
  standing position would be needed.
- The CPU side of long frames belongs to the "Frame spikes and performance
  evidence" thread (PR #39); only GPU stretch times were measured here.

## Commands

```
BRI_SHADOWS=medium BRI_TIME=1 BRI_FRAMES=400 BRI_VARIANTS=soft,ao \
  cargo run --release -p bri-client --bin lighting_probe -- \
  "save:C:\Users\Maxwell\AppData\Local\BlocklandReImagined\saves\<map>\Crazy Collage Madness.world.json"
BRI_MAP=skylands BRI_TIME=1 cargo run --release -p bri-client --bin lighting_probe -- afghanistan_dm
python tools/clean_targets.py --apply --keep shading-after
```
