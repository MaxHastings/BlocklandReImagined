# 2026-10-28 Preview 10 from merged main

Private folder-only playtest delivered from main 2ab6de5cebfd41837bc029c3742f0638ec07f085.
Includes PRs #382/#384/#385/#386; no runtime source changes for this handoff.
Warm five-executable `cargo build --profile playtest --locked` took 1m45s.
No competing gate, benchmark or GPU test was run. No game window was opened.

Folder: E:/Downloads/Destruction-preview-2026-10-28-r10-main/BlocklandReImagined-v0.3.7-destruction-preview.10-windows
Launch.cmd uses ordinary per-user settings/saves; previous previews were kept.
The external receipt and verification logs live in that folder's parent.

Packaged startup passed: 21 maps, 1396 definitions, 35/35 save pictures and
14 modern light maps. All 13,320 file hashes and exact file-set membership
verified, 120 unique package identities, 118 NPC recordings, all five executables
byte-match the warm output. Existing Butterfly Knife health warnings remain.

Packaging reused the main recipe in a private generated wrapper without ZIP
creation. Initial wrapper attempts failed on script-root binding/encoding before
copying; regenerated with an explicit tools path and UTF-8 BOM. The standard
public-release-default check rejected inherited enabled showcase Add-Ons. Those
stock defaults were checked separately, then the original private 120-package
selection (including CTF and current weapons) was restored and all hashes/startup
rechecked. No release-default pass is claimed for this customized private copy.
No user preferences or source/reference installations were altered.

Cold 5-second / repeat 3-second loading targets remain unverified across cases.
Maxwell owns interactive acceptance. This local handoff note is not an engine
change and does not alter the compiled main snapshot.
