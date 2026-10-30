# 2026-10-29 Approved hammer and wrench swing Foley

The broad gameplay-sound audit was canceled. Maxwell subsequently resumed only
the selected tool swings alongside the separately owned fridge/map ambience.
Vehicles remain excluded. This lane uses existing image-state sounds and weighted
profiles; no simulation, mixer, wire, schema or hit-hook change is needed.

## Source and acceptance

[artisticdude's Swishes Sound Pack](https://opengameart.org/content/swishes-sound-pack)
is recorded stick/clothes-hanger Foley under CC0-1.0. Exact archive, member,
processed-WAV and audition hashes are pinned in
[`tools/tool_swishes/sources.json`](../../tools/tool_swishes/sources.json).
Only `swish-1.wav`, `swish-6.wav` and `swish-10.wav` are selected. Maxwell heard
A/B/C and said, "yeah the tool swish is good", then asked to use all three for
variety. Following cancellation, he explicitly requested the approved tool swings
again. Root's bounded scope confirmation: "Tool swings, fridge and map ambience
only". Source selection is distinct from final integrated listening acceptance.

The prepared leaves match all three occurrences in their respective heard
audition WAVs byte-for-byte: mono48kHz PCM16, highpass120Hz, DC removal, peak cap
-12dBFS/RMS cap-24dBFS. They are new immersion, not recovered v20 swing sounds.
The original source authors only contact sounds for these tools.

## Change and regeneration

`tools/prepare_tool_swishes.py` verifies the archive, members, approved processed
hashes and receipt before installation. It adds three native resources, hashed
credits/provenance, and one equally weighted profile. Policy names the two stock
Fire states explicitly; the runtime remains generic. A conflicting authored
state sound is refused. Original contact sounds, state timings, animations and
definitions are retained. One accepted Fire entry produces one ordinary reliable
sound cue, including a miss; holding repeats through existing state transitions.

The native weapons regeneration step runs the augmentation after faithful
conversion and before its existing completion stamp. Its fingerprint includes
the builder, accepted sources and policy. Failed preparation leaves the step
incomplete; it never claims fresh provenance for missing or changed recordings.
Base ci-content carries the actual files. There is no optional sound Add-On,
dangling dependency, new toggle or copied receipt masquerading as regeneration.
Fresh preparation needs ffmpeg (the builder prints an OS-specific install command
if missing). Downloaded/generated media stays outside Git.

## Verification checkpoint

- `python -m unittest discover -s tools -p test_tool_swishes.py`: 2 pass.
  Challenges replacement of authored sounds, altered bytes, no mutation on
  rejection, idempotence and preservation of every unrelated field.
- Exact approved PCM comparison passes for all nine heard occurrences.
- Prepared/verified source set and augmentation of a separate native pack pass.
  Four profiles/five resources added; only hammer/wrench Fire sound fields change.
  Catalog SHA256 `03f47131b92f43058119f7ac4beb4ba189ceb86a9fa725670b1b45406effa5fa`.
- `rustfmt --edition 2024 crates/client/tests/tool_swishes.rs` and
  `git diff --check` pass. The native consumer is **not compiled/run yet**.
- Ignored artifacts/receipts: `artifacts/tool-swishes/`; canceled broad drafts and
  earlier candidate media preserved under `artifacts/gameplay-audio/`.
- No installed preview changed. Observed normal client PID8832 at
  `E:/Downloads/Map-Ambience-Preview-20261029/bri-client.exe`; no heavy local build,
  visible launch, device playback, GPU work or gameplay input automation run.

## Finite remaining handoff

Root serializes warm validation after the active audition: native Session
miss/hit/held/release/rejection -> two offline client listeners, all selected
leaves decoded/mixed, distance/mute/teardown; scoped warning-free test Clippy,
existing tool consumers and weighted-profile checks. Then correctly regenerated
base resources/full content identity/startup check and a warm private preview.
Exact-head platform CI/history/protocol checks and guarded integration remain
required. Final mix/timing acceptance is Maxwell's. This is not full alpha closure.

Native consumer command (point `BRI_TOOL_SWISH_WEAPONS` at the separate staged
native pack, `BRI_CONTENT` at the verified base content):
`cargo test --profile playtest --locked -p bri-client --test tool_swishes -- --ignored --nocapture`.
No cold/shared target directory is requested. The approved source/profile code
and actual content must stay paired when making the preview.
