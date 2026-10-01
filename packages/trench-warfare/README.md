# Trench Warfare

A classic Blockland game mode: Red against Blue on a field of dirt. Dig in
while the ceasefire holds, pile the dirt back up as cover, then fight
across the trenches. The first team to 20 kills wins the round.

**Being reworked to load the originals.** Max chose to play classic
Add-Ons from the player's own Blockland files (2026-10-01): the digging
tool, its models, icons and sounds come from the player's copy of the
original Add-On at runtime and are never committed or shipped. Our own
look-alike pick (`trench-kit`) is gone, and none of these Add-Ons is on the
default list. What stays here is our rewrite of the behaviour (the rules,
the HUD and the mode), which becomes the port of the original once its
names are known.

| Add-On | Side | What it is |
|---|---|---|
| `trench` | host | the rules and the battlefield: a generated world of dirt, clay and rock (`field.json`, `trench.rhai`), teams and uniforms, the dirt bag, rounds and scores |
| `trench-kit` | everyone | the digging tool: the player's copy of the original Add-On (not in this repository; `crates/sim/tests/trench.rs` uses a stand-in) |
| `trench-hud` | each player | the panel: the round's state, your team, your dirt and both teams' scores |
| `trench-mode` | host | the game mode: the field as its map, and its own mini-game with the pick, Gun, Spear and Sword and the no-jet player |

## How it plays

- **Pick.** Left click digs the cube you aim at into your bag (50 cubes).
  Right click piles one back onto the face you aim at. A box shows the
  cube the pick will dig. Rock, bedrock and sandbags do not dig, and
  nothing at the bases does.
- **Rounds.** 45 seconds of ceasefire to dig in (no damage), then 8
  minutes of fighting. First to 20 kills, or the team ahead when time
  runs out, wins the round. Ten seconds later the next one starts on the
  same field, trenches and all.
- **Teams.** Players join the smaller team and wear its colours. No
  friendly fire. Leaving the field puts you back at your base.
- **Commands.** `/givedirt <amount> <name>` hands dirt to a teammate,
  `/teams` shows the teams and kills, and admins can `/newround`.

## Credits

The design comes from Blockland's **Trench Digging** by lilboarder32 (a
remake of an old v8 mod), **Trench Digging Plus** by Platypi
([Blockland Glass](https://blocklandglass.com/addons/addon/829)), and the
Trench Wars servers that played them. The rules here are our own
implementation; none of their code is used, and their models, textures
and sounds are only ever read from the player's own copy.

Tests: `cargo test -p bri-sim --test trench`.
