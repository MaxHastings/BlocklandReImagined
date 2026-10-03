# 2026-10-02 Sol performance evidence for v0.2.1

Reviewed and retained the existing per-ray FxHashSet duplicate guard. It only
checks membership and does not determine traversal order, tied-hit selection,
collision, perception range or tick scheduling. The optimized profile then
identified persistent world-map lookups as a substantial remaining cost.
`walk_bricks` now passes its already fetched brick into private `ray_brick`,
removing an identical second lookup. This changes no collision or acceptance
predicate. No dependency, schema, protocol, manifest or lockfile changed.

The existing ignored `bot_battle_perf` diagnostic now labels the old `shots`
number correctly as live-projectile samples, separately counts unique observed
projectile IDs, reports the worst tick, sampled snapshot construction/encoding,
and a snapshot/brain evidence digest. World/tick/stop-after environment controls
support bounded repeatable captures and reject unavailable selected fixtures.
No wall-time threshold was added to a test.

On macOS Apple M1 Pro / 16 GB with the generated Bedroom and Beta City 16
content read-only (8,525 saved bricks + 16 in-memory spawn bricks), optimized
960-tick phases were repeated for original HashSet, FxHashSet alone and the
final Fx/reference version. Final Gun medians 0.627 / 0.635 ms compare with
baseline 0.810 / 0.792 ms; Rocket 0.447 / 0.458 vs 0.521 / 0.529;
Gravity Gun 0.574 / 0.583 vs 0.634 / 0.636; Spear 0.469 / 0.471 vs
0.514 / 0.513. All short-run steps stayed under 5 ms. Every sampled state
fingerprint, projectile count, searching sample and mean path length matched.
Full p50/p95/p99/max and repetition limits are in
[the performance audit](../audits/v0.2.1-performance.md).

Repeated 5-second optimized macOS `sample` captures confirm that the matching
SipHash branch disappears. Ordered-map lookup self samples fall from
587/2,423 test-thread samples (24.2%) to 416/2,306 (18.0%). Remaining sight
query / brick collision / set-growth costs remain more prominent than navigation
in this scene. Profiler-attached runs show roughly two-second maximum ticks
near attachment, so their maxima are not reliable gameplay evidence.

An unsampled final 20,000-tick Gun phase (166.7 simulation seconds) passed with
0.672 / 1.064 / 1.288 / 2.886 ms p50/p95/p99/max, no steps above 5 or 50 ms,
and exactly the baseline long-run state fingerprint and behavior counters.
This is headless macOS evidence, not shipped Windows FPS evidence.
The earlier unsampled debug 132/141 ms outliers and reported Windows frame
stalls are still unexplained. AMC City is absent from these generated fixtures.

Commands: `cargo test --release -p bri-chaos --test bot_battle_perf --no-run`,
then the reported test executable with `BRI_CONTENT` pointing at the main
checkout's content, `BRI_BATTLE_WORLD='Beta City 16'`, `--ignored --nocapture`;
`sample <pid> 5 1 -file <path>`; and
`cargo test -p bri-sim --test weapon_query --test building` (all nine pass).
`CARGO_TARGET_DIR` remained unset and builds were coordinated with root.
Raw `/tmp/bri-v021-sol-*` logs and sample paths are recorded in the audit.

Read-only client/host review found that the host step timer excludes replication
construction/encoding and all client work. World-effect attachment scans are
world-revision dependent; effect particle sampling, cosmetic collision and
GPU preparation are separate candidates for the frame stalls. None is claimed
as the proven cause. Root retains client ownership. Next: measure the integrated
NPC planning budget on this same harness, and capture shipped Windows host,
render-thread and effects/brick-revision evidence together. The alpha contract
and interactive acceptance remain open; no visible client was launched and no
original installation or content pack was written.
