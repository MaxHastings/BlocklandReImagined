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

**Lightmap brightness.** A baked lightmap holds half the light it stands for and a material's `bri_lightmap` `strength` brings it back (2 for the original game's maps; 1 for a level that
turned overbright off). A game can raise the brightness of every baked lightmap while a view needs it (night vision) with `App::set_lightmap_scale`; the lightmaps of physical materials follow it.

**Detail textures.** Ground stretched under one big texture blurs up
close. A material can also carry a small texture repeated many times over
it for close-up grain, as Torque terrain and Halo do: mid-grey leaves the
colour as it is, lighter brightens it and darker darkens it. Blender's
exporter cannot write one, so tools that write the `.glb` themselves add
it to the material's `extras` as `"bri_detail": {"index": <texture>,
"scale": <repeats across the colour texture>}`.

**Baked lightmaps.** A map compiled elsewhere can bring its baked light:
a second UV set on the mesh (`TEXCOORD_1`) and, in the material's `extras`,
`"bri_lightmap": {"index": <texture>, "strength": <multiplier>}`. The
texture stands for the sun and ambient light on that surface, shadows
included, so Dynamic lighting uses it in their place (lights, moving shadows
and highlights still add), and Unified and Classic draw it as an interior
lightmap (without the strength). Only materials with a lightmap change.

**What players stand on.** With no special meshes, everything drawn is
solid. For anything else, model simple collision meshes and name them
starting with `col_` (`col_floor`, `col_stairs`): then only those collide
and they are never drawn. Boxes and simple shapes collide best; a stairway
is better as a `col_` ramp.

**Where players start.** Add an Empty named `spawn` (Blender names copies
`spawn.001`, `spawn.002`, and every one is a spawn point). Without one,
players start above the middle of the map and the import says so.

**Teleporters.** Add an Empty named `teleport_in` where players step in
and one named `teleport_out` where they come out (copies such as
`teleport_in.001` work the same). Pair them with a `bri_channel` custom
property on both: an entrance sends players to the exit with the same
channel, taking turns when a channel has more than one exit. `bri_radius`
on an entrance is how near its middle a player steps to go through, 1 unit
by default. Players leave an exit facing along the Empty's Blender +Y
(Blender shows that as the green arrow of a Plain Axes or Arrows Empty),
keeping their speed. An entrance with no exit on its channel does nothing,
and the import says so.

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

A map with lights and no baked lightmap anywhere (in an interior, or on a static model's surface) is lit by those lights in every lighting mode (Classic and Unified would draw it dark, having no lightmap to show), as Dynamic lights a map.

Any other `bri_` property is kept with the light for its type (`bri_speed`
on a strobe, for example). A Spot light's cone angles are kept too.

**The sun.** Add a Sun light and point it the way the sunlight falls. Its
colour is the sunlight's; `bri_ambient` on it (a number, or a colour as
three numbers) is how bright the shade is, 0.45 by default. Without a Sun
the map gets a high, slightly warm one. A map has one sun; any other Sun
lights are left out.

**A backdrop.** A map can carry a model drawn behind it from an eye of its own: a sky room, a distant city.
`--backdrop MODEL.gltf` takes a glTF whose scene has the custom property `bri_backdrop` with the `eye` to look from
(`{"eye": [x, y, z]}`, in the file's own frame and unit). Each frame the backdrop is drawn first, from that eye with the
player's rotation and projection and no fog, then depth starts over and the map is drawn on top, so the backdrop shows
wherever the map draws nothing (an opening cut into it). The eye does not move with the player.

**Regions with their own fog.** `--regions FILE.json` divides the map into numbered regions, each optionally with linear
distance fog (`bri_content::regions::RegionMap`: a plane tree, the region on each side of each plane, and per region a `fog`
of `color` bytes, `start`, `end`, `blend_time` and `clear_to_fog`), written in the glTF's own frame and unit. The fog that
applies is the one of the region the eye is in; entering a region blends over its `blend_time`.
A region may also list `suns` (up to 8; 64 per map), each a `direction`, `color` and `position` in the same frame: a lit moving body
(see "Lit moving bodies") takes its region's `ambient` and the suns of the region it stands in, the one standing nearest its centre
first.

**How sound carries.** A regions file can also say how the map's sounds reach the listener. Top level `sound` is
`{ "zone_decay": 0..1, "blocked_reference": 0..1 }`; per region, `connects` lists the regions sound passes into from it (a
doorway, a window left open) and `reverb` is `{ "wet": 0..1, "decay_seconds": seconds }`, the room effects ring in while the
listener is in that region. With `sound` set, a sound placed in the world is heard as authored when nothing solid stands on the
line to the listener. When something does, the distance it is heard at is cut by `zone_decay` raised to the square of the steps
between the two regions (`ZoneSteps`: the first round of `connects` is one step, and each later round doubles how far the reach
runs, so a long row of rooms costs little more than two; regions the map does not join are never heard; a map with no `connects`
at all is cut by the wall alone), and the part heard at full volume falls to `blocked_reference` of that. Without `sound` every
sound carries as authored; `reverb` works without it. Music and interface sounds are never cut and stay dry.

**Decals.** The renderer's decal layer draws projected pictures (`bri_content::decals::DecalSet`) in the native frame, each picture a PNG
beside the file: a blob shadow, a stain, a light pattern. A decal is a world-to-picture matrix (orthographic, or perspective with a
divide), six planes that choose the map's triangles inside its volume, a blend (`modulate2x`, `alpha`, `add`), whether the picture
wraps, a `facing_limit` and fade for the flat surfaces and the ground (the terrain materials), and optionally a spin in degrees a
second. The renderer finds each decal's triangles when the map loads and draws them again with the picture on them, a little in
front of themselves, every frame. A set can also list `pictures` (`name`: what a game's scripts call it, `file`: its PNG beside the
file): pictures for projectors that attach while the game runs, held by the decal layer whether or not the map has a decal of them
(`--projector-pictures FILE.json` gives a map a set of only these, written by `projector_bake`: the game's Projector actors attach at run time, and the importer bakes none).

**Light baked into a mesh.** A node's custom property `bri_vertex_lights` names the fixed lights that reach its mesh
(`bri_content::vertex_lighting`: `lights` with `position`, `color`, `radius`, optional `glow` and `negative`, and an `effect`
of `{"kind": "point"}` or `{"kind": "directional", "direction": [x, y, z]}`; `ambient`; and `visible`, one hex mask per light with
a bit per vertex of the node's mesh, least significant bit first). Positions and radii are in the node's own frame, like its
POSITIONs. A layer-blended surface is lit by its vertex colours alone, so the importer writes the sum
`clamp(0.5 * ambient + sum colour * glow * (1 - 3t^2 + 2t^3) * max(n.l, 0))`, times any COLOR_0 the mesh already has, as that colour.

**Vertex-lit surfaces.** A glTF material with the custom property `bri_vertex_light: true` is drawn as the picture times twice
the mesh's vertex colour (0.5 is neutral), fogged, lit by nothing else, in every lighting setting: a mesh whose light was baked
into its vertices. The importer keeps the mesh's `COLOR_0` for such materials.

**Lit moving bodies.** A glTF material with the custom property `bri_actor_lit: true` (or `{ "sun_gain": 1.0, "light_gain": 1.0 }`,
each 0 to 8) is drawn as a body lit when it is drawn, vertex by vertex: the scene's ambient plus the sun and the best four lights
that reach the vertex, times the picture and twice. The gains scale the sun and the lights. For a character or any body that moves, whose light cannot be
baked into its vertices; it works on a body, an item or a map's own models. Every such body, held or dropped item and worn image takes
the same light, worked out in one place (`LightVolumeState::actor_light` and `actor_lamps` in the client, `actor_light` in the scene
shaders): its zone's ambient and suns, the map's baked room light as a floor under the ambient, and the best four of the map's lamps
and the effect lights that reach each vertex. There are no visibility traces, so a lamp lights a body through a wall.

**The original engine's lighting (`--lighting DIR`).** For a map the original engine lit (an asset lane's inputs: `*-instances.json`,
`*-lights.json`, `*-instances.npz`, `*-vertex-map.npz`; `--regions` for each zone's ambient) the importer does not bake the light. The
world mesh carries a light rig (`bri_content::vertex_lighting::Rig`: each static mesh's lights and visibility bits, the vertex each
mesh vertex is within them, and the frame the lights are in) and the colours are worked out when the map loads (`Rig::colors`, which
takes a per-light scale for a light that changes), then the surfaces of those static meshes are vertex-lit surfaces. A
terrain the lane gave sector lights for (`*-terrain-lighting.json` and `.npz`: per sector its lights and their per-vertex visibility bits, and the terrain's zone and
Level.Brightness) is lit as the engine's `UTerrainSector::StaticLight` lights it (the zone's ambient times Level.Brightness, then the sector's lights) from the same rig, and its
saved `COLOR_0` is not read. Materials with a `bri_lightmap` are drawn by the lightmap alone (`lightmap_exact`: picture times lightmap times strength, fog), in every lighting
setting, as the original draws a baked surface. See `gltf/static_lighting.rs` and `native-research/specs/render-path-per-kind.md`.

**The sky.** Without anything set, the sky is a plain daytime blue. Set
these as custom properties on the Blender *scene* (**Scene Properties >
Custom Properties**):

| Property | Meaning | Default |
|---|---|---|
| `bri_sky_color` | the sky's colour, three numbers from 0 to 1 | `[0.55, 0.7, 0.9]` |
| `bri_fog` | `[start, end]` in units: clear out to start, thick at end; nothing past end is drawn | starts at 70% of end; end is twice the map's width, at least 1000 |
| `bri_fog_color` | the fog's colour | the sky colour |
| `bri_fog_density_end` | distance where fog reaches its density; independent of geometry visibility | `bri_fog` end |
| `bri_fog_max` | maximum fog coverage, above 0 to 1 | 1 |
| `bri_fog_height` | units per e-fold above the eye, 1 to 100000 | 60 |

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

**Scattered models.** Grass, rocks or trees repeated over a map are a scatter layer: one model and a list of placements, drawn as instances that fade out
with distance. Export the model alone as its own `.glb` (one mesh at the origin), write the placements with `bri_content::scatter::encode` (a `BRIS` file of position, rotation
quaternion, scale and an RGBA tint each, in the model's own frame), and name that file and the fade range in the model's scene custom property:
`bri_scatter` = `{"instances": "grass.bin", "fade": [100, 140]}`. Import the map with one `--scatter grass.glb` per layer. An instance is drawn whole out to the first
distance, fades to nothing by the second and is not drawn beyond it; leave `fade` out to draw every instance at any distance. `order` (`far_to_near`, `near_to_far`; unsorted by default) sorts the instances by distance for drawing,
`modulate_2x` (true) doubles the tint as a texture modulated 2x does (a tint of 0.5 leaves the texture as it is), and `detail` (0 to 3) draws the layer only when the viewer's detail setting is at least that. Scatter layers have no collision and no gameplay meaning.

**Blinking lights.** A `strobe`, `pulse`, `pulse2`, `flicker` or
`runway` light goes between its colour and a second one, in step for every
player. Set these on the light too:

| Property | Meaning | Default |
|---|---|---|
| `bri_speed` | `very slow`, `slow`, `normal`, `fast` or `very fast` (or 0 to 4): 4, 2, 1, 0.5 or 0.25 seconds a cycle | `normal` |
| `bri_period` | the cycle in seconds, instead of `bri_speed` | |
| `bri_color2` | the second colour, three numbers | off (black) |
| `bri_phase` | 0 to 1: how far through its cycle it starts, to set lights in a row apart | 0 |
| `bri_attack`, `bri_sustain1`, `bri_decay`, `bri_sustain2` | pulse2 only: seconds fading up, held, fading down, held off | 0.25 each |
| `bri_steps` | runway only: it is on for 1/steps of each cycle | 4 |

A strobe snaps between its colours, a pulse fades smoothly, and a flicker
jumps to a random level eight times a cycle. For a runway, give each light
in the row its place as `bri_phase` (0, 0.25, 0.5 ...).

A Spot light lights only its cone: full brightness inside Blender's
**Spot Size** less its **Blend**, fading to nothing at the edge of Spot
Size. Map lights are separate from brick lights (the wrench's
FxLightData), as they were in Torque.

**Classic and Unified lighting.** Map lights draw in Dynamic lighting.
Players who choose Classic or Unified see your map lit by its sun and
ambient light, never dark. If you want your lighting baked into the
textures for them, use Blender's **Bake** (Cycles, Combined or Diffuse) to
a texture and use that as the material's base colour.

Give a material the custom property `bri_surface` (a short word such as
`dirt`, `grass` or `metal`) to say what it is made of: footsteps and
impacts look it up by the triangle that is hit. Put it on the `col_`
meshes' materials when they are separate.

Give a material the custom property `bri_material` (any name, 1 to 128 characters) and
a hit on a `col_` mesh made of it reports that name back (`NativeMap::surfaces.identity`),
so a game can find the material it struck, not only its `bri_surface` kind.

A `col_` mesh blocks every kind of query unless its node says otherwise.
Give the node the custom property `bri_blocks`, a list of the query types
it blocks: `move` (a player or mover walking into it), `ray` (a line of
sight or pointer ray), `sweep` (a projectile or other trace with width) and
`shot` (a bullet). A glass pane that stops bullets but not bodies lists
`["ray", "shot"]`; an invisible wall that stops only players lists
`["move", "sweep"]`. The meshes with the same list share one collider.

A static model node in a map's scene can be a **breakable prop**: give it a
`breakable` property listing what breaks it, `impact` (a player striking it
fast, as v20's glass breaks), `shot` (a bullet or projectile landing on it), or
both, such as `"impact,shot"`. It then plays its `explosion` (a weapons effect
id) and `sound` (a sound profile id), both optional, where it stood; hides; and
stops colliding a moment later, for everyone, and stays broken until the map
reloads. Add `indestructable` set to `1` to keep it whole. Without a
`breakable` property, nothing changes; v20's own glass keeps breaking to
impacts only.

A scene node of kind `emitter` keeps a particle emitter running where it
stands: steam from a vent, smoke from a stack. Its `effect` property is an
emitter id (one of your weapons pack's `effects.emitters`, or a base game
`v20/emitter/...`), and the node's up axis is the way it emits. An optional
`half` ("x y z", 0 to 1024 each, in the node's own axes) is the box the
particles start in, a point when absent. Whatever the emitter ends, it
starts again, so a finite one (its `lifetime` above 0) loops and one with no
end runs for good. How many particles are alive at once follows from the
emitter's `period` and its particles' `lifetime`. Each player's game runs
its own emitters; nothing is sent over the network, and nothing is drawn that
the camera does not see. A map holds at most 256.

A collision detail of a converted shape can set `two_sided`: its triangles then
block from both faces. By default only a face a move runs into blocks, as in
v20, so a mesh wound inward lets a body walk into it from outside.

## 2. Export

**File > Export > glTF 2.0**, format **glTF Binary (.glb)**, and under
**Include** tick **Punctual Lights** and **Custom Properties**. Leave
**+Y Up** on (the default). Selected Objects only if you want part of the
scene.

## 3. Import, check and play

The release includes `bri-import-addon` and `bri-addon-check`; use `./` before
their names on Linux/macOS or `.\` in PowerShell. On macOS they are inside
`BlocklandReImagined.app/Contents/MacOS/`. A checkout can build the same tools
as shown below (see the main [README](../../README.md)). Add `--sky FOLDER`
for a picture sky and `--terrain FOLDER` for terrain:

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
- `density_end` (optional, above `start`): distance where level fog reaches
  its authored optical depth. Defaults to `end`. Set a nearer density distance
  with `max` below 1 to haze distant cliffs without clipping them. `end` continues
  to control geometry visibility. All distances are native world units.
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
## Start from a v20 map

`bri-import-addon export-map` writes a converted map (the Bedroom, Slate or
any other) as a `.glb`, with its textures, `col_` collision, `spawn`
Empties and lights, ready to open in Blender and import again:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- export-map content bedroom bedroom.glb
```

## Bring over an old Blockland map

An old Blockland map Add-On (`Map_Something.zip`, or its folder: a `.mis`
mission with its `.dif` buildings, `.ter` terrain, textures and sky)
imports straight into a map Add-On, with its baked lighting, through the
same converter that makes v20's own maps. No Blender needed:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- map Map_Something.zip addons/something --v20 "/path/to/Blockland v20"
cargo run -p bri-package-runtime --bin bri-addon-check -- addons/something
```

`--v20` names your Blockland v20 folder, for the textures, skies and
models a map borrows from the base game; without it, a map that borrows
any fails and says to add it. A map that borrows buildings or terrain
textures from another map Add-On (`Map_Slate`, `Map_Slopes`) takes them
from v20's `Add-Ons` folder or the folder beside the map. File names match
whatever their case, as on Windows. A terrain texture the map never
shipped gets a plain grey stand-in, listed in the bundle's
`stand_in_textures`.

Every mission in the Add-On becomes one of its maps (a day and a night
version, say), named after its mission file, each in its own `map/<id>/`
folder; `--mission NAME` imports just
one. A single map's id comes from the file name without `Map_`
(`something`), or `--id`; the menu name from the mission, or `--name`.

A shape replicator (Torque's scattered trees and rocks) is placed once,
on import, as ordinary models on the terrain: from its own seed and
settings, keeping off water and slopes steeper than it allows. The
original also kept copies apart and off buildings, so a forest can differ
tree for tree. A mission's scripts, triggers and sounds are left behind:
game rules are Rhai scripts in this engine.

## Not yet

- Baking your models' lighting and shadows on import.
- Blinking lights in Classic and Unified lighting, which draw map lights
  baked.
