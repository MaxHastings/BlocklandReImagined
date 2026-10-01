# 2026-10-01 Slayer's team uniforms, kits and player types

Branch `claude/project-thread-t8k5dx` (Slayer/CTF).

## Engine seams (generic, capability-gated)

- `set_avatar_parts(p, #{ slot: part, face, decal })` (`player`): a rule
  dresses a player in avatar parts over their own (`hideAllNodes`,
  `unHideNode`, `setFaceName`, `setDecalName`). The pack's own spelling is
  kept; parts the server's pack lacks stay the player's, and the pack's
  accent repair runs after. `()` gives them their own. Kept across respawns.
- `avatar_choices()`: the avatar pack's lists in v20's order, plus
  `accents.<hat>` for each hat's accents, so a rule can read v20 list
  positions.
- `set_tools(p, [item, (), ...])` (as `give_item`): every tool slot at once
  (`forceEquip`), unknown items left out, a held brick re-held.
- `set_respawn_time(p, ms)` (`minigame`): a player's own respawn wait over
  the mini-game's (`setRespawnTime`), cleared by `()` and when they leave
  the game.
- `minigame(id).player_type` and `.loadout`: the game's own player type and
  start tools.
- Setting types `item` and `player_type`: a content id or `""`, checked
  against the server's items and player types; the Mini-Game window lists
  them.
- `Vitals.team`: name tags take the team's colour (protocol file
  `team-kits.md`).
- Rules templates: `{{name|lower}}`.

## Port

- Slayer's team settings Playertype, Start Equip 0 to 4, Sync w/ Minigame
  Loadout, Player Scale (1 to 5; the original's 0 is not a body size),
  Respawn Time and Uniform with its Custom parts and colours (index into
  v20's lists, `TEAMCOLOR`), and the mini-game's Allow Custom Face Decals.
  Defaults come from the copy's `team-preferences.cs` and `preferences.cs`;
  the Full uniform's skin colours from `Slayer_AiController.cs`.
- `createPlayer`: the team's kit on each new body (a synced team keeps the
  game's), then the uniform; the countdown freeze gives back the team's
  player type at the start (`startRound`).
- Live changes: `updateUniform` (and team colour changes), `updateDatablock`,
  `updatePlayerScale`, `updateRespawnTime` and `updateEquip` (a new start
  tool only where the member still carries the old one).
- Checked against the real copy: every pin matches and the defaults read
  as the original's (Full uniform, hammer, wrench and printer, standard
  player, synced loadout, custom faces allowed).

## Not yet

- Team captains, uniforms on player types with their own model,
  friendly-fire penalties, team swaps, bots, `.pathcam` saving, CTF's
  remaining parts.

## Tests

- `crates/addon-import/tests/slayer.rs`
  `teams_dress_their_members_and_give_them_their_kit`.
- `crates/addon-import/tests/ports.rs` `slayer_ports_apply_with_their_rules`.
- `crates/package/src/setting.rs`
  `item_and_player_type_settings_hold_a_content_id_or_none`.
- `crates/package-runtime/src/ops.rs`
  `uniform_kit_and_respawn_ops_stay_in_their_limits`.
- `crates/addon-import/src/ports.rs` `fill_lowers_names_for_ids`.
