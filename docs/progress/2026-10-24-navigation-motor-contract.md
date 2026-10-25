# 2026-10-24 Navigation and motor contract pivot

The user requested a strategy reset after repeated isolated navigation fixes.
The existing goal remains incomplete: a responsive, flowing assembled invasion
battle and a new playtest are required. Feature expansion is frozen for this work.

## Bounded evidence

External evidence is in
`../BlocklandReImagined-private-evidence/interdiction-ui-lighting-20261024/server-lag-report/`.

- `navigation-contract-72` samples 276 positions across the native map, including
  the retained route and failures. The normal motor settles from a clear initial
  pose for 120 fixed ticks, with automatic unstick disabled. Navigation and the
  motor see identical static/closed-mover geometry. Of 274 grounded observations,
  115 have over 0.1 height disagreement while horizontal movement stays under
  0.4. This identifies candidates, not 115 independently accepted gameplay bugs.
- Root cause: navigation classified centre-ray hits using a narrowed, raised
  box, whereas the motor uses the actual box/cylinder footprint against polygons.
  It could demand a foot height the body could not occupy. Source positions,
  grid nodes and actual body poses are different concepts.
- `navigation-contract-76` reduces that candidate count to 18 by resolving support
  across the footprint. `79/80` use the actual motor cylinder/box sweep plus full
  body admission. In capture 80, all 220 grounded observations with under 0.01
  horizontal movement agree within the existing height quantum/tolerance. Other
  observations moved under gravity or had transient ungrounded state; they remain
  explicit and do not establish standing acceptance.
- The new synthetic stair-edge regression compares navigation against the ordinary
  motor and an independently authored one-unit step. Existing sub-cell stair
  destination comparisons remain intact. Full body placement must preserve the
  requested floor using sub-cell horizontal clearance where it fits, rather than
  snapping the goal onto the next stair.
- Individual valid poses do not establish a valid connection. A straight shape
  cast rejects real step-up behavior, while endpoint-only admission cuts jambs.
  Walking-edge validation now calls the motor's stepping/collision routine and
  rejects lateral wall sliding. The existing steering-room preference is separate
  from collision admission. `motor-support-nav-84`: 43 pass, one pre-existing
  ignored comparison. No old comparisons were weakened or removed.

## Global routing comparison

Recast/Detour was built outside the repository, pinned to upstream commit
`9f4ce64458dfae86e1239c525ddc219c4e9e06f1`. Native motor collision export contains
1,417,642 triangles; bot-openable door geometry is omitted for this route-only
comparison. The export also includes very large boundary geometry. The bounded
probe rasterizes the extent of all original navigation landmarks plus explicit
horizontal/vertical margins, retaining all input triangles.

`recast-navigation-78` builds 25,849 polygons in 7.04 s and finds complete routes
from all four forest starts plus the basement to the deep target. Query times
are 0.78–1.65 ms, before motor, door, actor or networking execution. Prototype
failures remain retained: the unrestricted grid exceeded the probe budget; the
fine single mesh exceeded Detour data limits; a query pool of 65,536 exceeded
`DT_NULL_IDX` and was corrected to 32,768. These are configuration/tool results,
not corrected production gameplay.

Recast separates [geometry building, route queries and crowd steering](https://recastnav.com/).
Its [clearance configuration](https://recastnav.com/structrcConfig.html) and
OpenMW's [collision/navigation shape consistency](https://openmw.readthedocs.io/en/latest/reference/modding/settings/game.html#actor-collision-shape-type)
support keeping the motor authoritative and preparing global topology separately.
They do not prove a runtime transplant would solve this game's local behavior.

## Current implementation and next acceptance

`torque::support_height` reuses the motor's own box/cylinder sweeps. Native walk
sampling retains a supported full-body pose, and cached walking edges use the
motor step/collision implementation. Physical execution still uses the ordinary
player motor; the cylinder-contact correction below changes that motor.
Chassis support retains its prior mechanism. Baked profiles now identify shape
and wall-step policy as well as dimensions and walking limits.

The previous experimental graph is unbound from undelivered content 3; its exact
bytes are archived as `bundle-before-motor-80.json` and
`walking-navigation-before-motor-80.json`. The corrected graph bake
`motor-guidance-bake-85` stopped at its 180-second budget after more than 4,500
of 13,015 candidate pairs. All four connectivity probes remained disconnected
in that partial artifact. The installer rejected it and did not bind it into
content. This is an incomplete bake, not proof that the complete graph would
be disconnected. Do not rerun with an increased budget as the default next step.

Maxwell requested a strategy/goal pivot. Preserve the current corrections and
negative controls, freeze feature expansion, and stop extending the bespoke
global grid/landmark navigator while evaluating the already-built Recast/Detour
prototype as a bounded runtime candidate. Its static connectivity and build/query
timings justify that next experiment; they do not establish motor compatibility
or a production performance win. The existing motor remains authoritative for
physical movement; Add-Ons retain mission, allegiance and spawn policy. Keep
local searches bounded rather than repeatedly searching the whole facility.

The immediate milestone is an autonomous forest-to-deep-facility pursuit using
ordinary spawning and controls, with no external route driver, teleportation,
suicide recovery or intermediate-target success substituted for reaching the
actual destination. First isolate one actor; then exercise a group through doors
and stairs; then restore the assembled three-way battle. At each stage inspect
the first physical or decision failure, not only arrival or kill counts. Measure
full server tick and actual network responsiveness at the assembled population.
Only a flowing battle with an independently pinned package qualifies for another
playtest. The larger mission goal remains active and incomplete. No new playtest,
gate, merge or performance improvement is claimed by these diagnostics.

## Runtime candidate (90, not accepted)

The bespoke global graph implementation and `nav_bake` example are archived in
`pre-detour-89` and removed from this candidate. Optional prepared topology now
has one runtime implementation: Detour through pinned `recastnavigation-rs`
0.1.0 (default serialization disabled). A first binding candidate,
`recastnavigation-sys` 1.0.3, was rejected: its generated bindings on this host
omit the query methods. Its CMake compatibility experiment was also removed;
neither it nor its build-environment adjustment remains a dependency.

The replacement receives validated, portable polygon arrays, never opaque
native memory images. Recast topology comes from the retained native collision
export and upstream baker; openable doors remain live interactions. Coarse
polygon heights guide routing; the actual motor resolves support. Full
end-reference and output-capacity checks reject partial corridors. This is an
adapted BRI navigation mechanism, not original AA AI fidelity.

`recast-topology-88` repeats all five static connections and exports the mesh.
`cargo check -p bri-sim --examples --locked` passes (`detour-runtime-90.log`).
The unchanged ordinary-spawn traversal (`autonomous-detour-route-90`) builds in
7m04s after the dependency change, then crashes with Windows access violation
0xc0000005 before producing movement evidence. Exact candidate inputs are in
`detour-runtime-90-inputs/PINS.json`. It is explicitly a failed candidate.

`nav_query` isolates loading/querying from gameplay and physics. LLDB capture
94 identifies a null boundary edge in Detour's closest-point query. The cause
was our adapter supplying triangle fans without boundary flags, not missing
library functionality. The same inputs safely return all five routes after
correction (`detour-query-95`). A retained synthetic L-corridor regression also
challenges boundary projection, disconnected paths and invalid topology. Its
wrong-floor negative control initially fails (96): the BV tree can return a
nearest point beyond the requested height extent. Explicit endpoint-distance
validation corrects that assumption; both regressions pass in 98.

The unchanged autonomous traversal in `autonomous-detour-route-97` reaches the
house at 21 s and basement at 56 s, descends to Y=-67, then remains at open
Mover149 for the remainder of the deep leg. Exact source and content pins are
in `detour-runtime-97-inputs/PINS.json`. Arrival and battle acceptance still fail.

## Mesh-contact correction (103–110)

At the stalled position `[-40.45705,-67.35512,66.612885]`, a native contact
query finds the open Mover149 panel overlapping the cylinder by 0.15997 units.
Independent raw-pose motor attempts in three directions remain exactly frozen
(100/101), despite nonzero input and movement speeds. Fresh local planning
produces retreat waypoints, so a stale waypoint alone does not explain this jam.

A small synthetic slab reproduces the distinction: backing away succeeds with
a cuboid collider (102) but freezes with identical two-sided triangle geometry
(103). The cylinder sweep stops at initial penetration by default; a reverse
face therefore creates repeated zero-time collisions even during separation.
Using Parry's separating-contact behavior (`stop_at_penetration=false`) fixes
the retained comparison while keeping movement into the panel blocked (104).
The actual native-map retreat then moves out of contact (105), while movement
into the panel remains blocked. No teleport, non-solid door or suicide is used.
All motor library/integration checks pass (107); quick checks pass (109).

The first assembled rerun (106) incorrectly omitted the existing fixture and
failed deployment; it establishes nothing about navigation. Rerun 108 uses the
existing `run_solo_rounds.py` fixture and unchanged arrival criteria. It passes: house 20.5 s, basement 56 s and deep floor 238.5 s total, at
[-21.942696,-69.27,103.170685]. The zombie recovers from another contact
with Mover149 and continues. Motor clippy passes (110). Group traffic (111),
assembled battle timing and real network latency remain required before a
replacement playtest is accepted.

## Direct forest group (111–119, unaccepted)

The first group fixture (111) removed defenders only once. Mission streaming
then admitted more soldiers, so its isolation assertion correctly fails before
claiming traversal. The existing private fixture now removes non-zombies each
second without moving or steering any zombie. Production rules are unchanged.

Four naturally spawned zombies pursuing two stationary humans on the deep floor
fail the unchanged 360-second comparison (112): zero arrivals. The retained 3D
trace shows loops far from one another as well as a stair stall. Crowding cannot
explain all these failures. This is a stronger negative control than the staged
single-actor journey; 108 must not be generalized to complete forest pursuit.

Query isolation 115/116 shows a rejected initial attachment for Zw0: native feet
Y=-35.09 attach to a polygon at Y=-32.56 at identical XZ. The vertical guard
correctly rejects that wrong-floor attachment. Thus the older upstream static
claim of four complete routes included an unusable start projection. Do not
weaken the guard to restore that claim. The other three initial attachments are
valid. At all retained positions from 30 s onward, new queries return complete
routes for all four actors (119); static query connectivity alone still does not
establish runtime execution or retention of the corridor.

Candidate 117 requests Detour's funnel turns instead of every polygon crossing,
and logs the actual local guidance point alongside the gameplay goal when a
local search is partial. It changes no arrival thresholds. Its result is
pending; do not count this experiment as a corrected production behavior.

Quick checks pass for content and simulation (113: 429 simulation checks pass,
four pre-existing ignored); dependency notices are included by the shared
packager and document/layout checks pass (114). Motor focused checks (118)
find two broader regressions (`npc_profiles::each_body_plans_with_its_own_reach`
and `playground::a_bot_drives_the_look_steered_buggy_at_a_runner`). They are not
waived or added to known failures. Reproducible offline baking/installation must replace the
obsolete graph installer before prepared navigation is integrated or packaged.

## Simpler route ownership (121 onward)

Funnel turns alone do not correct direct group pursuit (117: zero arrivals).
Native support audit 120 finds grid-admission failures at prepared house-entry
points 12/14, and a 0.67-unit nav/motor height disagreement at point 13. That
shows the candidate's duplicate representations disagree at the entrance stairs;
they should not be layered into another set of exception handlers.

The exact pre-change source is archived in `pre-direct-route-121`. The current
candidate feeds the prepared corridor directly into the existing `Waypoint`
control executor. One `RouteSource` selects the plan provider; the grid does not
reconstruct prepared legs. Intermediate portal crossings remain intermediate
waypoints, while the final floor keeps its strict arrival rule. Goal changes and
observed execution failures invalidate the provider request. Ordinary movement,
body collision, door requests, combat and crowd steering remain authoritative.

The unaccepted full-body grid-placement experiment is restored to HEAD behavior;
only body shape/step-policy identity is added for prepared-profile admission.
Its earlier source and positive/negative observations remain archived. The
cylinder contact fix stays. This preserves the established planner on other maps
and for uncovered capabilities while removing roughly two hundred lines of
candidate reconciliation logic. It does not establish that direct execution is
correct by itself. `cargo check -p bri-sim --lib` passes (123). Direct group
124 improves from zero to three arrivals: Zw1 195 s, Zw0 203 s, Zw3 229.5 s.
The fourth, Zw2, stops at Mover90; the comparison remains failed. Both broader
regressions from 118 pass after restoring mature grid behavior (125). The failed
121/122 checks were local file-encoding/newline errors corrected in 123.

## Passage action (130 onward)

The retained Mover90 trace shows an interrupted opening at alpha 0.94444,
blocked with no resting key. A bot passage request was reusing human UseControl
toggling, which may close an interrupted opening. Separately, generic go_to
could not resume its existing target after being stopped. The pure same-target
resume regression fails before (132) and passes after (133); all ten mover
checks pass. Unrelated redirection rules stay unchanged.

Targeted passage is an opt-in extension to existing bots_open: one string mover
ID retains the current door while the bot backs away from actual body/leaf
contact, waits, then resumes its route. Commands without bots_open_target retain
their existing behavior. Ordinary movement clears the swing; aim and firing
remain active. The Add-On requests continued opening instead of human toggling.

The synthetic obstruction comparison reproduces a stationary jam when the
caller keeps pressing into the swing, and passes with the passage action (134).
Earlier short retreats lost the door identity and failed (130/131/133); those
results are retained. The exact content 3 script/scene preceding targeted
composition is archived in before-targeted-passage-135. Native four-forest group
135 is the next acceptance check; the synthetic result is not battle acceptance.


## Assembled passage and battle observations (135–139)

Targeted composition preserves every existing function, adds exactly one
cmd_aapassage and stays within the script budget. Four-forest run 135 still
fails 3/4: arrivals 202.5, 206 and 229.5 seconds. Mover90 now reaches full key 2
instead of stopping at alpha 0.94444. Zw2 still presses into its open leaf.
Thus the passage correction does work in production, but does not by itself
close deep pursuit. Native physical replay 138 confirms the stopped pose is
clear, forward is blocked, and retreat works. Probe 137 accidentally left
Mover90 closed; it is an invalid open-leaf comparison, preserved explicitly.

The same compiled consumer runs a 180-second twelve-zombie battle without a
compiler running (136). Session step median/p95/p99: 5.926/25.090/37.305 ms;
maximum 178.743 ms. These exclude QUIC, snapshots and trace writes. Quantized
bot thread CPU totals 105,000 ms; script call wall totals 39,391 ms. No performance
improvement or smooth-battle acceptance is claimed. Full 3D review finds isolated
stalls beside open Mover45/Mover90, not only crowd jams. Cross-floor Search and
Wander requests still spend thousands of local grid expansions.

Candidate 139 permits any covered positional goal to use the same prepared
provider. A bounded native local repair activates only when the next segment
actually intersects an open operable leaf. It rejoins the retained corridor
beyond the obstruction; ordinary physics, floor checks and search budgets remain
unchanged. This replaces rebuilding every global leg with repairing only the
observed dynamic obstruction. The group result is pending. A subsequent review
adds an explicit wait while repair is pending, keeping the stored corridor join
index stable; that adjustment is not included in 139's captured sources.

The library-consumer runner avoids unrelated executable builds but its initial
compile is still 4m45s (135); it is not a demonstrated cold-build speedup.
Battle 136 reuses that exact compiled binary and canonical fixture directly.


Run 139 passes all four unchanged forest-to-deep arrivals: Zw2 190.5 s, Zw1
202 s, Zw0 205 s, Zw3 230.5 s. This includes the previously permanent open
Mover90 obstruction, without guiding/relocating zombies or changing arrival
thresholds. The following 140 repeats this with the pending-repair wait above.
Complete mixed battle and network responsiveness are still open.


Run 140 passes with the pending-repair wait: Zw2 190.5 s, Zw0 202.5 s,
Zw1 205.5 s, Zw3 229.5 s. Runtime source remained unchanged through compilation
and the following battle. Compared with 135, Zw2's stay within ten native units
of Mover90 falls from 82–360 s (still stuck at capture end) to 82–96 s, then it
continues to the deep target. Full 3D paths remain in the external traces.

The exact same compiled consumer executes the restored 180-second mixed battle
141 without Cargo running. Session step mean/median/p95/p99 become
6.836/4.243/23.626/35.063 ms (136: 8.350/5.926/25.090/37.305 ms).
Bot thread CPU aggregate becomes 68,531 ms (136: 105,000 ms); script call wall
40,153 ms (136: 39,391 ms). Populations evolve differently because decisions
change: final defenders/zombies are 36/12 versus 37/12. Do not report a universal
performance percentage or network-latency acceptance. Full step spikes persist.

The retained battle reviewer finds longer stationary intervals at closed/blocked
Mover110 and crowded open Mover153. The isolated Mover90/Mover45 stalls from 136
are absent from its longest-stall list, but this is not complete battle acceptance.
Next correction should stay on those actual obstruction/interaction traces and
remaining CPU cost. Reproducible offline bake/installation, focused checks,
integration and replacement preview delivery are still unfinished.


## Initial obstruction and patrol convergence (142 onward)

Battle 141 retains Mover110 blocked at key 0 with from=to=0 for the entire
capture. Collision rollback restores the previously accepted state's endpoints
as well as its pose, losing a new opening target when a bot requests it after
the map-mover phase. The first synthetic attempt 142 has a local API typo.
An external-input-order comparison passes before correction (143), so it cannot
establish the bot-phase failure. The corrected tick-order regression reproduces
target 1 becoming target 0 (144). Mapping the accepted pose onto the requested
path preserves intent without accepting penetration; all four mover/action
comparisons pass afterward (146), including resume and obstruction clearance.

Native physical replay 145 removes the other actors at Mover153. One soldier's
same control passes while the other's segment still changes direction against
geometry; this distinguishes the two bodies rather than claiming every stall
is caused by a crowd. It uses the retained motor diagnostic binary; its older
grid-search output is explicitly excluded from current planner evidence.

The current private 148 diagnostic retains only naturally admitted Gd17/Gd18,
keeps their original brains/goals and records their convergence at Mover153.
No guard relocation, guide, non-solid body or replacement intent is supplied.
Derived route-provider/repair fields expose which current executor is active.
The result fails 1/2; this is an ignored diagnostic outside mandatory CI.

## Bounded rejoin correction and wrap (150–154)

Run 150 exposes the actual remaining failure: the chosen sparse corridor point
is about 16 units away, beyond the 10.56 local search boundary, although the
intended rejoin horizon is 7.04. Useful partial paths are discarded and the bot
returns to the known blocked segment. Owner 16 remains stuck for 120 s even
after its partner crosses. Thus a crowd system is not required to explain it.

The extracted old selection rule fails the source-free sparse-corridor negative
152. Attempt 151 was a missing diagnostic field in a unit literal, fixed before
the valid comparison. Sampling a rejoin along the corridor within its path-length
horizon preserves its tail/destination and interpolates height. Both sparse/dense
and sloped/bent/short-route checks pass afterward. Unchanged assembled run 153
passes both natural patrols: owner 16 at 3.5 s, owner 17 at 4 s after observation
begins (150: owner 17 at 82.5 s, owner 16 never crosses). No placement or goal
change is used. Exact input bytes are in bounded-rejoin-pair-153-inputs.

Maxwell requested wrapping. Final mixed battle 154 reuses that exact compiled
consumer with restored defenders and no compiler running. No new investigation
will be started in this checkpoint. The current continuation index is
2026-10-24-invasion-wrap-handoff.md. Integration/delivery remain explicitly open.

Final 154 completes 180 simulated seconds: Mover110/Mover153 fully open by 2/1 s;
the same reviewer's longest flagged commanded-but-stationary interval falls from
75.5 s (141) to 8.5 s. Session mean/median/p95/p99 become
6.472/3.922/23.055/33.614 ms, maximum 168.965 ms. Aggregate bot thread CPU
67,281 ms; script call wall 39,068 ms. No QUIC/snapshot/trace cost is included.
Populations differ (39 defenders/10 zombies vs 36/12); no universal performance
percentage or complete battle acceptance follows. Remaining outdoor/door87
anomalies are retained in targeted-passage-battle-154-flow.json. No new repair
is started at this wrap checkpoint.
