# 2026-10-01 Vehicle mouse and Add-On join tests wait on state

Two gate tests failed under 8-way parallel load and passed alone.

`vehicle_first_person::invert_mouse_in_vehicles_turns_the_nose_both_ways_through_the_app`
- The push was 30 looks, each followed by `run_for(1/60 s)` of wall time.
  The client makes one move per 1/120 s of frame time, at most 12 a frame
  (`MAX_STEPS`). On a slow machine each look's frame ran up to 12 moves, so
  the push stretched from 60 moves to several hundred. A stretched push can
  dip the nose into the ground and bounce it back up.
- The host's nose was read from the newest pose as the push ended. A seated
  driver's host runs one queued move a tick, and the hosted ticker skips
  missed ticks (`MissedTickBehavior::Skip`). Under load the host can still
  be up to the queue's length (30 moves or more) behind the client, so the
  pose had not taken the push yet.
- Every `until`/`run_for` bound was wall time.

Fix, in the test only:
- `until` and `run_for` count server ticks once in game, as in `item_ghost`.
  A wait whose game has stopped advancing still fails, at 20 times its
  budget plus 60 s.
- Each look is followed by exactly one 1/60 s frame of client time (two
  moves). Between looks the client is held at zero frame time until the
  host's pose has taken all but 4 of the moves sent, so moves never pile up.
- After the push, the client stays held while the host runs off its queue.
  A seated queue runs at least one move a tick, so a `driver_input` that
  stays the same for 120 ticks means every move has run. The host's nose is
  read from the first pose that includes the push's last move. The "before"
  value is the last pose before its first move. Both come from a record of
  every pose seen.

`add_on_join`
- `add_ons_the_host_turns_off_are_not_required_and_ones_it_runs_download`
  hosts six games in a row on one port. `leave` stepped a fixed 60 frames,
  but a host stops in the background (`Worker::finish`) and keeps its UDP
  port until it does. `leave` now waits until the apps are out of the game
  and the port can be bound again.
- `step` used a fixed 16 ms per frame, whatever the frame took; it now uses
  the frame's wall time, as `item_ghost` does. `until` counts game ticks
  once everyone is in game.
- The Blockhead Bot test's off case stepped 180 unpaced frames, so it saw
  almost no game time before checking that no bot spawned. It now waits for
  twice the server ticks the bot took with the Add-On on, and at least 3 s.

No assertion or tolerance changed.

Checks run in the cloud:
`cargo clippy -p bri-client --test vehicle_first_person --test add_on_join -- -D warnings`
passed. `cargo test -p bri-client --test add_on_join --test vehicle_first_person`
passed: the two content-free tests ran and the rest were ignored. The
content tests need converted v20 content and a GPU, so only the gate runs
them.

Still shared, needing production changes:
- Downloaded Add-Ons go to `<content>/.downloads` (`app.rs` `join`). Every
  test and run shares that cache, so a "downloaded" check can pass on an
  earlier run's copy.
- LAN hosts advertise on the fixed discovery port 28050 unless
  `BRI_TEST_DISCOVERY_PORT` is set. That only produces a warning.
