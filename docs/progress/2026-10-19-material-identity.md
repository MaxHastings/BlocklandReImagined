# Material identity on colliders (2026-10-19)

A glTF material's `extras.bri_material` becomes an entry of the map bundle's `identities` (material name to identity), and
`MapSurfaces` keeps it per collider triangle beside the surface kind (`identities`, `identity_triangles`, `identity(collider, triangle)`),
so a trace hit reports which material it struck. A `col_` primitive keeps its material when it names a `bri_surface` or a `bri_material`.
Test: `a_collider_triangle_reports_the_identity_of_its_material`. Played: nothing, headless only.
