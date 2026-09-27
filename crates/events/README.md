# Native vanilla brick events

`bri-events` is a Rust 1.93 standalone workspace. It owns native event programs, typed validation, scheduling and event-local state. It does not own physics, players, vehicles, networking or rendering. There is no Torque reader or VM in its dependency graph. Offline tools are separate Python programs in `crates/events-import`.

Current private catalog: `content/events-pack-002/catalog.json`, schema 1, **16 inputs and 65 outputs**. Every registered output compiles to a native internal operation or typed host intent. This is dispatch coverage; it is not a claim that the shared game has bound every intent. See `docs/research/events-runtime/coverage.csv` for all 65 routes and host obligations.

## Host setup and execution

```rust,ignore
use bri_events::*;

let catalog = Catalog::load(config.events_catalog)?;
let bindings = Bindings {
    palette_len: native_palette.len(),
    datablocks: native_event_choices_by_class, // BTreeMap<String, BTreeSet<String>>
};
let mut events = EventWorld::new(catalog, bindings.clone(), Limits::default())?;
events.install_brick(BrickProgram {
    id: Id { index: brick_id, generation: brick_generation },
    owner_scope: stable_brick_group_id,
    name: brick.name.clone(),
    rows: validated_editor_rows,
    print_count: initial_original_digit_or_zero,
    implicit_cancel_relays: false, // self onRelay -> fireRelay is detected automatically
})?;

// Trigger construction is an authoritative host operation, never a decoded client claim.
let mut input = Trigger::new(brick_id_with_generation, "onActivate", quota_origin);
input.client = Some(current_client_entity);
input.targets.insert(Slot::Player, current_player_entity);
input.targets.insert(Slot::Client, current_client_entity);
// Add MiniGame only under the source-backed scope rule; see semantics::minigame_target.
events.trigger(input)?;

// Call at an authoritative event phase, using a monotonic integer clock.
// Existing world ticks: migration::world_tick_to_us(tick) at 120 Hz.
let report = events.advance(now_us, &mut host_adapter)?;
// Replicate/save changed enabled flags by reading these current programs.
for brick in &report.changed_programs {
    host_store.update_event_program(events.program(*brick).unwrap());
}
```

`Host` provides four synchronous methods:

- `alive(Entity)` checks the exact identity and generation. Player, Client, Projectile, MiniGame and Brick IDs are separate classes. Bot/Driver are Player-class targets, not aliases for arbitrary vehicles.
- `permitted(&Trigger, Entity, output)` checks authority using the installed source program, captured client/session and current target/minigame state. Activating someone else's authored event is not equivalent to editing their brick: do not blindly replace this with activator edit-trust. Invalid clients must never supply target IDs or quota origins.
- `relay_neighbors(brick, direction, limit)` supplies an indexed, bounded spatial query. Use `semantics::relay_box` for the original 0.1-unit face slab. Runtime sorts/deduplicates results, excludes the source and enforces the source brick-group scope.
- `apply(&Dispatch)` returns `Applied`, `Deferred(reason)` or `Rejected(reason)`. `Applied` mutations must be visible before the next row. `Deferred` must make **no mutation**: the exact job remains queued and that origin pauses for the rest of the phase. Physics-dependent operations can use this boundary deliberately. Rejection is counted and recorded; it is not reported as applied.

`Dispatch` includes source/target, row, canonical input/output names, quota origin, captured client, requested due time and actual execution time. Client provenance is separate from editor target slots: `onRelay` only exposes Self but retains the invoking client for attribution. Never synthesize Player/Client targets from untrusted object numbers. Native content IDs in `Bindings` must come from the host's real catalogs, with original class filters and aliases resolved before admission; missing assets are errors, never substitutes.

Call `trigger` and then `advance` inside the weapons contact hook before default impact for zero-delay projectile Bounce/Redirect/Delete/Explode. Do not dispatch the same observational contact a second time. Reentrant relay/print chains are iterative queue work, not recursive host callbacks. Other subsystems can submit further inputs after consuming a phase and run another bounded phase at the **same `now_us`**.

## Ordering, limits and diagnostics

Within an origin, jobs order by requested due time then monotonic admission sequence. Matching rows and named targets are captured at activation. Named targets use a group/name index and deterministic ascending entity order. A relay appends its children behind already-scheduled siblings, preserving breadth-first authored order. Enable/toggle affects subsequent activations, not captured jobs. A finite zero-delay chain can complete in one phase when budgets permit; no 33 ms hop is inserted. Positive author delays remain positive. At a 120 Hz host cadence, deadlines are observed at the next available event phase, at most one host tick later absent overload.

Ready origins receive deterministic round-robin turns. Separate budgets bound dispatched jobs and newly expanded relay/print jobs; a single relay cannot hide thousands of allocations behind a one-job charge. Defaults: 4,096 rows/brick, 65,536 registered bricks, 131,072 pending jobs, 4,096 named targets, 1,024 origins, 32,768 dispatch attempts/phase, 8,192/origin, 8,192 expansions/phase and 4,096 expansions/origin. Root should choose lower phase quotas from the measured workload when sharing a tick with physics/AI/networking.

An activation larger than its configured expansion budget stays explicitly deferred until the host changes its configuration or cancels it; it is not partially executed or silently dropped. Initial `trigger` admission is atomic: on error, no cancellation prepass or tail of the new activation is committed. The host must retain/retry/report the input rather than ignore the error. Initial external activation preparation is bounded but synchronous; pace external trigger bursts as well as execution. A 128 MiB encoded-state budget also bounds admitted programs/jobs, independently of job count. Limits are configurable at world creation and checked on restore.

`RunReport` exposes work by origin, expanded jobs, deferred due count/age, capacity pressure, host deferral/rejection, stale generations, memory accounting, cancellation and deep zero-delay-chain diagnostics. Positive-delay cycles reset zero-delay depth. Budgets retain continuations at their original due time, so lateness is visible. Raising limits does not make overload disappear. See the measured envelope below.

## Cancellation and event-owned semantics

- Original zero-delay `cancelEvents` rows run as an activation prepass. They cancel previously scheduled **positive-delay** rows owned by their target source brick; newly admitted rows survive. Original direct `onToolBreak` delayed rows are the documented tracking exception. Named-target `onToolBreak` rows remain tracked.
- Self `onRelay -> fireRelay` programs infer original implicit delayed cancellation. Modernized zero-delay relay loops are not suppressed by the source's 15 ms flood gate or rewritten to 33 ms.
- `cancel_source(id, CancelMode::All)` and `cancel_origin(origin)` are explicit native administrative/lifecycle cancellation, including deferred zero-delay work. `AuthoredDelayed` reproduces the narrower original event command. Disconnect, map teardown and minigame cleanup must call the appropriate native cancellation, then remove affected brick programs. Generation checks remain required.
- Enable/toggle, print count wrap and overflow/underflow input chains are runtime-owned. The host applies `PrintDigit(0..9)` using original Letters/digit material aliases. Preserve the print-count cache with the program; initialize it from the original digit print or zero when importing.
- `disappear` owns its replaceable reappearance timer here. It dispatches `Presence` atomically: positive seconds hide rendering/raycast/collision then restore all three; zero shows and requests fake-dead revival; negative hides indefinitely. Reappearance survives a different triggering source brick's deletion because its owner is the affected brick. It is separate from authored `cancelEvents` tracking.
- Fake-kill/respawn, resource changes, spawning, health, burning, inventory, score, messages, projectile response and minigame commands are typed host intents. `semantics` supplies source math and predicates for health, bounce/redirect speed caps, variance/spawn positions, radius impulse falloff, directional slabs, vehicle recovery, sounds and input scope. The host must actually apply those rules and dependent subsystem effects. No generic string method dispatch is used.

## Persistence and migration

`save()` and `restore(catalog, trusted_bindings, bytes)` preserve program order/enabled state, opaque slots, print cache, delayed/zero-delay continuations, origin fairness cursor, sequence counters and reappearance timers. Restore checks catalog fingerprint, trusted current bindings, bounds, generations, target classes, action/row agreement, cancellation flags and deadline horizons. Snapshots are trusted server files, not uploads; never restore old client/session IDs as fresh authentication. Save alongside the authoritative world at the same clock. Maximum checkpoint input is 320 MiB; default admitted encoded state is 128 MiB. Save/restore are worker/pause operations, not per-frame work.

The original UI caps new delay entry at 30,000 ms. Native rows accept up to 300,000 ms to preserve the existing `bri-world` five-minute admission range during upgrade. Existing delay values must not be clamped on migration. `migration::legacy_event` converts all seven current native actions; unresolved content needs an explicit resolver. `migration::ui_event` plus `normalize_ui_row` maps the current UI values, original X/Y/Z event vectors to native `(x,z,-y)`, integer-row lists and original downward float-step quantization. `ui_catalog` emits the existing UI catalog JSON shape with **only caller-supplied input/output bindings marked supported**.

Offline `migrate_rows.py` converts selected original `+-EVENT` records and requires explicit source-name → native-ID aliases by datablock class. Unknown rows and index holes become `Row.preserved` slots that never execute, so later event indices remain stable. Preserve the untouched source records too. Do not blindly replace edited native rows from stale BLS records or copy existing UI opaque tokens to another brick; root's host adapter must merge them with the correct original identity/order.

Existing `World.pending` lacks original event row and input context. It cannot be reconstructed by triggering whole programs. Root must explicitly translate those already-authorized seven-action jobs with their stored target, source, order and due tick, or retain their old pending executor until drained while forwarding cancellation; neither automatic route is implemented here. Keep the original save until a migration comparison passes.

## Verification

```powershell
cargo test --manifest-path crates/events/Cargo.toml -- --include-ignored
cargo clippy --manifest-path crates/events/Cargo.toml --all-targets -- -D warnings
python -m unittest discover -s crates/events-import -p test_import.py
cargo run --release --manifest-path crates/events/Cargo.toml --example headless_probe -- content/events-pack-002/catalog.json artifacts/native-events-runtime/performance.json
```

Twenty Rust tests (including both private actual-catalog gates) and three importer tests cover all 65 dispatch routes, 4,096 rows, branching and zero-delay cycles, eight active origins, cancellation, host deferral, byte/admission/expansion limits, named scope/generation, timers, source math, migration and tamper-resistant checkpoint replay. The benchmark is the native scheduler with eight mutation-recording host adapters, not bots/physics/networking. Root must still bind all subsystem effects, editor/network/save paths and the complete eight-client gameplay workload before the alpha contract is satisfied.
