# 2026-10-12 v0.3.0 W2: body sockets, animation roles and muzzle shots

Branch `claude/v030-grips-sockets-8bh2qn`, into `v0.3.0-experimental`. W2 of
the v0.3.0 plan: any independently authored body holds any weapon in its own
hand, sits in any seat and climbs out, plays its own clips with safe
fallbacks, and every shot leaves the muzzle.

## What changed

- **Sockets** (`crates/content/src/body.rs`, `Archetype::sockets`): an
  archetype names its `right_hand`, `left_hand`, `eye` and `seat`, each a
  model node, an offset from the feet, or both. Unset sockets are the
  Blockhead's `Mount0`, `Mount1` and `Eye`, so v20 bodies are unchanged. An
  offset-only `eye` also sets the body's eye heights unless `movement` does.
- **Animation roles** (`Look::animations`, `RoleClips`): 19 roles (`idle`,
  `run`, `back`, `jump`, `sit`, `aim_right` and the rest) map to the body's
  own clips. An unmapped role asks for the Blockhead's clip name, then a
  fallback chain (`back` runs `run` reversed, `fall` plays `jump`, `sit`
  plays `idle`); a role with nothing leaves that layer in the bind pose. The
  load-time check that refused a body without all seven Blockhead clips is
  gone; a missing action clip is skipped, not an error.
- **Diagnostics**: the client names what a body lacks once per (archetype,
  model) in the Add-On faults log, for example "no `sit` animation; it plays
  its `idle` instead" or "its model has no `Hand_R` node for the
  `right_hand` socket, so items held in that hand are hidden".
- **Box bodies** (the Runner, the Commando) hold items at their hand
  socket, see from their eye socket, and sit: the body lowers by the seat
  socket, boxes below the hip fold forward, and the body tilts with the seat
  (`crates/client/src/packages.rs` `BoxPose`).
- **Muzzle shots, every gun** (Max chose "All guns now"): the host learns
  each image's barrel (offset, rotation, mount point to muzzle point) and
  each body's hand frame, and fires from the muzzle in that hand, pitched
  with the aim (`crates/sim/src/session/grips.rs`,
  `crates/weapons/src/muzzle.rs`). `correctMuzzleVector` shots converge on
  what the crosshair hits. A muzzle inside a wall (the sweep from the eye
  hits anything) fires from the eye, and a body or image the host has no
  grip for fires from the eye with a one-time warning.
- **A chosen game mode always runs**: `host_setup::hosted` dropped a mode's
  catalog when no package had a behaviour or world, so starting the
  Playground from Start Game hosted plain v20 Blockheads with no mini-game.
  The net and sim tests set the mode directly and never saw it.
- Wire: `crates/net/protocol-changes/archetype-sockets-and-animations.md`.
- Docs: `docs/modding/content-kinds.md` (sockets and animations),
  `docs/audits/interop-matrix.md` (the four W2 rows).

## Evidence

- `crates/content/src/body.rs` unit tests: role fallbacks, reversal,
  validation.
- `crates/client/src/avatar.rs`:
  `a_body_with_its_own_nodes_and_two_clips_holds_sits_and_names_what_it_lacks`,
  `a_body_with_no_clips_stands_in_its_bind_pose_and_still_holds`,
  `a_box_body_holds_at_its_own_hand_and_sits_with_folded_legs`.
- `crates/sim/tests/playground.rs`:
  `the_runners_shot_leaves_the_carbines_muzzle_in_its_hand_and_meets_the_crosshair`
  (muzzle at feet + (0.5, 0.88, -1.02), miss under 0.05),
  `a_carbine_pressed_into_a_wall_fires_from_the_eye`,
  `a_blockhead_bot_fires_the_carbine_from_its_own_hand`.
- `crates/client/tests/body_attachments_render.rs` (offscreen, lavapipe):
  the whole App hosts the Playground mode, the Runner holds the Carbine at
  (0.5, 0.74, -0.14) in its body frame (its `right_hand` socket), boards the
  Dune Buggy, sits (hand at 0.39, the `sit` action), climbs out and stands
  with the Carbine back at 0.74. Renders: `runner-standing.png`,
  `runner-seated.png`, `runner-climbed-out.png`.
- `python tools/check.py focused --base origin/v0.3.0-experimental`: fmt and
  clippy `-D warnings` clean for every changed crate, and every dependent
  crate's tests pass (bri-sim's and bri-net's Playground tests included).
- App tests touching bodies, mounts and held items pass: `app_item_render`,
  `held_items_render`, `horse_riding_render`, `remote_poses`,
  `sit_first_person`, `view_jitter`, `world_items`, `add_on_join`,
  `vehicle_first_person`. One `vehicle_first_person` case
  (`invert_mouse_in_vehicles_…`) timed out waiting for the host while other
  suites ran, and passed when retried alone (89 s).

## Not verified here

- Content-backed (`--include-ignored`) tests need the generated v20 content,
  which this container does not have; stock v20 weapons firing from the
  muzzle is covered by unit and synthetic tests only. The gate runs them.
- Bots still plan shots from the eye; `correctMuzzleVector` convergence
  keeps their shots on the aim point, but bot hit rates on v20 content were
  not measured here.
- `a_carbine_pressed_into_a_wall_fires_from_the_eye` also passes without the
  new wall guard, because the first-person eye sweep already catches that
  case; the guard covers third-person and bots.

## Next

- Wire the movement controller's animation intent (PR #107) to these role
  names when it reaches remote poses.
