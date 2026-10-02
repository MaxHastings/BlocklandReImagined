# 2026-10-01 Script-operation and weapon-script boundaries

Max asked, beside the client `app.rs` split, to formalize the
script-operation boundary (`sim/session/packages.rs` and
`package-runtime`) and to stop compatibility special cases accumulating in
the weapons runtime's state-script callback. Both are refactors with no
behaviour change.

- **Script operations.** The 1,300-line `apply_package_op` match and the
  physics and mini-game re-dispatch matches are gone. Each operation is a
  `Perform` impl in `session/packages/perform/<capability>.rs` (physics
  ops that change holds, tethers and vehicles in `movables/perform.rs`),
  dispatched from the runtime's `for_each_op!` list, so a missing impl is a
  compile error. The entity ownership check is `Perform::entity`.
  `packages.rs` went from 4,809 to about 3,500 lines.
  Design: [docs/architecture/script-operations.md](../architecture/script-operations.md).
- **Weapon scripts.** The twenty image-name checks in
  `WeaponsWorld::callback`, and the rest in the weapons runtime (akimbo
  left image, sports-ball keys, catches and drops, the dodgeball, football
  and horse-ray projectiles), are one table, `runtime/stock.rs`. A unit
  test fails if the runtime compares a datablock name again. Design:
  [docs/architecture/weapon-scripts.md](../architecture/weapon-scripts.md).

Evidence: `cargo clippy -p bri-sim --tests` and `-p bri-weapons --tests`
clean; `cargo test -p bri-weapons` and `cargo test -p bri-sim` pass in the
cloud (content-free; the Gate runs `--include-ignored` with content).

Next: nothing open on these two boundaries.
