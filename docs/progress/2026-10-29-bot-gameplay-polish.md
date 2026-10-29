# 2026-10-29 Bot gameplay correction set

## Finite finish line and current status

Maxwell authorized investigation, a warm preview and merge when root is satisfied.
His later finite scope, relayed by release coordinator chat
`01a1857e-2792-76f3-bb89-9a45627b6e6c`, supersedes an open-ended bot-polish
interpretation: finish this correction set on Afghan Zombies, ordinary Afghan,
ACM and the already-selected weapon/mode contrasts; one final bounded matched
matrix, scoped checks, updated warm preview, exact-head CI and serialized merge.
Do not open more seeds, general AI rewrites or unrelated audits. Human feel and
the full alpha contract remain separate and incomplete.

Production source is frozen after the completed bounded matrix and scoped
checks below. Remaining work: final warm build/startup receipt, branch push,
current-head platform/history CI and coordinator-serialized merge. The active
goal stays incomplete until those deliveries finish. PR 404 remains draft until
its final evidence is published: https://github.com/MaxHastings/BlocklandReImagined/pull/404

Branch `codex/bot-carrier-gameplay` reuses the clean, previously merged warm
`afghan-tank-navigation` worktree, based on remote main `030b43a03`.
The older dirty main checkout and original installations are untouched.

## Exact inputs and activation

Private receipts, snapshots, probe executables and reports are in
`artifacts/carrier-gameplay/`; `inputs.json` records absolute save paths, headers
and hashes. No original saved builds or game content enter Git.

| Save | Bricks | SHA256 / setup |
| --- | ---: | --- |
| TDM Afghan | 14,107 | `853efb27dd74d1dc36ff8d3c8e6e9e066527cfe991e93a79a6b0a67edaedf61d`; saved Slayer CTF, two eight-slot No-Jet teams |
| TDM Afghan with Zombies | 14,116 | `d960650bad8021e08c67a48df2845c37a8d4e6c81ea93dc12ae65d9f59f2b3fa`; saved CTF, Fuel-Jet combat teams plus yellow empty-kit zombies |
| Bedroom Capture The Flag ACM City | 16,489 | `6833a93c2f3a2654d756bfbe837bde04fbaff0a472a7f660d4e62078aed705fa`; no saved minigame |

ACM's actual Slayer spawn/flag colors are 0 and 7. Its explicit CTF setup uses
eight slots per side and ordinary No-Jet/spear/sword/HE settings. A separate
`ACM City Bot CTF.world.json` was added alongside Maxwell's original: only the
minigame header changes; compressed brick payload and original are unchanged.
The gun-kit Afghan TDM and ACM CTF variants likewise change only the private
minigame header. Earlier October 28 No-Jet Zombie traces are not substituted for
the current Fuel-Jet save. Crazy Collage is outside this comparison set.

The matched package selection SHA256 is
`4fa385648f10856264079f6a2740c417df27d7652a134bd4b97f6f78b962f8f3`.
It activates the installed extras, including `bot_zombie`. `packages` in each
report records actual loaded identities. Clean published defaults activate fewer
Add-Ons: initial clean-default `nav-*` diagnostic runs omitted zombies and are
NOT evidence about the original hang. Matched evidence uses `nav-matched-*`,
`watch-*`, `contact-afghan`, and `safe-*`. Installed selections remain untouched;
private activation is not promoted to public release defaults.

## Corrections and cause evidence

- Bounded navigation discarded a frontier whenever its closest partial segment
  returned to the same roof edge. Searches now retain unexpanded work across
  ordinary per-tick quanta, up to the existing sample cap. Actual offline roof
  evidence needed 86,622 nodes to discover the stairs; the original 6,000-node
  restart repeated the dead end. Mirrored detours must reach the goal, and an
  exhausted graph must stop. Useful partial routes still move immediately.
- Consuming recreated underfoot waypoints reset stationary headway. Only physical
  progress now clears that stall history; moving pursuit remains a control case.
- A still-visible retained enemy prevented ordinary reconsideration. The existing
  sight budget now periodically weighs alternatives with a hold margin; close
  visible and occluded alternatives are tested separately.
- A held unusable charge excluded every other tool. Fair equipment turns now
  consider alternatives, protecting a transient wind-up for its authored cycle
  and existing hold interval. Persistent failure can safely cancel/equip through
  ordinary controls. No forced sword preference or fabricated release is added.
- A feet-only approach timeout excluded an objective for 60 seconds. It now uses
  the existing short navigation retry. Authored waits and tool/rule failures
  retain their own handling. Canonical CTF state, not an image alone, identifies
  actual carriage and interruption intervals.
- Autonomous dismounts check the native first-clear authored exit and a bounded
  physical landing. Manual/forced source behavior is retained. Actual source
  scale and authored upward exits are accounted for; remote bottom surfaces are
  not useful landings. This prevents the observed rider loss below the map.
- Timed damage restoration could materialize walls inside a vehicle or player.
  It now reuses safe placement's exact shape-contact query and established 2 mm
  penetration tolerance, including physical vehicle shapes. Harmless support
  contact, real openings, water and noncolliding originals are distinguished.
  A blocked original, all descendants and section retirement stay intact until
  clear. Its deadline remains in the bounded round-robin queue, rather than
  monopolizing the first `restore_pending` entries. Explicit event collision
  operations and ordinary placement semantics remain unchanged.

The two persistent gameplay traps share that last cause:

| Matched case | Authoritative before evidence | After timed admission |
| --- | --- | --- |
| Afghan tank 6 | Bricks 1540/1724/505 hide at 15.425 s, all restore at 45.425 s; the first wall contacts are already 1.7–2.7 units deep. Hull settles at y≈-2.25; base 13 never changes. CCD was enabled. | Safe road 505 returns at 45.425 s; wall 1724 waits to 45.675 and wall 1540 to 46.1667. Hull stays above the road, y≈0.94. |
| Zombie bot 9 | Bricks 7345/8016/11319/11320 hide at 90.2833 s, restore at 120.2833/120.2917; bot freezes ungrounded at `[-9.919,5.338,-45.302]` through 179 s, with no fitting route start. | The matched 180 s run has no stationary-foot candidates; bot 9 continues play. The 600 s contrast also has none. |

The read-only v20 source is G17417–17469, recovered
`.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`, SHA256
`a720f7a188a615276befa34409eda68585ec9abbc629b7a8107f37dcf7b33a9b`.
Its reappearance script enables flags immediately; this does not prove native
overlap resolution. Waiting for clear physical space is an explicit safety
improvement to timed damage restoration, not a claim of exact native response.
No teleport, guessed escape offset, content-name bot rule or global drop tuning
is used. Simulation's existing safe-placement query owns the geometry mechanism;
the existing event scheduler owns admission/retry. No wire/save schema changes.

## Final bounded gameplay checks

All five final `safe-*` 180-second runs completed with zero control-step errors.
Living unmounted feet stayed at/above y≈0.20 on Afghan, y≈0.007 on Zombies, and
y≈286.40 on ACM; no below-map rider losses were retained.

| Setup | Stationary foot candidates | Stationary vehicle candidates / limit |
| --- | --- | --- |
| Afghan saved CTF | One 14-second stall, subsequently recovers | One tight-alley tank remains confined for 134 sampled seconds, with 79 attempted seconds and driver turnover |
| Afghan Zombies saved CTF | None | None |
| ACM explicit CTF | None | None |
| Afghan gun-kit TDM | None | One 26-second interval, 11 attempted seconds |
| ACM gun-kit CTF | None | One 14-second interval |

The already-selected 600-second Zombie continuation also finishes with zero
control-step errors and no stationary-foot candidates. Its three stationary
vehicle intervals are 32/16/10 sampled seconds (14/11/10 attempted seconds).
Canonical rules record six enemy-flag pickups, one own-flag recovery and one
completed capture. There are 184 soldier objective-carriage samples; route retry
interruptions remain brief rather than repeating the old 60-second exclusion.
A zombie-kind carrier separately chases/fights without a delivery plan; that
kind/provider policy is a declared limit. Counts are not capture-rate, human
feel, original-fidelity or performance acceptance. Timings are contended.

## Scoped technical checks

- `cargo test --profile playtest -p bri-sim --lib`: 480 pass, 5 ignored,
  including player/compound-vehicle holds, family/section retention, actual
  retirement cues, retry after departure, unrelated ready work and positive
  support/opening/noncolliding/water controls.
- `cargo test --profile playtest -p bri-sim --test brick_damage --test v20_events
  --test events_native --test package_brick_outputs --test plant_ground`:
  31 pass, 11 ignored native/corpus cases; `events_native` runs no case by default.
- Seven bot integration targets (`bot_carryable_objectives`, `bot_interactions`,
  `bot_navigation_spike`, `bot_perception`, `bot_routes`, `bot_tactics`,
  `bot_windup`): 77 pass, one original-content wind-up case ignored.
- `cargo clippy --profile playtest -p bri-sim -p bri-vehicles -p bri-chaos
  --all-targets -- -D warnings`: warning-free.
- Earlier unchanged vehicle-area checks: 35 vehicle units pass (3 ignored);
  native dismount scope 5 pass (5 original-content cases ignored, 63 filtered).
  Two Python analyzer regressions pass. Changed files are checked directly with
  rustfmt; Windows `cargo fmt --all` exceeds command-line length limits.
- History check passes against `030b43a03`; current-main history/protocol and
  platform CI are still required before the final merge. These are scoped checks,
  not a full workspace, asset corpus, GPU matrix or performance campaign.

## Preview, retained limits and final receipts

Interim preview `E:/Downloads/Bot-Gameplay-Polish-20261029` is
`v0.4.1-bot-polish2 (9cca5bf9a)`. It predates the timed-restoration correction.
An updated warm preview is the next deliverable, built with `--profile playtest`
for client/server/importer/checker/runner, with native startup verification.

The public v0.4.1 Windows ZIP was independently verified SHA256
`22ecb974e73eeba84b50da275f6d531edd089f2221907929d1dd3ac12a7655b7`.
All 13,426 clean published source files and copied unchanged assets were
size/hash-checked. Private manifests record replacement executable hashes,
code/version and the explicit tested config. Source installations and the
installed configuration stay untouched. Official source packaging freshness
correctly rejects the old bundle after runtime dependency edits; no importer
receipt is restamped. This is a private engine preview reusing verified assets.
Matched `--check` exits zero with two existing Butterfly Knife health warnings
(silent AbortChargefire sound/default kill message); those are outside the kits
and reported to the release coordinator, not silently suppressed.

Deferred under Maxwell's finite finish line: tight-space driving/crew re-entry
redesign, the brief recovered foot stall, richer own-flag-carrier recovery
provider dependencies, compound traversal refinement, and complete-alpha NPC
intelligence. The fatal materialization/rider-loss failures are corrected;
those remaining limits are not claimed solved and do not open new campaigns.

Rejected/failed investigation remains in private receipts and Git history:
lowest-frontier route selection failed a mirrored detour; overlap-only exits
left 252 below-floor samples; unbounded support accepted a floor 507 units down;
the harmless-drop recovery experiment passed a simpler test but did not free
the real bot and was removed; clean-default runs omitted zombies. Harness
repairs included an empty test filter, private constructor, off-grid test part,
late vehicle-catalog installation and observing section retirement through the
wrong channel. Final assertions retain the actual Session cue contract. A live
probe executable initially caused LNK1104 during relink; subsequent probes run
from private copies. None of these failed attempts is labeled a clean pass.

Root coordinated authoritative restore/query/test files with the release chat;
client debris, map ambience, liquid visuals and their mechanical initializers
are untouched. The separate random/gameplay-SFX audit was cancelled and no SFX
work is integrated. Release remains owned by the coordinator across four lanes.
Maxwell alone performs interactive testing. Local heavy work stopped whenever
a normal game was active, resuming only on explicit pause or observed exit.

Final updated-preview, exact-head CI and serialized merge receipts follow here.
