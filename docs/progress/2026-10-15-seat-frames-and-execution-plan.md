# 2026-10-15 Authored seat frames, eye origins and consolidated plan

Root continues on playtest-v031 / codex/v031-remaining-capabilities. Source base
14cbc901 remains unchanged, with substantial uncommitted candidate work. Private
source base c297f1e remains on the separate Halo fidelity worktree.

## Changes and rendered evidence

Added generic optional Seat.rider_pose: a body owning the requested clip uses
its authored model-origin frame; other bodies use their own pose and align the
hip to the seat. Legacy seats retain existing fitting. This addresses the mixed
Blockhead-relative versus original pelvis-origin conventions. The private vehicle
now binds its original driver, passenger and gunner markers and distinct original
Chief clips. Foreign bodies do not borrow his poses. Braced gunner and raised
passenger posture are visible in bounded captures; full weapon/grip/aim remains open.

Maxwell correctly rejected the first passenger-eye render: the windscreen frame
cut across its middle. Added an optional authored seat-local eye, shared between
client first-person camera and authoritative motor eye used by interactions. The
private map supplies original camera markers. No FOV/view lock was introduced.
Updated capture uses 90 degrees horizontal at 16:9 and clears the windscreen much
better. It lacks the held weapon and map backdrop, so it is not fidelity acceptance.

Owned/fallback pose frame regression, rotated/scaled seat-eye regression and motor
eye invalid-input/dismount regression passed. Source checks passed. The host's
standing hand/grip cache still needs a seated-pose audit: matching eye origins
alone does not establish full host/client muzzle parity.

## Verification and failures

Focused run for client, vehicles, vehicles-import and motor reached 14 steps in
780 seconds, then stopped compiling bri-net test loopback: its Image constructor
lacks first_person_nodes and first_person_effects. The run is not green. Preceding
client unit/other focused steps passed. Earlier failures from private-fixture
clippy (clamp and cloned slice) were fixed. The generated sample-map regression
also exposed native RGBA metadata drift; regenerated the verified sample target
and structurally checked that only paint_overlay=false on two materials changed.
Its dedicated regression then passed. Do not bypass or waive remaining failures.

No latest release rebuild/install, public push/merge/tag/publication or interactive
playtest occurred. The installed executable/private content are older than these
changes. All original installations remain read-only; private captures remain
outside public Git.

## Audit and execution plan

At Maxwell's request, consolidated the conversation, source findings, original
screenshots and historical private notes into private halo/EXECUTION-PLAN.md.
COVERAGE.md owns status; execution plan owns ordering, dependencies and exit
criteria. Detailed review files remain evidence/history, not readiness claims.
The complete source crosswalk remains an explicit investigation step.

The plan covers checkpoint stabilization; original rifle pipeline; full Warthog
appearance/dashboard/seats/controls/independent turret/rider motion/audio/collision
feedback; material-specific impacts/decals/contact and target shields/flesh;
Blood Gulch lighting/teleporter/ambience/haze/sun flare; characters/stock tools/HUD
lifecycles; inherited modding/map/network requirements; exact packaged playtest and
release gates; and later weapon/vehicle expansion without automatic release scope
expansion. Latest user references are preserved in ignored local artifacts.

Source sky recovery found warm haze cap .4 at 3/100 CE units versus the private
map's placeholder blue distant fog. Native fog end controls both extinction range
and geometry visibility; separate these before binding source ranges, or distant
cliffs would disappear. Original sun references and metadata were recovered by a
private Python importer; original flare composition and occlusion remain open.
No engine fog change or private runtime fog replacement was made in this pass.

All new observations remain tracked: rifle reflection, dashboard live meters,
vehicle-specific posture, rider bob/gunner knee bend, crash effects/audio, plasma
projectile/glowing impacts, sniper zoom/readout, hover/flight trails/audio/controls
and tank track articulation. Optional assets follow the current complete slice.
Maxwell performs all interactive gameplay and owns final feel/visual acceptance;
no videos or enormous new test framework required.
