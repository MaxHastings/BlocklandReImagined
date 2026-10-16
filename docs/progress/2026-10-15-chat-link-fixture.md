# 2026-10-15 Chat link fixture geometry

Root assigned only `crates/ui/tests/chat_links.rs` and this progress entry after
the normal-frame commit. No UI runtime or shared synthetic assets changed.
Halo's pinned preview remains independent of this fixture correction.

## Failure and correction

Gate candidate `ab10374f` passed the synthetic chat-link case but failed the
converted-content case at its MessageBox assertion: top screen was Play. The
fixture measured a link point before changing ChatSize through 0, 4 and 10, then
clicked the old point after ending on size 10. Runtime link hit-testing lays out
the current profile selected by ChatSize. The shared synthetic pack assigns the
same fixture font to all chat sizes, which masks this stale-coordinate case.

The test now finds the link in the current layout after those size changes and
verifies the actual click point maps to the expected URL. Its helper includes
the click inset in the searched coordinates instead of adding it after lookup.
It rechecks the point after toggling the cursor. The assertions that cursor-off
input opens no dialog, cursor-on input requires MessageBox confirmation, no
OpenUrl action occurs before confirmation, and hostile schemes are rejected all
remain in place.

## Verification

The worktree's ignored `content` junction reads the existing converted content in
`playtest-v031/content`; no source content or preview file was changed. Commands
use process-local `CARGO_BUILD_JOBS=4`, `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_INCREMENTAL=0`, leaving `CARGO_TARGET_DIR` unset. No OS window, browser or
physical input is created by this UI fixture.

- `cargo test -p bri-ui --test chat_links --locked -- --include-ignored`: both
  synthetic and converted-content cases pass, zero ignored, in 0.35s after a
  17.07s compile. The previously failing confirmation assertion is unchanged.
- `python tools/check.py quick --crates bri-ui`: formatting and unit tests pass
  in 22s; 226 passed, 11 existing ignores.
- `python tools/check.py focused --crates bri-ui --depth 0`: all three UI-only
  steps pass in 57s, including warnings-denied all-target Clippy and UI unit/
  integration/doc tests. No dependent workspace build or full gate was launched.
- `git diff --cached --check`: passed before commit.

No assertion was weakened and no test was newly ignored. The existing content
junction and active build cache are retained under root's integration direction.

Root owns gate timeout/crash investigations. A later isolated crash pass is
nondeterminism evidence, not a crash fix or a clean integrated gate. This test
change does not alter those outcomes.
