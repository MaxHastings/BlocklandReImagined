# Linux legacy wheel-button duplicate notch

Root traced the pinned winit0.30.13 X11 handler: both XI_ButtonPress and
XI_ButtonRelease reach xinput2_button_input, whose non-emulated buttons4-7
unconditionally became MouseWheel events. One physical legacy notch therefore
produces two application notches. Smooth-axis and emulated-pointer handling are
separate existing paths. Source reference:
https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/linux/x11/event_processor.rs

Root extracted that exact mapping without changing its behavior, then passed
press/release pairs through the actual native inventory and paint UI. All three
regressions fail on the unchanged mapping: release emits another event, the
newly picked-up fourth tool is skipped (slot2 instead of1), and paint jumps to
swatch2 instead of1. The same expectations pass3/0/0 when only wheel-button
release is suppressed, including UI scales1/1.5/2. Logs:
target-wheel-three-red.log and target-wheel-green-corrected.log. Earlier probe
import mistakes and confusing paint column with swatch were corrected before
this final unchanged-expectations red/green comparison; they are not evidence.

The published Apache-2.0 winit0.30.13 source is vendored with its license and
patch notice, excluded from first-party workspace membership, and selected by a
local crates.io patch. All upstream source files remain byte-identical except
the X11 dispatch helper extraction; the new small helper contains the release
guard. No dependency versions changed. This avoids a broad dependency upgrade
or lossy time/rate filtering after winit has discarded the button state.
Ordinary mouse buttons, smooth scrolling, Wayland and other platforms retain
their original handlers. Both packagers include the upstream license and
modification notice. The actual packaging fixture finishes successfully and
verifies350 manifest files. The first shell wrapper incorrectly propagated the
fixture's deliberately retained last negative-case exit code; the corrected
PowerShell success-status wrapper exits0 (target-wheel-package-confirmed.log).

Scoped client quick passes584/0/76 in78s. Focused exits0 in162s: the same client
units, all-target Clippy with warnings denied, formatting and explicit
x11_wheel3/0/0 (target-wheel-focused.log).
A dedicated Linux CI job compiles the real Linux backends and runs the exact
production mapping/real UI regression. That job has not yet run at this entry.

This establishes a specific X11 duplicate-notch defect and its native UI effect.
The report does not identify the user's display backend/device, so it is not a
capture of that user's physical Linux event stream. Agent validation remains
headless; no interactive game input, visible game or user's desktop input was
used. Full integrated gate/platform/package verification still precedes release.
