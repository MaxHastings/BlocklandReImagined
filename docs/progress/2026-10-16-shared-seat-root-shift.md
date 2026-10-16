# 2026-10-16 Shared seat root shift

Root explicitly granted the first mechanical helper/client extraction after the
static seat scale checkpoint. Branch codex/seat-root-shift starts from immutable
5e728a4ac9d112f829e36bda4674ae574eab4a3f, preserving the same checkout/cache.
Only new content/src/body_pose.rs, its lib.rs export, avatar.rs delegation of the
existing seat_shift block, this entry and one seam row are owned. No host assets,
state/clocks, layer selection, fitting/IK, manifests or wire changes are granted
or made. The deliberately red authority investigation42275af9 stays separate.

seat_root_shift receives native Shape/Pose, current owned-clip status, optional
world hip and raw rider feet/rotation/scale. Raw frame components preserve the
existing finite/normalized quaternion, positive finite scale and finite feet
checks exactly, rather than broadening them with a generic matrix inverse.
Owned clips return zero root shift. Missing clips find the same sampled native
hip and apply the same inverse rotation/scale algebra. Invalid/absent targets
keep the existing origin fallback and strict <8 legacy bound; valid explicit
targets retain the existing larger-shift behavior. The active mounted sit/root/
dead guard remains at the same client caller. No policy or acceptance is reduced.

## Evidence

Cargo uses CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4;
CARGO_TARGET_DIR stays unset. All code/headless/offscreen; no gameplay controls
or visible game launch.

- Three pure native-rig regressions exercise explicit world-hip invariance under
  scale/rotation, owned-root preservation, legacy invalid-input behavior,
  exact shift-bound distinction and unavailable/nonfinite hips.
- `python tools/check.py quick --crates bri-content,bri-client`: all 4 steps
  passed in 335 seconds; client583/76 ignored and content95/0 (including all
  three new pure regressions), total678 passed, zero failures, 76 ignored.
  The existing app loading test emitted its >60-second status marker and then
  passed unchanged; client tests took145.65seconds. No watchdog/tolerance edits.
  Log: target/seat-root-shift-quick.log.
- `python tools/check.py focused --crates bri-content,bri-client --depth 0 --tests bri-client:seat_fallback_anchor --tests bri-client:seat_pose_composition --tests bri-client:avatar_animation_render --tests bri-client:vehicle_first_person`:
  all 10 steps pass in 364 seconds; both changed crates pass warnings-denied
  all-target Clippy. Summed executions: 695 passed, zero failures, 82 existing
  ignored. Existing anchor4/0, ready7/0, render3/3 ignored and stock camera3/3
  ignored all pass. Log: target/seat-root-shift-focused.log.
- No compile/test failures in this extraction. Final diff review and
  git diff --check pass; exactly the five assigned files are included. No
  full gate, main push or changes to the separate known-red diagnostic branch.
- Root relayed Halo's independent exact5e728 scale consumer result:84 checks,
  zero source-frame failures; fresh original Chief renders were still ongoing.
  That is consumer evidence for the preceding scale correction, not authority
  or full original fidelity acceptance.

## Remaining work

This is a shared basis prerequisite. It does not fix the recorded static host
body/seat/action/aim discrepancy or register a new authoritative origin policy.
Root reviews subsequent canonical mounted layer/fitting and host asset/state
reservations separately. Server owns playback state at existing thread producers;
client render completion must not mutate gameplay. Halo consumes exact source
scale/pose corrections, and AA/Halo retain their distinct original fire/aim
crosswalk requirements. No full gate, main push, release or human acceptance is
claimed. Active cache retained per root.
