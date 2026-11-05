# 2026-11-04 v0.4.4 route controller ownership

The frozen-input extended gate on `95ba9d903` ran 531 binaries in 1120 s
and failed the four-floor stairwell objective, including its isolated retry.
The mirror/free-camera and mounted-personal-gun cases passed only on retry;
those are flaky results, not clean first-pass success. The mounted test's
first observed ray differed from a sampled view by about 0.00016 in one
component; its original assertion and the engine aim path are unchanged.
No source95 gate pass, main merge, tag or publication occurred.

The released v0.4.3 stairwell control passes. Restoring only preview.6
partial-route ranking does not change the failure. Restoring preview.6
safety behavior passes; using only the actual route endpoint also does not
fix the failure. The missing distinction is the active controller's intent:
`resolve` remembered the ordinary straight waypoint bearing even when an
admitted leap, jet or climb controller supplied a different direction.
The subsequent safety check could veto its own braking or takeoff correction
as though it were unrelated footwork.

The resolved route intent now follows that controller. Later diversions are
compared with its actual direction, and only a resolved leg controller owns
the flight/climb exemption. Ordinary drop depth is bounded by the current
leg's endpoint instead of scanning several later waypoints. This preserves
planned traversal and denies unrelated descents without a new tuning dial,
pathfinder, content identity or movement rule.

`cargo test --locked -p bri-sim --lib`: 524 passed, 5 ignored. The physical
support fixture checks admitted controller steering, a diversion's lack of
landing permission, a small step beside a deep drop, and an intentional
descent. `cargo test --locked -p bri-chaos --test bot_gauntlet --test bot_routes
--test bot_hardening --test bot_navigation_spike`: 27/10/6/8 passed, 9 ignored
in the gauntlet. The original four-floor flag, unreachable-roof and deck
assertions pass unchanged. Changed-area all-target Clippy passes with
`-D warnings` for sim, chaos and client.

The saved Bedroom stairs and balcony checks pass with their original rules.
The 60-second private tower still fails the stronger all-three-before-death
assertion, as does the matched approved preview.6 control. All three start
pursuit before their evidence expires; the ordinary player is killed by a
pursuer and there are no uncredited bot deaths. The earlier respawn-observer
experiment was discarded after three passes and one failure. This remains a
recorded alpha diagnostic limitation; it does not invite per-save NPC tuning.
Maxwell accepted the integrated NPC gameplay and preview.6. No private case
was changed or added to the full gate's existing fixture-only exclusions.

New source requires current importer receipts, exact-source platform CI,
full release gate, native startup/package checks and archive verification.
The prior candidate's artifacts cannot certify this source. Source availability
was rechecked for all 214 seam rows and 572 named items; the client app module's
stale documentation path now points to `app/mod.rs`. These are availability
checks, not claims of consumer completeness or original-game fidelity.
