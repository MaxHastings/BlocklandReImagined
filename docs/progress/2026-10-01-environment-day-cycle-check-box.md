# 2026-10-01 Environment window: the day/night cycle could not be turned on

Max, playing v0.1.11-test-d6152ab58: "environment controls work somewhat i
can't seem to use day night cycle tho".

Cause (`crates/ui/src/screens/environment.rs`):
- A check box flips its value and sends `Changed`, then `Click`. The window
  answered every `Changed` with a refresh, which put the model's value
  (still off) back on the box. The `Click` handler then read the box as
  off and set the cycle off. So Day Cycle (Simple and Advanced) and
  Vignette Multiply never turned on, and Day Length / Time of Day stayed
  greyed out.
- Fix: both check boxes are read on `Changed`, before the refresh.

A second fault behind it (`crates/ui/src/models/environment.rs`):
- The host stamps a cycle with its own tick, so after Apply the window's
  draft (stamped with the client's tick) never matched the host's. The
  window kept saying "Not applied yet", and the next Apply, for any other
  setting, restarted the cycle from the draft's time of day.
- Fix: when the host's cycle has the draft's day length and time of day,
  the draft takes the host's anchor tick. Picking a new time of day or
  length is still a new cycle.

Nothing else changed: the host, network and renderer already ran the
cycle (`atmosphere::resolve` on the replicated tick, so no bandwidth after
it starts; Classic and Unified both relight through `relit()` in
`scene.wgsl`). DayCycle files stay replaced by the built-in cycle, as the
2026-09-30 Environment entry decided.

Tests (both fail on the old code):
- `bri-ui --test admin_screens`
  `the_environment_window_applies_a_draft_through_the_host`: the Simple and
  Advanced Day Cycle boxes turn the cycle on and off, Time of Day comes
  alive, Apply sends the cycle, Vignette Multiply turns on.
- `bri-ui` lib `a_cycle_the_host_runs_shows_as_applied_and_keeps_turning`.

The first push (34430ac9) failed its own test on the Gate: Vignette
Multiply is the Advanced page's last row, below the scroll's fold
(y 514 on a 480-high canvas), so the click missed. This container's run
had stopped at the lib's GPU-only failures and never reached
`admin_screens`; `--no-fail-fast` is used now. The test scrolls the page
to the row, asserts it is above the fold, then clicks.

`cargo test -p bri-ui --no-fail-fast` passes apart from six offscreen
tests that need a GPU adapter this container lacks; `cargo clippy -p bri-ui --all-targets
-D warnings` is clean.

Max's check: Admin Menu, Environment, tick "Day and night cycle", set Day
Length (try 1m), Apply. The sun, light, fog and sky go through a whole day
in that time, for everyone in the server.
