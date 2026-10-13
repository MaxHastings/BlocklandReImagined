# 2026-10-13 Player types make their own sounds

Playtest report (v0.2.11, Pharzedia): "converted playertypes don't play their
sounds" and "i am horse with standard jump sound fx, not horse jump".

## Cause

The host's `CueKind::Jump` carried nothing about who jumped, and clients
mapped it to the standard `player.jump` trigger (`JumpSound`) whatever the
body. Pain and death cries did the same with `PainCrySound` and
`DeathCrySound`. So v20's horse (`HorseArmor.JumpSound = HorseJumpSound`,
Vehicle_Horse `server.cs:218`) jumped like a Blockhead, and Bot_Hole bots
that set `jumpSound = "";//Removed due to bots jumping a lot` (Blockhead,
Zombie, Shark and Horse hole bots) still made the jump sound. The Add-On
importer reported `jumpsound` as a field without a native equivalent.

## Change

- `Archetype::sounds` (`bri_motor::archetype::Sounds`): `jump`, `pain` and
  `death`, each an `AudioProfile` name. None plays the standard player's
  sound, an empty name none. v20's horse has `jump: HorseJumpSound`.
- Package archetype files take `sounds` (`ArchetypeDef::sounds`); each one
  named replaces the base's.
- The host puts the body's sound on `CueKind::Jump`, `Pain` and `Death`
  (`sound`), and the client plays it in place of the standard trigger
  (`ClientAudio::body_sound`). Wire change:
  `crates/net/protocol-changes/player-type-sounds.md`.
- The importer converts `jumpSound`, and `painSound`/`deathSound` under
  `useCustomPainEffects` as `Player::playPain`/`playDeathCry` read them
  (`allGameScripts-Vanilla.cs:9584-9615`: no `painSound`, no cry at all).
  A profile of the game's own plays from the bank by name; a name nothing
  defines plays nothing, as in v20. An Add-On's own `AudioProfile` is not
  in the bank, so that field stays reported as left out and the standard
  sound plays (no Add-On in `content/addons` today does this).

## Evidence

- `cargo test -p bri-sim --test player_types a_player_type_jumps_with_its_own_sound`:
  a horse player's jump cue carries `HorseJumpSound`, a Blockhead's none.
- `cargo test -p bri-client --lib a_player_types_own_sounds_replace_the_standard_ones`:
  own sounds play by profile, empty ones play nothing.
- `cargo test -p bri-addon-import --lib jump_and_custom_pain_sounds_become_the_archetypes`.
- `cargo test -p bri-motor archetype`.

## Next

Bundled Add-Ons pick up their `sounds` when re-imported
(`python tools/addon_bundle.py build --missing-ok`, then `install`); until
then their bots jump with the standard sound as before. The horse needs no
regeneration.
