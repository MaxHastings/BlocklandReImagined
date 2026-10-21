> **Historical note. Written 2026-10-19 (the file's date in the cloud project folder, `audits/test-cost-cut-list-2026-10-19.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Test cost cut list, 2026-10-19

Source of times: `gate-timings.json` from the PC gate (`.bri-gate`, saved 2026-10-14 by the gate itself): seconds per test binary, 480 binaries, **5,843 binary-seconds in all (97 min, matching the 99 min in pipeline-timing.md)**. Per-test times inside a binary are not recorded there, so verdicts are per binary, with the expensive tests named from reading the code. Nothing was run in the cloud: bri-client needs a GPU and the original files. Times are additive binary-seconds under the gate's parallel load, not wall clock.

The top 48 binaries make up 80% of the time. bri-client alone is 56% (3,278 s).

## The biggest single lever: every `synthetic_and_content!` test runs twice

47 client test files use `support/variants.rs`, which emits each body as a synthetic test and again as an ignored `content` test on the real v20 content. The gate runs `--include-ignored`, so every one boots the app and compiles shaders twice. Those 47 binaries are 2,867 s (49% of everything). The synthetic run proves the code; the content run only re-proves it on the real data. Verdict: **content variants run in the release-only gate** (`tools/release_gate`, the packaged-build check), except the handful whose whole point is real data: `content_root`, `app_flow`, `release_smoke`, `v20_fidelity`, `default_add_ons`, `old_saves`, `brick_face_coverage`, `save_corpus`. I could not split the two halves per binary (no per-test times), so I count the content half as 45% of each binary it touches. Please check this against a `--report-time` log from the PC before landing.

## Ranked table (slowest first, top 80% of time)

Verdict key: DELETE, REPLACE (smaller check named), RELEASE-ONLY (leaves the per-landing/merge gate, runs at the release gate), KEEP. "Saves" is binary-seconds.

| # | Binary | s | What it proves | Verdict | Why | Saves |
|---|---|---|---|---|---|---|
| 1 | client/app_soak | 512 | World-change path (rockets, brick return, deaths, map change) builds and uploads nothing whole; a tap inside one frame still fires | REPLACE | `long_soak_builds_and_uploads_nothing_whole` (5 rounds, "several minutes") is the bulk. Keep the 1-round synthetic `steady_play...` and the tap test (guards the delayed brick-return hitch). Long soak and both content variants go to `check.py soak` / release | ~430 |
| 2 | client/item_ghost | 368 | Picked-up brick item stays a faded ghost until the ordinary respawn; single player and LAN guest | REPLACE | 2 scenarios x 2 variants. Keep the single-player synthetic (tick-judged, cheap to assert). LAN guest-sees-ghost is replicated item state; RELEASE-ONLY. Content variants RELEASE-ONLY | ~280 |
| 3 | client/app_item_render | 346 | Held tools render from eye and original mounts; bricks in hand are grey | REPLACE | `held_items_render` (24 s) and `item_rendering` already cover held items. Keep only `bricks_in_hand_render_the_grey_brick...` synthetic; `native_core_tools...` and content variants RELEASE-ONLY | ~280 |
| 4 | net/all_features_soak | 297 | Bots, portals, vehicles, scripted Add-On, two QUIC clients, nothing panics, clients stay in step | RELEASE-ONLY | Written to mirror `crossover_soak` (says so in its header); both pass only on "nothing broke". Keep crossover as the one net soak | ~295 |
| 5 | client/vehicle_first_person | 168 | First-person camera when riding vehicles | KEEP synthetic, content RELEASE-ONLY | Pose composition is also covered by `seat_pose_composition` (7 unit tests) | ~85 |
| 6 | addon-import/bundled_in_game | 144 | The release bundle's originals host and play end to end | RELEASE-ONLY | It tests packaging of a release bundle; per-landing code changes cannot break it | ~144 |
| 7 | chaos/bot_soccer_match | 141 | Matches never lose the ball, stick, or fight where weapons do nothing; goals happen | REPLACE | 3 kits x 3 seeds x 150 s. One seed, 90 s, and drop the share checks that are `< 0.5` of everything (clumped, circling, idle, ignoring...) which only catch what `never()` already catches. Max's rule: absolutes only | ~100 |
| 8 | chaos/bot_battle_perf | 140 | Nothing. Header: "diagnostic only: timings are reported, never thresholded" | DELETE from the gate | Move to `tools/`/soak as a profiling example, not a test | ~140 |
| 9 | net/crossover_soak | 129 | Four real clients, 16 bots, 150 ms link, join/download, prediction stays in step | KEEP | The one real end-to-end net soak | 0 |
| 10 | client/lib | 116 | 740 unit tests | KEEP, needs per-test times | One 116 s binary hides a few slow unit tests (shader/app boot). Ask for `--report-time` | ? |
| 11 | client/mirror_render | 113 | Mirror Add-On shows what is behind the camera in Bedroom and Slopes, High vs Off | REPLACE | `render/mirrors.rs` (66 s, 13 tests) already proves planar mirrors. Keep one synthetic Bedroom check | ~90 |
| 12 | chaos/weapons_fuzz | 102 | Hostile world answers; weapon events stay finite, no panic | REPLACE | Real bug class, so keep, but 1 seed in the gate and the large sweep in soak | ~90 |
| 13 | chaos/bot_combined_perf | 99 | Nothing. "No timing assertions" | DELETE from the gate | Diagnostic, same as #8 | ~99 |
| 14 | client/hud_regions_render | 98 | HUD regions draw | content RELEASE-ONLY | Synthetic keeps the proof | ~45 |
| 15 | client/multiplayer | 96 | Host + client over loopback | content RELEASE-ONLY | | ~43 |
| 16 | client/remote_poses | 96 | Remote player's swings/look match what the swinger draws | content RELEASE-ONLY | | ~43 |
| 17 | client/screen_topologies | 90 | Screens drive single player, LAN host, internet host | content RELEASE-ONLY | Three hosted games per variant | ~40 |
| 18 | render/map_lighting | 85 | Lighting recovery against stock maps | KEEP synthetic, content RELEASE-ONLY | Shader/lighting regression guard | ~40 |
| 19 | client/brick_audit | 84 | Nothing. It writes renders for the Python v20 reference renderer | DELETE from the gate | A generator for `tools/brick_reference.py`, not a test | ~84 |
| 20 | client/body_attachments_render | 78 | Body attachments render | content RELEASE-ONLY | | ~35 |
| 21 | chaos/bot_gauntlet | 77 | Scenario bots: kills, not stuck/idle, no friendly fire (`sane`, all `< 0.5`) | KEEP, drop 3 `#[ignore]` tuning tools from `--include-ignored` | `all_dials_on` ("the push gate runs it"), `off_switches`, `dial_sweep`, `heeding_allies...` are measurement tools | ~40 |
| 22 | client/player_types_render | 75 | Horse draws as a horse; Fuel Jet energy shows | content RELEASE-ONLY | | ~35 |
| 23 | client/player_name | 67 | Typed Avatar name reaches the server | REPLACE | 540 lines to prove one string moves through ui -> net. A ui `field_flow` check plus a sim name test is 10x smaller | ~55 |
| 24 | client/add_on_join | 66 | Host offers only enabled Add-Ons; joiner downloads what it lacks | content RELEASE-ONLY | Real bug class, keep synthetic | ~30 |
| 25 | render/mirrors | 66 | Planar mirror semantics on an offscreen adapter | KEEP | The mirror proof (see #11) | 0 |
| 26 | client/app_flow | 65 | App loads, weather, flow on real content | KEEP | The "works on real content" probe | 0 |
| 27 | client/map_shapes | 64 | Kitchen palms visible to host and guest | content RELEASE-ONLY | | ~29 |
| 28 | client/save_corpus | 62 | Tricky saves host cleanly | KEEP | Already only runs when saves code changes | 0 |
| 29 | client/gltf_body | 59 | glTF bodies | content RELEASE-ONLY | | ~26 |
| 30 | client/view_jitter | 57 | Body and held item hold still against a turning camera | content RELEASE-ONLY | Real past bug | ~25 |
| 31 | chaos/session_chaos | 54 | Bots do everything at once; nothing panics or holds NaN | REPLACE | 4 seeds x 1200 ticks to 1 seed in the gate, 4 seeds in soak | ~40 |
| 32 | chaos/acceptance_unfamiliar | 54 | A package no code knows the names of works (platform principle) | REPLACE | 2 variants x 3 seeds; absolutes only. Gate runs 1 seed, soak all | ~35 |
| 33 | chaos/bot_firefight | 53 | Nothing now. "Regression investigation" of a v0.2.0 crash on real content | DELETE | An investigation, not a guard; `bot_hardening` covers crowds | ~53 |
| 34 | chaos/bot_hardening | 51 | Crowds and places that made bots misbehave, each found by a probe and kept | KEEP | Guards real past bugs | 0 |
| 35 | render/physical_surface | 46 | Normal map, glow, bare metal | KEEP | Shader guard | 0 |
| 36 | chaos/event_fuzz | 43 | Random wrench programs keep the host stepping, work bounded | REPLACE | 1 seed in the gate, more in soak | ~25 |
| 37 | client/night_qa | 43 | Nothing in the gate: an overnight matrix of every map x mode | DELETE from the gate | Manual `tools/` run | ~43 |
| 38 | addon-import/slayer | 40 | Slayer teams and CTF rules (49 tests) | KEEP | Cheap per test, rules guard | 0 |
| 39 | client/world_items | 35 | World items | content RELEASE-ONLY | | ~16 |
| 40 | client/brick_draw_budget | 34 | Draw counts for 44k bricks (not time) | KEEP | Deterministic counts guard the hitch class | 0 |
| 41 | net/bandwidth | 34 | Byte budgets of typical scenes | KEEP | Stable numbers | 0 |
| 42 | chaos/net_chaos | 33 | Bots over QUIC; no drop, no NaN | DELETE | Same property as `crossover_soak` plus `session_chaos` | ~33 |
| 43 | net/sized_join | 33 | 150 MB throttled join shows moving progress | REPLACE | 150 MB to 30 MB; the progress logic is the same | ~25 |
| 44 | client/stresslab_flow | 31 | Stress Lab hosts, mine, creeper, save | content RELEASE-ONLY | Known past flake already fixed at the cause | ~14 |
| 45 | client/brick_respawn | 31 | Brick respawn | content RELEASE-ONLY | | ~14 |
| 46-48 | camera_snapshot_freshness, horse_riding_render, shadow_render | 29, 29, 28 | Real past bugs / render | KEEP, content RELEASE-ONLY for the last two | | ~25 |
| rest | the other client `synthetic_and_content` binaries (1,191 s together, outside the rows above) | | | content RELEASE-ONLY | Same policy | ~530 |

Left alone: everything not listed (all under 28 s, 20% of time together).

## Totals

| Verdict | Binaries/tests | Binary-seconds |
|---|---|---|
| DELETE (from the gate) | 6 (bot_battle_perf, bot_combined_perf, brick_audit, bot_firefight, night_qa, net_chaos) | ~452 |
| REPLACE with something much smaller | 10 | ~1,015 |
| RELEASE-ONLY (content variants, the two soak/bundle files) | most of bri-client | ~1,590 |
| KEEP | 14 listed | 0 |
| **Total** | | **~3,060 of 5,843 (52%)** |

97 min of binary time becomes about 46. The heavy pool is the part that sets the pass time (3,200 s on three slots), and nearly all of this comes out of it.

## What this does not claim

- The per-binary times are an older gate run (file dated 2026-10-14) under 16-job contention. The ranking holds; absolute seconds move.
- "Content half = 45%" is an estimate. A `--report-time` log from the PC would turn it into a measured figure.
- `client/lib` (116 s) needs per-test times before I can name which unit tests to cut.
- `first_person_view` and `weapon_frame_probe` (named in pipeline-timing.md's top ten) are not in the engine repo at this commit; they may live on the train branch.
- Risk named: moving content variants out of the per-landing gate means a change that only breaks real v20 data is caught at the release gate. They were never what caught logic bugs, but it is a real trade.
