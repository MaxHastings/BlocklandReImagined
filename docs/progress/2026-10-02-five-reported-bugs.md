# 2026-10-02 Five reported bugs

Max reported fullbright grass in Bedroom and water at a distance in Slopes
at night, an unusable spray can after Change Map, doors that still would
not open or play sounds, and startup failing on Bot_Shark's hole brick.
He requested all five fixes on main and authorized pushing after verification.

Changes:

- Foliage now receives the live environment's outdoor illumination, relative
  to the map's authored daylight. Authored colour gradients, sway, animated
  luminance and alpha are retained. The same uniform is used in the player's
  view, mirrors and environment probes, including the first frame after a
  renderer rebuild. Water applies that daylight ratio to surface, shore,
  authored reflection and specular before fog, in both rendering paths.
- Replacing the asynchronously prepared map's building controller invalidates
  its world-sync cursor. Its first sync cannot be skipped just because the
  network replica has not changed since MapChanged: the new controller's
  palette length was zero. The HUD also clamps the swatch and remembered
  paint index when its colorset becomes smaller. Host and guest regression
  tests equip a spray can after the normal Change Map/loading sequence.
- Definition swaps replace occupancy bounds as well as geometry and collision.
  Actual closed doors are 4x1 studs, while open variants are 4x7 or 4x9;
  the old same-size guard silently rejected every click. Swap sounds are
  optional native bindings, emitted at the brick only after a successful swap
  and the cooldown. The target's noBrickSounds is respected. Import records
  the binding; the native catalog adapter also interprets the retained door
  fields in shipped catalogs. The simulation swap operation has no door policy.
- Enabled Add-Ons' native geometry assets can supply another Add-On's brick
  without declaring a placeable donor brick. Bot_Hole already ships 8xspawn
  as an asset, while Bot_Shark refers to its original brickFile path. That
  reference now resolves to the native asset with its authored box collision.
  Geometry assets stay out of the brick menu. Host, dedicated server and the
  client's background map preparation use the same geometry collection.
  This uses native content only; no Torque reader enters the runtime graph.
  The generated-content multiplayer run also caught an old importer problem:
  identical BLBs at different paths overwrote one native file's embedded id.
  Geometry uses the content index's authored identity; new imports hash native
  bytes (including identity) for filenames so each name remains independent.

Source evidence: the pinned Brick_Doors archive from
[Blockland Archivers](https://bl.kenko.dev/Add-ons/Retail/Brick/) has SHA256
`0df1cdd215dbe4dca0f57924ed752204b58fab106d0fc36d72dbc1fee1c19de2`, exactly
the bundle's pinned copy. Its Support_Doors dependency's
`setDoorDataBlock` plays BrickChangeSound on both transitions unless the
new datablock has noBrickSounds. The installed v20 bank's native
`v20/sound/brickchange` plays `base/data/sound/clickchange.wav`.
Downloaded source archives are ignored research artifacts, never committed.

Verification on the Mac, offscreen/headless only:

- `cargo check -p bri-client -p bri-addon-import --all-targets --locked` passed.
- Targeted clippy for client, importer, foliage and renderer, all targets,
  `--locked -- -D warnings`, passed.
- `bri-render --test unified_lighting water_follows`: passed for near and
  distant water, both plain and depth-mapped, with reflection and specular.
- `bri-foliage --test gpu offscreen_gpu_sway_depth_and_upload_bounds_synthetic`:
  passed, including a day/night pixel brightness comparison.
- `bri-sim --test special_bricks a_click_swaps`: passed, with different closed
  and open footprints, opening and closing sound cues and cooldown.
- `bri-sim --lib an_add_on_brick_reuses`: passed, now including a donor that
  provides only native geometry, with no extra menu definition.
- `bri-ui --lib a_smaller_colorset`: passed.
- `bri-addon-import --test installed a_door_swaps`: passed, including the
  generated native sound binding. The complete installed-import test target
  also covers distinct native files for identical BLBs with different names.
- `bri-client --test multiplayer two_clients_see_names_minigames_trust_and_follow_a_map_change`:
  synthetic and generated-content host/guest flows passed (2 tests).
  Negative control: temporarily removing
  world-sync invalidation makes the same test fail waiting for both cans to
  equip after the map change (exit 101); restored the fix immediately.
- `bri-client --test default_add_ons bundled_shared_geometry_hosts_and_real_doors_change_footprint -- --ignored`:
  passed against the actual bundled Bot_Hole, Bot_Shark and Brick_Doors in an
  isolated content copy. Shark enabled through Add-Ons starts a headless host;
  every actual closed door swaps to a wider open definition and back, with
  the native sound binding present. Base content hard links are read only.

Tests/builds use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` and leave
CARGO_TARGET_DIR unset. Logs are in ignored `artifacts/five-fixes-*.log`.
The first broader generated-content multiplayer run caught the duplicate-BLB
identity problem above and failed with Geometry asset identity mismatch;
after the loader/import fix, both variants passed. An initial test invocation
supplied two Cargo name filters and was rejected;
a later invocation used the wrong synthetic suffix and ran zero tests.
Both were corrected; the passing results above ran actual named tests.

The first full gate on `a87daa8e` passed history, workspace build, workspace
clippy and the content check (14 maps, 963 definitions, 35 pictures), then
stopped at older harness assumptions. Failures used retired pack folders
(UI 001/003, effects runtime 001, weapon debris 001, avatar rig 001,
map bundle 006, spray packs 009/008); the fresh checkout keeps the current
packs only. Tests now read the current pack revisions. The runtime effects
inventory assertion includes the current pack's 132 particles and 133 emitters.
The content menu assertion still checks exactly 166 base bricks, separately
from enabled Add-On entries. Foliage evidence writers create their output
folder before writing. No tests were skipped or marked known failures.

The admission stress campaign failed binding 127.0.0.2 on macOS (os error
49). It now uses the Mac's dual-stack wildcard host with idle peers on IPv4
loopback and the real player on IPv6 loopback, retaining distinct source
addresses without configuring system interfaces. Other platforms retain the
existing 127.0.0.2 scenario. The replay also requires at least one successful
idle connection. Targeted `bri-net --test stress_campaign
idle_handshakes_from_one_address_cannot_lock_out_real_players` passed (1 test).
The gate also reported several parallel GPU icon tests as flaky: each passed
when retried alone. The complete log is
`../.bri-gate/logs/a87daa8eadc4.log`.

The second full gate on `c724a433` passed build, clippy, content validation
and all but one of 317 test binaries (419 seconds). The bundled client
integration test repeated a Grapple Rope assertion failure alone: travelled
3.1678 tied and 3.7958 free. The ratio assumed a rope with no slack. The
native policy is a fixed-length leash, not a winch; the original port and
motor were unchanged. The integration test now waits for the replicated
attachment, checks its fixed anchor and length, checks the grip remains
within that length while held, waits for release and landing, then verifies
walking carries the grip beyond the old rope's reach. It also verifies
that the replay actually attached, which the distance ratio could miss.
The full hosted Add-On scenario passed after this repair (1 test, 77 seconds),
including its subsequent Fill Can, wrench and hole-bot interactions.
Logs: `../.bri-gate/logs/c724a4338847.log`, its `-retry.log`, and
`artifacts/five-fixes-bundled-integration.log`.

Max explicitly included useful refactoring, technical debt and other
integration issues encountered in this work. The shared native geometry
resolver, definition swap bookkeeping, destination sound bindings and
identity-preserving importer are the production repairs; these harness
repairs keep full verification useful on a fresh Mac checkout.

Next: rerun full `python3 tools/gate.py --push` with the integration repair;
Max verifies visual night lighting and door feel in his interactive playtest.
These fixes do not mark the full alpha contract complete or publish a release.
