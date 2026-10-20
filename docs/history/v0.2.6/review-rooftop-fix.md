> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-rooftop-fix.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: rooftop fix, hop landing over a real jump's air time (a84f8215, branch tip 00c6bd69)

Reviewed a84f8215 on its parent 89655e7 (ec7db637 + main d068f06e). 00c6bd69 is a progress note only.

Verdict: FAIL, one small must-fix. The cause and the main fix are right.

## Cause: real
- hop_lands judged drift over HOP_FLIGHT 0.8 s. A jump is up at jump_speed and back down under gravity: 2·12/20 = 1.2 s. At the reported 4.2 u/s from 3.4 u, the old judgement gave 3.36 u (lands, just) and the real hop gives 5.04 u (off the deck). The numbers line up exactly with the fall.
- Deriving the time from the body's own tuning and deleting HOP_FLIGHT is the right fix: one source, no constant. It is consistent with reach.rs:416/:582, which already derive the jump rise from the same jump_speed and gravity.

## Must-fix: a dodge hop with jets flies longer than the hop it is judged as
- The hop dodge (extras.rs ~464, `_ if t < HOP_TICKS`) holds jump AND `extra.jet = jets.is_some()` for HOP_TICKS (30 ticks, 0.25 s). Jet thrust is jet_acceleration 2000/90 ≈ 22 u/s² (motor/player.rs:691, :1649), which is more than gravity 20. So a jet-capable bot's dodge hop rises for longer and stays in the air well past 2·jump_speed/gravity.
- `up = hop_lands(feet, velocity, &tuning)` (extras.rs:385) still judges it as a plain hop, so near a ledge the same fall can happen through the dodge.
- This is inferred from the code, not run. Confirm with the same kind of test the lane already wrote: measure a jetting hop's air time.
- Fix (either, no new constant):
  (a) judge a jetting hop over its jetted flight, from the motor's jet numbers or `route::Jets`, which already measures jet flight; or
  (b) a hop dodge only jets when the landing check covers that flight. Otherwise it hops plain.
- Test: a jet-capable bot near an edge never takes a hop dodge that carries it off.

## Asked: is the 0.1 s tolerance a magic number?
It's a test bound, and it's justified in size: 12 ticks around 1.2 s is the grounded flag flipping a few ticks off take-off and landing. But it has no stated source. Small, not blocking: name it with its reason (e.g. in ticks, "the ground check's reach either side"), or tighten it to the ticks the motor actually differs by. The test's real claim (the formula matches the motor, within a few ticks) holds.

## Asked: does other code assume a fixed flight time?
- No other fixed hop/jump flight constant in bots, route, reach or nav (searched FLIGHT / AIR / HOP_ / jump_speed uses). route.rs:365 and reach.rs derive from tuning.
- The one remaining mismatch is the jetting hop above.
- HOP_TICKS / STRAFE_TICKS are control durations, not flight predictions. STRAFE uses `tuning.forward * STRAFE_TICKS / 120`, a bare tick rate, which is on the v0.2.7 tick-constant pass.

## Evidence
I didn't re-run the gauntlet; the lane reports bot_gauntlet 18/18 (rooftop fell=0, knocked_off=0, kills=52, team_kills=0), all_dials_on, bot_tactics 16/16, bot_perception 5/5 and bri-sim bot tests 193. Re-run the rooftop and the new hop test after the jet fix.

## Round 2: f6c83670 (delta on 00c6bd69). PASS with one one-line must-fix.
- The jet must-fix is fixed. hop_flight models the climb at jump_speed, then (jet_acceleration − gravity) for the jet time, then the fall from the top. It reduces to 2·jump_speed/gravity without jets, and `can_jet` gates it.
- The dodge's "up" ways are judged over HOP_TICKS of jets whenever the bot has jets. Ground hops and the strafe aside count none.
- Counting full jets for HOP_TICKS when energy runs out sooner judges a LONGER flight, so it can only refuse more hops (safe side).
- New test a_hop_that_fires_its_jets_is_judged_over_its_jetted_flight flies a real jetted hop. Ran here: hop_tests 2 passed.

### (b) LANDING_SLACK: typed, not derived. Must-fix (one line)
`const LANDING_SLACK: f32 = 0.1;` while its comment says three 32 ms ticks (= 0.096). The motor exports `bri_motor::player::TORQUE_TICK` (player.rs:18, 0.032). Write it as `3.0 * TORQUE_TICK` (via bri_sim::player's re-export), so the number is its reason.

### (a) 193 → 172 bot tests: a different filter, nothing disappeared
Counted with `cargo test -p bri-sim --lib -- --list`:
- tests under the bots module: 172 at 00c6bd69, 173 at f6c83670 (+1, the jetted hop test);
- the whole lib: 337 → 338;
- names containing "bot": 181 → 182.
No test was removed between the rounds. The "193" from last round matches none of these filters, so it was a different selection (inferred: probably a broader filter or other test binaries).

### Note (no change asked)
New code adds `/ 120.0` tick-rate literals (extras HOP_TICKS as f32 / 120.0, the tests). That's the existing style in bots (no shared tick constant exists yet), and it's on the v0.2.7 tick-constant pass.
