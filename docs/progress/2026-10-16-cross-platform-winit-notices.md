# Correct shared package notice coverage before publication

Root's pre-publication review found that the macOS packager is separate from
the Unix/Linux packager. The initial wheel backport added notice/license inputs
to Windows and Linux, leaving macOS without those new files. No release/tag has
been published. This is a root packaging omission, not an engine regression.

All three packagers already invoke package_guides.py for their shared document
step. That helper now copies the exact vendored winit license and modification
notice too, ensuring macOS receives them before its final manifest is hashed.
The Windows/Linux inputs remain present and produce the same bytes. No app
binary, signed bundle, content pack or engine behavior changes.

Validate the actual shared helper outputs and Python tool suite plus the real
packaging fixture, then integrate after the current frozen591ff3 gate finishes.
The corrected candidate must be built and verified on all three platforms before
publication; do not publish the older build-only591ff3 artifacts. Current
builds/gate are left intact. Record the new immutable candidate and exact checks.

Validation passed: actual shared document step produces8 files, including exact
license/patch bytes without relying on either platform-specific copy list;
Python compilation and all99 tool tests pass (32.317s); real Windows packaging
fixture exits0 and validates350 manifest files. Logs target-notice-tools.log and
target-notice-packaging.log. Diff whitespace clean. Corrected macOS artifacts
must still be checked after the next immutable build before publication.
