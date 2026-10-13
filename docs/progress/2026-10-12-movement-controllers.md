# 2026-10-12 Add-On movement controllers (v0.3.0 W3)

What changed: Add-Ons can give a body its own movement as a WebAssembly
controller that runs on the host (authority) and on the predicting client
(same bytes). It is policy over the v20 motor: once per 32 ms tick it may
change the velocity, take the jump button, keep 16 bytes of memory and set
an animation intent; the motor still does all collision. Design, interface,
determinism rules, gaps and next steps:
[docs/architecture/movement-controllers.md](../architecture/movement-controllers.md).

Decisions:
- Controller as a layer before the motor tick, not a replacement `step`:
  no second physics engine, Torque collision fidelity kept, Blockhead
  bit-identical (hook skipped without a controller).
- Fresh instance per step (purity enforced), NaN canonicalisation, fuel
  only (no epochs), only `bri.sweep` importable.
- Module bytes travel in the checkpoint's archetype table so the client
  runs exactly the host's bytes.

Evidence:
- `cargo test -p bri-controller`: host rules (memory, fuel, imports, NaN,
  sweeps, decline) and both samples built from their `.wat`.
- `cargo test -p bri-sim --test movement_controller`: kick predicted at
  150 ms each way with loss, correction 0; a client without the controller
  corrected by 2.9 units then bit-identical; Wall Kicker and Diver predicted
  at once; Blockhead beside a kicker bit-identical to a server without the
  Add-On.
- `cargo test -p bri-net --lib codec`: the table carries modules as binary
  and the client binds what arrives.
- `cargo test -p bri-controller --release step_cost -- --nocapture`: 16.3 µs
  a step.

Next: trust prompt entry for controllers, intent in remote poses, fault
reporting to the host console, triple jump, a Rust guest crate.
