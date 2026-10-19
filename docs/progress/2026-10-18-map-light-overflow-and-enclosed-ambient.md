# 2026-10-18 Map light overflow and enclosed ambient

Two small generic lighting seams from the Pipeline lighting lane
(`codex/aa-lighting`). Neither names a map; both are inert for a map that
does not use them.

## Map light overflow

Before: a map with more than 24 lights lost every light past the 24th
("a map draws at most 24 lights"). A converted source map can author hundreds.

After: plain lights past the 24 shaded slots load as
`MapScene::overflow_lights`. The client appends the nearest to the point-light
pool each frame (`overflow_point_lights`, behind the effect and vehicle
lights, up to `MAX_POINT_LIGHTS`). Pool colour w 1 selects the same linear
falloff the shaded lights use (`point_illumination`), so a lamp does not change
shape when it moves between the two. Cones and animated lights past the budget
are still left out and named in the scene's omissions.

Limits: these lights cast no shadows, so a lamp can light a floor on the far
side of a wall inside its radius; the 24 shaded slots follow node order, not
the player. Both are the next step (per-player shaded selection).

Evidence: `render/tests/map_light_budget.rs` (24 shaded, overflow order, cone
omission, `enclosed`), `overflow_point_lights` unit test, `shader_validation`,
`dynamic_lighting`, `dynamic_spot`, `dynamic_unlit`, `modern_lighting` and
`persistent_scene` pass.

## Enclosed ambient

Before: one ambient light everywhere, so an interior lit by its own lamps
could not be darker than the open yard.

After: the sun node may set `enclosed` (RGB). In Dynamic with Soft Shading the
ambient blends from the map's ambient to `enclosed` as the skylight exposure
field (`sky_exposure`) finds a surface closed off from the sky; outdoors, and
anywhere the field has not measured, keep the ambient. `Camera::enclosed`
carries it (w 1 when set); `SceneData::enclosed_ambient` is the loaded value.

Not done: Classic/Unified draw authored maps with sun and ambient only
(`map-packages.md`), so they ignore it; a server's ambient override does not
scale it.
