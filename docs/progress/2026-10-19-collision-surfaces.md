# Collision surfaces from `col_` meshes (2026-10-19)

The glTF map importer gave `col_` primitives no material ("collision is never drawn"), so a `bri_surface` on a `col_`
mesh's material never reached the bundle's `surfaces` and a hit on that collider had no kind, although
docs/modding/maps.md says to put `bri_surface` on the `col_` meshes' materials. A collision primitive now keeps its
material when the glTF material names a `bri_surface`; any other collision primitive stays material-less as before.

Test: `a_col_mesh_keeps_the_surface_its_material_names` (importer unit test: the collision detail's primitives carry the
material and the bundle's `surfaces` names it). Played: nothing, headless only.
