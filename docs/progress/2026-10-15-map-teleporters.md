# Teleporters on imported maps

Maps can now carry teleporter pairs, as a general capability for any
imported map (asked for while bringing over a Halo level, but nothing in
the engine knows about Halo).

## Authoring

In Blender, Empties named `teleport_in...` are entrances and
`teleport_out...` are exits. A `bri_channel` custom property pairs them and
`bri_radius` on an entrance sets how near its middle a player must step (1
unit by default, scaled with the model). An exit faces along its Empty's
Blender +Y. The glTF importer writes them as `teleporter` scene nodes with
`role`, `channel` and `radius` properties, lists them in the import report,
and warns about an entrance with no exit on its channel.
`docs/modding/maps.md` documents this.

## Runtime

`Scene::teleporters` reads and validates the nodes (at most 256, radius up
to 100 units, channel up to 64 bytes). The native map load carries them
into the host session (`Session::set_teleporters`). Each tick after
touches, a live player whose feet enter an entrance they were not already
standing in is moved to the next exit on that channel, turned to the
exit's facing, with their velocity turned by the same amount. Inputs
queued before the move are dropped so prediction does not pull them back.
A player who lands inside another entrance must step out of it before it
fires, which stops ping-ponging between paired pads.

## Checks

Unit tests cover reading role, channel, radius and facing from nodes, the
importer writing them from a glTF room, and the session's entrance test and
exit rotation.
