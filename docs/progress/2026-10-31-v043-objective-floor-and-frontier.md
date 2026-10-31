# Objective routing and floor-aware exploration

Maxwell reported flag runners staying below an elevated flag and failing a
route before eventually trying stairs. The exploration visit cache alone did
not govern flag objectives. Two relevant objective-path defects were identified:

- Feet-only objective `at_work` used horizontal distance only, suppressing stall
  recovery even when the objective was well above or below the bot. It now
  also requires the objective to be within the body's height. Other objective
  actions and interactions retain their separate existing reach behavior.
- A pedestrian objective search could discard a frontier at a soft expansion
  limit after making horizontal headway, even when the usable route first went
  around a wall or toward distant stairs. It now resumes the existing bounded
  frontier like a pursuit. The forced-limit detour test failed before this
  change and passed afterward. This retains the existing per-tick and hard
  bounds; large-crowd cost and route latency still require release validation.

Exploration visit memory separately records heights within each horizontal
cell using the current body's height, with 256 bounded entries. A destination
is no longer marked reached merely because the bot is directly below it;
nearby teammates on another floor no longer count as crowding the same point.
An active dated-search commitment also prevents a competing idle exploration
choice. This is a foundation for, not an implementation of, the requested
connected-building search.

Added a real package-backed CTF scenario: a no-jet runner starts under a flag
on a six-unit-high deck, uses ten brick-height stair steps, picks it up and
delivers it. The first capture occurred at tick 1809 (about 15 seconds), within
the test's 30-second bound. The same simple scenario also captured at tick 1809
with the old height check, so it does **not** reproduce Maxwell's delayed
replanning or prove that report fully fixed. The separate soft-limit test
proves the objective frontier defect; actual multi-floor saved-game behavior,
including flag returns, remains to be validated. Private comparison output:
`v043-upper-flag-first.txt` and `v043-upper-flag-prior.txt` in diagnostics.

At Maxwell's request, added a four-level no-jet CTF scenario with full floors
under the objective and repeated stair flights, starting outside the floor
footprint. The first version failed because its vertically overlapping solid
flights blocked each other; it did not establish a navigation defect. The
one-level control captured successfully. With flights separated, the runner
captured four times in 180 seconds, first at tick 4983 (about 42 seconds),
although return-trip stalls remain visible. This is an open-sided structural
fixture, not a test of enclosed-building doorways. Reports:
`v043-four-floors.txt`, `v043-one-floor-isolated.txt`, and
`v043-four-floors-separated.txt`.

Also adapted the actual Beta16 tower save, replacing its player spawn with a
real Slayer flag stand and zombie holes with ordinary bot spawns in an in-memory
copy. Existing Slayer CTF rules and ordinary pickup/return produced a capture
without jets: pickup tick 3417, capture tick 6548 (roughly 28.5 and 54.6 seconds).
The runner spent no sampled ticks directly below the flag in that run. A
repeat returned at tick 6638; incidental traffic can alter the return.
Moving the stand from the wall to the room's center also passed: pickup tick
3379, capture tick 6728, no jets and no time directly beneath the flag.
The original save is not written. This demonstrates one real tower route, not
that every flag level, return configuration or building has been solved.
The content-dependent test is `bot_tower_ctf` and needs `BRI_CONTENT` and
`BRI_TOWER_SAVE`; output `v043-beta16-ctf-nojet.txt`.

Checks passed: `cargo test --locked --profile playtest -p bri-sim
an_objective_keeps_its_frontier_for_a_detour_after_a_soft_limit --lib`,
`cargo test --locked --profile playtest -p bri-sim work_tests --lib`,
`cargo test --locked --profile playtest -p bri-sim explore::tests --lib`,
`cargo test --locked --profile playtest -p bri-chaos --test bot_knowledge`
(12 cases), `cargo test --locked --profile playtest -p bri-chaos --test
bot_gauntlet flag -- --nocapture` (upper-flag and existing CTF), and the
long-distance CTF run. `cargo clippy --locked --profile playtest -p bri-sim
-p bri-chaos --lib --test bot_gauntlet --test bot_tower_ctf -- -D warnings`
passed. The final flag filter covered all three synthetic CTF scenarios, and
both private tower variants passed with `--ignored --test-threads=1`.
