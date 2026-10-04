# 2026-10-04 App soak of the world-change path and five stutter fixes

Branch `fix/soak-and-stutters` from `53f05cf` (v0.2.3). Headless only: no
window, audio device or OS input; Maxwell owns interactive playtests.
No production code in `brick_fade.rs`, `world_chunks.rs`, `app/frame.rs`,
`app/mod.rs`, the wrench/minigame screens, sim or net server was changed
(other lanes own them).

## Why a soak

The existing perf probes time `ChunkedWorld::update` in isolation. They skip
`app/net_events.rs` (world log, chunk jobs, query mirror) and
`app/render.rs` preparation, which is how the brick-return hitch slipped
through. `crates/client/tests/app_soak.rs` drives the normal `App`, every
frame rendered offscreen, through ordinary UI actions to the hosted server
over loopback QUIC, on the made-up content root (no generated content):

1. host single player, create a Brick Damage mini-game (hammer, rocket, gun;
   respawn 1 s, brick respawn 2 s), load a 6x6 wall of all four fixture brick
   shapes in every palette colour in front of the player (36 bricks);
2. rocket the wall (warm-up), then again, waiting with the hammer out
   (`showBricks`) until every brick has returned and all debris expired;
3. a death plus click-to-respawn, every tool, first/third person: once as
   warm-up, then again;
4. Admin Change Map to the second fixture room, use every tool (warm-up),
   then again.

`App::work_counters()` (new, `bri_client::perf::WorkCounters`) reports
deterministic work counts: whole scene uploads and textures made
(`SceneRenderer::upload_counts`, new), shared/palette geometry uploads,
world-item model builds/uploads, debris looks built, chunk jobs and chunks
rebuilt, music-brick and hidden-outline whole-world passes and bricks
examined. Every steady phase (the repeats) must make zero whole scene
uploads, textures, model builds, look builds and whole-world passes; a blast
cycle may start at most 4 chunk jobs; hidden outlines may examine at most 8
changes per wall brick; the new map reads its world at most once.

Run (about 60 s in a debug build on lavapipe; part of `cargo test`):

```sh
cargo test -p bri-client --test app_soak
# long: BRI_SOAK_ROUNDS rounds of each steady action (default 5)
BRI_SOAK_ROUNDS=10 cargo test -p bri-client --release --test app_soak -- --ignored long_soak
```

Each run writes its phase table to the test's `app-soak` output folder
(`phases.txt`).

## Results

Before = `origin/main` plus only the counters (branch `soak-before`, commit
`7a2a6b9`, not for main: it counts the old `sync_music` and the old render
hidden-outline rescan). After = this branch. Same synthetic run, 6x6 wall.

| Steady phase | Before | After |
|---|---|---|
| Blast to debris expiry and return | 4 scene uploads, 10 textures, 4 item models rebuilt, 2 music + 3 hidden-outline whole-world passes | 0 / 0 / 0 / 0 + 0 (72 changed bricks examined) |
| Death, respawn, every tool, both views | 11 scene uploads, 24 textures, 11 item models rebuilt, 1 hidden-outline pass | all 0 |
| Every tool on the new map | 6 scene uploads, 18 textures, 6 item models rebuilt | all 0 |

Before fails the soak at its first steady check ("blast to expiry and
return: 4 whole scene uploads with 10 textures"); after passes. The world is
tiny here, so a whole-world pass is cheap; on a 100k-brick save each one is
100k bricks on the main thread per replica revision.

## Fixes (each with a guard that fails before and passes after)

1. **World-item models** (`world_items.rs`). `prepare` took the cache and put
   back only models drawn this frame, so a weapon put away for one frame
   (switch, death/respawn, view toggle, projectile again) re-meshed,
   re-textured and re-uploaded. Undrawn models now stay idle, most recently
   drawn first, within `WorldItemLimits::models`. `cached_models` still
   counts drawn models; `idle_models` the kept rest. Guard
   `tests/world_items.rs a_model_missing_for_a_frame_is_not_built_again`:
   before `model_builds` 2 vs 1; after passes, and GPU `model_uploads` stays.
2. **Avatar layout changes** (`avatar.rs`). `upload` dropped the GPU scene
   on any re-layout (skis, hands hidden by an image, translucent paint) and
   re-uploaded face/decal/surface textures. It now uploads geometry on the
   existing textures (`upload_geometry_shared`), falling back to a whole
   upload only if materials differ. Guard
   `avatar::tests::a_restructured_body_keeps_its_textures`: before
   (scenes, textures) (6, 36) vs (1, 6) over 5 re-layouts; after passes.
3. **Whole-world scans per revision.** `ClientAudio::sync_music` walked every
   brick and cloned sound names each revision: now `MusicBricks` follows
   `Bricks::diff` (guard `audio::tests::music_bricks_follow_...`). The render
   pass re-scanned every brick for hidden outlines whenever a revision cleared
   `hidden_uploaded`: now `hidden_outlines::HiddenOutlines` keeps the hidden
   set from `Bricks::diff`, as `rule_regions::Outlines` does, and rebuilds
   only when the outlined set, palette or an outlined brick changes; the
   per-revision invalidation in `net_events.rs` is gone (guard
   `hidden_outlines::tests::...`). Both guards are on the new types, so
   "before" for them is the soak's before table.
4. **Debris look cache** (`brick_debris.rs`). With 64 looks kept, each new
   look evicted the least recently used look not yet visited this frame,
   including live looks whose bodies came later, which were rebuilt the
   same frame. Kept looks are now all marked first. Guard
   `brick_debris::tests::a_full_look_cache_evicts_only_looks_nobody_wears`:
   before 128 builds vs 96; after passes. The palette material-list clone per
   look build was left: it happens in `world_chunks::build_brick` (another
   lane's file) and is now rare, once per new look.
5. **Details panes** (`ui/screens/addons.rs`, `modes.rs`). Every `UiUpdate`
   reset the scrolled details pane to the top; now only a new selection or
   new text does (as `report.rs`). Guard `crates/ui/tests/details_scroll.rs`:
   before scroll 0 vs 120; after passes.

## Verification

All results above were produced with a per-worktree
`[profile.dev.package.<crate>] codegen-units = 37` config, because the
shared target directory otherwise mixes other worktrees' workspace-crate
artifacts. `clippy -D warnings --all-targets` on bri-client, bri-ui and
bri-render is clean (with an empty `CLIPPY_CONF_DIR`: the main checkout's
untracked `clippy.toml` is not part of this tree). bri-client lib tests:
444 pass; the two Add-On icon tests fail here only because they write to
`<worktree>/target/`, which does not exist with a shared target dir.
`world_items`, `app_item_render`, `held_items_render`, `app_flow`,
`clear_bricks_flow`, `details_scroll` and the bri-ui suite pass.

## Next

Change Map rebuilds every world-item model (the adapter resets with the
session); that is load-screen work, not steady play, and was left. The long
soak and a real-GPU Windows run are the next evidence.
