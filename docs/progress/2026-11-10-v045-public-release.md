# 2026-11-10 v0.4.5 public release

Published [v0.4.5 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.4.5) at 2026-11-10T18:34:26Z from
`befa91e18a9560a5863fd44643bc21f57c3c28d0`. The public latest-release endpoint returns v0.4.5. Anonymous downloads
of all three ZIPs match SHA256SUMS and the independently verified native artifacts;
the annotated tag resolves to the exact tested source. Public release-note links
point to the versioned source. Published archives are retained unchanged.

[PR 442](https://github.com/MaxHastings/BlocklandReImagined/pull/442) and the
[generalized integration, PR 443](https://github.com/MaxHastings/BlocklandReImagined/pull/443)
are merged; [issue 428](https://github.com/MaxHastings/BlocklandReImagined/issues/428)
is closed. See [release notes](../releases/v0.4.5.md),
[API migration](../modding/v045-migration.md) and
[one-folder/ZIP authoring](../modding/co-located-addons.md).

## Verification and retained limits

Final exact-tree scoped gate: 4,089 passed, zero failed, 352 ignored across 302
result groups (including doc-test groups). Scope was changed crates/direct
consumers, their relevant integrations and selected consumer unit tests, plus
formatting, warning-free clippy, Python tool tests and documentation checks.
Commands: `python tools/gate.py --jobs 4`, then the required locked
`python tools/gate.py --push --jobs 4`; the pre-push hook remained enabled and
reused its locked parent's exact commit/base proof. These were scoped passes,
not new clean extended-gate passes. The isolated gate used
BRI_GATE_DIR=C:/Users/Maxwell/Desktop/Games/BlocklandReImagined-worktrees/v044-release-gate,
BRI_GATE_CONTENT=E:/Downloads/BlocklandReImagined-Release-Evidence/v045-2026-11-10/content
and matching BRI_CONTENT; CARGO_TARGET_DIR remained unset.

Earlier extended release validation ran 532 binaries and passed whole-workspace
build/clippy. Initial failures exposed stale private input packs, missing fixture
paths and a stale stock presentation checksum. Published v0.4.4 base inputs were
verified and normal stock-fixture regeneration changed only weapons_sha256;
all other fields and asset bytes were identical. Both affected targets then
passed in full (default_add_ons 4, hosted 3), and the fixed save corpus passed
23/23. No assertions or skips were weakened. One private Tower CTF room case
failed concurrently and passed alone: retained as flaky, with follow-up owned
by the bot/navigation integration harness. Both original failure and corrective
logs remain available. Core performance runs (32 bots, 3,600 ticks) passed:
Collage 6.96 ms against 8 ms; Afghan 4.29 ms against 5.5 ms. These earlier core
runs preceded the final authoring-checker/NPC-policy corrections, whose focused
and consumer checks passed separately.

[Candidate CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/42853272019)
and [main CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/42859286495)
each passed all seven executed jobs. Candidate CI retried an Azure cache
ServerBusy failure before compilation. A delayed duplicate PR run left GitHub's
required-check selector seeing a cancellation during the guarded main push;
GitHub accepted the authorized push with that warning despite the successful
candidate run. The new main check and all main test shards were required to
pass before publication, and did pass. No hook or verification flag was disabled.

[Native build run](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/42854465392)
holds all three platforms at this source. macOS 14's retired runner was replaced
with supported macos-15. macOS initially failed downloading sccache (HTTP 500),
then a retry was cancelled without a runner due to hosted ARM capacity; the
subsequent native run passed. These were infrastructure failures during the
[reported GitHub incident](https://www.githubstatus.com/incidents/hnf21m9jkl3c),
not passing retries of failing software tests. Native package/startup checks
passed, and the extracted Windows ZIP passed `bri-client.exe --check` locally.
Every archive manifest file was independently hashed; all 19 effective base
roles, including three override variants, match across platforms (3,132 base
files). Public downloads were then streamed and hashed independently.

## Inputs and private consumer handoff

All 100 original archives matched unchanged catalog pins and were regenerated
with the current converter recipe. NPC generation produced 118 recordings;
ambience produced four approved clips for 21 maps. Base content came from the
verified published v0.4.4 ZIP/SHA256SUMS and manifest. The designated v20 source
and other original installations remained read-only. No original content,
private Add-On assets or downloaded tools were committed.

The supplied eight private package folders pass authoring checks. Their four
co-located ZIPs pass byte-for-byte packing, transactional replacement installation
(including npc_popculture_rules's independent ID), and real headless host checks:
ordinary light mounting, exported state, Render claim/mute/name-tag/self-lit
presentation and cleanup without diagnostics. Static case checking found no
mismatch among 476 resolved JSON file references. The updated private handoff is
v045-private-single-zip-addons.zip, with migration README and file/hash receipts,
in the external evidence directory. The temporary observer initially carried an
invalid client-code marker; it was removed, its manifest passed the authoring
checker, and the final installed-ZIP/host rerun passed in 9.26 seconds. Those
private consumers are not new bundled
content. Bundled NPC reaction policy 1.0.2 honors mute at queue and delivery.

This is technical validation, not interactive playtest or source-fidelity
acceptance. The author's private harness was not attached. Full alpha acceptance
remains incomplete, as do the retained known issues. Maxwell requested publication
without an interactive playtest.

## Public archive hashes

| Archive | Bytes | SHA256 |
| --- | ---: | --- |
| BlocklandReImagined-linux.zip | 259,214,133 | `bf85067c8f7c46d96cb2d3d958702bbc63dd01469973f18102e4b93519aa4d41` |
| BlocklandReImagined-macos.zip | 231,226,159 | `2297468cb5a07aaa872576fe2ad1a7f8c6d35eb6356d119eb8ae32f4046f09c9` |
| BlocklandReImagined-windows.zip | 261,798,879 | `d279772acc03d4de27862143ffb88b6c2c9cd2ea94cf384674cf317e5622d5ee` |

## Preservation and cleanup

Primary checkout reconciliation preserved 84 local files in the SHA-verified
Codex archive; the unrelated unmerged jeep-test branch remains intact. The
reused worktree's prior ignored files were archived separately: 79,696 files,
3,907,388,442 input bytes, every archived file verified. Its ZIP SHA256 is
`da59573632e5eb53a609a156540f80df8ade894417372f9d3de3f9e72837ea7d`.
Owned worktree build outputs were removed after publication; the primary
checkout's target and generated content were preserved. Automatic safety review rejected recursive checkout-folder deletion as
"blocked by policy". The repository bounded cleaner and explicit Cargo cleanup
removed about 97 GiB of owned build output instead. The retired checkout
folders remain with their source/ignored files and verified backups; other
threads' outputs were kept. Exact phase receipts are retained externally.

Exact commands, first failures, inputs, private test source, check reports,
manifest proofs and public-download verification are retained under
`E:/Downloads/BlocklandReImagined-Release-Evidence/v045-2026-11-10`.
