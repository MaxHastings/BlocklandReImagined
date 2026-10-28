# 2026-10-27 Blockhead landing playtest

Maxwell requested the next obvious missing action sound: use existing material
and install landing feedback in the same v0.3.7 Windows build. Walking F is
already merged in PR377; its samples, marker policy, shuffle selection and
latest -1.5 dB trim are outside this pass. No new Foley recording or interactive
automation is needed. This is new ReImagined feedback, not recovered v20 audio.

## Small playable change

- Reuse the existing `Sounds::land` / client `Footsteps::update` mechanism.
  Stock Blockheads name `bri:foley/blockhead/land_01`; Horse retains its own
  policy. No movement, animation, networking or save schema changes.
- `assets/blockhead-footsteps/land_01.wav` is Maxwell's selected **B**: dedicated
  `impact_land_light2.wav` with a quieter filtered `impact_medium2.wav` body,
  six-ms softened attack and compact decay. He rejected the first F-derived
  pilot as another footstep/click, selected B, and finalized the level at a
  clean +6 dB over B's audition after playing. This gain is baked into PCM
  because native profile volume supports only 0..1. No footstep layer remains.
  It is mono PCM16 at 48 kHz, 0.28 s, peak -8.33 dBFS, with a silent tail.
  `make_landing.py --source <supplied-foley-directory>` reproduces the exact
  selected/adjusted bytes; `landing-recipe.json` retains processing/source hashes.
  Maxwell explicitly requested merging this sound. No broader source license
  or CC0 status is asserted; original recordings remain external/read-only.
- The existing native catalog installer/refresh owns the added sound/resource;
  bootstrap already hashes this asset folder. Normal positional audio is reused.
  No Add-On or setting is required; the main README is untouched.
- Landing eligibility uses the last airborne downward speed, with the existing
  4 units/s threshold, rather than the maximum reached during the whole flight.
  Otherwise braking a fast fall with jets could bank an impact for a gentle
  touchdown. No timer, cooldown or forced animation contact was added.
- Keep one fixed level and one variation for this baseline. Hard-fall scaling,
  new surfaces and further walking polish are deferred.

## Evidence and handoff

- Real shared player motor on a flat collider: normal jump, 2-unit and 8-unit
  falls; sample the resulting poses at 30/60/144 fps for two distinct owners.
  Each produces exactly one stock landing at its own feet on the grounded
  sample, no airborne/repeated hit. `cargo test -p bri-client --test
  blockhead_landing --locked` passes (one test, 18 scenario/owner combinations).
- A client regression covers braking descent to -1 or +2 units/s, first grounded
  observation and small support fluctuations: no landing impact is due.
- Native importer coverage checks both the four walking clips and the landing
  profile, resource hash and idempotent refresh through the normal catalog.
- Quick passes in 88 s: client 697, motor 14, importer 12 unit tests. Focused
  passes in 143 s: all-target Clippy for those three crates, motor/importer
  suites, client units, the real-motor landing test and four synthetic avatar
  integration checks. Converted avatar tests remain separate existing ignored
  release checks. Optimized client/server build, full candidate gate and
  installed startup validation follow. Logs are under this worktree's
  `artifacts/landing-playtest`; the installation receipt records exact binaries.
- The final +0.5 dB edit (from +5.5 to +6 dB) is installed into the same game
  folder and actual startup validation passes. Measured increment +0.50000 dB,
  no clipping.
  Source and installed WAV are byte-identical, SHA-256
  `bf793da0c4c8c779c5f8edc1ff1537ecf0aa694bd630374491a198063f14bbc6`.
  Existing executables, walking WAVs/gains and package selection are unchanged.
  `LANDING-B-GAIN-RECEIPT.json` and immutable local revision backups record it.
  Final asset/importer quick checks pass; focused passes in 39 s (all-target
  importer Clippy, importer suite, and real-motor client landing integration).
  The exact source WAV is independently reproduced from the retained recipe.
  Full gate passed integration revision `a4f541744`: build/content/Clippy,
  557 test binaries with no retries, and release bot performance (Collage
  5.36 ms / 7.00 ms budget; Afghan 3.00 ms / 5.50 ms budget). All PR checks on
  that revision passed (run 40607778896). Subsequent edits only finalize the
  requested PCM gain and matching recipe; runtime code/timing are identical.
  The gain-only edit is validated through the asset/importer and installed
  startup paths, retaining the completed full engine gate rather than repeating
  its unaffected simulation/render/stress cases. Windows PR CI must pass on
  the final head before merge.

Install into `E:\Downloads\BlocklandReImagined-v0.3.7-windows`, with immutable
backups and rollback on startup-check failure. Preserve all existing catalog
content except the new owned landing profile/resource; prove that before
rebinding the item-presentation catalog fingerprint. Maxwell alone tests normal
jumps, a small ledge, a higher drop and a jet-braked landing, then listens during
ordinary building/navigation. This entry does not claim listening acceptance.
