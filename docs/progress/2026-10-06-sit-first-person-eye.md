# 2026-10-06 First-person eye follows the sit pose

Max reported that after `/sit` the first-person camera stayed at standing
eye height, and that crouching while sitting crouched the first-person view
while the third-person body only raised its arms.

## Cause

Riders already saw from their posed `Eye` node (`App::rider_eye`, v20's
`Player::getCameraTransform` at `pos` 0, 0x5ab7d0). On foot the camera used
the motion system's eye: standing height eased by the crouch thread
(`Motion::local_eye`), which knows nothing about the held `sit` action. So the
first-person view ignored the sit, and crouching dropped it by the crouch
eye keys while the drawn body, whose `sit` owns the Eye node over the crouch
thread, stayed sitting with its arms raised.

## Change

`App::rider_eye` is now `App::posed_eye` and covers a player sitting on foot
(`Vitals::sitting`) as well as riders: the first-person eye is the drawn
body's `Eye` node, through every thread it plays. First and third person
now show the same body: the view drops with the sit (v20 rig: 2.16 to 1.65
over the feet, and 0.4 back), crouching while sitting keeps the eye where the
sitting body's eye is while the arms come up, and moving (which ends the sit)
hands the eye back to the motion system at standing height. Vehicle and
player-mount seats are unchanged (same function).

## Evidence

- New `crates/client/tests/sit_first_person.rs`: a hosted game, `/sit`, crouch
  while sitting, then walk. Checks the first-person eye equals the drawn
  body's `Eye` node while sitting and crouch-sitting, and returns to standing
  height. `cargo test -p bri-client --test sit_first_person --
  --include-ignored` (BRI_CONTENT on the generated v20 content): both
  variants pass. With the `vitals.sitting` condition removed the synthetic
  variant fails ("sitting first-person eye ... is not the sitting body's Eye
  node").
- An offscreen render on the v20 content (slate, a brick column ahead,
  temporary test not committed) showed the first-person view lower while
  sitting, unchanged by crouch while sitting, and the third-person body
  sitting with raised arms when crouching.

## Not changed

The server still fires a sitting player's shots from the standing eye
(`peer.player.eye()` in `session/weapons.rs`); it has no posed rig. Riders
share that gap.
