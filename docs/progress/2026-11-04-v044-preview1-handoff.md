# 2026-11-04 v0.4.4 preview 1 handoff

Maxwell requested a playable preview after the vehicle, zombie and destruction
work. Delivered E:/Downloads/BlocklandReImagined-v0.4.4-preview.1-windows,
launchable through Launch.cmd, reporting v0.4.4-preview.1 (7febdae7b).
No interactive game session was automated.

The build includes merged #433 (vehicle presentation), #435 (Add-On search
scrolling), #436 (zombie tower pursuit), and #437 destruction head 7febdae7b.
#437 platform CI remains pending at handoff; preview access is not held on it.

Built all five shipping executables using BRI_VERSION=v0.4.4-preview.1 and
cargo build --locked --profile playtest with explicit client/importer/server/
Add-On checker/runner binary targets. No release LTO or ZIP work blocked the
private preview. The first optimized build in this reused worktree completed
in 7m57s; its warm debug output had already run the changed-area checks.

The main checkout's old bundle cache lacks import generation receipts.
A fresh private evidence bundle copied 102 original Add-Ons from the intact
v0.4.3 release, verifying their previous receipts, and took generated packs
from current verified content. package_playtest.ps1 used SkipArchive and
PreviousImportsForPrivatePreview, the exact executable SHA256, and the pinned
shader compiler archive. This is explicitly a private preview with prior
conversions, not current importer-generation or release acceptance. No original
installation, existing preview or global Add-On choice was modified.

Package verification passed 13,458 file hashes, default Add-On checks and ten
reference menu images. Packaged bri-client --check .\content exited 0:
21 maps, 1,372 brick definitions, 35/35 converted-save pictures and 14 modern
light parameter sets. It reports 19 existing Add-On problems and 226 fallbacks;
these are not clean content/fidelity acceptance. Native GUI executables were
explicitly awaited with Start-Process -WindowStyle Hidden -Wait for version and
startup checks; direct PowerShell invocation did not reliably await them.

Final destruction changed-area evidence: 785 client unit tests pass, 79 ignored;
warning-free client all-target clippy; maintained format/docs/history checks.
Both private two-rocket sequences pass CPU and real-model-upload GPU replay.
Representative captures inspected; map rendering/effects/weapon view are omitted
from those captures. Pirate overflow debt peaks at two client frames, Tower zero.
No platform or optimized performance result is inferred from debug timings.

Private build hashes, startup output, exact repro instructions and the preview
bundle receipt live under E:/Downloads/BlocklandReImagined-Release-Evidence/
v044-preview-2026-11-04. The user was given a launch link and the three ordinary
playtest sequences. Zombie replay uses normal player spawn and ordinary controls:
all three pursue upstairs; observed first walking is approximately 1.2/1.2/2.1
seconds, not instant reaction. Interactive acceptance remains Maxwell's.
GPU suites and benchmarks remain off after delivery while he may be playing.
