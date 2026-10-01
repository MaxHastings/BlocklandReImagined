# One orbit camera for Throwing and Slayer's watch

Thread: Capture the Flag (Slayer). Asked by the coordinator: one seam, with
Throwing's struggle left as it is.

## What changed

v20 has one camera for both: `%client.camera.setOrbitMode(...)` then
`setControlObject(%client.camera)`. Two seams had grown for it: Throwing's
`orbit_camera` (`ControlObject::Orbit`, the click still reaching
`on_activate`) and the rules' `watch` (`ControlObject::Spy`, the admin's
spy camera, borrowed). Now there is one.

- `ops::OrbitBody` says what the body does under the orbit: `Acts`
  (Throwing's held player; the default) or `Frozen` (`watch`: no actions,
  every key to `on_observer`). `Op::OrbitCamera` carries it; `Op::Watch` is
  gone.
- `ControlObject::Orbit` carries `body`. A frozen orbit is a rules camera
  (`rules_camera()`), so the sim refuses actions and `ControlPlayer` and
  routes keys to `on_observer`; the client treats it as spectating. An
  acting one keeps sending the click as `Activate`.
- Script: `watch(p, t)` is the frozen orbit 8 units out (the corpse
  camera's distance); `watch(p, p)` on a dead player is the corpse camera.
  `orbit_camera(..., distance, "frozen")` gives the same from any Add-On.
  `orbit_camera(p, ())` ends only an acting orbit and `watch(p, ())` /
  `orbit_camera(p, (), "frozen")` any rules camera, so Throwing and Slayer
  never end each other's camera; each also refuses to lay its kind over
  the other's.
- `Spy` is the admin's spy only. An admin's spying body still takes no
  actions (`watching`).
- Fixed on the way: a living player under Slayer's round-end camera could
  click back to their body (it was `Corpse`, which `ControlPlayer`
  allowed). It is now a frozen orbit around themselves and stays until the
  reset.

Protocol: `crates/net/protocol-changes/orbit-camera-body.md`.

## Evidence

- `sim/tests/script_api.rs`
  `an_orbit_camera_either_lets_the_body_act_or_freezes_it`: one test for
  each kind (acting: click to `on_activate`, spectator keys refused;
  frozen: keys to `on_observer`, click refused, no clicking out; neither
  release ends the other kind; a bad body name is refused; the dead get the
  frozen kind and their own corpse camera).
- Throwing's `ports.rs` test unchanged apart from naming `body: Acts`.
- Slayer's spectate and round-end tests assert the frozen orbit, and that
  a frozen player cannot click out.
- `cargo clippy --workspace --all-targets --locked -D warnings` clean;
  `bri-sim`, `bri-package-runtime`, `bri-addon-import` (slayer, ports) and
  the client lib tests pass.
