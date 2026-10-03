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
phase duration is 60 seconds. Neither timing nor sustained-activity results have
yet been accepted; optimized runs remain outstanding.

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
observations. No timing or activity success has yet been inferred from setup.

Unchanged prediction limits remain explicit: projectile interception extrapolates
observed target velocity; movement or geometry changes after launch can invalidate
that prediction. Native default Box actor bounds match the motor's collision box.
Custom Ball actors use the existing broader future endpoint bounds; ordinary
current sweeps remain exact. This pass does not claim a general future-shape
predictor, universal tool behavior, mounted-weapon readiness or vehicle role
opportunity-cost improvements.
