# 2026-10-07 Tests that passed only on retry in the gate

Gate failures (relayed from the PC's gate logs):

- `app_soak` steady play (3 of 4 gates) and long soak (once): "Timed out
  waiting for the map's lighting to finish loading: 120s of game time",
  after scene compiles of 14-59 s.
- `a_game_entered_before_the_world_pipelines_compile…` (lib): failure text
  not seen.
- `bundled_in_game` client test ("The server ran 1212 ticks without
  answering command 13"): handed to the PC capabilities lane, which can
  reproduce it.

## Not reproduced here

This container (4 cores, lavapipe) ran each alone and beside the other soak
tests, with 4 to 28 busy-loop processes: all passed. The changes below are
reasoned from the code, not proven against the gate's failures.

## Changes (8b2e3398)

- **Soak lighting wait**: waits on the work, with only `wait::STALL`, like
  `app_item_render`'s (2026-10-07-load-flaky-waits). The lighting source and
  bake load on workers that a loaded machine slows while the host ticks on.
  The failure now names which part of `map_lighting_settled` is missing
  (`App::map_lighting_parts`).
- **Held-pipelines test**: a frame used to fail if it took 2 s of wall time.
  The claim is that a frame does not wait on the compile; the compile is
  held until the test releases it, so the test now checks the pipelines are
  still unbuilt after each held frame. The hold also ends after the outer
  guard, so a frame that did wait returns and fails instead of hanging.
- **`Client::command`**: its no-answer failure says how current the client
  was when it sent (update tick, newest pose tick, unread events), to tell a
  client behind on its own stream from a host that did not answer.

## Rocket through the soak's wall (28x load only)

At 28 busy loops one soak run failed "bricks knocked out: 5s of game time".
Trace: the first blast knocked out all 36 bricks, they came back 192 ticks
later, and the second rocket flew through the respawned wall, with each
client step spanning about 60 ticks.

`brick_damage::a_rocket_knocks_out_respawned_bricks_again` (new, server
only, no load) blasts two bricks under the shortest brick respawn (2 s),
waits for them to stand again and fires a second rocket: it knocks them out
again. Respawned bricks are solid, so the soak's miss is an artefact of
60-tick client steps at that load, not a game bug. Not chased further.
