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

## Follow-up: an Add-On's own sound files

Maxwell: "Add-Ons should be able to play their own sound files like
Mario's wahoo". Add-On sound files already reached joining players: the
whole package downloads, and its weapons pack's `sounds` (its own `.wav` and
`.ogg`, hashed into the weapons identity) load into the client's pack
sounds, which every profile-named sound (weapon states, vehicle and brick
event sounds, and now archetype sounds) resolves first. So:

- The importer points `jumpSound`/`painSound`/`deathSound` naming the
  Add-On's own `AudioProfile` at that sound's id in its weapons pack
  (`<namespace>:sound/<name>`), instead of leaving it out; a profile whose
  file is missing plays nothing, as in v20.
- A hand-made Add-On names its own sound id in `sounds`
  (`docs/modding/content-kinds.md`, "Sounds").
- `crates/client/tests/fixtures/stand-in-wahoo` (CC0, a generated tone) is a
  two-file Add-On giving the standard player its own jump sound.

Evidence:
- `cargo test -p bri-addon-import --test player_sounds`.
- `cargo test -p bri-client --test add_on_join wahoo`: a guest without the
  Add-On downloads it, loads its sound, jumps and hears
  `test_wahoo:sound/wahoo`, never `player.jump`; the host hears it too.

- Hosting a plain map dropped a package catalog with no mode, world or
  rules (`crates/net/src/host_setup.rs`), so an Add-On of bodies alone (an
  imported player type, or one that `adjusts` v20's) was never installed:
  its archetypes, and their sounds, did not exist in the game. A catalog
  with archetypes now runs. `main` has the same filter.

Not changed: the `playSound` brick event offers only the base game's
sounds; an Add-On's own still plays from its rules (`play_sound`).
