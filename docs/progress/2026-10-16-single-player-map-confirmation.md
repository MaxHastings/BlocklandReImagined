# 2026-10-16 Retain actual single-player App map confirmation coverage

Root requested read-only follow-up to Maxwell's single-player report: confirming
Change Map with Yes appears to do nothing, map kind unspecified. Original B1
source/Session/QUIC fix761b695f remains unchanged on its own pushed branch/PR178.
This separate test-only branch starts at that exact immutable head. Root granted
only new client/tests/admin_map_change.rs and this entry; no product paths.

The actual public App runs headlessly with null audio and no GPU/window. It hosts
SinglePlayer, consumes server-derived permissions and map list, opens the real
admin screens, selects the UI model target, advances an ordinary frame, presses
the actual Change button and Yes callback (or Enter control), pumps the normal
worker/network/scene loading path and requires completed InGame plus the exact
new map ID. Host owner and menu teardown are checked. Confirmation and network
commands are not injected. UI layouts and two native map providers are invented
fixtures, not original pixel or physical-device evidence. The providers have the
same short name main, different roots and placements.

Read-only before/after probe receipts used the same final predicate on released
5956e3a383a1c241bd5192a3aa8cc4f358f2e503 and fixed
761b695f844490ac4d1970cf6eded71caf146ea5. Released stock Yes/Enter both pass;
Add-On Yes/Enter both emit request7 and close menus, then receive host rejection
invalid native value while retaining mapA. Fixed stock and Add-On Yes/Enter all
complete in the new map. This explains the apparent Add-On no-op; no independent
stock single-player Yes defect reproduced. Logs:
target/sp-app-diagnostic-released-final.log and target/sp-app-diagnostic-fix.log;
exact client build receipts sp-app-build-released-final.log and
sp-app-build-fix.log. Probe source is target/sp-app-diagnostic.rs.

The initial read-only probe selected the model then clicked before a nonzero UI
frame refreshed the disabled Change button. Corrected the driver, not the engine;
exclude that initial timeout as bug evidence. An intermediate PowerShell probe
rewrite also failed quoting; retained receipts are the final successful probe
builds/runs above. Earlier cached-library standalone Rust probes failed matching
feature variants and were superseded by coherent Cargo build receipts.

The formal regression retains four routes in two tests. It fails promptly on
visible admin Rejected status rather than treating rejection as a guessed load
timer. The actual released host rejection is retained in the read-only baseline
receipt; no impossible UI-listed invalid target is fabricated to force another
negative. Existing B1 Session/QUIC tests retain malformed/wrong-kind/unlisted and
unauthorized controls. This adds no App/UI/admin/net production behavior.

Cargo uses CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=2, target directory unset. No interactive gameplay, original
source edits, main/full gate or release changes.

Final unchanged-code validation:

- New integration target: two passed/zero failed/ignored (four App routes),
  35.05s compile/1.42s tests, target/sp-map-confirmation-target.log.
- `python tools/check.py quick --crates bri-client`: two steps/73s,
  584 passed/zero failed/76 ignored, target/sp-map-confirmation-quick.log.
- `python tools/check.py focused --crates bri-client --tests
  bri-client:admin_map_change`: four steps/120s,586 passed/zero failed/76 ignored
  across two summaries. Client all-target Clippy (including the new integration
  target) passes68.5s. Final explicit map target two/zero/zero,1.71s tests.
  Log: target/sp-map-confirmation-focused.log. Standard plan has no further
  client dependents; no narrower dependent depth override was used.
- Final whitespace check passed. Exactly two new granted paths, no production
  edit. Immutable test checkpoint parent is761b695f, pushed on its separate
  codex/v032-sp-map-confirmation-regression branch after the actual exits.

Root reviews the combined source/test receipt and required Windows/integrated
checks; no release/readiness closure is asserted here. A future specific stock
single-player failure remains reportable despite these bounded positive controls.
