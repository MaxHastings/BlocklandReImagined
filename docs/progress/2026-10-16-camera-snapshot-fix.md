# 2026-10-16 Current stock context before Add-On camera selection

Root reviewed design checkpoint `936f84a4d8d3a87046705624041963eee5af6767`
(based on clean Stage A `c8998ac1652b60129ba25c07de22c64404197122`) and granted
implementation on `codex/camera-snapshot-fix`: client app/render.rs,
client_code.rs, client-sandbox host.rs, directly relevant tests and own docs.
App frame.rs/view.rs remain production read-only. No manifests, imports,
capabilities, wire, authoritative aim/movement or original policy changes.

`begin_camera_frame` prepares one draw time/input batch, bound to its existing
per-control owners, and supplies complete current stock World/View to a
non-executing sandbox context refresh. Camera callbacks then select that draw's
shot. The ordinary later frame callback reuses the prepared timing/input and
receives a distinct resolved-environment World. World actor/vehicle/life/held
data comes from the actual current presentation; current selectors are not
patched onto old transforms. Image meshes remain real immutable model-local
geometry. Existing environment resolution/application is shared between phases;
no extra culling, upload, native animation or physics step occurs.

View mode describes underlying stock controls/current life independently of
the previous Add-On camera flag. FOV/aim/sway use drawn controls consistently.
Existing renderer visibility behavior, body/item stepping and stock-synced held
placement remain in their existing owners; no final-shot-relative item rewrite.
Prepared input is intersected with retained ownership after camera callbacks;
new owners cannot inherit the batch. Replacement preparation, stop and device
loss drop abandoned input. Frame-only callers prepare/dispatch once.

## Evidence and corrections

All cargo commands use CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4; CARGO_TARGET_DIR remains unset. No visible game or interactive
playtest. Root's gate performance diagnostic received its requested quiet build
window; its candidate/gate/release is independently root-owned.

- Initial corrected-boundary `cargo test -p bri-client --test
  camera_snapshot_freshness --locked`: 45.01-second compile, 0.12-second tests;
  four passed, zero failed/ignored. Both previously strict red assertions are
  enabled and pass. Log: target/camera-snapshot-fix-first.log.
- Expanded target adds abandoned draw/release/trap controls and a real hosted
  App/offscreen test. Its first run passed five boundary tests but rejected the
  App fixture package directory `.` under existing package validation. Replaced
  it with an ordinary `camera` directory. Log:
  target/camera-snapshot-fix-app-first.log. Also use the actual FOV preference
  rather than a direct Controls value that App's normal tick would overwrite.
- Further App fixture setup failures were retained: the second run needed the
  full package manifest schema/API/license/provides fields; the third selected
  wrench slot 1 while waiting for hammer slot 0; the fourth showed session
  entry correctly replaces an `enable_packages`-only list with the content
  root's actual package list. The fixture now installs its invented package in
  the temporary synthetic root's normal packages.json, using the real load and
  start paths. Logs: target/camera-snapshot-fix-app-second.log through
  target/camera-snapshot-fix-app-fourth.log. No production workaround was made.
- The fifth run passed six boundary controls and reached actual App callback
  count, unchanged Eye/held and consecutive first-person assertions, then
  failed because the fixture changed FOV mid-test while the normal glide was
  active. Set the user preference before hosting instead. Log:
  target/camera-snapshot-fix-app-fifth.log. Added actual same-draw death after
  positive re-acquisition, alongside mode/unequip transitions.
- Corrected targeted actual App/offscreen test passed: 29.58-second compile,
  51.47-second test, one passed/zero failed/ignored (six boundary cases filtered).
  Log: target/camera-snapshot-fix-app-sixth.log. Actual normal package loading,
  hosted native actor and posed hammer geometry are used. Consecutive renders
  without a tick increment the real Wasm frame count exactly once with dt=0;
  a controlled .016 tick advances time exactly once. The original Eye and held
  transform remain unchanged across those draws; the user's FOV73 remains.
  Captured look affects the same rendered draw without changing body controls.
  Third-person exit/reentry, unequip/reacquire and current local death all
  suppress/reactivate the real callback in the appropriate same draw. Cosmetic
  fault count remains zero. No original Source profile or gameplay-aim claim.
- `python tools/check.py quick --crates bri-client,bri-client-sandbox`:
  four steps in 98 seconds; 598 passed, zero failed, 76 ignored. The new
  prepared/frame-only/empty-path clock test passes, including the existing
  .25-second elapsed clamp. Log: target/camera-snapshot-fix-quick.log.
- Final review caught resolved environment being populated even when no running
  package requests a World snapshot. Guarded that update with the existing
  reads_world eligibility, retaining the old empty/default path. This adjustment
  happened after the initial quick/Clippy steps, before focused client render
  targets compiled; supplemental final quick/Clippy evidence is required below.
- `python tools/check.py focused --crates bri-client,bri-client-sandbox --depth 0
  --tests bri-client:camera_snapshot_freshness --tests bri-client:vehicle_first_person
  --tests bri-client:held_items_render`: nine steps in 385 seconds; 664 passed,
  zero failed, 81 ignored. Both all-target Clippy checks passed. Full sandbox
  tests include avatar/physics/budgets/shaders/showcase controls; final adapter
  render targets passed seven freshness cases without ignores, three synthetic
  vehicle camera cases and one synthetic held-item render case. Ignores are 76
  existing client units, one existing sandbox case and four generated-content
  render variants. Log: target/camera-snapshot-fix-focused.log.
- Final `cargo fmt --package bri-client --package bri-client-sandbox --check`
  and `cargo clippy -p bri-client --all-targets --locked -- -D warnings` both
  pass after the reads_world guard; Clippy took 26.28 seconds. Logs:
  target/camera-snapshot-fix-final-fmt.log and
  target/camera-snapshot-fix-final-clippy.log.
- Supplemental `python tools/check.py quick --crates bri-client,bri-client-sandbox`
  passes after the final guard: four steps in 128 seconds; 598 passed, zero
  failed, 76 ignored. Log: target/camera-snapshot-fix-final-quick.log.
  `git diff --check` passes. Root's
  unchanged integrated gate owns performance measurements; no worker gate or
  push is run. All strict freshness tests remain enabled in the final chain.

## Remaining acceptance

Root integrates and gates the atomic corrected chain. The original Source
consumer must rerun against the immutable fix using the new actual App order;
its historical stale receipt remains retained. Original Source camera/movement
laws and mounted authority/fitting work are not closed by snapshot freshness.
