> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-shove-landing.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: `reach::shove_landing`, dc0c93c (one commit on b3b84f4)

This is a read-only review of the one commit, `crates/sim/src/reach.rs` (+109). Nothing was run here.

## Verdict: approve the shape. Two fixes before the fighting lane builds on it.

It's what the hand-off asked for:
- it flies the real motor on the target's tuning, against the real fixed collision and water, with no input;
- the impact is the motor's own vector;
- the result is `None` when the flight doesn't land in time;
- a ledge test pins it.

The test passes the same way the hand-off reads: lands below, past the edge, impact down; into nothing it gives `None` with the allowance spent.

## 1. `allowance`: acceptable, and still one budget, if the caller lends it from `Budget`

- Passing a `&mut u32` of motor ticks keeps `reach` independent of `session::bots`. That's better than making the bots' `Budget` public.
- It stays **one** limiter only if the fighting caller:
  - takes the allowance from `Budget` (its remaining share, converted to ticks);
  - charges back exactly what `shove_landing` spent (`before − after`).
- A fixed per-call allowance constant on the caller's side would be a second dial. Say that in the fighting PR, and I'll check it there.

## 2. Fix: the collider copy isn't charged to anything

- Each call builds a fresh physics world. It copies every fixed collider inside a box sized from the *whole* allowance:
  - fall ½·g·t² = 40 units at 2 s with gravity 20;
  - flat speed·t;
  - then runs `detect_collisions` over them.
- That cost scales with how many bricks are near the target, not with the allowance. In a dense build that's possibly hundreds or thousands of colliders, inserted and broad-phased per call (inferred; not measured).
- Spots weigh pushes from up to seven places, so one planning turn could repeat this many times for the same target.
- **Fix, either of:**
  - **(a) Build once per target per planning turn and reuse it (preferred).** Cover the union of the candidates' boxes. Fly each candidate in it, removing the player body after each.
  - **(b) Charge the copy to the same allowance**, for example one tick per so many colliders, with `None` once it's spent.
- Either way, the 2000 µs `bot_think_time_16` bar can only be judged once fighting wires it in. Run that bench in a large build as part of the fighting PR.

## 3. Fix: return the strongest impact of the flight, not only the landing tick's

- The session gives **every** tick's `motion.impact` to the fall rule (`session.rs:2808` → `step_combat`). `MotionEvents::impact` is "velocity removed by collision this tick", so a shove that slams the target into a wall on the way down hurts there too.
- Returning only the impact on the `landed` tick means bots under-value shoving someone into a wall or down a stepped drop.
- **Smallest fix:** keep the largest-magnitude impact seen from launch to landing, and return that instead. The fall rule rises with speed, so that's the harm that counts.
  - Summing harm over every hit would be exact, but it needs `fall_harm` per hit. Not worth it now.
- Also say in the doc comment that it's the strongest impact, so the capabilities lane's `fall_harm(.., impact)` reads it as one hit.

## Other checks

- **Impact matches what `step_combat` reads:**
  - It's the same vector (`before_collision − velocity`, from `MotionEvents`), with the same Fall-vs-Impact direction test and guard facing.
  - `events.landed` only fires on a stepped Torque tick (`ticked`), the same as in the game.
  - Fix 3 is the only gap.
- **Scale:** the doc comment says "its scale applied". The fighting caller should pass the target's live `peer.player.tuning()` (what the motor flies it with), not rebuild a scaled tuning. Say that in the doc so nobody scales it twice.
- **Determinism:**
  - The fresh world, fixed inputs and a tick count, never wall time, make it deterministic.
  - The copied colliders come back in the main world's BVH order. That order follows the session's own insertion history, so replays match. It could only affect motor contact order in exact ties. Fine.
- **Water:** a fall into deep water floats and never lands, so it spends the whole allowance and returns `None`. That's correct (no fall damage there), but it costs the most. If (a) is done, it's charged once per candidate, which is fine.
- **Not covered** (air steering, other bodies and vehicles, openings, unstreamed terrain tiles) is written down. All of these just make the bot mis-predict now and then, and the shot judge corrects for that, as the design intended.

---

# Re-check: 2fd8ced (on dc0c93c)

**Both fixes are in. Passes. Merge exactly 2fd8ced.** It contains b3b84f4, so merging it lands vehicle handling and shove landing together.

1. **The copy is charged.**
   - Each collider copied spends one unit of the same allowance (`checked_sub(1)?` inside the query loop).
   - The query is a lazy iterator, so a dense build stops copying once the allowance runs out. It doesn't copy everything and then fail.
   - There's still one limiter.
   - Treating one collider as costing about one motor tick is a rough exchange rate, not a measurement. The fighting PR's `bot_think_time_16` run in a large build is what proves it, as agreed. If it shows big builds spend the whole allowance on copying, so that pushes there are never valued, that's when to add a per-target cache.
2. **The hardest impact.**
   - The hardest impact from shove to landing is kept by length, and the doc comment says so.
   - The new test is real. A wall at x=1 and a shove at +12 give `impact.x > |impact.y|`, which the old landing-only result (~(0,−1,0)) fails.
   - The sign is right: impact = velocity removed, so it's +x into a +x wall.

Nothing else changed.
