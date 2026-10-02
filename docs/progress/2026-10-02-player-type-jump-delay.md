# 2026-10-02 Player types: jump delay as a whole tick count

Max's v0.1.12 build failed every join with Frog's Weaponry on:
"Archetype weapon_frogs_weaponry:archetype/deployedarmor: movement:
invalid type: floating point `0.0`, expected u8". The importer wrote a
PlayerData's `jumpDelay` as a float (`(n * 4.0).clamp(..)`), while the
motor's `jump_delay_ticks` is a u8, so any Add-On player type with
`jumpDelay` broke the archetype merge at join. It now writes a rounded
whole number (`crates/addon-import/src/player_types.rs`); it is the only
integer movement field the importer writes.

Guards: `player_types` `a_jump_delay_is_a_whole_tick_count`, and the Frog's
stand-in DeployedArmor now sets `jumpDelay = 0`, so
`tier_port.rs frogs_spinner_slows_and_deploys_and_the_launcher_slows`
failed on the old code with the same error and passes now.
`cargo test -p bri-addon-import` green; clippy clean.
