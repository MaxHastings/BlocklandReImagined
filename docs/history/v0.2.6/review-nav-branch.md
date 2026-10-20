> **Historical note. Written 2026-10-08 (the file's date in the cloud project folder, `v0.2.6/review-nav-branch.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: claude/bot-navigation at 17d1f460 (base c514ff4e)

**Verdict: the direction is right, but it is not ready to merge yet.** Five of the seven design must-fixes are met in the code. The two that are open are the ones behind the Afghanistan freeze and the think-time bar. The lane is already working on both, and this note says what those fixes should respect. Tests: `cargo test -p bri-sim --release --lib -- nav:: reach:: route::` gives 56 passed, 0 failed, 1 ignored, at 17d1f460 in the cloud.

## Design must-fixes against the code

| # | Must-fix | State | Evidence |
|---|---|---|---|
| 1 | Moves cost time | ✅ | Every edge is in seconds (nav.rs `Search::step` ~1600-1660): walk, crouched walk, swim, hop, and fall from `Motion`. The leap costs `stop + seconds`, floored at the walk so the estimate stays a lower bound. |
| 2 | Standing-takeoff leap table, negative rises, executor-replayed controls | ✅ | `reach::leap` (reach.rs:449) flies `crate::route::leap` (route.rs:508), the same controller the bot runs. It works from a standstill over a void and covers rises above and below. One controller is shared by measuring and playing, which is what we asked for. |
| 3 | Bounded branching and measurement | ✅ | 16 headings, `ARCS_PER_TICK` 24 with a cache (nav.rs:55), `MEASURED_KEPT` 64 in an Arc (reach.rs:86), and `MAX_EXPANSIONS` / `MAX_JET_TESTS`. |
| 4 | Cap the crawl ceiling | ✅ | The crawl search is removed and the ceiling probe is capped at `body.jump + 0.2` in `sample`. |
| 5 | Settled spot reaches every `feet()` consumer, with a bounded settle | ✅ | `Node` carries `ox/oz` and `Node::feet()` (nav.rs:497) is the only cell-to-world conversion outside `sample`. `settle_box` ends because each push is at least `SETTLE_GAP` and the spend per axis is capped at half a cell (nav.rs:319-366). `place` keeps the floor height (nav.rs:432). |
| 6 | Root-cause the drop stall | ✅ | 52a9799 never routes through a floor, and the drop sweep is done at the takeoff height (nav.rs ~905-920). |
| 7 | Drop limit from the host fall-damage rule | ⏳ v0.2.7 | `drop: 4.0` is still a literal (nav.rs `Body::of`). This was allowed to slip. |

## Must-fix before merge

1. **🔴 Jumps from a crouched takeoff use a height the bot cannot reach. This is probably the Afghanistan cubby freeze (inferred; I have no match replay).**
   - `edge()` (nav.rs:1005) only looks at the landing (`to.low` gives `crawl_jump`). It never looks at whether the body is crouched where it takes off.
   - `crawl_jump` comes from `reach::lands_on(.., crawl: true)` (reach.rs:512). That measures a *running* jump, standing under open sky, into a window.
   - A bot crouched in a cubby has neither the run-up nor the headroom. Yet `neighbours` plans a 1.0 rise there as a jump: its takeoff sweep (~925) uses the crouched box, which fits under the roof, so the plan passes.
   - The fix must be a third measured number on `Reach`: the highest ledge reached jumping from a crouched standstill under a crawlspace roof. Measure it with the same `stand` / `step` harness and the walk leg's own controls. `edge` then takes the takeoff's crouch into account (`!ground.clear(body, node.feet(), false)`, which `neighbours` already computes for drops).
   - Not acceptable: a special case for 1.0, or for the map.
   - Add a test in nav.rs: a crawlspace with a step inside it that the crouched standstill cannot climb gets routed around, or returns a partial path.

2. **🔴 Failure memory missed a 50-98 s refreeze. Name the lapse from the readout before patching it.**
   - Failure memory exists to catch exactly this case. The candidates, all read from the code and none confirmed:
   - (a) `avoid_walk(feet, ..)` (bots.rs:3377) records `feet.y` at the moment the window closes. After the stall hop, a bot in the air records a height up to a jump above its floor. `avoided()` (nav.rs:742) then matches only within `crouch_height * 0.5`, so the failure is filed against a floor that doesn't exist and never matches. Fix: record the node the leg started from (or the planned `from` waypoint), never the live feet.
   - (b) The stall judge runs only while `walking`. `at_work` (within `body.width + 1.0` of an `Objective` / `Interact` / `Arm` point, bots.rs ~3246) switches it off. A cubby next to an objective point would never be judged.
   - (c) After `MAX_REPLANS`, `objective.fail` clears the goal for `next_wander + 120` ticks. If the objective is picked again and the avoid entry missed under (a), the same plan comes back.
   - The lane should look at the frozen bot's `acted.stalls`, `acted.replans` and `held_back` over the freeze and say which of these it was. Then fix that one lapse in the shared path. Not acceptable: a cubby-specific timer.

3. **🔴 Think time must be under the 2000 µs bar on Afghanistan before merge.** It is 2.5 ms now. The budgets are already named constants. If they are lowered, the commit should say what got slower, for example the share of searches that finish in one tick. Not acceptable: raising the bar, or loosening a test.

4. **🔄 Name the step slack (small; it is in changed code).**
   - `body.step + 0.05` appears in at least five places: `edge`, `link`, `entered`, `jet_edge`, and the `level` closure in `leaps`.
   - `nearest` uses `body.step + 0.5`, and the ceiling probe uses `- 0.1` / `+ 0.2`.
   - Each one should be a named constant saying what it allows for. They must also stay one value, so the walk test and the leap "off an edge" test can never disagree about what counts as level.

## Max's question: can new move kinds (brick breaking, rocket jumps) be added without touching the search?

**Not in this shape, but it is one small step away.**

- Today the search has three hard-wired generators: `Nav::neighbours` (walk, jump, swim, portal), `Search::leaps`, and `Search::jet_edge`. `Search::step` calls each one by name, and each returns a different shape:
  - `neighbours` returns `Step` tuples, whose cost `step` then works out itself;
  - `leaps` returns `(Node, seconds)`;
  - `jet_edge` returns a ready-made `Came` whose cost is `jets.cost` (not plain seconds).
- `Mode` is a closed enum, and the route executor matches on it.
- Adding a rocket jump today touches `Mode`, `Search::step` (a new call and its own cost and bound logic), and the executor in bots.rs / route.rs.
- The shape that holds is a list of move providers. Each provider takes `(nav, ground, body, node)` and returns edges of the form `{to, seconds, mode}`. Each also has a controller in route.rs, and a measurement in reach.rs that flies that same controller, as leaps already do. The search then only relaxes edges and applies one bound check. That also fixes a small inconsistency today: the leap bound ignores portal exits (`links`), but the walk bound counts them.
- Brick breaking also needs the edge to carry a world precondition ("this brick is breakable by what I hold"). That should come from the rules and provider semantics, never from brick names, per AGENTS.md.
- **Recommendation:** leave this for v0.2.7. No player hits it, and the current three are coherent. Do the provider refactor as the first step of the v0.2.7 move work, before any fourth kind lands. Don't bolt a fourth kind onto `step`.

## Fine as is

- Failure memory keyed by move and floor (f1699c6, 17d1f46) is the right granularity. It is bounded by `MAX_AVOIDED` with expiry.
- "Leap only off an edge" (6913af6) keeps leaps from replacing walks on level ground.
- Every leap arc is swept (`sweep_arc`) and cached per node pair.
- Close Quarters went from 12% to 7.2% stuck, as the lane measured. That is a real gain.

## v0.2.7

- Drop limit from the host fall-damage rule (design must-fix 7).
- Move-provider seam (above).
- `leg_push_speed`'s `6.0` (route.rs:557) is an existing literal and should come from the jet tuning.

---
**2026-10-08 update: Max waived the Afghanistan think-time target.** In Max's words: "who cares about 2.1ms per tick". The branch is at about 2.1 ms/tick on Afghanistan. Must-fix 3 now only holds the release bar: `bot_think_time_16` must stay under 2000 µs (495 µs reported). Must-fixes 1 (measured crouched-takeoff jump), 2 (name and fix the failure-memory lapse) and 4 (name the step slack) still stand for the re-review.

---

## Final check: 03a19527 (merge at Max's call). ✅ Nothing a player would hit.

Tests in the cloud: `cargo test --release -p bri-sim --lib -- nav:: reach:: route:: work_tests` gives 59 passed, 0 failed, 1 ignored.

| Round-2 must-fix | At 03a19527 |
|---|---|
| 1. Measured crouched step | ✅ `Reach::crawl_step` is measured with the same motor harness, bisected and capped at the step. `edge()` uses it for low floors (nav.rs ~1122). For the standard body it measures **1.0, equal to the step** (test print), so for the default player it changes nothing. My crouched-step theory was not the cause of the freeze. A crouched takeoff into a standing-height jump is still rejected by the takeoff sweep in `neighbours` (the roof blocks the standing box), so nothing plans a jump under a roof. |
| 2. Failure-memory lapse named | ✅ Cause (b) from my list: `at_work` reset the progress judge every tick near an Arm goal. The new rule judges stalls while a waypoint is still ahead (bots.rs `at_work`). Lapse (a) is also closed: failed walks are recorded from `Brain::footing`, the last grounded spot. As the lane says, the unit test pins the new rule but doesn't replay the freeze. That's acceptable at Max's call. The Afghanistan rerun had no freeze, and stuck time fell from 14% to 6.3%. |
| 3. Think time | Afghanistan waived by Max (about 2.1 ms). The 16-bot bar holds at 495 µs. |
| 4. Step slack named | ✅ `STEP_SLACK` is used everywhere `+ 0.05` was. The other tolerances (`+0.5` in `nearest`, the ceiling `-0.1/+0.2`) are still literals. Not a player issue; v0.2.7. |

Leap cache (217d2b21): `Nav::leapt` is cleared by `clear()` and dropped by `invalidate()` within the farthest leap plus the apex of a change, so a built or broken brick can't leave a stale leap. Failed moves are still filtered after the cache, per tick, so failure memory still applies. The "no edge round it" pre-check uses the eight neighbours. A 16-heading leap from a spot pushed off-centre can first step into a cell two away, which the old per-heading test also allowed. At worst a leap is missed; nothing wrong gets planned. v0.2.7, with the move-provider seam.
