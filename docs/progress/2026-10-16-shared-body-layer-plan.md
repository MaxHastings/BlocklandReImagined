# 2026-10-16 Shared native body layer planning

Root granted Stage A after design checkpoint20a84b1. Branch codex/body-layer-plan
starts from clean immutable d614f46d367d9dc92e15bbc44d6359550df10760, keeping
intentionally red authority diagnostics separate. Production reservation is only
content/src/body_pose.rs plan_body_layers/plain types and client/avatar.rs layer
block delegation. Associated tests, architecture/progress and one seam row are
included; no host registration/state, fitting/IK, new clocks/tweens, vehicle
policy dependency, manifests or wire changes.

The pure plan borrows Animation references only from the immutable Rig, owns its
acted/unacted vectors and skipped names, and returns each transition boundary
separately. Action inputs are a borrowed slice; the Avatar adapter preserves its
existing body0/body1/action/gesture order without introducing a four-action cap.
Native NodeTrack masks and exact staged absolute/priority sorting plus final
stable additive-only partition remain unchanged. Existing active-owned-seat and
whole-image ready eligibility stay at the client caller.

Mode, direction, phase, elapsed clamp, CrouchThread mutation, Channels/snapshot
state, transition expiry/weight, lifecycle invalidation and actual native sampling
remain Avatar-owned. The existing look_position implementation/callers compute
look_phase; the planner only selects SeatAim/Look and applies the selected clip
duration. Reference-tool fallback, support liveness, hand fitting and world seat
IK remain in their existing paths. This is not a new animation framework or a
resolution of original Halo/AA grid/rate/interrupt laws.

## Evidence

Cargo uses CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4;
CARGO_TARGET_DIR remains unset. Code/headless/offscreen only; no visible game or
interactive controls.

- Before production edits, `cargo test -p bri-client --test body_layer_plan --locked`
  passed both new native AvatarMesh/sampler regressions (21.27second compile,
  .08second tests) against the legacy layer block. This includes four absolute/
  additive base/look combinations, sparse replacement channels, exact action
  order, both acted/unacted transition snapshots and boundaries; noncentral/
  clamped look phases and differing Look2s/SeatAim4s durations, including invalid
  seat aim fallback. Log: target/body-layer-plan-legacy.log. An unused Mat4 test
  import warned on that first compile; removed before scoped validation.
- First delegated build caught a client closure return-lifetime mismatch for
  borrowed ActionRef names. Replaced the generic closure with mapping over the
  existing input-reference array; action order and ownership are unchanged.
  Log: target/body-layer-plan-targeted-first.log.
- Added three pure planner tests: exact stable order/both boundaries across
  absolute/additive base/look, actual RoleClips fallback and selected overlay
  durations/ready omission, and owned diagnostics/rig-only return lifetime with
  loop versus additive timing and seven input actions (no new four-action cap).
- Corrected frozen `cargo test -p bri-client --test body_layer_plan --locked`
  passes the same two legacy-matching cases after delegation (153second compile,
  .06second tests), with no compiler warnings. Log: target/body-layer-plan-targeted.log.
- `python tools/check.py quick --crates bri-content,bri-client`: four steps in
  149 seconds; 681 passed, zero failed, 76 ignored. Log:
  target/body-layer-plan-quick.log.
- `python tools/check.py focused --crates bri-content,bri-client --depth 0
  --tests bri-client:body_layer_plan --tests bri-client:seat_fallback_anchor
  --tests bri-client:seat_pose_composition --tests bri-client:avatar_animation_render
  --tests bri-client:vehicle_first_person`: eleven steps in 229 seconds;
  700 passed, zero failed, 82 ignored, with both all-target Clippy checks passing
  under `-D warnings`. This includes all three pure planner tests, the two
  legacy-matching client tests, existing anchor/ready composition tests and
  offscreen synthetic render/camera cases. The six generated-content render/
  camera variants remain ignored by this source-free run, alongside the 76
  existing client unit ignores. Log: target/body-layer-plan-focused.log.
- `git diff --check HEAD` passes. Production remained frozen throughout the
  final quick/focused run. No full gate, push or interactive playtest was run.

## Remaining work

Root owns integrated candidate3436de89 gate/CI and subsequent main integration;
no worker gate or push. Stage B fitting, Stage C socket/tool deltas and later
server immutable assets/body identity/producer state require separate grants.
The retained real Session red fixture on42275af9/20a84b1 remains the future
integration target, not replaced by these green planning regressions. No gameplay
origin policy is inferred from FirstPersonOrigin or cosmetic muzzle placement.
Original per-game fidelity and final human acceptance remain required. Cache
retained for assigned work.
