# Models from Blender (glTF)

`bri-import-addon gltf` turns a glTF 2.0 model (`.glb`, or `.gltf` with its
files) into the game's native model: a player or bot body with its own
skeleton, clips and textures, a held weapon, or a vehicle or prop. Blender
exports glTF from File > Export > glTF 2.0; so do Blockbench, Maya and most
other tools.

The release includes `bri-import-addon`: run it directly with the arguments
after `--` in these source-build examples. Use `.\bri-import-addon.exe` in
PowerShell, `./bri-import-addon` on Linux, or the tool inside
`BlocklandReImagined.app/Contents/MacOS/` on macOS. Rust is only needed to
build the tools from this checkout.

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
| `map` | a whole map Add-On: `package.json` and `map/` with the scene, model, PNGs and `bundle.json`; `spawn` empties are spawn points and lights are map lights | turn it on and pick it in Start Game ([Making maps](maps.md)) |
| `brick` | a brick Add-On: `package.json` and `assets/brick-catalog/` with the brick (`<id>.brick.json`, the format converted v20 bricks use), its catalog entry and its collision | turn it on; the brick is under Special > Models in the brick menu ([Bricks](#bricks)) |

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

## Bricks

`--kind brick` makes one brick from the whole model:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- gltf table.glb addons/table --kind brick --name "Table"
```

It is an ordinary brick once the Add-On is on: placed from the brick menu
(Special > Models), painted, saved in builds, sent to players who join, and
wired with events like any other brick, `onActivate` included.

- **Size.** The brick's grid box is the model's bounds rounded up to whole
  studs (0.5 units) across and plates (0.2 units) up, with the model
  centred in it. A 1 x 1 x 1 unit table is a 2 x 2 brick 5 plates tall.
  Model at 1 Blender metre = 1 unit, or pass `--scale`.
- **Look.** Every face draws in the paint colour the player picks; the
  model's textures and material colours are not used, and nothing animates.
- **Collision.** Each `col_` mesh is a collision box the size of its
  bounds. With none, the brick collides as its whole grid box.
- **Bricks on it.** Like a basic brick, it has studs over its whole top and
  takes bricks under its whole bottom.

## 2. Bodies (characters)

**Rig.** One armature, any bone names. Skinning is read from vertex groups
(up to 4 bones per vertex; extra weights are dropped and reported). Rigid
parts parented to a bone also work. Keep it under about 20,000 triangles and
128 bones.

**Sockets.** Add an Empty (Plain Axes) for each, parented to the bone that
carries it, placed where it should be in the rest pose. Their rotation does
not matter: the importer lines each socket up with the body, and the engine
holds every weapon level as the Blockhead does whichever way the hand bone
points (see "Grips on imported bodies" in `content-kinds.md`).

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
| `side_right` | strafing right | a loop. If missing, the body plays its `side` clip reversed |
| `jump` | on takeoff | once |
| `fall` | in the air | a loop; if missing it plays `jump` |
| `crouch` | crouching down | once; its last frame is the crouched pose |
| `crouch_run`, `crouch_back`, `crouch_side` | moving while crouched | loops |
| `crouch_side_right` | strafing right while crouched | a loop. If missing, `crouch_side` plays reversed |
| `prone`, `prone_run`, `prone_back`, `prone_side`, `prone_side_right` | lying prone (`movement.prone`, `set_prone`) and crawling | loops that pose the lying body themselves. With a `prone` clip the body is not laid face down by its posture frame; without one it is, as before |
| `aim_right`, `aim_left`, `aim_both` | holding an item in that hand, or both | a held pose: the body holds the last frame. Key only the arm bones |
| `sit` | seated in a vehicle or on a rideable | a held pose; one frame is enough. If missing it sits in `idle` |
| `death` | on death | once; it holds the last frame |
| `swim` | in water | a loop |
| `look` | looking up and down | first frame looking straight up, middle frame level, last frame straight down. Key only head, neck, spine and arms |
| `look_crouch` | looking up and down while crouched | the same frames as `look`. If missing a crouched body uses `look` |
| `look_prone` | looking up and down while prone | the same frames as `look`. If missing a prone body uses `look_crouch` |
| `head_side` | free look | first frame head turned fully left, middle forward, last fully right |
| `head_up` | head tilted up under a pack | first frame neutral, last tilted |

`look`, `head_side` and `head_up` are layered on top of everything else (the
importer stores them relative to their middle or first frame), so a running
Mario still looks up. Any other action (`stomp`, `wave`, `ground_pound`) is
kept under its own name for rules to play. Blockland names also work
(`root`, `armreadyright`, `death1`).

A clip that is not layered (`aim_right`, `sit`, `crouch` over `run`)
takes over exactly the bones it has keys for, even a bone it keys at rest:
key the off hand still in `aim_right` and the run's swing of that hand stops
while aiming. Bones it has no keys for keep following the clip below, so
don't key the legs in an aim clip. Blender's exporter can drop keys that
never change (Animation > Optimize Animation Size): turn that off if a
"hold still" bone goes missing from a clip.

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

**Prone and lean (optional).** `movement.prone` lets the body lie down
(`set_prone` in rules): `{ "height": 0.5, "eye": 0.35, "seconds": 0.7, "forward": 1,
"backward": 0.5, "sideways": 0.5 }`, its box height (at most the crouch's),
eye height, the seconds the first-person eye takes to go down or come up
(0: as fast as a crouch) and crawling speeds. A lying body does not jump, and
neither does a body whose rules set its speed scale to 0. The body is drawn face down along its
facing, its middle over the box, so its head and feet reach past the box:
a lying body is hit only around its middle. `movement.lean` lets it lean
(`set_lean`): `{ "distance": 0.5, "angle": 0.3, "seconds": 0.25 }`, how
far the eye slides sideways (at most half the box's width, so a lean
never sees past the body it can be shot at), how far the view and the
body tilt (radians, up to 0.6) and how long a full lean takes. An optional
`body_angle` tilts the drawn body by that much instead (0 leaves it upright
while the view still rolls). v20's bodies have neither.

**Getting out of geometry (optional).** `movement.unstick` frees a body that
a door, a lift, a prop or a placed brick has closed around:
`{ "seconds": 0, "probe": 0.55, "reach": 1, "step": 0.5 }`. A body inside
solid geometry for `seconds` (0: at once) is moved, each way tried only if
the body then fits: straight up (sunk into a floor), then by a vote of four
probes around it (thin slices at the middle, `probe` of the body's radius
away, default 0.55; at most two free ones count, the body moves by the sum of
their offsets and twice as far with just one), then to the nearest free spot
within `reach` (tried every `step`, sideways and upward). It falls from there.
Other players never count, as they move aside. v20's bodies have none.

**Momentum (optional).** `movement.momentum` makes the body keep its speed:
`{ "acceleration": 81.92, "friction": 8, "stop_speed": 0.4, "air_control": 0.35, "crouched_share": 0.5, "walking_share": 0.5 }`.
A held move adds `acceleration` a second on the floor and turns the velocity toward the
move at `friction` a second; the move's own speed still caps it. Let go and the floor
takes `2 x friction` of the speed each second until the body is under `stop_speed` (then it
stops dead). In the air a held move adds `air_control` of the acceleration, and a body already
at the move's speed cannot gain more, and a body whose way one step ahead is blocked cannot steer at all (above 0.05). `crouched_share` (default 1) scales only the floor acceleration (never the speed cap, which is the move's own speed, so a crouch speed is set in `crouch_forward` and the others, not here) while crouched or lying, and `walking_share` (default 1) while the body is walking (rules call `set_walking`). v20's bodies have none: they are steered straight
to the move's speed by `movement.acceleration`.

**Walking down stairs (optional).** `movement.follow_floor: true` keeps a walking body on the floor when it falls away under it
by no more than `step_height` (down a stair, along a slope): it is lowered onto the floor below at once instead of dropping step by
step, so it keeps its speed and its grip. A jump or a rise still leaves the floor. v20's bodies fall off every edge.

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
| `supportPoint` (optional) | where the left hand holds a two-handed gun; without it the left hand goes halfway from `mountPoint` to `muzzlePoint` |

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

**Live digits.** A counter on the gun itself (rounds left on a rifle's
display) is an object per digit in the converted `.shape.json`, each with
a `live` block: `{ "value": "rounds", "place": 0 }` for the ones,
`"place": 1` for the tens. The object's material frame `d` shows digit
`d`; with `"uv_step": [0.1, 0]` the mesh needs only one set of texture
coordinates, on the strip's `0`, and digit `d` moves them `d` steps (a
strip of `0123456789` across a texture). `"blank_leading": true` hides a
digit with no value yet (no leading zero); `scale` multiplies the value
first. Values are `rounds` (in the magazine), `reserve` and `magazine`
(a full one), from the holder's own gun; other players' copies keep
their authored frame.

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
and down uses an authored `look` clip driven by the gunner's pitch. The
gun's sound goes in the vehicle pack's `weapons.json` `sounds`, as above.

## 5. Textures and materials

- One **Base Color** image texture per material, PNG or JPEG, up to 2048x2048
  (1024 is plenty). The Base Color factor multiplies it; a material with no
  texture becomes its plain colour.
- **Full materials**: a material's **Normal** map, **Metallic/Roughness**
  texture (glTF packs metal in blue and roughness in green, as Blender's
  exporter writes it), **Occlusion** map and **Emission** texture come
  along with it, each up to 2048x2048. The surface then draws physically
  based: bumps catch the sun, metal reflects the world at its roughness,
  creases darken, and the emission glows in the dark. The Normal Map
  Strength, Metallic, Roughness and Emission Strength values apply as in
  Blender. Each map is saved as a texture named after the material with
  `_normal`, `_metal_rough`, `_occlusion` or `_glow`. Vertex colours and
  shape keys are ignored.
- **Metal**: a material without those maps whose Metallic value is
  0.5 or more draws as bare metal, at its Roughness value, reflecting the
  world. Its Base Color is the metal's colour (steel about 0.56, gold 1.0
  0.71 0.29).
- **Glow**: a material whose Emission Strength times its brightest Emission
  colour channel is 0.5 or more and has no Emission texture draws at full
  brightness, like an Unlit one. Fainter glow, or an Emission texture,
  glows on top of the lit surface.
- Alpha: Blend mode "Alpha Blend" and "Alpha Clip" draw see-through
  where the texture is; "Opaque" draws solid and ignores the texture's
  alpha (a specular or colour-change mask left there does no harm).
- Every material takes the map's light, sun and shadow as a Blockhead
  does. Only an "Unlit" (emission shader / KHR_materials_unlit) or glowing
  material draws at full brightness whatever the light: keep that for
  glowing parts.

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


## Texture coordinate motion

A material may supply `extras.bri_uv_motion` (native shape material `uv_motion`):

```json
{"bri_uv_motion":{"scale":[1,1],"offset":[0,0],"scroll":[0.1,0],"center":[0.5,0.5],"rotation_degrees":0,"rotation_rate_degrees":180}}
```

The renderer scales the model's UV, rotates about `center`, then adds `offset`
and `scroll * seconds`. Rotation and rotation rate use degrees and degrees per
second. Unspecified fields default to identity/zero. The cosmetic render clock
is shared across world views; mesh positions and lighting UVs stay unchanged.
The same operation works in Classic and Dynamic presentation. Invalid/nonfinite
motion is rejected. Static-map glTF export preserves this metadata and keeps two
materials distinct when they share an image but have different motion.

Two optional additions come from Unreal Engine 2's texture modifiers:

- `waves`: two entries (u, then v) of `{"kind":"pan"|"stretch"|"stretch_repeat"|"jitter",
  "rate":cycles per second,"phase":fraction of a cycle,"amplitude":size}`. A pan slides the
  texture back and forth, a stretch scales it about `center`, `stretch_repeat` runs a quarter
  sine per cycle and restarts, and `jitter` shakes it by a random offset redrawn 60 times a
  second.
- `rotator`: `{"base_degrees":[pitch,yaw,roll],"rate_degrees":[..],"wave_amplitude_degrees":[..],
  "wave_rate":[..],"wave_phase":[..]}`. Each axis angle is `base + rate * seconds + amplitude *
  sin(2 pi (fract(wave_rate * seconds) + wave_phase))`. The picture is turned by the upper-left
  two-by-two of the rotation matrix those angles make, about `center`; yaw is the in-plane
  turn. It replaces `rotation_degrees` and `rotation_rate_degrees` when present.

A material may also supply `extras.bri_uv_chain`: an array of motions, outermost first, for
several texture modifiers around one picture (a rotator around a panner around a texture).
They are multiplied out each frame into one map, `acc = acc x motion` in the order given,
so the first motion acts on the coordinates first; when present it replaces `bri_uv_motion`.

This is a single UV transformation per material. Independent layer combinations,
meter thresholds and live per-instance shader inputs are separate capabilities;
texture motion alone does not implement them.

## Preserving an authored hand pose

Imported bodies receive generic hand fitting by default. If a rig already has
its own correct hand frames and hold clips, its archetype can supply:

```json
{"sockets":{"preserve_hand_pose":true,"right_hand":{"node":"socket_right_hand"},"left_hand":{"node":"socket_left_hand"}}}
```

This uses the named raw marker frames and leaves arm clips/support placement as
authored. The importer retains the raw nodes alongside its leveled Mount children;
choose the actual intended attachment frame. It does not repair an incorrect
marker or invent missing animation. Stock-tool first-person fallback remains
available. One- and two-handed weapon roles are sampled separately for the host's
standing muzzle cache, following the image's both_arms setting. Dynamic/seated
shot pose parity needs separate verification when authoring a custom rig.

Animated alpha-mask shadow casters still use their static UVs. The texture motion
feature above currently covers visible materials; do not rely on animated cutout
shadow silhouettes.

## Authored crouch locomotion

The default body composition layers the dedicated `crouch` thread over movement,
holding its final pose while crouched. Imported bodies whose crouched idle and
movement clips already contain the complete crouch pose can opt out of that layer
at the archetype root:

```json
{"crouch_composition":"locomotion"}
```

This preserves the selected crouch idle/run/back clip and its playback phase.
The default `"overlay"` retains the stock Blockhead behavior. Ready, aim and action
layers still compose normally; this setting does not suppress those channels or
repair an incompatible hold clip. Collision, eye height and stance controls are
unchanged. Hidden or box-body Blockhead stand-ins retain the legacy overlay.
The resolved archetype carries this policy in `look.crouch_composition` for clients.
