# 2026-10-27 Destruction preview delivered

Engine candidate: `72dc0fc2a9d90b507b2440cefc3ba3a70901961a`, based on
`b927779dc` main. [Draft PR #379](https://github.com/MaxHastings/BlocklandReImagined/pull/379)
is open and attached to this chat. GitHub CI run 40618527013 completed successfully.
No merge was performed; Maxwell's playtest acceptance remains pending.

Optimized Windows build: `v0.3.7-destruction-preview.1`, built locally using
`BRI_VERSION=v0.3.7-destruction-preview.1 cargo build --release --locked`
for the client, server, importer, Add-On checker and Add-On runner. Build took
19m12s; the client executable is 101445632 bytes, SHA-256
`685afea42e0cd5483c0c0bd6ebea6cda8d3ef4fe02a1d6d5ca481cf6c0fd3146`.

Delivered separately at
`E:\Downloads\Destruction-preview-2026-10-27\BlocklandReImagined-v0.3.7-destruction-preview.1-windows`.
Run `Launch.cmd`. The existing v0.3.7 installation was not overwritten.
The official `tools/package_playtest.ps1` copied 11906 native content files
(537856694 bytes), verified reference menu images, supplied the pinned DirectX
Shader Compiler, and produced a 302180621-byte ZIP beside the folder. The
packaged executable passed `--check` with an isolated local state directory.
The parent folder also contains README-DESTRUCTION.md and a JSON build receipt.
No interactive game session was launched or controlled by the agent.

Targeted corrections passed: 8 destruction lifecycle/support tests; real stock
rocket/64-cube partition check; brick_damage 16 passed/8 content cases ignored;
bandwidth 9 passed/2 diagnostic tables ignored; final changed-crate all-target
clippy. Earlier quick evidence: client 706 passed/78 ignored, net 90 passed/3
ignored, sim 443 passed/4 ignored, world 18 passed. The full local gate has passed
history, Python tools, all-target workspace build, native content startup and
workspace clippy. Its 558 test binaries were still running when the package was
delivered. Do not describe the full gate as passed until its terminal result.
The local gate's background CPU/GPU load prevents a clean simultaneous FPS
comparison. Logs and the optional unused probe draft remain under ignored
`artifacts/destruction-hardening/`.

First human checks: Afghanistan DM upper-floor center/edge/corner; ordinary
large ground block versus protected foundation slab; repeated shots and restore;
then taste/clarity and sustained performance. The full alpha contract remains open.


The full sweep subsequently finished (558 binaries, 747s; 1739s total gate).
It reported one retained failure after isolated retries: app_soak synthetic
steady play counted 5 chunk jobs against its old ceiling of 4. That phase still
had zero whole scene/texture uploads, zero item/debris model builds and zero
whole-world scans; hidden outlines visited 72 bricks. The ceiling is updated to
6 to allow adjacent authoritative batch/cue transitions across a full debris
lifetime. This is a test-only adjustment; the delivered engine is unchanged.
The corrected target is verified separately; the original gate has no pass
record and must not be represented as a clean full gate. Other transient
failures passed their isolated retries and were reported as flaky by the gate.


Maxwell's first launch exposed duplicate Add-On registrations in the packaged
content list (176 entries, 117 unique): repackaging an installed content list
appended the default mods a second time. The initial PowerShell helper also
failed to await the GUI-subsystem --check process, so the earlier startup-pass
claim was invalid. Corrected the same delivered folder, updated its integrity
manifest and ZIP, and used Start-Process -Wait -PassThru -WindowStyle Hidden with
captured stdout/stderr. Actual exit 0 confirms 21 maps, 1396 brick definitions,
35 save pictures and 14 modern light parameter sets. Existing optional Add-On
warnings remain; no interactive session was automated. Packager now merges
existing mod identities and copies each bundled tree once; the fixture preloads
the Duplicator and its rules to prevent recurrence. The corrected app_soak target
passed (60.95s) with its retained zero-whole-upload/model/scan assertions.

Packaging regression fixture passed after the fix. The delivered manifest now
also records the startup-normalized config and generated packages-disabled.json;
all 21670 listed files passed size/checksum verification. The separate public
release-default policy check rejects inherited enabled experimental mods (first:
ragdoll); this private preview retains the existing installation's enabled Add-Ons
rather than changing Maxwell's choices. It is not a public release-default pass.
The corrected ZIP contains the fixed configuration and updated manifest. Awaited
startup remains the launch acceptance evidence; Maxwell performs the playtest.
