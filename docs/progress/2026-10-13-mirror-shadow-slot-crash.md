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
