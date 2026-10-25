# 2026-10-24 Coarse walking guidance candidate

Branch: `codex/interdiction-server-latency-20261024`, after `c2fa7dc4bf998c882d3a59775257bf75c3cefcf2`.
Private evidence: `../BlocklandReImagined-private-evidence/interdiction-ui-lighting-20261024/server-lag-report/`.

The unchanged externally guided descent completes after the floor-arrival fixes
(`guided-deep-route-after-arrival-62`): house 17 s, basement 56 s, deep 235.5 s
total. This establishes native motor capability, not autonomous route selection.
Unguided deep pursuit remains the required comparison.

Native geometry baking separates topology from live execution. The prototype
uses source-authored landmark positions as candidate hints in the private
converter; only native geometry/profile checks admit edges. At 1,200 local
expansions (`navigation-bake-64`) all four forest-to-deep probes fail. Increasing
offline local work to 6,000, the existing runtime lifetime limit, connects all
four (`navigation-bake-67/68`): 1,953 nodes, 24,491 directed connections. No
runtime per-bot budget was raised. These are static graph results, not walks.

Optional bundle `navigation` now loads a versioned walking graph and clearance
profile. Shared reverse shortest-path fields guide the existing local search;
each leg still uses live collision and ordinary controls. Incompatible bodies
fall back to ordinary navigation. Failed legs retain the gameplay goal and
fall back rather than silently skipping the blockage. Maps without a graph
retain their previous navigation path. No wire format changes.

A native collision comparison passes: routing around a wall requires walking
away from the target; deleting the connecting edge fails; a different floor
and an oversized body cannot attach. `cargo check -p bri-sim -p bri-net
--examples --locked` passes. Autonomous ordinary-spawn traversal run
`autonomous-deep-guidance-69` **fails** after reaching house (19 s) and basement
(57 s): the local search substitutes a deeper drop at the first stair and
falls back toward the surface. The graph's walking-only capability contract
was not applied to its runtime legs. Run `autonomous-deep-walking-70` restricts
those local legs to the baked walking/drop profile, retaining the full motor
and ordinary planner for other goals. Its result is pending. Exact run-69 source and
input hashes are in `navigation-runtime-69-inputs/PINS.json`.

Not yet accepted: autonomous descent, group battle flow, graph CPU savings,
two-client latency, final gate/integration, or a new packaged playtest. The
private converter's `install_navigation.py` preserves diagnostics and installs
only completed connected bakes. Experimental content 3 is undelivered; prior
delivered previews remain unchanged.
