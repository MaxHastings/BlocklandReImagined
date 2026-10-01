# 2026-10-01 Slayer's End of Round Report, with Capture the Flag's columns

Branch `claude/project-thread-t8k5dx` (Slayer/CTF).

## Engine seams (generic, capability-gated)

- `bri_package_runtime::report`: a score table (`Report`: title, banner,
  columns by key, sections of named rows with an optional palette colour
  and cells by column key) and `ColumnChange`, with limits (11 columns
  after the name, 4 sections, 256 rows, 64 characters a text).
- `show_report(p, report)` / `hide_report(p)` (`chat`, `Op::ShowReport`):
  the host sends the report to one player in `Notice::Report`; the client
  opens a Report window (`ScreenId::Report`, native: Slayer's
  `Slayer_CtrDisplay` had no layout in the copy) laid out as Slayer's ML
  text was: banner in Arial Bold 24, header in Arial Bold 20, names in
  Arial Bold 15 in their team's paint, tab stops per column. Escape, Enter
  and Close close it.
- `report_column(game, key, title, cells)` / `report_column(game, key, ())`
  (`minigame`, `Op::ReportColumn`): another Add-On retitles and refills a
  column by row key, adds one or takes one out, for that game. The host
  keeps a game's changes (at most 8, forgotten with the game) and puts them
  in when it sends pending reports at the end of the tick's package work,
  so it does not matter whose round-end hook runs first. This is the
  generic form of Slayer's `scoreListInit` / `scoreListAdd` callbacks.
- Protocol file `score-reports.md`.

## Port

- Slayer: the three EoRR preferences (Display End of Round Report, Display
  Team Scores, Display Victory/Defeat, defaults from the copy) are game
  settings. At a round's end with the report on, every member gets the
  report: VICTORY or DEFEAT for their side, a Teams section (with teams
  and Display Team Scores) of team score, kills, deaths and rounds won,
  sorted by score, then the players by score, a team player's Rounds Won
  blank. Titles and words are pinned from the copy's `sendScoreListAll`
  and `endRound`. A reset or leaving the game closes it.
- Capture the Flag: counts flag pick-ups (only a flag taken from its stand)
  and returns per player and team, clears them at a reset, and at a round's
  end shows them in place of Kills and Deaths under the copy's titles,
  blank for none, as `scoreListInit` / `scoreListAdd`. Changing to another
  mode takes its columns out.

## Player option

- Slayer's client preference "Disable End of Round Report" is Options >
  Gui Options > "Hide end of round reports" (`$pref::HUD::HideReports`,
  off by default): the client keeps a report it is sent but never opens
  its window. It is the player's own choice, so nothing goes over the
  wire. Tests `a_report_opens_its_window_unless_the_player_hides_reports`
  and `gui_options_end_with_hide_reports_and_check_for_new_versions_toggles`.

## Evidence

- Tests: `the_end_of_round_report_shows_teams_and_players_with_flag_columns`
  (VICTORY / DEFEAT, columns, sorting, CTF tallies, blank cells, closing at
  the reset, the two display options, the report off),
  `a_column_change_retitles_refills_adds_and_removes_by_key`,
  `a_report_past_its_limits_is_refused`,
  `a_report_lays_out_its_banner_header_and_coloured_rows_as_plain_text`,
  `a_score_report_shows_in_column_order_as_plain_text`.
