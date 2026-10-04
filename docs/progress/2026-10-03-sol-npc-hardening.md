# 2026-10-03 Sol NPC planning, recovery and sustained diagnostics

Active v0.2.2 work on `codex/v0.2.2-hardening`; this note records the NPC lane,
not release or interactive acceptance. Original installations and the main
generated content remain read-only. Maxwell owns interactive gameplay.

## Bounded planner and truthful failed-search reuse

The previous uniform-cost search exhausts its existing node/candidate limits
on eight ordinary independent guarded switches. The new planner keeps the same
live limits (256 accepted nodes, 4096 candidates, depth 12, 32 actions) and uses
a bounded admissible estimate. It derives mandatory equality prerequisites
shared by every possible setter, excluding guards potentially supplied by an
earlier authored effect group. An Add effect makes that equality's prerequisites
ambiguous, so no prerequisites are inferred. Action costs are conservatively
partitioned across the predicates each action could establish; shared effects
cannot be counted multiple times against the same action cost. Satisfied
predicates stop dependency traversal, so consumed old prerequisites need not be
restored. Optional analysis has a 128-predicate graph and existing 4096-term
work ceiling; over-budget analysis falls back to the original zero estimate.

The same partition with unit action cost proves a lower bound on required
steps. Only an initial bound greater than the existing depth returns a proved
depth failure immediately. Sixteen independent actions remain outside the
supported depth envelope; the change diagnoses/reuses that failure rather than
pretending the larger puzzle executes. One action setting sixteen predicates
remains accepted. Search still returns no partial plan on budget exhaustion.

Each brain retains at most one bounded failed model. Every retry still grounds
the current authoritative model and compares current facts, action identities,
preconditions, ordered effect groups and game/round/team context. Complete
NoPlan/depth proofs survive changes to positive travel costs. Budget failures
require identical costs because queue order affects their outcome. Unsupported
grounding is not cached as a complete search proof. Global fair planning turns,
ordinary controls, event scheduling and all search caps remain unchanged.

Objective approach deadlines now account for control ticks spent in another
behavior. Scheduled event due/observation deadlines continue in real simulation
time. Bot Explain exposes cumulative searches/reused failures through root's
diagnostic hook; these counters are not wire state.

## Verification so far

- Actual-control objective suite: `cargo test --locked -p bri-chaos --test
  bot_objectives -- --nocapture`, `/tmp/bri-v022-sol-objectives1.log`, passed
  9/9 including ordinary four/eight-switch composition, delayed partial progress,
  inaccessible alternative and collateral/grounding negatives.
- `cargo test --locked -p bri-sim --lib session::bots:: -- --nocapture`,
  `/tmp/bri-v022-sol-bot-units1.log`, passed 59/59 before the additional depth
  proof tests. Planner correctness compares optimal costs with unguided search
  across 80 varied small models, and minimum-step bounds with unit-cost search.
- Standalone tests compile the actual owned planner source with `rustc --edition
  2024 --test /tmp/bri-v022-planning-test.rs`. Latest 20 deterministic cases pass;
  one optimized timing diagnostic is ignored by default. Its standalone unused
  `Unsupported` warning is due to the wrapper not linking live grounding.
- Expanded objective suite initially passed 10/11: failed-model reuse survives
  wandering and an ordinary LoadBuild adding a solution invalidates the cache
  and restores physical activation/score. The new interruption setup asserted
  an objective before its ordinary planner turn. It now waits ordinary ticks
  for acquisition. The final grounded interruption, source-add invalidation and
  ordinary four/eight-switch cases all pass in the 11/11 suite; each successful
  goal verifies canonical onRuleRoundEnd color FX. Latest compiled evidence:
  `/tmp/bri-v022-sol-controls11.log`.
- Latest scoped bot units pass 60/60, one diagnostic ignored:
  `/tmp/bri-v022-sol-bot-units2.log`. Strengthened controls uncovered test setup
  issues still being resolved: an all-white saved palette made a paint observer
  invisible after color remapping; the round-end observer now uses admitted
  color FX. A jet-follow opponent caused an escalating air chase, so the
  interruption fixture now uses grounded following. Two one-round weapons
  equipped successfully without damaging the target; missing live projectiles
  cannot establish whether shots missed or expired that tick. The inventory
  fixture now authors eye rays and records real Tracer cues to diagnose ammo
  recovery while retaining ordinary aim and turn limits.
  Existing direct/splash/charge/melee, close-blast and long-flight actual controls
  continue to pass. At that checkpoint the new ammo/rest fixtures were pending;
  final control results are recorded below.

Independent review also found shared scheduler defects: a resting/rider bot
could monopolize the next planner turn, and skipping the brain tick retained
Objective behavior without accounting for paused approach time. Root excludes
those controllers from planning selection and calls the owned `State::suspend`
hook at both early returns. Waiting event deadlines still advance. New ordinary
authorized Add-On add/rest fixtures verify other controllers retain turns and
a greater-than-30-second pause resumes to score plus canonical round-end FX;
their final coordinated results are recorded below.

## Sustained measurement setup and outstanding work

The headless battle harness now selects 12/16 bots, includes optional mixed
inventory, reports every ten-second window's combat activity and observed
health loss/deaths, active-step distributions, objective searches/reuse and
sampled process RSS. Its same-process session-construction repeats exercise
data/collision reload; they are explicitly not a live network map-change test.
Snapshots and MessagePack encoding remain outside the Session::step timer.
Observed live projectile IDs and sampled health changes are lower bounds, not
emitted-shot counters. Behavioral assertions require combat in every window
and damage; there are no wall-time pass thresholds.

The main generated Blockhead policy predates the published objective policy.
An explicit APFS content copy at `/tmp/bri-v022-content-policy` uses the exact
published `packages/blockhead_bot/assets/bots.json` bytes. Policy SHA-256 is
`8e7c291ff43dcffbdd1770c724bedb0537e085a203919e42a1a497f58a001870`;
provenance is `/tmp/bri-v022-policy-provenance.json`. Main content is unchanged.

Next: scoped clippy/regressions; coordinated optimized city and combined
objective/difficult-search measurements after root's body/Shark changes settle.
Preliminary standalone timing concurrent with a root compile is excluded from
acceptance evidence. Current sustained battle cost is not a comparison against
historical runs with different policy. Mac headless measurements do not establish
shipped FPS or a Windows crash/hang fix. Those reports remain unresolved.

## Additional native launch admission finding

Native muzzle rays can follow normalized projectile launch velocity, including
shooter inheritance, rather than the look vector the adapter validates. The
owned extraction now requires an explicit descriptor for inherited/converged
muzzle rays, eye-fallback paths and movement-dependent range. Eye rays remain
supported; straight zero-inherit positive-speed muzzle rays remain supported.
Player-collision-disabled rays are also excluded as non-actor attacks. A focused
moving-shooter negative proves launch and look differ and tests supported eye
and straight rays; the final linked bot suite passes this negative. This correction does
not add a ray predictor or change weapon runtime policy.

## Final bounded control batch

- Ordinary rest/resume: **2/2 pass**, `/tmp/bri-v022-sol-rest-tactics-final2.log`.
  A Shared bot-kind package declares the Server rest-rule companion, matching
  real provider ownership. Physical creator-owned spawners retain grounded
  objective authority. Initial LoadBuild required ordinary settling ticks;
  launching the next build before that scheduled operation completed failed
  with "There is another load in progress" and was corrected in the fixture.
  A resting earlier controller no longer starves the next ready controller.
  A greater-than-30-second pause resumes its approach to score and canonical
  round-end FX. Scheduled waits keep absolute deadlines.
- Actual weapon controls: **4/4 pass**,
  `/tmp/bri-v022-sol-tactics-protection-final2.log`. Four unfamiliar native
  weapon families still launch/hit; nearby splash risk chooses the useful
  second slot; an out-of-flight-envelope projectile closes distance and hits.
- Bot units: **61 pass, one timing diagnostic ignored**,
  `/tmp/bri-v022-sol-bot-units-final.log`. Includes optimal-cost comparisons,
  conservative depth proofs, cache invalidation, moving-ray admission and
  actual postmovement wall/friendly safety negatives.
- Root's liquid-sight regression: **1/1 pass**,
  `/tmp/bri-v022-sol-water-sight-final.log`. Water remains selectable by the
  building tool while sight traverses it and an ordinary solid wall occludes.

The depleted-magazine journey exposed a real readiness defect rather than a
missing-live-projectile artifact. Two actual eye-ray Tracer cues occurred at
ticks 74 and 93, but the target's canonical spawn immunity lasted 300 ticks;
both rounds were spent and both slots exhausted while target health remained
100. Evidence is `/tmp/bri-v022-sol-tactics-traces.log`.

The native adapter now reuses canonical `spawn_protected` at the postmovement
attack boundary. Choices retain their ballistic aim/movement. Proven non-firing
charge holds continue, and charge release is withheld while the target is
protected. Earlier drafts that skipped choice or cancelled every protected
charge regressed Rocket/Spear control; both were corrected, with all families
passing after protection. Two finite unfamiliar one-round eye-ray magazines now
emit exactly two rays after tick 300, damage the target to 80, mount both slots,
and emit no extra ray/damage after redrawing exhausted stored equipment. Shared
reserve remains zero; no refill or runtime damage-policy change was added.

The combined benchmark uses ordinary human Respawn commands when canonical
vitals allow them, retaining a live opponent after death rather than timing an
idle corpse. Per-window human health-loss deltas replace persistent damaged-state
samples. Quiet four/eight-latch phases assert the authored round-end FX; default
phase duration is 60 seconds. Optimized results are recorded below; activity assertions remain functional,
with no timing thresholds.

The long city fixture adds an explicitly labeled ranged encounter option:
`BRI_BATTLE_RANGED_SIDES=1` uses the same unmodified published Blockhead kind
on both sides. Their spawners belong to two ordinary authored creator principals;
both creators join through join_verified and ordinary MiniGame Join. Existing
brick-owner alliances and bot MiniGame membership provide opposition, requiring
no policy clone, special runtime handler or team schema change. Default retains
the actual stock Blockhead-versus-Zombie encounter, whose intrinsic conversion
can legitimately end activity. That baseline must be reported independently.
With REQUIRE_ACTIVE, each ten-second ranged window must show every configured
bot controller participating plus observed damage. Diagnostics distinguish living
Fight/Chase/Fly control activity from actual damage and lower-bound projectile
observations. The initial optimized Gun pilot verifies activity, while the longer Rocket
transition exposes a sustained-activity failure detailed below.

Unchanged prediction limits remain explicit: projectile interception extrapolates
observed target velocity; movement or geometry changes after launch can invalidate
that prediction. Native default Box actor bounds match the motor's collision box.
Custom Ball actors use the existing broader future endpoint bounds; ordinary
current sweeps remain exact. This pass does not claim a general future-shape
predictor, universal tool behavior, mounted-weapon readiness or vehicle role
opportunity-cost improvements.


## Serial optimized evidence, current candidate

Root reserved an exclusive build/measurement slot after combined clippy,
client startup/check and all control regressions passed. Release build used
`cargo test --release --locked -p bri-chaos --test bot_battle_perf --test
bot_combined_perf --no-run` with CARGO_TARGET_DIR unset; completed in 1m59s,
`/tmp/bri-v022-sol-optimized-build.log`. Engine/chaos/planner production source
matches candidate `abcb22e1`. Measurements run on this Mac headlessly, with no
concurrent compile/render. These are current costs, not a historical v0.2.1
comparison or shipped client FPS/Windows hang evidence.

The actual planner source compiled standalone with `rustc --edition=2024 -O
--test /tmp/bri-v022-planning-test.rs` and ran its ignored diagnostic, 200 serial
samples for each model/algorithm. `/tmp/bri-v022-sol-planner-optimized.log`:

| Independent switches | Search | p50 / p95 / p99 / max ms | Actual result |
| --- | --- | --- | --- |
| 4 | Zero guidance | .0355 / .0370 / .0445 / .0536 | Full plan |
| 4 | Bounded guidance | .0175 / .0185 / .0202 / .0263 | Full plan |
| 8 | Zero guidance | 1.7180 / 2.0195 / 2.2070 / 2.2832 | Node budget exceeded |
| 8 | Bounded guidance | .0898 / .0964 / .1079 / .1159 | Full eight-step plan |
| 16 | Zero guidance | 1.3094 / 1.4098 / 1.4969 / 1.7441 | Node budget exceeded |
| 16 | Bounded guidance | .0280 / .0355 / .0431 / .0473 | Proved depth limit exceeded |

This diagnostic compares the same bounded search implementation and models.
Zero-guidance directly invokes search; guided `plan` includes input validation,
action sorting and guidance analysis, so setup is included only on the guided
side. Neither variant executes gameplay here; ordinary-control tests and the
following composed benchmark verify delivery. The wrapper's unused Unsupported
warning comes from not linking live grounding, not the workspace clippy result.

The combined sixteen-controller optimized fixture passed six fresh phases,
7200 ticks (60 simulated seconds) each, in 10.07 wall seconds:
`BRI_COMBINED_TICKS=7200 target/release/deps/bot_combined_perf-b03210230cbd0589
--ignored --nocapture`, `/tmp/bri-v022-sol-combined-optimized.log`.

| Model/phase | Full step p50 / p95 / p99 / max ms | Active control p50 / p95 / p99 / max ms | Searches / reused |
| --- | --- | --- | --- |
| 4-switch objective | .091 / .180 / .258 / .645 | .099 / .211 / .267 / .357 | 42 / 0 |
| 4-switch mixed fight | .199 / .331 / .388 / .721 | .212 / .337 / .393 / .721 | 47 / 0 |
| 8-switch objective | .092 / .180 / .251 / .482 | .104 / .230 / .302 / .482 | 108 / 0 |
| 8-switch mixed fight | .205 / .339 / .415 / .627 | .214 / .345 / .418 / .627 | 82 / 0 |
| 16-switch objective | .091 / 1.190 / 1.310 / 1.716 | No active objective | 685 / 267 |
| 16-switch mixed fight | .221 / 1.342 / 1.495 / 2.182 | .246 / 1.377 / 1.523 / 2.148 | 951 / 1 |

Four/eight objectives each reach score17 plus the authored canonical round-end
FX. Quiet objective activity finishes after approximately 12 seconds; its later
quiet windows are inactive and are not sustained-planning performance evidence.
Mixed phases use a fresh session and ordinary human respawn commands, and every
one of the six ten-second windows records Fight control plus observed human
health loss (200 in the first window, 300 in every subsequent window). Four/eight
objective controllers finish their goal during mixed combat; fighting continues.
Observed distinct live projectile IDs are 98/103/109 for mixed4/8/16, not emitted
shot counts. No full-step sample exceeded5ms in these six phases.

Sixteen independent latches make no objective progress and expose bounded
model/planning diagnostics while ordinary combat remains active. Failed-search
reuse is not universal: these moving-controller models produce 685 searches and
267 reuses in quiet, and951 searches/1reuse in mixed. Fresh grounding/travel/facts
can invalidate the retained model, and full model discovery remains a tail cost.
The controlled no-plan fixture separately proves unchanged failures reuse and
fresh source changes invalidate; do not infer all sixteen-latch retry work has
been eliminated from the composed run.

### Sustained city acceptance failure retained

A 12-bot Beta City 16 pilot (8,537 bricks including spawners), ordinary creator
Blockhead opposition and published policy, passed all six ten-second Gun windows:
`/tmp/bri-v022-sol-city-pilot.log`. 7,162/7,200 active steps, all12 participants,
observed health loss8840 and86 deaths. Full step p50/p95/p99/max
1.008/1.537/1.842/3.169ms, no samples above5ms. Full-world snapshot MessagePack
p50/p95/p99/max2.978/3.153/3.320/3.320ms, measured outside Session::step;
this is not the host's incremental network workload. Sampled process RSS Gun
start250464KiB/end251136KiB; session construction1348.096ms. The fixture holds
converted-world/catalog allocations; this is no GPU or client memory evidence.

The intended 300-second-per-phase, two-construction-repeat matrix failed during
the first Rocket phase after Gun. Exact command used BRI_BATTLE_WORLD='Beta City
16', BRI_BATTLE_BOTS=12, BRI_BATTLE_RANGED_SIDES=1, BRI_BATTLE_TICKS=36000,
BRI_BATTLE_REQUIRE_ACTIVE=1, BRI_BATTLE_MIXED=1, BRI_BATTLE_RELOADS=2.
`/tmp/bri-v022-sol-beta12-sustained.log`:

- Gun: all30 windows show damage and all12 controllers participating, 35,600
  active/36,000 steps; step1.016/1.501/1.790/8.350ms (p50/p95/p99/max),
  two above5ms, none above50ms; evidence fingerprint3c077e06b24f3f7f.
- Rocket: step.702/1.213/1.523/19.406ms,18 above5ms, none above50ms;
  observed damage5410/deaths58/67 distinct live projectile IDs, but19/30 windows
  fail sustained damage/full-participation acceptance. Last four windows have
  zero Fight controller ticks despite visible Chase/Fly keeping coarse active
  step counts true. Fingerprint769ca55027516532. These lower timings are not
  accepted as sustained-combat improvement.
- The failure stops the harness: Gravity, Spear, mixed and second construction
  repeat were not measured by that command. Stock Rocket metadata has no finite
  magazine; no ammo-exhaustion conclusion is justified. Root authorized public
  phase-end diagnostics and an explicit ordinary brick_damage setting for
  investigation, preserving canonical default=true. Production source remains
  unchanged. Cause and follow-up results are pending below.


The canonical default=true replay with public phase-end diagnostics reproduces
identical gameplay counts and fingerprints: `/tmp/bri-v022-sol-beta12-diagnostics.log`
(81.47s, test passes only because REQUIRE_ACTIVE was omitted to collect evidence;
it does not pass the sustained criterion). Gun has43,290 observed health loss,
429 deaths and2,023 live IDs; Rocket5410/58/67. Gun step
1.032/1.525/1.815/10.745ms,8above5; Rocket.697/1.210/1.531/6.992ms,5above5;
none above50. Default geometry and production code are unchanged; only reporting
and a currently unused explicit brick_damage toggle were added to the harness.

At Rocket phase end, all8,537 world bricks/all12 vehicle-spawner bricks remain;
all12 bots are alive with100health, mounted Ready Rocket launchers and ordinary
unlimited lives in the same MiniGame. No missing-spawner, exhausted-magazine or
ended-round explanation is supported. Ten controllers remain Fly/Chase with
near-zero velocities and crowded feet around x105-106,y288-292,z98-103; visible
enemies are approximately1-4m away, inside useful Rocket splash-clearance range.
Several are jetting with ceiling/body contacts. Two others Search. The safety
guard correctly withholds close blast attacks. Source review identifies a
plausible movement recovery gap: ranged Fly motor ignores the pursuit back_off
result, often applies jet with zero horizontal direction below the target or
at crowded altitude. This is evidence for flight/contact/standoff investigation,
not a confirmed general collision root cause or a fixed mechanic. Root owns the
shared motor. The no-brick-damage option was not run and no causal claim is made
from its availability. The failing criterion remains intact.

Replay RSS Gun start251600KiB, sampled max252304KiB, end178784KiB;
Rocket start178784/end178912KiB. The previous failing run retained a larger
allocation residency; neither sample alone establishes an allocator leak or
true peak. The matching canonical fingerprints permit a paired replay timing
comparison; no randomized behavior difference is used as speedup evidence.


### Narrow recovery correction authorized after the failure

Root subsequently prioritized fixing the reproduced generic movement deadlock
and granted only the ranged Fly branch in shared bots.rs. The proposed branch
was reviewed before editing. Actual visible close enemies (computed back_off or
full3D distance below the current weapon's near band) now release lift and steer
away with the same deterministic sidestep used by Fight. A target directly
above but outside3Dnear does not force retreat. Ranged roofed/actual-ceiling
contacts release blocked lift and keep existing horizontal escape. Existing
melee approach, charge/trigger controls, native safety and work caps are retained.
No aim/control/state or transform is injected by the new test: its elevated
stationary enemy, nearby physical spawner and low roof are authored before
ordinaryLoadBuild/MiniGameCreate. Fixture and canonical city regression evidence
remain pending. The original acceptance failure is not removed or relabeled.
The exclusive measurement slot was released so the newly authorized projectile,
guest-saving/music and screenshot work can proceed; new timings will wait for
production source stability and single-process measurement scheduling.


Read-only host audit: net/server.rs's periodic update drains dirty bricks and
builds changed tools/weapons/vitals/entities plus other typed deltas. Ordinary
broadcasts serialize that Delta, not this harness's entire Snapshot.
Checkpoint::from_session uses the persistent brick map for deferred streaming.
The host PerfWindow measures Session::step separately from replication/encoding.
Consequently the3ms full-world diagnostic encoding is not attributed to actual
20Hz host traffic; headless Session timings also exclude those host costs.
Actual host-loop, client rendering/debris and Windows hang/crash costs remain
unmeasured here. No network/protocol implementation was changed by this lane.


## New physical-objective request: read-only feasibility

While correctness builds were held for coordinated new projectile/music/JPEG
source edits, root requested discovery for creator-authored ball/hoop/vehicle
return and future flag goals. No implementation was started. Current objective
index/model/observation ledger admit activation, player region entry and bot touch,
not ObjectEnter. Projection supplies no Object context. Canonical rules already
observe runtime vehicles (including balls), require the object's actual spawn
brick to share the region creator, exclude foreign-game mover credit, and
attribute Player/Instigator to real contact/hold credit or the actual driver.
Object kind/spawned-by/speed are canonical queries; delayed rows capture real
context and evaluate guards at due time. Package entities/flags are not included
by the current vehicle-only ObjectEnter observer.

The small proposed extension is an internal object+region delivery step backed
by native push/board/drive mechanics and explicit declared hold-tool mechanics,
composed through existing bounded objective actions. Existing interaction
steering aims at enemies, and legacy Carry automatically seeks open space and
flings; both need narrow goal/control integration rather than a mode handler.
GravityGun hold/command hints do not declare its authored90,000force/min2.5/max60
or release semantics. Unknown scripts cannot be inferred from names. An explicit
package-owned descriptor must match the real provider/image and execution must
observe actual held-object identity/permission/progress. Static descriptor travel
estimates choose only eligible methods with switching hysteresis; actual scoring
comes exclusively from native ObjectEnter and captured-object/due-time effects.

Suggested acceptance is realownedspawner delivery with unfamiliar IDs, switch
prerequisites plus physical delivery, elevated hold-tool delivery, actual wheeled
return, parameter-based push-versus-grab choice, and wrongspawner/game/claim,
changed goal, losthold and delayedguard negatives. Existing8object discovery,
32action/128target grounding and global work limits must stay bounded and be
measured with16active controllers. Arbitrary CTF packages, scripted collateral
and speed-constrained trajectories are not automatically supported. Root received
exact file boundaries/proposal; implementation waits separate approval and does
not replace the pending ranged-flight regression/canonical city replay.


### Focused recovery correctness and explicit capability pause

After root froze shared dependencies and granted the single Cargo slot:

- `cargo test --locked -p bri-chaos --test bot_tactics -- --nocapture`: all5
  actual-control fixtures passed (0.21s). The new low-roof/elevated-enemy Rocket
  fixture observed close Fly, lift release and recovered blast clearance before
  a surviving native projectile; canonical human health changed100→11.465359.
  No actor transforms, brain goals, safety bypasses or simulated score inputs
  were used. Full existing direct/splash/charge/melee, safe second-slot, finite
  ammo/protection and slow-flight approach regressions also passed.
- `cargo test --locked -p bri-chaos --test bot_brain
  a_jetting_bot_closes_on_an_enemy_on_a_high_brick_platform -- --nocapture`:
  existing melee flight regression passed (0.35s).
- `cargo test --locked -p bri-weapons --test image_seams -- --nocapture`:
  all6 image schema/state tests passed, including explicit manipulation limits.

Cargo was released immediately. These correctness checks do not complete the
canonical sustained city Rocket replay, whose failed prior activity windows are
still recorded above. No new optimized benchmark or Windows fix is claimed.

Root then relayed Maxwell's explicit stop-before-expanding instruction. New NPC
capability implementation is paused. The frozen fourteen-journey acceptance note
exists at `docs/audits/objective-driven-integration.md`; all new journeys remain
open. `bots/physical_objectives.rs` is an unlinked, uncompiled draft, preserved
for review. It has no runtime effect. Known gaps include cumulative discovery
work before filtering/source cross-products, actual surface-grip distance for
elevated hold acquisition, claims/selected driver continuity, mechanical hand/
projectile methods and broad metamorphic actual-control evidence. No additional
provider edits or objectives/module integration were made after the pause.

The read-only actual-pipeline audit sent root confirms the current desired state
is only projected WinRound for the bot; current grounded actions remain whole
activation/player-region/bot-touch inputs with canonical grouped guards, delay
barriers and scalar effects. Root's newly expanded ObjectEnter index/captured
ledger alone does not make ObjectEnter a planner action. Ordinary activation/
walking/contact observations execute the current objectives; no planner writes
canonical success. Existing enemy-driven interaction and open-space Carry are
control adapters, not physical-goal providers. Unknown tool legacy fallback
still estimates short-range no-projectile tools as melee movement, and must not
be described as generalized typed tool understanding. No production branches
on example game/content identities were found in the audited bot/planner files.

### Rule abstraction checkpoint and typed provider lifecycle

After the actual-pipeline audit, root authorized resumption under
`docs/v022-delivery-contract.md`. The first linked refactor separates desired
RulesRound state and exact causal rule projection from available Brick controls.
Activate/Region/Touch now use provider-owned live geometry validation, ordinary
activation/movement, captured admission and canonical observed progress. The
single grouped rule interpreter and pure bounded planner remain unchanged.
Native admission previews are prefix effects on hypothetical facts; current
Alive/Score facts and the actual world remain unchanged by projection.

Root's serialized baseline passed 67 bot units (one ignored), both rest/control
fixtures, all11 existing objective fixtures, and all8 independent creator
acceptance fixtures. The old fanout negative expectation was updated to the new
precise `objective model/grounding budget exceeded` diagnostic; its actual budget
failure was retained. Independent cases include canonical player/team winners,
translated/renamed/reordered checkpoints and puzzle composition, active rules
and team changes, and precise depth/action negatives. Logs:
`/tmp/bri-v022-creator-baseline.log` and root's abstraction/rest baseline logs.

The next source checkpoint adds typed Rule/Package causes, Package executors and
ordinary-control View fields. Package actions do not invent EventWorld inputs.
Real carriage changes advance pickup; the declared canonical counter completes
return. Desired state retains its original completion baseline across action
replanning. One globally fair turn selects one desired candidate and performs at
most one bounded search; impossible desired candidates cool and rotate rather
than permanently masking an available package task. Rule and package actions
share facts, preconditions, effects and all existing model limits. Failed-search
reuse now also compares complete desired identity/completion, while a sole
unchanged native goal still reuses its negative result. Package-only snapshots
do not require Wrench sources or an event world. These new typed changes are
source-written, pending the next serialized correctness checkpoint; they are
not yet acceptance evidence.

The unlinked physical draft now scans observed bodies once per model, charging
visits before filtering, and reuses that bounded set across sources. Hold derives
surface-grip geometry from the native helper and uses native lift-capacity
admission instead of duplicating integration constants. It keeps a real hold
through captured input/delay settling; arrival alone does not release or declare
success. Shared controls, claims and seat routing remain root-owned. Physical
journeys and the post-fix sustained city replay remain open.

### Linked physical, compound-rule and Enemy checkpoint

The typed provider checkpoint is now linked. Root's serialized compile passed
with the actual-life DeathResult observer; physical real-control acceptance is
still pending. The common lifecycle represents Rule, compound Rules and Package
causes, with Brick, Physical, Package and Enemy executors. Native causal prefixes
precede globally delay-ordered guarded groups, using the single existing rule
projector. Enemy waits require both exact actual-life credited death and actual
per-source victim-input admission, then authored due time; the observation is
not inferred from predicted damage.

Physical discovery charges every visited body before filtering once per model,
then reuses the bounded set across creator sources. Candidate metadata is charged
before cloning. Exact original body/spawner/name/definition/scale and source
region stamps are revalidated, as are tool descriptor, permission and occupancy.
Canonical Kind/SpawnedBy text guards use the existing condition comparator;
false immutable guards do not generate an impossible decoy action. Native grip
geometry/lift admission comes from movables rather than a second integrator.
Actual credited improvement toward the goal renews the shared claim; attempted
inputs or unrelated velocity do not. Root owns all equip/trigger/boarding and
chassis controls, exact held-object Carry suppression, cross Body/Seat claim
conflicts and preemption. The provider never moves objects or awards success.

Live physical rejection reasons now pass through the common Step invalidation
and execution failure, distinguishing missing/changed object, permission,
occupied body and unsupported geometry. Cheap combat availability retains unknown
script fallback, including native volley/last-shot metadata, so a declared known
noncombat hold tool can use objectives without masking unknown attacks.

New `bot_physical_objectives.rs` authors paired unfamiliar contact/ground-control-
seat/declared-hold/elevated-hold scenes. It asserts actual displacement, selected
method, exact held object or seat occupancy and actual credited RoundResult.
Hold uses the shipped generic Rhai physics commands under renamed package/tool
IDs. These fixtures have not yet run; no physical journey is marked complete.
Original installations/content are unchanged. Post-fix sustained city replay,
interference/identity/tool-change negatives, hands/physical-hit breadth and the
independent held-out composition remain open.

The first physical actual-control run passed the paired contact delivery, but
exposed a method admission defect: all methods used native physical-grab
`may_move`, which intentionally forbids moving one's own ridden vehicle.
Ordinary successful boarding therefore invalidated Drive. The provider now
checks native contact/hold permission only for those methods and uses existing
ride/seat/occupancy permission for Drive. Unmounted approach calls the extracted
shared oriented seat geometry, without another boarding executor. Hold fixtures
initially replaced the synthetic weapon pack and invalidated configured item
physics; they now merge their authored hold items/images, preserving validation.

`/tmp/bri-v022-grounded-control-regression.log` then passed all11 legacy objective
fixtures and three of four paired physical families: contact, ground control-seat
and cheaper declared hold. The interruption fixture retains its >30-second
requirement and now authors an actually useful native positive-damage Gun before
setup; real hostile health changed100→98.32964, then ordinary disconnect allowed
canonical objective completion. An unarmed staring contest is no longer treated
as proof of combat preemption.

Elevated Hold still failed: it made real credited displacement/lift, then lost
its claim without a winner. Read-only shared-control analysis found the legacy
Carry state still initializes while `objective_holding` suppresses only utility;
its later swing can clear locomotion for the selected Objective. This is a second
control owner, not an elevated-goal success. Root received the narrow exact-held-
objective Carry suppression finding; its fix/rerun is pending. No acceptance or
performance claim was weakened to accommodate this failure.

Root corrected exact-held-object control ownership: active Physical Hold clears
legacy Carry state rather than merely suppressing Carry utility. Elevated Hold
then passed both unfamiliar translated variants. The longer quarter-turned
chassis exposed a separate ordinary mounted-control issue. Read-only analysis
established reverse body heading must target goal-heading+PI and reverse steering
must account for actual rolling direction; the former adapter reused forward
heading error while reversing. Root corrected the common native controller with
coherent reverse heading, actual signed-speed response, yaw-rate damping and
ordinary corner/arrival braking from authored speed/brake/mass. Claim limits and
canonical physics stayed unchanged.

`/tmp/bri-v022-native-drive-heading.log`: all4 physical actual-control fixtures
passed (0.33s), each with its renamed/translated variant. Assertions cover exact
native holding, actual own control-seat occupancy, body displacement and actual
credited RoundResult. The oriented variant lengthens the ordinary chassis hull
and rotates its authored spawn; it was retained when it failed. Root's shared
bot-unit batch passed71 (one ignored), including conflicting Body/Seat claims
and conservative hybrid script-attack classification. Native Image.scripts with
an independent projectile must retain combat fallback even alongside a hold
descriptor. Contact/hold/ground-control-seat mechanisms are now frozen for the
independent held-out composition. Identity/interference/tool-loss/delay negatives
and sustained optimized current-source measurements remain open; no broader
native hand-flip or weapon-to-object action-provider support is claimed.

The next physical8/package7 checkpoint kept all seven package fixtures passing,
but physical delay positives/guard-negative and the over-limit body negative
failed (`/tmp/bri-v022-physical-package-adversarial.log`, physical5/8). Read-only
inspection found two fixture admission errors before the intended cases:
SavedBuild's eight identical white palette entries map into synthetic live white
index3, so the authored Self Color0 due-time guard was false from setup; default
per-player native physics-vehicle quota5 prevents creating the ten requested
bodies. Neither failure proves hold claim expiry or a provider-budget escape.

Fixture-only corrections preserve the live palette and assert actual initial
goal Color0, and configure ordinary server physics-vehicle admission to ten,
then assert all ten real bodies exist within24m of the synchronized bot. The
physical discovery limit remains8; no planner, authority, claim or timing limit
was changed. The corrected delayed admission, six-second grip retention, due
guard change and over-limit workload remain pending their scheduled rerun.

`/tmp/bri-v022-delayed-hold-corrected.log` then passed7/8, including both delayed
admission cases. The explicit ten-body setup assertion still saw only one:
extra spawn bricks had integer centers and were filtered out by ordinary
build-grid admission; half-unit plates require quarter-unit centers. Their
authored coordinates now obey that grid, with independent loaded-spawner,
actual-body and actual-distance checks. Delay success is now tightened from
ever-held evidence to exact native grip on every tick until the due winner.

Independent source review also found a genuine causal-model gap: Physical
ObjectEnter effects could be repeated by search while the executor kept the
body inside the region. The scoped repair adds the existing Region/Touch
rearming semantics to the same physical provider: initially occupied exact
regions select a bounded nearest horizontal exit waypoint, ordinary contact,
hold or seat controls move the original object, and only observed native
center outside clears rearm before approaching again. No event, rule or
physics state is injected. Reentry travel participates in the existing cost;
budgets are unchanged. Paired two-entry counter/winner cases now cover all
three methods, including authored initially-inside variants. These additions
are source-stable but unverified until the scheduled physical9 rerun.

Physical9/legacy checkpoint (`/tmp/bri-v022-physical9-legacy.log`) passed all11
legacy tests and7/9 physical tests. The ten-body negative now proves actual
admission and bounded rejection. Tight grip sampling exposed a real delayed
hold failure previously hidden by eventual winner/ever-held evidence. Repeated
entry also failed. Bounded transition traces established successful first
admission followed by legacy interaction treating the self-owned objective
claim as a mismatched enemy claim, putting the same resource on cooldown before
the next action. Root corrected success/preemption handoff without failure.

`/tmp/bri-v022-physical9-common-approach.log` still passed7/9: actual rearming
now starts and achieves a real exit, but approaching the rear directly through
the moving body pushes it away from the goal. The static navigation grid cannot
route around this dynamic hull. Separately, delayed Hold circles its destination
as body-to-goal residual direction flips near arrival; actual native grip then
snags. Advisory release after real admitted held waiting did not alone fix the
grip failure. These are retained control failures, not reduced acceptance.

The next scoped provider checkpoint uses constant finite expanded-hull arc
geometry for ordinary contact approach, with outward-only recovery and actual
rear alignment; native motor/collision still owns movement. Left/right, rotation,
inward/nonfinite and chord-clearance unit cases accompany the unchanged real
journeys. Hold corrects range along actual actor-eye-to-desired-grip sight rather
than the singular body-to-goal residual. Root will extract the proven contact
helper for existing combat Push to keep one mechanical owner. Current control
changes are pending actual-control verification. No physics, claim or planner
budget was raised.

Battle and combined diagnostic harnesses now report existing per-package VM
milliseconds, drained outside timing per phase. VM work is included in full
Session::step timings; VM counters exclude native package snapshot, own-state
and entity-variable cloning, and query/model setup. Descriptor8192-byte and
VM work limits do not bound those snapshots/clones; existing native entity/store
limits do. Provider-enabled sustained measurements remain mandatory.

`/tmp/bri-v022-physical9-bounded-approach.log`: all9 physical actual-control
fixtures passed (1.17s) with the scoped arc/radial repair. This includes exact
native grip throughout the six-second admitted wait, a due-time guard change
that produces no fabricated winner, ten actual nearby bodies rejected within
the unchanged discovery budget, and physically observed repeated exit/entry
plus actual RoundResult through contact, declared hold and control seat. The
repeated-entry variants include both outside and authored initially-inside
objects. The independent creator interference/identity/replacement/tool-loss
rerun and common helper extraction remain pending; source stays frozen until
that run completes.

Root extracted the proven expanded-hull approach into the existing native
interaction owner. Physical now imports `interactions::push_point`; legacy
combat Push uses the same geometry and actual pushing readiness. No local
duplicate remains. `/tmp/bri-v022-linked-controller-rest.log` reran all9
physical tests successfully (1.02s), with physics interactions11, Search4 and
tactics5 also passing. The mounted native charge regression subsequently
passed all15 interaction fixtures after root fixed cancellation before an
authorized release (`/tmp/bri-v022-mounted-release-corrected.log`).

The independent tool-disappearance negative produced a real, credited physics
completion after repair had already stopped. Its bounded chronology observes
tool loss at tick200, no native grip and no grounded plan; the freely moving
original body enters the exact region at371 and canonical score31/winner is
recorded at374. This is ordinary credited coasting, not invented planner
completion. The independent owner is reviewing the acceptance assertion; no
provider erases native momentum, credit or legitimate events to prevent this
outcome. The other three creator adversarial cases pass.

Final strict library clippy exposed a392-byte package completion variant next
to a24-byte native round variant and two needless context borrows. The package
stamp is now boxed, with its sole construction coordinated with the package
owner; context borrows are removed. This storage cleanup changes no desired
state, observation, budget or execution behavior. Its strict rerun remains
pending. Final optimized current-source city fights and provider-cost reporting
remain outstanding; earlier timings do not cover the newly linked providers.

Root's coordinated rerun subsequently passed strict sim-library clippy
(`/tmp/bri-v022-sim-library-clippy-corrected.log`, 9.64s) and all184 sim library
tests, including the previously ignored planner diagnostics
(`/tmp/bri-v022-sim-library-boxed-final.log`, 4.55s test execution). Package
completion boxing and the owner-specific cleanup therefore compile and verify
together. These correctness timings are not optimized performance evidence.

A final bounded-discovery review found the creator's indexed source set was
cloned before the existing64-source admission check. That check now precedes
cloning, preserving the exact failure and model limits while avoiding a full
copy of an already rejected creator index. Root approved this narrow change;
the next focused regression checkpoint includes it. No timing benefit is claimed
without measurement.

Independent native chronology exposed an actual control-recovery gap alongside
a fixture identity mistake: a hold ray briefly acquired a nearer decoy while
the actor approached the intended body. Physical previously recognized only
the correct grip but kept the trigger down for any wrong grip. The directive
now requests ordinary trigger-up for the bot's observed different held object;
it keeps the same approach/deadline and cannot force a ray, grip, position,
impulse or credit. Root is separating selected hold-tool control ownership from
actual exact-object holding, so legacy Carry cannot steal this release/retry.
Exact holding still alone qualifies for admitted-wait lease handling. The
independent identity-preserving loss fixtures and physical9 will verify the
coordinated recovery; no outcome is claimed before that rerun.

`/tmp/bri-v022-final-ordinary-controls.log` verifies the coordinated wrong-grip
release and count-before-clone source: creator acceptance8, independent held-out
composition1, native interactions15, rest2, legacy objectives11, physical9,
physics interactions11, search4 and tactics5 all pass. Physical9 includes exact
six-second native grip retention, permission/guard/budget negatives and genuine
repeated entry through all three methods. The independent identity-preserving
tool-loss/adversarial closeout and final optimized sustained measurements still
remain distinct acceptance work.

`/tmp/bri-v022-native-loss-guard-final.log` closes the independent creator
adversarial checkpoint: all5 pass (2.60s), with actual imported CTF1 and typed
carryable7 also passing. Loss callbacks now retain the original exact object
identity instead of comparing a decoy baseline with the selected body. The
tool-loss positive observes release/invalidation and accepts only a genuine
native coasting outcome; the paired due-time guard negative observes actual
guard revocation and rejects completion despite physical entry. Native momentum,
credit and rule semantics are preserved. Exact-spawner decoys, replacement after
real movement and two-bot competition also pass their transformed variants.
The remaining closeout in this lane is strict affected-target checking and
exclusive optimized, provider-enabled sustained city/combined measurements.

Final exclusive measurement began from committed `c66bdff0`, with fresh APFS
content snapshot9471files/392,843,334bytes, SHA256
`161735207b50e5abe574a5b86fca698c9cd969210f74271888a7cd6e98a16a92`.
The first30-second Beta City12-controller Gun/Rocket pilot passed real damage
and all-controller participation in every10-second window. Gun step
p50/p95/p99/max1.086/1.682/1.955/3.769ms; Rocket0.770/1.344/1.594/3.020ms,
2419.4observed health loss and26deaths. These are plain-battle pilot costs.

The newly explicit loaded-provider report exposed a coverage gap: the default
package selection loads no objective-query providers even though the installed
CTF manifest opts in. Root approved a harness-only ordinary selection flag,
`BRI_BATTLE_CTF_PROVIDER=1`, adding the installed Slayer and CTF packages and
following their declared server companions through the existing package loader.
The harness requires the actual CTF query namespace to be loaded before timing;
it preserves default plain selection when the flag is absent. Final source
provenance/rebuild and a new provider-enabled activity pilot precede long runs.
Without authored flag sources these city fights measure empty-offer discovery,
not populated CarryReturn action grounding. Imported CTF/package acceptance
proves that separate actual-control path; total VM duration is not a query count.

The first explicit-provider attempt correctly refused before timing because the
new selector incorrectly guessed package version1.0.0; installed Slayer is4.1.5,
so ordinary validation excluded it and its dependents. No provider-enabled
measurement was recorded. The selector now reads installed `PackageInfo`,
checks its exact ID, uses its actual version and canonical `side()` derivation,
then retains ordinary loader validation/companion following and the loaded
provider requirement. Root approved the narrow correction and checkpoint
rebuild; snapshot and production code remain unchanged.

The corrected explicit-provider pilot (`20fa89e3`,
`/tmp/bri-v022-sol-final-provider-rocket-pilot2.log`) passed both 30-second
Gun/Rocket phases with the actual `gamemode_slayer_ctf-rules` namespace loaded.
Its damage/participation fingerprints match the plain pilot. Gun step
p50/p95/p99/max was 1.137/1.903/3.441/9.713 ms; Rocket was
0.797/1.513/2.133/5.771 ms. This measures empty-offer discovery; the installed
provider receives no authored flag/team offers in this diagnostic. VM duration
is neither an exact query count nor the full snapshot/model preparation cost.

The first full 300-second Beta City 12-controller run remains a failed receipt:
`/tmp/bri-v022-sol-final-beta12.log`, source `20fa89e3`, unchanged snapshot
SHA256 `161735207b50e5abe574a5b86fca698c9cd969210f74271888a7cd6e98a16a92`.
Gun passed every strict 10-second activity window, with 42,770 observed health
loss/425 deaths and step p50/p95/p99/max 1.043/1.552/1.820/7.252 ms. Rocket
had 35,973 active steps of 36,000, 25,135.2 observed health loss/253 deaths,
and step 0.754/1.283/1.639/10.926 ms, but failed windows 16, 18, 19 and 21.
Window 16 had all 12 participating controllers and 12,916 Fight ticks, with
zero damage; the other three had 11 participating controllers and real damage.
These do not establish a permanently silent brain or a physics/runtime stall.
All 12 spawners survived, and phase-end living peers retained Fight/Fly/Search
control. One apparent overdue corpse was still within the canonical extra
120-tick brick-bot respawn wait. No Windows FPS, crash fix or measured v0.2.1
speedup is claimed. Later weapon/reconstruction phases did not run after this
failure; they remain required work.

Root approved bounded harness-only window diagnostics without changing the
strict acceptance condition. Reports now retain exact participating/missing
IDs, per-controller life/visibility/behavior/mounted-state counts, actual spawn
transitions, and distinct observed live projectile IDs by source. Strictly
failing windows capture their public end poses, vitals and thoughts. All reads
occur outside timed `Session::step`; mounted states and sampled projectile IDs
are observations, not a complete shot-event counter. Production and inputs are
unchanged. The original failed receipt remains preserved, and a final-source
replay must establish the specific missing-controller and zero-damage causes.
The exclusive compute lease was explicitly released to root for reviewed
rendering integration; no NPC Cargo or timing run is active during that work.
