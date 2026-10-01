# 2026-10-01 Tier+Tactical: Medic 1

Kai's Medic 1 (the gauze gun and the stimpack booster) gets its port,
`weapon_package_medic1`, pinned to the Gate's real copy (sha256
1dedc7af…da01c). Its covers are checked against the real copy's text and
the CC0 stand-in.

Engine, each a generic seam:

- The emote slot (v20 image slot 3, `Player::emote`): `WeaponsWorld::emote`
  wears an image there, replacing the last; it follows its own states by
  timeout and runs their commands for the wearer (the heal over time is
  `medigunHealImage`'s looping `onHeal`). Stock emotes, pain, burn and the
  teleport flash now wear their v20 images through one `emote_cue`, so a
  scripted image and a stock emote replace each other as in v20. Death
  takes it off. Clients play an Add-On image from the cue; an empty name
  takes it off. `Player::emote`'s spam check comes from the v20 source
  (`v20-emote-source.txt`): emotes under a second apart count, ten quiet
  seconds forgive them, past five counted they are dropped; pain skips
  it, flames mount directly. It applies to the stock emotes (not sit) and
  to `emote`, unless `skip_spam`. Guard: `cargo test -p bri-sim --test
  combat quick_emotes_past_five_are_dropped_until_ten_quiet_seconds`.
- Script op `emote(player, image|()[, skip_spam])` (capability `player`, the image
  checked like `mount_image`), `player(p).emote` (the worn image),
  `bottom_print(p, text, seconds, hide_bar)` and `lan()` (`$Server::LAN`).

Importer:

- Datablock fills read a projectile's `speed` and `inherit` (the syringe
  throw's velocity).
- A `ForceRequiredAddOn` of a base-game Add-On the reference lacks reads
  as the base package (Medic needs `Weapon_Gun`); with a reference whose
  names it all resolves and none of its content named, it stays `unused`.
  Import tests list those base packages through one `tests/common` helper.

Not yet: Kai's `MedicHealEnemy` Slayer team check waits for Slayer's teams;
the port heals within the same minigame, on a LAN host outside one, and
the healer's own bots, as `TT_canHeal`'s other branches do. Its settings
wait for the settings seam.

Checks: `cargo test -p bri-weapons --test emote_slot`,
`cargo test -p bri-addon-import --test tier_port` (medic1 included), and
the wider run and clippy before the push.
