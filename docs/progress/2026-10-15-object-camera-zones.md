# 2026-10-15 Object cameras and object zones

Max asked for a game where you are a golf ball rather than a character
("Golf With Your Friends"), and for whatever serves more kinds of games
long term. A ball is already a vehicle of the Ball family (the Steel Ball
Kit), and rules already push objects, so two engine seams were missing:

- **`orbit_object(p, object, distance)`**: a rule's point camera that
  follows an object. `OrbitPoint` gains `object`; the host moves the point
  to the object's middle every tick (`Session::step_object_cameras`), and
  the client circles the vehicle's drawn (interpolated) box middle, so the
  camera does not step at snapshot rate. If the object goes, the camera
  stays where it was. Wire change: `protocol-changes/orbit-object-camera.md`.
- **Zones that notice objects**: `"objects": true` on a zone also checks
  every vehicle's world box (`Session::object_boxes`, rotation included)
  and calls `on_zone_object(object, brick, event)` with `enter`, `leave`
  and `tick`. Players still call `on_zone` as before.

Neither names golf: a race line, a pressure plate or a ball goal use the
same calls.

Evidence: `cargo test -p bri-sim --test object_camera_and_zones` (a ball
dropped on a cup enters the zone once, the camera falls with it, a push
sends it out and the zone hears it leave while the camera follows).

Next: a minimal golf Add-On in project files (a ball per player, charge and
release shots with `on_trigger` and `push`, strokes and a cup zone) to
prove the seams in play. A rolling-body player visual and a sphere sweep
for controllers stay open and unneeded for this route.
