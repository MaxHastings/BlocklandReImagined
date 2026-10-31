# 2026-10-30 Next release execution readiness

Maxwell asked whether any preliminary planning remained before approving a steady
step-by-step implementation and release. Completed a bounded execution readiness
pass and added the [execution checklist](../plans/2026-10-30-next-release-execution.md).
The detailed plan remains pending approval; no runtime implementation began.

Verified current main is `9390c39cf`; pinned Rust 1.93.1 MSVC/cargo and active
toolchain respond, Python/PowerShell 7/git are present, sccache responds and is the
configured rustc-wrapper, CARGO_TARGET_DIR is unset, and the default pre-push hook
exists. These checks are not a compilation or full Windows SDK/link verification.
Approximately 1,149 GiB on C: and 10,203 GiB on E: are available at inspection.

Inspected registered worktrees, cleanliness, target presence and ancestry. The
historical main checkout is dirty, the gate checkout/cache is reserved, voip-review
has unmerged stock-gun-policy work and the clean detached docs-audit checkout is
separate/outside the prescribed root. None was repurposed. At implementation start,
recheck for an idle merged canonical checkout; otherwise create one canonical
worktree and retain warm outputs. No new worktree or cargo build was created here.

Preserved 29 private evidence files (136.4 MiB) under
`E:/Downloads/BlocklandReImagined-Release-Evidence/planning-2026-10-30`:
five diagnostic saves, recordings 131–135, six installation logs, five supplied
screenshots, Bot Add-Ons/Bot_Town source ZIPs, both matching installed v21 default
shaders, the two BadDream attachment ZIPs and the private extractor/results.
`INPUTS.json` records original source paths/URLs, relative snapshot paths, bytes
and SHA256. Every copied/downloaded file was hash-checked; downloads match the
previously recorded attachment hashes. Originals remain unchanged and private
content never enters Git or public packaging. Existing destination conflicts are
refused rather than overwritten.

The checklist now states the dependency order, closure evidence per increment,
private-preview cadence, scoped versus final-release checks, scope triage and
publication authority. Root can continue independent work while a live diagnostic
capture is pending; a missing reproduction does not justify a false fix or silently
dropping acceptance. Product choices already resolved need no repeated approvals.

Actual commands: git worktree/status/merge-base/ls-remote/show/config/rev-parse;
cargo/rustc versions; rustup active-toolchain; sccache --show-stats; read-only input
presence and disk checks; Python hash-verified evidence copies and bounded attachment
downloads. No game, audio playback, GPU test, soak, benchmark, build or gate ran.
Remaining prerequisite is Maxwell's approval, then isolated checkout setup and the
first diagnostic/implementation increment.
