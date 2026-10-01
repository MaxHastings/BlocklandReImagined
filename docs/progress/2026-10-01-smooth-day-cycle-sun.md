# 2026-10-01 Day/night cycle: the sun turns every frame

Max: "why is the day night cycle tick based rather than smooth? shadows
look like they are a damn clock ticking."

Cause: `atmosphere::DayCycle::sun_time_at` held the sun for a step of
1/1440 of a day or one second, whichever was longer, so a 300 s day moved
it 1.2 degrees once a second (6 degrees for a 60 s day). The step was
there so a turning sun would not redraw the kept brick shadow layers
every frame. The client also resolved the sky at the last received tick,
not its smooth estimate.

Fix:
- `resolve` and `DayCycle::time_at` take a fractional tick; the client
  passes `motion.server_tick()` (its per-frame server clock), so the sun,
  light, fog and sky move every frame. No new network traffic.
- `sun_time_at` and `SUN_STEP_OF_DAY` are gone.
- `kept_shadows::plan`: while the sun differs from last frame's, every
  cascade draws directly and no layer is redrawn (a layer could not be
  used twice). Once the sun holds still, the layers come back as before.
  Cost: only with a cycle running and Brick Shadows on (off by default),
  the frame is the direct-draw frame from before kept layers. Nothing
  changes without a cycle, so the perf headline is untouched.

Test: `bri-content` `the_sun_turns_every_frame` (60 fps frames in one
second of a 300 s day: each frame's sun is new, 0.005 to 0.05 degrees
past the last). On the old code consecutive frames in a second share one
sun, so it fails. Clippy is clean on content, render, ui and
package-runtime. bri-client was not built here (no ALSA in this
container); its change is the one `resolve` call.
