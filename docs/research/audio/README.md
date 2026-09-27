# Vanilla audio pipeline — handoff

Delivered for [the Opus audio brief](../../coordination/opus-vanilla-audio.md).
Astra owns integration, and Maxwell judges sound and timing in a playtest. No
in-game acceptance is claimed.

## Status

Root integration update, 2026-09-26: both crates are now main-workspace members.
Root reran all runtime tests on Windows with BRI_AUDIO_PACK set, including both
actual-pack offline tests, then the expanded workspace tests (321 passed at this
checkpoint). Windows cpal-output tests compile and link with `--no-run`; all-target
Clippy passes with that feature. No device was opened and nothing was played.
Client settings, menu notes, ghost movement and initial authoritative jump/plant/
break/tool cues are now connected and tested silently. Music bricks, player loops/
water/death and weapon/vehicle bindings remain pending. See
[runtime integration](../../runtime-audio.md). The missing title track stays unbound.

| Deliverable | State |
| --- | --- |
| Complete inventory and native pack | `content/audio-pack-001`: 119 of 119 source files → 112 unique clips (original bytes, SHA-256), 22 descriptions, 131 sounds (129 ready, 2 unavailable because the files are missing from the reference), 255 trigger mappings, versioned schema, provenance, diagnostics |
| Repeatable converter | `crates/audio-import` (`bri-audio-import`, `bri-audio-evidence`): read-only, no script execution, deterministic, never overwrites a pack |
| Native runtime | `crates/audio` (`bri-audio`): typed commands, attached/looping sources, listener, bounded voices with observable culling, streamed music, master/channel/music gain, explicit device adapter (cpal) and hardware-free Null/Offline adapters |
| Evidence | every clip decoded; source checksums verified against Maxwell's disk; profile bindings checked; offline renders plus peak/finite diagnostics; tests and Clippy with warnings denied |
| Handoff | this README, [coverage](coverage.md), [unresolved list](unresolved.md), [integration guide + trigger map + settings](integration.md), [root patch](root-integration.patch), [runtime model](runtime-model.md), [inventory findings](inventory-findings.md) |

## Commands (Windows, from the project root)

```powershell
# Convert (writes the next free content\audio-pack-NNN; the installation is only read)
cargo run --release --locked --manifest-path crates/audio-import/Cargo.toml --bin bri-audio-import -- `
  --v20 "E:\Downloads\B4v21Launcher\versions\Blockland v20" `
  --decompiled .research\v20-dso --decompiled-label .research/v20-dso `
  --out-root content --evidence artifacts\native-audio --coverage docs\research\audio\coverage.md

# Offline evidence renders (never plays audio)
cargo run --release --locked --manifest-path crates/audio-import/Cargo.toml --bin bri-audio-evidence -- `
  --pack content\audio-pack-001 --out artifacts\native-audio

# Tests and lints (hardware-free)
cargo test --locked --manifest-path crates/audio/Cargo.toml
$env:BRI_AUDIO_PACK="$PWD\content\audio-pack-001"; cargo test --locked --manifest-path crates/audio/Cargo.toml --test real_pack
cargo test --locked --manifest-path crates/audio-import/Cargo.toml
cargo clippy --all-targets --locked --manifest-path crates/audio/Cargo.toml --features cpal-output -- -D warnings
cargo clippy --all-targets --locked --manifest-path crates/audio-import/Cargo.toml -- -D warnings

# Headless client-loop example (silent)
cargo run --release --locked --manifest-path crates/audio/Cargo.toml --example client_integration -- content\audio-pack-001
```

Standalone crates keep their own `target/` (ignored per crate). After the root
patch, use the normal workspace commands instead.

## Dependencies added (standalone crates only; root manifests untouched)

`symphonia` 0.6 (wav, pcm, ogg, vorbis), `cpal` 0.18 (optional feature
`cpal-output`), `rtrb` 0.3, `serde`/`serde_json` 1, `sha2` 0.10, `zip` =6.0.0.
The rationale is in the [runtime README](../../../crates/audio/README.md#backend).

## Evidence (`artifacts/native-audio/`)

| File | Content |
| --- | --- |
| `audio-pack-001-clip-validation.json` | per clip: sha256, format, rate, channels, bits, frames, duration, peak, RMS, packets, corrupt packets (0), non-finite samples (0) |
| `audio-pack-001-import-summary.json` | converter totals; runtime self-check (129 sounds loaded, hashes verified, 10.1 MB resident, 15 streamed clips); binding check (all 87 datablock bindings resolve to playable sounds; only TitleMusic/AudioButtonOver triggers are unavailable) |
| `reference-source-hashes-device.tsv` | `sha256sum` of every audio file, archive, script and DSO in the reference install, **taken on Maxwell's machine** |
| `audio-pack-001-source-verification.json` | 183 of 183 inventory hashes match the device hashes; 184 of 184 staged copies match (`verify_source_hashes.py`) |
| `audio-pack-001-offline-renders.json` | every sound rendered once through the runtime (131: 129 played, 2 fail cleanly as unavailable; 0 render failures), plus 9 scenes with per-channel peak/RMS, finiteness, clipping, voice and event counts |
| `audio-pack-001-renders/*.wav` | 16-bit 48 kHz stereo offline mixes: menu notes, building, player, gun distance falloff, rocket fly-by with attached loop and despawn, spray loop stop, streamed music brick walk-away and music toggle, voice pressure, gain buses. **Not played by me.** |
| `test-and-clippy.log` | test and Clippy gate output (see below) |

Sync check: all 170 delivered files (crates, docs, pack, evidence) were re-hashed in the project folder on Maxwell's machine and are byte-identical to the verified originals.

How this pack was produced: the converter ran in Claude's Linux workspace
against a staged copy of the reference files. Every staged file was re-hashed
against hashes computed on Maxwell's machine. That is why
`manifest.json → generator.arguments` shows a staging path. Re-running the
command above on Windows directly against `E:\...` should produce identical
clips and ids; only `generator.arguments` will differ.

## Platforms actually built and tested

- **Linux x86_64** (Rust 1.93.1 and 1.95.0): all tests run with the pack
  present, and Clippy passes with `-D warnings`, with and without `cpal-output`.
  The cpal/ALSA backend is compiled but never opened.
- **Windows x86_64 MSVC** and **macOS aarch64** (Rust 1.93.1): `cargo clippy
  --all-targets -D warnings` type-checks only, including the cpal WASAPI and
  CoreAudio backends. Not linked, not run.
- No audio device was opened on any platform. The Windows build, device output
  and all listening are left to Maxwell and Astra.

## Files changed

All new. No existing engine, client, UI, network, root-manifest, lockfile or
acceptance files were edited. The original installations were not modified.

- `crates/audio/`: `Cargo.toml`, `Cargo.lock`, `.gitignore`, `README.md`,
  `src/{lib,schema,error,decode,spatial,bank,command,engine,runtime,device,wav}.rs`,
  `tests/{common/mod.rs,runtime.rs,bank.rs,real_pack.rs}`,
  `examples/client_integration.rs`
- `crates/audio-import/`: `Cargo.toml`, `Cargo.lock`, `.gitignore`, `README.md`,
  `src/{main,convert,model,report,tscript,vfs}.rs`, `src/bin/audio_evidence.rs`
- `docs/research/audio/`: `README.md`, `coverage.md` (generated),
  `inventory-findings.md`, `runtime-model.md`, `unresolved.md`,
  `integration.md`, `root-integration.patch`
- `content/audio-pack-001/` (generated, git-ignored): `manifest.json`,
  `inventory.json`, `coverage.md`, `README.md`, `clips/` (112 original files)
- `artifacts/native-audio/` (git-ignored): the evidence listed above

## Suggested `docs/progress.md` entry (for Astra to adopt; not edited here)

> Opus delivered the vanilla audio pipeline: `crates/audio-import` →
> `content/audio-pack-001` (119 source files, 112 unique clips, 131 sounds,
> 129 ready; TitleMusic and AudioButtonOver files are absent from the
> reference) and the `crates/audio` runtime (cpal device adapter plus
> hardware-free outputs). Evidence is in `artifacts/native-audio`. Integration:
> `docs/research/audio/integration.md`. Audio acceptance items stay open until
> gameplay wiring and Maxwell's playtest.
