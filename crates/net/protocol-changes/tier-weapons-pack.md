Weapons packs players download carry new fields (Tier+Tactical ports):
`Shot::free`, `Shot::hitscan` (`explosion`, `flown`, `player_sound`,
`other_sound`), `Shot::rested`, magazine `Check::spend` and
`Check::keeps_reload`, `ImageCommands::mount`, and `Pack::external_projectiles`
(a pack firing another package's projectiles), `Item::rotate` (an item that
turns where it lies), magazine `display_ticks` and `display_scripts`. A
holder's magazines are kept per tool slot. Archetype looks carry
`first_person_only` (v20 `firstPersonOnly`).
Explosive 1: magazine `from_reserve` (grenades counted from the reserve),
state `arm_once`, children `max_count` and `steps`, aura `players_only`,
`effect`, `target_sound` and `max_pulses`.
Explosive 2: children `on_hit`, `angles`, `redraw` and `max_times`, aura
`max_targets`, and the image shot's `lob`.
Medic 1: the emote cue carries an Add-On image id, or an empty name to
take the worn image off; script op `Emote` (with `skip_spam`),
`Print::hide_bar`, and `PlayerView::emote`.
Melee Extended: hitscan `sounds` (pairs drawn per shot) and `damage`;
state shots may hitscan.
Melee Extended II: `Image::guard` (a raised shield's cover, damage and push
scales, reflection, clang, sounds, durability and break burst) and
`DamageType::special` (Support_SpecialKills icons). Content schema only.
Short Rifle: hitscan `ricochet` (`times`, `damage`, `shooter`). Damage types
and explosions two Add-Ons declare differently are kept apart as
`<package>:<Name>` in the merged pack. Content schema only.
Weapon fields from server settings: `Pack::bindings` (a pack field a
server setting decides), magazine `supply` and `hide_display`; and
`Checkpoint::weapon_settings` and `Delta::weapon_settings`: the values of
the settings the host's weapons pack binds, so players derive the same pack.
