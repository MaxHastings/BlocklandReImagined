# 2026-10-03 v0.2.3 Standard crouch collision audit

Maxwell requested comparison of crouched player collision dimensions with v20. This lane reviewed source read-only and prepared a test-only patch; it did not change player tuning or automate an interactive playtest. Root owns integration and all runtime execution.

## Source and conclusion

Fresh reads of recovered v20 `PlayerStandardArmor` at `/Users/maxhastings/Documents/Blockland-Max/BlocklandReImagined/.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs:8783–8784` and the secondary `bl-decompiled/v20/server/scripts/allGameScripts.cs:8773–8774` under that same research root confirm standing `VectorScale("1.25 1.25 2.65", 4)` and crouched `VectorScale("1.25 1.25 1.00", 4)`. Recovered declarations alone give datablock fields, not native collision units. The checked native executable receipt already recorded in `docs/player-simulation.md` and the 2026-09-27 player-clearance entry in `docs/progress.md` establishes the engine's independent quarter scale, object scaling, and feet-at-bottom convention. No fresh native executable disassembly was run by this lane. The Windows E: reference is unavailable on this Mac.

The current standard motor therefore has the same box dimensions: 1.25×1.25×1.00 crouched, 1.25×1.25×2.65 standing, multiplied by canonical object scale. Feet remain the bottom; the crouched centre is feet +0.50×scale. `PlayerTuning::scaled`, `shape`, `pose`, `Player::world_bounds`, and the actual polygon sweep use this same shape. There is no proven collision-size discrepancy and no production correction is warranted. Rendering eye/animation blending does not interpolate these collision dimensions: crouch changes collision at the next authoritative 32 ms motor tick.

Importer `player_types::convert` quarters original PlayerData square boxes. Existing importer coverage checks both exact standard authored fields; differing horizontal widths and unknown external-base stance widths remain explicitly unsupported rather than silently converted. This audit does not introduce a second collision identity or special-case standard player names.

## Runtime boundary requested

`/tmp/bri-v023-crouch-collision-tests.patch` adds `standard_crouch_collision_matches_v20_clearance_at_each_player_scale` in sim player integration tests. At scales .75, 1 and 1.5, it drives the real Player through ordinary forward/crouch controls: standing cannot enter a low gap; crouching passes a just-large-enough 1.29×1.04 scaled gap but cannot pass a 1.21-wide or .96-high gap. Releasing crouch beneath the roof must retain the short box, its actual collider, bottom feet and middle. Walking out must restore the standing height. This supplements existing crouch ceiling, step/lintel, jump/jet and corner collision tests with exact width/height and scale boundaries.

```sh
cargo test --locked -p bri-sim --test player standard_crouch_collision_matches_v20_clearance_at_each_player_scale -- --nocapture
cargo test --locked -p bri-sim --test player
```

Temporary proposed source passed rustfmt; patch passed git apply --check. No Cargo command ran in this lane because root owns the compute lease. Runtime result pending root.

## Limits

Matching the standard box does not establish all movement or arbitrary add-on fidelity. `docs/audits/torque-quirks.md` separately records v20 extending standing-clearance checks while rising/holding jump; the current motor checks the actual standing shape and then performs its swept movement. The precise original extra extension is not freshly established here, so no speculative clearance enlargement is proposed. This dimension audit does not close that broader movement comparison, the unreproduced Windows firefight NaN, or the user's death disconnect report.
