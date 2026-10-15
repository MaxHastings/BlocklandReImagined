# Imported map playtest fixes, and full model materials

Maxwell played the imported maps and reported four problems. Separately,
he approved bringing imported models' full look (normal, metal/roughness,
occlusion and glow maps) into the game as a general capability.

## Arctic Challenge: platforms with a bright rim

The mission lighting bake filled a 10-texel border around each surface's
lightmap rectangle, and it added light rather than setting it. Stock v20
interiors leave that border free. Community exporters pack rectangles two
texels apart, so the neighbours' borders landed inside each platform's own
rectangle and saturated its edges to white, leaving a darker middle.
`scene_lighting::bake` now gives every texel inside a rectangle to its
surface, and fills a border texel only once and only where no rectangle
lies. Checked with an offscreen render of the platforms before and after,
and by reading the baked sheet (the rim was 255, the face 194).

## Creek: no water

Creek's water block is authored at (352, 952), 2048 wide, on a terrain
from -1024 to 1024. Torque keeps water in the terrain block's own space and
wraps it, so the block covers the whole terrain. `water::terrain_wrapped`
moves a non-repeating block into the terrain block the same way. A block
as wide as the terrain covers all of it. A narrower one that still crosses
the edge keeps its in-block part and gets a warning. Shape replicators use
the same rule to keep trees out of the water.

## Grass and foliage on imported maps

`fxGrassReplicator` and `fxFoliageReplicator` nodes become a foliage pack
(`foliage.json` and its images) beside the map's bundle. The client's
`PreparedFoliage::load_for_map` prefers it over the shared pack, so
Creek's grass and Paradise's seaweed, coral and plants draw. The
replicator reading moved into `bri_foliage::Definition::from_replicator`,
which `bri-foliage-import` (Bedroom) shares. Its output is unchanged.
bri-foliage's renderer is now behind a default `gpu` feature, so the
importer depends on the content model only. Torque draws sizes and sway
times between the two values in either order (Paradise gives sway 10 to 5
seconds), so the reader sorts them.

## Creek: no waterfall

`ParticleEmitterNode`s become `map_effects.json`: the emitter and particle
datablocks the map's own scripts declare (read literally with
`effect_script`, namespaced `<map>:emitter/...`), and where each node
stands. The client adds them to the effects pack when the map loads
(`WorldEffects::set_map_effects`) and keeps them running. A particle whose
texture the game lacks draws the cloud, as v20 did.

## Full materials for imported models

glTF Normal, Metallic/Roughness, Occlusion and Emission textures are no
longer dropped. Each is saved as a helper material named after its
material (`_normal`, `_metal_rough`, `_occlusion`, `_glow`), and the
material's new `surface` record points at them with the factors. The
renderer's new Physical material kind (11) draws them: a cotangent-frame
normal map (the tilt is flipped for WebGPU's y-down framebuffer),
metal/roughness specular against the environment probe, occlusion and
glow. Map models, items, props and bodies all use it when a material has
a surface. GPU tests: `crates/render/tests/physical_surface.rs`.

## Open

- Paradise's banners: the bunting shape and its texture convert and are
  bound. What Maxwell saw is not reproduced yet; a screenshot will say.
- The emitters run in the brick effects world. Map emitters are few, so
  they bypass the nearest-source budget.
- Audio emitters (Creek's frogs and stream) are still not imported.

## Commands

    cargo build --release -p bri-addon-import --bin bri-import-addon
    bri-import-addon map Map_Creek fix/Creek --v20 "<Blockland v20>"
    scene_snapshot fix/Creek/map creek:map/creek out.png 20 -30 -380 -690 75
    cargo test -p bri-render --test physical_surface
