# Vanilla event runtime evidence

2026-09-26. New isolated `crates/events` and offline `crates/events-import`; no existing/shared engine files, source installation, visible window, desktop input or audio playback were modified/used. Root owns integration and progress records.

## Source inventory

The offline compiler scans all 79 ZIP packages in Maxwell's designated E: reference and recovered core `allGameScripts-Vanilla.cs`. Pack002 contains 16 inputs, 65 outputs and five byte-identical source proof files. Core contributes nine inputs and all 65 outputs. Teledoor adds enter/exit, Key adds match/mismatch, Sports adds touchdown/ball hit, and Tutorial adds map-specific tool break. Those are the only registration-bearing source packages found. Tutorial's input should be exposed only when its actual host hook is available; inclusion is not an invented global tutorial behavior.

Independent verification compares each proof file with its separate original/recovered input, checks exact registration identities/source lines and compares all 65 typed parameter/append-client specifications against the transferred UI pack. The UI pack has only the nine core inputs; seven add-on/map inputs are newly accounted for. See `artifacts/native-events-runtime/verification.json`. `coverage.csv` names every output, its route, original source line and outstanding host binding. Presence of unrelated packages is not a claim of vanilla compatibility.

## Source semantics retained or deliberately modernized

Recovered core lines 111–493 establish activation snapshots, zero-delay CancelEvents prepass, delayed schedule ownership, enable/toggle lists, implicit relay cancellation and the direct onToolBreak exception. Lines 17134–17142 register the core input targets. Lines 17380–17416 register the 37 brick outputs; GameConnection contributes five, Player fourteen, MiniGame five and Projectile four. Source line numbers are verified in pack002, not guessed from an alternate decompilation.

Core `serverCmdAddEvent` floors float values to the authored step from the minimum; UI migration preserves this. Runtime validates typed values and native datablock membership. String display and safe native markup are host responsibilities; no string is evaluated. Original fallback substitution for an unresolved datablock is intentionally replaced by an explicit binding error/preserved row, so missing content cannot silently become a different light, item or projectile.

The original self-relay admission rewrite to at least 33 ms, directional 33 ms schedule and 15 ms relay flood suppression conflict with Maxwell's explicit zero-delay modernization requirement. The native scheduler instead orders same-time work and budgets both execution and expansion. Author-specified positive delays remain intact. The source's silent flood/over-quota drops are replaced with atomic admission errors or retained internal continuations, observable by origin and due age. This is intentional engineering behavior, not a claim that the old engine scheduled zero-delay relays this way.

Native same-origin ordering is due time then admission sequence; relay descendants append behind existing siblings. Different ready origins are round-robin rather than one build draining a global queue. Later row enables do not unschedule captured outputs. Cancellation of old positive-delay schedules occurs before a new activation commits. Native administrative cancellation additionally covers deferred zero-delay loops.

`fxDTSBrickData::disappear/reappear` enables all rendering/raycast/collision flags on return, not saved previous values. Its timer is not the ordinary authored cancel list. Print-count increment/decrement wraps base ten and emits overflow/underflow, then updates the original digit print. The host receives the print mutation synchronously before queued dependent output execution.

Original input adapters still matter: PlayerTouch applies spawn immunity and held-admin-wand exclusion; MiniGame target selection differs between explicit Legacy LAN and normal matching scopes; BotTouch separates Bot, seat-zero Driver and fallback quota client; ProjectileHit separates projectile source Player, Player's Client and captured invoking client. `Trigger.client` is deliberately distinct from target slots. Bot and Driver target class is Player. There are **no Vehicle-class output registrations** in this vanilla set; vehicle event commands target the spawn brick. Root must bind these actual hooks rather than infer targets from arbitrary IDs.

Native semantics helpers implement damage/heal behavior without reviving dead players; reflected/redirected projectile speed cap 200; original uniform velocity variance; thin-brick projectile spawn axes; squared-distance radius impulse falloff; face relay slabs; vehicle recovery's human/nested-passenger exclusion; sound looping/spatial predicates and item/fake-kill placement math. Physics, damage permission, inventory, native effects/audio, minigame lifecycle and presentation remain authoritative host bindings. The weapons contact hook and minigames typed commands already provide appropriate neighboring interfaces; they were read, not edited.

## Tests and measured envelope

Twenty Rust tests pass, including actual pack compile/execute coverage for all 65 output routes. Three offline importer tests pass. Clippy with warnings denied passes. Cases cover 4,096 authored rows, source-order branching, same-time cycles, eight independent origins, named group scope/generations, atomic overload rejection, resumable internal branches, origin cancellation, source CancelEvents semantics, enabled-row snapshot behavior, byte/expansion limits, preserved indices, timers, typed migration and save/restore corruption rejection.

Release benchmark `artifacts/native-events-runtime/performance.json` ran on Windows x86_64, AMD64 Family 25 Model 97 Stepping 2. Sustained eight-origin loops with 64 rows/area and 512 dispatch attempts/phase measured median **0.5661 ms**, p95 **0.7507 ms**, maximum **1.0425 ms** over 240 phases. All eight origins received equal service; maximum overdue age was zero.

The overload case used **4,096 rows per area × eight origins**, 4,096 dispatch attempts/phase, 512/origin, 8,192 expansions/phase and 4,096 expansions/origin over 120 phases. Median was **7.0738 ms**, p95 **15.5098 ms**, maximum **31.9385 ms**. Work remained queued; maximum observed overdue age was **75 ms**. Expansion budgets staggered large batches, with service differences bounded below one authored activation in this workload. The 9,661,602-byte checkpoint saved in 64.88 ms and restored in 324.11 ms, then matched the next phase's schedule counts. These are scheduler/recording-host CPU times; the combined gameplay tick needs additional budget for real host mutations, bots, physics and networking.

An earlier overload probe exposed repeated full-queue scans and recompilation during relay expansion (p95 about 85.7 ms). Compiled program caching, indexed delayed cancellation and separately charged expansion work reduced that observed cost. The final report above contains the rerun after checkpoint accounting consistency fixes; an earlier optimized run measured 7.36 ms p95, while the final run measured 15.51 ms. These runs are not isolated machine benchmarks, and the higher final result leaves limited shared tick budget under overload. Old figures are diagnostic history, not current performance claims. A single external `trigger` still prepares its bounded activation synchronously. Hosts must pace incoming trigger bursts; very large fanout beyond configured expansion quota is explicitly retained/reported, not promised to finish within a fixed frame.

## Reproduce and handoff

```powershell
python crates/events-import/import_events.py 'E:/Downloads/B4v21Launcher/versions/Blockland v20' .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs content/events-pack-002
python crates/events-import/verify_catalog.py content/events-pack-002 'E:/Downloads/B4v21Launcher/versions/Blockland v20' .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs content/ui-pack-003/ui-pack.json artifacts/native-events-runtime/verification.json
python -m unittest discover -s crates/events-import -p test_import.py
cargo test --manifest-path crates/events/Cargo.toml -- --include-ignored
cargo clippy --manifest-path crates/events/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path crates/events/Cargo.toml --example headless_probe -- content/events-pack-002/catalog.json artifacts/native-events-runtime/performance.json
```

Conversion requires a fresh output path outside the canonical original root; preserve pack001 as the earlier prototype. Import tooling reads literals and original rows only, never runs scripts. Native event resource bindings and save migration are explicit host inputs. The README documents API, cancellation, original coordinate conventions, large-row UI integration and the existing `World.pending` migration gap.

Remaining alpha work: shared Session/world event replacement and current pending-save migration; real Player/Client/Projectile/Vehicle/Bot/MiniGame intent adapters; actual input hooks and minigame/trust provenance; editor capabilities/replication/transport; original message markup and resource semantics in presentation; combined eight-client/bot/physics/network load measurement. Isolated dispatch tests are not evidence those user-facing behaviors are already playable. No acceptance checkbox or scope requirement is waived by this handoff.

