# v0.4.4-preview.2 handoff

Playable folder:
`E:\Downloads\BlocklandReImagined-v0.4.4-preview.2-windows`.
Launch `Launch.cmd`. The packaged executable reports
`v0.4.4-preview.2 (1d35924df)`, from
`1d35924dfdc133fe1e290823539a942853a6fe4e`.

This combines the Bedroom zombie follow-up with the revised destruction
contact response. #438 merged as
`a5f768b2e438585d49202201f8b7fc49289a6c7a` after all seven executed checks
passed (the conditional Windows test job was skipped). The connector returned
stale in-progress job states; direct Actions API queries confirmed the actual
successful jobs at the reviewed head. No workflow retry was needed. #437 stays
open for final-head CI and Maxwell's judgment of the changed collapse motion.

Build: set `BRI_VERSION=v0.4.4-preview.2`, then
`cargo build --locked --profile playtest -p bri-client --bin bri-client
-p bri-addon-import --bin bri-import-addon -p bri-net --bin bri-server
-p bri-package-runtime --bin bri-addon-check -p bri-sim --bin bri-addon-run`.
The warm worktree remains active for follow-up; no source installation or
other worktree was changed.

Changed-area validation is in the
[contact-response receipt](2026-11-04-destruction-contact-reaction.md) and
[zombie receipt](2026-11-04-zombie-traversal-followup.md). On the combined
tree, all 524 simulation unit cases pass, with five existing measurements
ignored. Warning-free all-target clippy passes for bri-client, bri-net,
bri-sim and direct consumer bri-chaos. History passes against current
origin/main. No full workspace, local GPU suite, visible game window or
automated interactive session was used for this handoff.

Packaging used `tools/package_playtest.ps1` with `-SkipArchive` and
`-PreviousImportsForPrivatePreview`, the exact executable SHA256, the verified
private Add-On bundle from preview.1 evidence, and the pinned DXC archive.
Verification with `-VerifyPackage` and matching private-preview flags passed
for 13,458 files, default Add-Ons and ten reference menu images. An initial
verification invocation used release mode and correctly rejected the private
prior-imports marker; it was rerun in the declared private-preview mode.

Packaged `--version` and `--check .\content` were awaited using hidden
`Start-Process -Wait -PassThru` invocations. Startup exits zero: 21 maps,
1,372 definitions, 35/35 save pictures and 14 modern-light parameter sets.
The existing 19 Add-On problems and 226 fallbacks remain. Reusing verified
v0.4.3 original imports does not establish current importer freshness or
release fidelity. This is a private playable preview, not the v0.4.4 release.

Private binary hashes, startup output and instructions are under
`E:\Downloads\BlocklandReImagined-Release-Evidence\v044-preview2-2026-11-04`.
Previous previews and user saves are retained.

## Maxwell's playtest

Use the saved player spawn and ordinary zombie/minigame settings. Stay alive
and stationary long enough to observe pursuit in:

- `ZombiesHaveTroubleGettingMeStairCase`: use the stairs without walking off
  while staring at the target.
- `MattressHouseZombiesCan'tGetMe`: prefer the viable house/stair route over
  repeating the impossible shortcut jump.
- `ZombiesCantGetMeBeta16CityTower`: retain the accepted all-three pursuit.

Replay the same two-stock-rocket sequences in Jetpuff's Towers and Pirate
World, using the diagnostic positions recorded in the earlier motion receipt.
Judge the first fall, contact, turning and breakup: the new response derives
from observed contact rather than a canned base hinge. CPU evidence does not
establish that the motion feels right. Pressure fallback still holds angular
orientation while checked translation continues. No interactive acceptance is
claimed; local GPU checks remain off during Maxwell's playtest.

## Speedkart give-back

`E:\Downloads\Speedkart-addon-v0.4.4-preview.1.zip` contains the three native
packages at version 1.0.1 and `README-FIRST.md` with porting notes. Its SHA256
is `12c6d0fbd53837ea2734968e51d9011c2afa558601c26aa45d12b99e400157c1`.
All three packages also pass preview.2's packaged checker. The archive's
existing preview.1 filename remains accurate for its original validation;
the native presentation API is unchanged in preview.2. The supplied assets,
scripts, credits and provenance are preserved. Dense-vehicle horn targeting
and interactive driving acceptance remain explicit in the README. No engine
binaries or third-party assets were put in Git, and no message/file was sent
to the author on Maxwell's behalf.
