# 2026-10-13 Environment window: one model under the same window

Max asked for v0.3.1 to "unfuck" the Environment window: after the Sky,
Ground and Water pickers landed, its controls contradicted each other (a
picked sky against the Look list, Night against the day cycle). A docked,
redesigned panel was rendered and shown; Max preferred the original window
and asked for "a little cleanup and harmonized so things don't conflict
under the hood". The window's layout and controls are unchanged; what each
control does is not.

## What conflicted

- A Look (Clear Day, Golden Hour, Sunset, Night...) and a picked Steam sky
  both wrote the same light and fog values, so whichever was touched last
  won. Picking a sky cleared the Look's highlight; Map Default kept the
  sky but threw away its light.
- A Steam sky switched the day/night cycle on.
- Night set blue light colours, so with the cycle on it gave a dark noon.
- Time of Day did nothing unless the cycle ran.

## Decisions

- **Layers, not last-write-wins.** A picked sky's light and fog are the
  base, as the map's own are. Another sky swaps the base and keeps values
  set by hand in Advanced, the time of day and a weather a Look set. A sky
  never touches the day cycle.
- **Looks are a time of day, a weather, or both.** Clear Day = noon and
  clear air; Golden Hour, Sunset and Night = a time of day (moving a
  running day there); Overcast and Thick Fog = a weather. Sunset then
  Thick Fog is a foggy sunset. "Map Default" is now "Default": the sky's
  (or map's) own light, weather and sun, keeping a running day running.
  The highlight follows what the settings are, so it survives a sky pick.
- **Time of Day always works.** With the cycle off it holds the sun there;
  the cycle's check box lets time pass, and turning it off holds the sun
  where it is. Night is the day cycle's own night (moonlight, the night
  tint), the same with the cycle on or off.
- **Advanced** gains a Cloud Cover row for the weather's cloud.
- **Engine, two small mechanisms** (wire note
  `crates/net/protocol-changes/environment-time-and-weather.md`):
  `DayCycle::paused` holds the sun at a time of day, and
  `Settings::overcast` (0 to 1) dims the sun, spreads its light into the
  ambient and greys fog, sky and flare before the day cycle shades them.
  Script keys `time_passes` and `overcast`; `time_of_day` alone now holds
  the sun instead of failing (`docs/modding/rules.md`).
  `atmosphere::presets()` is gone; the Looks live in the UI model.
- **PR #31 (Enhanced Sky)** is still open and conflicts with main; it
  would fit as one more Sky choice rather than its own checkbox.

## Evidence

- `cargo test -p bri-content --lib atmosphere`, `-p bri-ui --lib
  environment` (Look layering, a sky keeping the Look and time, Default,
  held time), `-p bri-ui --test admin_screens`, `-p bri-sim --test
  script_api` (`overcast`, `time_passes`, held time), `-p
  bri-package-runtime`.
- An offscreen render with the real v20 UI pack: the unchanged window
  with Sunset still picked over a Steam sky.

## For Max

As admin on Slate: pick Sunset, then a Steam sky (Sunset stays picked and
the sun stays low), then Thick Fog (a foggy sunset), then Default. Turn
the day cycle on, pick Night: it is night, and it keeps turning.
