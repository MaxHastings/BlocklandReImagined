> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-vehicle-handling.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: measured vehicle handling, claude/v0.2.6-measured-moves-x42j1c @ 7977fc1 (range c753ead..7977fc1)

This is a read-only review of the whole range: 8 files, +1018/-179. Nothing was run here.

## Verdict: passes, on one condition. Check it against Max's real vehicles before merge.

The code is clean and does what the design note says. The old guesses are gone. The one thing nothing yet shows is that **the real vehicles measure at all**. Every test uses the synthetic test car. That matters because the PR makes a failed measurement end the drive (`interactions.rs`: `handling.is_none()` → lost affordance, bot gives up). If the stock Jeep, or any wheeled vehicle Max's saves use, comes back `None`, bots stop driving it entirely. That's a regression a player would hit.

**Condition (on the PC, one short run, no chaos suite):**
- Measure every `Family::Wheeled` definition in the real vehicle pack at scale 1 (`Handling::measure`).
- Report for each: `Some` or `None`, `top`, `tightest`, ticks spent and wall time in release.
- An `#[ignore = "needs generated content"]` test that asserts every wheeled definition measures is the natural home for this. The gate's content run then keeps checking it.
- If any comes back `None`, fix that before merge.

## What I checked

**Magic numbers: fine.**
- `DRIVE_THROTTLE` 0.8 is the old inline clamp (`clamp(0.25, 0.8)`), now named and shared by the driver and the measurement. The reason is in the doc: the measurement uses what the driver gives.
- `BUDGET_SECONDS` 120 has its reason in the comment (the stock car takes 58 s, so a chassis twice as slow still measures whole). Its test proves the cap ends a short budget early.
- `TURN_SPACING`, `STEADY`, `HOLD`/`READ_SECONDS`, `STOPPED`, `SETTLE_SECONDS`, `LOCKS`, `ALLY_ROOM` (the old `+ 2.0`) and `ARC_STEP` (π/8) are all named with a one-line reason.
- `FLOOR` 4000 is fine for any vehicle under about 60 u/s (inferred).

**Determinism: fine.**
- The measurement runs in its own physics world, with fixed controls and a tick budget, never wall time. The result depends only on the definition and the scale.
- The global cache is keyed on (pack fingerprint, id, scale bits). Whatever process or session measures first, every later lookup gets the same `Arc`.
- A cut measurement is also deterministic, because the cap counts ticks.

**Cost at join, and hitch risk: small, but it is a one-off hitch on the sim tick today.**
- `Handling::of` runs inside the drive step (`interactions.rs`), the first tick any bot drives a given definition and scale. The lock is held while it measures.
- The design note measures the stock car at "a few hundredths of a second" for 6,978 ticks. At the 120 s cap that's about twice that.
- So it's one stall of tens of milliseconds per vehicle type, the first time one is driven. That's a skipped frame or two for everyone on a listen server.
- The body `Reach` measurement already does the same (~90 ms). The warm-up-at-join item is the right fix and isn't needed for this PR, provided the real-vehicle wall times from the condition above come out similar. If a real vehicle takes far longer (hundreds of ms), say so before merge.

**Missing arc-brake chaos test: the code relies on one thing it doesn't prove.**
- The new hazard check sweeps the hull and the ally corridor along the arc the chassis is turning on. Only the existing straight-line tests cover it, and the lane says one try at an arc test passed even with the sweep switched off. So no test tells a correct arc from a mirrored one.
- I checked the sign by reading:
  - `curvature = ω_y / speed` with `Quat::from_rotation_y`. Both are right-handed about +Y, and the rotation is applied to the travel vector, so backing up turns the same way the hull yaws.
  - The clamp to `1 / tightest` is right. So the sweep goes the right way (inferred from the conventions, not run).
- A **static** test would prove it cheaply, without the controlled-turn trouble the chaos attempts hit:
  - put a stationary chassis on a floor, with a box at the point its arc reaches at that curvature;
  - assert `bot_vehicle_path_blocked` is true there, false for the same box mirrored across the nose line, and false with curvature 0.
- Not blocking, given the sign reads right. I'd ask for it with the warm-up item, or now if the lane is in this file anyway.

**Nothing dead left behind.**
- `route::Chassis`, `CORNERING`, the `braking` formula and the arrival `sqrt(2·b·d)` are all deleted, with no callers left (grep for `Chassis` only finds the unrelated `VehiclePart::Chassis`).
- `bot_vehicle_clear` is now a thin wrapper over `bot_hull_clear`. It's still used elsewhere, so that's fine.

## Smaller notes (v0.2.7, not blocking)

- **No standstill margin.** The old stop distance was `0.3·v + v²/2b + 1`. The new one is the measured distance only, so at near-zero speed the hull check looks about 0 ahead (and with zero length there are no chords at all).
  - A driver nose-to-wall can creep into it at a crawl until the no-headway rule backs it up.
  - Allies on foot keep `ALLY_ROOM` and the nose length, so this only affects walls and hulls. It's harmless at that speed, but worth a look if Max sees bots nudging walls.
- **The reverse test's absolute (`ended > began`) is weak.** A reverse chase whose facing creeps from −0.99 to −0.98 passes.
  - Each stretch "swings the nose toward the target by a meaningful turn" would hold it better.
  - One possible form is ending at or past side-on, or turning at least a set share of the angle it started off by. That depends on what the lane's traces show for the behind-target case.
- **A wheeled vehicle with `wheeled_flight`** now also needs a successful measurement to be driven. That only matters for planes, which the plan cut.
- **`MEASURED` never drops entries** for old pack fingerprints. It's tiny; fine.
- In the turn read, `held` divides by every read tick, including ones skipped below `STOPPED`. That only matters for a chassis crawling under 0.1 u/s, which ends the run anyway.

---

# Add-on: b3b84f4 (one commit on 7977fc1)

**Condition met. Merge exactly b3b84f4.**

- `every_installed_wheeled_vehicle_measures` loads the installed vehicle pack (`content/<vehicles role>/vehicles.json`) and fails on any wheeled definition that measures empty. It's `#[ignore = "needs installed content"]`, so the PC gate's `--include-ignored` run covers it on the main checkout's content.
- `every_test_catalog_car_measures` always runs. The three test chassis take about 30-35 ms each in release, which confirms the hitch is small.

**Two notes:**
1. Libtest captures `eprintln!` on a passing test, so the gate log won't show the per-vehicle times. If Max wants the real hitch numbers, the PC thread can run it once with `-- --ignored --nocapture`. It's not needed for the merge.
2. It covers the stock vehicle pack only. Add-On vehicles live in their own packs, so they aren't covered. That's fine for this PR; it's a v0.2.7 line to widen it to every installed pack with vehicles.

The design-note wording slightly misquotes my suggestion. I suggested a **static** check: a stationary chassis plus a box on its predicted arc, calling `bot_vehicle_path_blocked` directly, rather than a car turning in place. Either works, but the static one avoids the controlled-turn problem. Not blocking.
