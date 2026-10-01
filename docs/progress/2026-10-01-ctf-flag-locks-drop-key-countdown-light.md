# 2026-10-01 Capture the Flag: locked flags, Drop Tool key, countdown, light

Branch `claude/project-thread-t8k5dx` (Slayer/CTF).

## Engine seams (generic, capability-gated)

- Brick values: `set_brick_field(brick, key, v)` (`brick_events`) keeps a
  value on a brick as the calling Add-On's `key`, as a v20 script kept a
  dynamic field on a brick; `()` clears it. Every Add-On reads it with
  `brick_field(brick, key)` or `brick_field(brick, "namespace:key")`. Keys
  are 1 to 32 letters, digits or `_`; values at most 256 bytes, 32 per
  brick, 16384 in all. They go with the brick (dropped as removals reach
  the packages' dirty reader).
- `Command::DropKey` and `on_drop_key(player)`: the Drop Tool key with
  nothing in hand goes to the host (the client used to refuse it) and
  Add-Ons are asked in load order, as `serverCmdDropTool` with `currTool`
  -1 was packaged.
- `Drop::name` (`DropName { text, color }`) and `name_drop(id, text, c)`
  (`player`, only the Add-On's own `drop_item` drops): floating text over a
  dropped item in a palette colour (`setShapeName`, `setShapeNameColor`).
  The client's name tags draw it over the middle of the item's box
  (`getBoxCenter` plus the HUD's vertical offset).
- `Image::light` (`ImageLight { radius, color }`): the importer reads
  `hasLight` with `lightType = ConstantLight`, `lightRadius` (at most 100)
  and `lightColor`; other light types are noted, not drawn. The client
  starts an effects light at each mounted image that has one, tinted by
  the paint it is worn in (`SourceOptions::paint` now tints lights).
- Protocol file `ctf-flag-drop-key-and-names.md`.

## Port

- Slayer's `setTeamControlLocked` locks any brick of the game (as the
  original), kept as the brick value `locked` (the colours); capture
  points read it there.
- CTF refuses a locked flag ("This flag is locked. You cannot capture it
  yet.") and a locked stand or return point ("This <ui name> is locked.
  You cannot return flags here yet."), both pinned from the copy.
- `on_drop_key` drops a carried flag with Enable Manual Flag Dropping on;
  the pickup hint gained the original's second line about the key.
- A dropped flag names itself with the seconds left until it goes home, in
  its colour, every check (`flagRespawnTick`'s `setShapeName`).
- The flag image declares `hasLight`, `lightType`, `lightTime` and
  `lightRadius` from the copy's `server.cs` (`flagHasLight` and friends)
  with a white light colour, so the carried flag glows in the paint it is
  worn in (the original made one image per colour with `lightColor =
  %color`).

## Evidence

- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- Tests: `bri-addon-import`, `bri-package-runtime`, `bri-package`,
  `bri-weapons`, `bri-weapons-import`, `bri-net`, `bri-minigames`,
  `bri-chaos`, `bri-sim`, `bri-client` lib and `weapon_effects` pass. Two
  `bri-fx-runtime` GPU tests fail here for lack of a graphics adapter.
- New tests: `a_locked_flag_cannot_be_taken_nor_a_flag_returned_to_a_locked_stand`,
  `the_drop_tool_key_with_tools_put_away_drops_a_carried_flag`, the
  countdown in `a_dropped_flag_falls_in_its_colour_and_its_team_recovers_it`,
  `a_worn_image_with_a_light_glows_in_its_paint_and_goes_out_with_it`,
  `a_constant_image_light_is_kept_and_other_kinds_are_noted`,
  `brick_values_stay_within_their_limits_and_go_with_the_brick`, the light
  in `slayer_ports_apply_with_their_rules`, and the empty-hand drop key in
  the client's tool routing test.
- Real copies (Gamemode_Slayer 4.1.5, Gamemode_Slayer_CTF) import and
  apply: the lock messages, the Drop Tool hint and the countdown fill from
  the copy, and the flag image carries `light { radius 20, colour white }`.

## Next

Slayer's end-of-round report with CTF's Flag Pick-ups and Flag Returns
columns; bots; `.pathcam` saving.
