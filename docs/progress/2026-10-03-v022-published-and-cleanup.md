# 2026-10-03 v0.2.2 publication and cleanup

## Published source and validation

[v0.2.2 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.2.2)
was published at 2026-10-04T00:27:53Z, from
`e3c9010fccea923351c2968e52f240eca7f1f00b`. Main was advanced through
`python3 tools/gate.py --push`; PR #21 is merged and the annotated release tag
resolves to the same source. Public unauthenticated GitHub API reads verified
that this is the latest public release and that all four assets are present.
The `ci-content` and `addon-bundle` releases remain private drafts.

The final content-backed gate passed in 574 seconds: build, strict Clippy,
startup/content checks and all 340 test binaries (466 seconds for tests).
The startup check found 14 maps, 963 bricks, 35 pictures, 14 modern sidecars
and no Add-On health problems. Original save-corpus coverage was explicitly
skipped because its source folder is unavailable on this Mac; no such result
is claimed. Gate receipt: `../.bri-gate/logs/e3c9010fccea.log`.

Max's final instruction, “Lets just publish now god damn”, authorized publication
while the required Windows CI test suite was still running. Windows build,
format, tool tests and strict Clippy had already passed; the full CI conclusion
was not claimed at publication. The push used the full local gate without
bypassing its hook; GitHub reported the account's authorized status-check bypass.
[Windows CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37161683721)
and the subsequent main runs retain their real results.

The three platform packaging workflows passed from the exact release source:
[Windows](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37161691786),
[macOS](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37162670320),
[Linux](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37162671605).
All ZIP CRCs, safe/unique paths, listed file sizes/SHA-256 values and source/version
stamps passed. All 9,379 common content files are byte-identical across platforms;
seven packaged guides/credits match after newline normalization. Debug symbols
match released PE GUID/age, Mach-O UUID and ELF build IDs and remain retained.

| Archive | Bytes | SHA-256 |
|---|---:|---|
| Windows | 136,017,708 | `3e2095a1114d5fb14ed84b7167c5329331efa93027a4f8467481f4edbd8d7dea` |
| macOS | 123,977,090 | `5eb343e324d37a0acc2979ad8b20edcf8cbae69fe938327449b6bab1372296a8` |
| Linux | 140,532,429 | `f33f4bf09944b713c7e2b9e63b4788d259221ac6306150e8ba13a9378223f067` |

The late Duplicator effects report exposed a reproduced stale GPU particle-atlas
reload defect. Successful Add-On installation now refreshes the atlas before
main/secondary rendering; failed/cancelled reloads retain the existing state.
GPU and real package-install regressions pass. The submitted Windows log lacked
producer/reload details, so this is a verified matching failure mechanism rather
than a claim that every possible effects crash is cured. See the
[diagnosis and final corrections](2026-10-03-v022-final-gate-and-effects-crash.md).

## Cleanup completed after publication

The [branch audit](2026-10-03-v022-branch-retirement-audit.md) found no useful old
runtime handoff left to merge. Before retirement, a complete 57-ref Git bundle
was verified and every live remote tip matched the reviewed inventory.
`dist/v0.2.2/branch-archive/published-pre-cleanup.bundle` is 16,923,113 bytes,
SHA-256 `aa8b17e196197fe0339b2a887ff08d51450c9b3bbe7e0596d25e45975d9376ef`.
Sixteen obsolete GitHub branches were deleted atomically using exact tip leases.
The only remaining remote/local branch is main; release tags remain intact.
Eight local branches and two completed worktrees were removed after backing up
uncommitted Workshop wording, 149 GUI evidence files, lighting/crash evidence,
compressed private snapshot inputs and their scripts/logs. Generated extracted
snapshot copies were discarded; their full hash inventory was retained.

Nine obsolete local package/download paths were removed. Together with the two
worktrees, the removed directories/files accounted for 4.66 GiB of
allocated storage before deletion. This is not a guaranteed disk-free delta:
APFS allocation and retained evidence backups differ. The primary build target,
dedicated gate target, original installations, current content, saves, current
release archives/symbols and the unrelated untracked Mac setup note remain.
Windows PC storage is not accessible from this session; no Windows cleanup is
claimed. Optional old temporary benchmark inputs remain as evidence.
Machine-readable receipts are retained in `dist/v0.2.2/`.

The doc cleanup corrects the native render budget, compressed save representation,
budgeted loading and damaged-save isolation. The documentation index points to
current events/bots and labels older interaction/progress pages as historical.
STATUS now points to the public release and keeps experimental/fidelity/performance
limits visible. The release tag is immutable; this documentation-only follow-up
is a later main commit and does not change packaged gameplay code.

Remaining limits are intentional and recorded in the packaged known issues and
mutation guide. Bulb/material polish remains deferred by Max. Human playtesting
belongs to Max; no interactive game controls were automated.
