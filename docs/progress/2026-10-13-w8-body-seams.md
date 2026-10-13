# 2026-10-13 W8: a body that faces where it runs, and holds at its own hands

Branch `w8-bodies`, the second W8 slice (per-body seams). Max's Mario
playtest: "the running and animation looked off ... and he holds weapons
wrong". Mario faced the camera while running across it (strafing and
moonwalking), and an imported body's shots left its eye.

## What changed

- **A controller turns the body.** A movement controller sets out flag 4
  and a heading at record offset 92 to face the body its own way, turning
  it at whatever rate it chooses; the camera and aim stay the look's. A
  tick without flag 4 hands the body back to the look.
  `PlayerState::body_yaw` is what every view draws (rigged and box bodies);
  others receive the heading with the pose (`RemotePose::controller`, one
  protocol change file, `controller-attack-and-heading.md`), never the
  controller's memory, and the client blends it between poses.
- **More for a controller to read and say.** The attack button (in flag 64,
  `MoveInput::attack`), the archetype's gravity (offset 96), "no motor
  gravity this tick" (out flag 2), `bri.sweep_from` (a sweep from an offset
  for ledge grabs), and air drag and speed resistance allowed at 0.
- **Intents look and sound like something.** An archetype's `intent_clips`
  plays the model's clip for each controller intent in place of run, jump
  and fall; `intent_sounds` plays a sound where the body is when the
  controller changes to an intent. Both were engine fields an Add-On's
  archetype JSON could not set until this change.
- **Run clips keep pace with the ground.** v20's `Player::updateAnimation`
  plays run, back and side at `|v . dir| / speed`, the speed from the
  sequence's ground transform, capped at `maxTimeScale` (1.5). The client
  played every clip at 1x. It now scales them (`BodyKind::time_scale`),
  by the clip's own ground motion or, for clips without one, the
  archetype's `ground_speeds`; `max_time_scale` overrides the cap. The
  Blockhead's clips change only if its converted sequences carry ground
  motion. Test: `locomotion_clips_play_at_the_speed_the_body_moves`.
- **Shots leave an imported body's hand.** The host only knew the
  Blockhead's hand nodes. It now reads each enabled Add-On body's hand
  nodes from its package (`content_identity::package_body_grips`), in its
  own raised-arm pose (`body_grips`, with the archetype's role clips), on
  listen and dedicated hosts.

- **The buggy's gun is the Warthog's chaingun.** Vehicle guns could only
  fire one shot a pull. `weapon.automatic` fires while held with a rate
  and a spread that rise as it is held (Halo's `weap` numbers), and
  `weapon.recoil` sets the per-shot kick that was fixed at 5. The buggy
  takes the chaingun's tags (`docs/audits/vehicle-handling.md`); its rounds
  push a tenth as hard as the old bolt so a burst does not fling a target.
  Test: `the_gunners_chaingun_spins_up_spreads_and_does_not_kick`.

## What the engine did not own

Checked against #109 (grips, sockets, roles): the client already draws a
held item at the socket node. Mario's grip looked wrong because the item
takes the socket node's own axes (a hand bone points it along the bone)
and his rig has no `aim_right` clip, so the arm hangs. Both are in the
Mario model; the Mario controller thread was told, and
`docs/modding/content-kinds.md` now says how to author the sockets.

## Evidence

- `crates/controller/tests/host.rs`
  `a_controller_reads_attack_and_gravity_and_turns_the_body`: the record's
  new words in and out, and the heading lapsing without flag 4.
- `crates/net/src/stream.rs`
  `a_controllers_heading_and_intent_reach_others_but_not_its_memory`: the
  heading and intent replicate in at most 5 more bytes; the memory does not.
- `crates/client/tests/gltf_body.rs`: the host finds an imported glTF
  body's right hand with the arm raised.

## Next

Hold-to-fire with spin-up and spread for vehicle guns (the Warthog chain
gun), fall damage per body, then sound buses and impact decals.
