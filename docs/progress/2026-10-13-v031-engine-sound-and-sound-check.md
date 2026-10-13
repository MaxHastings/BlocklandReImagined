# 2026-10-13 v0.3.1: vehicle engine sounds and decoded sound checks

Two engine gaps from Maxwell's v0.3.0 preview playtest, fixed as generic
capabilities for any Add-On rather than for the Halo or Mario Add-Ons. The
branch starts from the `v0.3.0-experimental` head so it rebases after
v0.3.0 ships.

## Vehicle engine sound

The Halo Warthog Add-On ships an engine loop, but no vehicle could play
one: vehicle sounds were one-shot cues only, and the mixer fixed a voice's
pitch and gain when it started.

- `bri-audio`: `AudioRuntime::set_voice_params(handle, gain, pitch)` changes
  a playing voice's volume and pitch multipliers in place
  (`Command::SetVoiceParams`; `Voice::pitch_scale` multiplies the resample
  step). Gain glides over one block like distance changes; gain is clamped
  to 0..4, pitch to 0.25..4, non-finite values are ignored or zeroed.
- `bri-vehicles`: `Definition::engine_sound: Option<EngineSound>`
  (`sound`, `idle_pitch` 0.8, `full_pitch` 1.6, `idle_volume` 0.6,
  `full_volume` 1.0, `full_speed` defaulting to `max_speed`), validated in
  `Pack::validate`. Optional, so no schema bump (as `gravity_scale`).
- Client: `audio::engine_loop` decides per vehicle per frame (running while
  the seat that drives it is taken, any seat when none drives; off when
  destroyed), `ClientAudio::sync_engines` keeps one positional loop per
  vehicle, moves it, sets its pitch and volume from presented speed, and
  fades it out over 0.15 s. Driven from `app/frame.rs` beside the image
  loops. Nothing new goes over the wire, so no protocol change.
- Speed drives it, not throttle: throttle is only known for the locally
  driven vehicle, and every player should hear a vehicle the same way.
- The Playground buggy carries a generated 1 s engine loop
  (`tools/make_playground_assets.py`, no random noise, so the other
  generated files are byte-identical) as the reference.
- Docs: `docs/modding/content-kinds.md` (vehicles), `weapons.md` (pack
  `sounds`), `README.md` (the checker).

## The Add-On checker decodes sounds

`bri-addon-check` never opened sound files, so 79 Halo sounds with an
unreadable header passed and then played silence. Now:

- `SoundAsset::decoded_file` is the one path that reads and decodes an
  Add-On sound (bounded read, Symphonia via `decode_all`, length limits);
  the client's `PreparedSounds::load` and the checker both call it, so a
  file the checker passes is a file the game plays.
- Every non-`stock` sound in each checked weapons pack is decoded; one that
  fails is an error, `check.sound`, naming the profile, file and reason.
  `stock` sounds are the installed game's and are not checked.
- A vehicle's `engine_sound` naming an Add-On sound no pack in the set
  declares is an error; one that does not loop is a warning
  (`check.vehicles.engine_sound`).

## Evidence

- `cargo test -p bri-audio --test runtime a_playing_loop_changes_volume_and_pitch_in_place`:
  half gain halves the RMS (linear curve), double pitch doubles zero
  crossings, NaN/inf leave the mix finite, and the loop never finishes.
- `cargo test -p bri-package-runtime --test check`: an ADPCM-tagged WAV,
  a garbage `.ogg` and a missing file each fail by name while a PCM WAV and
  a stock sound pass; the buggy checks clean; a non-looping engine sound
  warns and an undeclared one fails.
- `bri-client` unit test `an_engine_runs_with_its_driver_and_rises_with_speed`.

## Next

- Maxwell to hear the buggy and the Warthog in a playtest; tuning the
  defaults is a data change.
- The client-code Add-On sounds (`client.sounds`, `bri-client-sandbox`)
  still check only extension and size.
