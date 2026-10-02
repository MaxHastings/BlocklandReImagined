# Rule Workshop: free-build isolation and deferred-fact diagnostics

2026-10-02. Requested by the integration coordinator after reviewing the
83939023 Workshop source snapshot. This is a separate corrective commit, with
no packaging/workflow changes and no interactive game session.

## Findings and changes

The region comment/design required players to share the builder's MiniGame,
including both having no MiniGame in free build. The implementation only
filtered when the builder had a match, so a free-build region counted players
inside someone else's match. Player observation now compares the two
`Option<GameId>` memberships directly before occupancy/Enter/Stay/Leave logic.

Object behavior is deliberately preserved separately: only the builder's
spawned objects are observed; uncredited objects are eligible. In a match, a
credited mover must share that match. Free-build object ownership does not
become dependent on the credited mover's match. This prevents an isolation
correction for players from silently changing the physics laboratory policy.

The deferred native match-fact queue remains capped at 256, but overflow now
reports the fact, MiniGame and cap through the existing 64-entry event diagnostic
queue instead of silently discarding it. The region-budget warning uses that
same bounded sink. This reuses the existing diagnostic helper; no new budget,
provenance framework, runtime path, save/network schema or public Add-On API.

## Regression evidence

- A real free-build region excludes a player inside a different MiniGame and
  does not execute its Enter color action; it observes the free-build builder
  and observes the other player after they leave their match.
- A real owned ball remains observable with mover credit from a MiniGame player,
  but transferring its spawner's ownership excludes it. The fixture uses the
  current ball identity after leaving a match, because canonical match teardown
  can respawn the ball; the first fixture incorrectly retained the old identity.
- 256 native facts queue, overflow reports `256 facts`/`fact skipped`, repeated
  overflow leaves exactly 64 diagnostics, observation drains the queue and the
  next phase can queue again.
- `cargo test --locked -p bri-sim --lib session::rules::tests`: **24 passed**.

The GUI-first-impressions Windows artifact 04 continues from 83939023; it does
not include these later runtime corrections. At the coordinator's instruction
it is not restarted: final v0.2.0 is rebuilt from the combined immutable source.
The branch-only artifact launcher/workflow choices remain the coordinator's
packaging reconciliation responsibility. Workshop has no profile-name gate.

Full affected-crate tests and warnings-denied clippy are recorded below when
complete. This evidence establishes isolation/bounds behavior, not subjective
creator playtest acceptance or a permanent rule architecture.

## Combined-gate fixture correction

The coordinator's full gate found a client preservation test still treating a
60,000 ms row as unsupported. The GUI's documented alpha limit is 300,000 ms.
The fixture now includes an editable row at the supported cap. Above-cap
inspection and submission are explicitly rejected, because brick inspection
already enforces the same world cap: that invalid row cannot legally appear as
preserved. Unknown output vocabulary and a preserved unknown imported input
retain checks against dropping, duplication, enabled/text/token modification
and unchanged round-trip preservation. The supported long row is
explicitly editable, can be changed to 60,000 ms and sent, and leaves unsupported
rows untouched. This changes only the test fixture, not the supported cap.

The coordinator owns default package/toolbelt reconciliation: physics examples
remain available but opt-in in production; Toys remains enabled without
replacing the standard toolbelt. No package, launcher or workflow edits here.

Full affected event/UI/simulation/world tests after the runtime corrections:
`cargo test --locked -p bri-events -p bri-ui -p bri-sim -p bri-world --lib --tests`
reports **876 passed, zero failed, 137 ignored**. The same all-targets clippy
command recorded in the UI review passes with warnings denied; it will be
rerun after the client fixture correction.

Final source-review checks before commit:

- `cargo test --locked -p bri-client tool_ui::tests -- --nocapture`: **15 passed,
  zero failed, 3 content-dependent ignored**, including explicit cap rejection
  and the supported long-delay edit/preserved immutability cases.
- `cargo clippy --locked -p bri-events -p bri-ui -p bri-sim -p bri-client -p bri-world -p bri-net --all-targets -- -D warnings` passed after the fixture correction.
- `git diff --check` passed. No production package/workflow files were edited.

The coordinator also requested investigation of the deterministic Slate chaos
bad-frame failure. That investigation is separate from these corrections and
will receive its own source commit/evidence.
