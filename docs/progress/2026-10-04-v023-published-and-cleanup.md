# 2026-10-04 v0.2.3 publication and cleanup

## Published source

[v0.2.3 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.2.3)
was published at `2026-10-04T08:43:26Z` from
`7089836612c1b0201096b815f4219cea7f5399fc`. The release tag is annotated and
immutable. Main advanced through `CARGO_BUILD_JOBS=2 BRI_CONTENT=<main content>
python3 tools/gate.py --push`; both the coordinator and pre-push hook used the
same exact-commit passing gate receipt. [PR #22](https://github.com/MaxHastings/BlocklandReImagined/pull/22)
is merged. The required [Windows CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37184136071)
passed before merging or publication, including the previously failing synthetic
brick audit and every discovered lighting group.

Unauthenticated GitHub API reads confirm this is the latest public release,
with exactly three platform archives and `SHA256SUMS`. The `ci-content` and
`addon-bundle` releases remain private drafts. The final source differs from
the gameplay wind-down candidate `55b1f058` only in offscreen tests and their
progress note; no runtime behavior was added after the wind-down.

## Verification and its limits

The final local content-backed gate passed in 527 seconds: build, strict Clippy,
content startup and 351 test binaries including ignored native checks. One
native tool-render check reported “GPU recreation changed static third-person
item render” in the pooled run and passed its isolated retry under the existing
flake policy. No assertion, threshold, watchdog or known-failure waiver changed.
This flake is recorded rather than counted as an initially clean run.
Receipt: `../.bri-gate/logs/7089836612c1.log`.

The default external save corpus skipped because its configured source folder
is absent. A separate run against the available original Steam saves passed
1/23: the 6,275-brick A.T.C. Fort. The other 22 source fixtures are absent here;
no complete corpus pass is claimed. That run used `55b1f058`, whose production
loading/runtime source is unchanged in the final commit.

The earlier [Windows check](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37178912923)
failed a synthetic audit GPU wait and the combined lighting binary's cumulative
600-second deadline. Nothing was published on that result. The
[test-only correction record](2026-10-04-v023-windows-gpu-verification-blockers.md)
explains the mechanical eight-group split retaining all 20 test bodies/attributes,
and synthetic 640×400 audits retaining native 1280×800 reference comparisons.
All 20 lighting tests and six brick audits passed locally. A separate GPT-6.1
Sol High review verified inventory, dimensions and unchanged coverage; final
Windows CI confirms the release verification blockers no longer occur on that
run. This does not establish a cause or fix for the user's gameplay hitch.

## Exact-source archives

All three platform workflows passed at the immutable final source:
[Windows](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37184141177),
[macOS](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37185634377),
[Linux](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37185635700).
Each checked content and startup from its extracted package. Final archive
checks verify CRCs, unique safe paths, manifest file sizes/SHA-256 values and
version. All 9,388 common content files have identical paths/hashes across
platforms, excluding the platform-specific package configuration. Seven shared
guides/credits agree after newline normalization. Published asset sizes/digests
match the locally verified archives and checksums.

The downloaded Mac app also passed a local headless startup check and reports
`v0.2.3 (708983661)`. No visible game or interactive play session was automated.
The Mac app targets Apple silicon and is ad-hoc signed; Linux targets
x86-64/glibc 2.35 or newer. Native debug-symbol artifacts from these builds are
retained by each workflow for 90 days; Mac packaging checks dSYM UUIDs against
the extracted executables.

| Archive | Bytes | SHA-256 |
|---|---:|---|
| Windows | 136,208,258 | `8f65f8d7a43fc862b42f15f913f49c419c447de94ef6379f2f1e8019af633144` |
| macOS | 124,101,014 | `d260b9091dcf691a033374e533ec69529faa7de9fe62d93d07b2ff28122a8276` |
| Linux | 140,699,636 | `b7a3ee25435154ee9a011fd88d80acd761e959edd79b6891e09ae07186f858e6` |

## Cleanup after publication

The useful borrowed-Add-On brick handoff from the remaining Claude branch is
integrated, including the independent stable-binding correction. Its other
commit only merged earlier main. Both remaining non-main GitHub branches were
retired atomically with exact-tip leases after verifying their current tips.
Main is now the only local/remote branch; all release tags remain. The complete
recoverable branch bundle is
`dist/v0.2.3-evidence/final-branch-retirement.bundle`, 16,955,070 bytes,
SHA-256 `898564a77f6b6a10605191a30a3d9f8ab46238b627e5c139ba32d8947423a43f`.

Removed 27 explicitly inventoried obsolete generated paths: old downloadable
archives/extractions, superseded draft archives, converted bundle backups and
temporary native startup-check copies. Their pre-deletion allocations total
3.024 GiB; this is not a guaranteed APFS disk-free delta. Current archives,
content/private bundles, original installations, saves/preferences, research
and verification evidence, retained crash symbols, main/gate build targets and
the unrelated untracked Mac setup note remain. The dedicated gate worktree is
infrastructure, not an abandoned feature checkout. Windows PC storage is not
accessible from this session; no Windows-machine cleanup is claimed.

STATUS, the documentation index, features, known issues and release/work-plan
entry points now identify the published version and distinguish historical
plans from current results. Shark's verified capture death-type/icon checks
replace the stale incomplete-icon claim; broader creature behavior stays partial.
This documentation-only main follow-up does not change the release tag or
packaged gameplay source. Machine-readable publication, branch, archive and
cleanup receipts remain under `dist/v0.2.3-evidence/` and
`dist/v0.2.3-release-assets/`.

The Windows firefight NaN crash, reported death disconnect, destruction/respawn
hitch and tank retreat behavior remain unreproduced. No causal fixes or Windows
performance gains are claimed for those reports. Arbitrary scripts, jet-assisted
carrying, high-hoop throws, undeclared Add-On semantics and full Shark/Zombie/
lighting fidelity remain outside verified coverage. Interactive acceptance
belongs to Maxwell; the full alpha contract remains open.
