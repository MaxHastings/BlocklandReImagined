# Collision surfaces from `col_` meshes (2026-10-19)

The glTF map importer gave `col_` primitives no material ("collision is never drawn"), so a `bri_surface` on a `col_`
mesh's material never reached the bundle's `surfaces` and a hit on that collider had no kind, although
docs/modding/maps.md says to put `bri_surface` on the `col_` meshes' materials. A collision primitive now keeps its
material when the glTF material names a `bri_surface`; any other collision primitive stays material-less as before.

Test: `a_col_mesh_keeps_the_surface_its_material_names` (importer unit test: the collision detail's primitives carry the
material and the bundle's `surfaces` names it). Played: nothing, headless only.

## Per-collider query masks

A `col_` node's `bri_blocks` (some of `move`, `ray`, `sweep`, `shot`; docs/modding/maps.md) becomes `Detail::blocks`: the importer
makes one collision detail per distinct mask, `bri_physics::content` builds each as a collider whose collision-group
memberships are its mask's bits (`bri_physics::blocks`), and queries pass `query_groups(kind)`. Moved: player motion
(`moves_against`) asks for `move`. Weapons: line of sight asks for `ray`, a bullet for `shot`, a projectile in flight and an item
sweep for `sweep`. Unmasked colliders and every caller that passes no kind meet everything, as before. Not yet moved: navigation
probes, the client's building traces, vehicle and prediction casts (they still meet every map collider).
Tests: `col_meshes_keep_the_query_types_they_block` (importer), `blocks::tests` (groups), `collider_blocks` (a walker meets only walls that list `move`).
