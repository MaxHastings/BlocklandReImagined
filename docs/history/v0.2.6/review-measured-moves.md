> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-measured-moves.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: measured movement, branch claude/v0.2.6-measured-moves-x42j1c @ 2515cce (main ac2bdc2 merged)

This was read-only: nothing was built or run. I read all of `reach.rs`, the `nav.rs` and `route.rs` diffs, the shared-file edits, the new tests and the two progress notes.

## Verdict: passes. It can merge once the lane's chaos suite comes back green.

## Against its own plan

**No fudge factors left in the predictions.**
- Removed:
  - `rise * 0.8`, replaced by the measured ledge;
  - `JET_RANGE` 30 and `JET_CLIMB` 32;
  - the climb, cross and sink formula;
  - its extra 0.5 s;
  - the `seconds >= 0.5` rule.
- A leg now flies when its energy holds the leg's measured jetting time (`route.rs` `Jets::of`, `flight`).
- Patience is `PATIENCE` × the measured time. Before it was 2× plus 2 s on a guess.
- The bisection's upper bound still uses the jump formula (`reach.rs` `ledge`). It only bounds the search; the answer is measured.

**No special cases.**
- No content names anywhere.
- The window test builds a wall from ordinary bricks, and the grid finds "a floor only a crouched body fits".
- The vehicle `Body` literal gains `crawl_jump: 0.2` beside its existing 0.2 jump, which is the same shape as before.

**The measurement is keyed on the full tuning.**
- `Reach::of` matches the whole `PlayerTuning` by `PartialEq` (`reach.rs` `of`), and that derive covers every field (`motor/src/player.rs:574`).
- A scaled body is measured as `tuning.scaled(scale)` (`nav.rs:88-93`).
- Bots pass their live `player.tuning()` with scale 1 (`bots.rs:1719, 1723`), so `Body` and `Jets` read the same measurement.

**One mechanism per job.**
- The measurement flies the same leg controller the bot flies (`route::jet`), and presses jump and crouch on the same `PRESS_NEAR` rule (`reach.rs` `lands_on`, `bots.rs:2877-2889`).
- So what is measured is what the bot will do. There's no second model of the body.

**The shared-file edits are as described.**
- `bots.rs`:
  - `PRESS_NEAR` replaces two literal 1.6s;
  - crouch waits until off the ground on a jump-into-crawl;
  - the jet leg starts from the planned launch point and passes the tuning.
- `extras.rs`: a single `.as_ref()`.
- `interactions.rs`: one field.

**Tests can fail for real.**
- `reach.rs`:
  - lower gravity jumps higher and jets on less;
  - a stronger jump lands higher;
  - stronger jets fly quicker;
  - jets too weak to lift give none;
  - one measurement per tuning.
- `nav.rs`: a window up a wall is jumped into crouching, and one higher than measured has no path.
- Chaos: `a_bot_jumps_crouching_through_a_window_up_a_wall_to_reach_its_enemy`.
- The parked jets test is back on.
- Nothing was loosened. These are sanity absolutes, per Max's rule.

## Not blocking

1. **The cost for odd bodies isn't bounded the way it is for the stock one.** This is inferred from the code, not measured.
   - The stock tuning costs about 90 ms once. The worst case is set by:
     - `MAX_SAMPLES`² jet legs, which is 256;
     - each run up to `MAX_SECONDS` (30 s, which is 3,600 motor ticks);
     - two bisections from step height up to `jump_speed²/2g`.
   - Validation lets gravity be any value above 0 (`player.rs:743`). A low-gravity Add-On body therefore gets a much wider bisection and long flights that may never settle. That could stall the first tick a bot with that body plans for whole seconds, for every player.
   - That is exactly the body the acceptance run (§7) uses.
   - Not this PR's problem for vanilla, since Max accepted 90 ms. But the warm-at-load design must say how it bounds this: for example, a total tick budget for one measurement, past which it takes what it has.
2. **The leg controller still has fixed numbers:** `leg_push_speed`'s 6.0, plus the 0.9 and 0.3 landing tests. The measured times include them, so they don't skew predictions. The progress note lists them. Name them when the vehicle work is in that code.
3. **`Body.drop` is still a flat 4.0** (`nav.rs`). That's the next thing to measure. It's the same fall rule the ledge-push harm reader (plan item 7) needs, so the two should share it.
4. **`highest` is conservative.** It's set from the first height where any crossing in the row fails (`JetReach::measure`). A long crossing that fails can then forbid a short climb at that height. That's rare and safe.
5. **Stale doc line:** the progress note still says `route::JUMP_TAKEOFF`, which is now `PRESS_NEAR`.
