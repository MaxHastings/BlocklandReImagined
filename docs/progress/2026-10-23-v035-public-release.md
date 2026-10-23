# 2026-10-23 v0.3.5 public release

Published [v0.3.5](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.3.5)
at 17:42:48 UTC / 12:42:48 PM America/Chicago from
`5a16c9317c2920bb6eb16211c57143a2c46b4073`. The tag and all four uploaded
asset digests were checked against the verified inputs; GitHub identifies it
as the latest release. No published tag or prior release asset was replaced.

Maxwell's final scope is this public release. Further AA2 investigation,
gameplay work and matching private preview delivery belong to the separate
working thread, based on this engine pin and private main `b3efce071`.
The overall alpha/AA2 fidelity contract remains incomplete. Main can resume
separately verified integration after this release handoff.

## Integrated verification

- `python tools/gate.py`: all 541 binaries ran, strict clippy and content check
  passed. One named loopback case, `a_resume_over_kept_bricks_downloads_only_the_changes`,
  failed during the parallel run and passed alone; the gate recorded the flake.
  Initial content check lacked generated `ui-pack-005`; copying the verified
  pack into the primary content fixed the input deficiency before the full run.
- Performance budgets passed: 32 bots / 3,600 ticks, release build, mean step
  5.10 ms on Crazy Collage Madness (7.00 ms budget), 3.97 ms on TDM Afghan
  (5.50 ms budget). These are simulation budgets, not gameplay FPS claims.
- Windows PR CI [39988314203](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/39988314203)
  passed. `python tools/gate.py --push` integrated the exact tested commit
  into main; PR300 was automatically marked merged.
- Build-only release run
  [39989266217](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/39989266217)
  passed Windows, Linux and macOS at that exact source revision. Release panic
  capture and retained matching debug-symbol checks passed. macOS also passed
  signature verification and extracted-app startup checks on its runner.

## Actual distribution bytes

Each ZIP was independently streamed against its manifest for every file's
size/hash, unique safe paths and exact membership. Windows has 13,167 payloads;
Linux and macOS each have 13,161. All select 78 packages. Each contains the
547 verified UI images, including all ten curated menu photos and the fourteen
unchanged map thumbnails. Compiler files/notices match the distribution pin;
original Add-On credits and the corrected throwing rule are present. Historical
playtest handoffs and internal design/test cards are absent from the downloads.

All 13,144 native content payloads match across platforms; selection JSON is
semantically identical, with only platform line endings differing. The 3,115
base-pack payloads are byte-identical. Public archives exclude private AA/Halo
packages. Extracted final Windows client reports `v0.3.5 (5a16c9317)`;
`--check` exits zero with Add-On health zero. Its server loaded Bedroom and
completed a bounded loopback run. All five executables match the archive
manifest, without missing-native-resource/doubled-assets warnings. PE checks
found no `vcruntime*`, `msvcp*` or `concrt*` imports in the five executables or
the two pinned compiler DLLs; the compiler DLLs use Windows UCRT API sets.

| Archive | SHA256 |
| --- | --- |
| Windows | `a095b6f474b2755e9ce914721a395ca023672433a0dfd518a5b210294ff959e3` |
| macOS | `a0303e1eadfa3007e18ab42b72446f0e6924c3e81da1ac462ec3c5c99899425b` |
| Linux | `72bc2e2ec32b5d0913338248f789449a622181af2edfcf6d2d308d23ff69f1c7` |

`python tools/publish_release.py --version v0.3.5 --sha 5a16c9317c2920bb6eb16211c57143a2c46b4073 --run 39989266217`
used prepared final notes and a fresh ignored output directory. GitHub's asset
digests match these ZIP hashes and the uploaded SHA256SUMS. Local detailed
receipts/logs remain under `.research/v034-fixes/`, including
`final-*-archive-receipt.json`, `final-packaged-windows-startup-receipt.json`,
`final-platform-comparison.json` and `published-release-receipt.json`.

## Rendering limits retained

Full App offscreen map checks used the exact release source and verified
archive content: DX12/DXC High, DX12/FXC Low, Vulkan High, and software graphics
with Auto/default and explicit DXC. World and menu pixels were inspected.
These do not establish native presentation on every GPU or source fidelity.

An initial coarse nonblack statistic accepted an indistinct software/FXC
640-pixel capture. A settled 1280-pixel landmark check instead showed a black
world with working HUD on that forced legacy-compiler path. Disabling bloom
did not fix it; Auto/default and explicit DXC on the same device show the world.
This remains a known limitation, disclosed in the published notes. It does not
identify or close Lizzy's RX550 report. Loading traces now identify real work;
the optimized contended Bedroom sample spent about 4.6 seconds before server
startup, including 2.6 seconds hashing 499 MB. No 100 FPS claim is made.

Original installations and unrelated dirty primary-checkout files were
preserved. The old private preview and the mission roots referenced by the AA2
thread remain intact; they are not disposable release cleanup targets.
