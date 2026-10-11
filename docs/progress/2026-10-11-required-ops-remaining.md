# 2026-10-11: `require` pre-checks the remaining operations that change things

This follows `2026-10-11-reliable-ops-precheck.md` (PR #53) and covers the
operations that note listed as unchecked: mini-game rules, bots, vehicles,
copy work and `place_voxel`.

## What changed

- **Mini-game rules** (`perform/minigame.rs`). The ten operations that only
  change the mini-games now share one `Rule::rule` function each, which
  takes the mini-games and the player list. `require` runs it on a clone of
  `MinigamesWorld` (now `#[derive(Clone)]`), and `perform` runs the same
  function for real and then applies the effects. The refusal a script
  hears is therefore exactly the one the operation would give. The copy is
  made only for a required operation. `set_respawn_time` checks that the
  player exists.
- **Bots** (`bots/lifecycle.rs`). `check_rules_bot`, `check_rules_bot_tool`
  and `check_rest_rules_bot` were split out of the operations and are
  called by both the check and the operation. `add_bot` checks the bot
  limit, room on the server, the kind, the game and the team. It does not
  foresee being given a body or being placed in the game.
- **Vehicles** (`movables/perform.rs`). `check_spawn_vehicle` covers the
  package's kind, its 64-vehicle limit and the server's vehicle limits.
  `check_remove_vehicle` covers ownership. Both are shared with the
  operations. The spawn still only learns whether the spot is clear when it
  spawns. The player is told about a server limit exactly as before.
- **Copy work** (`perform/build.rs`, `perform/world_edit.rs`). The 24 copy,
  ghost, fill and super-cut operations check the one thing they refuse up
  front: that the asking player's command started them (an administrator
  may also cancel or drop anyone's). `load_copy` also checks its tool. What
  the copy job finds later is reported as it happens, as before.
- **`place_voxel`** (`packages.rs`). `package_check_place_voxel` checks for
  a generated world, the material, the bounds, the brick limit, the edit
  share and free space.
- `docs/modding/rules.md`: the table lists each of these and what refuses it.

## Tests

`crates/sim/tests/required_operations.rs` adds 21 impossible cases. Each one
is refused with the operation's own reason, charges nobody and is reported
once as `op.required`; without `require` the same call runs and reports as
before. It also adds 9 possible cases in sequence: create a shared game,
colour it, reset it, put a player in, hold their respawn, set their respawn
time, set their team, plant a (missing) copy, and end the game. All are kept
and none fails afterwards. While writing the test, `create_minigame` with
the default settings failed as `UnknownContent` (this fixture lacks v20's
default loadout items) and was refused by `require`, which is that
agreement working.

Checked on Linux: `cargo test -p bri-sim` and clippy `-D warnings` for
bri-sim and bri-minigames. Windows is left to CI.
