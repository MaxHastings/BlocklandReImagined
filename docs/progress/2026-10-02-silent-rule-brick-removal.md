# Rule brick removal is silent

Max (playtest, 2026-10-02): the Trench shovel worked, but every split and
merge of dirt played the hammer's break debris and sound.

Cause: the `remove_brick` rule op went through `kill_one_brick`, which
emits a `BrickKill` cue (debris and break sound). Torque's
`%brick.delete()`, which `remove_brick` stands for
(docs/modding/torque-equivalents.md), removes a brick with no effect.
Trench Digging's TrenchDigging.cs splits, takes and merges dirt with
`delete()`, so nothing should break.

Fix (generic, no Trench case): `Session::delete_one_brick` removes a brick
without a cue; `kill_one_brick` is that plus the cue. `remove_brick`
deletes; a rule's `explode` blast still breaks bricks with debris.

Guard: `crates/sim/tests/hardening_packages.rs`
`a_rule_removing_a_brick_breaks_nothing` fails on 39a1cbbe ("a rule's
removal broke the brick like a hammer") and passes now.

Checked: bri-sim hardening_packages, blocks, mode_and_voxels,
brick_damage; bri-client trench_dig_flow; clippy -D warnings on bri-sim.
