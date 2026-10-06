# 2026-10-05 Duplicator selection takes the brick keys before a ghost

Max, v0.2.4: with a ghost brick already out, selecting a build with the
duplicator highlighted it, but the numpad still moved the old ghost brick
instead of the selection.

Cause: `Building::ui_action` (`crates/client/src/building.rs`) sent the
shift, rotate and plant keys to a copy in hand first, then to the free ghost,
and only to the held image's own key commands (`ImageCommands::shift`,
`rotate`, `plant`) when no ghost was out. The New Duplicator port takes those
keys for its selection (it becomes a copy only once a brick key takes it up),
so any ghost left out stole them.

Fix, in the shared path rather than a duplicator check: a held image that
declares a brick key gets it before a ghost left out, as v20 Add-Ons that
packaged `serverCmdShiftBrick`/`RotateBrick`/`PlantBrick` did. Order is now
copy in hand, then held image, then ghost. The ghost stays where it was and
answers the keys again once the image is put away. No wire change.

Test: `building::tests::a_held_image_takes_the_brick_keys_before_a_ghost_left_out`.
Command: `cargo test -p bri-client --lib building::`.
