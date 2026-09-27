# Opus task brief: complete vanilla audio pipeline

Prepare a complete vanilla sound/music conversion pipeline and an integration-ready
Rust audio runtime for Blockland ReImagined. This brief is for Maxwell to give to
Opus; it does not authorize Astra to start or message another agent automatically.

Read `AGENTS.md`, `docs/alpha-contract.md`, `docs/vanilla-reference.md` and the
current workspace before choosing dependencies or APIs. Preserve the original
game's identity. Native gameplay and client integration are being developed by
Astra in parallel; audio is your isolated ownership area.

## Source and boundaries

- Project: `C:\Users\Maxwell\Desktop\Games\BlocklandReImagined`.
- Authoritative read-only asset source:
  `E:\Downloads\B4v21Launcher\versions\Blockland v20`.
- Existing recovered scripts are under `.research/v20-dso/` and
  `.research/bl-decompiled/v20/`. Use them as evidence, never execute them.
  Shared assets/base files are hash-identical between the old and new installs.
- No visible game launch, desktop input, interactive playtesting or audible
  playback. Maxwell performs those. Do not open the system audio device during
  automated tests. Offline WAV output and in-memory mixer tests are allowed.
- Do not change either original installation. Do not create subagents.

Own only new `crates/audio/`, `crates/audio-import/`, `docs/research/audio/`,
generated `content/audio-pack-001/` and evidence in `artifacts/native-audio/`.
Use a fresh numbered pack if that output already exists. Do not edit existing
engine/client/UI/network crates, root manifests/lockfile or acceptance documents.
Keep new crates independently buildable with explicit local workspace boundaries
until Astra integrates them. Include the exact root integration patch as a document.
All deliverables must actually be synced to the project directory before handoff.

## Deliverables

1. **Complete inventory and native pack.** Audit all core/default sound and music
   sources, inherited audio descriptions/profiles and call sites. Initial census
   finds 104 WAVs and 15 OGGs; these are discovery counts, not an allowlist. Include
   UI/menu notes, building/tools, player/jets/footsteps, weapons/projectiles,
   vehicles, environment, brick music and event-triggered sounds where authored.
   Capture gain, pitch, looping, 2D/3D, distance attenuation and related parameters
   with file/line evidence and explicit uncertainty. Preserve original audio bytes
   where supported; document any necessary conversion. Stable IDs, versioned
   schema, hashes, provenance and missing-reference diagnostics are required.
   Torque readers belong only in the offline converter.

2. **Native Rust playback runtime.** Choose a practical maintained backend for
   Windows/macOS/Linux; justify it briefly. Use game-facing typed commands for
   one-shots, looping/attached sources, listener/source updates, stop/despawn,
   music and master/music/effects gain. Document Y-up coordinates, units, timing
   and threading. Bound voice/memory use with observable prioritization; repeated
   events must not leak voices, and missing devices/resources must fail cleanly.
   Stream longer music where appropriate. Keep device opening explicit and behind
   an output adapter so normal tests need no hardware. Do not design a mod SDK or
   change the network protocol.

3. **Evidence.** Decode/validate every inventoried clip, verify source checksums
   and profile bindings, and exercise looping, stopping, moving sources/listeners,
   gain, attenuation, channel handling and voice pressure through a hardware-free
   test backend or offline rendering. Record representative offline mixes and
   relevant peak/finite-sample diagnostics without playing them. Run tests and
   Clippy with warnings denied; report only actually built/tested platforms.

4. **Concrete handoff.** Provide a README, coverage table, unresolved behavior
   list and integration example showing how Astra's client creates the runtime,
   updates the listener and emits/stops attached sounds. Map vanilla gameplay
   triggers to stable native sound IDs. Identify settings bindings needed in the
   existing UI. List exact commands, dependencies, evidence and all files changed.

Done means all available vanilla audio is accounted for in a checked pack and the
runtime is ready to wire into real gameplay. A catalog alone or an audible demo
is insufficient. Do not claim full in-game acceptance: Astra owns integration,
and Maxwell still needs to judge the sound and timing in his actual playtest.
