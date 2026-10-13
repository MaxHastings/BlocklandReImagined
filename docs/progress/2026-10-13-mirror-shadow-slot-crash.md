# 2026-10-13 Third mirror plane no longer crashes the client

Max's v0.2.11 crash (`crash-20261013-020817.txt`, map_slate_desert, RTX 4070
SUPER on DX12) was a wgpu validation panic in `Queue::write_buffer`: "Copy at
offset 51456 for 80 bytes would end up overrunning the bounds of the
Destination buffer of size 51456". The session log shows the `mirrors` pass
had just started appearing in the GPU timings.

## Cause

The sun shadow caster buffer (`ShadowMaps::caster`, 256-byte slots of 80
bytes each) is sized `FAR_SLOT + 1` slots. `FAR_SLOT` counted the player's
cascades and map layers (`MAX_CASCADES * 2`) but not the kept brick layers
that `kept_offset` places after the map light cube faces, while
`view_slot` (where each mirror or window plane's own cascades go) does
count them (`MAX_CASCADES * 3`). So the buffer was one `MAX_CASCADES` block
short: 201 slots, 51456 bytes. With three live reflection planes, the third
plane's map layers land at slots 200..203: cascade 0 silently shared the far
layer's slot, and cascade 1 wrote slot 201, offset 51456, past the end. That
is exactly the reported numbers. One or two planes never reached it, which
is why mirrors worked until a third plane showed up in view.

The slip came in with the far map layer (c6048f1).

## Fix

`crates/render/src/shadow.rs`: one `VIEW_SLOTS` constant now gives the first
plane view's slot, and both `view_slot` and `FAR_SLOT` derive from it, so the
buffer grows to 205 slots and the far layer has its own slot again.

## Evidence

- New unit test `shadow::tests::caster_slots_are_distinct_and_inside_the_buffer`
  enumerates every caster, map, lamp, cube, kept, plane-view and far slot and
  checks each is inside the buffer, none is shared, and none is unused.
- With the old `FAR_SLOT` restored it fails with
  `slot at 51456 overruns the 51456-byte caster buffer`, the crash's numbers.
- `cargo fmt -p bri-render -- --check`, `cargo clippy -p bri-render
  --all-targets -- -D warnings` and `cargo test -p bri-render --lib`
  (66 passed) pass in the Linux cloud container. The render integration
  tests that need a GPU adapter fail there identically with and without the
  change; the Windows CI and the gate run them.

## Next

Max: stand where three mirror or window planes are in view on any map (the
spot from the crash on map_slate_desert) with sun shadows on; the client
should keep running and each plane should keep its own shadows.

## Second crash: X11 cursor grab on launch

A Linux player (Pharzedia, X11) hit the same overrun during a flag match
with bots, then a crash on relaunch reported as an "x11 cursor grab error";
the third launch worked. `Runner::sync_cursor` (`crates/client/src/platform.rs`)
turned a refused `set_cursor_grab` into an error, and `resumed` ends the game
on any error from it. X11 refuses `XGrabPointer` while another client holds
the pointer (the window manager mapping or focusing the window, a launcher)
or before the window is viewable, which is a launch-time race.

A refused grab is now a warning in the log: the cursor stays free,
`grabbed` stays false, and `GrabRetry` lets the next pump (every tick) try
again after 250 ms, warning once per run of refusals. A refused release is a
warning too. Unit test:
`platform::tests::a_refused_cursor_grab_retries_after_a_pause_and_warns_once`.

`cargo clippy -p bri-client --all-targets -- -D warnings` passes;
`cargo test -p bri-client --lib`: 517 passed, and the same 6 GPU-adapter
tests that fail in the container on main fail here.
