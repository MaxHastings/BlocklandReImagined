# 2026-10-09 PR #36: jet history survives skipped presentation

Follow-up on `codex/jet-release-animation` for Maxwell's review of
`936a2536`. This supersedes the presentation-owned history in
`2026-10-09-jet-release-animation-fix.md`. Maxwell will send the updated PR
to Claude for v0.2.8 review and merging; this thread does not merge it.

## Finding and correction

The review is valid: an avatar that never posed an active jet tick still
picked jump on the first released-but-rising state. A new native/synthetic
regression failed on `936a2536` with `jump` instead of `root` in both cases.
It uses an actual floor, eight Torque ticks of jets, release while still
rising, and no intervening avatar poses or remote snapshots. A third mesh
first sees the remote only after release.

The shared player motor now records `JumpState::jet_flight` after every
movement tick, using accepted jets (after energy/player-type gates). It
retains the history through release until landing or an actual jump.
Teleport, seat placement and a scripted grounded state also clear it.
Owner corrections carry the field with the existing movement state, so
prediction restoration/replay preserves it.

Compact remote poses use spare bit 8 in their existing flags byte. This
preserves flight history after sampled/lost jet ticks without enlarging
that datagram item. Pose-change detection includes history changes.
`jet-flight-history.md` adds the required protocol-version increment;
host and client must use the same version. No physics values or controls
changed. Avatar posing now reads this authoritative history directly and
has no separate flight latch to become stale after a missed landing.

The new regression also skips all landing poses, then checks the next real
jump locally and remotely. Existing checks still cover held tools, outfit
rebuilds, simulated/rendered flag disagreement, falling and respawn.

## Validation

All commands use the existing worktree with `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_INCREMENTAL=0`, and no `CARGO_TARGET_DIR`. Native tests read the main
checkout's generated content via `BRI_CONTENT`.

- `cargo test -p bri-client --lib jet_history_survives -- --include-ignored
  --nocapture` on the original PR implementation: both synthetic/native
  variants failed at the unwanted jump selection.
- `cargo test -p bri-client --lib avatar::tests::jet -- --include-ignored
  --nocapture` with motor/replicated history: all six checks passed.
- Ran the rebuilt client unit-test binary with `avatar::tests`: the existing
  avatar suite and the added synthetic check passed; native/GPU checks stayed
  ignored except for the six focused checks above.
- `cargo test -p bri-net --lib released_jet_flight_history -- --nocapture`:
  passed. Remote history survives actual compact encoding/decoding with jets
  off and no extra bytes; owner corrections round-trip the entire movement
  state, and history changes are counted by pose-change detection.
- File-scoped rustfmt and `git diff --check`: passed.
- `cargo clippy -p bri-client -p bri-net -p bri-motor --lib --tests --
  -D warnings`: passed.

No visible game window or gameplay input was automated. Maxwell's
third-person confirmation and the full alpha contract remain open.
