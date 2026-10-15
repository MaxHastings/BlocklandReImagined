# 2026-10-14 Map Add-Ons: the v0.3.1 map gaps

Thread: Map Add-Ons (branch `claude/project-thread-w9trq5`). Max chose "all
gaps": v0.3.1 waits for these map features. All are general engine and
importer features; Blood Gulch (private repo) was the end-to-end test, and
nothing is special-cased for it.

## What changed

- **Bricks on map levels.** A glTF map's level is marked `surface: level`
  and the sim treats it as an interior, so bricks can no longer be buried in
  its walls (found building on Blood Gulch).
- **Sun, sky and water** (`gltf/map_environment.rs`): the first Blender Sun
  light is the map's sun (`bri_ambient`); scene properties `bri_sky_color`,
  `bri_fog`, `bri_fog_color`, or `--sky DIR` with six faces in `.dml`
  order; meshes named `water...` become water volumes (`bri_depth`,
  `bri_liquid`). The client keeps an Add-On map's own sky on a reset
  (`environment_choices::set_sky_for`).
- **Materials**: a stated metallic factor of 0.5 or more makes bare
  `Metal` (also drawn on Add-On map statics, `scene_loader.rs`); emission
  of 0.5 or more draws self-lit.
- **Heightmap terrain** (`gltf/map_terrain.rs`, `--terrain DIR`):
  `height.png`, `layer0-7` with `weight` masks and `detail` become a native
  `Terrain` and `TerrainInstance` (power-of-two side up to 1024, last row and
  column empty so the block does not wrap), with sunlight and the
  terrain's own shadows baked into its Classic/Unified lightmap.
- **Light animation** (`bri_content::light_animation`): strobe, pulse,
  pulse2, flicker and runway from the light's `speed`/`period`, `color2`,
  `phase`, pulse2 timings and runway `steps`, applied as each map light's
  run-time tint on wall-clock time so every player blinks together.
- **Spot cones**: the map light uniform's light stride grew from 48 to 64
  bytes with a `spot` word (direction and cone, `scene::spot_words`);
  `SceneRenderer::set_map_light_spots` sets them; `light_given` applies
  the cone.
- **Old Blockland map Add-Ons** (`bri-import-addon map`,
  `crates/addon-import/src/v20_map.rs`): `bri-convert` and `map_bundle`
  became library passes (`bri_convert::convert_root`,
  `bri_convert::map_bundle`) and the importer runs them on the Add-On laid
  out as a v20 install (with `--v20`'s `base/` hard-linked in), then renames
  the map into the Add-On's namespace. `bri-addon-check` now finds a
  binding's texture by its file name, as the game does.
- In-game terrain editing (Max's direction) recorded in
  `docs/architecture/seams.md`.

## Evidence

- `python tools/check.py focused` over bri-content, bri-addon-import,
  bri-render, bri-client, bri-package-runtime and bri-convert.
- New tests: `a_map_brings_its_own_sun_sky_and_water`,
  `a_map_brings_its_own_heightmap_terrain`,
  `metal_and_glow_come_from_the_material`, the light animation unit tests,
  `dynamic_spot_lights_only_its_cone` (software GPU), and
  `an_old_blockland_map_add_on_becomes_a_map_add_on` (a synthetic
  `Map_Hills.zip` with a `.mis`, `.ter` and texture, loaded by the sim and
  both renderer paths).
- Offscreen renders of a test map: the metal wall, and terrain blending
  grass and sand.

## Not yet

- Blinking lights in Classic and Unified, which draw map lights baked.
- Models' shadows on baked terrain; lightmaps from a second UV map and
  detail textures for glTF maps.
- Moving the vanilla maps onto the map kind (step 5's other half).
- Weather (precipitation) for map Add-Ons.
