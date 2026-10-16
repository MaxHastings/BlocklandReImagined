# 2026-10-15 Explicit first-person image origin

Root assigned `codex/image-origin` from immutable `84da72c8` after reviewing
Source's private desired contract and the read-only mount/pose triage. The
contact-audio commit remains separate. This slice adds optional typed Eye/Mount
placement, with absent selector preserving legacy numeric inference. It does
not change texture limits, seat composition, fallback anchors, firing authority,
manifests/lockfile, wire data, source assets or the pinned preview.

`FirstPersonOrigin` lives with native Image data and is carried to client
ImagePresentation. Both base identity validation and authored Add-On presentation
loading reject conflicting origins. Presentation generated from a native Add-On
copies the selector. Explicit identity Eye with follow_arm=false bypasses host
hand, model correction and unrequested action queries; Eye with arm following
retains the existing action transform. The base loader retains forced stock arm
following only for absent origin selectors; explicit origins preserve authored
follow_arm. This boundary was confirmed by root and is tested with a supplied
nonidentity action and an unchanged legacy-absent image. Explicit Mount selects
the ordinary hand path even with nonzero eye fields. Third person is unchanged. No epsilon offset,
content-name condition or guessed Source camera behavior is introduced.

Root granted constructor-only defaults in client actor_effects source/test,
sim/tests/showcase.rs, net/src/content_identity.rs and net/tests/{bandwidth,
loopback}. All six were full literals requiring None; assertions/logic remain
unchanged. All ImagePresentation literals are within the assigned items.rs.
The importer adds only the legacy None constructor default. Root's intentional
one-row ownership addition is included.

## Evidence

Validation uses process-local CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, with CARGO_TARGET_DIR unset. Only code/headless/offscreen
checks; no visible game or interactive controls.

- Initial targeted `cargo test -p bri-client --lib --test image_origin --locked image_origin`:
  one placement unit and three integration cases passed before the final
  Add-On conflict/base-loader boundary cases were completed.
- Initial `python tools/check.py quick --crates bri-client,bri-weapons,bri-weapons-import,bri-net,bri-sim`:
  10 steps in 306 seconds; 1,087 passed, zero failed, 83 existing ignored.
  Its client check predates the final base follow_arm boundary fix; other crates
  did not change afterward. Log: `target/image-origin-quick.log`.
- Final-code `python tools/check.py quick --crates bri-client`: 579 passed,
  zero failed, 76 existing ignored; two steps in 67 seconds. Log:
  `target/image-origin-final-client-quick.log`.
- Final-code `cargo test -p bri-client --test image_origin --locked`: all four
  integration cases passed, zero ignored, including the base loader's false arm
  policy under a supplied nonidentity action and preserved legacy arm behavior.
  Log: `target/image-origin-final-targeted.log`.
- Root approved focused depth 0 for all five changed crates plus the explicit
  image-origin, item-pose, actor, bandwidth, loopback and showcase targets. This
  schedules verification ahead of one root-owned full integrated candidate;
  it does not reduce fidelity acceptance.
- Final focused command:
  `python tools/check.py focused --crates bri-client,bri-weapons,bri-weapons-import,bri-net,bri-sim --depth 0 --tests bri-client:image_origin --tests bri-client:item_pose_ownership --tests bri-client:actor_effects --tests bri-net:bandwidth --tests bri-net:loopback --tests bri-sim:showcase`.
  All **21 steps passed in 1,124 seconds**, including warnings-denied all-targets
  Clippy for all five changed crates. Summed executions: **2,114 passed, zero
  failures, 254 existing ignored**, including the explicit requested repeats.
  Explicit targets: image_origin 4/0 ignored; item_pose_ownership 2/0;
  actor_effects 14/1; bandwidth 9/2; loopback 31/2; showcase 33/0.
  Log: `target/image-origin-focused.log`.
- Final format check across all five changed crates and `git diff --check`
  passed. Comments/docs were clarified without changing behavior after the last
  client quick rerun. No full gate, source-content render, main push or release
  publication was performed. No validation failures occurred in this slice.

The checked new mechanism now needs the exact committed SHA consumer check below;
these test counts are not a blanket original-to-native fidelity certificate.

Synthetic tests use distinct nonidentity eye, hand, image transform, correction
and action frames. They exercise exact identity eye without correction/hand/action
queries, Mount with nonzero eye fields, all third-person policies, legacy zero
and nonzero behavior, independent arm action, serde omission/roundtrip/invalid
selector, generated native presentation, base identity mismatch and Add-On
presentation conflict diagnostics. No private content is embedded.

## Release work and next closure evidence

This mechanism is not full Source viewmodel fidelity acceptance. Root must hand
the exact engine SHA to Source for its original-model consumer revalidation with
explicit Eye, identity eye-local transform and independent follow_arm policy.
Source owns original viewmodel/hand composition and camera crosswalk evidence;
its synthetic desired fixture must select the new origin explicitly while the
legacy zero-field regression still expects hand placement. Consumer results,
faithful agreed Add-On sets, combined checks and Maxwell's final cross-game
playtest remain required. Root owns main integration/gates and the release hold;
no independent merge, push or publication occurs here.

Seat composition, fallback hip anchor and authoritative mounted-pose corrections
remain active release work under root, to be assigned sequentially after this
checkpoint. Contact repeat/promotion/audibility rules remain Halo's recovery
responsibility: recover governing native handlers/counting windows and compare
material/repeat cases against original behavior before binding. A labelled
adaptation or diagnostic/import success cannot close those fidelity items.
The active checkout cache is retained for the assigned release work.
