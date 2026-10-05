# 2026-10-05 Host views: what the host lists never meets a script's limits

Lane `fix/slayer-bot-fill`, the open item from the bot-fill fix: the same
kind of bug as the first-names list, in everything the host lists.

## Cause

Rhai counts a value's size over everything inside it: all strings
together against 4096 bytes, all map entries together against 1024. Host
functions returned arrays of maps, so the result itself, or a script's own
array gathering them, grew with the build and the server:

- `bricks(kind)`: about 15 entries and the kind's id per brick, so about
  70 bricks of one kind passed 1024 entries. Slayer's spawn picker,
  region and capture-point lists, and CTF's flag stands threw.
- `players()` / `bots()`: about 60 entries per player, plus tools, item,
  archetype and name text. About 17 players passed the entry limit. With a
  5-tool loadout, about 13 members passed the text limit. Slayer gathers
  members (`everyone()`, `members()`) this way on every fill and sort.
- `entities()`, `objects()`, `objects_near()`, `drops()`, `minigames()`:
  the same, growing with vehicles, entities, dropped items and games.
- `bot_kinds()`: 64 kinds with ids up to 96 bytes passed the text limit.
- `avatar_choices()`: every slot's names together, growing with avatar
  packs.

## Change

These now return **views** (`crates/package-runtime/src/script/view.rs`).
A view is a custom Rhai type holding the host's map behind an `Arc`.
- **Not counted.** Rhai does not count inside custom types, so neither a
  host list nor a script's array of views meets the value limits.
- **Reads like a map:**
  - property access falls back to the indexer, so `p.name` works, and a
    missing field is `()`;
  - `"key" in p`, `keys`, `values`, `len`, `is_empty` and `remove` work;
  - `for key in p` walks the keys, and `type_of` says `"map"`;
  - `+` with a map works, and `==` is identity (or the same id and
    fields).
- **Writes.** Setting a field on a script's copy is copy-on-write.
- **Same view twice.** Each call caches views by what they are (player,
  brick, entity, drop, mini-game and id), so asking twice gives the same
  view (`player(1) == players()[0]`).
- **Memory stays bounded.** A call makes at most 16 384 views
  (`MAX_VIEWS`) and writes at most 1 MiB into them, counting a shared
  view's copy (`MAX_VIEW_WRITES`). Before views, the per-value limits were
  the bound.
- **Plain on the way out.** A view is turned back into a plain map
  (`view::plain`) wherever it leaves the script: `to_json`, so state and
  every operation, plus `show_report`, host data, minigame snapshots, bot
  objectives and each call's returned value.
- **Scripts unchanged.** No port script needed to change.

Unchanged and bounded by content-independent caps: `palette()` (numbers),
`bricks_in` (ids), and a mini-game view's `teams`, `members` and `loadout`.
`bricks(kind)` still lists at most 4096 bricks of one kind (lowest ids
first); past that a build's extra bricks of that kind are not listed.

## Evidence

All commands run with `--config .cargo-wt.toml`.

The two guard tests fail without views. I ran them with views switched
off by a temporary environment switch, since removed:
- `five_hundred_team_spawns_still_spawn_each_team_on_its_own` (slayer.rs)
  fails with `script.limit: on_pick_spawn: Length of string too large`.
- `host_lists_of_any_size_never_meet_a_scripts_limits`
  (hardening_sandbox.rs) fails with `script.limit: f: Length of string
  too large`.

The tests:
- `five_hundred_team_spawns_still_spawn_each_team_on_its_own`: a loaded
  build with 250 Team Spawns per team, then a Save & Reset to two new
  teams at Preferred Player Count 3. Each team gets 3 members (5 bots),
  every member stands on its own team's side, and there are no problems.
- `host_lists_of_any_size_never_meet_a_scripts_limits`: 64 players and 16
  bots with long names and 5 long tools, gathered into one script array,
  plus 64 long bot-kind ids. It also checks that views read like maps,
  identity, copy-on-write, and that a returned view is a plain map.
- `views_a_call_makes_and_writes_are_bounded`: 20 000 players are refused
  ("reads at most"), and 1000 copies of a view with a 4 KB field written
  into them are refused ("changes at most").
- Suites:
  - `cargo test -p bri-package-runtime`: all pass.
  - `cargo test -p bri-addon-import`: all pass (slayer 42, ports 36).
  - `cargo test -p bri-sim` on every test that runs package scripts: all
    pass.
  - `cargo test -p bri-chaos` on bot_carryable_objectives,
    bot_creator_adversarial, shark_policy, bot_objectives,
    bot_search_objectives and script_effects_fuzz: all pass.
  - clippy `-D warnings` on bri-package-runtime, bri-sim and
    bri-addon-import: clean.
