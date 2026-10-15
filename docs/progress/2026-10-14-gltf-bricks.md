# 2026-10-14 Bricks from Blender models (glTF)

Maxwell approved "all three" from the modder-gaps check; this is the bricks
one. `bri-import-addon gltf MODEL --kind brick` turns a glTF into a brick
Add-On, reusing the glTF reader the map Add-Ons use (PR #149).

The output is the existing native brick format, with no new brick type:
`assets/brick-catalog/<id>.brick.json` (quads, footprint, attachment grid,
collision boxes), a one-entry `stock-catalog.json`, `catalog-audit.json`
and `native-collisions.json`, the files Import Add-On writes for v20 bricks.
`package.json` provides kind `bricks`, so the engine merges it like any
Add-On's bricks: brick menu, saves, replication to joining players, events
and `onActivate` all come for free.

Decisions (limits for now):
- Grid size is the model's bounds rounded up to studs and plates, with the
  model centred in it. Studs over the whole top, sockets under the whole
  bottom, as a basic brick.
- Paint colour only: every triangle is a `Print` surface with no print, the
  plain painted face. Textures, material colours and animation are not used
  (the report says so).
- Collision: one box per `col_` mesh's bounds, else the whole grid box.
- Menu: Special > Models; no icon yet (the menu shows none).

Evidence: `crates/addon-import/tests/gltf_brick.rs` builds a made-up 1 x 1 x 1
unit table (top and leg), imports it with `--kind brick` and loads it with
`Definitions::load`: a 2 x 2 brick 5 plates tall, 24 triangles, one 1 x 1 x 1
collision box. Docs: `docs/modding/gltf-models.md#bricks`.

Next: a menu icon rendered from the model; textures once bricks can carry
their own images.
