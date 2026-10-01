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
