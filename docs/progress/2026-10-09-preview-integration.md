# 2026-10-09 Preview integration: Enhanced sky, Soft Shading, Gravity Gun model

Owner: the "Review and merge to main" thread (integrator). Max asked for one
preview build to playtest all three together; nothing here lands on main
until he has played it and confirmed each piece by name.

## Branch and PR
- Integration branch `claude/project-thread-psbgg3`, draft PR #34. Not to be
  merged as is: after Max approves, each piece lands on main through
  `python tools/gate.py --push` (squash first; the gate rebases and drops
  merge commits).
- Merged in: Enhanced sky (PR #31, `1f05f69`), Soft Shading and AO
  (`claude/project-thread-5q1fwl`, `ab78990`), Gravity Gun model
  (`claude/project-thread-mnrxit`, `d3a2709`). The sky and shading threads
  have stopped and handed their work to this one.

## Changes made here
- Resolved the sky/shading clash in `crates/client/src/graphics.rs` and
  `app/render.rs` (both settings kept; Original sky is forced first, then
  sky-tinted ambient and AO).
- AO runs between the opaque and blended geometry
  (`SceneRenderer::render_world_split`), so water, glass and see-through
  bricks are no longer darkened; it fades with the camera fog
  (`fog_along`), so fogged creases keep the fog colour.
- Sky-tinted shade never brightens: up faces get the sky's tint at the same
  brightness, down faces darken (0.75), level faces are unchanged.
- Tests: AO shader validates (both depth textures); a render test in
  `crates/render/tests/lighting_environment.rs` checks the split pass alone
  draws exactly as before and AO darkens a crease and brightens nothing.

## Verified (cloud container, lavapipe)
- clippy `-D warnings` on bri-client, bri-render, bri-ui; their tests pass
  except `bri-client/launch::a_startup_failure_tells_the_player...`, which
  needs libxkbcommon-x11 the container lacks (environment, not code).

## Not yet verified / open
- PC session (started 16:38 from this thread) on 4952fd6: off-equals-main
  pixel check per map and lighting mode, real-map renders, GPU timing, gate
  check without push. Its report comes back to this thread.
- Sky PC report (from the sky thread, head b081d4d, RTX 4070 SUPER):
  Enhanced costs about +0.03 to 0.04 ms a frame. Look issues for Max's
  playtest: Bedroom Dark shows black silhouettes against a bright generated
  sky (the sky ignores the map's darkness); strong orange/magenta at
  sunset/twilight; the sun disc is small. Images:
  `BlocklandReImagined-worktrees\sky-real\*_montage.png` on Max's PC. Its
  probe (`sky_probe.rs`) is committed only locally as 26087693 in the
  `enhanced-sky` worktree.
- Windows CI: PR #31 hit software-GPU wait timeouts in shards 2 and 3
  (item_ghost, multiplayer, mirrors past 600 s). Main is also red on
  `persistent_scene::display_colors_match_on_srgb_and_unorm_output_attachments`
  (same timeout; seen again on docs PR #33). Docs-only PR #33 also had
  `bri-render/mirrors` run past 600 s in test (3), with
  `a_portal_the_eye_is_about_to_go_through_shows_what_the_far_side_will`
  failing, so the mirrors timeout happens without any sky code. The
  timeouts look like main's software-GPU flakiness rather than the sky;
  timing `cargo test -p bri-render --test mirrors` on main vs the branch
  would confirm it.
- Known AO limitation: Glow bricks (colour FX 3) still get crease shading;
  excluding them needs a mask the frame does not have yet.

## Landing rule (Max, 2026-10-09 17:08)
- Gravity gun stays off main until Max has tested it; it lives only on its
  own branch and in the preview zip.
- Sky (host opt-in, Original by default) and Soft Shading/AO may land once
  the PC check shows "off" draws exactly like main and CI is green; ask Max
  per PR, naming what was verified.
- Docs PR #33 and housekeeping can land.

## Other landings the integrator is tracking
- Bot HE Grenade fix (`claude/v027-bot-play-85fvra`, includes the paint name
  bar fix): landing through the gate from its own thread, with Max's
  approval. Its `acceptance_unfamiliar` change counts an enemy within 4
  units of a grenade at any point of its life; the integrator recommends
  counting only once the grenade has landed (patch below), as a follow-up.
  Verified in the container (release, bot branch head): with the patch the
  test still passes, landed grenades near an enemy per run are 2, 5, 5 on
  variant 0 and 5, 5, 11 on variant 1.
- The bot fix landed on main as 3c8ed1f (v0.2.7.2) with the looser grenade
  check; the preview branch merged it (d40fbd8, no overlapping files).
- PR #32 closed: its change is on main as 1b26a1d.
- Docs PR #33: approved by Max to land through the gate. Before landing,
  update its "Known after v0.2.7.1" bullet (the HE Grenade fix has now
  landed as 0041b68..3c8ed1f) and its audit line about PR #32 (closed as
  landed). Landing needs `python tools/gate.py --push` on Max's PC.

### Landed-only grenade check (proposed patch for acceptance_unfamiliar)
```diff
diff --git a/crates/chaos/tests/acceptance_unfamiliar.rs b/crates/chaos/tests/acceptance_unfamiliar.rs
index 03a9914..73faa54 100644
--- a/crates/chaos/tests/acceptance_unfamiliar.rs
+++ b/crates/chaos/tests/acceptance_unfamiliar.rs
@@ -915,6 +915,10 @@ fn play(v: Variant, seed: u64) -> (Seen, Vec<u8>) {
             }
         }
         for (id, at) in &live {
+            // Only once it has landed: lying still since the last tick.
+            if !last_seen.get(id).is_some_and(|was| was.distance(*at) < 0.05) {
+                continue;
+            }
             let Some(team) = grenades.get(id).and_then(|t| now.get(t)).map(|b| b.team) else {
                 continue;
             };
```

## Paused (Max, 2026-10-09 17:10)
Max paused all work except the v0.2.7.2 release. State at the pause:
- Preview branch `claude/project-thread-psbgg3` (draft PR #34) carries
  main 3c8ed1f, sky, Soft Shading/AO with fixes, and the gravity gun model.
  Nothing from it is on main.
- PC Remote Control session stopped mid-step. On Max's PC it left:
  worktree `..\BlocklandReImagined-worktrees\preview` at 4952fd6 with an
  uncommitted `lighting_probe` extension (per-run Classic/Unified/Dynamic
  modes, Soft Shading, AO, original/enhanced sky, sun height, MSAA, GPU
  time per stretch), not compiled or pushed; `enhanced-sky` worktree with
  unpushed commit 26087693 (`sky_probe`); `soft-shading` worktree with
  unpushed WIP c816e534 plus uncommitted edits. Nothing was deleted.
- Container: render/client tests on 4952fd6 were stopped part way; the
  client lib (494) and all completed render test files passed; the only
  failure was `launch::a_startup_failure...`, which needs libxkbcommon-x11
  missing from the container.
- Next on resume: finish the probe and the off-equals-main check, real-map
  renders and GPU timing on the PC; update and land PR #33 via the PC gate;
  build the private preview zip (release.yml dispatch, publish=false) and
  send Max a test list; ask Max per PR before sky and Soft Shading land.
