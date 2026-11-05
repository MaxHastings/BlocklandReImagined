# 2026-11-04 Speedkart balance source review

The next playtest goal retains Maxwell's accepted preview.3 NPC implementation.
Only vehicle presentation/controls and the unaccepted cosmetic destruction
follow-up are being changed. Local heavy validation remains off while his
playtesting status is pending.

The supplied native Jeep declares pitch force 1000, roll force 900 and yaw force
600, but no `wheeled_flight`. These fields were already represented independently
in the native schema. Runtime applied all three only inside the optional
flight-settings branch, silently removing the controls from ordinary cars.

Read-only disassembly of the designated vanilla `blocklandv20.exe` confirms:

- Field registration at 0x570432 identifies maxForwardVel at datablock +0x388;
  0x570491/0x5704a8/0x5704c2 identify roll/yaw/pitch at +0x398/+0x39c/+0x3a0;
  0x5704f0 identifies stallSpeed at +0x3a8.
- The WheeledVehicleData constructor at 0x5715f0 sets maxForwardVel and
  stallSpeed to zero (stores at 0x571654 and 0x571694).
- WheeledVehicle::updateForces divides speed minus stallSpeed by maxForwardVel
  and clamps the result before applying pitch/yaw/roll (0x575425..0x575639).
  With a zero denominator, positive excess speed saturates to one; zero or
  negative excess returns zero through the comparison branches. Our helper
  previously returned zero for every zero-denominator case.

The correction applies the existing authored control torques independently of
wings, thrust or surfaces. Optional flight settings still own those aerodynamic
features. Zero-default control scaling is expressed as a finite piecewise result
rather than allowing NaN/infinity into physics. No vehicle is reclassified as
flying, no wire/save schema changes and no changing center of mass is introduced.
Host simulation and client prediction use this same vehicle implementation.

A synthetic ordinary wheeled vehicle now challenges pitch, yaw and roll with
both absent and explicitly zero flight settings, moving and at rest. Existing
stall, flying-car, skis and steering checks remain required. The first fixture
compile exposed a missing schema-type import; that import is corrected.
`cargo test --locked -p bri-vehicles --test flying_jeep` then passes six cases,
with five generated-content cases explicitly ignored. The client's targeted
`a_tool_hand_lets_go_of_the_seat_grip` unit check passes. Maxwell confirmed he
finished playtesting before these local checks started. Native-content, visual,
consumer/clippy checks and driving acceptance remain pending.
The subsequent run with `BRI_CONTENT` pointing at the isolated release inputs
and `--include-ignored` passes all 11 flying-Jeep cases, including generated
content. `cargo clippy --locked -p bri-vehicles --all-targets -- -D warnings`
also passes. These are vehicle-mechanism checks, not rendered hand/head or
interactive Speedkart acceptance.
Raw disassembly remains outside Git under the release-preparation evidence
directory's `speedkart-source-review`; the original executable was not run or
modified. Raw/decompiled source is not part of the repository or package.

Head behavior is being assessed separately from vehicle balance. The existing
audit's intentional stable mouse-steering camera is preserved. Read-only
Player::processTick comparison (0x5b2c81..0x5b2d7f) confirms the null-move split,
retained pitch and conditional strafe/free-look route. It does not establish a
missing automatic yaw twist when steering. Both rendered steering modes remain
to be checked before claiming that part complete.

The vehicles runtime change invalidates the current Add-On producer recipe.
The staged source originals remain available, but current importer receipts must
be regenerated before strict release packaging; prior receipts are not relabeled
as current. Private playable-folder delivery precedes merge-only full checks.
