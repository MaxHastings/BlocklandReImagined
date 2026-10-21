# History

Historical notes, plans, reviews and audits that used to live only in the project's cloud folder, kept here so they are in git. Each file starts with a note giving its date. Nothing here is current guidance; for that read the rest of `docs/`.

Branch bundles (the full git history of old branches) are not in git; see `branches/` for what they hold and which work has landed. Unlanded patches are in `shelved-patches/`.

## Folders

| Folder | Files | What |
|---|---|---|
| `audits/` | 4 | Bug-pattern, bandwidth, test-cost audits |
| `bots/` | 19 | Bot and NPC design notes, reviews, edge cases |
| `branches/` | 0 | Branch cleanup records and the landed-check |
| `capabilities/` | 1 |  |
| `catch-up/` | 1 |  |
| `cleanup/` | 1 |  |
| `codex-handoff/` | 1 |  |
| `events/` | 2 |  |
| `fifteen-minute/` | 1 |  |
| `gravity-gun-model/` | 4 | Gravity gun model source (script + .blend) |
| `loose/` | 14 | Notes from the project folder root |
| `notes/` | 1 |  |
| `perf/` | 3 | Performance audits |
| `perf-audit/` | 1 | Performance opportunities |
| `ports/` | 2 |  |
| `release/` | 8 | Release notes, merge plans, definition-of-done |
| `shared-bricks/` | 2 |  |
| `shelved-patches/` | 0 | Patches not on main |
| `synthetic-fixtures/` | 1 |  |
| `tech-debt/` | 3 | Tech-debt plan and definition-of-done |
| `tier-tactical/` | 1 |  |
| `total-conversion/` | 1 |  |
| `v0.2.5/` | 1 |  |
| `v0.2.6/` | 57 | v0.2.6 bot plan, design docs and lane reviews |
| `v0.2.7/` | 2 |  |
| `v0.3.0/` | 5 |  |
| `v0.3.1/` | 1 |  |
| `v027/` | 2 |  |
| `v029-addon-compat/` | 1 |  |

## Files

| File | Date | Size | Title |
|---|---|---|---|
| [audits/bug-patterns-2026-10-06.md](audits/bug-patterns-2026-10-06.md) | 2026-10-06 | 8 KB | Bug patterns, round 2 (2026-10-06) |
| [audits/reported-bugs-2026-09-27.md](audits/reported-bugs-2026-09-27.md) | 2026-09-27 | 7 KB | Reported bugs vs main (checked 2026-09-27 ~23:30Z) |
| [audits/test-cost-cut-list-2026-10-19.md](audits/test-cost-cut-list-2026-10-19.md) | 2026-10-19 | 10 KB | Test cost cut list, 2026-10-19 |
| [audits/test-suite-under-10-plan-2026-10-19.md](audits/test-suite-under-10-plan-2026-10-19.md) | 2026-10-19 | 5 KB | Plan: gate under 10 minutes of tests and under 10 of compile |
| [bots/bot-ideas.md](bots/bot-ideas.md) | 2026-09-30 | 6 KB | Blockhead Bots: where they are and where they could go |
| [bots/bot-performance-2026-10-11.md](bots/bot-performance-2026-10-11.md) | 2026-10-11 | 6 KB | Bot performance on Afghan DM: measurements |
| [bots/collage-32-bot-lag-2026-10-13.md](bots/collage-32-bot-lag-2026-10-13.md) | 2026-10-14 | 6 KB | Crazy Collage Madness, 32-bot deathmatch: where the CPU goes |
| [bots/foundation-check-2026-10-06.md](bots/foundation-check-2026-10-06.md) | 2026-10-06 | 16 KB | Bot foundation check, 2026-10-06 |
| [bots/human-feel-2026-10-06.md](bots/human-feel-2026-10-06.md) | 2026-10-06 | 12 KB | How the bots come across as human, 2026-10-06 |
| [bots/human-variation-candidates.md](bots/human-variation-candidates.md) | 2026-10-06 | 7 KB | Human variation from what bots already know: candidates, 2026-10-06 |
| [bots/mood-design-note.md](bots/mood-design-note.md) | 2026-10-07 | 8 KB | Design note: one mood instead of many dice (v0.2.6 or later) |
| [bots/next-version.md](bots/next-version.md) | 2026-10-07 | 4 KB | Bot ideas for after v0.2.4 |
| [bots/npc-behaviour-review.html](bots/npc-behaviour-review.html) | 2026-10-06 | 38 KB |  |
| [bots/npc-edge-cases.md](bots/npc-edge-cases.md) | 2026-10-05 | 47 KB | NPC edge cases and simplification audit |
| [bots/recording-bot-sessions.md](bots/recording-bot-sessions.md) | 2026-10-12 | 3 KB | Recording a game with bots so it can be debugged later |
| [bots/recordings/2026-10-12-match-1-notes.md](bots/recordings/2026-10-12-match-1-notes.md) | 2026-10-12 | 2 KB | match-1 (2026-10-12, v0.2.10 f4ce0ae): what the recording shows |
| [bots/removal-candidates.md](bots/removal-candidates.md) | 2026-10-06 | 9 KB | What to remove from the bots, 2026-10-06 |
| [bots/review-2026-10-06-act-stage.md](bots/review-2026-10-06-act-stage.md) | 2026-10-06 | 8 KB | Bot review: act stage and soccer scoring, 2026-10-06 |
| [bots/review-2026-10-06-round3.md](bots/review-2026-10-06-round3.md) | 2026-10-06 | 112 KB | Bot review, round 3: edge cases, seeing why, tests, fair and fast (2026-10-06) |
| [bots/review-2026-10-07-final.md](bots/review-2026-10-07-final.md) | 2026-10-07 | 9 KB | Final check of the released bot branch (ba8af8c), 2026-10-07 |
| [bots/robotic-tells.md](bots/robotic-tells.md) | 2026-10-05 | 15 KB | Bot tells that read as robotic on camera (v0.2.4 showcase) |
| [bots/second-opinion-2026-10-05.md](bots/second-opinion-2026-10-05.md) | 2026-10-05 | 19 KB | Bot work: second opinion (2026-10-05) |
| [bots/vehicles-and-calls-design.md](bots/vehicles-and-calls-design.md) | 2026-10-11 | 9 KB | Bots, vehicles and calls: design note (2026-10-11) |
| [capabilities/plan.md](capabilities/plan.md) | 2026-10-07 | 12 KB | Gameplay capabilities and vanilla cleanup: phase 1 plan |
| [catch-up/main-since-v0.1.15.md](catch-up/main-since-v0.1.15.md) | 2026-10-04 | 3 KB | What changed on main since v0.1.15 (e05af0df) — read-only catch-up |
| [cleanup/cleanup-pass-2026-09-29.md](cleanup/cleanup-pass-2026-09-29.md) | 2026-09-29 | 7 KB | Stability and cleanup pass (2026-09-29) |
| [codex-handoff/sky-shading-ao-brief.md](codex-handoff/sky-shading-ao-brief.md) | 2026-10-09 | 7 KB | Brief: finish Enhanced Sky, Soft Shading and Ambient Occlusion, then build Max a preview |
| [events/minigame-events-proposal.md](events/minigame-events-proposal.md) | 2026-10-01 | 13 KB | Making mini-games from the wrench |
| [events/rule-system-brief-review.md](events/rule-system-brief-review.md) | 2026-10-02 | 6 KB | Review: the "Rule/Event System Overhaul" brief |
| [fifteen-minute/findings.md](fifteen-minute/findings.md) | 2026-10-20 | 1 KB | 15-minute mini-game path: partial findings (parked 2026-10-20) |
| [gravity-gun-model/export.py](gravity-gun-model/export.py) | 2026-10-09 | 3 KB |  |
| [gravity-gun-model/geom.py](gravity-gun-model/geom.py) | 2026-10-09 | 7 KB |  |
| [gravity-gun-model/opus/gravity-gun.blend](gravity-gun-model/opus/gravity-gun.blend) | 2026-10-09 | 121 KB |  |
| [gravity-gun-model/render.py](gravity-gun-model/render.py) | 2026-10-09 | 3 KB |  |
| [loose/branch-archive.md](loose/branch-archive.md) | 2026-10-01 | 1 KB | Archived GitHub branches (2026-10-01) |
| [loose/bundle-list-3saves.md](loose/bundle-list-3saves.md) | 2026-10-02 | 22 KB | Shared brick packs across three saves |
| [loose/filming-kit.md](loose/filming-kit.md) | 2026-10-05 | 4 KB | Filming kit for the v0.2.4 video |
| [loose/platform-door-closers.md](loose/platform-door-closers.md) | 2026-09-27 | 28 KB | Platform door-closers audit |
| [loose/platform-principles.md](loose/platform-principles.md) | 2026-09-27 | 16 KB | Platform principles |
| [loose/release-notes-v0.1.11.md](loose/release-notes-v0.1.11.md) | 2026-10-01 | 2 KB | BlocklandReImagined v0.1.11 |
| [loose/release-notes-v0.1.12.md](loose/release-notes-v0.1.12.md) | 2026-10-01 | 2 KB | BlocklandReImagined v0.1.12 |
| [loose/round1-soccer-main.md](loose/round1-soccer-main.md) | 2026-10-05 | 2 KB | Bot watch round 1: Soccer Field Goo, slot 1, v0.2.4 (main 43c7b178) |
| [loose/round2-bot-watch.md](loose/round2-bot-watch.md) | 2026-10-06 | 5 KB | Bot watch round 2: v0.2.4 (main 43c7b178) vs bot lane tip af324cc |
| [loose/showcase-design.md](loose/showcase-design.md) | 2026-09-28 | 6 KB | Gravity Gun and Steel Ball: current design |
| [loose/table.md](loose/table.md) | 2026-10-06 | 6 KB |  |
| [loose/unmerged-worktrees-archive.md](loose/unmerged-worktrees-archive.md) | 2026-10-05 | 1 KB | Unmerged worktree archive (2026-10-05) |
| [loose/v0.1.12-findings.md](loose/v0.1.12-findings.md) | 2026-10-02 | 76 KB | v0.1.12 playtest log findings |
| [loose/worktree-inventory.md](loose/worktree-inventory.md) | 2026-09-28 | 5 KB | BlocklandReImagined worktree inventory (2026-09-27, ~23:30 local) |
| [notes/buffer-memory.md](notes/buffer-memory.md) | 2026-10-14 | 5 KB | How Blockland ReImagined allocates buffer memory |
| [perf/large-build-rendering-recommendation.md](perf/large-build-rendering-recommendation.md) | 2026-10-12 | 5 KB | Large-build rendering: what to do for v0.2.11 |
| [perf/waste-audit-2-load-paths.md](perf/waste-audit-2-load-paths.md) | 2026-10-12 | 15 KB | Wasted-work audit 2: loading, hosting, menus and map changes |
| [perf/waste-audit-v0.2.10.md](perf/waste-audit-v0.2.10.md) | 2026-10-12 | 78 KB | Performance waste audit for v0.2.10 |
| [perf-audit/performance-opportunities-2026-09-28.md](perf-audit/performance-opportunities-2026-09-28.md) | 2026-09-29 | 18 KB | Performance opportunities outside the open lanes (2026-09-28) |
| [ports/magazine-seam.md](ports/magazine-seam.md) | 2026-10-01 | 4 KB | Magazine seam (shared by Tier+Tactical, Adventure, Sniper and any port) |
| [ports/rules-companion.md](ports/rules-companion.md) | 2026-10-01 | 4 KB | Host rules for imported Add-Ons (the `-rules` companion) |
| [release/release-notes-v0.2.10.md](release/release-notes-v0.2.10.md) | 2026-10-12 | 7 KB | New |
| [release/release-notes-v0.2.9.md](release/release-notes-v0.2.9.md) | 2026-10-11 | 3 KB | New |
| [release/v0.1.0-alpha-notes.md](release/v0.1.0-alpha-notes.md) | 2026-09-29 | 3 KB | Blockland ReImagined v0.1.0 (alpha) |
| [release/v0.2.10-definition-of-done.md](release/v0.2.10-definition-of-done.md) | 2026-10-11 | 6 KB | v0.2.10 definition of done |
| [release/v0.2.10-merge-plan.md](release/v0.2.10-merge-plan.md) | 2026-10-12 | 19 KB | v0.2.10 merge plan |
| [release/v0.2.9-ask-audit.md](release/v0.2.9-ask-audit.md) | 2026-10-11 | 18 KB | v0.2.9: what Max asked for vs what shipped |
| [release/v0.2.9-merge-plan.md](release/v0.2.9-merge-plan.md) | 2026-10-11 | 2 KB | v0.2.9 merge plan (drafted 2026-10-11, nothing executed) |
| [release/v0.2.9-pc-script.md](release/v0.2.9-pc-script.md) | 2026-10-11 | 3 KB | v0.2.9 PC script (for a session on Max's PC, repo: C:\Users\Maxwell\Desktop\Games\BlocklandReImagine |
| [shared-bricks/bundle-list-2of3.md](shared-bricks/bundle-list-2of3.md) | 2026-10-02 | 7 KB | Default brick bundle, two-of-three rule (v0.1.13) |
| [shared-bricks/bundle-list.md](shared-bricks/bundle-list.md) | 2026-10-02 | 4 KB | Shared missing bricks: default bundle list (v0.1.13) |
| [synthetic-fixtures/lavapipe-unified-crash.md](synthetic-fixtures/lavapipe-unified-crash.md) | 2026-10-01 | 2 KB | lavapipe crash in the Unified lighting path |
| [tech-debt/definition-of-done.md](tech-debt/definition-of-done.md) | 2026-10-01 | 2 KB | v0.1.11 definition of done |
| [tech-debt/plan.md](tech-debt/plan.md) | 2026-10-01 | 11 KB | Tech debt: what keeps slowing the lanes down, and the plan (v0.1.11) |
| [tech-debt/v0.1.11-definition-of-done.md](tech-debt/v0.1.11-definition-of-done.md) | 2026-10-01 | 2 KB | v0.1.11: definition of done (proposed 2026-10-01, defaults until Max says otherwise) |
| [tier-tactical/tt-design-notes.md](tier-tactical/tt-design-notes.md) | 2026-10-01 | 3 KB | Tier+Tactical Kai: design notes (our own summary, no original code) |
| [total-conversion/total-conversion-audit.md](total-conversion/total-conversion-audit.md) | 2026-09-29 | 14 KB | Total conversion audit |
| [v0.2.5/finish-plan.md](v0.2.5/finish-plan.md) | 2026-10-06 | 10 KB | v0.2.5 finish plan |
| [v0.2.6/locomotion-design-note.md](v0.2.6/locomotion-design-note.md) | 2026-10-07 | 7 KB | Locomotion lane: design note for the next three items |
| [v0.2.6/review-acceptance-run.md](v0.2.6/review-acceptance-run.md) | 2026-10-07 | 6 KB | Acceptance run (test only): 685621e9 on claude/project-thread-uoh3ap-acceptance |
| [v0.2.6/review-addon-reimport.md](v0.2.6/review-addon-reimport.md) | 2026-10-07 | 4 KB | Review: Add-On re-import after a schema change |
| [v0.2.6/review-ally-harm-2.md](v0.2.6/review-ally-harm-2.md) | 2026-10-08 | 13 KB | Re-review: teammate-harm check, round 2 |
| [v0.2.6/review-ally-harm.md](v0.2.6/review-ally-harm.md) | 2026-10-07 | 6 KB | Review: teammate-harm check (fighting lane) |
| [v0.2.6/review-capabilities-loop3.md](v0.2.6/review-capabilities-loop3.md) | 2026-10-07 | 11 KB | Review: capabilities loop 3 (plan row 4), claude/project-thread-uoh3ap @ f8115830 (range c753eada..f |
| [v0.2.6/review-capabilities-rebase.md](v0.2.6/review-capabilities-rebase.md) | 2026-10-07 | 1 KB | Capabilities rebase: d233e7fc (claude/project-thread-uoh3ap-rebased) on fighting 608cf674 |
| [v0.2.6/review-capabilities.md](v0.2.6/review-capabilities.md) | 2026-10-07 | 5 KB | Review: gameplay capabilities (PR #29), head 8d4365a, base main |
| [v0.2.6/review-ci-timeout.md](v0.2.6/review-ci-timeout.md) | 2026-10-07 | 2 KB | Review: CI timeout fix, claude/fix-ci-timeout @ df1198cc (base main c753eada3) |
| [v0.2.6/review-command-wait.md](v0.2.6/review-command-wait.md) | 2026-10-07 | 2 KB | Review: bundled_in_game "command 13" wait fix |
| [v0.2.6/review-crash-capture-hang.md](v0.2.6/review-crash-capture-hang.md) | 2026-10-07 | 10 KB | Design review: crash capture hang, snapshot dump (Windows, crates/crash/src/windows.rs) |
| [v0.2.6/review-crash-hang.md](v0.2.6/review-crash-hang.md) | 2026-10-07 | 2 KB | Crash-capture hang: 0ff3381 (claude/fix-linux-boot-c5rao2, on ee04184) |
| [v0.2.6/review-crash-snapshot-wait.md](v0.2.6/review-crash-snapshot-wait.md) | 2026-10-07 | 4 KB | Review: crash-capture test, wait for the snapshot clone to go |
| [v0.2.6/review-design-acceptance.md](v0.2.6/review-design-acceptance.md) | 2026-10-07 | 5 KB | Review: acceptance run design note (db76ec7c, doc-only on c424c8b0) |
| [v0.2.6/review-design-ally-harm.md](v0.2.6/review-design-ally-harm.md) | 2026-10-07 | 10 KB | Review: design note, expected harm to the bot's own side (design-ally-harm.md) |
| [v0.2.6/review-design-capabilities.md](v0.2.6/review-design-capabilities.md) | 2026-10-07 | 5 KB | Review: design note, capabilities lane (loop 3, timed throws, harm reader, acceptance half) |
| [v0.2.6/review-design-fighting.md](v0.2.6/review-design-fighting.md) | 2026-10-07 | 6 KB | Review: design note, fighting lane (visibility + where to stand) |
| [v0.2.6/review-design-grenades.md](v0.2.6/review-design-grenades.md) | 2026-10-07 | 6 KB | Review: design note, timed throws / grenades (plan row 5) |
| [v0.2.6/review-design-ledge-pushes.md](v0.2.6/review-design-ledge-pushes.md) | 2026-10-07 | 7 KB | Review: design note, ledge pushes (plan row 7) |
| [v0.2.6/review-design-locomotion.md](v0.2.6/review-design-locomotion.md) | 2026-10-07 | 3 KB | Review: design note, locomotion lane (vehicles, warm at load, shove landing) |
| [v0.2.6/review-door-fixture.md](v0.2.6/review-door-fixture.md) | 2026-10-07 | 1 KB | Review: door fixture fix, 2515cce..79c9c48 (net 2 files) |
| [v0.2.6/review-doorway-settle.md](v0.2.6/review-doorway-settle.md) | 2026-10-07 | 4 KB | Review: doorway settle, 79c9c48..8810f4f (nav.rs + bot_brain.rs) |
| [v0.2.6/review-fighting-final.md](v0.2.6/review-fighting-final.md) | 2026-10-07 | 6 KB | Fighting final review: claude/v0.2.6-fighting-ki8k6d at 78a400e6 |
| [v0.2.6/review-fighting-spots.md](v0.2.6/review-fighting-spots.md) | 2026-10-07 | 14 KB | Review: fighting PR (sight + where to stand), claude/v0.2.6-fighting-ki8k6d @ 764f9e6f vs main bfdf8 |
| [v0.2.6/review-fog-sky.md](v0.2.6/review-fog-sky.md) | 2026-10-08 | 6 KB | Review: fog and sky (claude/fog-sky-floor-jjyotf at 4571b9c, on stack-plain) |
| [v0.2.6/review-gate-speed.md](v0.2.6/review-gate-speed.md) | 2026-10-07 | 5 KB | Review: gate speed (db87ecd1 on 498d7f58, claude/fix-gate-speed) |
| [v0.2.6/review-ledge-pushes.md](v0.2.6/review-ledge-pushes.md) | 2026-10-07 | 6 KB | Ledge pushes: claude/v0.2.6-fighting-pushes-ki8k6d at 107dd555 |
| [v0.2.6/review-linux-boot.md](v0.2.6/review-linux-boot.md) | 2026-10-07 | 5 KB | Review: Linux startup failures (9ffe6d2 on 498d7f58, claude/fix-linux-boot-c5rao2) |
| [v0.2.6/review-load-flakes-2.md](v0.2.6/review-load-flakes-2.md) | 2026-10-07 | 3 KB | Review: load-flake fixes, round 2 (8b2e3398) |
| [v0.2.6/review-load-flakes.md](v0.2.6/review-load-flakes.md) | 2026-10-07 | 5 KB | Load-flaky test fixes: 0a3bcc8f (claude/fix-mirror-recursion-dv7x1g) |
| [v0.2.6/review-mac-windows-launch.md](v0.2.6/review-mac-windows-launch.md) | 2026-10-07 | 5 KB | Review: Mac and Windows launch fixes (fa3113f..ee04184, claude/fix-linux-boot-c5rao2) |
| [v0.2.6/review-measured-moves.md](v0.2.6/review-measured-moves.md) | 2026-10-07 | 4 KB | Review: measured movement, branch claude/v0.2.6-measured-moves-x42j1c @ 2515cce (main ac2bdc2 merged |
| [v0.2.6/review-minigame-set-favs.md](v0.2.6/review-minigame-set-favs.md) | 2026-10-07 | 1 KB | Review: Create Mini-Game Set Favs state (97b5fc1) |
| [v0.2.6/review-minigame-setup-before-create.md](v0.2.6/review-minigame-setup-before-create.md) | 2026-10-07 | 3 KB | Review: mini-game Setup/Teams before Create (dd0a4b3) |
| [v0.2.6/review-mirror-flicker.md](v0.2.6/review-mirror-flicker.md) | 2026-10-07 | 6 KB | Review: mirror tunnel flicker fix (f28e30fd on 498d7f58, claude/fix-mirror-recursion-dv7x1g) |
| [v0.2.6/review-nav-branch.md](v0.2.6/review-nav-branch.md) | 2026-10-08 | 10 KB | Review: claude/bot-navigation at 17d1f460 (base c514ff4e) |
| [v0.2.6/review-nav-design.md](v0.2.6/review-nav-design.md) | 2026-10-07 | 7 KB | Review: v0.2.6 bot navigation design note (with change 5) |
| [v0.2.6/review-outside-npc-review.md](v0.2.6/review-outside-npc-review.md) | 2026-10-07 | 6 KB | The outside NPC review: release risk, and a v0.2.7 consolidation plan |
| [v0.2.6/review-plant-limit.md](v0.2.6/review-plant-limit.md) | 2026-10-07 | 4 KB | Plant limit in dense builds: 92501ea (claude/fix-plant-limit-dense-builds) |
| [v0.2.6/review-portal-shadows.md](v0.2.6/review-portal-shadows.md) | 2026-10-07 | 6 KB | Review: far portal sun shadows (PR #30), head 2b5d786, base main |
| [v0.2.6/review-ragdoll-corpse.md](v0.2.6/review-ragdoll-corpse.md) | 2026-10-07 | 6 KB | Review: ragdoll budget + corpse collision (93fbffae, 2 commits on df1198cc) |
| [v0.2.6/review-ragdoll-never-stops.md](v0.2.6/review-ragdoll-never-stops.md) | 2026-10-07 | 7 KB | Review: ragdoll never stops for lag (a521921c) |
| [v0.2.6/review-release-addon-check.md](v0.2.6/review-release-addon-check.md) | 2026-10-07 | 4 KB | Review: release zip check fails on left-out Add-Ons (8b066d83) |
| [v0.2.6/review-replay.md](v0.2.6/review-replay.md) | 2026-10-07 | 16 KB | Review: match replay (PR #27), head cc621a3, base main a78b3a5 |
| [v0.2.6/review-rooftop-fix.md](v0.2.6/review-rooftop-fix.md) | 2026-10-07 | 4 KB | Review: rooftop fix, hop landing over a real jump's air time (a84f8215, branch tip 00c6bd69) |
| [v0.2.6/review-shove-landing.md](v0.2.6/review-shove-landing.md) | 2026-10-07 | 5 KB | Review: `reach::shove_landing`, dc0c93c (one commit on b3b84f4) |
| [v0.2.6/review-slate-load.md](v0.2.6/review-slate-load.md) | 2026-10-07 | 4 KB | Review: Slate load "freeze" (26e86779 on 498d7f58, claude/fix-slate-load-s5hahr) |
| [v0.2.6/review-stand-up-camera.md](v0.2.6/review-stand-up-camera.md) | 2026-10-07 | 2 KB | Review: first-person stand-up camera (PR #26), head e2048f2, base main a78b3a5 |
| [v0.2.6/review-stock-tools.md](v0.2.6/review-stock-tools.md) | 2026-10-07 | 2 KB | Stock tools read by their data: 5fd6cfaf (on 1bfaba28) |
| [v0.2.6/review-timed-throws.md](v0.2.6/review-timed-throws.md) | 2026-10-07 | 8 KB | Review: timed throws (grenades), claude/project-thread-uoh3ap @ a942ba48 (range 66a286b..a942ba48) |
| [v0.2.6/review-vehicle-handling.md](v0.2.6/review-vehicle-handling.md) | 2026-10-07 | 7 KB | Review: measured vehicle handling, claude/v0.2.6-measured-moves-x42j1c @ 7977fc1 (range c753ead..797 |
| [v0.2.6/review-vehicle-wreck.md](v0.2.6/review-vehicle-wreck.md) | 2026-10-07 | 3 KB | Review: vehicle explosion white parts (PR #28), head a819214, base main a78b3a5 |
| [v0.2.6/review-whole-picture-fixes.md](v0.2.6/review-whole-picture-fixes.md) | 2026-10-07 | 3 KB | Review: whole-picture fixes (capabilities lane) |
| [v0.2.6/review-whole-picture.md](v0.2.6/review-whole-picture.md) | 2026-10-07 | 7 KB | Whole-picture review: v0.2.5 (2cf0b3a4) → final main + pending stack, as one change |
| [v0.2.6/review-wind-up.md](v0.2.6/review-wind-up.md) | 2026-10-07 | 6 KB | Review: bots' wind-up loop (f884a925 on c424c8b0 + db76ec7c) |
| [v0.2.6/whole-picture-notes-bots.md](v0.2.6/whole-picture-notes-bots.md) | 2026-10-07 | 2 KB | Whole-picture working notes: bot module sweep (combined tree d068f06e + ec7db637 + ed0680c0 + 92501e |
| [v0.2.6/whole-picture-todo.md](v0.2.6/whole-picture-todo.md) | 2026-10-07 | 1 KB | For the whole-picture review (coordinator note, 16:25Z) |
| [v0.2.7/driving-teamkill-bisect.md](v0.2.7/driving-teamkill-bisect.md) | 2026-10-08 | 3 KB | v0.2.6 gate failures: driving and teamkill bisect (Linux, cloud) |
| [v0.2.7/release-list.md](v0.2.7/release-list.md) | 2026-10-08 | 1 KB |  |
| [v0.3.0/gltf-authoring.md](v0.3.0/gltf-authoring.md) | 2026-10-13 | 10 KB | Preparing models in Blender for Blockland ReImagined |
| [v0.3.0/movement-controllers.md](v0.3.0/movement-controllers.md) | 2026-10-12 | 16 KB | Movement controllers |
| [v0.3.0/overlap-review.md](v0.3.0/overlap-review.md) | 2026-10-14 | 8 KB | v0.3.0 overlap review (HEAD 3a37b4d3) |
| [v0.3.0/w2-assessment.md](v0.3.0/w2-assessment.md) | 2026-10-12 | 4 KB | W2 assessment: grips, sockets and animation roles |
| [v0.3.1/release-notes-draft.md](v0.3.1/release-notes-draft.md) | 2026-10-15 | 2 KB | v0.3.1 release notes (draft, collected as PRs merge into v0.3.1-experimental) |
| [v027/bot-ownership.md](v027/bot-ownership.md) | 2026-10-08 | 13 KB | Bot brain ownership map (main d6161ef, 2026-10-08) |
| [v027/punch-list.md](v027/punch-list.md) | 2026-10-09 | 13 KB | v0.2.7 punch list |
| [v029-addon-compat/report.md](v029-addon-compat/report.md) | 2026-10-11 | 3 KB | v0.2.9 Add-On compatibility report |
