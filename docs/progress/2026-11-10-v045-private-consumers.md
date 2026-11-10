# 2026-11-10 Private consumers before v0.4.5

Maxwell supplied eight native RenderMan/flashlight/walkie/NPC package folders
in Archive.zip (SHA256
`6690741f522a1af8de3596d8902608e0bc4e2ef3c5254481dcd2b7590c8637af`).
They remain private evidence, outside the repository and public release.
Publication and the pending guarded merge were stopped before either occurred.

The actual consumers exposed two remaining generic gaps. NPC reaction policy
now checks advisory `voice_muted` before queueing and before delivery, dropping
pending reactions when muted; policy version is 1.0.2. The co-located checker
now resolves normal and wrapped sibling dependencies in one bounded authoring
workspace, retaining normal catalog validation and missing-dependency errors.
Render's real Halloween dependency reproduced the original checker failure and
passes after the fix. A regression also checks both dependency directions
between standalone and wrapped packages, and removal of the external dependency.

Private copies were migrated to the reviewed interfaces: explicit state exports,
typed aim lock and refreshed weapon identity, namespaced images and authored
size/backing, opt-in quiet spawning and anchored locomotion, and periodic camera
lease renewal even when parameters do not change. A genuine private cleanup
bug queued removal twice for registered bots; the stray sweep now excludes
already removed IDs. Engine ownership checks were retained.

All eight actual package checks pass with their sibling Halloween dependency.
A temporary private-consumer integration test used the real headless dedicated
host loader, Library activation/companions, ordinary light toggle, documented
Blockhead bot provider, Render spawn/claim, advisory mute, hidden name tag,
self-lit face and clear lifecycle with no diagnostics. It passed in 7.74s;
source and logs are retained externally, not committed as a private-asset test.
This establishes technical integration, not visual fidelity or interactive
acceptance. The author's described private harness source was not attached.

Commands: `cargo test --locked -p bri-package-runtime --test co_located
--test npc_reactions` (3 + 9 pass), `cargo clippy --locked -p
bri-package-runtime --all-targets -- -D warnings` (pass), and the temporary
`cargo test --locked -p bri-net --test private_native_family -- --nocapture`.
The new regression fixture initially used a reserved package prefix and then
an invalid client-code declaration; these fixture errors were corrected rather
than weakening the loader. Private migration ZIP, 14 changed-file hashes,
untouched extraction and exact check outputs live under
`E:/Downloads/BlocklandReImagined-Release-Evidence/v045-2026-11-10`.

The checker source changes the converter recipe identity. Regenerate all 100
pinned originals and the NPC/ambience bundles before the final platform builds;
do not reuse obsolete generation receipts. Earlier extended validation and
corrected fixture reruns remain recorded in the engine-review entry, including
the concurrent Tower CTF flake; they are not a clean whole-gate command pass.
Next: final scoped/history/protocol checks, platform CI, verified three-platform
artifacts, guarded main integration and publication. Full alpha acceptance
remains open.
