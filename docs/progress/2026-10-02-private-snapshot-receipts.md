# 2026-10-02 Private content snapshot receipts and verified handoff

Source commit: `c4c0f484e` on `codex/content-reload`, based on `792f65c6`.
All source work remains unlanded for the release coordinator's requested review
and combined gate. Root owns workflow/software-rendering integration; preserve
its newer changes when reconciling our fixture edits and Python test discovery.

## Final validation

- Full headless Tutorial walkthrough passes in 172.38 seconds, completing all
  fifteen covered goals and hitting 58/58 target-practice targets. This exercises
  valid `StartTutorial` continuation after the async worker. The initial Bricks
  timeout was reproduced with the previous gate executable; the test clicked
  at y=767 in a 720-pixel viewport. Search plus the Buy button corrects the test
  without changing or skipping a lesson.
- Final real bundled shared-geometry/door host test passes: actual imported
  Bot_Hole/Bot_Shark geometry and open/closed door footprints and sound references.
- Final workspace Clippy with `--all-targets --locked -- -D warnings`, formatting,
  diff whitespace check and 14 Python regression tests pass.
- Earlier final reload (8), importer installed (9), role helper (1), default
  Add-On (4) and download/join (10) results are in the preceding entry. Valid
  HostGame, LAN guest, downloaded JoinServer and Tutorial continuation are covered.
- Fresh archive installation validates all 87 pinned originals, 132 default and
  companion folders, 65 enabled packages and 1,651 native geometry identities.
  Startup check passes: 14 maps, 963 brick definitions, 35/35 pictures; no Add-On
  health problems. No visible game or OS input was used.

## Uploaded private drafts

Both releases were checked after upload as `isDraft: true`, with one asset each.
Both assets were downloaded again; SHA-256 matches the local archive exactly.
No public release was created or published.

- `ci-content.zip`: 50,845,310 bytes; SHA-256
  `c2cd6ef0d0057b8259dc531c67131c9e13b567f1a57b30df6652b9aa0c78ec25`.
  Metadata source `c4c0f484e`; 18 base packs, 2,979 files. The integrity manifest
  is verified again after downloading. This archive contains no packages.json
  override, so extracting its packs preserves the player's package selection.
- `addon-bundle.zip`: 35,748,665 bytes; SHA-256
  `74d081a62a8c9a9b5c6324971ca78db493891a1f7b24ce41553a8ef6408c7eb9`.
  Metadata `refreshed_at_commit` is the full source commit; all 87 originals and
  source credits remain. The metadata states v0.1.15 recovery, Brick_Doors-only
  original-source reimport, 13 embedded-ID repairs and absent full source inputs.

The earlier CI archive remains backed up under this worktree's
`artifacts/ci-snapshot-before/ci-content.zip`. Generated content and archives
are ignored, not committed.

## Combined-gate content handoff

All paths below start at:
`/Users/maxhastings/Documents/BlocklandReImagined-worktrees/content-reload`.

- Final independently extracted and checked content:
  `artifacts/addon-snapshot-after/content-roundtrip`.
- Final independently extracted originals/companions bundle:
  `artifacts/addon-snapshot-after/bundle-roundtrip`.
- Final archives:
  `artifacts/ci-snapshot-after/ci-content.zip` and
  `artifacts/addon-snapshot-after/addon-bundle.zip`.
- Uploaded/downloaded byte verification:
  `artifacts/snapshot-download-verification/`.
- Fresh startup log:
  `artifacts/addon-snapshot-after/roundtrip-check.log`.

The gate uses the **primary checkout's content**, not this worktree's.
Before gating, root should verify/extract the final CI archive's base packs
into `/Users/maxhastings/Documents/BlockReImagined/content`, preserving
packages.json, then run the coordinated checkout's
`python tools/addon_bundle.py install --bundle <bundle-roundtrip absolute path>
--content /Users/maxhastings/Documents/BlockReImagined/content`.
The installer targets bundled originals/companions; preserve and integrate the
bot lane's own repository packages separately. Do not replace the whole primary
content tree or selection list with our staging tree. Recheck primary startup
before the combined gate. No primary content or source files were modified by
this lane during staging.

Temporary command logs are `/tmp/bri-final-{clippy,fmt,python,doors,snapshots,roundtrip}.log`,
`/tmp/bri-tutorial-final.log`, `/tmp/bri-reload-tests.log`,
`/tmp/bri-door-import-tests.log`, `/tmp/bri-default-addons-tests.log` and
`/tmp/bri-addon-join-tests.log`. They and generated artifacts can be retained
before removing this worktree after landing. Its target is not cleaned until
then. No private saves corpus is available here; no interactive or cross-platform
acceptance is claimed. Final combined gate, Windows checks and main push remain
with the coordinator.
