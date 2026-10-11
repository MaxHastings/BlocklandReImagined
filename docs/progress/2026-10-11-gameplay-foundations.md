# 2026-10-11 Gameplay foundations: confirmed pickups, item restore modes, required operations

Branch `claude/project-thread-9yxylk`, off main `b650033` (v0.2.8). Max's ask
(2026-10-10, item 4, and P1-14 to P1-16 of the v0.2.9 brief): make successful
gameplay outcomes observable to Wrench Events and Add-Ons, give item
spawners restoration beyond a fixed timer, and stop Add-On operations from
committing dependent state when a required action fails. Reuse what exists;
no new framework.

## What was there

- A spawn brick's item (`ItemSpawn`) came back only after `respawn_ms`
  (`item_spawners::picked_up`). `MiniGameSO::Reset` already revealed every
  item of the game's builders (`Effect::ResetBricks { reveal_items }`), and
  the `setItem` output restocked one.
- Nothing fired on the brick when its item was taken. The only hook was the
  Add-On `on_pickup`, which runs before the pickup and decides it: the
  attempt, not the result. A touch with full tools left the item in place
  and started no timer, so the engine already knew the difference.
- A script call returns operations the session performs after the call
  (`docs/architecture/script-operations.md`). `run_package` committed the
  call's state first and performed the operations after; a failing one was
  an `op.failed` warning. So `add_player(p, "bits", -5); give_item(p, gun)`
  charged a player with full tools and gave nothing: Max's example exactly.
  `take_item` of an item the player lacks did nothing and reported nothing.

## What changed

**Lifecycle (`crates/world`, `crates/sim/src/item_spawners.rs`,
`session/items.rs`).** `ItemSpawn::restore: ItemRestore { Timer, Reset }`,
`#[serde(default)]` so every existing save reads as `Timer` (v20's
behaviour; `BUILD_SCHEMA` unchanged, the field is left out of saves when it
is `Timer`). `Reset` sets `available_at` to `item_spawners::NEVER` on pickup:
the item stays a ghost until the mini-game resets (the existing reveal) or
an event restores it. The wrench dialog gets a "Respawn only on mini-game
reset" box under the respawn time (`WrenchField::ItemRestore`, added to the
recovered layout like the Team row), the fill wrench carries it
(`WrenchFill::item_restore`), and `crates/net/protocol-changes/item-restore-mode.md`
records the wire change.

**Events (`crates/events/src/rules.rs`, `session/rules.rs`).** The core
catalog gains the input `onItemPickup` (targets Self, Player, Client,
MiniGame; a v20 catalog that declares one keeps its own) and the brick
outputs `restoreItem` and `hideItem` (`RuleOp::RestoreItem`, `HideItem`).
`step_items` fires `onItemPickup` only after `picked_up` succeeded, so a
touch that found no room, hands busy or the item gone fires nothing, and two
players touching one item in one tick fire it once (the second touch finds
`available_at` in the future). A coin brick is now
`onItemPickup → Client → incScore 1`, with no Add-On.

**Add-On result hook (`package-runtime/src/content.rs`,
`session/packages/item_hooks.rs`).** `on_collected(player, item, info)` runs
once a player has really taken an item of the package: from a spawn brick or
a drop, taken or used up. `on_pickup` stays the attempt.

**Required operations (`package-runtime/src/script.rs`,
`session/packages.rs`, `perform/mod.rs`).** Every operation function now
returns a handle (an integer; scripts that ignored `()` are unaffected) and
`require(handle)` marks the call as depending on it. `run_package` checks
each required operation (`Perform::check`, implemented for `give_item`,
`take_item` and `remove_brick`; the default says yes) before the call's
state is kept or anything performed; a refusal drops the whole call, reports
`op.required` and is the command's reply, so the player hears "P's tools are
full". A required operation that still fails while performed rolls the
package's state back to what it was before the call, unless another write
replaced it meanwhile (an output the operations fired ran the package
again), which the report says. Unrequired operations behave as before. The
check for `take_item` is strict: nothing to take is a failure, where the
unrequired call still does nothing quietly.

**Hooks that edit the world mid-loop (`session/items.rs`, after the
integration thread's review).** The pickup loop re-finds its spawner after
every hook: `on_pickup` may remove the brick it was asked about (the
soccer-crash PR's guard, ported here so this branch merges on its own), and
`onItemPickup` and `on_collected` run after the take, so `reconcile_items`
runs again after them. A spawner a result hook removed offers nothing to a
later contact in the same tick, and no respawn is booked for a gone brick.
`crates/sim/tests/item_hook_removal.rs` covers both, in both restore modes.

## Decisions

- No transactions. "Check what you can, then keep the call only if every
  required step happened" is the whole mechanism; operations already done
  before a late failure stay done, documented, with "put required ones
  first" as the rule. Making every call all-or-nothing by default was
  rejected: `on_tick` telling a player who just left would then lose the
  tick's state.
- One restore mode beyond the timer, not a menu of them. Reset and manual
  restoration are one mode because a reset already restores everything a
  game's builders own; `restoreItem`/`hideItem` give the manual control, and
  they work on timer bricks too.
- The pickup event is the brick's, not a new "collected" object type. The
  brick already has identity, owner, trust and a save format.

## Evidence

Headless, in this container (no v20 content):

- `cargo test -p bri-sim --test item_lifecycle`: a reset brick's item stays
  gone for 700 ticks while the timer brick's comes back, the view a late
  joiner receives shows it gone, the reset brings it back and one member
  takes it once; `onItemPickup` runs once when two players touch one item
  the same tick, nothing on an empty brick, `restoreItem` and `hideItem`
  work, a brick that stops spawning an item takes its item with it; an old
  save without `restore` reads as `Timer` and `reset` round-trips.
- `cargo test -p bri-sim --test item_hooks`: `on_collected` once for an
  ammo box used up and a gun taken, not for a mine left or a touch with full
  tools; `cmd_buy` with full tools keeps 10 bits, no sale, the refusal is
  the command's reply; with room, 5 bits and the gun; `require(99)` is a
  script error.
- `cargo test -p bri-ui --lib item_restore_box`: the wrench box shows and
  sends the mode.
- `cargo test` over bri-events, bri-world, bri-ui, bri-package-runtime,
  bri-bls, bri-minigames and the sim suites items, tools,
  hardening_session, v20_events, script_api, packages, samples, showcase,
  session, package_brick_outputs and package_brick_inputs: all pass.
- `cargo test -p bri-client --lib`: 491 pass; the 6 that fail here
  (`a_save_picture_is_drawn...`, `installed_effects_replace_the_atlas...`,
  `render_scale_draws...`, `original_hud_icons_upload...`, `a_restructured_body...`,
  `a_game_entered_before_the_world_pipelines_compile...`) all open
  `bri_ui::gpu::Headless` and this container has no graphics adapter; they
  touch nothing this branch changes.
- `cargo check --workspace --tests` and `cargo clippy --tests -D warnings`
  over bri-sim, bri-package-runtime, bri-events, bri-world, bri-ui,
  bri-client, bri-bls and bri-chaos: clean.

Not run here: the full gate (`tools/gate.py`) and `bri-client --check` need
the content packs on Max's PC.

## Left unchanged, and limits

- `Perform::check` exists for three operations. Others (`damage`,
  `plant_brick`, `spawn_entity`, scores) rely on the perform-time rollback
  of state; adding a check is a few lines each when a real Add-On needs it.
- A required operation that fails after earlier operations of the same call
  were performed cannot undo them (a removed brick stays removed).
- `onItemPickup` is for spawn-brick items. Dropped items have no brick;
  Add-Ons hear those through `on_collected`.
- The `Reset` mode uses `available_at = u64::MAX`; clients already draw any
  future `available_at` as a ghost, so no client change.
- P1-18 (scripted two-client journeys with fault injection) is not in this
  branch: it is a test-infrastructure lane of its own. The pieces that
  belong to this lane (reset, pickup race, late joiner's view) are the sim
  tests above.

## For Max to playtest

1. Wrench a brick, give it an item, tick "Respawn only on mini-game reset",
   pick it up: it should stay a ghost until you reset the mini-game.
2. Wrench Events on that brick: `onItemPickup → Client → incScore 1`; pick
   it up with two players at once and check one point is scored.
3. A second brick with `onActivate → [the first brick, by name] → restoreItem`,
   and another with `hideItem`: clicking them brings the item back and
   takes it away.
