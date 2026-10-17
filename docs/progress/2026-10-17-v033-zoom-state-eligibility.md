# 2026-10-17 Mounted image-state eligibility for visual Zoom

Root granted exactly weapons/lib.rs, client/controls.rs and client/app/view.rs,
new weapons/tests/zoom_states.rs, existing client/app/tests.rs/inline controls
tests, modding/weapons.md and own notes. Branch codex/v033-weapon-handling starts
at published e4d9bb05; saved31f/722/4b branches and old diagnostic artifacts remain
preserved. No main integration/push/publication or full gate in this lane.

Why now: AA existing-API scope overlay and sprint held-bind consumers passed, so
the parked synthetic missing-pose diagnostic did not justify new pose or input
APIs. Root then reviewed an actual coupled AA App/QUIC red20261017-071002,
fixture SHA ba516b07310d083fa1d61ae8a526c93ee2f5f3f546079c19982fb2700b37d2ad:
Ready/Fire/WaitRelease positives passed, but accepted Draw/Lower/Hold/Raise/Reload
states retained FOV70/scope and hid the lowered rig. Neither set_fov90 nor an
image variant fixes same-mounted-image eligibility. AA final immutable receipt
is separately root-owned; this public test uses no AA assets or source values.

Implementation: optional Zoom.allowed_states (None/omitted legacy all; Some[]
none). At most64 unique nonempty ASCII names of at most128 bytes, no controls
or all-whitespace; reject casefold duplicates. Pack validates each reference
against exactly one state of the owning image. allows_state handles casefold
matching and fails closed on unknown/missing state for a restricted profile.
Old Controls.set_aim without state delegates to set_aim_in_state(...,None) and
is fail-closed for restricted profiles. Current App passes accepted hand0 state.
Temporary gating retains the full profile and aim_level. aiming gates derived
scope/FOV/sensitivity/crosshair/wheel/sway/forced-FP; a specifically barred
retained profile also blocks keyboard genericZoom fallback. Movement still uses
the retained on_jet/jets profile, so jets:false cannot become jetting just because
the current state bars visual aim. No authority/fire/state/pose/protocol change.

Source-free controls cover metadata/roundtrip/bounds/casefold/ambiguity, legacy
omission, all-barred and missing-state old setter, right-button ownership with
jets false/true, keyboard fallback, wheel retention/reaim and sway. Actual App
fixture uses made-up eight-state gun, real local host/package commands and
ordinary RMB/LMB. Explicit first-person checks distinguish hidden scoped rig
from visible barred rig, while authoritative fire/ammo remains native policy.
Includes release/focus/different-item swap/death/respawn; same-ID remount identity
is expressly not inferred from equal IDs. No original AA3D/interpolation claim.

Initial metadata attempt failed before tests because Python's Windows default
encoding wrote a literal non-ASCII negative test as CP1252. Preserved log
artifacts/v033-zoom-states-metadata-first-encoding.log; replaced the test literal
with an ASCII Rust Unicode escape. Rerun metadata target PASS4/0. No waiver or
ignore. Further targeted results and frozen-source tiers will be recorded below.
Every Cargo run uses jobs2/test threads8 and CARGO_TARGET_DIR unset.

## Targeted green and freeze

Metadata4/0; client controls40/0 including four new gate controls; actual App1/0
in5.63s on the first App run. Real owner/mount and normal package commands
publish each barred native state before scope/FOV/visibility assertions.
RMB held throughout: Draw/Lower/Hold/Raise/Reload -> overlay none, normal90FOV,
explicit first-person rig present; no jet. Ready reaims without releasing RMB.
One ordinary shot12->11 reaches WaitRelease while LMB stays held and scope stays
on; each barred fire attempt preserves11. Release/focus/different-item swap,
death/respawn and fresh intent pass. No same-ID remount claim. Existing synthetic
optional effects warnings are retained. Archived old redundant diagnostic import
is absent from the new shipping regression; old pose driver/reds untouched.

Before tiers, formatted only granted Rust files and captured their SHA256 in
artifacts/v033-zoom-states-frozen-source.json. No midrun source edits after this
freeze. Quick and full normal focused tier results follow below.

## Final validation and wind-down

Quick PASS: 4 steps / 62 seconds, client599 + weapons13 = 612 passed,
0 failed, 76 existing ignored. Full normal focused PASS exit0: 14 steps /
2038 seconds, aggregate result rows 2747 passed, 0 failed, 278 existing ignored
(includes explicit repeated metadata target). Both all-target clippy checks pass.
Commands: `python tools/check.py quick --crates bri-weapons,bri-client`;
`python tools/check.py focused --crates bri-weapons,bri-client --tests bri-weapons:zoom_states`.
All five frozen Rust source hashes reverified before commit. Logs are preserved
outside target in artifacts/v033-zoom-states-*.log; no new ignores or waivers.

Maxwell directly instructed this chat to wind down all Halo/AA/later work and
complete only v0.3.2 publication. Verified via gh release view: v0.3.2 already
published 2026-10-17T03:17:49Z, so no republish. This bounded v033 branch is
saved separately, not merged into main or the release. Actual AA coupled consumer
on this new engine pin remains unrun; no source fidelity or integration acceptance
claim. No new consumer rounds or feature work. Root was notified and owns global
cleanup. Preserve source, branch and evidence; this worktree target can be removed
after validation completes without affecting main packaging.
