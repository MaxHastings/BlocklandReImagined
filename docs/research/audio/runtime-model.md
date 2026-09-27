# Vanilla audio behaviour model and evidence

This page explains how the runtime reproduces v20 audio behaviour and how
confident each rule is. Line numbers point into the recovered scripts in
`.research/v20-dso/`. All 16 `.dso` originals are hash-identical to the
reference installation's (`inventory.json` → `scripts[].original_dso`). Scripts
are evidence only and were never executed.

Confidence levels:

- **Authored:** read directly from v20 script/datablock text.
- **Engine-family:** from TGE-family engine source. The reference is OpenMBG
  `engine/source/audio/audio.cc` at commit
  `9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7`, the same commit the project
  already uses for `.research/openmbu-reference/mbg-*`. The Blockland v20
  binary was **not** disassembled.
- **Native choice:** a deliberate modern decision, listed so it can be revisited.

## Parameters per sound (authored)

| Parameter | Source | Runtime field |
| --- | --- | --- |
| Clip | `AudioProfile.fileName` (`~/`, `./` and plain paths; case-insensitive) | `SoundEntry.clip` |
| Gain | `AudioDescription.volume` | `Playback.gain` |
| Looping | `isLooping` | `Playback.looping` |
| 2D/3D | `is3D` | `Playback.spatial` |
| Distances | `ReferenceDistance`, `maxDistance` | `Spatial` |
| Channel | `type` (`$GuiAudioType=1`, `$SimAudioType=2`, `$MessageAudioType=3`, client script lines 1-3) | `Playback.channel` |
| Pitch | never authored in stock content | 1.0 |
| Cones, environment | never authored (engine defaults 360/360/1 and 0) | not applied |
| Streaming | `isStreaming` never authored | native choice: music streams (see below) |

The 22 stock descriptions are listed in `coverage.md`. The commonly used ones:

| Description | ref/max | Loop | Channel | Defined at |
| --- | --- | --- | --- | --- |
| AudioDefault3d | 20/100 | no | 2 | `allGameScripts-Vanilla.cs:6876` |
| AudioClose3d | 10/60 | no | 2 | `:6885` |
| AudioClosest3d | 5/30 | no | 2 | `:6894` |
| AudioDefaultLooping3d | 20/100 | yes | 2 | `:6903` |
| AudioCloseLooping3d | 10/**50** | yes | 2 | `:6912` |
| AudioClosestLooping3d | 5/30 | yes | 2 | `:6921` |
| AudioMusicLooping3d | 10/30 | yes | 2 | `:20246` |
| AudioGui | 2D | no | 1 | `allClientScripts-Vanilla.cs:4` |
| AudioClientClose3d | 10/60 | no | 2 | `:143` |
| AudioBGMusic2D | 3D 10/60, volume 0.8 | yes | 1 | `:128` (played through `alxPlay`, i.e. 2D) |

## Distance attenuation (engine-family, high confidence)

TGE calls `alDistanceModel(AL_NONE)` ("similar to DSound model w/o min distance
clamping", `audio.cc` `prepareContext`). Each update, `alxUpdateMaxDistance`
then sets the gain itself:

    atten = 1 - (clamp(dist, ref, max) - ref) / (max - ref)
    gain  = linearToDB(sourceVolume * channelVolume * masterVolume * atten)

`bri_audio::spatial::torque_attenuation` implements the first line exactly. It
is unit-tested and shown in the offline render `04_gun_distance`: 0 and 10 units
give identical RMS, then 20/35/50 units fall off linearly, and 59/70 units are
not started.

## Gain curve (engine-family, medium confidence)

`linearToDB` is a 128-entry lookup table (`audio.cc` `logtab`) applied to the
product above. The runtime copies it verbatim (verified by script against the
source). The only change is interpolation between entries, so a moving source
does not change level in 128 audible steps. Consequence: at the stock master
volume of 0.9 (`base/client/defaults.cs:48`), full-volume sounds come out at
amplitude 0.41, about -7.7 dB. **Uncertainty:** the Blockland binary was not
checked for this table. Maxwell should A/B `GainCurve::TorqueTable` against
`GainCurve::Linear` in a playtest. Switching is one config field.

## Voice budget and culling (engine-family, medium-high confidence)

| TGE constant | Value | Runtime (`VoicePolicy`) |
| --- | --- | --- |
| `MAX_AUDIOSOURCES` | 16 | `max_real_voices = 16` |
| `MIN_GAIN` | 0.05 — non-looping sources at or below it (volume x channel x approx. attenuation, **excluding master**) are not started | `min_start_gain` |
| `cullSource` | when no source is free, take the lowest score below the new score | same |
| looping images | culled loopers go to an inactive list instead of stopping | virtual voices |
| `MIN_UNCULL_PERIOD` / `MIN_UNCULL_GAIN` | 500 ms / 0.1 | `uncull_delay_ms`, `uncull_gain` |

Native differences, chosen for predictability:

- Scores use the linear attenuation above. TGE's per-update score for real 3D
  sources uses `ref/dist`.
- Virtual PCM loopers keep advancing their play position. TGE restarts the
  buffer on uncull.
- The virtual list is bounded (128). A new looper beyond the bound is rejected
  with `CullReason::VirtualLimit`.

## 2D, 3D and placement (authored)

- `alxPlay(profile)` has no transform, so it plays 2D even if the description
  is 3D. This covers the title music with its 3D `AudioBGMusic2D` description.
  `Placement::Listener` reproduces it.
- `GameConnection::playSound` (`allGameScripts-Vanilla.cs:18177`) accepts only
  non-looping **3D** profiles and plays them through `client.play2D`, i.e. 2D.
  `fxDTSBrick::playSound` (`:17499`) plays the same kind of profile 3D at the
  brick through `ServerPlay3D`, unless the brick's fake-dead time exceeds 120
  (units not verified).
- `ShapeBase::playAudio(slot, profile)` attaches to the object; deleting the
  object stops it. That maps to `Placement::Attached` plus `despawn`. Projectile
  `sound` loops and image `stateSound`s follow their objects too.
- Explosion `soundProfile`s and `ServerPlay3D` calls use a fixed position
  (`Placement::World`).

## Multi-channel clips (engine-family)

OpenAL does not spatialise multi-channel buffers. The runtime plays stereo clips
unpanned and unattenuated. Only three Return to Blockland UI sounds are stereo.
Vanilla deletes stereo *music* datablocks (`createMusicDatablocks`), and the
converter reproduces that rule; all 15 stock tracks are mono.

## Music (authored, plus a native streaming choice)

- Music datablocks are generated at load (`allGameScripts-Vanilla.cs:20255`;
  launcher-patch override at `base/server/scripts/allGameScripts.cs:279`). The
  rule is documented in `crates/audio-import/README.md`. All 15 stock tracks are
  accepted.
- A music brick plays a *looping* `AudioEmitter` at the brick
  (`fxDTSBrick::setSound`, `:11500`, requires `specialBrickType "Sound"`, a
  looping description and a uiName). That maps to `Placement::Attached` on the
  brick entity, and `despawn` when the brick goes.
- `$Pref::Audio::PlayMusic` off calls `alxStopAll()`; on re-applies every
  `AudioEmitter` profile (client script `:2732`). Native choice: only music
  loops suspend, and they restart from the top when re-enabled.
- Native choice: every music clip is marked `stream = true`. The runtime keeps
  the original compressed Ogg bytes (about 1.5 MB total) and decodes
  incrementally instead of holding decoded PCM for the 77 s After School
  Special. Resident memory for the whole pack is about 10 MB.

## Channels and settings (authored)

- Options sliders: `OptAudioVolumeMaster` → `$pref::Audio::masterVolume`
  (listener gain), `OptAudioVolumeShell` → channel 1, `OptAudioVolumeSim` →
  channel 2 (client script `:2528-2530`, `:3613-3647`).
- The stock options test tone is `lightOn.wav` on `AudioChannel<n>`.
- Stock defaults (`base/client/defaults.cs`): master 0.9, channels 1..8 at 1.0,
  PlayMusic 1, MenuSounds 1, PlantErrorSound 0 (the later assignment wins),
  PlayBrickMoveSound 1, PlayBrickPlantSound 1.
- **Dedicated-server quirk:** `$SimAudioType` is defined only in the client
  script. A dedicated server would evaluate `type = $SimAudioType` in server
  datablocks as 0. The converter resolves the global across all scripts, as on a
  listen server (type 2). See `unresolved.md`.

## Engine-bound client profiles (authored plus binary strings)

`BrickBreak`, `BrickChange`, `BrickMove`, `BrickPlant` and `BrickRotate` are
client-local `new AudioProfile`s (client script `:152-180`) followed by
`loadBrickSounds()` (`:182`). The names appear as strings in `blocklandv20.exe`
next to `BrickBreakSoundEvent`, `$Pref::Audio::PlayBrickMoveSound` and
`$Pref::Audio::PlayBrickPlantSound`. The engine plays them; the exact emit
position (ghost brick versus planted brick) is inferred, not verified.

## No authored footsteps, jets or environment sounds

- `PlayerStandardArmor` sets only `JumpSound`, `impactWater*` and
  `exitingWater`. There are no footstep, jet or land-impact sound fields.
- The jet add-ons (`Player_*Jet`) contain no audio.
- Map missions contain no `AudioEmitter` or other sound objects. Checked in all
  14 map archives and `newMission.mis`.
- `ArmorMoveBubblesSound` and `WaterBreathMaleSound` are defined but bound by no
  datablock field or script. They stay in the pack, flagged `unbound-profile`.
