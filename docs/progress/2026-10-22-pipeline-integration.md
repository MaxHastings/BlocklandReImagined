# 2026-10-22 Pipeline integration

Maxwell authorized resuming builds/tests, integrating Pipeline, merging verified
work into main and cleaning finished work. The previous overnight build hold is
superseded. Interactive playtests remain his.

Integration starts from engine main `8a9184fac`. It combines #276 (128 control
binds, alternate defaults and authoritative crouch), #277 (map-volume bounds
and tie-breaking), #288 (broken replacement models and overlapping shot
colliders), and #291 (client-owned map particle sources). Existing ownership
rows in `docs/architecture/seams.md` identify these mechanisms. Package policy,
original content, extraction and source-specific acceptance stay private.

Review of #291 found that replacing map emitters forgot their handles without
stopping their looping sources. Added an integration regression using two
different emitters at the same node, map removal and checkpoint reset. It
failed before the fix (old source remained), then passed after stopping replaced
sources with Drain. Disconnect now also clears map declarations; ordinary
effect-session resets restart only the current map.

Validation so far: the quick tier passed 16 steps in 643 seconds; the complete
actor-effects target passed 15 enabled tests with its one existing ignored test
retained for the full gate. `cargo build --locked -p bri-client -p bri-render
-p bri-addon-import --bin bri-client --bin prepare_lighting --bin
bri-import-addon` passed. Focused checks and combined gate results will be
recorded after they complete. Logs are in the existing engine worktree's ignored
`.research/pipeline-integration/` directory.

The existing engine worktree `aa-engine-main` was reused. The original local
capture/lighting experiment tip `43ece5f76` remains preserved, and its test-only
experiments are excluded from this production integration. No new worktree was
created. A Git 2.12 command incompatibility during branch setup was corrected;
the original local branch ref was restored and integration restarted from
current main before validation.

The private Pipeline assembly was independently regenerated against this engine
candidate. Its control builder now consumes the larger bind limit, its map
emitter conversion preserves source placement midpoints, and assembly prepares
and validates the room-lighting sidecar. An unfinished ledger syntax error and
the builder's failure to propagate ledger errors were corrected privately.
Static package paths, weapon/presentation hashes, 54 binds and ten scene/effect
references pass; those checks alone do not establish gameplay fidelity.

Outstanding acceptance includes the ordinary Pipeline match/objective loop and
Maxwell's terrain/building overlap and bright stair/catwalk strip reports. This
integration does not claim full AA fidelity, finish Interdiction, publish a
release or close the broader alpha contract.

Focused verification passed: 11 steps in 1,273 seconds, including Clippy
with warnings denied for all eight changed crates, tests of 28 dependent
crates, and client/render unit tests. The full integrated gate is next.

An initial gate invocation was stopped after discovering the commit had not
been created: Git 2.12 requires --add for a new path in update-index. The
corrected staging/commit precedes the authoritative gate run below.
