# Slayer's bots through a generic rules-bots seam

Slayer 4.1.5 fills teams with bots: a team's Preferred Player Count
(`botFillLimit`) adds `BlockheadHoleBot`s named "Bot" and a random first
name until the team has that many members, and sends them away as players
join. This ports it through generic seams, with the Blockhead Bot Add-On as
the bot kind (Slayer needed Bot_Hole and Bot_Blockhead; ours is both).

## Engine
- **Rules bots** (capability `bots`): `add_bot(game, #{ kind, name, team })`,
  `remove_bot`, `rest_bot`, `bot_tool`, `bot_kinds()`, `bot_limit()`, and a
  player's `spawner`. They share the 16-bot cap with spawn-brick bots. The
  engine brain spawns them where members spawn, roams from wherever they
  stand (`hReturnToSpawn` off), fights whoever the damage rules allow,
  respawns them as soon as the game lets it, and drops them when their game
  ends or they leave it. A package can only touch its own bots.
- Rules bots are members to the rules: `on_spawn`, `on_loadout`, `on_leave`,
  `on_pick_spawn`, zones and player state keys include them (state is
  dropped with the bot). Spawn-brick bots still hear none of it. Join and
  leave chat stays quiet for them.
- `bot_arm` keeps a weapon the rules put in hand.
- `message_box(p, title, text)` (`MessageBoxOK`, capability `chat`).
- Bot kinds carry `first_names` (at most 256, 1-16 chars); the Blockhead Bot
  Add-On has 46 of our own.
- The minigame map has `points_kill_player`.
- No wire change, so no protocol file.

## Slayer rules
`botFillTeam`, `updateBotFillLimit` (refused with "Slayer | Error" without
the bot Add-On), `addBotToGame`, Kill Bot points (`GameConnection::onDeath`),
the Bot damage setting for hole bots (`canDamage`), Bot respawn time,
`useRandomTool`, a random skin in uniforms, rest during the countdown and
between rounds (`stopHoleLoop`/`resetHoleLoop`), `endGame`/`onRemove`
deleting bots, and `/teams listmembers` counting them. Objectives, rally
nodes and team bot holes are dead code in 4.1.5; `assignObjectives` is
pinned as empty in ports.json.

## Tests
- `crates/sim/tests/script_api.rs`
  `a_mini_games_rules_add_rest_arm_and_take_away_their_own_bots`.
- `crates/addon-import/tests/slayer.rs`
  `a_teams_preferred_player_count_fills_it_with_bots`.
- `cargo test -p bri-sim -p bri-package-runtime -p bri-package
  -p bri-addon-import -p bri-net -p bri-chaos` and clippy on the touched
  crates pass.

## Next
`.pathcam` saving (needs saved mini-game settings).
