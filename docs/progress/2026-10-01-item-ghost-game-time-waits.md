# 2026-10-01 item_ghost waits count game time

Batch158's gate saw `item_ghost::lan_host_and_guest_both_see_each_others_pickup_ghosts`
fail under 8-way parallel load with "Timed out waiting for standing in the
ghost until it returns". It passed alone in 59 s. All eight captures had
finished, so the failure was in the host's pickup cycle, while the host
stood in the ghost for the 8 s (960 tick) respawn.

Cause: the hosted server's ticker skips missed ticks
(`MissedTickBehavior::Skip`, `crates/net/src/server.rs`). On a loaded machine
game time runs slower than wall time, but the test bounded the wait with a
62 s wall clock. The sim was still on its way to the respawn tick when the
wall clock ran out. The client's motion also drops time on a frame longer
than 12 ticks (`MAX_STEPS`), so walks slow down under load in the same way.

Fix, in the test only:
- `until` counts its budget in the server ticks every app has seen, once all
  apps are in game. Loading and joining are still wall time, because no
  tick exists before then.
- `run_for` and `walk_into`'s give-up also count ticks.
- A wait whose game stops advancing still fails, at 20 times its budget of
  wall time plus 60 s.

The assertions and budgets are unchanged; only what the budgets are counted
in changed.

Checks: clippy `-D warnings` on the test target and rustfmt were run in the
cloud. The test needs the converted Bedroom and a GPU, so the gate runs it.
