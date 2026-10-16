# 2026-10-16 Camera snapshot freshness design and strict reproduction

Root assigned a read-only production investigation plus one new source-free
test, architecture note, own progress entry and seam row. Branch
`codex/camera-snapshot-design` starts at the clean Stage A checkpoint
`c8998ac1652b60129ba25c07de22c64404197122`. No production freshness changes are
authorized at this checkpoint. The intentionally red tests must remain outside
an integrated include-ignored candidate until corrected.

Read private immutable Source PR 30 handoff at
`0c970aba271039718a89e927aa70b19dac0cf1a0` and its actual callback source.
The public fixture independently authors invented Wasm and image geometry;
no original package code/assets were copied. Inspected real App camera/frame
order, ClientCode clock/input transfer, sandbox host imports/context, current
body/item posing and stock mode selection. The architecture note proposes
complete stock and resolved snapshot phases, explicit one-draw input/timing
ownership and exact mutable paths for root review. It flags prior addon_camera
mode feedback and drawn-controls versus live-controls differences.

## Evidence

Uses CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4,
CARGO_TARGET_DIR unset. No visible game or gameplay input.

- `cargo test -p bri-client --test camera_snapshot_freshness --locked`:
  52.51-second compile, 0.39-second tests; two passed, zero failed, two strict
  requirements explicitly ignored. Actual Wasm callbacks and host imports run;
  matching metadata without geometry cannot acquire. Log:
  target/camera-snapshot-targeted.log.
- Explicit `cargo test -p bri-client --test camera_snapshot_freshness --locked
  -- --ignored --nocapture`: exit 101; both strict tests failed in 0.03 seconds.
  Current look returned [0,0,-1] instead of independently normalized
  [0.118934356,-0.059467178,-0.9911197]. The lifecycle assertion recorded stale
  camera shots for all three transitions: death, unequip and first-person exit.
  Its checks also proved LOOK was released by the fresh frame and the subsequent
  draw had no camera. Log: target/camera-snapshot-strict.log. This failure is the
  retained requirement, not a passing integration claim.
- `python tools/check.py quick --crates bri-client`: two steps in 74 seconds;
  583 passed, zero failed, 76 ignored. Log: target/camera-snapshot-quick.log.
- `python tools/check.py focused --crates bri-client --depth 0
  --tests bri-client:camera_snapshot_freshness`: four steps in 78 seconds;
  585 passed, zero failed, 78 ignored, all-target Clippy `-D warnings` passed.
  Log: target/camera-snapshot-focused.log. This normal run does not claim the
  two deliberately ignored strict requirements pass.
- `git diff --check` passed. No production changes, full gate or push.

## Next

Root reviews the concrete phase contract and grants production paths before
implementation. The future correction must make strict same-draw assertions
green, verify real App mode/time/side-effect controls, then provide an exact SHA
for the original Source consumer rerun. Stage A is independently reviewable;
broader fitting and mounted authority remain open.
