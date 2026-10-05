# Slayer team bots: the first-names list stopped every mini-game event

Maxwell's playtest (v0.2.3 and the v0.2.4 preview): Team Deathmatch, four
teams with Preferred Player Count 1, Save & Reset. No bots appeared, the host
was left on no team, and nothing was logged.

## Cause

`bot_name` read Slayer's whole first-names list with `data_lines`. The real
`first-names.txt` is 5,161 names (36 KB); Rhai counts every string in an
array against `max_string_size` (4,096), so the call failed with
`script.limit: Length of string too large`. It runs inside `fill_bots`, which
every `teams`, `settings` and `reset` event reaches, so each event stopped
there: no bots, and the steps after it (sorting the host) never ran. The test
fixture's list has five names, so `tests/slayer.rs` never saw it.

Found by hosting the installed v0.2.4-preview content headlessly with
Maxwell's enabled Add-Ons (`add-on-choices.json`) and his steps, then marking
`on_minigame` in a scratch copy of the rules; the last mark was in
`fill_bots` before `bot_name`.

## Fix

- Rules read a data file a line at a time: `data_line_count(id)` and
  `data_line(id, i)` replace `data_lines(id)` (only Slayer used it).
- Slayer's `bot_name` picks its line by index.
- `four_teams_of_one_fill_with_bots_named_from_a_long_list` imports Slayer
  with a 5,000-line list. Without the fix it fails with the playtest's
  `Length of string too large`; with it every team but the host's has one
  `Bot Name…`.

Evidence: `cargo test -p bri-addon-import --test slayer` (new test plus
`a_teams_preferred_player_count_fills_it_with_bots`); the headless real-content
run with the fixed rules gave one Blockhead bot on each of Teams 1, 3 and 4
and the host on Team 2.

## Open

- Script errors in mini-game events reach only `package_diagnostics`; the
  player sees nothing. They should reach the log and Add-On problems.
- A new team's fill adds a bot to every team before the host is sorted, then
  sends the extra one away; a stale `joined` event for it then reports
  `Bot N is not one gamemode_slayer-rules added` and `no player N`. Harmless,
  but noisy.
- A plain Deathmatch of bots fighting each other is not possible: brick bots
  side with their builder, and Slayer's Preferred Player Count is team-only.
