# 2026-10-03 v0.2.3 NPC robustness and bounded Shark fidelity

Root owns integration, existing engine/test/port files, commits and packaging.
This lane prepared reviewable patches in `/tmp`; original source installations
and published v0.2.2 archives remain unchanged. Maxwell alone performs interactive
playtests. No visible game launch, gameplay/UI automation, new schema, wire change,
content-name solver or bot-only physics path was used.

## Windows repeated-entry failure and physical controls

Published v0.2.2 Windows CI run 37161683721 failed only
`repeated_object_entry_requires_real_exit_and_reentry_for_each_physical_method`:
declared physical hold, renamed echo-chamber composition at offset 24, initially
inside its wide region. Exact downloaded log:
`/tmp/bri-v022-windows-ci-failed.log`, failure lines 560–590. The macOS suite had
passed, but that final result hid a failed first attempt followed by recovery.

Root applied temporary, environment-gated instrumentation while this lane held
an exclusive compute lease. Command (two bounded diagnostics of the same test):

```
CARGO_BUILD_JOBS=2 BRI_NPC_TRACE=1 cargo test -p bri-chaos --test bot_physical_objectives repeated_object_entry_requires_real_exit_and_reentry_for_each_physical_method --locked -- --test-threads=1 --nocapture
```

Both uncorrected macOS runs passed 1/1. Logs:
`/tmp/bri-v023-npc-rearm-trace.log` and
`/tmp/bri-v023-npc-rearm-move-trace.log`. At tick 150 the exact body was held
at native acquisition distance 14.425817, while the provider had planned a
roughly 4.54-unit acquisition standoff. Ordinary controls correctly requested
negative z toward the holder's inverse hold solution; initial positive z
movement was residual airborne velocity, not a navigation inversion. The hold
stalled and was lost by tick 240. The unheld approach used the desired exit's
radial sight instead of a stable body/delivery side; passing exit z45.96 before
reaching body z56.23 flipped its point past the body to z60.71. The bot then
physically pushed the body away. Windows expired its approach reservation at
939 rather than recovering to a win. Lease extension or a larger timeout would
not correct those controls.

The initial proposed acquisition gate/current-side correction proved the early
range defect: root's strengthened actual-grip regression failed on the original
roughly 15-unit body-to-holder spacing (`/tmp/bri-v023-npc-before.log`). However,
the first correction failed 2 of 14 physical tests (12 passed), including long
hold delivery and repeated entry (`/tmp/bri-v023-npc-after.log`). It is not an
accepted correction.

Two focused after-correction diagnostics also failed with unchanged time budgets:
`/tmp/bri-v023-npc-rearm-after-trace.log` and
`/tmp/bri-v023-npc-long-after-trace.log`. The repeated composition physically
exited by 394 and admitted the first entry at 456. At 463 an explicit
objective-tool-to-no-view transition released its valid exact 4.036-unit native
grip during authoritative observation/model repair. The freed body continued
through the actor; actor velocity reached [9.55, 0, 29.1] at 491, and its position
returned to the authored spawn at 733. Death is inferred from the brain's dead
reset and respawn behavior; these diagnostics did not record canonical death
results or vitals, so the exact damage cause remains unproven. No native
vertical/range rejection was logged. The long delivery retained a valid native
grip but became blocked near the prior creator position, hopped without useful
travel and expired its idle claim at 717.

Revised proposal `/tmp/bri-v023-npc-revision.patch`, applied atop the first
proposal, reuses the existing physical approach arc to reach the body's
**delivery-rear** standoff without a through-body chord, gates the ordinary
trigger at that final standoff, and leaves the native hold motor/navigation
unchanged. It retains only a validated exact native hold view for one model
repair turn after an actual changed observation; the holder stops and aims at
the observed grip while the next grounded action is selected. No completed
rule is executed again. Invalid equipment/permissions/objects, terminal goals
and failures do not gain that retention. Regression geometry includes the
actual counterfactual through-body chord, translations and rotation; the real
repeated-entry test checks acquisition range and grip continuity between its
two actual inputs. Root's continuity-only baseline also failed at the real release boundary in the
counted-courtyard composition (tick 225),
`/tmp/bri-v023-npc-continuity-before.log`. With the revised production correction,
root ran the complete physical suite: **14 passed, 0 failed**, including the long
hold and both acquisition/continuity assertions,
`/tmp/bri-v023-npc-revision-after.log`. Original time budgets were retained.
Windows and interactive verification remain outstanding.

All temporary instrumentation was removed after each compute lease. No compute
lease remains held by this lane.

## Combat held/charged sequence proposals

Static audit found two independently reviewable sequence breaks:

- Ordinary hand-weapon selection can equip a newly available native weapon
  while a scripted hold tool has a canonical `held_by` sequence. The
  normal package then releases because its tool is no longer equipped.
- A native unchanged participant/equipment wind-up stores charge continuation,
  but another inventory selection is preferred before that continuation if the
  current intercept/range temporarily fails.

`/tmp/bri-v023-combat-regression.patch` contains regression-only tests.
`/tmp/bri-v023-combat-fixture.patch` corrects package installation timing through
an ordinary fixture helper that installs packages before any join/spawn.
`/tmp/bri-v023-combat-production.patch` adds minimal selection guards;
`/tmp/bri-v023-combat-canonical-grip.patch` narrows the initially integrated guard
to canonical grips. The initial full `/tmp/bri-v023-combat.patch` is superseded. The held test uses the
shipped real Gravity Gun package, waits for actual bot acquisition of the human,
then grants an unfamiliar native gun through ordinary inventory authority. It
checks held tool selection, normal carry/lift/release and actual victim momentum.
The charge test adds that alternative after a real wind-up, moves the human
outside a declared band and back through normal controls, and observes the
retained native charge's actual projectile. Neither test injects a grip, bot
control, objective or score. The first hold regression had an invalid setup (package installation after
players joined), so its failure was not evidence. After that fixture correction,
root's before run failed both tests at intended boundaries: the actual held human
remained held while the selected tool changed to the unfamiliar gun at tick 75;
the native wind-up restarted into Activate.
`/tmp/bri-v023-combat-before-corrected.log` records those failures.

The initial production run passed 12 of 13 tactical tests, including the held
sequence, but the new charge regression still failed:
`/tmp/bri-v023-combat-after.log`. Static tracing found a second wind-up break:
ordinary tracking controls depended on Fight/Chase/Fly despite a valid retained
participant/equipment charge intent. A stationary authored guard temporarily
selects Return/Wander outside its band; that behavior filter aborts its native
charge. `/tmp/bri-v023-combat-charge-control.patch` removes that behavior filter
while retaining current native choice/image/trigger and release-only validation.
Objective tool controls still explicitly preempt/cancel charges. The next full
tactics run again passed 12 of 13: retained wind-up/selected-slot assertions now
passed, but no actual projectile was observed within the original five seconds
(`/tmp/bri-v023-combat-final.log`). The test had assumed equal outbound/inbound
movement durations brought the human back into range without checking actual
return. `/tmp/bri-v023-combat-return-fixture.patch` keeps the same time budget,
requests ordinary reverse controls until observed band return, asserts that
return, and includes actual feet/hand state/thoughts if release still fails.
That fixture run failed its actual-return assertion, before its fire assertion:
the human remained near x−25 while the bot had strafed to x−34.43
(`/tmp/bri-v023-combat-return-after.log`). Disabling chase/wander/fly did not
disable ordinary Fight strafing. `/tmp/bri-v023-combat-steered-return.patch`
steers the human toward the observed live bot after the same outbound segment;
all assertions and the 600-tick budget are unchanged. Root then ran the full
tactical suite: **13 passed, 0 failed**, including actual band exit/return,
retained native wind-up, actual projectile release and Gravity Gun human
carry/lift/release (`/tmp/bri-v023-combat-steered-after.log`). No production fix
was added for the fixture motion error. Broader weapon thrashing is not claimed
solved; Windows and human mutation cards remain outstanding.

The initially suggested reaching-only guard could retain an uncatchable visible
target indefinitely. It was removed rather than adding speculative timer state:
only an actual canonical `held_by` grip prevents hand-weapon reselection. A failed
reach remains eligible for ordinary alternative selection. Tool/permission/body
loss resumes selection; intentional native carry release remains its owner.
Gravity Gun alone was not demonstrated broken: the shipped state graph remains
Grab until trigger-up, and the physical traces do show real human carrying.
Do not attribute every reported spam-click symptom to these hypotheses.

A separate Drive admission/execution mismatch was identified: discovery checks
body ride capability and allied passengers, while a retained directive omits
those checks. It is deferred behind the proven release blocker and sequence
regressions, with no patch or acceptance claim.

## Source-backed Shark correction proposal

Read-only Bot_Shark.zip provenance matches the existing port:
`02f9ec67645e6b3736be651fe4298a8796fe8d1f2edd7515028212d96b70ac04`.
Retained original files are byte-identical to their archive entries:

- `dist/v0.2.2/research/shark-bot-base.cs`:
  `253dc6f9a5f1c2ef3b1dae6a7261d1981d20f00995c5662b49e1844ed5033c65`
- `dist/v0.2.2/research/shark-support.cs`:
  `05d1796db404b1836d6107a5e855544610bd1f41f3ecf7c654714d0ff8c18d1f`
- `dist/v0.2.2/research/shark-server.cs`:
  `f5d8774a9ae2727666bacba9c88138c5371b6d1155ae3e04634061a6b581ffb0`

The original declares `AddDamageType("SharkHoleBite", ...)` and
`hMeleeCI = "SharkHoleBite"`. Its icon is already converted, but the generic
importer returns before collecting damage types when no weapons/sounds/brick
emitters exist. That loses standalone damage-type policy. Proposed
`/tmp/bri-v023-shark.patch` retains generic type-only packs and uses the extracted
source bite type for Shark melee, capture recognition and actual human
capture-kill attribution. The Player branch explicitly applies 1000 damage with
`$DamageType::SharkHoleBite` (retained `shark-bot-base.cs:378`); the AIPlayer
branch instead uses infection or `kill()`, whose complete parity is not claimed. Its original-free type-only importer regression uses two invented
renamed packages/icons. The real capture regression observes the canonical death
chat/icon after ordinary damage/capture ticks. Root approval covered this bounded
scope; patch apply-check passed, Cargo validation is pending.

Forced vehicle ejection occurs in the source's **vehicle collision** callback,
not as a seated-player bite. Current package snapshots expose neither the needed
vehicle-seat occupant identity nor that callback. An on-damage approximation
would invent source behavior, so it is deferred. The original escape loop and
white-shark aggression/strafe overrides lack a present behavior-control seam.
Zombie infection labels similarly remain unsupported. No claim of complete
Shark/Zombie fidelity is made.

## Avatar editor animation and lighting

Read-only v20 reference `Blockland-Max/BlocklandReImagined/.research/bl-decompiled/v20/client/scripts/allClientScripts.cs:12249`
starts the avatar-options body `run` thread at speed 0.85, alongside its separate
head-up/accessory thread and authored camera rotation/orbit. The current preview
had always sampled time zero and refreshed only on editor changes. Root applied
`/tmp/bri-v023-avatar-preview-animation.patch`: a local editor clock samples the
native run thread, retaining the existing authored appearance and head-up layers.
The actual `Preview::render` texture is composited through the UI renderer in
synthetic and generated-content offscreen tests. Root reports **2 passed, 0 failed**,
`/tmp/bri-v023-avatar-preview-after.log`; the native portrait had 14,361 changed
lower-body pixels and zero loop comparison error. Generated images are in
`artifacts/avatar-editor-preview/`; no visible game launch was used.

Root's image review found the portrait still dull. Imported Avatar_Preview fields
retain white directional light and ambient 0.5. Preview meshes use normal posed
vertices and inverse-transpose normalized normals, not instanced avatar geometry.
The source-facing camera sees the model's top/front/+x surfaces, but the local
preview supplied the authored source direction as shader light travel. The scene
shader lights against the negative travel vector, leaving these faces ambient-only.
`/tmp/bri-v023-avatar-light-regression.patch` adds a generated-content offscreen
counterfactual with the exact same avatar, pose, camera, normals and renderer,
changing only the white key to zero. `/tmp/bri-v023-avatar-light-production.patch`
flips the local preview travel vector, retaining authored light colors. Root's
actual-content baseline failed with only 0.0238 mean RGB directional-key contribution
across 39,568 opaque pixels (`/tmp/bri-v023-avatar-light-before.log`). After the
correction the same native comparison **passed 1/1**, with mean contribution
93.9267 (`/tmp/bri-v023-avatar-light-after.log`). Independent before/after image
review shows the original white chest, bright red/blue clothes and lit skin facets
restored. Original and corrected captures retain the same pose and authored outfit.
The original current portrait is retained at `/tmp/bri-v023-avatar-preview-original.png`;
corrected and ambient-only captures are in `artifacts/avatar-editor-preview-light/`.
The exact closed v20 GuiObjectView binary shading convention was not recovered;
this is evidence for correcting our local renderer convention, not a claim of
binary-exact Torque shading parity. Public newer Torque3D GuiObjectView source
alone does not prove the older custom preview's light sign.
Root then reran both synthetic and generated-content actual portrait integration
tests with animation and corrected lighting together: **2 passed, 0 failed**,
`/tmp/bri-v023-avatar-preview-light-final.log`. Native lower-body changed pixels
were 17,452, synthetic 16,780, with zero loop error for both.

## Supported hanging placement through map surfaces

The user reported placing a brick on a Bedroom surface, then extending downward
through its solid interior by hanging from that brick. Normal `check_placement`
accepts geometric brick support and tests native triangle contact separately;
contact alone can miss a candidate wholly behind the floor shell. Root applied
the regression-only `/tmp/bri-v023-map-occupancy-regression.patch`. Its real normal
plant admitted the first hanging plate before the correction, an intended failure
in `/tmp/bri-v023-map-occupancy-before.log`. The fixture uses the actual native
interior adapter, a source-like closed floor shell and copied connected plates,
with translation, rotation and unfamiliar world names. It asserts atomic rejection
and ordinary stacking above the floor.

The initial proposed signed-volume `/tmp/bri-v023-map-occupancy-production.patch`
was **not applied or accepted**. Independent review found that FIX_INTERNAL_EDGES
pseudo-normals do not prove a closed outward-oriented manifold. Treating an open
or disconnected MAP_TAG mesh as filled could reject real free space. The original
DIF importer retains authored convex hull points and plane-oriented collision
faces (`crates/convert/src/interior.rs:109`, `:289`), but the runtime adapter exposes
its triangle shell; an arbitrary map collider cannot imply solid topology.
Performance's open/disconnected panel control is retained for verification.

The narrower proposed `/tmp/bri-v023-map-connector-production.patch` reuses exact
rotated authored attachment-cell pairs from `grid::connected` and refuses brick
support through an intervening nonterrain map surface. Any clear connector still
supports the candidate; geometric ownership checks and native ground/terrain
support remain authoritative. A root whose cell centre sits at the allowed floor
dip can still support stacking from above, but cannot bridge the floor downward.
Loaded structural graph connectivity and global physics remain unchanged. This
stops the proven first normal hanging transition; it does not claim general
filled-volume classification of arbitrary triangle meshes. Independent review also
found an internal copied-connector gap: group placement had checked only the old
world, allowing a copied column to carry aggregate support through its own floor-
crossing connection. The proposal now follows the same clear attachment pairs from
actual supported copied members through a temporary grid index, preserving any
alternate clear route. The copy control uses a trusted existing anchor above a
native floor and a copied three-plate column. Root's complete before building
suite passed 11 and failed 2 at the intended external hanging and internal copied
floor crossings (`/tmp/bri-v023-map-connector-before.log`); floor-dip, open-gap and
partial-clear-copy controls passed before the correction. Root review caught an
inconsistent first group draft that treated support obstruction as a collision
error even when support was exempt. That branch was removed before production
application: normal required-support plant/copy paths enforce the obstruction,
while fits, administrator free placement and restore retain their native collision
checks and support exemptions. The patch and
`/tmp/bri-v023-map-connector-regression.patch` passed apply checks. Root applied
the corrected production and ran the complete building suite: **13 passed,
0 failed**, `/tmp/bri-v023-map-connector-after.log`. Both intended crossing
failures now reject atomically; native legacy planting/targeting, floor-dip,
open-gap and partial-clear-copy controls pass. The new grid unit remains pending
root's focused run. The additional normal plant
regression covers the floor-dip endpoint and a copied partial-floor composition
whose second stud connector remains clear. The grid regression checks all four
rotations, exact matching cell pairs and preservation of a second clear connector.

Follow-up audit found that the actual atomic blueprint job uses sliced
`check_plant` followed by `plant_try(..., true)` and bypasses direct `plant_group`.
The direct 13-test result therefore does not by itself prove the user copy flow.
Root approved sharing the canonical group preflight with this real job. The initial
`/tmp/bri-v023-blueprint-support-production.patch` proposal extracts a crate-private
incremental GroupSupport state, seeded by the existing native per-brick checks.
Root review identified unnecessary retained gameplay Brick data at the one-million-
brick limit. The final proposal stores compact definition ID/rotation and bounds,
roots/cuts and an index; it never retains copied events, source records or other
brick metadata. The crate-private compact grid entry delegates to the identical
rotated attachment algorithm used by existing Brick wrappers. Both direct API and
blueprint Check use that exact connector predicate/reachability;
copy jobs advance under their existing SEARCH/PLANT/SCAN budget before any free
placement. Normal alternate clear paths remain valid, and explicit float/restore
exemptions remain unchanged. No new map-volume policy is added. The test-only
`/tmp/bri-v023-blueprint-support-regression.patch` goes through ordinary
PlaceBlueprint commands with installed real fixture packages, normal source
brick planting and a requested 30-unit sliced budget (the existing minimum share
clamps it to 32 units). It checks every intermediate tick for
no invalid publication, final atomic rejection across a full native floor and
successful copied hanging through the second clear stud beside a partial floor.
Both patches pass apply checks. The first real-flow baseline had an invalid fixture:
the source plate was refused as Stuck because the host spawned too near it
(`/tmp/bri-v023-blueprint-support-before.log`); it did not reach the map-support
bug. Root moved spawn/join away along z and extended the separate source platform
so both full and partial-floor cases stay ordinarily grounded. No collision or
permission production rule changed for that setup failure. The second baseline did reproduce actual invalid first publication at y−0.1
(`/tmp/bri-v023-blueprint-support-before-2.log`). Its native pivot snapped z0.25
to0.5, placing atz0.75; this still proves a floor crossing, but missed the intended
above-floor anchor. Root corrected the command to the exact native stud-corner
pivot z0; baseline3 again reproduced invalid publication
(`/tmp/bri-v023-blueprint-support-before-3.log`). Additional regression-only
`/tmp/bri-v023-blueprint-support-proof.patch` derives the actual target with the
canonical Placement routine and asserts its real world-anchor stud connection.
It proves default required support by an ordinary unsupported-air copy attempt
and then proves the existing explicit package floatadmin exemption. No acceptance
assertion or copy work budget was widened. Final real-flow after checks later passed as recorded below. Retained compact preflight state is O(copy length),
capped by the existing one-million-brick copy limit;
neighbor work yields under the original budget. No million-copy performance or
memory measurement is claimed. No Cargo lease is held by this lane.

## Release lane handoff and remaining checks

- Physical objective correction: full 14-test after pass; final integrated rerun passed as recorded below. The authoritative objective/provider/ordinary-control/
  observation/repair pipeline remains intact. No timeout widening or native
  hold/nav bypass was introduced.
- Combat commitment: full 13-test after pass; final integrated rerun passed as recorded below. Canonical grips and unchanged native charge sequences retain ownership;
  misses/reaching alone never lock inventory.
- Avatar options: actual-content lighting comparison 1/1 pass and combined
  synthetic/content animation integration 2/2 pass. Authored appearance/light
  colors are retained; exact older binary shading parity remains unknown.
- Placement: direct authoritative building suite 13/13 pass. Actual atomic
  blueprint entry coverage and bounded shared preflight passed the complete
  20-test Advanced Duplicator and 14-test building suites as recorded below. Explicit support exemptions are retained; no generic
  closed-volume map classifier or global collision change is claimed.
- Shark: source-backed bite damage/icon and type-only importer correction remain
  a bounded pending patch, requiring importer/real capture verification and
  affected generated pack refresh. Ejection/escape/white-aggression and Zombie
  labels remain open; their required semantics are not invented.
- This lane changed no schema, wire protocol, manifest/lockfile or original
  installation. Existing source and test changes were applied only by root.
  No compute lease remains held by this lane. Windows CI, final release gates
  and Maxwell's interactive acceptance are root-owned remaining evidence.

New mechanism expansion stops here. Deferred Drive retained eligibility,
planned throws/jet carrying/hooks/stacking and emotional architecture were not
implemented. Diagnostic motor-restore equivalence does not prove the producer of
the reported user death/disconnect; source attribution remains unknown.

## Human mutation cards and limits

After root provides an accepted packaged build, Maxwell can test:

1. Author a two-entry object counter with a wide region and initially occupied
   renamed loose body. Give its bot a declared hold tool. Move the body/region,
   rotate the approach and block a narrow route. Expect a real exit then entry,
   retained grip during repair and eventual two physical inputs; observe explicit
   failure/repair when a route is impossible. No score change without inputs.
2. While a Gravity Gun bot actually holds a player/body, grant another usable
   weapon. Expect its normal lift/swing/release to finish before switching.
   Then remove/replace the held tool or change permission/team/round: expect a
   truthful release or invalidation rather than a stale continuation/win.
3. Let a native charged weapon begin a real wind-up, move across its attack-band
   boundary and back, then disconnect or replace the target/tool. Expect retention
   for temporary motion and cancellation for changed participant/equipment.
4. Let a source-backed Shark capture kill finish and inspect the declared bite
   message/icon. Separately record vehicle collisions and struggle attempts;
   ejection/escape/white aggression/infection-label gaps remain open.
5. Open avatar options with the original default outfit, then change skin, chest,
   pack and face. Expect a continuously looping native walk/run preview with
   lit facets and authored colors; changing accessories must retain its own
   head-up layer and keep the loop continuous.
6. Plant a plate on a map floor, then attempt to hang a plate below it or copy
   a connected column through it. Expect atomic rejection at the intervening
   surface. Repeat beside a real open edge/gap and on a floor-dipped stock
   surface: valid clear connections and above-floor stacking must remain valid.
   Headless normal-plant integration passed; arbitrary filled-volume occupancy
   and administrator floating placement are not claimed verified.

Supported scope remains the current contact/hammer, declared native hold and
native control-seat mechanisms through ordinary physics. No jet carrying,
planned throws, hook transports, stacking, emotional architecture or generalized
new seam is added. Tests must pass at original budgets, and Windows revalidation
plus Maxwell's interactive handoff remain outstanding.


## Fresh review: bounded support and live copy authority

The first compact preflight proposal is superseded, not accepted as complete.
Fresh independent review found three concrete blockers: a matching authored face
can perform up to a million map rays inside one charged PLANT operation; the
neighbor query eagerly collected all intersecting members before yielding; and
world-root/trust checks remained cached while the actual blueprint job later used
free placement. A normal anchor removal between Check and Place could invalidate
the supported component. Existing sliced editor jobs also retained a start-time
Actor, so ordinary trust revocation could permit remaining cut/paint operations.

Root's normal Cut plus DemoteTrust regression reached its intended failure in
`/tmp/bri-v023-copy-authority-before.log`. The separately reviewed
`/tmp/bri-v023-copy-authority-production.patch` refreshes the canonical current
actor and each job's original Build/Paint gate before every slice, preserving
PlantAs destination/admin policy and administrator-only float policy. Undo uses
its original entry policy; it adds no equipped-tool or death requirement. Atomic
plant denial invokes only existing provenance-owned cleanup. Root applied this
production correction; its after verification remains pending at this entry.

`/tmp/bri-v023-blueprint-support-bounded.patch` replaces the unsafe compact draft.
It retains definition ID/rotation/bounds, reuses the exact authored cell predicate,
and advances internal group connections one raw cell and spatial query one bucket
entry at a time. Each map-ray cell costs SEARCH; every candidate or empty bucket
costs SCAN. Synchronous support queries explicitly refuse after 256 counted
candidates/cells/probes with the existing Limit result, and oversized temporary
index footprints are bounded before insertion. Direct synchronous group support
has a 4096-unit drain limit; ordinary sliced blueprint traversal retains its
existing work share and yields. This bounds support work; it does not claim every
preexisting native collision/overlap routine has been made incremental.

A private support epoch changes on brick attachment/removal, support identity
(owner/definition/rotation/position), collision changes, map collider enablement
and terrain replacement. Player motion, ordinary ticks, paint and names do not
change it. Check/Place invalidates when this epoch changes; a partly published
atomic copy uses existing provenance Undo. Own publications advance the expected
stamp before package callbacks, so callbacks cannot silently bless a changed
world. Changed live trust/admin also invalidates cached supporting-neighbor
permission checks. Explicit float/free/restore policy remains separate from
required-support reachability. This draft passed formatting and apply checks;
three focused regressions cover one-entry query stepping/deduplication, resumption
inside a million-cell authored face, and ordinary Limit refusal for a renamed
64×64 blocked hanging attachment. Root execution and independent review remain
required; no success is claimed yet.

Root's final integrated existing NPC checks passed 14 physical-objective tests,
13 tactics tests and 17 interaction tests in
`/tmp/bri-v023-{physical,tactics,interactions}-integrated.log`. These results do
not verify the new support/authority proposals or outstanding release platforms.

Read-only review of remote commit fef2d9c3 found its generic borrowed Add-On brick
conversion relevant to Shark's Bot_Hole mesh dependency. Its output filename uses
only a geometry-byte hash while mesh IDs use source paths, so two borrowed paths
with identical bytes can overwrite one JSON file with different IDs. Root was
asked to correct this alias collision and add a renamed identical-byte control
before integration; the full remote branch was not cherry-picked.


Root integrated the corrected bounded preflight and ran the complete actual
Advanced Duplicator and native building suites: **20 + 14 passed, 0 failed**,
`/tmp/bri-v023-support-integrated.log`. This includes actual required-support
PlaceBlueprint across the native map, clear second-stud/open-gap controls,
explicit floatadmin behavior, ordinary live trust revocation, and a real second
editor Cut removing the sole anchor after preflight. The stale-anchor baseline
first had an invalid player-overlap fixture, then reached the intended surviving
unsupported-copy failure after moving the destination
(`/tmp/bri-v023-support-epoch-before-2.log`). No budget or time assertion was
widened. Fresh review confirmed cursor/freshness logic and identified one final
raw native AABB contact loop; each raw contact candidate now consumes the shared
scalar allowance before filtering or calling contact. Focused grid cursor tests
remain root-owned pending execution unless separately reported.

Root applied the bounded source-backed Shark patch and the remote borrowed Add-On
geometry change, correcting its filename to hash mesh identity plus bytes. The
original-free regression-only `/tmp/bri-v023-borrowed-brick-alias.patch` supplies
two renamed lenders with identical BRICK bytes, imports both into one borrower,
and loads its actual native Definitions alone. Each loaded mesh ID must match
its own authored catalog binding and dimensions. Importer, native capture and
pack refresh checks are still pending at this writing; application is not itself
verification. No original source installation was edited.


The full original Shark input archive was located read-only at
`/Users/maxhastings/Library/Application Support/Steam/steamapps/common/Blockland/Add-Ons/Bot_Shark.zip`.
A fresh SHA-256 on 2026-10-03 returned the pinned
`02f9ec67645e6b3736be651fe4298a8796fe8d1f2edd7515028212d96b70ac04`
(36,640 bytes), and all three archive script hashes matched the retained research
files listed above. The Steam installation also retains Bot_Hole and Bot_Zombie
archives. Root was given this explicit existing read-only search/reference path
for reproducible regeneration; no partial reconstructed source or manual
in-place generated-pack refresh is necessary. This does not change the vanilla
v20 fidelity reference or alpha package scope.


Root's focused importer checks passed: borrowed alias **1**, Bot_Hole creature
ports **2**, and standalone type-only policy **1**,
`/tmp/bri-v023-imports-corrected.log`. The type-only fixture initially treated
resource native_file as output-root relative; root corrected its assertion to
the existing assets-relative resource contract. No importer production behavior
changed to pass that fixture error.

For reproducible partial bundle refresh, the executable temporary handoff
`/tmp/bri-v023-refresh-creatures.py` uses existing `addon_bundle.import_copy`,
`problems` and `copy_with_companions`. Python syntax checking passed; this lane
did not run regeneration or Cargo. It refreshes only the pinned original Shark
and Zombie archives against explicit read-only Steam references and generated
installed content, with no recovered core scripts. It checks source hashes,
port application, companion policy, credit/provenance and exact authored brick
IDs; new borrowed mesh bindings and native collision recipes must be local and
consistent. It preserves all 88 original entries (86 unchanged), existing credit
bytes and all other bundle files, and records partial refresh rather than a full
88-source rebuild. It retains a recoverable old-bundle backup and rewrites the
private bundle zip; upload remains root-owned.

Read-only scan of all existing bundled stock catalogs found borrowed
`v20/add-ons/...` geometry only in Shark and Zombie, both referencing Bot_Hole
spawn BLBs. No additional package refresh is justified by that bounded scan.
Python binding checks are not a native physics load. Separate test-only
`/tmp/bri-v023-refreshed-geometry-probe.patch` adds an ignored generated-content
probe driven by BRI_REFRESH_BUNDLE, calling actual Definitions::load on both
refreshed catalogs independently without the lender catalog. It checks authored
mesh IDs and finite nonzero native collision bounds. Root may run it after
regeneration and before private publication, or pass its already-built executable
to the refresh script. Native source-backed Shark capture remains a separate
root-owned verification.


Final root verification receipts supersede the pending status above: the focused
exact attachment/query cursor checks passed **2/2**,
`/tmp/bri-v023-grid-cursors.log`. Root executed the partial original-input
creature refresh and installed its generated bundle: **88 entries, 86 unchanged**,
`/tmp/bri-v023-creature-refresh.log`; zip SHA-256
`8e00f2a8a8155c061156b13ed1c83062cef411ff88d42cf19430753517706bb8`.
The actual native Definitions probe passed **1/1**, loading the regenerated
Shark and Zombie brick catalogs independently without lender geometry,
`/tmp/bri-v023-generated-geometry.log`. This is a two-package partial refresh,
not a claim that all 88 original packages were rebuilt.

At this receipt, the final native Shark policy suite is still unresolved:
`/tmp/bri-v023-shark-final-3.log` reports **13 passed, 2 failed**. Both actual
ordinary encounters reach a genuine human capture, then fail the requirement
that the five-second observed capture finishes through ordinary damage. Root
owns the remaining diagnosis; importer/geometry success does not close capture
fidelity or authorize a weakened assertion.

The lane independently reviewed the new canonical passage frame source without
running Cargo or changing shared code. Two concrete correction gaps were sent
for the author to repair: switching out of a driven vehicle unconditionally
promoted its pending speculative carry, and a future driver input acknowledgment
could enter the sticky acknowledgment state and reject all later valid poses.
No additional duplicate crossing execution, removed-link persistence, map-reset
or vehicle-removal blocker was found. Canonical rider-frame mapping must use
compatible admitted player/vehicle bases: their datagrams have independent
stream ages and vehicles can have history from before the current driver boarded.
This source review is not a passing runtime receipt for the pending corrections.


The passage author supplied and root applied the rider-frame correction. Own
Player Pose now pairs its frame with the mounted vehicle's frame from the same
authoritative tick. Vehicle trips advance the actual occupants' Player frames
before dismount intents; prediction rebases independent vehicle history onto
this rider origin. Target departure waits for the admitted body pose rather than
promoting an unresolved vehicle carry. Follow-up independent review caught and
closed older cached preboarding vehicle poses, incoming walking-to-driver pending
carry, an immediate walking carry's presentation stamp, and stale-tick admission
before malformed future acknowledgments. Bootstrap/admission requires a vehicle
pose at least as new as the paired body basis, avoiding cross-stream clock guesses.
The latest source review has no remaining concrete blocker.

`/tmp/bri-v023-mounted-rider-first.log` reports **9/9 passed**, including real
ordinary and native jet ejection through both accepted and rejected trips, with
seat-first and body-pose-first delivery. Root owns the final rerun after all
follow-up edits; this lane did not run Cargo. The source-backed Shark finishing
policy also passed bounded independent source review: it retains the captured
holder/victim/game only through its originating tick, then passes typed finish
damage through ordinary damage/death policy without recapturing the released
human. Final regenerated native Shark execution is still root-owned pending.


Final Shark receipt closes the earlier capture failure: root's regenerated native
policy suite passed **17/17**, `/tmp/bri-v023-shark-final-complete.log`, including
both actual ordinary imported-mouth and authored-large-body capture encounters,
typed finish without recapture, actual credited damage/death and source bitmap
message through the native private minigame notice channel. The intermediate
assertion read public chat instead of that existing private channel; root fixed
the observation, preserving the finish/death requirement. Source-supported
ejection/escape/white-aggression and Zombie label limitations remain as recorded.
Root refreshed the two original creature packages again with this finishing fix,
retaining **88 entries / 86 unchanged**,
`/tmp/bri-v023-creature-final-refresh.log`. Its final zip SHA-256 supersedes the
earlier receipt: `3090adc946c6bb9b812b2e0a435a65f4603091dd0aa37a4090d93ea6c55f9669`.

Final independent frame review verified idle basis retention for unchanged paired
markers and continuing readiness for the same active target. Actual occupancy now
comes from live vehicle seats at each crossing, so the delayed Session mount
index cannot invent a trip for a rider already jet-ejected in that same tick.
One remaining normal passenger-to-driver same-vehicle admission case was sent
to root: unchanged passage markers preserve the earlier boarding tick, so a
cached former-driver pose with a future input acknowledgment must not bootstrap
a new driver prediction. Root owns its bounded admission correction and final
Motion suite receipt.


The last same-vehicle promotion blocker is source-closed by root's applied
bootstrap-only acknowledgment admission: an unstarted target waits while its
cached driver acknowledgment exceeds the local input sequence. An already
active target still reaches the original stale-tick-first malformed future-ACK
containment. Independent review found no remaining blocker in this scoped
correction. The native and synthetic regression variants use two real Session
players, ordinary jump boarding, former-driver input numbering above 10,000,
actual jet ejection and PrevSeat promotion into seat zero. They assert unchanged
paired markers, rejection of the real cached former-driver pose, and successful
prediction startup from the genuine current-driver pose. Their execution and
the final full Motion rerun remain root-owned.


The full gate on 56fa1f82811e exposed three additional lane failures. The shifted
rotation fixture assumed absolute f32 rotation/subtraction commutes within
1e-5; its corrected local-frame rotation assertion retains that tolerance and
the translated-world hull/trigger checks. The guarded tool-loss negative removed
the instrument after one meter, relying on the former remote-grab launch to
reach an elevated goal. Its revised authored trigger waits for an observed
inbound region approach, then removes the tool/revokes the due guard without
changing velocity or physics. The first nine-tick forecast proved actual loss
404, native release409, free entry410 in one transformed world but the second
entered before release. An incremental eighteen-tick forecast was handed off;
the stronger actual free-entry and guard-skip assertions remain unchanged.
Root owns its remaining verification.

Workshop's normal synthetic race recipe selected a 16x16 baseplate, whose
256 unsupported floor samples exhausted a single 256 allowance already spent
on bucket/contact work. The first proposed 512 combined allowance was rejected
and superseded. Root applied separate finite stages: existing support/connector
256, raw contacts256, and floor/terrain samples based on exact declared footprint
with an explicit 4096-cell cap. Renamed ordinary16x16 and64x64 controls require
valid floating publication and ground support after more than256 misses on a
partial floor; the existing blocked large attachment still must refuse Limit.
This restores bounded ordinary footprint capability without relaxing a test or
allowing million-ray attachment scans. Gate retries/full validation remain
root-owned.

Independent review of the native face-quilt draw correction found its bounded
strip proof coherent but flagged coordinate-wise in-plane skin clamping: moving
a skew outward apex to the logical corner can fill a real tiny authored gap.
The author/root received the exact four-quad counterexample and the narrow
normal-axis-only normalization correction. Runtime/offscreen verification and
this review's final closure remain pending.


Native face-quilt review is now closed. Final source normalizes only the
face-normal outward skin and retains every authored in-plane vertex. The exact
skew/sliver four-quad negative is included, so normalization cannot invent its
missing projected wedge. Critical vertex/edge intersections preserve the strip
proof, the midpoint must be strictly representable inside each interval, and
64-quad/4096-breakpoint/16384-cell limits retain bounded conservative fallback.
Root's `/tmp/bri-v023-gate-corrections-final.log` records the unchanged largest
stock draw budget **1/1**, actual face coverage **10/10**, and both screen
topologies **2/2** passing. No remaining concrete blocker was found in the
scoped independent native-cover source review. Final full gate/publication and
remaining NPC gate-retry receipts stay root-owned.
