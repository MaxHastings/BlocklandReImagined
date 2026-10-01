# 2026-10-01 Bedroom Dark: the lamp's ceiling ring goes out with the bulb

Max's report (main d6152ab58): on Bedroom Dark, after breaking the bulb,
the ceiling still showed the lamp's ribbed ring and square pattern, and the
wall kept bright strips.

- Evidence: the Gate's `bulbdark1-*` probe run (`synthetic:bedroom:1`,
  `BRI_MAP=bedroomdark`, `BRI_BREAK=1`) and `bulbdark1-pixels-log.txt`.
  Bedroom Dark draws the same `bedroom.dif` and fits the same 24 lights as
  Bedroom; only the mission's sun and ambient are black, so nothing hides
  what stays lit. On the ring (`bedroom.dif/224/84`, texel 190:12400) the
  lightmap holds 180 where the lamp's light 0, which the rays see, gives
  119. Its share capped at 1, and the remaining 61 went to every hidden
  light in reach, two of which the ceiling faces away from: 2, 3, 5, 8 and
  9 each took 0.34. Lights 2, 5 and 8 are not the bulb's, so 31 of 178
  levels stayed after the break. The leftover was 0, so this was the split,
  not the compiler's lightmap.
- Fix (bake format 15): away from a patch edge, a texel brighter than the
  lights its rays see give hands the excess to those lights first, up to
  `SHARE_MAX` (about 2x) of each. Patch edges (where neighbours see a
  hidden light) and surfaces with no seen light (the shade's outside) keep
  their rules. Dynamic sheets store a share as `SHARE_ONE` = 128 per
  unit, so 1.0 stays exact and a share can go up to 1.99.
  `DynamicSheet::share` decodes it, and the shader's `SHARE_SCALE` decodes
  it the same way (a test checks they match).
- Guard: `a_lamps_bright_spot_goes_dark_with_it` (a ceiling with a spot at
  1.6x the fitted lamp light, and a room light behind it). It fails on
  f0cfe92de: 52 levels stay with the lamp out. It passes now, and the sheet
  still reproduces the authored light within 2 levels. All bri-render tests
  pass (`cargo test -p bri-render`), as does
  `bri-client --lib a_broken_bulb`. Clippy is clean.
