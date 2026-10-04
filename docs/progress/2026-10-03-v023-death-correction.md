# 2026-10-03 v0.2.3 death and player prediction rejection audit

Maxwell reports a disconnect after dying, approximately
`Movement prediction: Invalid authoritative player correction`. Root assigned
this as high priority. This lane keeps existing engine files read-only and
hands back a patch; no Cargo command, gameplay input or visible window was used.

## Exact boundary and narrowed candidates

`App::poll_session` calls `Motion::observe`; its error becomes a connection
failure with the `Movement prediction:` prefix and disconnects. Local poses
reach `Predictor::reconcile` (or `teleport` while mounted), which calls
`bri-motor::Player::restore`. Its single generic ensure rejects:

- Different owner from the predicted body.
- Feet nonfinite or any axis outside +/-1,000,000.
- Velocity nonfinite or any axis outside +/-1,000.
- Nonfinite yaw/pitch.
- Torque tick phase outside its partition or nonfinite interpolation
  `tick.from`/`tick.feet`.
- Invalid tether state.

The normal network Replica::pose boundary already rejects nonfinite current
feet, velocity, yaw/pitch as `Invalid player pose`. If the reported full prefix
is exact, a current nonfinite feet/velocity value would normally fail earlier
under that different label. Remaining candidates include finite out-of-range
motion, interpolation state, tether or owner. This is source-level narrowing,
not captured failure attribution.

Normal death sets health/alive state, ejects seats/riders, releases equipment,
clears input queue and changes control to Corpse. The same kinematic player
motor continues ordinary corpse fall/collision. At the five-second timeout it
becomes a sensor and the visual body clears. There is no independently simulated
ragdoll replacement. Respawn dismounts, teleports using the canonical motor,
restores archetype/scale/energy and solidity, clears inputs and returns control
to Player. Neither ordinary path changes owner or writes an invalid tick phase.
Weapon/scripted pushes already have bounded native paths; no new velocity/NaN
clamps are justified by this report alone.

## Proposed handoff and evidence limits

`/tmp/bri-v023-death-correction.patch` replaces the one generic ensure at its
existing boundary with equivalent per-invariant checks. Errors retain the
original identifying prefix and now contain the rejected field and compact
actual values (owner, feet, velocity, look, tick phase/interpolation or tether).
Validation bounds, accepted state, real error/disconnect policy and wire/schema
remain unchanged. This is focused failure evidence for the next reproduction,
not a claimed cure. No diagnostics infrastructure or speculative recovery is
added.

`/tmp/bri-v023-death-correction-tests.patch` contains the separate regression.
It configures an ordinary high spawn and receives real 80-unit fall damage, or
uses the public canonical off-centre explosion operation for real lethal damage
and impulse. It submits ordinary movement inputs, sends every sampled
post-damage state through real `Predictor::reconcile` restoration/replay, waits
past the five-second corpse timeout, issues ordinary `Command::Respawn`, then
checks another 120 live ticks. No authoritative fields are manually made
invalid. The baseline may honestly pass: that would bound normal lifecycle
coverage, not reproduce the user's specific disconnect. The seated/flying case
has been read but is not represented by this proposed test yet.

Formatting and `git apply --check /tmp/bri-v023-death-correction.patch` passed.
Root owns runtime verification under the current compute lease. Commands:
`cargo test --locked -p bri-sim --test prediction normal_death_physics_and_respawn -- --nocapture`
and the existing `stale_and_forged_corrections_are_rejected_and_history_is_bounded`
regression, followed by relevant prediction/motor checks and strict Clippy.
No causally justified production-state correction is proposed until the actual
rejected field is identified. The separate old Windows firefight NaN clamp
panic remains unclosed and is not attributed to this death report.

## Root baseline execution

Root ran the new normal lifecycle regression against unchanged production.
`/tmp/bri-v023-death-lifecycle-before.log` reports **1 passed, 0 failed**. Both
real fall damage and lethal canonical explosion paths reach corpse timeout,
ordinary respawn and subsequent prediction without the user's reported error.
This is explicitly negative reproduction evidence, not a causal repair.

The explosion test's fourth `0.0` argument is **brick_radius**, not impulse.
`Session::explode(center, radius, damage, brick_radius, look, source, caller)`
always applies a native push after damaging each victim: `(away + Y*0.5) *
amount * 0.2`, then `Player::push` bounds speed at 200. The radius-four,
damage-1000 fixture places the blast hack point 0.5 from its center, so admitted
amount is 875. For its center offset `(0.3,1.0,0.4)`, the raw push is
approximately `(-46.96,-69.02,-62.61)` (104.4 magnitude) before map collision,
well below the restore per-axis 1000 bound. Thus this is real nonzero knockback,
although it is not a measured native Rocket explosion force or proof that the
user's particular collision/seat/tether case is covered. Root has the diagnostic
patch but had not applied it at this baseline; no numeric recovery was added.
