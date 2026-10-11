# 2026-10-11 Reliable operations: the pre-check covers 17 more operations

v0.2.9 (PR #42) let a rule `require(handle)` an operation so a call that
cannot do it takes no effect, but only `give_item`, `take_item` and
`remove_brick` could be checked up front. The rest relied on perform-time
rollback, which cannot undo operations of the same call that already ran.
Brief P1-16.

## What changed

`Perform::check` now exists for `plant_brick`, `place_brick`,
`drop_item`, `remove_drop`, `name_drop`, `damage`, `heal`, `teleport`,
`give_ammo`, `set_reserve`, `set_rounds`, `set_score` and `spawn_entity`,
and `remove_brick` also looks at the package's world-edit share. The table
of what each one refuses is in `docs/modding/rules.md`, "Required
operations". Each check shares its code with the operation (the
`package_check_*` helpers in `crates/sim/src/session/packages.rs`,
`check_reserve` and `check_set_rounds` in `bri-weapons`), so the two
cannot drift apart silently.

Two behaviours are new only for a `require`d operation:

- `plant_brick` that does not fit used to be skipped without a report (the
  v20 `plant()` contract). Required, it is now refused with "No room", so a
  shop does not charge for a brick that was never placed. Unrequired it is
  unchanged.
- `damage` to an entity that is gone used to do nothing; required, it is
  refused.

Not checked, and still caught only as the operation is carried out: room for
a spawned entity's body, vehicle spawns, mini-game rule changes, bot
operations, copy/paste operations, `place_voxel`, and the effects/chat
operations (which cannot meaningfully fail ahead of time). Two required
operations in one call that compete for the same last slot are not told
apart; the second still rolls the state back at perform time.

## Evidence

`crates/sim/tests/required_operations.rs` runs each case twice from one
script: `pay` (5 bits, `require(op)`, count a sale) and `free` (the same
without `require`). For every impossible request the paid call is refused
with `op.required` and the stated reason, the bits and the sale count do not
move; the free call is unchanged (`op.failed`, or silently skipped). The
agreeing half runs ten possible operations required and asserts none of
them fails afterwards, which is how two wrong checks (`place_brick` grid
snapping, `plant_brick` trust) were found and fixed while writing it.
`Allowance::peek` (a refill-free read of a share) has a unit test that it
agrees with `available`.

## Next

Vehicle spawns and the mini-game operations are the next to cover. A
required operation that depends on an earlier operation of the same call
(spawn then label) still needs a design: the check sees the world before
the call.
