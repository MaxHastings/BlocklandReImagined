# Issue 400: overload reproduction

The original soft-cone fixture can freeze upright when the wall-clock physics
budget is exceeded. `Ragdolls::advance` calls `settle_oldest` after an over-budget
frame; the latter sleeps every body in the jointed group when any one of them
touches a fixed floor. A deterministic test using the original fixture and
requesting budget settlement each simulated frame failed at three seconds:
hip `Vec3(-3.257146e-7, 0.90787923, 0.36821985)`. The tracked intermittent
failure reported hip y=0.775627. This shows a mechanism capable of the observed
failure; it does not prove the historical run took the same path.

Command: `cargo test --locked --profile playtest -p bri-client --lib ragdoll::tests::soft_cone_falls_even_when_physics_is_over_budget -- --exact --nocapture`.
The correction keeps the budget but only settles a supported group when each
body's vertical and angular motion has slowed. Horizontal sliding may still
be stopped under pressure. This prevents the reproduced early sleep without
requiring every limb to contact the floor. Bodies still moving vertically or
rotating continue to simulate, so severe overload may last longer; the
existing body-count budget remains. A stationary upright balanced assembly is
not categorically prevented from sleeping by this criterion.

Validation: the new overload case passed, as did all 7 `ragdoll::tests` and
24 `addon_physics::tests` under `cargo test --locked --profile playtest -p
bri-client --lib` with the respective filters. The original and forced-overload
soft-cone cases each passed 100 separate invocations of the compiled test
binary. `cargo fmt -p bri-client` and `cargo clippy --locked --profile playtest
-p bri-client --lib -- -D warnings` passed. This is bounded fixture evidence,
not proof of the original gate's exact cause or an exhaustive performance test
with many bodies. Extended release validation remains necessary.
