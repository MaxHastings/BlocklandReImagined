# 2026-10-16 Authored rotation interpolation prerequisite

Root accepted clean contact b5a82226 and assigned this bounded next atom from
that exact head on `codex/authored-rotation-interpolation`. Read current STATUS,
alpha contract, progress and platform principles; original installs untouched,
native/headless checks only. No new agents or worktrees.

Before production, proposed a Source-independent typed per-animation
rotation_interpolation selection: spherical default or normalized_linear, node
rotation keys only, preserving all layer/transition/ground/scale-axis semantics.
No content-name inference, densification or new framework. Keep Shape/ClipSet
schema1 as defaulted alpha extension, no protocol marker/version edit. Root
granted the exact constructor footprint after enabled red evidence.

Existing literals: fifteen across thirteen files:

```text
crates/content/src/animation.rs (two)
crates/content/src/body_pose.rs
crates/content/src/testing/avatar.rs
crates/convert/src/shape.rs
crates/addon-import/src/gltf/mod.rs
crates/client/src/world_items.rs
crates/client/src/vehicles.rs
crates/client/src/testing.rs
crates/client/tests/body_layer_plan.rs
crates/client/tests/seat_pose_composition.rs
crates/client/tests/seat_fallback_anchor.rs
crates/net/src/testing/vehicles.rs (two)
crates/net/src/testing/items.rs
```

Strict native tests deserialize invented authored policy and check actual native
pose/socket output against independent component math. Source
PR42/private4c932590 original SDK evidence is root-routed and remains distinct
from these invented controls. Read its policy-gap JSON and terminal/attachment
note: non-collinear invented endpoints X(.7), Y(2.0), quarter time produce SDK
(.28481275,.23297675,0,.92984056), independently matched by component math.
Original fractional playback, SDK s>0.001/source-section rules, sequence/IK and
attachments remain private consumer acceptance; this primitive implements no
source-specific branch.

## Evidence and corrections

All commands use CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, CARGO_TARGET_DIR unset. No visible game/playtest, no gate,
push, rebase or shared integration.

- First test-only compile failed on an ambiguous float clamp; annotated f32.
  Log: target/authored-rotation-interpolation-diagnostic.log.
- Before any production edit, enabled strict target exited101: one legacy
  control passed, three failed, zero ignored. Actual native pose did not match
  normalized-linear, unknown policy was silently dropped, and the inner-key /
  outer-layer expectation failed. Log:
  target/authored-rotation-interpolation-diagnostic-red.log.
- Root then granted the proposed API and fifteen mechanical defaults. First
  scripted constructor edit stopped on Windows default-encoding Unicode input;
  reran explicitly UTF-8. Its count assertion caught an accidental insertion in
  the converter's internal Pools struct declaration; removed that invalid line
  before compilation. Verified diffs show exactly fifteen literal fields and
  no unrelated text/encoding changes.
- Four original controls passed after implementation, zero failed/ignored,
  27.58s build, below.01s tests. Log:
  target/authored-rotation-interpolation-first.log.
- Expanded enabled target: eight passed, zero failed/ignored, 9.05s build,
  .06s tests. Added non-collinear unchanged-SDK numeric control, sign alignment,
  unit output, looping/negative time/endpoints/zero duration/single key, sparse
  rest, unchanged scale axes, ClipSet roundtrip, invalid keys/times/weights and
  additive/frozen-transition controls. Added a serialized-byte equality control
  for absent versus explicit spherical before the scoped tiers. Log:
  target/authored-rotation-interpolation-expanded.log.
- `python tools/check.py quick --crates
  bri-content,bri-convert,bri-addon-import,bri-client,bri-net`: ten steps in992s;
  876 passed, zero failed, 79 ignored (76 client, three net). Log:
  target/authored-rotation-interpolation-quick-first.log.
- First focused stopped at content Clippy after122s: the test oracle used the
  repository-disallowed f32::clamp. Changed that one test call to the repository's
  bri_console::Clamp::clamped, preserving finite/bounds behavior and all sampler
  production. Log: target/authored-rotation-interpolation-focused-first.log.
  Restarted scoped quick then focused after this concrete test-only correction.
- Corrected quick passed ten steps in95s: 876 passed, zero failed, 79 ignored.
  Log: target/authored-rotation-interpolation-quick.log. Corrected focused passed
  all five all-target Clippy checks with `-D warnings` before integration suites.
- Corrected `python tools/check.py focused --crates
  bri-content,bri-convert,bri-addon-import,bri-client,bri-net --depth 0`
  plus explicit content:authored_rotation_interpolation, client:body_layer_plan,
  client:seat_pose_composition, client:seat_fallback_anchor and client:world_items
  passed: twenty steps in861s, 85 result summaries, 1,225 passed, zero failed,
  112 ignored. All five all-target Clippy checks passed with `-D warnings`.
  The explicit authored target reports eight passed, no failures/ignores.
  Existing body-layer2, seat-composition7 and fallback4 controls pass; world
  items fourteen pass/thirteen content-dependent ignores. Full non-client
  suites run before explicit targets, including the planned repeated content
  target. Log: target/authored-rotation-interpolation-focused.log.
- `git diff --check` passed. Root received exact eighteen paths before commit.
  No production changes followed initial green sampling. Root's short release
  performance slot permits finishing this already-running pipeline; it is now
  complete, with no new heavy compile/render/soak started. Active target cache
  retained. Root owns integrated gate and Source exact-SHA original acceptance.

## Exact implementation footprint

Eighteen paths; semantic production only shape.rs and animation.rs. Constructor
additions preserve defaults; new test/design/progress plus one seam row complete
the grant. No contact production changes from parent b5a82226.

```text
crates/addon-import/src/gltf/mod.rs
crates/client/src/testing.rs
crates/client/src/vehicles.rs
crates/client/src/world_items.rs
crates/client/tests/body_layer_plan.rs
crates/client/tests/seat_fallback_anchor.rs
crates/client/tests/seat_pose_composition.rs
crates/content/src/animation.rs
crates/content/src/body_pose.rs
crates/content/src/shape.rs
crates/content/src/testing/avatar.rs
crates/content/tests/authored_rotation_interpolation.rs
crates/convert/src/shape.rs
crates/net/src/testing/items.rs
crates/net/src/testing/vehicles.rs
docs/architecture/authored-rotation-interpolation.md
docs/architecture/seams.md
docs/progress/2026-10-16-authored-rotation-interpolation.md
```
