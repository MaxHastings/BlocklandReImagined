# 2026-10-05 Slayer: bots in a plain Deathmatch

Lane `fix/slayer-bot-fill`, Max's follow-up: easy bots for a Deathmatch
with no teams.

## What changed

Ours, not original Slayer (marked so in the setting's help and in the
port's notes). Slayer's `fill_bots` took every bot away when the mode had
no teams.

- A game-level setting `ffa_bots`, "Bots (free-for-all)" on the Bots page,
  0 to 16, default 0, shown only while the Game Mode is Deathmatch
  (`shown_when` on `mode`). Team modes keep each team's Preferred Player
  Count. All of it is in the port's `behaviour.json` and `slayer.rhai`:
  the engine has no special case.
- In a mode without teams, `fill_bots` keeps that many teamless Slayer
  bots in the game, with the same server limit, failure reporting and name
  picking as the team fill (`bot_fill_to`, shared with `bot_fill`). Bots
  with no team are everyone's enemy; they spawn through the normal picker
  (plain spawn bricks, else the map).
- Switching to a team mode takes them away and the team fill takes over.
- Without the Blockhead Bot Add-On a count above 0 goes back to 0 and the
  owner is told, as Preferred Player Count goes back to -1.
- When the server's bot limit (16) keeps bots out of either fill, whoever
  runs the game is told how many, once each time that number changes
  (`tell_bots_short`, state `bot_short`).

## Evidence

slayer.rs, `cargo test -p bri-addon-import --test slayer` (with
`--config .cargo-wt.toml`):

- `deathmatch_bots_fill_the_game_with_everyones_enemies`: none by
  default; count 4 gives 4 teamless bots; `can_damage` between two of them
  is true; a kill by one raises its score; count 0 takes them away; no
  Add-On problems.
- `deathmatch_bots_give_way_to_the_team_fill_in_team_deathmatch`: 4
  free-for-all bots, then a Save & Reset to Team Deathmatch with two new
  teams at Preferred Player Count 1: no teamless bot is left, one bot joins
  the team the host is not on.
- `bots_the_server_has_no_room_for_are_told_to_the_owner`: two teams at 20
  get 16 bots and the owner hears "could not join" once.

## Next

Max's playtest of the Bots page in Deathmatch.
