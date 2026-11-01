# Afghan planning latency and stair-edge feedback

Maxwell reported idling CTF bots, stair-edge falls, and balcony zombies
attempting difficult short routes rather than reliably using the stairs.
The actual `TDM Afghan` saved build loaded all 14,107 bricks. In a 1,200-tick
baseline trace, 13 of 15 bots repeatedly searched without a route for most of
the interval; search frontiers grew beyond 20,000 nodes. A prior pursuit
change unconditionally resumed useful partial routes. The planner now follows
a partial only when it makes substantial progress toward a distant goal and
does not end near the goal's horizontal projection, while short, backward and
unproductive detours retain their frontier. In the candidate replay all 15
bots had movement routes by tick 486; this measures issued routes/input, not
completed captures. The retained-frontier detour regression still passes.

The walk graph now gives exposed lateral edges a cost, using nearby floor
support within the motor's step range. A straight shortcut checks lateral
support too; narrow-only paths remain legal. Support sampling invalidation
includes the wider margin. Lateral rather than forward/back exposure avoids
penalizing ordinary stair risers. A synthetic wide staircase first reproduced
edge-hugging and now passes both the raw and pulled-route check. The first
implementation changed a gap landing; a stronger preference for a centered
landing restored the existing real-motor ledge regression without removing
footprint-only landings.

Headless candidate replays, using the original saved builds and normal controls:

* Staircase: target death tick 2041 versus baseline 3241; sampled positions
  show all three zombies crossing the first turn without the two falls visible
  in the earlier run. These samples do not prove every turn or run is perfect.
* Cube: no-jet target death tick 7081 versus earlier 8479, with zero set-aside
  bricks; arbitrary stacking is not established.
* Tower: target death tick 5283; both existing real-save flag placements pass.
* Mattress balcony: target death tick 3007 versus baseline 2406. The group
  advanced farther through the house in this run, but the first kill was slower;
  this is mixed evidence and does not establish that every zombie reliably
  chooses the easiest route. The first safety-cost candidate failed to damage
  the target within 3600 ticks and was replaced, not accepted.

Reports live in private planning evidence diagnostics as
`v043-{afghan,stairs,balcony,tower,cube}-feedback-*`. `nav::tests` passed 47
with one pre-existing ignored measurement; the focused detour and projection
checks and both private `bot_tower_ctf` cases passed. Further changed-area
checks passed: `cargo test --locked --profile playtest -p bri-chaos --test
bot_routes` (10), `cargo test --locked --profile playtest -p bri-sim --test
nav_invalidation` (1), the raw/pulled stair route test, and `cargo clippy
--locked --profile playtest -p bri-sim --lib -- -D warnings`. No new
interactive playtest or general performance claim is made.

## Bounded crowd measurements

After Maxwell finished playtesting, ran the existing real-content
`bot_deathmatch_stress` harness against the 14,107-brick Afghan save, two
teams, fresh Slayer policy and the current navigation, using
`cargo test --locked --profile playtest -p bri-chaos --test
bot_deathmatch_stress -- --ignored --nocapture`. The 16-bot, 10-second run
had 2.792 ms mean step, 5.596 ms p99, zero of 1,200 ticks over 8.3 ms, and no
bot-step failures. The 32-bot, 30-second run had 5.741 ms mean, 10.298 ms p99,
241 of 3,600 ticks over 8.3 ms, and no bot-step failures. It also reported
partial routes/stalls and some network-view motion errors; a passing test is
not a claim of flawless play.

The existing 32-bot Afghan release gate uses a 5.5 ms mean on the release
profile, a five-weapon loadout, and its own packaged content. This playtest
profile run used a three-weapon loadout and cannot be directly certified
against that gate. The observed mean above the reference and spikes are a
performance concern; run the actual release-profile gate on the final candidate
and investigate a demonstrated failure rather than treating this as green.
