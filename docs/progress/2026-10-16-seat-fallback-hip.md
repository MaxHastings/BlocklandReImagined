# 2026-10-16 Explicit fallback seat hip

Root assigned codex/seat-fallback-hip from immutable
802bd76c2db3e310946cd21289d4173a450a4fb9 in the same clean checkout/cache.
Reserved schema/client seat-frame and avatar paths, four constructor defaults,
new synthetic test, relevant modding section, one new seam row and this entry.
Root additionally granted app/load.rs solely for the new Mounts map constructor,
and app/session.rs solely for the matching disconnect cache clear. The synthetic
App disconnect regression stays in the assigned frame.rs test module.
No original point is invented or bound; Halo owns source recovery and exact-SHA
consumer comparison. Root owns integration, main, gates and publication.

Optional rider_fallback_hip uses unscaled native seat-node coordinates (+Y up,
forward -Z), finite components within +/-64 and a selected rider_pose. Existing
static seat position/rotation is unchanged, with vehicle scale applied once to
the point. Full animated mount-node matrices already include scale. Target
calculation precedes passenger turn. Missing authored clips convert the world
target through actual rider inverse rotation/body scale before subtracting the
current sampled hip; owned clips preserve their root. Absent policy paths remain
unchanged; malformed runtime target data falls back conservatively.

## Evidence

Cargo uses CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4;
CARGO_TARGET_DIR stays unset. No interactive controls or visible game launch.

- `cargo test -p bri-client --test seat_fallback_anchor --locked`: all four
  synthetic integration cases pass, zero ignored; compile 117 seconds and tests
  0.05 seconds. Covers two sampled hip heights, nonunit rider/vehicle scales,
  tilt/turn/passenger turn, owned/missing clip and anchor, nonfinite/runtime
  matrix guards, schema validation and actual VehicleAssets/ClientVehicles
  static/animated point helper. Log: target/seat-fallback-targeted.log.
- Added the granted disconnect clear and actual synthetic App reset regression
  after that frozen run, before quick/focused checks. No build/source overlap.
- `python tools/check.py quick --crates bri-vehicles,bri-client,bri-vehicles-import,bri-sim,bri-addon-import`:
  10 steps passed in 267 seconds, 1,062 passed, zero failures, 83 existing ignored.
  Client 581/76 includes the synthetic App disconnect regression; sim 403/4,
  addon-import 58, vehicles 18/3 and vehicles-import 2. No production compile/test
  failures. Log: target/seat-fallback-quick.log.
- Root approved `python tools/check.py focused --crates bri-vehicles,bri-client,bri-vehicles-import,bri-sim,bri-addon-import --depth 0 --tests bri-client:seat_fallback_anchor --tests bri-client:seat_pose_composition --tests bri-client:avatar_animation_render --tests bri-client:vehicle_first_person`:
  **all 19 steps passed in 787 seconds**, all five changed crates passed
  warnings-denied all-target Clippy. Summed test executions: **1972 passed,
  zero failures, 274 existing ignored**. Anchor 4/0, previous ready-composition
  7/0, avatar render 3/3 ignored and vehicle first-person 3/3 ignored. Existing
  generated-content cases stay ignored in this scoped tier; no include-ignored
  gate or original-content acceptance is inferred. Log:
  target/seat-fallback-focused.log.
- No compile/test failures occurred in this slice. Initial read-only searches used
  two nonexistent fixture paths and a Windows directory glob; corrected them with
  rg --files and the actual testing.rs re-exports before using fixture APIs.
- Final scope is 15 assigned files, including constructor-only app/load.rs and
  disconnect-clear-only app/session.rs. Diff review and git diff --check pass.
  No authoritative server origin, movement, material, UI, manifest or wire edits.

Native source-free checks establish point-transform and pose composition math,
loaded VehicleAssets/ClientVehicles point selection, schema rejection and actual
App disconnect cache clearing. Per-frame cache rebuild, alive/mounted guards and
input propagation were reviewed in frame.rs. This does not claim an end-to-end
original mounted Session fire/obstruction result; that remains the next owned
mechanism/consumer work.

## Remaining closure

Halo must recover and bind exact original hip/seat evidence, consume the committed
SHA and compare actual fallback bodies/owned clips. Root reserves authoritative
mounted pose and Session obstruction separately. Scoped synthetic success does
not establish original aim, full integration or release acceptance. Keep cache
for sequential assigned work; no main push or full gate in this lane.
