# 2026-10-11 Environment favorites and live apply

Max asked (item 3 of his 2026-10-10 list) to save favourite environments
customised through the Admin Menu's Environment window, consistent with how
the game already keeps favourites.

## What changed

The Environment window (`crates/ui/src/screens/environment.rs`) has a
Favorites row above the status line: a slot list (`Slot 1` to `Slot 10`,
`(empty)` where nothing is saved), Load and Store. It is the same row the
Add-On Settings window uses for its favourites, and the slots live in the
player's settings file beside the others: `environment_favorites` in
`bri_ui::api::Settings`, keyed by slot, holding a
`bri_content::atmosphere::Settings` (the host's values over the map's own
look). Missing fields default, so older settings files load unchanged.

- Store keeps what the rows show (applied or not) in the picked slot and
  saves the settings through the host, as the mini-game and Add-On
  favourites do.
- Load fills the rows from the slot. Nothing reaches the server until Apply,
  and the host still checks the rank and every value.
- A day cycle is saved by its length and time of day with `anchor_tick` 0,
  because that tick belongs to the server it was set on
  (`EnvironmentModel::favorite`). Loading anchors the cycle at the current
  server tick (`EnvironmentModel::load_favorite`), so the look starts at
  the saved time of day.
- A slot whose values fail `Settings::validate` (a hand-edited file) is
  refused with a status line instead of filling the rows.

The window grew from 440 to 470 tall to make room; the 640x480 layout still
holds it. The Simple and Advanced pages and the colour picker are untouched,
as is the Enhanced Sky setting (still hidden). No renderer or shader files
changed.

## Live apply

Max's follow-up: changing a setting should show at once rather than needing
Apply after every nudge. The window now sends a change to the host by
itself once the rows have been still for 250 ms (`LIVE_DELAY_MS`): a
slider's steps become one request with the last value, a change made while
a request is in flight goes once the host answers, and a rejected change is
not sent again until the rows change (`sent_revision`). The host still
checks the rank and every value, and every player sees each applied step,
as they would after Apply. Apply stays as "send now" (a retry after a
rejection, or for anyone who prefers it); Reset and Load go live the same
way. The Simple tab's hint says everyone sees the look as it changes.

## Decisions

- Slots in the settings file, not named files: brick, avatar, mini-game and
  Add-On favourites are all slot maps in `settings.json`, and v21's
  `EnvironmentGui` had no favourites of its own to copy.
- Favourites hold the host's settings only (what Apply sends), not the map's
  authored values, so a favourite saved on one map gives another map the
  same overrides over its own look.

## Evidence

`cargo test -p bri-ui --test admin_screens --lib environment`: the new
`environment_favorites_store_and_load_the_rows_without_applying` (window:
empty slot, Store, list label, SaveSettings action, Load after Reset,
anchor tick, Apply, JSON round trip) and
`a_favorite_keeps_the_time_of_day_and_starts_its_cycle_from_now` (model)
pass with the existing environment tests, as does
`environment_changes_go_to_the_host_live_once_they_settle` (no request on
opening, one request after three slider steps, a change during a pending
request waits, a rejection is not retried, the next change goes). Full `cargo test -p bri-ui` and
`clippy -D warnings` results are in the PR.

## Not done

No interactive check: Max playtests. The favourites are per player
(client settings), not per server, which matches the other favourites.
