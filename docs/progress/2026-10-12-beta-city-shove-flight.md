# 2026-10-12 Beta City stutter: the shove flight

Beta City 16's gun battle stuttered (step p95 26 ms, max 43 ms, 313 ticks
over 5 ms on Maxwell's PC). Two timing runs of the diagnostics branch
`claude/v0210-step-breakdown` on his PC (v0.2.10 main merged in, evidence
`ad54ce4343285c6b`) traced it:

- 9094734: `turn: hand` 3.57 ms a tick and `turn: stand` 1.74 ms; on the
  slowest tick (721) hand 33.3 ms and stand 9.4 ms. Ray parts were tiny.
- 4b4da5e, every part of the hand chooser timed: `choose: shove` 5.13 ms a
  tick over 1283 flights (about 4 ms each), against 5.38 ms for the whole
  slot loop. Everything else in the chooser together is under 0.3 ms.

The shove (`reach::shove_landing`) flies the real player motor against a copy
of the fixed collision around the target, and charged one intercept solve per
motor tick, up to `PATH_TICKS` (256, two seconds of flight). A motor tick
against Beta City's brick chunks costs about a thousand intercept segments.
A local brick-city fixture (5376 planted 2x4s, never committed) showed the
motor steps are 75-85% of a flight and grow with the chunk sub-shapes
copied.

Changes:

- `combat::SHOVE_TICKS` = 60: a push is flown for at most half a second
  of motor ticks. The reach box the flight copies shrinks with it.
- Past that, `shove_landing` finishes a fall above something in closed form,
  `v² = vy² + 2·g·h` to what is straight below. The old fallback was only the
  speed when the allowance ran out, so a long fall now reads as at least as
  hard as before.
- Weighing places to stand (`spots`) reads the push the hand flew this
  turn (`State::for_place`) instead of flying one from every place.

Next: a PC timing run of this branch on Beta City 16 (same command as
above) to compare with 4b4da5e.
