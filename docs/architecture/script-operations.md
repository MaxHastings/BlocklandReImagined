# The script-operation boundary

Status: on main. Code: `crates/package-runtime/src/ops/` (what a script may
ask for) and `crates/sim/src/session/packages/perform/` (what the engine
does for it). The capability gate and the seams are described in
[package-runtime.md](package-runtime.md); this note is the shape of the
boundary itself.

## The two halves

A script call never touches the game. It returns a list of typed
operations (`bri_package_runtime::Op`), and the session carries them out
after the call succeeds.

| Step | Where | What it checks or does |
|---|---|---|
| Declare | `ops/<capability>.rs`, one struct per operation, implementing `ScriptOp` | Its capability, its script name and its shape limits (`bounded`). |
| List | `ops/list.rs`, one `Name = module,` line | The one list. `Op`, `capability()`, `name()` and the limits check are generated from it. |
| Authorize | `ops::authorize` | Limits, then the capability the package declares, then `op.foreign_entity`. Every operation of a call passes before any is performed. |
| Own | `Perform::entity` | The entity an operation acts on must be the calling package's (`op.not_owner`). |
| Perform | `Perform::perform`, in `perform/<capability>.rs` | What the engine does, with the package, the calling player and the tick (`OpCall`). |

`perform::perform` and `perform::entity` are generated from the same
`for_each_op!` list, so an operation added to `list.rs` without a
`Perform` impl does not compile. Nothing else in the session matches on
`Op`.

## Where an impl lives

In `session/packages/perform/<capability>.rs`, named like the operation's
own module in the runtime: `ops/player.rs` there, `perform/player.rs` here.

The exception is an operation that changes state one session system keeps
private. It is performed in that system's own `perform` module instead of
widening the system's fields: `session/movables/perform.rs` performs the
physics operations that change holds, tethers and spawned vehicles. An impl
calls the system's methods; it never reaches into another system's state.

## Adding an operation

1. Add the struct and its `ScriptOp` impl to `ops/<capability>.rs` and a
   line to `ops/list.rs`.
2. Have the script push it (`script.rs`).
3. Add its `Perform` impl to `perform/<capability>.rs`. Override `entity`
   if it acts on a package entity.

The compiler names any step 3 you forget.

## What this replaced

Until v0.1.13 the session performed operations in one 1,300-line match
(`apply_package_op`), which handed the physics and mini-game families to
two more matches ending in `unreachable!` and `bail!`, and kept the entity
ownership check as a hand-kept list of three operations in `run_package`.
The bodies moved unchanged.
