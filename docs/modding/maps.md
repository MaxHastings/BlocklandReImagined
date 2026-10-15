# Making maps

A map is an Add-On. You model it in Blender (or anything that exports
glTF 2.0), `bri-import-addon gltf --kind map` turns the `.glb` into an
Add-On folder, and the map appears in **Start Game** once the Add-On is on.
Friends who join download it with your other shared Add-Ons; nothing else
is needed on the host. The design behind this is
[map-packages.md](../architecture/map-packages.md).

Start from [`sample-map`](../../packages/samples/sample-map): a room with a
spawn point, a strobe light and a spot light. Open
`blender/sample-map.glb` in Blender (**File > Import > glTF 2.0**) to see
how it is laid out, and copy the folder to try it in the game as it is.

## 1. Build it in Blender

**Scale.** One Blender metre is one game unit. A Blockhead is 2.65 units
tall, a 1x1 brick 0.5 wide and 0.6 tall (a plate 0.2). If you modelled in
other units, `--scale` fixes it on import (`--scale 0.01` for centimetres).
Put the floor near Z = 0 in Blender; the game's up is Blender's up.

**What players see** is every mesh. Each material becomes one PNG: its
base colour texture with its colour factor baked in, or a plain colour when
it has no texture. Keep textures in PNG or JPEG.

**What players stand on.** With no special meshes, everything drawn is
solid. For anything else, model simple collision meshes and name them
starting with `col_` (`col_floor`, `col_stairs`): then only those collide
and they are never drawn. Boxes and simple shapes collide best; a stairway
is better as a `col_` ramp.

**Where players start.** Add an Empty named `spawn` (Blender names copies
`spawn.001`, `spawn.002`, and every one is a spawn point). Without one,
players start above the middle of the map and the import says so.

**Lights.** Add Point and Spot lights where you want them. The light's
colour is used. Its strength in watts is not, because Torque lights were
not physical: set these as custom properties on the light object
(**Object Properties > Custom Properties > New**):

| Property | Meaning | Default |
|---|---|---|
| `bri_radius` | how far it reaches, in units; nothing past it is lit | 10 |
| `bri_inner` | full brightness out to here, then fading to `bri_radius` | 0 |
| `bri_brightness` | multiplies the colour | 1 |
| `bri_light_type` | `omni`, `spot`, `strobe`, `pulse`, `pulse2`, `flicker` or `runway`, map2dif's light types | `spot` for a Spot light, `omni` otherwise |

Any other `bri_` property is kept with the light for its type (`bri_speed`
on a strobe, for example). A Spot light's cone angles are kept too. Sun
lights are left out: every map has one sun of its own.

Today every light draws as a steady omni light, whatever its type; the
type is saved, so your map plays strobes, pulses and spot cones without
changes once the engine does. Map lights are separate from brick lights
(the wrench's FxLightData), as they were in Torque.

**Classic and Unified lighting.** Map lights draw in Dynamic lighting.
Players who choose Classic or Unified see your map lit by its sun and
ambient light, never dark. If you want your lighting baked into the
textures for them, use Blender's **Bake** (Cycles, Combined or Diffuse) to
a texture and use that as the material's base colour.

## 2. Export

**File > Export > glTF 2.0**, format **glTF Binary (.glb)**, and under
**Include** tick **Punctual Lights** and **Custom Properties**. Leave
**+Y Up** on (the default). Selected Objects only if you want part of the
scene.

## 3. Import, check and play

From a checkout of this repository (see the main [README](../../README.md)):

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- gltf my-map.glb addons/my-map --kind map --name "My Map"
cargo run -p bri-package-runtime --bin bri-addon-check -- addons/my-map
```

The import writes `package.json` and `map/` (the scene, the model and its
PNGs, and `bundle.json`) and prints `IMPORT-REPORT.md`: the map's size,
its spawn points and anything it left out. The Add-On id comes from the
file name, or `--id`. `bri-addon-check` loads the map the way the game
does and names anything that would stop it: a missing spawn point, an
unknown light type, a model with no collision, a missing texture.

Put the folder in the game's `content/addons/`, open
**Start Game > Add-Ons**, turn it on, and pick the map in Start Game.

To change the map, edit it in Blender, export again and import into a
fresh folder; then replace the old folder with it. Keep the same id (the
same file name or `--id`) so saved worlds and servers still find it.

## Fog and haze

A map can bring its own fog: add a `fog` to its entry in
`map/bundle.json`'s `maps` (the importer does not write one, so add it
after importing):

```json
"maps": [{ "id": "my-map:map/my-map", "name": "My Map", "file": "my-map.scene.json",
           "fog": { "start": 60, "end": 1200, "color": [0.72, 0.76, 0.82],
                    "height": 30, "max": 0.85 } }]
```

- `start` and `end`, units: the fog begins at `start` and is at its
  thickest by `end`, the visible distance; nothing past `end` is drawn.
- `color`, `[r, g, b]` from 0 to 1: the fog, and with no sky of the map's
  own, the backdrop behind the world.
- `height` (optional, 1 to 100000, default 60): how quickly the fog thins
  with height above you. A small value keeps a thick haze down in a canyon
  and clear air over the cliff tops.
- `max` (optional, above 0 to 1, default 1): the most the fog ever covers.
  Below 1 it is a haze, as Halo's Blood Gulch has: far cliffs soften into
  it but never vanish, and they do not fade out before `end` (they simply
  stop there, so put `end` past your farthest geometry).

`bri-addon-check` refuses a fog with a field it does not know or a value
out of range. Rules can still change all of it while a game runs
(`set_environment`'s `fog_height` and `fog_max`, [rules.md](rules.md)).

## Not yet

- Opening a converted v20 map (the Bedroom, Slate...) in Blender, with its
  lights, to build on. This is next, through the same import.
- Terrain from a heightmap, and baking lighting on import.
- Strobe, pulse, flicker and runway animation, and spot cones, in the
  renderer (the types are kept in your map already).
