# 2026-10-16 v0.3.2 reviewed animation allocator input sync

Root accepted D2b-1 `59b0693e1d8a7c4f48d151d6b437e0a3f6f9a992`, then
granted only cherry-pick of reviewed weapons checkpoint
`9f233b3fa11d3944f558fe429a53dd4b19abb238` and validation of the combination.
Clean branch check preceded the pick. It applied without conflicts as
`87390ecfea5c24cb3fe48e1da88eb1435db525f9`, directly parented by accepted59b.
This is the immutable combined code pin; subsequent own receipt edits add no
runtime changes. Work remains isolated on codex/v032-engine-seams, with no
main/v0.3.1 merge or publication.

Inspected the exact three-path diff: weapons runtime/persistence.rs,
weapons/tests/animation_identity.rs and the original author's progress entry.
Resulting blobs for all three paths compare identically with9f233b3f. Its note
remains unchanged. No root manifests, lockfile or other production path changed.
The existing allocator watermark is now saved/restored, including removed
instances and queued bindings, with validation for duplicates, inconsistent
stamps and exhaustion. Weapon checkpoint schema4 rejects earlier alpha schemas;
there is no default or migration. AnimationStart, content/wire schemas and D2b-1
behavior remain unchanged. This resolves the previously queued schema3
allocator-restore omission through the reviewed input, not a local mechanism.

Original unchanged trajectory command:
`cargo test -p bri-weapons --test runtime save_restore_preserves_authoritative_trajectory_and_image_clock --locked -- --include-ignored`.
The first attempt passed synthetic and failed content because this worktree has
no content root (`target/d2b1-sync-trajectory.log`); no equality oracle failed.
Selected the weapons role from this branch's base-packages.json and used the
test's existing process-local BRI_WEAPONS_PACK override to read
`C:/Users/Maxwell/Desktop/Games/BlocklandReImagined/content/weapons-pack-009/weapons.json`.
SHA256 before and after was
`7BA8B577A0F290CCA08523237F679393D0F965C1D0916B08AA2B7AAC978E298A`.
No content was copied, regenerated or written. The rerun passed both synthetic
and converted-content cases:2 passed/0 failed/0 ignored
(`target/d2b1-sync-trajectory-content.log`). Complete future event and subsequent
checkpoint equality assertions remain unchanged.

Other targeted validation:

- `cargo test -p bri-weapons --test animation_identity --locked`:7 passed,
  zero failed/ignored (`target/d2b1-sync-identity.log`), including deferred
  provenance, removed-instance high-water mark, malformed/old schema rejection
  and exhausted allocator preservation.
- `cargo test -p bri-sim --test retained_animation_pose --test authoritative_animation_completion --test evaluated_animation_phase --test body_pose_assets --locked`:
  42 passed, zero failed/ignored (retention16, clock15, D2a9, D1body2;
  `target/d2b1-sync-pose-controls.log`).
- `cargo test -p bri-sim --lib retention_tests --locked`:3 passed, zero failed,
  407 filtered (`target/d2b1-sync-revision-controls.log`).
- `python tools/check.py quick --crates bri-weapons`: formatting plus13 unit
  tests passed, zero failed/ignored,6s (`target/d2b1-sync-quick.log`).

`python tools/check.py focused --crates bri-weapons` passed all11 steps in1281s,
including weapons all-target Clippy with warnings denied and the normal dependent
closure (`target/d2b1-sync-focused.log`). Across241 test summaries:

| Crate | Passed | Failed | Existing ignored |
| --- | ---: | ---: | ---: |
| bri-addon-import | 229 | 0 | 3 |
| bri-chaos | 272 | 0 | 26 |
| bri-client (unit) | 590 | 0 | 76 |
| bri-net | 217 | 0 | 20 |
| bri-package-runtime | 100 | 0 | 0 |
| bri-sim | 1097 | 0 | 126 |
| bri-stresslab | 3 | 0 | 0 |
| bri-weapons | 179 | 0 | 26 |
| bri-weapons-import | 11 | 0 | 1 |
| Total | 2698 | 0 | 278 |

The full sim suite includes the16 retention,15 clock,9 D2a,2 D1 body and three
private boundary controls. Weapons includes all seven identity controls. The
exact converted-content trajectory case was explicitly enabled in the separate
two-case target above; its ordinary tier ignore remains unchanged. All Cargo
commands used process-local CARGO_BUILD_JOBS=2 and RUST_TEST_THREADS=8;
CARGO_TARGET_DIR was confirmed unset. No source edits, new test hooks, weaker
oracles, ignore changes, interrupted builds or full integrated gate. The only
initial target failure was missing worktree content, corrected with the documented
existing override; no runtime failure was waived.

Final whitespace and scope review passed. The reviewed three-path pick remains
byte-identical to9f233b3f on those paths, directly on accepted59b; the only new
working change after87390ecf is this own receipt, staged separately with
`git diff --cached --check` passing for the final immutable handoff.

D2b-2 producers, motor/client extraction and renderer/material mechanisms remain
ungranted. AA PR67's actual tree packet
`689eda57ae14828f654f9916d449094ceb525d64`,
`aa25/notes/pipeline-deferred/snow-trees-followup/v032-handoff.json`, is queued
source evidence only; no renderer work or consumer parity claim is made here.
Root owns review and any next exact input or production grant.
