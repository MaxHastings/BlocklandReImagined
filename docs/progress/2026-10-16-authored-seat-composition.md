# 2026-10-16 Authored seat held-image composition

Root assigned codex/authored-seat-composition from immutable dd954e85, after
merging the explicit image-origin checkpoint locally and dispatching Source
consumer verification. This is the next bounded active release correction,
with no unrelated anchor, host-origin, material, texture, manifest/lock or wire
changes. Root owns main/gates/publication and the final release hold.

Optional rider_ready_policy on a native seat declares exact mounted-image-set
and ready-role contracts. A pure schema helper validates/bounds matches and
resolves the body's existing ready fallback provenance; the app supplies the
actual active seat/images and Avatar verifies the active owned seat clip before
omitting ordinary ready. Actions, gestures, look/seat aim, fitting/IK, transitions
and priorities are unchanged. Legacy/standing/missing/unmatched paths retain
layering. Matching ignores image ordering but never infers meaning from names,
Both hands or preservation flags. Unlisted secondary/attachment images fail
closed; explicitly listed attachments may participate.

The actual mount-image domain was verified in weapons/runtime.rs: IMAGE_SLOTS=4,
HAND_SLOTS then WORN_SLOTS. A mirrored schema constant is guarded by cross-crate
regression because vehicles has no weapons dependency and no new manifest
dependency is authorized. Contracts cap at 16 and images at four distinct slots;
IDs cap at 256 bytes. Duplicate image-set/role keys, slots, empty/invalid values,
unknown enum values and policy without rider_pose reject. Both-to-Right fallback
uses RoleClips provenance, not a client-only role/name heuristic. Eligibility is
three small bits, not a per-frame clone of authored policy strings.

Owned core files are vehicles/schema.rs, client/app/frame.rs, client/avatar.rs,
the new client seat_pose_composition test, modding doc and one exact seam ownership
row. Only constructor None defaults changed in the four granted paths:
vehicles/testing.rs, vehicles-import/lib.rs, sim/tests/showcase.rs and
addon-import/tests/adventure_port.rs. Existing AvatarAnimationInput literals
already use defaults except the assigned frame path; no extra ownership needed.

## Evidence

Cargo uses process-local CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4; CARGO_TARGET_DIR remains unset. All validation is code,
headless or bounded offscreen; no visible game or interactive controls.

- Initial cargo check -p bri-client --tests --locked passed in 100 seconds.
- The shared fallback helper extraction overlapped the first targeted build:
  vehicles had compiled its earlier API while client read the newly extracted
  covers_resolved call, producing a method-not-found compile failure. No runtime
  assertion had run. Froze/refreshed the sources and reran; first log is retained
  as target/seat-composition-targeted-first.log.
- A scripted edit initially assumed pre-rustfmt text; it stopped after adding the
  helper. Completed the caller edit against the inspected formatted source before
  the frozen rerun. No incomplete edit is accepted as evidence.
- Frozen `cargo test -p bri-client --test seat_pose_composition --locked` passed
  all six source-free integration cases, zero ignored (compile 281 seconds;
  tests 0.07 seconds). Added the frame adapter's actual MountedImage regression
  and a no-seat-context control before final quick/focused validation.
- `python tools/check.py quick --crates bri-vehicles,bri-client,bri-vehicles-import,bri-sim,bri-addon-import`:
  10 steps in 464 seconds; 1,061 passed, zero failures, 83 existing ignored.
  Client 580/76 includes the actual MountedImage frame-wiring regression.
  Log: target/seat-composition-quick.log.
- First focused attempt stopped at warnings-denied Clippy's vec_init_then_push
  in integration fixture setup (seven steps, 99 seconds). Replaced the four
  immediate pushes with vec! initialization, preserving the cases. No production
  logic changed. Failure log: target/seat-composition-focused-first.log.
- `python tools/check.py focused --crates bri-vehicles,bri-client,bri-vehicles-import,bri-sim,bri-addon-import --depth 0 --tests bri-client:seat_pose_composition --tests bri-client:avatar_animation_render --tests bri-client:vehicle_first_person`:
  all 18 steps passed in 992 seconds. All five crates passed warnings-denied,
  all-target Clippy; selected default suites and explicit client targets passed.
  Summed test executions: **1967 passed, zero failed, 274 existing ignored**.
  The final seven seat-composition cases pass, avatar animation rendering passes
  3/3 ignored, and vehicle first-person checks pass 3/3 ignored. Converted-content
  variants remain existing ignored cases; this is scoped evidence, not the full
  include-ignored gate. Log: target/seat-composition-focused.log.
- Final diff review and `git diff --check` passed; only the 11 assigned files are
  included. Root receives the atomic SHA for integration and Halo's exact-SHA
  original consumer comparison. No main push or full gate was run here.

Source-free regression covers distinct seat priority 12 and ready priority 14,
both hands, ordinary/no policy/unmatched tools and secondary/attachment sets,
invalid/ambiguous contracts, exact slot-domain agreement, standing/missing owned
clip/body switch, preserved Both-to-Right fallback and uncovered role, authored
actions/gesture/look/seat aim, scale/tilt/turn and preservation flags. Math samples
use the same invented clips through the native sampler and actual AvatarMesh,
not original assets or an imitation socket offset.

## Active fidelity closure and next action

Halo must bind the exact original held-image sets and applicable passenger clip
at the committed engine SHA, recover the original aim/action crosswalk, and rerun
actual source/native adapter comparisons. Synthetic neutral preservation is not
original aim acceptance. Root then reserves separate fallback-anchor and shared
authoritative mounted-pose/Session obstruction work sequentially; this slice does
not claim those fixed. Root's running full candidate gate at 1be98fd7 covers the
preceding integrated candidate, not this new slice. A later integrated candidate
and faithful agreed Add-On sets plus Maxwell's final all-game playtest remain
required. Diagnostic success, successful imports or labelled approximations do
not close release fidelity. Active cache is retained for assigned release work.
