# 2026-10-16 Static seat origin scale

Root assigned the narrow static seat-frame correction after the read-only
mounted-pose investigation42275af9. That diagnostic branch deliberately stays
separate because its two red parity tests must not enter an integrated gate.
This branch codex/seat-origin-scale starts from immutable19db868d, keeping the
validated seat/anchor chain and active cache. Allowed production edit is only
client/vehicles.rs::seat_transform and its directly associated tests; authoritative
vehicles/world.rs was read-only reference. Root owns integration and gates.

Halo private156dce182ef66f801613414583444c9f663e8d4c demonstrates the defect at
exact19db: 84 actual-current-frame comparisons pass, while 24 independent source
frame comparisons fail, max1.7608795 native units. The source-owned versus native
foreign-body fallback policies remain distinct; no private point is changed or
copied into public fixtures. The delivered preview is unchanged.

The client previously transformed an unscaled local seat translation. Existing
authority scales it (including the turret-rotated pivot displacement) before
multiplying by the body pose. Client now scales only the complete local translation
after its existing pivot rotation. Rotation stays unit, default scale1 is unchanged,
and the app's full animated mount-node override still carries its scale once.
There are no anchor metadata, body root, animation, hand/eye/aim policy, gameplay
origin, movement, wire or manifest changes. Seat-derived eye/anchor positions now
inherit the correct origin rather than compensating with offsets.

## Evidence

Cargo uses CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4;
CARGO_TARGET_DIR remains unset. No visible game or interactive controls.

- New native regression fails before production edit at scale0.5: invented client
  seat [4.0411057,5.017144,5.459909] versus actual authoritative
  [3.5205529,4.508572,5.2299547]. The regression loads real VehicleAssets and reads
  VehiclesWorld's actual snapshot seat pose. Log: target/seat-origin-scale-red.log.
- After the narrow translation correction, both new seat_scale unit tests pass
  (26.95-second compile, tests0.09seconds): actual authoritative scaled static
  seats, 24 owned/fallback hip combinations, and rotated turret/broken fallback
  at scales .5/1/2. Log: target/seat-origin-scale-targeted.log.
- Root additionally granted only the inaccurate static-transform sentences in
  docs/modding/seat-pose-composition.md; updated to explain the authority-matching
  scaled translation and animated matrix scale applied once.
- `python tools/check.py quick --crates bri-client`: 2 steps in 45 seconds,
  583 passed, zero failed, 76 existing ignored. Log: target/seat-origin-scale-quick.log.
- `python tools/check.py focused --crates bri-client --depth 0 --tests bri-client:seat_fallback_anchor --tests bri-client:seat_pose_composition --tests bri-client:avatar_animation_render --tests bri-client:vehicle_first_person`:
  all 7 steps passed in 176 seconds, warnings-denied all-target Clippy clean;
  600 passed, zero failed, 82 existing ignored. Existing anchor4/0, ready7/0,
  render3/3 ignored and camera3/3 ignored pass. The new red-before regression
  remains enabled and passes. Log: target/seat-origin-scale-focused.log.
- Final diff review retains only vehicles.rs (three production lines and its
  two regression tests), the granted narrow modding correction, this entry and
  one new seam row. git diff --check passes. Full original/gate coverage remains
  with root and Halo; the intentionally red authority diagnostic is not in this
  branch.

Root/Halo must revalidate the exact committed correction against the preserved
private independent source comparison. Shared authoritative body pose/clocks and
per-game firing/obstruction semantics remain separate, actively owned work.
No original fidelity, full gate/main landing or release acceptance is claimed.
