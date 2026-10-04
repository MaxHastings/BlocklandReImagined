# 2026-10-03 v0.2.3 mounted pursuit audit

Maxwell reports that bots board/use the Tank and fire, but fail to drive toward
a player who retreats or hides. This is active user-scoped correctness work,
not publication authorization. All interactive testing remains Maxwell's.
This lane reads production source and generated content only; root owns
integration and the serialized compute queue. No Cargo/GPU run has been made
for this report yet, and no production correction is claimed verified.

## Canonical control and evidence path

The imported Tank is an ordinary Wheeled definition: control seat 0, passenger
seat 1, weapon seat 2. These are declarations; no proposed code checks its name.
`bot_vehicle_body` supplies a conservative chassis navigation body only to a
Wheeled control-seat occupant. `bot_vehicle_weapon` supplies mounted attack
capability only to the actual weapon-seat occupant. Native hand combat keeps
its existing mounted fallback. Driver and gunner consequently have independent
brains, visibility and ordinary controls.

For a separate driver without the vehicle gun, `step_bot` explicitly sets
near/far to `(0, 0)`, so a visible ground enemy should select Chase and request
a goal at its observed position. The gunner may simultaneously select Fight
and continue independent world-space aim/firing. `bot_seated_input` converts
only the driver path into wheel throttle/steering and keeps gunner wheel
inputs zero. The source does not support a conclusion that the Tank gunner's
firing alone cancels the driver's requested movement.

Direct sight supplies actual observed positions. Once sight fails, existing
dated memory and `search_memory` supply the last anchor and at most four probes.
The native search never needs the hidden actor's current transform. A gunner's
fresh allied sightings may legitimately update the driver's dated evidence;
when neither can see the subject, neither may manufacture an update. The
published Blockhead chase-radius policy remains 48 m from its leash, rather
than unlimited pursuit. It is not weakened or silently expanded here.

## Navigation and local execution remain separate

The real Tank bounds are approximately 4.874 m wide by 6.470 m long. Its
conservative navigation footprint is the 8.10 m diagonal square, covering
turning orientations. The existing motor execution instead casts the actual
oriented chassis bounds, excludes its own hull/occupants and its currently
hostile subject, and brakes for stopping distance, collision and allied people.
Thus navigation can reject a road wider than the oriented hull but narrower
than that conservative footprint. This is a plausible route-availability gap,
not proof about Maxwell's scene. Shrinking the grid footprint would remove
turning clearance and is not justified by this read-only audit.

Chase/Search may also settle at a bounded partial route; the wheel adapter
retains bounded progress/reverse/replan/dismount handling for unavailable travel.
The earlier battle's stationary bot 8 has an exhausted search, but it was
on foot. Neither that receipt nor this source audit proves the same physical
cause for the vehicle report. Actual role, requested goal/waypoint, nav start,
local hazards and real chassis movement must be distinguished in a regression.

## Rejected hypothesis and valid control regressions

The first read-only hypothesis treated a Wheeled seat declaring both controls
and a weapon as a simultaneous armed driver. Deeper canonical inspection
invalidated that setup: `Definition::seat_role_for` prioritizes Gunner for any
non-actor weapon-bearing seat. Its ordinary controls therefore do not execute
wheel throttle. A combined-seat fixture would fail to prove a wheel recovery
bug, even though the bot adapter's private control-seat check also sees the
controls declaration. Root received this correction before any patch was
applied or any test was run. The proposed production hunk and combined-seat
fixture were removed; no supported-mechanism correction is inferred from them.

A separate steering hypothesis was also rejected. Although imported Tank/Jeep
content declares strafe-steering support, bots retain ordinary default steering
preferences `(false, false)`, allowing the existing mouse-steering adapter.
There is no evidence for changing those preferences or steering policy here.

`/tmp/bri-v023-vehicle-pursuit.patch` now contains **fixture additions only**
to invented `crates/chaos/tests/bot_interactions.rs`, with no production hunk
or new content:

- Two independent renamed/reordered chassis crews must physically advance
  while the visible human retreats under ordinary movement and the real gunner
  emits observed mounted projectiles. Diagnostics distinguish requested
  Chase ticks from actual chassis displacement.
- Two paired hidden-human placements must produce the same requested dated
  goal/timestamp and actual chassis trajectory, with real travel toward the
  prior observation. The human hiding setup uses the existing canonical admin
  DropPlayerAtCamera command; no bot is moved or assigned controls by a test.

Standalone rustfmt parsing and `git apply --check` pass. These valid separate-
role fixtures are **not applied or compiled** yet. Root will run them against
unmodified pursuit production to establish whether the failure is absent
movement intent, unavailable static route, local collision/ally/braking or a
control adapter failure. No runtime patch is justified until that distinction
is observed. Any failed receipt must be retained and explained rather than
weakened into a success claim. Geometry-rich/held-out reproduction and actual
cover navigation may still be required after the open-ground checks.

Root has the exact proposed paths and current patch. Suggested commands when
the root-owned compute queue permits (leave `CARGO_TARGET_DIR` unset):

```sh
CARGO_BUILD_JOBS=2 cargo test -p bri-chaos --test bot_interactions \
  an_armed_crew_advances_while_its_visible_target_retreats \
  -- --exact --nocapture
CARGO_BUILD_JOBS=2 cargo test -p bri-chaos --test bot_interactions \
  a_crew_pursues_only_the_last_observation_after_its_target_hides \
  -- --exact --nocapture
```

No user report, pursuit correction, timing improvement or full alpha acceptance
has been closed by this static checkpoint. The prior bounded battle trace
proposal remains separate and pending.
