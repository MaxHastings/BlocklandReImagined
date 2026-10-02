# 2026-10-02 Content reload, fixtures and private snapshot repair

Follow-up to the five reported bugs and subsequent main fixes. Branch
`codex/content-reload` starts at `792f65c6`. The release coordinator requested
an unlanded, committed handoff for combined review/gating; no public release
or main push is performed by this lane before that review.

## Changes

- Generated-content fixtures resolve declared package roles from their explicit
  content root instead of keeping importer revision folder names. Synthetic
  fixtures still control their own artificial paths. The shared helper has a
  two-root, renamed-directory test. Package ownership remains authoritative.
- Add-On selection, imports and downloaded server content prepare on one worker
  and install complete content/catalog/audio/derived assets on the UI thread.
  A newer request supersedes old preparation and keeps waiting actions; cancelled
  remote joins cannot install server content. Host, Tutorial and Join actions
  resume only for the current UI session. Workers transfer owned UI schemas,
  preserving UI-local `Rc` ownership without unsafe code. Explicit synchronous
  tool overrides remain supported. Reimports reload even when the list matches.
- Sound preparation decodes off-thread; UI installation only swaps ready assets.
  Failed preparation preserves the loaded content and answers waiting actions.
  Failure to read a local package list answers the UI instead of aborting its pump.
- Private CI snapshots now record and validate every archived file's SHA-256,
  exact membership, safe paths and nonempty pack folders. Both content upload
  paths refuse published releases. Gate and CI discover both Python test files.
- Brick_Doors has a source-hash-pinned native port: native swaps and sounds replace
  Torque Support_Doors. An applied port must explicitly handle the dependency and
  remove its manifest requirement before the import report marks it ported. The
  original script call and resolution evidence remain in that report. Unmatched
  source copies retain their requirement.
- The Tutorial walkthrough used offscreen icon coordinates after Add-Ons extended
  the first brick section. The previous gate executable reproduces the Bricks
  timeout. The fixture now searches through the real field and purchases through
  the real Buy button, without a window or OS input.

## Content evidence and limitations

The full v20 reference and original Add-On source collection are absent on this
Mac. Restore all 87 pinned originals from the verified shipped v0.1.15 Windows
release; no package is omitted. Reimport only Brick_Doors from its verified
archive (SHA-256 `0df1cdd215dbe4dca0f57924ed752204b58fab106d0fc36d72dbc1fee1c19de2`).
Normalize 13 legacy embedded native mesh identities against their authoritative
content index (giantbrickpackv2 and demianscbfix), preserving geometry and art.
Keep original credits and source provenance. Bundle metadata explicitly states
that this is recovery with targeted repair, not a full original-source reimport.

The staged combined content root is:
`/Users/maxhastings/Documents/BlocklandReImagined-worktrees/content-reload/artifacts/addon-snapshot-after/content-verified`.
It contains 18 base packs, 132 default/companion folders, 65 enabled packages and
1,651 verified native geometry identities. The final current client `--check`
reports 14 maps, 963 brick definitions, 35/35 thumbnails and no Add-On health
problems. The primary checkout is owned by the bot lane and was not modified.
Before the combined gate, sync repaired original/companion content through the
bundle installer into primary content, preserving the bot lane's own packages.
A worktree-only untracked content symlink was used for native tests, never added.

## Validation

`CARGO_TARGET_DIR` remained unset. Local cargo commands use
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`; no interactive playtest occurred.

- Workspace all-target check and clippy with `-D warnings`: pass.
- `cargo fmt --all --check`: pass.
- Python unittest discovery: 14 pass (seven gate, seven snapshot-integrity tests).
- Package role helper: one pass, including precise missing-role failure.
- Client reload unit tests: eight pass; pending/latest choices, stopped/failed
  worker, cancellation, cancelled remote superseded by local choices, failed
  downloaded-content fallback, tool overrides and UI parse-error handling.
- Importer installed tests: nine pass, including native door swaps, independent
  identities and matched/unmatched native door framework dependency reports.
- `default_add_ons --include-ignored`: four pass, including fresh-checkout host,
  default plane, LAN guest and actual bundled shared geometry/door footprints.
- `add_on_join --include-ignored`: ten pass, including downloaded-content rejoin,
  synthetic and converted vehicles, bot selection and all repository Add-Ons.

During development a tool-choice regression restored packages.json instead of
an explicitly supplied tool set; the new regression test and successful join
suite cover its correction. The initial default-screen wait used a zero game
budget and was corrected. Tutorial initially stopped at Bricks, reproduced by
the previous executable; its offscreen click and search-field keyboard focus
were diagnosed rather than suppressed. Full walkthrough result and final
snapshot upload receipts follow in the next entry.

The combined main gate and platform release checks belong to the coordinator.
The private Maxwell saves corpus is unavailable on this Mac; its previously
recorded explicit skip remains a limitation, not an acceptance pass. Alpha
handoff and interactive acceptance remain incomplete.
