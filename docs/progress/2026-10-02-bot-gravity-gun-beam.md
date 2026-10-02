# A bot's Gravity Gun beam shows

Max (v0.1.13 playtest): a Blockhead bot grabbing him with the Gravity Gun
showed none of the gun's effects; a player's gun did.

## Cause

The effects Add-On draws each player's `beam` from the Gravity Gun's
public player state. `Session::package_view` left bots out of the player
state it replicates, so clients never saw a bot's beam. The gun's
`on_tick` also only walked `players()`, so a bot's beam would stay on
after it died or put the gun away.

## Fix

- `package_view` replicates every peer's public state, bots included.
- `gravity.rhai` `on_tick` walks `players() + bots()`.

## Evidence

`crates/sim/tests/showcase.rs` `a_bots_beam_shows_on_everyones_screen`:
the builder's view holds the bot's beam `[2, builder, 1, ..]` while held,
and beam off after the bot dies. Fails without either change ("the bot's
beam reaches the builder"; "off once it died: [2,1,1,2.5]"). sim showcase,
packages, hardening_packages, script_api and net package_sync/showcase
pass; clippy clean.
