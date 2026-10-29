# 2026-10-29 Bot gameplay polish across three real saves

Working branch `codex/bot-carrier-gameplay`, based on current remote main
`030b43a03`. Maxwell authorized investigation, a warm private playtest and
merge after supported corrections and the required checks. The idle, clean,
merged Afghan worktree is reused; the older main checkout stays untouched.

## Standard and inputs

Treat TDM Afghan, TDM Afghan with Zombies, and Bedroom's Capture The Flag ACM
City as ordinary gameplay cases. Vary weapons/situations in explicit session
setups, explain recurring anomalies from authoritative observations and actual
controls, and fix causes at existing owners. Favor simplifying conflicting
state over extra special cases. Success counters alone do not establish fun,
believable combat or navigation. Every supported correction needs a meaningful
regression and contrasting integrated evidence. Retained limitations stay
explicit. Deliver the warm playable folder after changed-area/startup checks;
merge after scoped checks and required platform/history/protocol CI. Maxwell
alone performs interactive acceptance; full alpha acceptance remains open.

Private inputs and reports are under ignored `artifacts/carrier-gameplay/`.
`inputs.json` records absolute paths, SHA256, brick counts and saved minigames.
Original saves/installations are read only.

- TDM Afghan: 14,107 bricks, SHA256
  `853efb27dd74d1dc36ff8d3c8e6e9e066527cfe991e93a79a6b0a67edaedf61d`;
  saved Slayer CTF, two eight-slot teams, No-Jet bodies.
- TDM Afghan with Zombies: 14,116 bricks, SHA256
  `d960650bad8021e08c67a48df2845c37a8d4e6c81ea93dc12ae65d9f59f2b3fa`;
  saved Slayer CTF, two eight-slot combat teams and the empty-kit yellow side.
  The current save selects Fuel-Jet bodies for teams 1/2, unlike the earlier
  October 28 No-Jet trace. Preserve this rather than treating old results as
  current evidence.
- Capture The Flag ACM City: 16,489 bricks, SHA256
  `6833a93c2f3a2654d756bfbe837bde04fbaff0a472a7f660d4e62078aed705fa`;
  no saved minigame. Actual Slayer spawn colors are 0 (four) and 7 (three),
  with matching flag colors and tanks. Maxwell authorized session-only team
  setup. `acm-minigame.json` uses the ordinary Afghan CTF settings/loadout,
  eight slots per side and those colors. The source build is unchanged.
  A separate `ACM City Bot CTF.world.json` was added alongside the original for
  Maxwell's playtest; only its minigame header changes and its compressed brick
  payload is byte-identical. No existing save was overwritten.

The existing `bot_vehicle_routes` diagnostic executes normal brains, rules,
weapons and movement without injected AI goals/controls. Its new optional
`BRI_DRIVE_MINIGAME` supplies explicit setup only when a build has no minigame.
Baseline content is the extracted, verified v0.4.1 Windows content folder.
Concurrent compilation/runs are diagnostic gameplay checks, not isolated
performance measurements.

## Investigation and checks in progress

Initial commands: `git fetch origin main`; verify the reused branch is merged;
`git checkout -b codex/bot-carrier-gameplay origin/main`; native header/packed
brick inspection with Python msgpack/zstandard; build and run the existing
ignored saved-match trace with `--profile playtest`, 600 seconds for Zombies
and an initial 180 seconds for ACM. Reports/logs retain exact failures.

The first attempted build used binary names as Cargo package names and was
rejected before compilation. The corrected build selects the five `--bin`
targets explicitly. This is a command correction, not a production defect.
No full gate, GPU suite or interactive game was launched.

## Supported causes and corrections

- A bounded route search discarded its frontier even when its closest partial
  endpoint was the same dead end. On the real north rooftop, 6,000 expansions
  returned one waypoint at the edge; an offline 86,622-node search found the
  ordinary stairs behind it. The first attempted frontier-endpoint change
  reached the real stairs but failed a mirrored synthetic repeated-detour test:
  it could cycle. That experiment was rejected, not accepted by weakening the
  test. The correction resumes the existing Search on a no-headway budget
  cutoff. Per-tick sampling/expansion limits remain; resumed searches have a
  hard bound of the existing MAX_SAMPLES node capacity. Exhausted graphs do
  not resume. Normal useful partial segments still execute immediately. The
  same mirrored repeated-detour regression now passes.
- Re-created underfoot waypoints reset the physical stall judge without any
  physical displacement. Removing that reset leaves one physical headway
  judge in charge. Stationary and actually moving controls challenge the
  correction in `replanned_underfoot_waypoints_cannot_erase_a_stationary_walk`.
- Visibility locked a retained hostile forever. Reassessment now uses the
  existing shared sight budget on a per-bot cadence, the same distance/team
  terms and the shared hold margin. A nearby visible opponent can displace a
  distant one; an occluded opponent cannot. A qualified synthetic visibility
  regression passes. Its first fixture failed to join at an obstructed floor
  pose; corrected initial spawn clearance is a harness repair.
- A held native charge excluded all alternative equipment, even after long
  periods with no authorized release. Fair weapon turns now weigh alternatives;
  a transient interruption preserves the current wind-up for one authored attack
  cycle plus the existing hold interval. A persistently unusable attack can
  yield through ordinary safe cancellation/equip. The new actual-control close
  attack regression and the unchanged transient range-excursion, moving-target,
  charge safety and native-hold assertions pass. No rule says a sword must beat
  a gun merely because it is close.
- Later actual gameplay exposed a distinct 60-second delivery exclusion after
  a locomotion approach timeout. Known feet-only actions now use the same short
  navigation retry as other failed routes; authored waiting/tool/action failures
  retain their own failure handling. The initial candidate's carrier 11 had
  repeated long unsupported-semantics periods while still showing the carried
  image. A new opt-in `BRI_DRIVE_RULE_SAMPLES` records canonical package state
  once per second to distinguish actual carriage, epochs, rules and diagnostics
  instead of treating an image or a persistent diagnostic as a complete event.
- An ordinary Afghan candidate exposed tank occupants leaving a partly buried,
  obstructed chassis and subsequently falling below the map. The vehicle's
  original last-point dismount fallback can choose a blocked point. A shared
  read-only exit query reuses its exact authored exits and rider clearance.
  The first overlap-only query did NOT close the actual Afghan reproduction:
  252 living unmounted below-floor samples remained. Returning the exact exit
  and checking a landing also left 258 such samples. Neither is accepted as
  a fix on isolated test evidence; the actual exit/control chain is being
  traced. Bots count actual successful exits.
  The manual/forced source behavior is retained in its existing tests, augmented
  with blocked/clear query controls. This is an exit-admission correction, not
  a claim that chassis contact or all tight turns are solved; actual rerun and
  The final cause of the false landing was a remote bottom surface 507 units
  below the exit, accepted by an unbounded query for bodies without fall damage.
  Exit admission now shares foot navigation's bounded drop policy, actual body
  scale, and fixed support/water semantics. The 180-second `bounded-afghan`
  rerun has zero living below-floor samples (previous candidates: 252/258),
  and both affected bots continue play. A regression includes the remote bottom
  floor, ordinary nearby landing, empty space, and the departing dynamic hull.
  This closes the observed exit loss, not the underlying hull contact defect.

## Gameplay and verification checkpoints

Baselines: 600 seconds of the current Zombie save, 180 seconds each of ordinary
Afghan and ACM CTF, plus 180-second gun-loadout variants on Afghan Team Deathmatch
and ACM CTF. Variants change only the private compressed minigame header; packed
bricks are byte-identical. All completed with zero control-step failures. ACM's
baseline recorded one actual capture, demonstrating that it can complete a
normal CTF journey with the explicit setup. Counts alone are not acceptance.

The first integrated four-correction candidate completed all five scenarios.
Its Zombie run recorded a real capture and no stationary vehicle candidates,
but exposed the longer delivery timeout exclusion. Its ordinary Afghan run
exposed below-map exits; these were not hidden by passing control checks.
`candidate-*` reports are superseded investigation evidence, not the final build.
Additional rule-state and exit-admission runs are in progress.

At the first candidate checkpoint: 476 sim units pass, five deliberately ignored;
77 relevant integration cases pass, one deliberately ignored actual-original case
(`bot_carryable_objectives`, `bot_interactions`, `bot_navigation_spike`,
`bot_perception`, `bot_routes`, `bot_tactics`, `bot_windup`). Warning-free clippy
passes for all sim/chaos targets with `--profile playtest`. Five playable
executables build; official packaging input validation passes against the
unchanged v0.4.1 content and verified bundle. These precede the final timeout/exit
edits and are not promoted to a final-head pass. No performance guarantee is
inferred from contended headless timings.

The final source checks, rerun results, private playable handoff and merge/CI
receipts will be recorded below. Human feel and full alpha acceptance remain open.

## Packaging boundary

The initial official `package_playtest.ps1 -ValidateOnly` passed before vehicle
runtime edits. After them the original bundle's freshness check correctly
refuses packaging: its importer dependency stamp includes vehicle runtime
sources. No receipt is restamped and the published content remains unchanged.
For the requested private engine preview, the handoff may reuse the published
v0.4.1 portable folder after checking every published manifest hash, replacing
only the five built executables and writing new executable hashes/provenance.
This is an engine preview with unchanged verified content, not a newly
source-regenerated release or a claim that the official freshness gate passed.

Windows `cargo fmt --all` exceeds the command-line length limit; changed files
are checked directly with rustfmt. A probe relink initially failed LNK1104 while
its executable was running; subsequent probes run from private executable copies.
Temporary debug instrumentation also caused an E0382 compile failure, corrected
before rerunning checks. These are recorded failures, not retries labeled clean.
