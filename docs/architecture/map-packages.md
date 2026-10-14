# Map Add-Ons: authored maps from Blender, TrenchBroom and converted v20

Status: approved by Maxwell on 2026-10-14 ("one place for maps, one
pipeline", vanilla included); building. The first version is scoped in
"Keeping it small"; sections marked deferred are direction, not commitment.
Nothing here changes the alpha contract.

## The problem

Nobody can make a map for this game today. The only way a map gets in is
converting a v20 mission and its `.dif` interiors with `map_bundle`, and a
`.dif` only comes from Torque's map compiler (map2dif, Constructor). Modders
already know that path is broken: pharzedia's Bedroom extensions crashed,
leaked light, glitched lightmaps and had bad collision when compiled with
map2dif, and they could only guess at the original lamp positions.

Four requests from the community (2026-10-14) all come back to that:

- pharzedia wants to open the *converted* Bedroom (our native format, not
  the original `.dif`), extend it, keep its recovered dynamic lights, add,
  move, recolour and remove lights (point or spot; rainbow, strobe, blink),
  and ship the result as an Add-On. They expect to do their own collision.
- Wilfred wants new maps made directly in Blender, or in a Quake-style
  editor such as TrenchBroom, without touching Blockland's maps at all.
- pharzedia cannot add a new map (terrain plus interiors) to the map list
  in native formats at all; only converted v20, v0002, RTB, AIO or TBM
  content reaches it.
- pharzedia also noticed that shadow-casting lights exist only on converted
  maps: nothing they can put on a brick, a model or a glTF casts a shadow.

A Blender script that reads `*.interior.json` would answer the first
question literally and none of the others: the edited map still could not
come back in. The design below fixes the cause instead.

## What is in the way today

Evidence is from main at `ee244a8`.

- **Maps are base content only.** One package holds every map, the
  `v20-map-bundle` with role `map_bundle` (`crates/package/base-packages.json:4`).
  A role allows one package (`crates/package/src/packages.rs:59`). There is
  no `map` content kind (`crates/package-runtime/src/content.rs:78`); `world`
  means a chunked generator.
- **A hard-coded list decides which maps exist.** `LOADABLE_MAPS` names the 13
  v20 missions and the Tutorial (`crates/sim/src/map.rs:16-31`). Start Game,
  Change Map and the dedicated server all filter by it
  (`crates/client/src/content.rs:619-657`, `crates/net/src/map_content.rs:229-252`).
- **Joiners never download a map.** Packages with a role are never offered
  (`crates/net/src/packages.rs:49-66`), so a joiner must already have the
  exact bundle. Shared Add-On content, by contrast, downloads on join
  through `bri_package::sync` (2 GiB a package, 256 MiB a file).
- **The format is a conversion output, not an authoring format.** A scene
  (`crates/content/src/scene.rs`) places `*.interior.json` files whose
  vertices carry Torque lightmap coordinates and whose lighting lives in
  embedded and baked lightmaps (`crates/content/src/interior.rs`).
- **Map lights are inferred, not stored.** DIF keeps no light objects, so
  `map_lighting::fit` recovers up to 24 lights by inverse rendering
  against the lightmaps, and `prepare_lighting` writes them to the bundle's
  `lighting-parameters.json` (position, colour, inner and outer falloff;
  `crates/render/src/lighting_parameters.rs:13`). They are approximate.
- **Two unrelated light systems.** Map lamps become `LampLight`s, the only
  lights that cast shadows (at most 4 a frame at Best;
  `crates/render/src/shadow.rs:81`). Brick and image lights are v20
  `FxLightData` (`crates/content/src/effects.rs:56`), which carry the colour,
  brightness and radius curves that make rainbow, strobe and blink. They
  render as unshadowed `PointLight`s (`crates/render/src/scene.rs:2113`).
  Neither system has spot lights.
- **The default lighting mode needs lightmaps.** Unified is the default
  (`crates/ui/src/screens/options.rs:125-136`); it decomposes a map's
  lightmaps. Only Dynamic lights a map entirely live.
- **The glTF importer already does most of the geometry work.** It reads
  meshes, Base Color textures, Empties and `col_` collision meshes into a
  native `*.shape.json` (`crates/addon-import/src/gltf`). Static models in a
  map scene already get triangle-mesh collision from their collision detail
  (`crates/physics/src/content.rs:54-94`). It ignores lights
  (no `KHR_lights_punctual`).

## The decision

**A map is content an Add-On provides, like a weapon or a body. Vanilla's
converted maps use the same kind.**

That follows the platform principles: the engine owns the mechanism (load a
scene, collide with it, light it, send it to joiners), the package owns the
map. A map Add-On is `shared`, so it downloads on join with no new
transport. And vanilla is the proof the kind is expressive enough: when the
converted Bedroom can be exported to a map Add-On and loads identically,
pharzedia's workflow is just "export, edit, re-import".

The runtime never reads glTF or `.map` files. Authoring tools turn them into
native files at import time, as `bri-import-addon gltf` already does for
bodies and vehicles, which keeps Torque and editor readers outside the game's
dependency graph (AGENTS.md).

## The format

A `map` entry in `package.json`'s `provides`, id `<package>:map/<name>`,
pointing at a `*.map.json`:

```json
{
  "schema_version": 1,
  "name": "Bedroom Extended",
  "description": "The Bedroom with a hallway and a bathroom.",
  "preview": "maps/bedroom-extended.png",
  "models": [
    { "id": "room", "model": "models/room.shape.json" }
  ],
  "nodes": [
    { "kind": "model", "model": "room", "transform": [16 floats] },
    { "kind": "spawn", "transform": [16 floats] },
    { "kind": "light", "light": "lamp-bulb", "transform": [16 floats], "shadows": true },
    { "kind": "light", "light": "v20:light/rainbow", "transform": [16 floats] }
  ],
  "lights": [
    { "id": "lamp-bulb", "type": "point", "color": [1.0, 0.85, 0.6],
      "brightness": 1.0, "radius": 12.0 }
  ],
  "environment": { "sun": { "azimuth": 30, "elevation": 50, "color": [1,1,1], "ambient": [0.3,0.3,0.35] },
                   "sky": "v20:environment/sky_blue2" }
}
```

- **Geometry** is one of two native asset types, both already loaded by
  today's map path:
  - ordinary models (`*.shape.json`), drawn and collided as map static
    models already are. Collision is the model's collision detail, a
    triangle mesh, so editor geometry can be exact. This is what Blender
    maps use.
  - lightmapped architecture (`*.interior.json`), with lightmap UVs, its
    lightmaps and authored collision. This is what converted v20 maps use,
    so they keep their Classic and Unified look. Authoring tools are not
    required to produce it.
- **Nodes** are a flat list of absolute transforms in native space (x right,
  y up, -z forward; 1 unit = 1 Blender metre), the scene's existing rule.
  Kinds start with `model`, `spawn`, `light` and `terrain`; water, foliage
  and bounds follow the existing scene node kinds when a map needs them.
- **Terrain** is a heightfield the Add-On ships (a 16-bit PNG height image,
  its square size and height scale) plus texture layers and their blend
  masks, loaded into the same `TerrainField` and terrain mesh that converted
  `.ter` files produce (`crates/content/src/terrain_field.rs`). Sculpted in
  any tool that exports a height image (Blender, World Machine, Gaea).
- **Lights** are one engine light definition, the existing `FxLightData`
  shape (colour, brightness, radius, optional curves, flare) plus `type`
  (`point` or `spot`, with cone angles) and a node-level `shadows` flag.
  A node names a light defined in this file or any light id the Add-On can
  see, so the v20 rainbow, strobe and blink lights are available by id, and
  an author's own animated light is a curve in the same format.
- **Environment** names a sky from the Environment pack or ships its own
  faces, plus the sun. Weather stays the Environment window's.
- Unknown fields are errors, as in every manifest.

## Lights become one system

Brick lights, image lights and map lights should all be the same engine
light: a definition (with optional animation curves) placed at a transform,
optionally casting shadows. The renderer's shadow budget (4 lamps at Best,
`pick_lamps`) then chooses among every shadowed light near the camera, not
only lamps recovered from lightmaps.

That answers pharzedia's last point directly: a brick light, a light on a
model or a light in a map Add-On can cast shadows, and the rainbow, strobe
and blink they know from v20 are animation curves the engine already
samples (`Light::sample`). It is a renderer change (spot cones, per-light
shadow flag, one light list) independent of the map kind, and it is useful
even before map Add-Ons exist.

## Authoring paths

All three end in the same files.

1. **Blender (and anything else that exports glTF).**
   `bri-import-addon map scene.glb <out>` extends the glTF importer:
   - meshes become one or more models; `col_` meshes become collision, as for
     vehicles;
   - Empties named `spawn`, `spawn.001`, ... become spawn nodes;
   - Blender lights, exported with `KHR_lights_punctual`, become light nodes
     (point and spot keep their colour, power-derived brightness, range and
     cone);
   - glTF `extras` (Blender custom properties) carry what glTF cannot say:
     `bri_light: "v20:light/strobe"` to use a named animated light,
     `bri_shadows: false`.
2. **Converted v20 maps as templates.** `bri-export-map <bundle> <map-id>
   <out>` writes a converted map as a map Add-On *and* a `.glb` with its
   recovered lights as glTF lights and its textures attached. That `.glb`
   opens in Blender with the lamps where the game puts them; pharzedia edits
   and re-imports it through path 1. A round-trip test (export Bedroom,
   import the `.glb`, compare geometry, collision and lights within
   tolerance) keeps both directions honest.
3. **TrenchBroom (Quake `.map`).** `bri-import-addon map level.map <out>`
   reads Valve 220 brushes and entities. Brushes are convex solids, which
   is exactly what collision wants, and point entities (`light`,
   `info_player_start`) become lights and spawns. A game configuration file
   for TrenchBroom ships in the repo so the editor knows the entities and
   texture folder. This is the smallest importer of the three.

## Lighting modes

Converted vanilla maps keep their lightmaps (lightmapped architecture
above), so every mode looks as it does today. Authored maps have no lightmaps. Dynamic lights them live and needs nothing
more. Classic and Unified (the default) read lightmaps, so a map Add-On
needs one of:

- **(a) Bake at import.** The importer bakes lightmaps from the authored
  lights and sun with the existing offline bake (`scene_lighting.rs`, a port
  of v20's `SceneLighting`), so every mode looks right. Extra importer work,
  bigger downloads, and lights that move or animate are baked at one state.
- **(b) Dynamic for authored maps.** A map Add-On always renders as Dynamic,
  whatever the menu says, with a note in the map's description. No bake;
  needs Dynamic's quality and performance to hold up on authored maps.

Recommendation: (b) first, because Dynamic is where the renderer is heading
and it is what makes animated and movable lights real; add (a) only if
playtests show Classic and Unified players need it.

## Identity, joining and limits

- Map ids are namespaced (`bedroom-extended:map/main`), as principle 2
  requires. The protocol already sends the map id as a string
  (`crates/net/src/protocol.rs:587`).
- `LOADABLE_MAPS` goes away when vanilla moves onto the kind; the map lists in Start Game, Change Map and the dedicated server
  also take every enabled Add-On's maps. `GameMode.map` already accepts a
  package id.
- A map Add-On is `shared`: the host's copy downloads on join and its hash
  is part of the join comparison, with the existing sync limits. A map gets
  its own budget on top (triangles, lights, texture bytes) that
  `bri-addon-check` reports, so a server owner sees a too-heavy map before
  players do.
- Saves keep naming the map by id. A build made on a map Add-On that is
  later missing loads its bricks and reports the map, as principle 1
  requires from beta on.

## Keeping it small

Maxwell's constraint (2026-10-14): help pharzedia without creating
technical debt we must maintain. So the first version reuses what exists and
adds one format reader at most.

**The core, which we would keep anyway.** Every piece reuses an existing
mechanism; the new code is glue.

1. **`map` content kind and loader.** A `*.map.json` places existing
   `*.shape.json` models (drawn, and collided through their collision
   detail, as map static models already are), spawns and lights. Terrain
   uses the existing `TerrainField`, Start Game lists Add-On maps, and
   joiners download it as they do any shared Add-On. Tested with a
   hand-written fixture map.
2. **glTF map import**, extending `bri-import-addon gltf` rather than adding
   a tool. Its only new reads are Empties named `spawn` and Blender lights
   (`KHR_lights_punctual`, point only). glTF stays the one interchange
   format we support.
3. **Export of a converted map to `.glb`**: geometry, textures and the
   recovered lights from `lighting-parameters.json`. This gives pharzedia
   Bedroom in Blender with its lamps in place, and it goes back in through
   step 2. A round-trip test guards both directions.
4. **Map lights reuse the existing light definition** (`FxLightData`, so
   rainbow, strobe and blink by id) and the existing lamp-shadow path.
   Authored lights join the recovered ones in `pick_lamps`; no second light
   system.

**Deferred until someone needs it.** Each item below is a feature to
maintain, and none is needed for the workflow above.

- **TrenchBroom import.** A second importer to maintain. Blender can import
  Quake `.map` files with existing community add-ons, then export glTF.
- **Spot lights.** New renderer work (cone shadows). Point lights first.
- **Shadows on brick and image lights.** Worth doing, but it touches the
  effects pipeline and the shadow budget everywhere; it is its own change.
- **A lightmap bake for Classic and Unified.** Authored maps render as
  Dynamic (option (b) above).
- **A Blender Python reader for `*.interior.json`.** It would be a third
  format reader, and step 3 replaces it.

5. **Vanilla on the kind (decided).** The v20 converter writes its maps as
   map Add-Ons, carrying their lightmapped architecture. The `map_bundle`
   role's own map list, `LOADABLE_MAPS` and the second loading path go
   away. There is then one place for maps and one pipeline, and the
   converter is just an importer like the glTF one. This lands before beta,
   when the map format freezes with the other schemas; after that, changes
   only add fields.

## Decisions (Maxwell, 2026-10-14)

- One map format, one loader and one map list for every map, vanilla
  included, meant to last for years. The core (steps 1 to 5) is approved.
- Authored maps without lightmaps render as Dynamic.
