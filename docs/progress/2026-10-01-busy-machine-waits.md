# Busy-machine waits (batch171 and 171b)

Six host and join tests failed in full gate runs on a loaded PC and passed
alone.

## Causes and fixes

- **The host's first tick stalled on big event worlds.** The first event
  phase installed every brick's event program on the server loop (3.4 s in a
  debug build for the 48 heavy bricks in `state_limits`, longer under load).
  The joiner's QUIC handshake, spawned behind it, missed its 10 s wait ("No
  server answered"). `Session::prepare_events` now does that scan before the
  host serves and on the map loader's thread. `EventWork::installed` counts
  the programs each phase installs; `prepared_events_leave_the_first_tick_nothing_to_install`
  proves the first tick installs none once prepared.
- **`Client::command` gave every reply 10 s of wall clock.** A newcomer's
  reply follows the rest of the world on its stream (net chaos). It now
  fails when the host runs 1200 ticks (10 s of game time) without the reply,
  or sends nothing for 60 s.
- **Client tests waited on wall clock before entering a game.** One shared
  wait, `crates/client/tests/support/wait.rs`, now serves add_on_join,
  app_flow, item_ghost, vehicle_first_person, default_add_ons,
  release_smoke, map_shapes, wheel_flow, view_jitter, held_items_render,
  app_item_render and player_types_render. In game, its budget is server
  ticks. While loading, it fails only when an app's loading progress
  (`App::loading_revision`) or server tick has not moved for 300 s. The
  headless host test inside app.rs uses the same rule.

## Not fixed here

`every_tank_seat_sees_from_the_riders_eye_for_host_and_guest::synthetic`
fails about 3 runs in 4 on main when its binary runs in parallel. Seat 0's
rendered eye trails the latest tank pose by 0.06 to 0.13 m along z, against
a 0.05 m tolerance. It compares the presented camera with the newest network
pose while the tank drifts. That is a separate flake for the vehicle owner.
