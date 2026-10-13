# Models from Blender (glTF)

`bri-import-addon gltf` turns a glTF 2.0 model (`.glb`, or `.gltf` with its
files) into the game's native model: a player or bot body with its own
skeleton, clips and textures, a held weapon, or a vehicle or prop. Blender
exports glTF from File > Export > glTF 2.0; so do Blockbench, Maya and most
other tools.

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- gltf mario.glb addons/mario --kind body --name "Mario" --height 1.6
cargo run -p bri-addon-import --bin bri-import-addon -- gltf rifle.glb rifle-model --kind item
cargo run -p bri-addon-import --bin bri-import-addon -- gltf buggy.glb buggy-model --kind prop
```

| `--kind` | Writes | Then |
|---|---|---|
| `body` (default) | a whole Add-On: `package.json`, the model and its PNGs under `assets/`, `assets/content.json` and an archetype (`assets/archetypes/<id>.json`) whose `model` is the imported body (`<id>:body/<id>`, provided as `body` content). Everyone draws it, so the Add-On is shared: friends who join download the body and its textures | check it with `bri-addon-check`, adjust the archetype's `movement` (its collision box, sized from the model; see Bodies), name it in a mode or `set_archetype` |
| `item` | `models/<id>.shape.json` and its PNGs | copy `models/` into your weapon Add-On's `assets/` and name `models/<id>.shape.json` as the item's and image's `model` in `weapons.json` ([Weapons](weapons.md)) |
| `prop` | the same, for vehicles, wheels, projectiles and props | name it from `vehicles.json`; the report lists the `mount` and `hub` empties' positions for its seats and wheels |

Every run writes `IMPORT-REPORT.md` beside the output: size, sockets, each
clip and how it plays, the animation roles the body has no clip for and
what it does instead, and every part of the model it could not use, with
the fix. `--json` prints the same report as JSON.

Options: `--id` (the file and package name; from the file name otherwise),
`--name` (a body's menu name), `--height UNITS` or `--scale FACTOR`,
`--fps` (clip samples per second, 30 by default), `--right-hand`,
`--left-hand` and `--eye` (a node to use as that socket), `--loop CLIP`
(an extra clip that loops), `--box-width` and `--box-height` (a body's
collision box), and `--lod TRIANGLES` and `--lod-pixels N` (its distant
detail).

## 1. Scale, axes and origin

- **1 Blender metre = 1 game unit.** A Blockhead stands 2.65 tall and 1.25
  wide; a 1x1 brick is 0.5 wide and a plate 0.2 tall. A Mario next to a
  Blockhead is about 1.6 tall; a Spartan about 2.9.
  If your model is a different size, pass `--height 2.9` (body, feet to top)
  or `--scale 0.01` and the importer scales it; you don't have to rescale in
  Blender.
- **Face -Y in Blender** (Blender's Front view, numpad 1, shows the face and
  the gun barrel pointing at you). Keep +Z up. Export with the glTF
  exporter's default "+Y Up" on.
- **Origin between the feet**, at floor level, for a body. For a weapon, the
  origin can be anywhere; the `mountPoint` empty decides where the hand holds
  it.
- **Apply transforms** (Ctrl+A, All Transforms) on meshes and the armature
  before export. Uniform scale on objects is handled; non-uniform scale on a
  bone parent is approximated and warned about.

## 2. Bodies (characters)

**Rig.** One armature, any bone names. Skinning is read from vertex groups
(up to 4 bones per vertex; extra weights are dropped and reported). Rigid
parts parented to a bone also work. Keep it under about 20,000 triangles and
128 bones.

**Sockets.** Add an Empty (Plain Axes) for each, parented to the bone that
carries it, placed where it should be in the rest pose. Their rotation does
not matter: the importer lines each socket up with the body.

| Empty name | What it is | If missing |
|---|---|---|
| `socket_right_hand` | where a held weapon's grip goes (the palm, not the wrist) | the importer uses a right-hand bone (`hand.R`, `RightHand`, `mixamorig:RightHand`) and warns |
| `socket_left_hand` | the second, akimbo hand | the left-hand bone, as above |
| `socket_eye` | first-person camera, between the eyes | placed at 92% of the body height and warned about |

You can also point at any bone instead: `--right-hand hand.R --eye head`.
Rigged bodies sit through their `sit` clip, so no seat socket is needed.

**Animation clips.** Each Blender action becomes one clip, named after the
action. Name them by role (the names are not case-sensitive):

| Action name | Plays | Author it as |
|---|---|---|
| `idle` | standing still | a loop (first frame = last frame) |
| `run` | moving forward | a loop, running in place (no forward drift of the root) |
| `back`, `side` | moving backward, strafing sideways | loops; if missing, `back` plays `run` reversed, `side` plays `run` |
| `jump` | on takeoff | once |
| `fall` | in the air | a loop; if missing it plays `jump` |
| `crouch` | crouching down | once; its last frame is the crouched pose |
| `crouch_run`, `crouch_back`, `crouch_side` | moving while crouched | loops |
| `aim_right`, `aim_left`, `aim_both` | holding an item in that hand, or both | a held pose: the body holds the last frame. Key only the arm bones |
| `sit` | seated in a vehicle or on a rideable | a held pose; one frame is enough. If missing it sits in `idle` |
| `death` | on death | once; it holds the last frame |
| `swim` | in water | a loop |
| `look` | looking up and down | first frame looking straight up, middle frame level, last frame straight down. Key only head, neck, spine and arms |
| `head_side` | free look | first frame head turned fully left, middle forward, last fully right |
| `head_up` | head tilted up under a pack | first frame neutral, last tilted |

`look`, `head_side` and `head_up` are layered on top of everything else (the
importer stores them relative to their middle or first frame), so a running
Mario still looks up. Any other action (`stomp`, `wave`, `ground_pound`) is
kept under its own name for rules to play. Blockland names also work
(`root`, `armreadyright`, `death1`).

Missing clips are fine: each missing role falls back (a body with only
`idle` and `run` still works) and the report lists what each missing one
costs.

**Materials.** Optional: name an object after a Blockhead colour slot
(`chest`, `pants`, `headskin`, `larm`, `rarm`, `lhand`, `rhand`, `lshoe`,
`rshoe`) and it is painted with the player's avatar colour for that slot.
Every other object draws with its own textures, untinted. For team-coloured
armour (a Spartan), make the plates one object named `chest` with a plain
white material: the player's chest colour paints it.

**Size and collision box.** The body's archetype gets a collision box from
the model: as tall as the model (after `--height`), and as wide for that
height as a Blockhead (1.25 for 2.65), with its eye heights from the eye
socket. Set it yourself with `--box-width UNITS` and `--box-height UNITS`
(Mario's SM64 hitbox is 37 units in radius and 160 tall, so `--box-width
0.74 --box-height 1.6`), or later in the archetype's `movement` (`width`,
`stand_height`, `crouch_height`, `stand_eye`, `crouch_eye`, `sit_eye`).
The box, not the mesh, is what bullets, explosions, other players and
vehicles hit.

**Distant detail (optional, off by default).** Classic-era models are light
enough to skip this. For a heavy model, `--lod TRIANGLES` adds a lighter
copy with that many triangles that the body draws when it is small on
screen (under 100 pixels tall: about 20 units away for a Blockhead at
1080p; `--lod-pixels N` changes that). It keeps the skeleton, skinning,
textures and sockets. To make it yourself, add a simpler copy of a mesh
named `<name>_LOD1` (a `Body_LOD1` for `Body`), skinned to the same
armature or parented to the same bone; a mesh with no `_LOD1` copy then
draws as it is at every distance.

## 3. Weapons and other held items

A static mesh (no armature needed) with two Empties:

| Empty name | What it is |
|---|---|
| `mountPoint` | where the hand grips it |
| `muzzlePoint` | the barrel's mouth: where shots and the flash come from |
| `ejectPoint` (optional) | where shells fly out |

The barrel points at -Y (toward you in Front view). The output model is named
from a `weapons.json` item and image `model` (see
`docs/modding/weapons.md`).

**Sounds and effects** are separate files beside the model in the Add-On
folder; the importer converts only the model. Sounds are `.wav` or `.ogg`
under `assets/sounds/`, named in `weapons.json` under `sounds` and played by
an image state's `sound` (equip, fire, reload, empty). A muzzle flash or
tracer texture is a PNG under `assets/textures/`, listed in
`assets/presentation.json` and named by a particle's `texture` in the pack's
`effects` (see `docs/modding/weapons.md`).

## 4. Vehicles

The importer converts the model and keeps every node; the vehicle's handling,
seats and wheels are written in `vehicles.json`. Name these Empties so the
report can list their positions for that file:

| Empty name | What it is |
|---|---|
| `mount0`, `mount1`, ... | seats in order: `mount0` is the driver |
| `hub0`, `hub1`, ... | wheel centres, suspension fully compressed |
| meshes named `col_...` | simple convex collision hulls (boxes are best); not drawn |
| `muzzlePoint` | a mounted gun's barrel mouth |
| `pivot` | the point a mounted gun swivels about |
| `turret` | where a separate turret model sits on the hull |

Wheels are separate models (one tire `.glb`, axle along X).

**Mounted guns.** The gunner is another seat (`mount2` on a Warthog), marked
as the weapon seat in `vehicles.json`; the gun fires from `muzzlePoint`
where the gunner looks, swivelling about `pivot`. A gun fixed to the hull
needs nothing more. A gun that visibly turns (the Warthog's chain gun) is
its own `.glb`, imported with `--kind vehicle`, with its own `mount0` (the
gunner), `muzzlePoint` and `pivot`; the hull gets a `turret` Empty where it
sits. It turns left and right with the gunner's aim; tilting the barrel up
and down needs a `look` clip, which glTF turrets do not support yet. The
gun's sound goes in the vehicle pack's `weapons.json` `sounds`, as above.

## 5. Textures and materials

- One **Base Color** image texture per material, PNG or JPEG, up to 2048x2048
  (1024 is plenty). The Base Color factor multiplies it; a material with no
  texture becomes its plain colour.
- Normal, roughness, metallic, emission and occlusion maps are ignored and
  listed in the report. Vertex colours and shape keys are ignored too.
- Alpha: Blend mode "Alpha Blend" draws see-through; "Alpha Clip" and
  "Opaque" draw solid.
- An "Unlit" (emission shader / KHR_materials_unlit) material draws unlit.

## 6. Export settings (File > Export > glTF 2.0)

- Format: **glTF Binary (.glb)** (one file with textures inside).
- Include: Selected Objects or Visible Objects; include Custom Properties off.
- Transform: **+Y Up on**.
- Mesh: Apply Modifiers on, UVs on, Normals on; Vertex Colors off.
- Armature: Export Deformation Bones Only off (sockets parented to non-deform
  bones still need them).
- Animation: on; Mode **Actions** (Blender 4.x) so every action is exported.
  Give each action a fake user (the shield icon) so it is not dropped.
  Sampling on, rate 1.
- Shape Keys: off.

## 7. Rights

A server sends its Add-Ons to everyone who joins, so a body made from
another game's model is shared with every player. Set the package's
`license` and `provenance` to what you may share; the importer writes
`proprietary` until you do.

## How it converts

glTF faces +Z and the game faces -Z, so the model turns half a turn about
Y. Native nodes have no scale: a node's even scale moves into its
children's positions and its meshes, and an uneven one is kept only on a
mesh without children. Clips are sampled at `--fps`; a loop's last key
wraps to its first. `look`, `head_side` and `head_up` are stored as
additive overlays relative to their middle (or first) frame, as the
Blockhead's are; channels a clip leaves at the rest pose are dropped so
they do not override the layers below. Socket nodes (`Mount0`, `Mount1`,
`Eye`) are added as children of the empty or bone they follow, turned so
their axes are the body's in the rest pose. The distant detail is a
second detail level (`detail100` and `detail0`, each object with a second
mesh) made by quadric-error edge collapse: a vertex moves onto a
neighbour and takes its weights and UVs, a seam's copies move together,
and no collapse crosses between parts on different bones. The client
picks a body's detail each frame from its height on screen. Code:
`crates/addon-import/src/gltf/`; tests: `crates/addon-import/src/gltf/tests.rs`
and `crates/client/tests/gltf_body.rs`.
