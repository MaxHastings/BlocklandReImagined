# 2026-10-10 P90 native v0.2.8 playtest port

Branch `codex/rallypack-p90-v028` starts at release `b6500335e`, not newer
main, to verify Maxwell and his friends can keep their installed v0.2.8.
Maxwell accepted a playable appearance-focused port rather than exact fidelity
to every Rallypack feature. No engine or network changes were needed.

Added `tools/build_rallypack_p90.py` and `tools/rallypack_p90/`: a private
content recipe for a self-contained shared data Add-On and host-only Rhai
rules companion. Original ZIPs, scripts and converted assets stay outside Git.
The source installations remain unchanged. Native output preserves the P90
model, magazine, textures, icon, looping shot/tail audio and required framework
reload/distant sounds, shell model, flash/smoke emitters and recoil effect.
Rules implement 50 rounds, two spare magazines, chambered-round reloads,
partially used magazine retention, reload cancellation, aim toggle, movement
slowdown, authored spray recoil, minigame damage and respawn reset.
The source's ordinary bullet shape path is absent in the ZIP; no invented
replacement is supplied.

This is a playtest port: generic framework penetration, random ricochets,
near-miss suppression and preferences are not reproduced. Distant audio is a
private non-positional cue, and loop/tail timing is approximated. Maxwell owns
the interactive appearance/gameplay check; none was automated or launched.

Evidence:

- Built the exact-release `bri-addon-check` and `bri-addon-run` with
  `CARGO_PROFILE_DEV_DEBUG=0`, no shared target directory. Loader and bounded
  idle rules run passed after fixes.
- `cargo test -p bri-net --test rallypack_p90 -- --ignored --nocapture`, with
  `BRI_P90_TEST_ROOT=E:\Downloads\BRI-P90-native-work\candidate-4` and
  `BRI_P90_CONTENT_ROOT=E:\Downloads\BlocklandReImagined-v0.2.8-windows\content`:
  three tests passed: ordinary firing/reload/cancel/respawn; authoritative
  minigame damage; real network clean-cache download/hash verification,
  cache reuse, and equipment visible to a late joining client. Host rules
  are not downloaded. This loopback test does not establish internet/NAT
  connectivity for Maxwell's eventual server.
- Corrected unsupported Rhai `clamp` and float `pow` calls found during firing;
  earlier load attempts also exposed a reserved property name, wrong players
  iterator assumption and an unnecessary external-projectile declaration.
  Test fixture compilation errors were corrected against the release API.
- Final recipe invocation generated `E:\Downloads\BRI-P90-native-v028`, then
  installed its two folders under the release's `content\addons`, disabled.
- Released `bri-client.exe --check` with P90 temporarily enabled passed, exit
  0: 14 maps, 988 brick definitions, 35 save pictures, 14 lighting maps.
  P90 had no Add-On health problems. Existing Butterfly Knife warnings remain.
  The installed package selection was restored byte-for-byte in `finally`.
  No window or audio device opened. An earlier staging-root check failed
  because content junctions point outside the root; reran against the real
  release with isolated client state instead.

Next: Maxwell restarts the game, enables Rallypack P90 in Start Game → Add-Ons,
hosts, gives it through a minigame loadout or item spawn brick, and has his
v0.2.8 friend join. Check the model/icon, aim, sounds, shells/flash, firing and
reload together. The first live internet join remains user-owned acceptance.
