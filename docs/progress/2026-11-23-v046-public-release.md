# 2026-11-23 v0.4.6 public release

Published [v0.4.6 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.4.6)
at 2026-11-23T22:36:19Z from `93cf9b02565d3a1c4641b93f0a4ad1b90b1c889e`.
The annotated tag resolves to the exact gated source. That source is on main
through `961616b3813f0f4254c4638ac1ef4b92c6eb9955`, whose tree is identical.
All three anonymous downloads match the independently verified Windows x86-64,
Apple-silicon macOS and Linux x86-64 artifacts and the public SHA256SUMS.
The latest-release API returns v0.4.6. The [publication workflow](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/45281168955)
reused the verified build. The two public note links were updated to point to
the tagged source.

[PR 448](https://github.com/MaxHastings/BlocklandReImagined/pull/448) connects the
versioned compiled GLB reader to stock content, Add-On import and runtime consumers.
[PR 454](https://github.com/MaxHastings/BlocklandReImagined/pull/454) integrates
unified filtered sun shadows and the coarse distant map, background voice
activation with focus-sensitive push-to-talk, and group selection in Add-Ons.
[PR 455](https://github.com/MaxHastings/BlocklandReImagined/pull/455) updates the
real-content regression probes to read compiled models and verify the Zombie's
native bot and body. See the [release notes](../releases/v0.4.6.md).

## Validation

- A clean Windows checkout with empty content and target built through
  `python tools/bootstrap.py --v20 <reference>` at the model-pipeline head.
  All 124 checked stock detail tables matched, including the ten Kitchen tree
  levels. Encoding the previous release's same 124 models also yielded
  byte-identical compiled results. The release has 229 compiled base models.
- `python tools/gate.py --release-content --jobs 4 <release-sha>` passed
  5,761 tests in 534 binaries with zero retries, workspace build and
  warning-free Clippy, shader compiler setup, and startup with no Add-Ons
  left out. A separate exact-source `save_corpus` run checked all 23 cases.
  The release-mode budgets passed at 6.82/8.00 ms on Collage and
  4.20/5.50 ms on Afghan with 32 bots and 3,600 ticks per scenario.
- The earlier gate exposed stale JSON readers, an outdated Zombie import
  expectation, and missing private fixtures. The rerun used the filtered v20
  reference, an isolated four-archive community sample, the staged Bot Town
  import with its native Bot Hole dependency, and the actual Beta16 tower save.
  The Slopes synthetic mirror case passed only on retry in the earlier gate
  and passed on the first attempt in the final gate.
- [Candidate CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/45266444822)
  and [main CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/45270541410)
  each passed all seven executed jobs. The history checks passed and the
  pre-push hook stayed enabled.
- The [native build](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/45267180442)
  verified startup and packaging on all three operating systems. Every
  manifest file was hashed independently. All 3,132 base files and 10,302
  Add-On files match across platforms. The extracted Windows ZIP also
  passed startup and the offscreen two-client hosting, Add-On import and
  loopback smoke. The smoke logged the audio-device underrun and reopen
  warning already observed in v0.4.5. Live microphone and visual acceptance
  remain player checks.

## Release inputs

The read-only v20 reference and Steam sources supplied 19 base packs.
The Add-On bundle regenerated all 100 pinned source archives. Approved
ambience previews and cached recordings were verified before rebuilding the
118 NPC recordings and the four ambience clips for 21 maps. Original
installations and other worktrees' existing files were preserved. The
bundles were verified against their private draft asset digests. The
original archives and private fixtures stayed out of the repository.

## Public archives

| Archive | Bytes | SHA256 |
| --- | ---: | --- |
| BlocklandReImagined-linux.zip | 260,825,273 | `1281eee688aaabb1b60a04d2b7953252509f87d3caa4d0f1887b966b5a9877c0` |
| BlocklandReImagined-macos.zip | 232,825,453 | `81647efe5436857a914a658d88b2a6da47d69cb3216cc1e5d2a607a621082769` |
| BlocklandReImagined-windows.zip | 263,708,824 | `c05b0240c44a34a180075e41d40d39304d73c626874c77d531660804165be53f` |
