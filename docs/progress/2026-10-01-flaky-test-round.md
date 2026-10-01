# 2026-10-01 Flaky test round

These were the known flaky tests that failed under the gate's parallel load and
passed alone. Each now waits on state. No #[ignore], no skip, no looser
assertion.

- **Host drops clicks after a stall** (`sim/src/session/weapons.rs`, a real
  bug behind app_flow's printer step). A lapsed input lease (60 ticks
  without movement) cleared the player's queued trigger presses. After a
  stall, the reliable trigger commands can reach the host before the
  movement datagrams that renew the lease, so the click was lost. The lapse
  still lets go of the button but keeps queued presses. Test:
  `a_click_that_beats_the_movement_after_a_stall_still_swings` fails
  before and passes after.
- **app_flow** (`native_host_cancel_rehost...`, which includes the printer step,
  and the weather test). `until` counts its budget in server ticks seen in
  game. A wall-clock bound is used only when nothing advances. The GPU
  readback waits for the copy.
- **vehicle_first_person** invert mouse and **add_on_join**: see
  `2026-10-01-vehicle-mouse-and-add-on-join-waits.md`.
- **stresslab_flow** stopped waiting on the save when the staging file
  appeared, before the rename. It now drops the App, which waits for the host
  to stop and save, then checks.
- **sandbox shader**: the endless-loop cap came from timing the GPU, and frame
  stops used the 8 ms wall budget. `gpu.rs` has a
  `render_offscreen_at_speed` seam, and the test renders at two given
  speeds, asserting each cap exactly and that the caps differ. Tests not
  about wall time use budgets without time limits.
  `wall_clock_time_is_a_budget_too` still asserts a 20 ms stop lands within
  2 s, because wall time is what it tests. Under load, 5 of 5 runs of the old
  file failed and 15 of 15 runs of the new one passed (llvmpipe).
- **event_storm** zero-delay loop: a 120 s timeout covered 1,200 debug-build
  ticks, which take 30 s idle and up to 760 s under load. The loop now ends
  when the replica reaches the tick, and fails only if no message arrives for
  300 s.
- **natpmp mappings**: the fake router quit after counting 4 datagrams, so
  a 250 ms RFC resend could take the delete's place. The retry schedule is
  now a parameter (`RFC_WAITS` in play). The test router serves until the
  delete arrives, and the test asserts exactly 4 requests in order. A forced
  300 ms router delay fails the old test and passes the new one.

Still wall-clock in production, and not changed: `client/src/network.rs`
fails a connection after 120 s, including a host's own content preparation.
A loaded debug build hosting Bedroom twice could hit that.

Checks: clippy `-D warnings` across the workspace; bri-sim, bri-net and
bri-client-sandbox tests; and bri-client lib plus the touched test targets
(the content ones are ignored here).
