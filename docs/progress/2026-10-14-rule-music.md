# 2026-10-14 Music from Add-On rules

Rules can now play, stop, loop and fade a music track for one player or for
everyone, for v0.3.1 (Max approved it from the modder gaps check).

- Script API (`effects`): `play_music(p, track[, #{ looping, fade }])`,
  `play_music_all(track[, options])`, `stop_music(p[, fade])`,
  `stop_music_all([fade])`, one op (`ops::Music`). `loop` is a Rhai
  keyword, so the option is `looping`.
- Host: `Notice::Music(MusicPlay)` to the player; the everyone track is kept
  on the session and sent to joiners with the Music Files notice, and a map
  change stops it. Wire change file `crates/net/protocol-changes/rule-music.md`.
- Client: one rule music voice, played as the track's asset with the music
  bus, no 3D placement and the requested looping (`SoundAsset::as_music`);
  fade in ramps `set_voice_params` per frame, fade out is `stop_with_fade`.
  A track is an Add-On pack sound or a bank sound (`musicData_*`).
- Add-On sounds marked `"music": true` in `weapons.json` load through
  `SoundAsset::music_file`: decoded once to validate, then streamed, up to
  10 minutes and 8 MiB instead of the 30-second sound cap. The checker
  uses the same loader.

Evidence: `cargo test -p bri-sim --test script_api rules_play_and_stop_music`
(everyone, joiner, stop, one player) and `cargo test -p bri-audio --test bank
an_add_on_music` (a 40 s WAV is refused as a sound, loads as streamed music).

Not done: no in-game listen check (Maxwell's playtest), no Add-On using it.
