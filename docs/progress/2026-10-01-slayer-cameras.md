# 2026-10-01 Slayer cameras: path cameras, spectating, fly-through

Branch `claude/project-thread-t8k5dx` (Slayer/CTF), Step 8 of the Slayer
port, after main bc27f00c5 was merged in.

## Engine seams (generic, `player` capability)

- `follow_path(p, knots)` / `follow_path(p, ())` and `on_path_node(p, knot)`:
  Torque's `PathCamera`. `ControlObject::Path`; the path
  (`crates/sim/src/session/camera_path.rs`: Catmull-Rom with kink, linear
  and position-only knots, leg time = length / mean speed, a jump knot cut
  to at once, 120 ticks a second) replicates in `Vitals::camera_path` and
  the client samples it on the server clock. Up to 20 knots.
- `free_camera(p)` (`ControlObject::Observer`: flown and reported like the
  admin camera, without its orb or drop) and `orbit_point(p, at, distance)`
  (`ControlObject::Point`, `Vitals::camera_point`, 0.5 to 100 units).
- `on_observer(p, button)` with `Command::ObserverButton`: a spectator's
  fire, jump, jet and light keys (`Observer::onTrigger`, `serverCmdLight`).
  The client sends them while dead with the respawn held or under a rules
  camera; the host accepts them only then.
- `player(p).camera` (`getControlObject().getTransform()`).
- Rules cameras are only taken from the body or another rules camera, and
  `ControlPlayer` cannot leave a rule's path, free or point camera.
- One brick view for scripts: main's Trench `BrickInfo` and this branch's
  `BrickView` are one struct; `brick(id)` and `bricks(kind)` both give
  `turns`, `min` and `max`.

## Slayer port

- Spectating (`GameConnection::spectateInit` and the rest): out of lives,
  five seconds after death, orbit the next living player (or capture
  point, `setOrbitPointMode` 1.2 above it at 4.5); fire/jet step, jump or
  light change mode (orbit, free, auto); the auto camera glides from 5
  units behind (or in front, looking back) to 0.5 at 1 unit a second and
  120 degrees, skipping blocked glides (`containerRayCast` for players and
  bricks), retrying each second, and moving on at the end of each glide.
  Team-Only DeadCam, Enable Auto-Camera Mode, Spectate Capture Points.
- Fly-through (`FlyThroughCam.cs`): `/createFlyCam`, `/setKnot [speed type
  path]`, `/setJump`, `/testFlyCam`, `/deleteFlyCam` (owner or admin, up to
  `maxNodes` counting the creation point, which is not flown, as the
  original's `popFront`); each reset flies every member first and starts
  the countdown at the end, or during the countdown with Countdown During
  Fly-Thru; Rounds Between Fly-Thrus. All numbers pinned from the copy.
- Pins verified against the real Slayer 4.1.5 text: spectate 5000 ms,
  grace 1000 ms, 1.2 / 4.5, 2.8 / 2.2, 5 / 0.5, glide 1, FOV 120, retry
  1000, maxNodes 20, defaultSpeed 7, prefs true/false/false, 0/1/1.
- Fixed a duplicate `server/core/resources/datablocks.cs` key in Slayer's
  covers (the second hid the team-spawn pin).

## Protocol

Files in `crates/net/protocol-changes/`: `addon-settings`,
`rules-brick-inputs`, `respawn-held`, `archetype-uses-items`, `item-idle`,
`camera-path` (this step adds `ControlObject::Observer` / `Point`,
`Vitals::camera_point` and `Command::ObserverButton` to it).

## Tests

- `crates/sim/tests/package_camera_path.rs` (paths, palette and zone
  period, free/orbit cameras and spectator keys).
- `crates/addon-import/tests/slayer.rs`:
  `a_player_out_of_lives_spectates_and_changes_cameras`,
  `the_fly_through_camera_flies_everyone_before_the_round`.
- `crates/addon-import/tests/ports.rs` `slayer_ports_apply_with_their_rules`.
- `crates/client` `controls::rules_cameras_fly_freely_or_circle_their_point`.

## Not yet

Uniforms, team loadouts and player types, bots, `setTeamControlLocked`
and Slayer's other output events (needs a package outputs seam), the
StartFlyThrough output, saving a fly-through path with mini-game settings
(`.pathcam`).
