# bri-audio — native audio runtime

Game-facing audio for Blockland ReImagined. It plays the converted vanilla pack
(`content/audio-pack-NNN`, produced by `crates/audio-import`) with the authored
Torque parameters: description volume, looping, 2D/3D, reference/max distance
and vanilla channel. It contains no Torque readers.

Status: a root workspace member. The client plays sounds through it
(`crates/client/src/audio.rs`) with the `cpal-output` feature on.

## Backend

Output goes through **cpal 0.18** (feature `cpal-output`, off by default):

- It is the maintained, lowest-level cross-platform output crate in Rust. It
  targets WASAPI on Windows, CoreAudio on macOS and ALSA on Linux (JACK and
  PulseAudio/PipeWire through ALSA). rodio and kira are built on it.
- A small mixer of our own sits on top, not rodio or kira. Fidelity needs
  engine-side Torque linear rolloff, the TGE gain table and TGE-style voice
  culling. It also needs the exact same mixer to run with no device for tests
  and offline evidence. Higher-level engines hide or reimplement these parts.
- Decoding uses **Symphonia 0.6** (pure Rust, maintained). It reads WAV PCM
  8/16-bit and Ogg Vorbis, the only formats in the vanilla content.

Opening a device is always explicit (`OutputKind::Device`). Tests, CI and
headless servers use `OutputKind::Null` or `OutputKind::Offline`, which run the
same mixer inline and never touch hardware.

## API in one screen

```rust
let bank = Arc::new(SoundBank::load("content/audio-pack-002", &BankOptions::default())?);
let (mut audio, why) = AudioRuntime::open_or_null(bank.clone(), RuntimeConfig::default());
// or AudioRuntime::new(bank, cfg, OutputKind::Offline | Null | Device)

audio.set_listener(Listener { position, forward, up })?;              // once per frame
let h = audio.play("JumpSound", Placement::Attached { entity: EntityKey(id), position })?;
audio.play_trigger("brick.plant", Placement::World(pos))?;            // vanilla trigger table
audio.play("AdminSound", Placement::Listener)?;                       // 2D
audio.update_entity(EntityKey(id), new_pos)?;                         // moves all attached sounds
audio.stop(h)?;                                                       // 5 ms declick
audio.despawn(EntityKey(id))?;                                        // entity deleted: stop its sounds
audio.set_volume(VolumeControl::Master, 0.9)?;
audio.apply_ui_volume("sim", 0.5)?;                                   // bri_ui UiAction::SetVolume
audio.set_music_enabled(false)?;                                      // $Pref::Audio::PlayMusic
audio.update(dt);                                                     // per frame (mixes inline outputs)
for ev in audio.drain_events() { /* Started/Finished/Stopped/Culled/Virtualized/Revived */ }
let stats = audio.stats();                                            // voices, culls, peak, clipping...
```

Sounds are addressed by stable id (`v20/sound/jumpsound`, `v20/music/peaceful`)
or by vanilla datablock name (`JumpSound`, `musicData_Peaceful`), without regard
to case. See `examples/client_integration.rs` for a complete client loop.

## Conventions

- **Coordinates:** native world space, right-handed, **Y up**, in original Torque
  world units. Converted content maps Torque `(x, y, z)` to native `(x, z, -y)`.
  Distances do not change, so authored `referenceDistance`/`maxDistance` apply
  as-is. Listener right is `forward x up`.
- **Units:** gains are linear (0..=4 accepted; vanilla uses 0..=1). Times are in
  seconds. The mixer's sample rate is the device rate (48 kHz for inline outputs
  by default).
- **Timing:** commands apply at the start of the next mixer block. That is 256
  frames, about 5.3 ms at 48 kHz, plus the device buffer. Gains ramp linearly
  across each block, so moving sources and volume changes do not click. A new
  voice starts at full gain with no fade-in, so transients match vanilla. A stop
  fades out over 5 ms. Pitch is 1.0 because no stock description sets pitch.
- **Threading:** `AudioRuntime` lives on the client thread and is `Send`.
  Commands go through a bounded lock-free SPSC ring (`rtrb`, 4096 entries).
  Events and counters come back through a second ring and atomics. With device
  output, a dedicated `bri-audio-output` thread owns the cpal stream and the
  mixer runs in its callback. Loading and decoding happen on the caller's thread
  inside `SoundBank::load`. The mixer does no I/O and no name lookups. Streamed
  music decodes incrementally in the callback; it allocates only when refilling
  its small decode buffer or restarting a loop.

## Playback model (evidence in `docs/research/audio/runtime-model.md`)

- **Attenuation:** Torque linear rolloff. Gain is 1 inside `referenceDistance`,
  0 at and beyond `maxDistance`, and linear in between. OpenAL runs with
  `AL_NONE`, so the engine applies attenuation itself.
- **Gain curve:** the linear product `volume x channel x master x attenuation`
  maps to amplitude through the TGE `linearToDB` table, interpolated
  (`GainCurve::TorqueTable`, the default). `GainCurve::Linear` is available if
  Maxwell's A/B listening prefers it.
- **Panning:** equal-power stereo from listener-relative azimuth for mono
  sources. Multi-channel clips are not spatialised, as in OpenAL. 2D sounds play
  centred.
- **Placement rule:** a 3D description with `Placement::Listener` plays 2D,
  exactly like vanilla `alxPlay(profile)` and `client.play2D(profile)`.
- **Voices:** `VoicePolicy::default()` matches TGE:
  - 16 real voices.
  - A one-shot whose gain (excluding master) is at or below 0.05 does not start.
  - When no voice is free, the quietest real voice that is quieter than the new
    sound is taken. A looping voice taken this way becomes *virtual*: silent,
    but its position and time keep running. It revives after 500 ms if its gain
    is above 0.1. A one-shot taken this way ends.
  - At most 128 virtual voices.
  - Every decision is visible as an `AudioEvent` and in `AudioStats`. Repeated
    events cannot leak voices, because one-shots retire, loops are bounded and
    slots are preallocated.
- **Buses:** master and vanilla channels 0..8. Channel 1 is the "Shell"
  (interface) slider and channel 2 the "Sim" (effects) slider. The native
  `Music` bus multiplies on top for title and music-brick loops. Disabling music
  suspends music loops, and re-enabling restarts them from the top, as vanilla
  re-applies emitter profiles. Unlike vanilla's `alxStopAll`, other sounds keep
  playing.
- **Output:** the mix is hard-clamped to ±1, with clipped samples counted. There
  is no limiter, and vanilla OpenAL had none either. Non-finite samples cannot
  reach the device.

## Failure behaviour

| Situation | Result |
| --- | --- |
| No device, or the device fails to open | `Err(NoDevice/Device)`; `open_or_null` returns a working Null runtime and the reason |
| Built without `cpal-output` | `Err(Unsupported)` for `OutputKind::Device` |
| Unknown sound id or name | `Err(UnknownSound)` |
| Authored file missing from the reference (e.g. `TitleMusic`) | `Err(SoundUnavailable { reason })` |
| Corrupt or modified pack file | `SoundBank::load` → `Err(Integrity)` (SHA-256 verified) |
| Decoded PCM over budget (64 MiB default) | `Err(MemoryBudget)` at load |
| Command queue full | `Err(QueueFull)`, counted in `stats().commands_dropped` |
| NaN/inf positions or an invalid listener | ignored; the previous value is kept |
| Stream decode error | voice ends with `Culled { reason: StreamError }` |

## Commands

```powershell
cd crates/audio
cargo test --locked                                   # hardware-free; asset tests skip without a pack
$env:BRI_AUDIO_PACK="..\..\content\audio-pack-002"; cargo test --locked --test real_pack
cargo clippy --all-targets --locked -- -D warnings
cargo clippy --all-targets --locked --features cpal-output -- -D warnings
cargo run --release --example client_integration -- ..\..\content\audio-pack-002   # offline, silent
```

Only Maxwell runs audible output:
`cargo run --release --features cpal-output --example client_integration -- ..\..\content\audio-pack-002 --device`.
