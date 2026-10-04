# 2026-10-03 Native body delivery controls

Lane: GPT-6.1 Sol High; root owns integration and serialized verification.

Maxwell's actual candidate playtest reported weak walking pushes, staring and
combat distraction while trying to deliver a Steel Ball to an authored
`onObjectEnter` / Instigator win. He also observed a successful Gravity Gun win.
The working physical hold remains supported; this change addresses concrete
missing native controls and retention, without a game or content-name handler.

Source findings and changes:

- `Method::Push` previously requested locomotion only. It now additionally
  approaches from the destination's rear axis and uses ordinary empty-hand
  `Activate` / `ActivateRelease` after exact native ray, permission and velocity
  preflight. The shared native vehicle activation still owns its mass-scaled
  impulse and movement credit. Native clicks are bounded in frequency and never
  remain held. Walking contact remains the fallback for a moving body.
- Native Hammer delivery uses ordinary inventory selection and image triggers.
  The same typed callback resolver drives human `tool_fire` and NPC admission;
  native tool ray and vehicle permissions are shared. Hammer combat similarly
  advertises only the actual native contact attack and traces the tool's ray,
  including non-raycast bricks. No bot code applies a force or damage directly.
  Item IDs and item labels can vary; the installed engine callback identity
  cannot be inferred for arbitrary renamed or overridden image scripts.
- A damageable visible creator previously selected Fight (.8) over Objective
  (.65) when the bot had a Gun. A noncombat desired state now retains attention
  unless dated, actual hostile damage evidence makes combat an interruption.
  Enemy desired states retain ordinary fight/pursuit and existing utility.
- An objective resource lease previously ended at 15 seconds even during useful
  transport; approach timed out at 30 seconds. Renewal now requires accumulated
  new distance reduction greater than .25 units and actual own movement credit.
  Retreat/oscillation, attempted input and tiny jitter do not renew the transport
  deadline. Discovery, planner, model and work limits are unchanged. Idle leases
  still expire rather than monopolizing a body.

Initial validation state: **not yet verified**. Root serializes compilation with
an independent mounted-charge correction. Single-file rustfmt and diff whitespace
checks pass. The first root compile exposed an obsolete geometry-test helper
reference after sharing `push_approach`; its thin test-only point adapter was
restored, keeping all geometric assertions.

Focused verification requested:

```sh
cargo test --locked -p bri-sim --lib session::bots::claims::
cargo test --locked -p bri-chaos --test bot_physical_objectives -- --nocapture
```

New ordinary-control regressions require a real Hammer delivery and canonical
winner, delivery rather than attacking a passive visible creator with a Gun,
real human weapon damage followed by an actual combat interruption/return fire,
and a long horizontal native hold delivery. The existing hand delivery now also
requires an observed ordinary native click. The Hammer variant renames the item
while retaining the genuine native image callback.

Known boundary: horizontal Hold uses the existing bounded segmented walking
route. Jet transport to destinations beyond a grounded holder's vertical reach
is still unsupported: the present Fly controller follows enemy evidence, while
physical Hold pins its holder point to current floor height. Reusing/refactoring
that controller needs separate ordinary-control proof; this patch does not claim
flight, an invented throw trajectory, or universal manipulation of unknown tools.

No interactive gameplay, original installation changes, commits, manifests,
lockfiles or protocol edits in this lane.

Root's first compiled checkpoint: claims **8/8 pass** in
`/tmp/bri-v022-npc-claims-regressions.log`; native Pong regressions **9/9 pass**
in `/tmp/bri-v022-pong-native-regressions.log`. Full delivery fixtures remain
pending. The approved next source checkpoint releases Hammer's trigger during
an already admitted delayed outcome (Hold retains its grip), clears injury
priority on life/game/round/team invalidation, and strengthens the hand/Hammer
variants with differently named 900/1200-mass, 1.25-half-width bodies. This is
synthetic physics evidence; it does not copy original game art or scripts.

Final physical checkpoint: **13/13 pass, 1.37 seconds**, root command
`cargo test --locked -p bri-chaos --test bot_physical_objectives -- --nocapture`,
log `/tmp/bri-v022-native-body-delivery-final-tests.log`. Heavy hand clicks and
Hammer swings produce real object motion and canonical wins; passive-creator
Gun retention, the credited attacker/retaliation case, long horizontal Hold,
prior permission/delay/budget negatives, and repeated actual region exit/entry
all pass. This is focused headless evidence, not release or interactive signoff.

Two fixture corrections were required, without production changes to fit them:

- A semi-automatic human Gun needs actual release/repress after its aim has
  arrived; the first draft held one off-target initial press. The negative now
  uses ordinary repeated clicks, stops on death, accepts the existing Fly combat
  phase, and requires canonical death attribution to the bot after real injury.
- Native hand activation became a legitimate cheaper alternative for a short
  vehicle exit/reentry. The powered-seat/Hold repeat cases now author longer
  real exit/entry distances (20-unit region and matching outside start), preserving
  required method, initially inside/outside, at least two physical entries and
  canonical counter/winner assertions. The hand alternative stays available.

The common executor still reports `rearm` until an actual exit, while executing
its real selected controls; the initial diagnostic-only regression was corrected.
Actual native Hammer combat elimination remains a requested additional focused
fixture before claiming that capability is verified beyond its linked admission.

The subsequent source-only checkpoint closes the independently found out-of-band
injury gap: a dated real injury, an available attack, and the existing pursuit
leash temporarily withhold a non-Enemy objective view. The selected common Step
and completion baseline remain intact; the existing control accounting pauses its
approach deadline. During that pause, new environmental interaction discovery
does not displace ordinary Chase/Search. Passive visible creators and known
noncombat bots retain their useful delivery. Enemy-objective and native weapon
charge execution are unchanged by this patch.

Added `an_injured_hammer_bot_pursues_a_ranged_attacker_outside_its_attack_band`:
the human walks to an authored Gun item, fires normally beyond Hammer reach,
then stops damaging the bot. The Hammer-only bot must actually approach, swing,
and produce a canonical credited kill. The long Hold case now authors a
170-unit horizontal route inside the existing floor and requires its measured
delivery duration to exceed 1800 simulation ticks. These strengthened assertions
are pending root's serialized Cargo run; targeted formatting and diff whitespace
checks pass. Vertical jet carriage remains unsupported as documented above.

First strengthened checkpoint: root's compiled physical suite passed **13/14**,
including the 170-unit Hold delivery and its actual `elapsed > 1800` assertion
(`/tmp/bri-v022-final-body-and-tactics.log`). The new retaliation case stopped
before combat: its extra 1x1 Gun-spawner plate was authored off-grid. The fixture
now uses the same half-stud-centered coordinates as the other ordinary scene
plates and reports actual players, inventories, static items and loaded bricks
on any further pickup failure. This setup repair changes no runtime behavior or
injury/pursuit/credited-death expectations. Retaliation rerun is pending.

Final strengthened closure: root's exact command
`cargo test --locked -p bri-chaos --test bot_physical_objectives -- --nocapture`
passes **14/14, 1.48 seconds**, recorded in
`/tmp/bri-v022-final-physical-14-retry.log`. The corrected ordinary build admits
the Gun supply; actual human pickup, an injury beyond Hammer range, Hammer-only
ordinary pursuit, native Fire, and the canonical credited kill all pass. The
170-unit horizontal Hold journey also completes with measured elapsed time
greater than 1800 simulation ticks. Existing passive-creator retention,
permissions, due-time guards, discovery-budget negatives, heavy hand/Hammer
delivery, and real region exit/reentry continue to pass in the same suite.
Independent source review closes the out-of-band arbitration finding. This
supersedes the pending focused assertions above; it does not constitute full
release, GPU, platform or interactive-playtest signoff.

Implementation and fixtures are frozen. No further capability expansion in
this lane: vertical jet transport is still unsupported; arbitrary renamed
image callbacks still need a truthful native/tool semantic provider. The
shared ordinary native controls own every tested impulse, hit, pickup and
outcome; no score/win/pose/force shortcut was added.

Full gate `9650a6b19178` later exposed a stale interruption fixture that expected
over 30 seconds of combat against a passive opponent. Giving that opponent its
authored low-damage Gun through ordinary equip/aim/press/release controls exposed
an actual integration defect: the visible-target retention still preferred the
passive Author over the actor that had genuinely injured the bot. The bounded
trace shows attacker 3 damaging bot 2 at tick 33, followed by the bot pursuing
and damaging Author 1 until reaching its existing 48-unit leash. This explains
the interrupted duration shortfall without changing the leash or acceptance.

The root-approved correction changes only the existing sight-selection helper:
current incoming or retained dated injury prefers that exact canonical enemy
through the existing visibility test. An unseen attacker during a retained
objective provides no fresh passive target; ordinary dated hurt/memory/search
remains responsible, with unchanged expiry and no hidden live-position update.
The same fixture now requires actual attacker selection and return damage,
unchanged passive-author health, more than 3600 combat ticks, disconnect, and
canonical score/round resumption. Source-only checkpoint formatting and diff
whitespace pass; root's focused compiled rerun is pending. No charge execution,
damage, geometry, score or win shortcut was introduced.

The shared injury-priority correction passes the full physical suite **14/14**
and tactics suite **11/11** in
`/tmp/bri-v022-injury-priority-physical-tactics.log`; independent source review
also passed. A second interruption-fixture issue was then measured: its human
controller relentlessly followed to 1.6 units, forcing the Gun bot to retreat
past its unchanged 48-unit leash. Late samples show distance 45.57 at tick 1800,
48.10 at tick 1920, and repeated Objective/Fight alternation at that boundary
despite fresh actual injuries. No leash or runtime guard was relaxed.

The opponent now maintains a legitimate ranged standoff with ordinary forward
movement beyond 6.5 units, backward movement below 5.5, and idle movement between;
native Gun movement already prefers at least five units. The run remains 34
seconds and still requires **more than 3600 actual combat ticks**, bilateral
native damage, visible selection of the real attacker, unchanged passive-author
health, disconnect, retained objective resumption, and the canonical score/round
observer. Full `bot_objectives` **11/11 PASS, 0.85 seconds**, recorded in
`/tmp/bri-v022-objectives-valid-standoff-retry.log`. This supersedes the pending
injury-priority checkpoint above. NPC source and fixtures are frozen again;
the separate full gate/release remains root-owned and incomplete at this record.
