# 2026-10-27 Blockhead jet audio playtest

Maxwell selected his Desktop `jet.wav` EQ/lighter attack, then the separate
`jet-sustain-less-hiss-toothbrush.wav` revision (SHA256
`d1628f59d9c0aff67d724562ddce839e3208e84a39caaa6fb4e1effb548b503e`).
Request: install that version into the same v0.3.7 E: playtest folder. Work
reuses the warm `footstep-turning-f` checkout/target on
`codex/blockhead-jet-playtest`, based on origin/main `b927779dc`. Main checkout's
user changes and root README are untouched.

## Implementation and asset

- First-party `sounds.jet` character policy selects a native held sound profile;
  default Blockheads inherit it, Horse has none. Packages can override it.
- Client observes presented accepted `jetting`, with alive/unmounted/unridden
  eligibility. No raw mouse/key events, animation events or changes to physics.
  Source follows feet, including during release, through normal positional
  attenuation/panning, world volume, room hearing and voice budgets.
- Generic PCM sustain metadata owns intro-loop offset and attack/release. Mixer
  runs a sample-clock raised-cosine50ms attack/150ms release. Short gaps resume
  current amplitude/phase, one voice per actor, without repeating attack.
  Natural stop/eviction events retire client handles; session clear stops all.
  Virtual voices still advance envelope and release. Invalid PCM loop regions
  are refused. Output-device rate changes rescale active envelopes.
- `assets/blockhead-jets/make_jet.py` takes the exact authored source by hash,
  folds to mono for positional playback, excerpts1.5–7.35s, retains intro and
  crossfades final80ms into held audio. Clip5.85s, loop returns to0.33s, no extra
  EQ/gain. Derivative SHA256
  `b055b73e23f0b23535bb3175188b56b8071ae09e661c8a47ff13a4598d730710`.
  Prior desktop exports remain intact. Metadata and attribution accompany it.
- Existing native weapons importer/refresh installs the clip/profile/resource;
  no Add-On, dependency or parallel audio engine. Protocol change file records
  optional replicated character sound binding; client/server must be paired.

Underlying sources: daveincamas' AirCompressorRelease, Freesound25709, CC BY4.0;
conradts' electric toothbrush, Freesound190579, CC0; Maxwell authored additional
EQ and lighter attack. Inputs currently use public MP3 previews. This is a local
playtest candidate; settle original-master quality and added-layer provenance
before publishing assets. No downloaded original recording enters Git.

## Validation and handoff (in progress)

`cargo check -p bri-client -p bri-weapons-import` passed. Initial command used a
nonexistent `bri-server` package (actual binary belongs to `bri-net`) and whole
workspace `cargo fmt --all` exceeded Windows' command-length limit; corrected
to package-scoped commands. First compile exposed one stream interpolation
call missing the new loop parameter; fixed with zero for non-looping stream.
Asset reconstruction reproduces exact derivative hash. Quick tier running;
new hardware-free tests cover loop intro exclusion, block-size-independent
envelope, feathered handle reuse, exact release, virtual release, invalid region
and session clear. Focused, full integrated gate, paired warm build and installed
startup check remain pending. Do not claim gameplay acceptance from these tests.

Target: `E:/Downloads/BlocklandReImagined-v0.3.7-windows/Launch.cmd`.
Before install, back up old paired binaries/catalogs, retain F walking and
landing B+6dB hashes, update dependent presentation catalog fingerprint, and
check installed content headlessly with isolated state. Roll back on failure.
Maxwell alone playtests short taps, a hold past six seconds, feathering, energy
cutoff, stopping/death/mount/disconnect and another player's positional sound.
No interactive game window launched by the agent. No PR/push yet.

## Local handoff priority and coordination

Maxwell asked why the local listening pilot was waiting, then explicitly asked
coordination with “Model blast cuts and baseplates”. Reprioritized the local
playtest ahead of the broad release gate: do not call this release/merge ready.
Audio mixer integration tests all3 passed. Targeted client handle/release/clear
test passed after correcting its expectation: ordinary session clear has a5ms
declick, so check zero voices after10ms rather than immediately. Initial broad
quick run reached client unit tests but was stopped in unrelated long offscreen
portal/render cases to prioritize the warm executable; it did not pass the full
quick tier. No new audio regression was found by the targeted test.

Paired build uses `cargo build --profile playtest -p bri-client -p bri-net
-p bri-weapons-import --bin bri-client --bin bri-server --bin bri-weapons-import
--locked`. This reuses this checkout's existing playtest artifacts and sccache;
no new/shared target directory. Keep source uncommitted until focused checks.

Explicit user-authorized messages sent to physics chat01a17fdc-0faf-7e61-bc7c-
30fe5671ac29. Read-only verification found clean physics head1fba39778 in
`plastic-destruction-playtest`, separately packaging under
`E:/Downloads/Destruction-preview-2026-10-27`. Audio target remains the original
v0.3.7 folder; physics preview is untouched. These are separate source/executable
candidates, not a combined build. Sharing CPU/cache can slow concurrent compiles,
but they do not share Cargo target or destination folders. Await any updated
physics destination before copying; no combined-source acceptance claimed.

## Installed local pilot

Warm paired playtest build passed in8m39s. Scoped mixer3/3 and client held-voice
test1/1 passed; full quick/focused/release gate remain pending, not merge-ready.
Physics chat confirmed its updated target is
`E:/Downloads/Destruction-preview-2026-10-27-r2`; no collision with audio install.

Installed into `E:/Downloads/BlocklandReImagined-v0.3.7-windows` and headless
`bri-client --check <installed content> <isolated state>` passed exit0. Source
remains uncommitted over b927779dc; receipt explicitly records dirty source and
tracked-delta hash, not a false claim that base commit contains these changes.
Paired binaries:

- client SHA256 `ddc806d3166571ca55a2d9eceb8c9b9183c4ba0ca0fc0758efe3c096aa7bc05e`
- server SHA256 `ca04cea41fddf93d85bdc7d1e81b17b795467592d03e4f5879075b2cb87a3b0c`

`JET-PLAYTEST-RECEIPT.json` in installed folder holds backups, source/profile,
binary/asset/catalog hashes and pending interactive status. Backup/check logs:
main checkout ignored `artifacts/jet-audition-20261027/integration/
installed-playtest-20261028T041853Z` (UTC identifier; installed October27 local).
All unrelated weapon definitions/resources retained; F step files and landing
B+6dB hashes unchanged. Only owned Foley refresh, new jet asset, dependent
presentation fingerprint, attribution and paired executables updated.
Original Desktop exports, read-only v20 reference, physics preview, main source
checkout and GitHub README untouched. No Add-On or visible game launched.
Maxwell can now use existing Launch.cmd and listen during ordinary play.

## Playtest gain adjustment

Maxwell requested jets+4dB and landing+2dB. Applied gain only to owned source
and installed PCM16 clips; profile/envelope/timing unchanged. Jets now SHA256
`6571f7f71319125710ad439c1738c12a3e9a42e33316873f3af3e66b89dede01`,
peak-15.66dBFS; landing now+8dB total over B, SHA256
`cfc92556c244854df3ca9db4d3173efb681cb64ac85f341bf4604ebc9c7b0166`,
peak-6.33dBFS. No clipping/normalization/limiting. Updated resource hashes and
dependent presentation fingerprint; source recipes/generators reproduce gains.
Installed headless startup check passed again. Executable and F-step hashes
unchanged; no rebuild, main push or physics-preview update. Current installed
`AUDIO-GAIN-RECEIPT.json` and refreshed jet receipt record this local gain state.
Backups/check logs under main ignored jet-audition integration/gain folder.

Maxwell requested another jets+4dB/landing+2dB pass. Applied to same local build,
making jets+8dB over first install and landing+10dB over B. New PCM hashes:
jet `33d4dc43b25a2f864bdf69c12d5d7cdb239289e6204ffaa5905cc34c2195ce21`
(peak-11.66dBFS), landing
`eb321f1ee104f22cd7f9a3cdb84c0e86b13898646d408f2a6b2fbcf7f770d491`
(peak-4.33dBFS). No clipping; installed headless check exit0, paired binaries,
F steps, profiles and ASR unchanged. Current receipts and source generators
updated for sequential PCM gain passes; physics preview untouched.

Next requested pass: jets+3dB, landing+1dB. Both now+11dB over their respective
first jet install / original B audition. Jet SHA256
`97635d15d97765efea7e2f2a93d8c98665fef4d06191f9bcb8ff98dfce52b8b7`
(peak-8.66dBFS); landing
`d2d0e79668fa032d57318fbc3efe21275db361f414f9249f3133e01d5d3ea1d5`
(peak-3.33dBFS). No PCM clipping; same installed startup check passed. No
executable rebuild or timing/profile change. Recipes, generators and current
gain receipt updated; previous files backed up before the edit.
