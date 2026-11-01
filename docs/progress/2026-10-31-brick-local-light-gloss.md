# Brick gloss under ordinary local lights

The new explicit brick gloss responded to the sun and shaded map lights but
ignored ordinary live point lights. A focused offscreen test first reproduced
identical matte and glossy pixels under a point light. Both scene shaders now
add a bounded local-light highlight through the existing per-cell light list,
using the existing cone and the corresponding Classic/Unified or Dynamic
attenuation. The contribution is direct light, scaled by the existing brick
material strength; it does not change alpha or the default of materials that
do not opt in. Map overflow is excluded to avoid duplicating the map-light
path. Ordinary point-light shadowing remains whatever that existing light path
provides; this is not a new shadowing implementation.

The retained test passes for individual overlays and batched brick surfaces in
Classic, Unified and Dynamic: the local lamp produces a highlight, a lamp
behind the surface does not, and removing the lamp leaves the material dark.
The existing sun/specular and normal-map offscreen controls also pass.
`cargo clippy --locked --profile playtest -p bri-render --lib --test
persistent_scene -- -D warnings` passes. These are technical pixel controls,
not close-up art direction or a performance measurement; each highlighted
fragment now iterates the existing bounded local-light list once more, so
many-light rendering cost remains a release consideration. Stock normal maps
and broader material coverage remain open.
