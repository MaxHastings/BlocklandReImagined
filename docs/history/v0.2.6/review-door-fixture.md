> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-door-fixture.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: door fixture fix, 2515cce..79c9c48 (net 2 files)

## Verdict: passes. It fixes the fixture's assumption; the test isn't loosened.

1. **It fixes the assumption, not the test.**
   - The room's walls were one `TALL` course, 3.0 high. That only kept the bot in because the old jump estimate (`rise × 0.8`, 2.88) was below 3.0. The measured jump is 3.50, so the wall was really jumpable all along.
   - `barrier_courses()` now stacks courses up to the measured stock ledge plus a margin: 3.5 + 0.5, rounded up to 2 courses, so 6.0.
   - The door column omits only the bottom course. The door is 15 plates (3.0, centred at 1.5), the same height as one course, so the course above it (3 to 6) sits flush on the door with no overlap and no gap.
   - The door opening is the same set of z positions as before: `|z| < 1.0` now against `|z| > 1.0` skipped before, on a 0.5 grid where ±1.0 never occurs.
   - The assertions in `door_room` are untouched: opened, then out past x = -1.
   - A future truer jump can't quietly open the wall again, because the height follows `Reach`.
2. **No magic numbers or content names.**
   - The course height comes from the `TALL` definition's own `height_plates × PLATE`.
   - The ledge is `Reach::of(&PlayerTuning::default())`, the same tuning the stock bot body uses.
   - `BARRIER_CLEARANCE` (0.5) is a named, documented test-fixture margin. It doesn't change the result here, since 4.0 rounds up to 2 courses either way.
   - The only ids are the existing test fixture ids (`test/brick/door`, `TALL`).

Small, optional: `barrier_courses` is a good shared helper. Any other fixture whose walls assume "taller than a jump" should use it too. Grep for `TALL` walls in the chaos tests when next in that code.
