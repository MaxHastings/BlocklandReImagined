# v0.4.3 first private preview

Delivered `E:/Downloads/BlocklandReImagined-v0.4.3-preview.1-windows`; Maxwell can
launch it through `Launch.cmd`. No interactive session was launched by the
agent. The binary reports `v0.4.3-preview.1 (2cd609484)`. This increment contains
the confirmed input-admission correction, bounded local movement diagnostics,
bot spawn-frame correction and requested audio derivatives. Navigation, sessions,
rendering and remaining release scope are still being implemented; no GitHub
v0.4.3 publication or completed alpha is claimed.

Built the actual five shipping executables with `--locked --profile playtest`.
The private folder was produced with `tools/package_playtest.ps1 -SkipArchive`,
the measured executable SHA256 and pinned shader-compiler archive. No release
LTO, ZIP creation or merge-only gate held this preview. Keep its outputs warm.

Actual package verification checks all 13,458 manifest files and default Add-Ons.
An independent Python pass also matched every file size and SHA256. Packaged
`bri-client --check`, using separate private state, passed with 21 maps, 1,372
brick definitions and 14 modern-light parameter sets, without opening a window
or audio device. Known map fallback warnings remain in the actual startup log;
this is not a clean source-fidelity or complete release-gate claim. Exact five
executable hashes, manifest hash and startup outcome are recorded privately in
`diagnostics/v043-preview1-verification.json` under the preserved release evidence.

The first packaging check caught a real installed-source edge case: optional
Add-Ons already active in `content/packages.json` survived even when shipping
defaults declare them off. Windows packaging now validates identity conflicts
first and removes those known optional entries from the startup list while still
installing their files. It does not change the source installation's choices.
The corrected actual folder passes the existing default-selection verifier.

An intermediate manual attempt put unsupported `enabled` fields on package
entries; native startup rejected it. Restored the supported schema and corrected
the packaging mechanism instead. Two failed assemblies are retained privately,
and only the verified third assembly occupies the delivered preview path.
The synthetic PowerShell packaging fixture independently fails before this case
because it assumes all non-original defaults have `path` and cannot construct
current generated Add-Ons. That pre-existing fixture gap and Linux/Mac installed-
source selection parity require correction before the final release gate; they
were not skipped or reported as passing. The native package is the direct
before/after regression evidence for this preview's Windows correction.

Maxwell was asked to play normally for the elusive movement symptom and briefly
check audio balance. New health lines live in the preview's session logs. Optional
bot and ragdoll Add-Ons retain their normal shipping defaults; use the relevant
enabled environment for their tests. The native bot baseline's active package
configuration is preserved separately in `diagnostics/v043-bot-baseline-enabled-packages.json`.

Maxwell may now be playtesting. Do not run local GPU suites, full gates, soaks or
benchmarks during that session. Continue source investigation and appropriate
small headless checks. The release goal remains active, with his local-feel/mix
acceptance and all remaining agreed features open.
