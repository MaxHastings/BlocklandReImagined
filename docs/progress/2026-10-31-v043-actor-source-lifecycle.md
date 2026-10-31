# 2026-10-31 Destroyed actor sources suspend production

The Session regression reproduces a vehicle being created from a fake-dead
spawn brick. Production now checks `damage_hidden`, independently of authored
visibility/collision flags. Reconciliation keeps an existing same-kind actor;
explicit brick respawn events do not delete/replace that actor while its source
is destroyed. Bot creation, event respawn and ordinary dead-bot respawn use the
same source boundary. Rules-created bots without a brick retain their lifecycle.
Restoration reuses an existing actor and lets an overdue dead bot return.

The existing native vehicle scheduler owns due-times. Its read-only
`VehiclesWorld::spawn_definition(SpawnId)` reports a live or pending registration,
so a dirty source need not cancel/recreate a pending vehicle just because the
body is gone. When a deadline arrives while the source is destroyed, native due
processing finishes but Session refuses production; restoring the source later
reconciles the overdue spawn normally. Restoring before the deadline preserves
the existing native pending clock. No second scheduler, fabricated actor,
periodic reset or wire/save field is introduced.

Recovered v20 `allGameScripts-Vanilla.cs::fxDTSBrick::spawnVehicle` at 16844
checks source type/vehicle datablock but has no explicit fake-dead check, unlike
its static-item spawn script. That script alone cannot establish the native
engine's complete destruction behavior. This consistent production policy follows
Maxwell's approved destruction expectation, rather than claiming an unverified
vanilla match. Existing actors continue through temporary source destruction;
ordinary removal/reconfiguration retains its existing owner.

Failures and corrections are retained in private `destroyed-actor-*` diagnostics:

- First test compile named the wrong DamageKind module; corrected to the Session
  combat type. The next run reaches the real destroyed-source spawn failure.
- Initial guards pass that assertion. The first restoration fixture used a
  three-plate ordinary brick, whose top intersects the bot's established spawn
  placement. Diagnostic output confirms the restored source and missing bot.
  Use the actual supported one-plate vehicle-spawn definition, with authored
  invisible/noncolliding flags as a positive control. Do not invent a bot offset
  to compensate for an unrelated fixture. That lifecycle case now passes,
  including live identity, event refusal and natural bot death/restoration.
- The first pending-clock fixture tried to replace vehicle definitions after a
  player joined; setup correctly refuses it. A shared fixture constructor now
  installs an optional pack before joining. No setup guard is weakened.

Completed focused checks:

- `cargo test --locked --profile playtest -p bri-sim --lib
  session::destruction::tests`: all 24 pass. The pending-clock case exercises
  actual vehicle damage, removal, native checkpoint due-times, dirty-source
  suspension and restoration on both sides of the deadline.
- `cargo test --locked --profile playtest -p bri-sim --test vehicles`: 36 pass;
  34 established converted-content cases remain ignored by that default harness.
  No ignore or weakened assertion is introduced here.
- Existing `brick_bots_inherit_the_spawn_frame` unit case: 1 pass.
  `cargo test --locked --profile playtest -p bri-chaos --test bot_builder`:
  all 7 pass, including actual builder power/configure/release and infection.
- `cargo clippy --locked --profile playtest -p bri-vehicles -p bri-sim
  --all-targets -- -D warnings`: passes. Formatting and `git diff --check` pass.

These checks challenge source production and ordinary consumers; they are not
interactive acceptance, converted-content comparison or a full release gate.
No failing test is accepted by an unchanged retry.

Brick looping audio, final effect pixels, combined acceptance and final release
checks remain separate outstanding work; this is not a release-complete claim.
