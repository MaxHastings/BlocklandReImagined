# Death collision: host and predictor controls

The host death transition calls `set_corpse` on the player motor; this changes
its collision group instead of making it a universal sensor, and clears its
own support tag. Respawn restores the live collision group. The client supplies
only living, unmounted remote poses to the prediction mirror. Cosmetic ragdoll
physics is separate from the authoritative player collider.

The existing support test now also restores the lower motor to a live body and
checks that another body can land on it and identify it as support. A new
headless host/predictor case walks into a live player, kills that player with
the real suicide command, supplies the predictor the same eligible remote set,
and checks both that the host walks past the corpse and that prediction stays
within 0.05 units throughout. It tests the predictor's removal behavior with
the set supplied explicitly; it does not exercise network delivery or the App's
vitals filter itself.

`cargo test --locked --profile playtest -p bri-motor --test still_controller`
passed all five; the targeted `cargo test --locked --profile playtest -p
bri-sim --test prediction
prediction_stops_colliding_with_a_player_as_soon_as_the_host_kills_them --
--exact --nocapture` passed; scoped formatting and `cargo clippy --locked
--profile playtest -p bri-motor --test still_controller -p bri-sim --test
prediction -- -D warnings` passed. Two fixture warnings about absent weapon
hand nodes are unrelated and were not hidden.

These controls do not reproduce Maxwell's reported lingering collider. Real
network/vitals transitions, weapon deaths, mounted/scaled players and cosmetic
ragdoll combinations still need investigation before claiming the issue fixed.
