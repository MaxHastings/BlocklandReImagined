# Normalize server teleport headings before weapon frames

2026-10-02. Integration coordinator requested investigation of the deterministic
combined-gate Slate failure recorded in `.bri-gate/logs/b8205f9947c2{,-retry}.log`.
All work remains in the isolated Rule Workshop tree; the bot-owned primary
checkout, package defaults and production packaging are untouched.

## Reproduction and root cause

`BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content BRI_CHAOS_SEED=9e3779b9 cargo test --locked -p bri-chaos --test session_chaos content_chaos_never_panics_fails_or_replicates_nan_map0_shard0 -- --ignored --nocapture`
reproduced exactly: Slate, 1200 ticks, 701 commands, with weapon-frame failures
at ticks **710 and 1109**. Temporary field diagnostics identified both frames as
`yaw=-4.712389, scale=1, direction length squared=1, speed=0`. No NaN, excessive
velocity or invalid aim was involved.

The third spawn-brick quarter-turn supplies `-3*pi/2`. Server teleport accepted
any finite heading and retained that value unchanged, while movement inputs and
weapon frames require yaw in `[-pi, pi]`. A later look update could conceal the
invalid intermediate state. This is a canonical relocation bug; making physics
packages opt-in would not correct the heading.

## Small fix and regression

`Player::teleport` now normalizes finite yaw with the same full-turn wrapping
already used by vehicle-seat placement, preserving facing while satisfying the
input/weapon-frame range. Teleport validation and weapon validation are not
weakened. The weapon frame's failure diagnostic retains its checks and now
names yaw/scale/direction length/speed, so another bad frame is diagnosable.

A real-session spawn test exercises all four authored spawn rotations through
plant/disconnect/resume, checks the spawn location and equivalent facing,
checks heading validity before any movement/look repair and runs the next
weapon tick. It failed on the original implementation at rotation 3 with
`yaw=-4.712389`. It now passes for both synthetic and generated original content
(**2 tests**).

The exact formerly failing chaos seed now has **zero step errors**, with the
same 1200 ticks, 701 commands, rejection counts, planted/loaded objects and
mounted/projectile exercise counts. Normalization fixes the bad intermediate
state without skipping or weakening the replay.

## Integration overlap

The only bot-owned-file overlap is the small teleport-yaw hunk in
`crates/motor/src/player.rs`; this was explicitly flagged to the coordinator.
Other code/test paths are `crates/weapons/src/runtime.rs` and
`crates/sim/tests/special_bricks.rs`. No bot mechanisms or vehicle behavior were
changed. The correction has its own commit, separate from the free-build/
deferred-budget/client-fixture fixes. The GUI-review Windows snapshot 04 is
still from 83939023; final combined v0.2.0 must include these later source fixes.

Broader simulation/weapon tests and warnings-denied clippy are recorded below
when complete. No visible window, gameplay input or human playtest was run.

## Final validation

- `cargo test --locked -p bri-sim --test special_bricks every_spawn_rotation -- --include-ignored --nocapture`: **2 passed**, synthetic and original content.
- Exact Slate seed replay command above: **1 passed**, 1200 ticks and no step
  errors; `/tmp/bri-workshop-chaos-fixed.log` holds the report.
- `cargo test --locked -p bri-sim -p bri-weapons -p bri-motor --lib --tests`:
  **717 passed, zero failed, 133 ignored**. No new ignored exception.
- `cargo clippy --locked -p bri-motor -p bri-weapons -p bri-sim -p bri-net -p bri-client --all-targets -- -D warnings` passed.
- `git diff --check` passed.

The coordinator cherry-picked source corrections e0181731 as 66da61bc while
preserving combined production packaging/profile edits, and requested this
separate heading correction for the final immutable v0.2.0 build. This branch
neither merges main nor publishes a standalone release. The broader release
gate remains the coordinator's responsibility.
