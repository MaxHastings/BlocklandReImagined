# 2026-10-06 Seated eye and the small bug-hunt fixes

The non-bot known issues for v0.2.5 left open in
`2026-10-06-bug-pattern-hunt.md`. Each fix is its own commit with a test
that fails without it (checked by disabling the fix and rerunning).

## Seated and sitting players see from the sit pose

The host used the standing eye for a body on `/sit` or in a seat posed
`sit`, so every ray (shots, clicks, hammer, carrying, dropped items, the
F8 camera, Add-On aim) started a full standing height above the seat. The
client's brick ghost ray and name tags did the same.

- Read from the generated rig (`content/avatar-rig-001/rig.json`, m.dts):
  the `Eye` node's last `sit` key is `(0.00217, 1.63535, -0.01381)` under
  `start`, with `main` and `start` unanimated. In the body frame that is
  1.635 up and 0.262 behind the box centre (standing is 2.156 up, 0.141
  ahead; the same transform reproduces both baked values).
- `PlayerTuning` gains `sit_eye` and `sit_eye_forward` (scaled with the
  body; the horse and player-type vehicles keep their standing eye).
  `PlayerState::posed_eye` and `Archetypes::posed_eye` take a `seated` flag.
- The motor `Player` carries `seated`; `Session::sync_seated` sets it each
  tick from `/sit`, a vehicle seat's `pose` or a mount point's `pose`
  (`sit_posed`), and `/sit` sets it at once. `Player::eye()` then gives
  the sit eye to every existing caller. The sit owns the node over the
  crouch thread, so crouching while seated keeps the sit eye, as the
  client's `sit_first_person` test already shows for the camera.
- Client: `app::mounts::sit_posed` (the rule the avatar already used to
  play `sit`, now shared) feeds the building controller's eye and the name
  tags.
- Wire: `Checkpoint::archetypes` carries the new fields
  (`crates/net/protocol-changes/sit-pose-eye.md`).
- Test: `session::a_sitting_player_sees_from_the_sit_pose_eye` (the F8
  camera starts at feet + 1.635).

## Smaller findings, each checked against v20's core scripts

From `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`:

- **Hammer and stacks:** the hammer now breaks a brick whose stack
  (`Simulation::stack_owner`) is the swinger's, without the builder's
  trust, as `hammerImage::onHitObject` does. Test
  `tools::the_hammer_breaks_others_bricks_built_on_your_stack`.
- **Bricks in hand refused by an Add-On:** the policy's reason is now a
  centre print, beside the put-away notice. Test in
  `carry_rules::a_rule_hears_the_fire_button_vetoes_tools_and_takes_a_rider_off_a_vehicle`.
- **Riding:** `WheeledVehicleData::onCollision` asks ride trust only when
  the spawn brick's builder is on the server (`findClientByBL_ID`), so an
  absent builder's vehicle is anyone's outside mini-games, and it
  centre-prints why it refuses: "<builder> does not trust you enough to
  ride." or the mini-game messages. `can_ride` follows that; a refused
  landing on a spawn-brick vehicle prints once per landing, and the click
  to board carries the same text. Test
  `vehicles::an_untrusted_rider_is_told_why_and_an_absent_builders_jeep_is_anyones`.
- **Events edits:** `serverCmdAddEvent` takes one line at a time and drops
  one it cannot read. A wrench events send now leaves out each bad row and
  says which line and why in chat; too many rows still refuse the edit, and
  the host's `edit_brick` still refuses a bad row. `tools::event_binding_...`
  and `hardening_session::event_rows_are_bounded_and_catalog_checked` were
  updated to the new rule (no bad row reaches the brick either way).

## Commands

`cargo test -p bri-sim --test session --test tools --test carry_rules
--test vehicles --test hardening_session` (synthetic variants): all pass.
`cargo check -p bri-client --tests`,
`cargo clippy -p bri-motor -p bri-vehicles -p bri-sim --tests -- -D warnings`:
clean. Then `python tools/gate.py --push`.

## Not done

Looping weapon sounds still place at the standing eye
(`app/frame.rs`), and an Add-On's client-side view of others' eyes and an
orbit camera's focus use the standing eye; none of these aims anything.
