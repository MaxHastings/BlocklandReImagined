# 2026-10-04 Quick clicks: the input path holds, the soak's fire() did not

Max's soak with real content on his Windows PC failed on the second blast:
`Timed out waiting for bricks knocked out: 5s ... images
[rocketlauncherimage state "Ready"] bricks (36, 0)`. The question was
whether a press and release landing in one client frame, one send or one
host tick is lost, which would lose real players' clicks on a slow machine.

## Finding: no click is lost in the game's input path

Traced `HeldControl::Fire` end to end:

- `crates/ui/src/ui.rs` `held_control` (~line 897) emits one
  `GameAction::Held` per edge, event-driven, never by polling state per
  frame, so a press and release in one frame are two actions.
- `crates/client/src/app/actions.rs` `dispatch` drains every action in
  order; `crates/client/src/building.rs` (~line 1549) turns each edge into
  its own `Command::WeaponTrigger { down }` and `App::command`
  (`app/session.rs` ~line 989) queues each to the net worker
  (`network.rs` `request_with_aim`, a 64-deep channel; a full queue errors,
  it never coalesces).
- The net client writes each as its own reliable request; the server
  applies each as it arrives (`crates/net/src/server.rs` ~line 1492).
- The host queues edges: `Session::weapon_trigger`
  (`crates/sim/src/session/weapons.rs` ~line 211) pushes each onto a
  per-player queue, and `step_weapons` (~line 270) takes one edge per
  tick, so a press is held for at least one tick before its release, the
  way v20 registers a click. A lapsed input lease (a stalled client) keeps
  queued presses rather than dropping them.

Existing guards already cover the host and wire hops:
`native_gun_quick_trigger_edges_use_host_tick_pose_and_reliable_sound`
(`crates/sim/tests/session.rs`, both edges in one tick),
`a_click_that_beats_the_movement_after_a_stall_still_swings`
(`crates/sim/tests/tools.rs`) and the loopback trigger test
(`crates/net/tests/loopback.rs`). Nothing covered the whole App path.

## Changes

- New guard `a_tap_inside_one_frame_still_fires` (`app_soak.rs`): the
  normal App hosts the soak's game, takes the rocket, and requests press
  and release before one frame, so they leave in one send and land in one
  host tick; the rocket must knock out the wall. It passes. With a
  temporary mutation making the host trigger level-based (a release
  cancelling an untaken press, in `weapon_trigger`) it fails with exactly
  Max's shape: `Timed out waiting for the tap's rocket to knock bricks
  out ... bricks (36, 0)`. The mutation was reverted.
- `Run::fire()` holds the trigger until the image in hand takes the press
  (hand 0 leaves "Ready", or a shot of the player's is in flight, or bricks
  are out: one slow frame can span a whole fire cycle), failing within 2 s
  of game time with a message naming it; then it lets go. The dead
  player's click to respawn is `Run::click()`.
- Phase measurement: phases ended on game state but not on completed
  lighting loads. A map's lighting-mode source or compatibility bake lands
  on its own worker's time and then uploads the whole map scene again
  (`prepare_render`: `gpu_scene = None`), which on a slow machine can land
  in the next, steady phase. New probe `App::map_lighting_settled` (source
  and bake finished, the switchable sheets taken, the scene on the GPU);
  the soak waits on it before its first phase and inside the map change.
  The equip condition moved into `App::switchable_sheets_due` so the probe
  and the renderer share it.
- Phases report game ticks (`ticks`); wall time is kept as
  `wall_seconds`, reported only. Steady phases now also require one scene
  renderer to have counted the whole phase (`counted`), so a renderer
  still compiling or rebuilt mid-phase cannot hide uploads. No threshold
  changed.

## Not fixed, worth knowing

`step_weapons` forces the trigger up while a player's input lease has
lapsed (60 host ticks, 0.5 s at 120 Hz). When the lease renews, a trigger
the player is still *holding* stays up until they click again: a hitch
over half a second stops automatic fire. Clicks are not lost (queued
presses wait for the lease). Restoring the held level on renewal is a
small change but a policy call for the lease's owner.

What made Max's second rocket miss is not established from the log
alone: the image was "Ready" and no brick went out. The new `fire()` will
now name a press the image never takes, separately from a blast that
fails to break bricks.

## Commands

- `cargo test -p bri-client --test app_soak` (synthetic): 2 passed
  (`a_tap_inside_one_frame_still_fires`,
  `steady_play_builds_and_uploads_nothing_whole`), 3 ignored.
- Same with the mutation: the tap guard failed as above.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
