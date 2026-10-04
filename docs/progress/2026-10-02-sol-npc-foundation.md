# 2026-10-02 Sol High NPC foundation and integration

Work continued through 2026-10-03.

Root relayed Maxwell's latest GPT-6.1 Sol High instruction (superseding AGENTS
Luna) and full evidence-based scope autonomy. The task stayed in the existing
dirty `fix/v0.2.1-playtest` worktree; no commits/pushes/manifests/lockfiles, new
worktrees or gameplay automation. All validation is headless/code. Root owns
the combined v0.2.1 Windows/macOS/Linux release and unfinished alpha contract.

## Delivered decisions and paths

`docs/audits/v0.2.1-work-plan.md` inventories every reported repair, NPC
objective/capability/navigation/cooperation package and combined release DoD,
ownership/dependency/evidence/state. Crash R11 and release V01/V02/V03 remain
open. `bot-objective-spike.md` now leads with implemented scope, exact bounds
and limits; earlier proposals are explicitly historical. The owned seam row
records runtime integration without implying a frozen mod API.

Pure `session/bots/planning.rs` uses bool/i64 facts, positive checked costs,
ordered guarded effect groups, deterministic Dijkstra labels including depth,
and separate entry/candidate/action/fact/model limits. This fixes the old
cost-only deep-route dominance bug and successor-allocation cap gap. Sixteen
standalone tests passed; the linked module's tests pass in the sim library.

Root explicitly approved canonical physical NPC Player/MiniGame slots for all
inputs declaring them. `session/events.rs` uses one `input_context` builder for
ordinary dispatch and grounded inspection, preserves Bot/Driver and the real
quota client, never invents SlotClient. `session/rules.rs` shares canonical
scope/authority helpers. This deliberately changes v20's absent NPC physical
slots during alpha; actual touch remains onBotTouch. `events/runtime.rs` adds
read-only row_intent/row_targets inspection only; tests cover missing/named/
derived targets, source/class validation and no scheduler side effects.

`session/bots/objectives.rs` grounds complete supported activation/region/touch
input groups from actual creator-owned indexed programs, scoped numeric
variables, scores, real allowed round operations and typed Color. Guards see
prior due-time/row ordered effects. Cosmetic ColorFx/ShapeFx/PlaySound have no
callbacks. Unknown collateral and source/actual Target variable-change child
chains are explicit unsupported. Relevant timer/score/end effects intersect
observed dependencies; unrelated creator clocks/color/foreign-owner events do
not globally disable valid objectives. Independent review caught the named
Target child omission; both direct and global reaction adapters now reject it.

Grounding separately caps source64/rows256/actions32/cumulative targets128,
facts128/terms4096/stringbytes65536 before per-target clones or retained row
snapshots. Pure search has entries256/depth12/candidate4096/facts128 and the
same term/string bounds. One rotating eligible bot models/searches each tick;
completion reads only bounded retained observations. Same-modulo IDs do not
starve one another. Steps return copyable aim/point/wait views, never clone the
full model per tick. Actual admitted-input tracking is bounded1024 and survives
unchanged cosmetic reinstalls.

The brain uses ordinary movement/aim, inventory unequip and real Activate/
ActivateRelease command sequences. Region/touch completion requires actual
admission, including physical exit/reentry when already inside; proximity and
elapsed time are not success. Changed source/scope/round/team/death, missing
observations or lack of progress invalidate steps. Eight bounded failed-action
entries cool down for60s, letting a reachable alternative win. Existing Explain
prints up to four related NPC reasons/status lines. Advancing partial nav
segments continue; blocked segments fail and release the objective.

Objective utility is engine mechanism; Blockhead `bots.json` opts in, other
creature/package policy persists. Performance-owned `bots/tactics.rs` and
`bots/combat.rs` are linked as `hand_combat` to preserve existing canonical
combat namespace. Brain state and shared global budgets select typed inventory
weapons and desired trajectories, retain normal error/turn/reaction/movement,
and use ordinary trigger semantics. Narrow `session/weapons.rs` integration
prepares actual frames after movement, then validates current intent/identity/
actual direction/current velocity with the same plan-tick budget before
consuming a supported trigger. Charge holds/recovery avoid redundant path
casts; real release authorization occurs only after live geometry validation.
Opaque package, mounted and portal executors retain their previous path.

Rule Workshop Door panel was incorrectly using a floor plate; tall Door and
Gate panel selection now requires a typed filled single-box collision recipe,
not a lexical window/hole. Their bottoms are seated on a downward-observed
floor and both use height-based lift. The regression includes an earlier
non-solid geometric decoy and real open/delayed-close authored events.

## Evidence, failures and corrections

- `cargo test -p bri-sim -p bri-events --lib --locked`:
  `/tmp/bri-v021-sol-npc-final-libraries.log`: events1 pass; sim145 pass,
  one generated-v20-content test ignored. Includes planning16/tactics19,
  context/owner/fairness, solid Door actual collision delay and rule regressions.
- `cargo test -p bri-chaos --test bot_brain --test bot_interactions
  --test bot_objectives --test bot_tactics --locked -- --nocapture`:
  brain18 and interactions15 pass after typed integration in
  `/tmp/bri-v021-sol-native-combat-regressions.log`.
- `/tmp/bri-v021-sol-native-combat-compositions2.log`: objectives8 pass:
  unfamiliar four activations, reversed ordered regions/delays, repeated entry,
  unrelated timer plus96decor/cosmetics, unknown collateral, relevant timer,
  inaccessible cheap region followed by actual reachable win, and4096named
  fanout rejected before per-target projection. Explain reason assertion passes.
- Combat unfamiliar direct/splash/charged/melee launches actually hit and slow
  projectile approaches rather than waiting indefinitely. Final safe-slot and
  multi-charger/current-frame negative evidence belongs to performance/review
  lane; combined final commands/results will be appended on completion.
- Independent `bot_navigation_spike` seven pass, including actual named-target
  child rejection, rotated doorway, >155-unit segmented objective route and
  temporary narrow-corridor blockage. Three actor idle stack minfeet
  0.010/2.672/5.335 remains grounded10s; solo climber cannot reach5-unit platform,
  actor support permits it. Fixed Nav returns Partial in exactly that world.
  `bot-navigation-feasibility.md` records the gap and supported delivery pivot.

Early fixture failures were meaningful harness corrections: rendering field
is `visible`; bounds require Vec3; cosmetic colors need valid palette entries;
LoadBuild can remap identical palette entries, so fanout tests compare actual
before/after colors. Derived Add-On targets need a custom target class and
package output, not a native class alias. A recipe test must allow the normal
phase to install named targets before static input inspection; the final test
uses that phase and actual delayed close. First cargo check used explicit
DEBUG=0/INCREMENTAL=0 once; all subsequent final checks use the normal dev
profile, no CARGO_TARGET_DIR override. No failure is marked acceptance done.

## Limits and handoff

This is a grounded expressive experiment, not universal NPC learning. No
objective transport of the exact spawned ball, general variable/relay child
closure, temporal planner over arbitrary pending jobs, team-score guards,
hookshot/Gravity Gun delivery, cooperative role/support jobs or mounted typed
inventory provider is claimed. Physical stack feasibility is proven; AI
support/navigation transition remains unimplemented. Real round cancellation
is observed, projected WinRound never announces victory; general winner
identity diagnostics would need an additional canonical observation.

The existing editable Workshop race/regions plus a planted Blockhead spawner
provide the human authoring/playtest path; Explain reports active/unsupported
reason. Root preserves all earlier repairs and reviews the combined candidate,
full gate and three-platform artifacts. R11 remains unresolved; V01/V02/V03
remain open. Interactive acceptance is Maxwell's responsibility.


## Integration freeze evidence — 2026-10-03

Own runtime/foundation paths are frozen for root integration after the final
review corrections. `cargo test -p bri-sim -p bri-events --lib --locked` passed
events1 and sim152, with one generated-content fixture ignored:
`/tmp/bri-v021-sol-npc-final-libraries2.log`. This includes two-real-game
creator scope, shared combat budget, release-only charge admission including
initial mount onFire rejection, live postmovement wall/actual-ray-miss negatives
and the independent thin-water clamp regression.

Final `cargo test -p bri-chaos --test bot_tactics --locked -- --nocapture`
passed all3 in0.21s: `/tmp/bri-v021-sol-native-tactics-final.log`. Unfamiliar
direct/splash/charge/melee images actually damage the target; nearby unsafe
blast range chooses the real second-slot Gun and damages through ordinary
controls; slow-flight minimum approach is7.875 from initial29.748 before
damage. The earlier close fixture incorrectly reset its authored distance
at MiniGame creation, then required observing a live projectile even though
a fast shot can launch, hit and disappear within one tick. The corrected
fixture observes actual selected image, canonical damage and every later
Rocket's safe launch clearance. The slow fixture now measures approach before
target death instead of the subsequent ordinary Return position. These are
harness corrections, not disabled assertions or special NPC modes.

Latest brain18/interactions15 remain passing. After the final authoritative
source-edit correction, `cargo test -p bri-chaos --test bot_objectives --test
bot_navigation_spike --locked -- --nocapture` passed8 each:
`/tmp/bri-v021-sol-npc-source-invalidation.log`. Steps and failed-action
cooldowns retain/revalidate program owner/name/rows; active steps also check
the actual physical source owner, and retained name bytes consume grounding
budget. Changed/removed sources invalidate rather than executing stale plans.
This closes a source-edit permission gap without altering the executor.

Final `cargo clippy -p bri-sim -p bri-events -p bri-chaos --all-targets --locked
-- -D warnings` passed11.07s: `/tmp/bri-v021-sol-npc-final-clippy.log`.
`git diff --check` passes. All final runs use the normal dev profile. Cargo
was released to performance for optimized combined timing; no build runs
remain in this lane. Root's combined workspace/platform gate and R11 remain
open, and original Windows/AMC hangs are still unexplained.

Existing creator playtest guide is root-owned
`docs/rule-workshop/V0.2.1-PLAYTEST.md`; no extra NPC mode or UI was added.
