# 2026-10-13 Stereo weapon sounds are heard where they go off

Maxwell reported the Mini-Nuke's sound playing flat in both ears at full
volume, unlike other weapons.

## Cause

The Add-On's data was right: `mininukeFireSound` uses `AudioClosest3d` and
`mininukeExplodeSound` uses `AudioDefault3d`, so both import with
`local: false` (checked in the generated
`content/addons/weapon_mini_nuke/assets/weapons.json`). Both `.wav` files are
44.1 kHz **stereo**. The mixer copied OpenAL's rule that multi-channel buffers
are never spatialised (`crates/audio/src/engine.rs`, `gains`): a stereo clip
skipped attenuation and panning and played at full gain in both ears, from any
distance. Vanilla v20 behaved the same way through OpenAL, so this is a
deliberate departure from the reference, for an implementation flaw rather than
the sound's identity.

## Change

A voice placed in the world with a 3D description is now attenuated and panned
whatever its channel count: `mix_voice` folds a stereo frame to mono
(`(L + R) / 2`) and `gains` treats it as a mono source. 2D sounds (no 3D
description, or `Placement::Listener`) keep their channels. The rule lives in
the engine, so every stereo sound placed in the world is fixed together: any
Add-On weapon's launch, blast or projectile loop, and any base sound whose clip
is stereo.

## Evidence

- `crates/audio/tests/runtime.rs`: `placed_stereo_clips_are_spatialised`
  (panned left/right, linear falloff to half at mid range, silent at max
  distance) replaces `stereo_clips_are_not_spatialised`; it fails on the old
  engine. `unplaced_stereo_clips_keep_their_channels` keeps the 2D behaviour.
- `crates/client/src/audio.rs`:
  `an_add_on_weapons_stereo_sounds_fade_with_distance` loads a stereo `.wav`
  as an Add-On pack sound and checks it is loud at 5 units, fainter at 35 and
  silent at 100.
