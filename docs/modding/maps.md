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
on a strobe, for example). A Spot light's cone angles are kept too.

**The sun.** Add a Sun light and point it the way the sunlight falls. Its
colour is the sunlight's; `bri_ambient` on it (a number, or a colour as
three numbers) is how bright the shade is, 0.45 by default. Without a Sun
the map gets a high, slightly warm one. A map has one sun; any other Sun
lights are left out.

**The sky.** Without anything set, the sky is a plain daytime blue. Set
these as custom properties on the Blender *scene* (**Scene Properties >
Custom Properties**):

| Property | Meaning | Default |
|---|---|---|
| `bri_sky_color` | the sky's colour, three numbers from 0 to 1 | `[0.55, 0.7, 0.9]` |
| `bri_fog` | `[start, end]` in units: clear out to start, thick at end; nothing past end is drawn | starts at 70% of end; end is twice the map's width, at least 1000 |
| `bri_fog_color` | the fog's colour | the sky colour |

For a picture sky, put six pictures named `front`, `right`, `back`,
`left`, `up` and `down` (PNG or JPEG) in a folder and import with
`--sky that-folder`. `front` faces Blender's +Y, and the rest go round in
that order. This is the order and layout of a Blockland skybox's `.dml`
list, so a Blockland sky's six pictures work renamed. Hosts can still pick
another sky in the Environment window, and the map's own comes back when
they clear their pick.

**Water.** Name a flat mesh `water` (or `water.001`, `water_pond` and so
on). It isn't drawn as a model or collided with. Instead it becomes a body
of water players swim in: its top is the surface, and it goes down to the
lowest point of the map. Its material's picture is the water's surface,
or a gentle ripple in its base colour when it has none, and its alpha is
how see-through it is. These are custom properties on the water object:

| Property | Meaning | Default |
|---|---|---|
| `bri_depth` | how deep it goes below its surface, in units | down to the bottom of the map |
| `bri_liquid` | `water` or `lava` (lava hurts), or one of v20's liquid types | `water` |

**Terrain.** For rolling ground bigger than you want to model, make a
folder holding:

- `height.png`: a grey heightmap, black the lowest and white the highest,
  seen from above with Blender's +Y at the top. 16-bit PNGs keep slopes
  smooth. It is resampled to a square power of two (up to 1024) if it
  isn't one.
- `layer0.png` to `layer7.png` (or JPEG): up to eight ground textures. Each
  covers 8 of the terrain's squares before it repeats.
- `weight1.png` to `weight7.png`: grey masks, the size of the heightmap,
  white where that layer shows. `layer0` fills whatever the others leave,
  unless it has a `weight0.png` too.
- `detail.png`, optional: a fine grain blended over the ground up close.

Import with `--terrain that-folder`, and size and place it with these
custom properties on the **Scene**:

| Property | Meaning | Default |
|---|---|---|
| `bri_terrain_size` | how wide the terrain is, in metres | `512` |
| `bri_terrain_height` | how far white is above black, in metres | `64` |
| `bri_terrain_center` | `[x, y, z]` in Blender: the middle of the terrain, at the height of black | `[0, 0, 0]` |

Players walk on it, and bricks build on it as they did on v20's terrain.
The sun's light and the terrain's own shadows are baked into it on import
for Classic and Unified lighting; your models don't cast shadows on it
there yet. Heightmaps and masks from World Machine or Gaea work, and so
does a greyscale picture you paint.

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

From a checkout of this repository (see the main [README](../../README.md)).
Add `--sky FOLDER` for a picture sky and `--terrain FOLDER` for terrain:

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

## Start from a v20 map

`bri-import-addon export-map` writes a converted map (the Bedroom, Slate or
any other) as a `.glb`, with its textures, `col_` collision, `spawn`
Empties and lights, ready to open in Blender and import again:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- export-map content bedroom bedroom.glb
```

## Not yet

- Baking your models' lighting and shadows on import.
- Strobe, pulse, flicker and runway animation, and spot cones, in the
  renderer (the types are kept in your map already).
