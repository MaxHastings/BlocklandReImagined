# 2026-10-03 Sol native weapon tactics and bounded performance

Added pure capability suitability, finite-safe exact discrete interception,
stable inventory hysteresis and a bounded live native adapter. The central NPC
owner links the adapter and validates actual post-movement launch frames before
ordinary trigger consumption. The adapter admits stock Gun/Rocket/Bow/Spear and
explicit mechanical Sword flight using current hand eye==muzzle frames. Unknown
script tools, unmodeled secondary effects, mixed charge graphs, portal paths and
mounted weapons keep their existing executor. Full scope and limits are in
[the capability audit](../audits/bot-weapon-capabilities.md).

Added the read-only `ammo_on_equip` projection with shared internal view/rounds
builders. All 13 magazines tests passed, including never-drawn/stored depleted
slots, shared reserves, unlimited/from-reserve supply, stowed last grenade,
invalid inputs, no mutation and custom mounted image fidelity. No manifest,
dependency, serialization or protocol changed.

Pure interception tests verify the native coast law, fractional endpoints,
moving targets and inheritance, low/high arcs, finite/degenerate/unreachable
cases, deterministic budgets, unfamiliar IDs and all attack families. Live
adapter tests cover fair owner-ID collisions, reserved query accounting, charge
holding/releasing/aborting, mixed/initial-fire graph exclusion and actual ordinary
movement into a raycast wall after an earlier valid launch frame.

Actual-control fixtures use unfamiliar item/image/projectile IDs and ordinary
bot spawners, authored spawn points, mini-game loadouts and controls. They verify
real direct/splash/charged/melee shots and damage, useful second-slot equipment
inside an unsafe first-weapon blast envelope, and meaningful approach before
slow-projectile combat. Fixture corrections preserve the acceptance: mini-game
creation resets the human to authored spawn points; a close bullet may expire in
its spawn tick; a successful kill can make later distance increase during return.
No brain objective/target or bot movement is injected.

New ignored `bot_combined_perf` builds 16 objective-enabled controllers, separate
4/8 independent-latch models and fresh mixed-combat sessions. It reports all-step
and active-control p50/p95/p99/max, bounded-model diagnostics, objective score
progress, real damage, and **distinct live projectile IDs** (not actual shot
count). It asserts actual control/progress evidence, never a machine timing
threshold. The existing real-content diagnostic now includes the already
converted ACM City save (16,478 authored bricks plus16 in-memory bot spawners),
read-only. Original installations and generated content remain unchanged.

Final linked regression/clippy and optimized combined measurements are pending
at this entry's initial write; they are not marked complete yet. Raw diagnostics
are under `/tmp/bri-v021-sol-*`. Source review found and fixed speculative-vs-live
launch direction differences, charge starvation under budget waits, native
secondary collateral exclusions and the initial-state fire bypass. Root deferred
optional vehicle utility expansion to the next iteration. Next: final current
source checks, repeat optimized combined/real-city measurements, record exact
commands/results and freeze this lane. Windows frame stalls and earlier debug
outliers remain unexplained until platform-specific host/client capture.

Final linked source checks: sim library 152 passed/1 ignored and events library 1
passed (`/tmp/bri-v021-sol-npc-final-libraries2.log`), including 20 pure tactics
and 5 adapter tests. All 3 actual-control combat fixtures passed
(`/tmp/bri-v021-sol-native-tactics-final.log`). Close-range combat observed the
useful second-slot Gun inside the unsafe rocket envelope and its authored 10
damage, despite zero surviving live projectile IDs. The slow 5-unit/s weapon
approached from 29.75 m to 7.88 m before dealing 10 damage. Four unfamiliar-ID attack
families all dealt actual damage. Scoped clippy for sim/events/chaos all targets
with `--locked -- -D warnings` passed
(`/tmp/bri-v021-sol-npc-final-clippy.log`). Final launch validation also rejects
any current image/projectile capability or final actor scale that differs from
the planning descriptor. These results close the source defects; optimized
combined measurements below remain a separate acceptance item.

Optimized measurement is now complete and the lane is frozen. The command
`cargo test --release -p bri-chaos --test bot_combined_perf --test bot_battle_perf
--no-run` passed in 1m38s, with `CARGO_TARGET_DIR` unset. Each binary then ran
directly, sequentially and unsampled; every phase was repeated twice. Build and
run logs are `/tmp/bri-v021-sol-combined-release-build.log`,
`/tmp/bri-v021-sol-combined-release{1,2}.log` and
`/tmp/bri-v021-sol-integrated-{beta,acm}-release{1,2}.log`.

The 16-controller four-latch quiet and mixed phases made real objective progress
and reached the authored score 17/win, with 19,277/19,417 objective
controller-ticks respectively. Both mixed models had 761 fight controller-ticks,
41 distinct live projectile IDs and real damage. Quiet/mixed four-latch maxima
were 0.649/0.453 and 0.611/0.606 ms across repetitions. **Eight unordered
independent latches currently exceed the search model: zero objective progress
and 36,488 planning-budget/model-limit controller samples per phase.** Combat
continues, but this is a practical creator expressive ceiling. No budget was
inflated or acceptance reduced to hide it. Eight-latch quiet p95 was
2.593/2.575 ms; mixed p95 was 2.636/2.612 ms and maxima 2.923/2.925 ms. This
identifies bounded synchronous objective search as the next practical tail cost
in this fixture; future goal-directed search needs measured investigation.

Current real-city four-weapon phases also passed twice, with identical state
digests/live-projectile/navigation counters across each world's repetitions.
Beta City Gun p50/p95/max was 0.921/1.604/2.787 and 0.907/1.579/2.828 ms;
ACM City Gun was 0.200/0.408/2.150 and 0.203/0.419/2.189 ms. All distributions,
active-control subsets and projectile-count limits are recorded in
[the performance audit](../audits/v0.2.1-performance.md). No combined or city
step exceeded 5 or 50 ms. These city scenes use unchanged generated content:
its Blockhead Bot policy lacks source `objective:1`, so objective-enabled
coverage comes from the combined harness; release staging must carry the
current package policy. Current combat behavior differs from the original
paired query-optimization baseline, so the integrated rows do not claim the
old percentage saving on identical gameplay.

No further engine, test, manifest or content changes are planned in this lane.
Windows frame stalls and the original crash remain open; these macOS headless
measurements do not substitute for platform capture or Maxwell's playtest.
