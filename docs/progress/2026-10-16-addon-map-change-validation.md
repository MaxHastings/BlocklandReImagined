# 2026-10-16 Add-On map changes rejected before loading

Maxwell reported that a hosted game's stock map changes work while Add-On map
changes do not. Root assigned read-only diagnosis at exact released commit
5956e3a383a1c241bd5192a3aa8cc4f358f2e503, then granted only admin/src/lib.rs,
admin/tests/authority.rs, new net/tests/addon_map_change.rs and this entry.
Branch codex/v031-addon-map-switch-fix reuses the idle shared-engine-seams
checkout. Previous9f233b3f and853d02f3 branches/checkpoints remain preserved.
Root owns the next candidate; the published v0.3.1 release remains immutable.
Root subsequently assigned this correction to v0.3.2 row B1 in approved
48ffe16c93af4307fd74fc4415bacb0ddcc15d07:docs/releases/v0.3.2-readiness.md.
Read that acceptance contract; no other mechanism or readiness row is owned here.
The scoped fix/test receipt does not replace root's Windows/integrated acceptance.

The provider/catalog/host/client paths preserve the full authored map ID.
MapCatalog lists namespace:map/name and resolves its native bundle directory;
HostSetup loads it; the client forwards that full ID from the admin map list.
But Action::ChangeMap validation allowed only alphanumeric characters and ._-/,
rejecting the colon required by every Add-On map. Session's admin boundary calls
that validator before dispatch, so neither host loading nor client preparation
begins. This is independent of a particular map pair or its assets.

Baseline reproduction uses two invented native map packages, each providing its
own bundle under a distinct package/root, with the same short name main and
different authored placement. Both are loaded through the actual content root,
map provider catalog and dedicated HostSetup. Direct loading succeeds. Ordinary
Session.command first accepts a stock target then rejects the listed second
Add-On ID. A real QUIC loopback host and guest complete Add-On-to-stock and
stock-to-stock transitions, then the host rejects the Add-On target with
invalid native value. Both new tests fail at that exact request:
target/addon-map-change-red-final.log, zero passed/two failed/zero ignored,
.30s tests. Admin validation independently fails first-map:map/main:
target/addon-map-admin-red.log, zero passed/one failed/zero ignored.

Initial diagnostics and fixture corrections are preserved under target: an
ignored standalone rustc probe could not link mixed cached admin feature variants
and was superseded by Cargo's ordinary Session/network tests; net fixture first
needed the client module import and connect resume argument, then legal manifest
fields (license, no side/requires). Final red fixtures load without those warnings
and require no original content. No production edit preceded root's exact grant
and the ordinary Session baseline evidence.

The nine-line production correction selects the existing ContentId parser for
namespaced IDs and requires kind map. Existing stock-ID validation, 128-byte text
limit, traversal constraints and Session listed-map membership remain. No new
grammar/parser/dependency, protocol/schema, provider/UI/client/loader behavior.
Tests retain malformed paths, wrong content kinds, size limits, unlisted targets
and unauthorized callers as negatives, and exercise full-ID round trips across
both native providers and stock controls while preserving connection identity.

All Cargo commands use CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=2 and CARGO_TARGET_DIR unset. No visible game, interactive
automation, original installation writes, full gate or main/release modification.
Final scoped validation, unchanged code:

- Targeted admin authority: thirteen passed/zero failed/ignored (.07s).
  Targeted native map Session/QUIC: two passed/zero failed/ignored,1m22s compile
  and .27s tests. Logs target/addon-map-admin-green.log and
  target/addon-map-change-green.log.
- `python tools/check.py quick --crates bri-admin`: two steps/1s, formatting
  passed; admin has zero lib unit cases (no failure/ignore). Semantic coverage is
  in its authority integration target, not hidden by that empty lib target.
  Log: target/addon-map-quick.log.
- `cargo fmt --package bri-net --check` passed. Explicit new-target lint,
  `cargo clippy -p bri-net --test addon_map_change --locked -- -D warnings`,
  passed29.05s (target/addon-map-net-clippy.log).
- `python tools/check.py focused --crates bri-admin --depth 1 --tests
  bri-net:addon_map_change`: nine steps/1751s,2,383 passed/zero failed/251 ignored
  across197 summaries; admin all-target Clippy passed. Direct dependent
  addon-import, chaos, client, net and sim suites passed. Explicit map target
  final rerun two/zero/zero,.47s. Log: target/addon-map-focused.log.
- Final whitespace diff check passed. Exactly four granted paths, only nine
  production lines, no main merge, full gate, release tag or artifact change.

Root queued a separate single-player report that confirming Change Map appears
to do nothing, map kind unspecified. The current receipt verifies ordinary
Session/QUIC behavior, not the actual App/UI confirmation route. Read-only
comparison of that route on released5956 and this fix is next after immutable
handoff; it may expose another bug. No release/readiness row is closed here.
Root owns immutable diff review, required Windows CI and next-candidate
integration/publication. This branch is pushed only after the recorded exits.
