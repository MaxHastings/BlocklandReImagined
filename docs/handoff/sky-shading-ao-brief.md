# Brief: finish Enhanced Sky, Soft Shading and Ambient Occlusion, then build Max a preview

Repo: `MaxHastings/BlocklandReImagined`. Read `AGENTS.md` first and follow it
(testing boundary, gate, worktrees, no original game content in commits).

## Goal
Finish the sky, Soft Shading and AO work, verify it, and give Max a private
preview zip he can playtest. Do **not** include the Gravity Gun work and do
**not** merge anything to main.

## Rules Max set
- Nothing goes to main without Max saying yes to that specific PR, by name.
  When he does, land it only with `python tools/gate.py --push` from his PC
  (never `--no-verify`, never force-push main).
- Push every meaningful change to a GitHub branch straight away.
- Enhanced Sky is a host opt-in: Environment > **Enhanced Sky** is off by
  default, so every map keeps its original sky. Players can always force the
  original with Options > Sky = **Original** (`$pref::Video::Sky` = 1).
- Soft Shading and Ambient Occlusion are two Options toggles
  (`$pref::Video::SoftShading`, `$pref::Video::AmbientOcclusion`). They
  default on but only act in the Unified and Dynamic lighting modes. Classic
  lighting, or both toggles off, must draw exactly like main.
- Max does all interactive playtesting. Do not launch a visible game window
  or drive mouse/keyboard; use headless tests and offscreen renders only.

## Where the work is
Integration branch `claude/project-thread-psbgg3` (draft PR #34), based on
main 3c8ed1f (v0.2.7.2). Its first-parent history:

| Commit | What |
|---|---|
| 18a8f65 | Merge Enhanced Sky (PR #31, `1f05f69`) |
| 0a19fe3 | Merge Soft Shading and AO (`c6b4940`), settings clash resolved |
| ac8be97 | Merge Soft Shading clippy fix (`ab78990`) |
| 4952fd6 | AO only on opaque world, fades in fog; sky tint never brightens |
| c4d7a2e | **Gravity Gun model merge: exclude this** |
| later | progress notes and the merge of main 3c8ed1f |

To work without the Gravity Gun, create your own branch from the
integration branch and revert that one merge:

```sh
git fetch origin claude/project-thread-psbgg3
git checkout -b codex/sky-shading-preview origin/claude/project-thread-psbgg3
git revert -m 1 --no-edit c4d7a2e
```

Afterwards `git diff origin/main --stat` should show no
`packages/showcase/gravity-gun-*` or `crates/client/src/items.rs` changes.
The handoff note `docs/progress/2026-10-09-preview-integration.md` on the
branch has the full history; add your own progress file per
`docs/progress/README.md`.

## Already done and verified
- Sky and shading settings both kept (`crates/client/src/graphics.rs`,
  `crates/client/src/app/render.rs`): Original sky is applied first, then
  sky-tinted ambient and AO.
- AO runs between opaque and blended geometry
  (`SceneRenderer::render_world_split` in `crates/render/src/scene.rs`), so
  water, glass and see-through bricks are not darkened. It fades with the
  camera fog (`fog_along` in `crates/render/src/ambient_occlusion.wgsl`).
- Sky-tinted shade (`hemisphere()` in `crates/render/src/scene.wgsl`) never
  brightens: up faces get the sky's tint at equal brightness, down faces
  darken to 0.75, level faces are unchanged.
- Tests: the AO shader validates for both depth texture kinds
  (`crates/render/tests/shader_validation.rs`). A render test in
  `crates/render/tests/lighting_environment.rs` checks the split pass alone
  draws exactly as before, and that AO darkens a crease without brightening
  anything.
- In a Linux container (software GPU) clippy `-D warnings` is clean on
  bri-client, bri-render and bri-ui. The client lib tests (494) and every
  render test that ran pass. The only failure, `launch::a_startup_failure...`,
  is a missing system library there, not code.
- Bot grenade fix is on main; nothing to do for it here.
- Sky cost on Max's RTX 4070 SUPER: about +0.03 to 0.04 ms a frame.

## Open items, in order
1. **Off equals main.** On Max's PC, render each map below in Classic,
   Unified and Dynamic with Soft Shading off, AO off and Sky = Original, on
   main and on your branch. They must match pixel for pixel. Also confirm
   Classic matches main with everything on. `lighting_probe` refuses
   Dynamic today; extend it (or `perf_probe`/`scene_snapshot`) so one run
   can set mode, Soft Shading, AO, Original/Enhanced sky, sun height and
   MSAA. A previous attempt at this probe is uncommitted in Max's
   `..\BlocklandReImagined-worktrees\preview` worktree; reuse it if useful.
2. **Real-map renders, Original vs Enhanced sky and shading/AO on vs off:**
   Skylands, Slate, Bedroom seen through a window and from outside, Bedroom
   Dark, Kitchen, and a dense build save. Save PNG montages and note anything
   wrong.
3. **Known sky look problems to fix or tune:**
   - Bedroom Dark shows black silhouettes against a bright generated sky,
     because the generated sky ignores the map's darkness.
   - Strong orange/magenta at sunset and twilight.
   - The sun disc is small or invisible.
   Earlier montages: `..\BlocklandReImagined-worktrees\sky-real\*_montage.png`
   on Max's PC. That thread's `sky_probe.rs` is only in the local commit
   26087693 in the `enhanced-sky` worktree.
4. **Floor brightness:** check that floors and ground indoors and outdoors
   are no brighter than main with Soft Shading on.
5. **AO risks:**
   - Glow bricks (colour FX 3) still get crease shading. Excluding them
     needs a mask the frame does not have yet.
   - Re-check see-through surfaces, water and glass on real maps, to confirm
     the split pass fixed them there too.
6. **GPU timing:** per-stretch GPU time ("world", "occlusion",
   "world blended") on Max's PC, at 1080p and 1440p, with MSAA on and off.
   Report the AO cost.
7. **Windows CI:** PR #31 and PR #34 hit software-GPU wait timeouts
   (item_ghost, multiplayer, `bri-render/mirrors` past 600 s). Docs-only PR
   #33 hit the same mirrors timeout, so it is probably main's flakiness, not
   the sky. Confirm by timing `cargo test -p bri-render --test mirrors` on
   main vs your branch. Fix any real slowdown. Never skip or disable a test;
   a test already failing on main goes in `tools/gate-known-failures.toml`.
8. Run `python tools/gate.py` (no push) on Max's PC and fix anything it
   finds.

## Build the preview zip
Run the Release workflow (`.github/workflows/release.yml`) manually on your
branch with `version` = `preview-2026-10-09-sky-shading` (or today's date)
and `publish` = **false**. It then builds the Windows, macOS and Linux zips
as a workflow artifact only, with no GitHub release and no tag on main.
Give Max the Actions run link.

## What Max should test (send him this list with the zip)
1. Host a map, open Environment and turn **Enhanced Sky** on and off: the
   sky should change only when it is on.
2. With Enhanced Sky on, Options > Sky = Original should bring back the
   map's own sky.
3. Bedroom Dark with Enhanced Sky: no black cut-outs against a bright sky.
4. Sunset and twilight with Enhanced Sky: colours not too orange or pink.
   Check the sun disc is visible.
5. Options > Soft Shading and Ambient Occlusion, in Unified and Dynamic
   lighting: corners and creases get soft shade, and floors are no brighter.
6. Water, glass, see-through and Glow bricks: none should get dark smudges
   or outlines.
7. Fog: creases should fade into fog with no dark lines.
8. Turn everything off, or switch to Classic: it should look exactly like
   v0.2.7.2.
9. Frame rate on a big build with everything on vs off.

When Max approves, land Enhanced Sky and Soft Shading/AO as separate
squashed commits through the gate, each only after he names it.
