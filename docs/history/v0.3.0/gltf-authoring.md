> **Historical note. Written 2026-10-13 (the file's date in the cloud project folder, `v0.3.0/gltf-authoring.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Preparing models in Blender for Blockland ReImagined

This is the contract the glTF importer (v0.3.0 W5) reads. Prepare characters,
weapons and vehicles against it now; the importer turns a `.glb` into a
native Add-On model. The command will be:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- gltf mario.glb out/mario --kind body --name "Mario"
cargo run -p bri-addon-import --bin bri-import-addon -- gltf ma5b.glb out/ma5b --kind item
```

Anything the importer can't use is named in its report, with the fix.

A body comes out as a whole Add-On that everyone draws, so friends who join
your game download it (the model and its textures) and see you as Mario.
Items and vehicles go into your weapon or vehicle Add-On, which joiners
download the same way.

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
- Alpha: Blend mode "Alpha Blend" and "Alpha Clip" draw see-through
  where the texture is; "Opaque" draws solid and ignores the texture's
  alpha (a specular or colour-change mask left there does no harm).
- Every material takes the map's light, sun and shadow as a Blockhead
  does. Only an "Unlit" (emission shader / KHR_materials_unlit) material
  draws at full brightness whatever the light: keep that for glowing parts.

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

## 7. Assets and rights

Models extracted from other games are fine to prepare and test on your own
PC. They never go in the repository, and since the game sends a server's
Add-Ons to everyone who joins, hosting them publicly is a distribution
question the importer can't answer. Its tests use only original or openly
licensed models (the Khronos glTF samples).
