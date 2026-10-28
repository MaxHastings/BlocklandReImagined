# Release bundle validation needs its toolchain first

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
release paths; no game or importer source changes and no content regeneration.

Focused Python release/tool checks, current-main history validation and platform
CI must pass before merging. Re-dispatch build-only release from the resulting
main, verify the run source and let the other working thread retain sole
publication ownership. Maxwell's explicit publication approval was verified in
that thread. No local heavy/GPU tests or visible gameplay while he playtests.
Failure logs and release input receipts remain in ignored
`artifacts/v040-release-ready/`.
