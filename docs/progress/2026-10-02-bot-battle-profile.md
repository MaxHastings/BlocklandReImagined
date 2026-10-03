# 2026-10-02 Bot battle profiling

Added an ignored headless diagnostic for a 16-bot Bedroom battle using the
generated Bedroom package and the converted Beta City 16 save. It records
per-`Session::step` p50/p95/p99/max, counts steps over 5/50 ms, fired shots,
and sampled bot path-search state across quiet, Gun, Rocket, Gravity Gun, and
Spear phases. The converted save was loaded from `worlds-pass-006` read-only;
the test adds 16 fixture spawn bricks in memory. This is a macOS debug build
measurement, not shipped Windows frame-time evidence and not a weapon accuracy
test.

One successful 960-tick run on Beta City 16 (8,541 bricks including fixture
spawns) measured Gun 7.514/12.834/15.145 ms p50/p95/p99 (max 21.039), Rocket
4.632/9.271/11.992 (max 16.191), Gravity Gun 5.015/7.343/8.635 (max 13.080),
and Spear 3.911/5.658/6.899 (max 8.951). Thus the latter three p50 values are
under the 8.33 ms 120 Hz tick budget, while Gun's p50 is near it; tails exceed
the budget across all four phases. This run did not observe a >50 ms Session
step. A previous run had a 132 ms Spear outlier, so intermittent stalls remain
possible and timings vary between runs. Loadout-change command latency was
0.11–0.18 ms in this run.

A 10-second macOS `sample` capture targeted the Beta City 16 Gun phase. In the
test thread, 4,633 of 7,973 stack samples were under `Session::step`; 4,541
were under `step_bots`. `bot_sight`/`Simulation::sight`/`target_filtered`
accounted for about 1,970 samples, with `walk_bricks` and `HashSet` insertion
and growth prominent beneath target filtering. This points to repeated
brick-target visibility traversal and temporary set work as a concrete hotspot
to investigate. It does not prove the cause of the longest individual ticks.
Navigation appears much smaller in this capture (individual `Nav::floor`,
`Ground::ray`, and search symbols had single-digit to low-double-digit
samples); this does not rule out pathfinding effects in other scenes.

The ignored test passed via
`BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-chaos --test bot_battle_perf -- --ignored --nocapture`.
The direct profiler run also passed. The binary was sampled with
`sample <pid> 10 1 -file /tmp/bri-v021-bot-battle-city-gun.sample`.
No interactive client was opened. Next, profile visibility traversal and
allocation behavior on the Windows build and representative AMC City input;
the available generated converted fixture was Beta City 16.

Follow-up optimization replaced `walk_bricks`' randomized standard
`HashSet` with `rustc_hash::FxHashSet` for its per-ray duplicate guard. The
predicate and membership behavior are unchanged. Two post-change Beta City
runs measured Gun p50 6.779/6.748 ms, Rocket 3.834/3.762 ms, Gravity Gun
4.538/4.442 ms, and Spear 3.779/3.777 ms; corresponding p95 values were
12.090/12.155, 8.192/7.920, 6.535/6.369, and 5.698/5.915 ms. Against the
preceding sampled run (p50 7.514/4.632/5.015/3.911 ms and p95
12.834/9.271/7.343/5.658 ms), these results indicate a useful reduction in
query cost, though timing variation remains and the profiler was not repeated
after the change. Shot counts and path samples were unchanged in both runs.

`cargo test -p bri-sim --test weapon_query --test building` passed all nine
tests, including brute-force long-ray comparison, targeting flags, map
occlusion, and swept weapon occlusion. The ignored battle profile also passed
twice after the change. These are debug macOS measurements; no Windows or
interactive performance claim is made.
