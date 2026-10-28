# Release bundle prerequisites and portable source identity

The approved combined v0.4.0 candidate's code matches main
`67d7e3da58af2e9fea643f9ae56469a7f3b4fd43`. Build-only release run
40787726867 stopped on all three platforms before compilation. Windows logs
showed Cargo metadata could not execute `sccache rustc -vV`: release workflows
set RUSTC_WRAPPER globally but installed the wrapper after Add-On bundle fetch.
The new generation check needs Cargo's converter dependency graph during fetch.

Move the existing Rust/cache/sccache setup before content fetch in Windows,
macOS and Linux workflows. Preserve the complete recipe/inventory validation,
source pins and generated assets. This changes workflow ordering only, with a
regression checking those dependencies precede bundle validation in all three
release paths. No game or native importer source changes.

A follow-up source-order diagnostic also exposed a genuine cross-platform
recipe mismatch hidden behind that first failure: Path ordering case-folds on
Windows. Cargo.toml sorts after base-packages.json/build.rs in two converter
dependencies there, but before them on Unix. Source tree fingerprints now sort
explicit relative POSIX strings and retain newline normalization. An invented
mixed-case/nested fixture checks the exact digest across line endings. Rebuild
the native bundle with the unchanged existing importer and pinned inputs to
produce genuinely regenerated outputs and portable receipts; do not restamp
old files or bypass generation validation.

The regenerated bundle's 10,048 payload files match the approved candidate
byte-for-byte excluding only generation receipts. All 164 shipping packages
validate. A remote round trip passed and macOS release run 40789661566 passed
the actual bundle fetch/portable recipe check. Linux reached inventory hashing
but Ubuntu 22.04's default Python lacks hashlib.file_digest (Python 3.11+).
Pin Python 3.12 before fetch on every release platform; the regression now
checks Python's minimum version as well as Cargo/wrapper setup order. Cancel
the superseded run and rebuild once from the corrected workflow commit. These
workflow-only changes do not alter the portable recipe or require reimporting.

Focused Python release/tool checks, current-main history validation and platform
CI must pass before merging. Re-dispatch build-only release from the resulting
main, verify the run source and let the other working thread retain sole
publication ownership. Maxwell's explicit publication approval was verified in
that thread. No local heavy/GPU tests or visible gameplay while he playtests.
Failure logs and release input receipts remain in ignored
`artifacts/v040-release-ready/`.
