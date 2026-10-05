# 2026-10-05 Slayer bot fill: failures visible, script data sized by the package

Lane `fix/slayer-bot-fill`. Max hosted with the Blockhead Bot Add-On, set
Slayer to Team Deathmatch with Preferred Player Count on the teams, pressed
Save & Reset, and got no bots (v0.2.3 and the v0.2.4 preview from
c5c7dfad).

## Cause

A Rhai script value is held to 4096 bytes of text **all together**: Rhai
adds up every string inside an array or map (`calc_data_sizes`), and checks
a native function's result and any `&mut` method target (`keys.contains`,
`changes.is_empty()`). Values the host handed Slayer grew with the content:

- `data_lines("first_names")`: Max's real list is 5161 names, 36 KB. Every
  bot fill threw, so the whole `on_minigame` call (auto-sort, round start,
  fill) was thrown away. Max's PC session found the exact line; its fix
  (line-at-a-time data reads, a20f1e5a) is merged in here, and Slayer's
  `bot_name` keeps its line's first word cut to `bot_name_limit()`.
- A `settings` event's `keys` and `changes`: arrays of every changed
  `namespace:key` of every running Add-On. A favourite or a Save & Reset
  touching ~100 settings is over 4096 bytes, so `is_key` threw.
- `bot_kinds()`: each kind carried its whole first-name list (up to 256),
  so a few bot Add-Ons were over the limit; Slayer looked its kind up
  through it on every fill.

The failures were silent: `run_package` noted them only in memory.

## Changes

- Settings events: `keys` is a map of changed keys to `true`, `changes` a
  map of key to the teams changed (`()` for the game's own), each holding
  only the settings of the hearing package and the Add-Ons it depends on
  (`settings_payload`, game_hooks.rs). A map's keys are not counted as
  text, and the set is bounded by the package's own manifest
  (`MAX_SETTINGS` 128 per package), not by how many Add-Ons run.
- `for key in map` walks a map's keys one at a time (a type iterator), so
  scripts never need `keys()` to gather them into one value.
- `bot_kind(id)` returns one kind; `bot_kinds()` entries count their first
  names (`first_names` is a number) and `bot_first_name(kind, i)` reads
  one.
- Slayer and CTF use `key in keys` and walk `changes` by key.
- Every package hook failure (script throw, refused or failed op) is told:
  a console warning, a chat line to the admins (each package and code at
  most once a minute), and a Script problem in the Add-On health report
  (logs/add-on-health.json and the Add-Ons screen), through the host's
  `package_problems`.
- A script error names the functions it came out of: `on_minigame →
  mg_settings → is_key: ...`. Rhai passes its own limit errors (data too
  large, budget, stack) straight out of every function and drops their
  line, so those name only the hook, with a hint about the text limit.
  Naming their line needs Rhai's `debugging` feature (a call stack); not
  done.
- Earlier on this branch: the double fill when the mode and the counts
  change together; a bot that cannot join is reported and the rest of the
  fill joins; bot names cut to `bot_name_limit()`.

## Decision: the 4096 cap stays

The cap is the sandbox's memory bound: the hardening tests rely on it
(60 000 strings of 4 KB in one array refused; nested containers counted as
one value). The fix is that host data no longer grows with the content or
the Add-On set, not a bigger cap.

Still open, same class: `bricks(kind)` lists up to 4096 bricks as maps
with their kind, name, item and UI name strings, so around 60-100 bricks
of one kind already passes 4096 bytes. Slayer's spawn picking lists plain
spawns (`bricks(plain_spawn())`). Max's 24 fit; a build with ~100 spawns
of one kind would throw there.

## Evidence

- `far_more_add_ons_than_max_leave_slayers_save_and_reset_working`
  (slayer.rs): an Add-On with 128 settings with 80-byte keys, all changed
  in the Save & Reset, and 64 bot kinds with 256 first names each. On the
  old payload shapes it fails with `script.limit: on_minigame: Length of
  string too large` (twice); now it passes.
- `new_teams_with_their_counts_in_one_save_and_reset_fill_at_once`: Max's
  minimal repro (four new teams in colours 1-4 at PPC 1, one Save & Reset)
  gives 3 bots, no problems.
- `a_big_builds_save_and_reset_fills_the_teams_and_starts_the_round`: the
  same in a loaded build of 52 000 bricks; the round starts at 0.
- `four_one_bot_teams_make_a_bots_only_deathmatch`: one bot a team,
  enemies, a kill scores.
- `a_map_is_walked_a_key_at_a_time_past_the_text_limit` and
  `a_script_error_names_the_functions_it_came_out_of` (hardening_sandbox).
- Commands (all with `--config .cargo-wt.toml`): `cargo test -p
  bri-addon-import --test slayer --test ports` (35 and 36 passed), `-p
  bri-package-runtime` (all passed), `-p bri-sim --test script_api --test
  hardening_packages --test server_settings --test packages`, `-p
  bri-client --lib add_on_health`, clippy `-D warnings` on the five
  crates.

## Next

- Deathmatch bots: a game-level free-for-all bot count for modes without
  teams.
