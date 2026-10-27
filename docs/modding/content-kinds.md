# Other content kinds

Each file in `provides` has a `kind`. Which kinds an Add-On provides decides
who needs it: only host kinds means the host only; only `model`, `hud`
and `binds` means each player; anything else (weapons, bricks, blocks) means everyone.
An Add-On cannot mix host kinds with `model` or `hud`: split it in two, the
visuals depending on the rules. `bri-addon-check` tells you which it is.

| Kind | Needed by | What it is | Example |
|---|---|---|---|
| `behaviour`, `script` | host | a game rule ([Game rules](rules.md)) | `packages/samples/sample-survival-points` |
| `world` | host | a generated chunk world: materials, a `generate(cx, cz)` function | `packages/stresslab/stresslab-world` |
| `entity` | host | a scripted creature: model, `think` function, speed, health | `packages/stresslab/stresslab-creeper` |
| `archetype` | host | a playable body: movement, collision `box`, `ball` or `cylinder` (a round body turns aside at a wall end or door frame where the box stops dead), steering, health, riding, model, camera distance, `camera` profile and `sounds` | `crates/sim/tests/unlike_modes.rs` |
| `controller` | host | a movement controller: a WebAssembly module an archetype names as its `controller`; clients receive it with the archetype table ([Movement controllers](../architecture/movement-controllers.md)) | `packages/samples/sample-wall-kick`, `packages/samples/sample-dive`, `packages/samples/sample-trooper` |
| `mode` | host | a Start Game game mode: name, the Add-Ons it runs, a map, and its own mini-game | `packages/stresslab/stresslab-mode`, `crates/sim/tests/mode_and_voxels.rs` |
| `model` | each player | a box model for an entity | `packages/stresslab/stresslab-creeper-model` |
| `hud` | each player | a HUD panel, or a layout that claims a HUD region and draws textured elements in it ([HUD panels and regions](hud.md)) | `packages/samples/sample-points-hud` |
| `binds` | each player | keys for an Add-On's commands, under its own heading in Controls (below) | `crates/addon-import/ports/tool_newduplicator/files/binds.json` |
| `weapons` | everyone | weapons ([Weapons](weapons.md)) | `packages/samples/sample-bubble-blaster` |
| `bricks`, `vehicles` | everyone | written by Import Add-On ([Old v20 Add-Ons and new bricks](old-addons.md)), or a `vehicles.json` you write (fields below) | `packages/showcase/steel-ball-kit` |
| `bots` | everyone | bots a Vehicle Spawn brick can hold: name and how they play (fields below) | `packages/blockhead_bot` |
| `body` | everyone | a player body: a native shape (`.shape.json`) an `archetype` names as its `model`, its PNGs listed in `assets/content.json`. Carried with its archetype, it makes the Add-On everyone's, so joiners download the body and see it ([glTF models](gltf-models.md)) | `crates/client/tests/add_on_join.rs` |
| `map` | everyone | a map in Start Game: a native map bundle's `bundle.json` holding one map, `<id>:map/<name>`, with its scene, models, textures, spawn points and lights beside it ([Making maps](maps.md)) | `packages/samples/sample-map` |
| `texture`, `block` | everyone | a PNG for block faces or HUD atlases (4096 px per edge, 4M pixels total, 4 MiB encoded); textures or flipbooks per face with named states. Textures share a 64 MiB decoded RGBA budget per Add-On | `crates/sim/tests/blocks.rs` |

Entities may spawn only their own Add-On's entity kinds.

`binds.json` gives players keys for an Add-On's commands, as a v20
client script's `$RemapName` entries did:

```json
{
  "schema_version": 1,
  "division": "New Duplicator",
  "binds": [
    { "name": "Copy Selection (Ctrl C)", "package": "tool_newduplicator-rules",
      "command": "ndcopy", "key": "ctrl c", "mac_key": "cmd c" },
    { "name": "Multiselect (Ctrl, Hold to use)", "package": "tool_newduplicator-rules",
      "command": "ndmultiselect", "key": "lcontrol", "hold": true }
  ]
}
```

Up to 128 binds go under `division` in Options → Controls, where players
can rebind them. A bind sends its package's command as if typed: one with
no arguments when pressed, or with `"hold": true` a command declared with
`"args": ["bool"]`, sent `true` as the key goes down and `false` as it
comes up. Commands a bind sends may not be `tool_only`. `key` (and
`mac_key` on a Mac) is bound by default when neither the bind nor the key
is taken, as the New Duplicator's own keys were; a key the player binds it to
instead is kept. Binds show only while the host
runs their package.

**Actions and control profiles.** A bind is an *action*: a named command
with a suggested default input. Add-Ons never read keys; players rebind the
action in Options → Controls. `key` (and `mac_key`) is the default key, `"alt_key"` a second
default key for the same action (AA fires on Ctrl as well as the mouse; it
is bound with the first and not listed apart), and
`"mouse"` a default mouse input instead (`button0` to `button4`, or `zaxis`
for the wheel; each wheel notch is one press). Mouse movement is not an
action input.

Actions belong to a *control profile*. Blockland's own is the default
profile, and a bind that names none goes in it, as above. An Add-On declares
more in the same file and puts its binds in one with `"profile"`:

```json
{
  "schema_version": 1,
  "division": "Arena Assault",
  "profile": "aa",
  "profiles": [{
    "id": "aa", "name": "Arena Assault",
    "player_types": ["aa-rules:archetype/soldier"],
    "mouse_sensitivity": 0.75,
    "layers": [{ "id": "aircraft", "when": "mouse_driver", "invert_mouse": true }]
  }],
  "binds": [
    { "name": "Sprint", "package": "aa-rules", "command": "sprint",
      "key": "lshift", "hold": true },
    { "name": "Fire Mode", "package": "aa-rules", "command": "firemode", "key": "v" }
  ]
}
```

- **Player types reference a profile.** `player_types` lists archetype ids
  (`your-id:archetype/soldier`).
  The active profile switches by itself with the local player's body, back
  to Blockland's for any other.
- **Layers.** A layer stacks on the profile while a rider matches `when`
  (`vehicle`, `driver`, `mouse_driver`, `passenger`) and is removed when they
  leave. Its `invert_mouse` (in vehicles) and `mouse_sensitivity` replace
  the profile's for as long as it is on.
  A layer's `when` may instead be `"rule:your-id:player/key"`: it is on
  while that rule value on the player is on (a number other than 0, `true`
  or a non-empty string), as a radio menu a rule has open. Binds with
  `"layer": "<layer id>"` (a rule layer of the file's profile, with a `key`)
  work only while it is on, and then win over the profile's own binds on the
  same key, so `1` to `9` can send radio lines while the menu is open and go
  back to weapon keys when it shuts. A layer's binds are press-only, fixed to
  their inputs and not in Controls (it is a mode, not a setting). Each takes
  a `key` or a `mouse` button (`button0` to `button4`, or `zaxis`), not both,
  so the left button can pick a radio line; no `hold`, `stock` or `screen`.
- **Inheritance.** Inputs a profile does not bind fall through to
  Blockland's (movement, chat, menus), so a profile lists only its own
  actions; its binds win over the same input there. `"inherit": false`
  makes a profile replace every input.
- **Moving a stock action.** `{ "name": "Crouch", "package": "aa-rules",
  "stock": "crouch", "key": "lcontrol" }` binds Blockland's own Crouch in the
  profile (also `moveforward`, `movebackward`, `moveleft`, `moveright`,
  `jump`, `jet`, `walk`, `mousefire`), so a profile that takes Shift for
  Sprint can give Crouch another key. It needs a `profile` and no `command`.
- **Controls menu.** A "Controls for" dropdown lists Blockland and every
  declared game and tags the one in use; opening Options mid-game lands on
  it. A game's list is its own actions, then the Blockland actions that
  still apply (grey group headings "Blockland: ..."; changing one binds it in
  this game only, and an input a game binds shows "Taken" on the Blockland
  action it replaces). Blockland actions a game has no use for are hidden:
  Building, Tools / Inventory and Drop Camera/Player follow a `building` HUD
  claim that applies to the profile's `player_types`; Jet follows the body's
  `can_jet`. An action with no key, or whose key another action took,
  draws red in the list. Clear All, Remap All and Defaults name the game and touch only it
  (Defaults restores each action's declared key and drops overrides). A
  layer's optional `name` ("In aircraft") labels it in the mouse panel's
  "Mouse while" dropdown. Rebinds, mouse sensitivity and invert are saved per
  profile (and per layer), and a conflict is flagged only within the profile
  being edited. Mouse sensitivity and invert declared in `profiles` are
  defaults; the player's own choice wins. Choices for a profile whose Add-On
  is not running are kept.
- Profile ids and layer ids are lower-case letters, digits, `_` and `-`;
  `blockland` is reserved. Existing binds and saved settings are unchanged.

**A mode's own mini-game.** A `mode` may carry a `minigame` block, and the
host then runs that one mini-game for the whole server, as v20's game-mode
servers did: everyone joins it on arrival, and nobody can make, join or
leave another (the Mini-Games screen says the server runs it). It owns the
world's own bricks, so its brick damage reaches them.

```json
"minigame": {
  "title": "Dig Off",
  "loadout": ["dig-kit:weapon/spade", "v20.weapon.gunitem"],
  "player_type": "v20.player.playernojet",
  "respawn_seconds": 5,
  "brick_respawn_seconds": 30,
  "self_damage": false,
  "building": false
}
```

| Field | Default | Meaning |
|---|---|---|
| `title` | the mode's name | the name in the Mini-Games list |
| `loadout` | none | up to 5 items everyone spawns with |
| `player_type` | the stock player | the body everyone plays |
| `respawn_seconds` | 5 | 1 to 30 |
| `brick_respawn_seconds`, `vehicle_respawn_seconds` | 30, 5 | how long a broken brick or vehicle stays gone |
| `points_kill_player`, `points_kill_self`, `points_die`, `points_break_brick`, `points_plant_brick` | 1, -1, 0, 0, 0 | v20's score settings |
| `falling_damage`, `weapon_damage`, `self_damage`, `vehicle_damage`, `brick_damage`, `building`, `painting` | all `true` | what the game allows |
| `use_all_players_bricks` | `false` | let the game break everyone's bricks, not only the world's |
| `bodies` | none | bodies players may pick for themselves with `/body`, besides `player_type` (archetype ids) |
| `spawn_protection_seconds` | 2.5 | 0 to 60: how long a new body takes no harm; 0 turns it off |
| `vehicle_return_seconds` | each vehicle's own | 0 to 3600: how long a brick-spawned vehicle may sit empty away from its brick before it goes back as a fresh one; 0 never |
| `death_view_seconds` | 0 | 0 to 30: how long the dead keep the camera on their body, looking where it fell, before they can look around |
| `hold_first_spawn` | `false` | a player who joins has no body until the rules release their spawn with `respawn(p)`, or a game reset spawns every member; they wait as the dead do (a menu to pick a team or role first). `hold_respawn(p, false)` only lifts the click block and spawns nobody |
| `keep_chosen_body` | `true` | a player who picked a body (a brick's ChangeDatablock, `/body`), before joining or in the game, respawns as it rather than as `player_type` or their team's body; `false` spawns everyone as the game says |

Teams are not a mini-game setting (v20 had none): a rule keeps each
player's team in a state key, refuses friendly fire in `on_damage` and
dresses teams with `set_avatar_colors`.

An archetype's `model` may be a package model, a v20 shape, or `"none"`
for no drawn body (client code can then draw its own). Package models draw
at the body's scale, and an entity takes a `scale` from 0.2 to 4. The
Commando's [archetype](../../packages/samples/sample-commando/commando-archetype.json)
is a whole new body in a dozen lines: no jet, 150 health and faster feet.
`movement` accepts any of the motor's constants by name (`gravity`,
`jump_speed`, `air_control`, `step_height` and the rest); `set_archetype`
switches a player between bodies at any time.

Every way of changing body is one transition: `set_archetype`, a brick's
`onActivate -> Player -> ChangeDatablock` (whose list holds v20's player
types and every named Add-On body), `/body <name>` and a mini-game's or
team's body. The player keeps their place, motion, scale and the same
share of health; a new movement controller starts afresh; a body that
cannot ride (`can_ride: false`) is set down beside its seat, and one that
can stays seated and later gets out with room for the body it is now; a
body that uses no items (`uses_items: false`) puts its tool away and keeps
it. A brick or `/body` refuses a bigger body where it would reach into a
brick or the map, and says so; an Add-On's `set_archetype` does not. A
body picked by a brick or `/body` is kept through respawns outside
mini-games and into a game that keeps chosen bodies (the default, so a
dialog-made game lets Mario and Master Chief fight); a game with
`keep_chosen_body: false` ends the pick. An Add-On's choice is kept
everywhere, and the latest choice wins. `/body` alone lists the
bodies one may pick: outside mini-games any named body, inside one its
`player_type`, the player's team's body and its `bodies`. `push_archetype(p, a)` lays
another body of the same model over theirs for a while (a machine gunner
slowed as they fire; v20's `pushDatablock`), keeping their damage, and
`pop_archetype(p, a)` lifts it; `set_archetype` meanwhile changes the body
underneath, and death lifts them all. An archetype with
`"first_person_only": true` keeps its player's view in the eye whatever
their camera toggle says (v20's `firstPersonOnly`: Tier 2's slowed
machine gunner), and the toggle takes over again once the body changes.

**Sockets and animations.** A body says where it holds things, sees from
and sits with `sockets`: `right_hand`, `left_hand`, `eye` and `seat`, each a
`node` of its model, an `offset` (feet at the origin, facing -Z), or both.
A rigged body names its own nodes; a box model or `"none"` body has no
nodes, so it gives offsets. Missing sockets fall back to the Blockhead's
`Mount0`, `Mount1` and `Eye` on a rigged body; a box body without a hand
socket hides what that hand holds, and without a `seat` it sits upright.
Every gun fires from its muzzle at the hand socket and converges on the
crosshair, falling back to the eye when the barrel is inside a wall. The
host reads an imported body's hand nodes from its package with the arm
raised to aim (its `aim_right` clip), as the Blockhead's.

**Grips on imported bodies.** An imported glTF body holds every image as
the Blockhead does, whatever way its hand bones point
(`crates/content/src/hold.rs`). The engine takes the hand socket's
position and ignores its turn: a held hand faces the Blockhead's aiming
hand (forward along -Z, up +Y), pitched with the look. An arm the body has
no `aim_right` (`aim_left`) clip for is raised in front of the chest by
two-bone IK, to the spot the Blockhead's arm reaches, scaled to the arm's
length. An arm with its own aim clip keeps that pose and only the hand
turns. A socket `offset` on a node is measured in that aiming hand's
frame, so the same image sits the same way in every body's hand. A
two-handed image (`aim_both`) pulls the left hand to its `supportPoint`
node (glTF `supportPoint` Empty), else halfway from the grip to the
muzzle. The host fires from the same fitted hand, pitching about the
shoulder. v20 shapes and the Blockhead keep their authored `Mount0` and
`Mount1`.

In first person an imported body draws its held images where the
Blockhead's are, at the same spot from the eye, so a gun sits in view the
same on every body. Its own arms are not drawn there: the view shows the
gun alone, as for v20 images with an `eye_offset`.

A rider whose body has a `sit` clip is moved so its hip sits where the
Blockhead's hip sits in its own `sit` pose; a seat authored for the
Blockhead seats every body. A seat tuned to one body's pelvis (a vehicle
from another game) still seats that body as it was tuned. Tests:
`crates/content/src/hold.rs`,
`the_blockheads_aiming_hand_is_the_frame_fitted_hands_take` in
`crates/client/src/avatar.rs` and `crates/client/tests/gltf_body.rs`.

**Animation libraries.** UE2 links animation sets to a mesh and `PlayAnim` finds a sequence by name without regard to case. An
archetype's `animation_libraries` (up to 8) lists asset ids (`package:asset/clips.shape.json`, an `asset` the library package lists in its
`assets/content.json`) of shapes whose clips join the body's, keyed by their original sequence names (a trailing space is part of a
name). The body's own clip wins over a library's of the same name; a library that is not enabled, or a clip on a bone the body lacks,
fails the body, named in the Add-On's health report. AA's `aa25-soldier-clips` carries all 409 Soldier_3P sequences this way
(`aa25/tools/build_soldier_clip_bank.py`). Test: `crates/client/src/avatar.rs`, `an_archetype_links_an_animation_library_by_original_names`.

**Movement controller intents.** An archetype's `intent_clips` maps the
animation intents a movement `controller` reports to the model's clips, and
`intent_sounds` to a sound played where the body is each time the
controller changes to that intent: `{"11": "triple_jump"}`. An intent
clip replaces run, jump and fall while the controller reports it; dying,
sitting and swimming still win, and a rider's intent is ignored.

**Blending.** Moving from one locomotion clip to another blends over a quarter second (v20's
`sAnimationTransitionTime`; the jump's is shorter). A thread a rule starts or ends on its own
(`play_thread` for a posture change or a hand action) cuts in as v20's do, unless the archetype
gives `action_blend`, in seconds (0 to 2): the whole pose then blends from what it was before the
thread started or ended over that time, so a posture clip does not snap to the next pose. AA's
`BlendChangeTime` is 0.25.

**Clip speed.** As in v20, the clip for `run`, `back`, `side` and the
crouched ones plays at the speed the body moves along the way it walks
(`|velocity . direction| / clip speed`), at most `max_time_scale` times
(1.5, v20's `maxTimeScale`): feet keep pace with the ground instead of
skating. A clip's speed at 1x is its own ground motion, which v20's
sequences carry. A clip without any (most glTF exports) keeps its own time
unless the archetype gives `ground_speeds`, in units per second at 1x:
`{"run": 9.0, "side": 6.0}`. Measure it as the distance one cycle's feet
travel divided by the cycle's length.

`look.animations` maps roles to the body's own clips: `idle`, `run`,
`back`, `side`, `side_right`, `crouch`, `crouch_run`, `crouch_back`, `crouch_side`,
`crouch_side_right`, `prone`, `prone_run`, `prone_back`, `prone_side`, `prone_side_right`,
`jump`, `fall`, `sit`, `aim_right`, `aim_left`, `aim_both`,
`look`, `look_crouch`, `look_prone`, `head_up`, `head_side`, `seat_aim`, `ladder_up`, `ladder_down`, `swim` and `death`. `seat_aim`
is a seated gunner's pose from aiming the gun at its highest (the clip's
start) to its lowest (its end): it holds at the gun's pitch across the
vehicle's `look_pitch` range in place of `look`, so the whole seated body,
knees included, bends with the gun. Unnamed roles use a clip of the
Blockhead's name if the model has one, then a fallback (`back` runs `run`
reversed, `fall` plays `jump`, `sit` plays `idle`), and a role with none
leaves the body in its bind pose for that layer. An archetype's `deaths`
picks among several death clips by where the killing hit came from:
`"deaths": { "front": ["die_back"], "back": ["die_front"], "left": [...],
"right": [...], "any": ["die_spin", "die_slump"] }`. Each list holds up to 8
of the body's own clips; the host rolls one of the side's (or of `any`
when the side has none or the death had no direction, a fall), so every
player sees the same one. A body that dies crouched or lying prone plays
one of `crouched` or `prone` instead, whatever the side: a body with `deaths`
must list `crouched`, and one with `movement.prone` must list `prone` (the
Add-On does not load without them, so a posture never falls in a standing
clip). A listed clip the model lacks is named in the Add-On faults log.
Without `deaths` the body plays its `death`
role. `ragdoll` makes the body go limp instead when it dies, drawn on each
player's screen only (nothing about it reaches the host or gameplay, and
every screen simulates its own, so they do not tumble identically):
`"ragdoll": {"bones": [{"node": "pelvis", "shapes": [{"kind": "capsule",
"radius": 0.12, "half_length": 0.1, "axis": "y", "offset": [0, 0, 0]}],
"mass": 0.22}, {"node": "thigh_l", "shapes": [{...}], "mass": 0.1,
"linear_damping": 1, "angular_damping": 0.5, "joint": {"swing": 0.5,
"swing_second": 0.2, "twist": 0.2, "damping": 1}},
{"node": "shin_l", "shapes": [{...}], "mass": 0.1, "joint": {"swing": 0,
"twist": 0, "hinge": [-1.5, 0]}}], "friction": 0.8, "max_speed": 40,
"max_spin": 10, "freeze": 10, "kick": {...}, "carry": [0.6, 1],
"falling": 0.2, "inertia": 4, "spin_scale": 13, "prone_scale": 0.35, "random_spin": 9}`. Each bone becomes a rigid body made of up to
8 `shapes` (a `capsule` lying along the node's `axis`, a `box` (optionally
turned by `rotation`, radians about x, y then z), a `ball`, each centred
`offset` from the node, or a `limb`: `{"kind": "limb", "radius": 0.05,
"length": 0.25}`, a capsule that spans the bone itself, from its node toward
the nearest listed bone that hangs from it, else its first child in the rig,
else on the way its parent ran, so the model supplies only sizes; `length` is
the straight middle's, and left out it is the bone's span, a bone with
nothing below it needing one) weighing `mass`, jointed at the bone's
node to the nearest bone above it in the skeleton that is listed. The first
bone is the root and has no joint; the others have a ball joint (`swing`,
optional `swing_second`, `twist`, radians, 0 to pi, round the pose it died in)
or a hinge (`hinge: [low, high]`), both about the node's x axis unless
`axis` (`"y"` or `"z"`) says otherwise, with `damping` (how much it resists
swinging or turning) and `twist_damping` (twisting; `damping` when left out).
Limits are hard unless a ball joint sets `cone_stiffness`: then `swing` and
`swing_second` are Karma's soft cone (AA2's `CONE_STIFFNESS` and
`CONE_DAMPING`, taken as stored): no stop, no force inside the cone, and past
it a spring of `cone_stiffness` (torque per radian past the limit) and a
damper of `cone_damping` (torque per radian per second) pull the swing back,
with the implicit spring-damper solve Karma uses. AA2 joints leave `damping`
at 0 (free swing is resisted only by the bones' own damping) and ignore the
twist stiffness and damping. The swing is read per axis (y, then z), a box
rather than Karma's ellipse. The body goes limp at once. `max_speed` and
`max_spin` cap every bone, `bounce` and each bone's `linear_damping` and
`angular_damping` shape its fall, and `freeze` seconds after it goes limp
hold it where it lies (0: never). The killing hit sets it moving:
`"kick": {"impulse": 10, "speed": 2, "up_kick": 0.1}` (speeds, units/s, along
the way the hit travelled: `impulse` given at the point struck, which turns
the body, `speed` to the whole body, `up_kick` added upward when it was not
already falling) unless the damage type carries its own `death_kick` in the
weapons pack. `carry` is how much of the speed it died at the body keeps
(along the ground, up or down); `inertia` and `spin_scale` turn the offset of
the hit from the body's centre into spin (`spin_scale` counts the offset
across the ground that much more than the height); `prone_scale` (0 to 1) is
what a shot that kills a prone body gives of all that, with no upward kick;
`falling` (units/s) is how fast down it must be going to get no `up_kick`; `random_spin` (radians/s) is the spin of a death with no direction. It lies as long as `corpse_seconds` and fades as `corpse_fade` says.
A body over the physics budget, or whose skeleton lacks a listed node, plays
its death clip as before and the Add-On faults log says why. A dead body lies 5 seconds (v20's `$CorpseTimeoutValue`) and then goes in a
puff; `corpse_seconds` (0 to 600) lets it lie longer, and `corpse_fade` (up to
`corpse_seconds`) makes it fade out over the last of those seconds instead of
puffing away. `corpse_stays: true` has it lie until its player spawns again (a round's reset), whatever `corpse_seconds` says. A player a rule holds out of the world (before a round's first spawn, out of lives) has no body at all: none is simulated, drawn or left as a corpse. The client names what a
body lacks once, in the Add-On faults log ("no `sit` animation; it plays
its `idle` instead"). The Playground's
[Runner](../../packages/playground/playground-runner/runner.json) and the
Commando are two box bodies with their own sockets. Tests:
`crates/client/src/avatar.rs` (`a_body_with_its_own_nodes_and_two_clips_…`,
`a_box_body_holds_at_its_own_hand_…`), `crates/sim/tests/playground.rs`
and `crates/client/tests/body_attachments_render.rs`.

**Camera.** A body owns how its player sees while it is theirs with
`camera`, so a heavy shooter can feel nothing like a Blockhead:

```json
"camera": {
  "fov": 72,
  "eye_offset": [0, 0.08, 0],
  "bob": { "height": 0.05, "side": 0.035, "stride": 3.2 },
  "sway": { "lag": 0.6, "max_degrees": 5, "settle": 6, "bob": 0.8 },
  "third_person": { "side": 0.9, "height": 0.25 }
}
```

- `fov` (30 to 140 degrees) is the body's field of view for a player at the
  stock 90; a player who set their own FOV gets the same ratio (72 seen at
  a 110 setting is 88). Aiming and zoom own the FOV while they run, and a
  rule's `set_fov` wins over the body.
- `eye_offset` moves only the first-person camera from the `eye` socket,
  right, up and forward along the heading (units at scale 1, within 2). Aim
  and shots stay at the eye. Eye height itself is the `eye` socket.
- `bob` dips the first-person camera `height` at each footfall and rocks it
  `side` across a cycle of two steps covering `stride` units, while walking
  on the ground; it eases in and out with speed.
- `sway` makes the held weapon trail the look by `lag` of each turn (0 to
  1), never more than `max_degrees`, settling at `settle` per second, and
  bob against the camera by `bob` times the camera's bob. `sway.walk`
  (`height`, `side`, `stride`, as `bob`) gives the weapon its own bob on
  screen, so the gun can bob while the view barely moves, as in Halo CE:
  `"sway": { "lag": 0.5, "walk": { "height": 0.03, "side": 0.025, "stride": 3 } }`
  with no camera `bob`. Client code drawing its own first-person model
  reads the same sway from `view`.
- `sway.look` (`side`, `up`, `rate`) shifts the held weapon on screen against a turn of the look: `side` units
  left at a turn right of `rate` radians a second (and the reverse), `up` units down at a pitch up at that rate,
  in proportion below it, settling at `settle`.
- `third_person` puts the on-foot chase camera's pivot over a shoulder
  (`side`, negative for the left) and higher (`height`); its distance is
  still `camera_distance`.

All of it scales with the body and applies only while that body is the
player's, on foot and not observing: a vehicle seat's camera, the
observer camera and a switch to another body each take over, and a body
with no `camera` sees exactly as the stock Blockhead does. The
[Trooper](../../packages/samples/sample-trooper/trooper-archetype.json)
has one; test `crates/client/tests/hud_regions_render.rs`.

**Sounds.** A body jumps, hurts and dies with its own sounds:

```json
"sounds": { "jump": "mario:sound/wahoo", "pain": "mario:sound/ow", "death": "" }
```

Each names a sound of your weapons pack's `sounds` (your own `.wav` or
`.ogg`, see [weapons.md](weapons.md); an Add-On with no weapons provides a
`weapons` pack holding just its sounds) or one of the base game's by its v20
name (`HorseJumpSound`). An empty name plays nothing, and one you leave out
keeps the base's: the Blockhead's jump, pain cry and death cry. Players who
join without your Add-On download its sound files with it, and everyone
hears the sound where the body is. The pain sound plays on hits that make
the Blockhead cry out (more than 10 damage). The
[stand-in wahoo](../../crates/client/tests/fixtures/stand-in-wahoo) gives
the standard player its own jump sound in two files; test
`crates/client/tests/add_on_join.rs`
(`a_guest_hears_an_add_ons_own_jump_sound_it_downloaded`). Importing a v20
`PlayerData` converts `jumpSound`, and `painSound`/`deathSound` under
`useCustomPainEffects`, to these.


Footfalls and landings are a body's own too:

```json
"sounds": { "footsteps": ["your-id:step_1", "your-id:step_2"],
            "stride": 3.2, "land": "your-id:land" }
```

`footsteps` (up to 8) plays one sound each `stride` units the body covers
on the ground (0.25 to 32; left out, 1.1 times its standing height), never
the same one twice running. Each body shuffles its current sound pool, playing
every distinct profile before reshuffling and avoiding a repeated sound at the seam;
changing surface or posture replaces the bag when the pool changes. `land`
plays where it lands after falling faster
than 4 units a second. Each client plays them for every body it draws, so
they cost nothing on the network; riders and sitters make none. v20's
Blockhead has neither; ReImagined's base Blockhead uses the authored contact
policy in `assets/blockhead-footsteps/sounds.json`. `surfaces` gives other footfalls by the surface kind
under the feet (a map material's `bri_surface`, see Making maps; bricks are
`brick`, terrain `terrain`, the rest `default`), in place of `footsteps` on
that kind: `"surfaces": {"metal": ["your-id:clang_1"]}` (up to 16 kinds,
1 to 8 sounds each). Test: `crates/client/src/footsteps.rs`.

`steps` has the clips mark the footfalls instead of the stride timer, as an
animation's footstep notifies do: `"fps": 30, "steps": {"run_f": [{"frame": 4,
"foot": "foot_l"}, {"seconds": 0.52, "foot": "foot_r"}]}`. Keys are lower-case
clip names (up to 32 clips, 16 marks each); a mark says when by `seconds` or
by `frame` (counted at `fps`, 1 to 240, required for `frame`), and `foot`
names the body's node that lands, so the sound plays there and on the
surface under it (left out: at the body's feet). The sound is picked from
`footsteps` or `surfaces` as above, by every client, for players and bots
alike, as the clip plays through the mark (forward, backward or round a
loop). A body with any `steps` makes no timer footfalls, so a clip that
marks none is silent; a body without them (v20's Blockhead) keeps the
timer.

A mark can name `"sound": "your-id:climb"` (with `"volume"`, 0 to 4 times the
sound's own, and `"radius"`, the distance it is heard out to) to play that
sound source there in place of the surface's footfall, as a ladder's climb
does; it leaves no mark. `"postures": {"crouch": "your-id:down", "stand":
"your-id:up", "prone": "your-id:prone"}` sounds once where the body is
when it enters that posture.

`step_noise` (0 to 1) makes each footfall a clip marks a noise bots hear, as
they hear fighting, and a mark can say its own with `"noise"`; the gait shows
only in how often the clip's marks come, so a clip with none (backing up,
strafing, crawling) makes none. `land_noise` (`{"factor": 0.5,
"jump_floor": 150}`) makes touching down after a fall a noise of `factor`
times the speed fallen over the larger of the body's jump speed and
`jump_floor`, 0 to 1 (a fall at jump speed is `factor`, faster ones louder).
The host follows the clip itself (`bri-sim`'s `gait`: the same pick and the
same rate from `ground_speeds` as the client), so bots hear a footfall when a
client would see it land; scripted threads, intent clips, seats and water are
not followed. Who hears how far is the listener's own (its kind's hearing).

A mark's `volume` and `radius` apply to a surface footfall too, so one clip
can be as quiet as a crouched walk and another as loud as a sprint while they
share their sounds (a sound's own volume times `volume`, heard out to `radius`;
none leaves them as authored). `posture_steps` gives a posture footfalls of
its own, by posture (`stand`, `crouch`, `prone`, `climb`), as `footsteps` and
`surfaces` give the body's others: `"posture_steps": {"prone": {"footsteps":
["your-id:crawl"], "surfaces": {"metal": ["your-id:crawl_metal"]}}}`. Every
footfall of a body in that posture (a mark, the stride timer, a landing, a
take-off) picks from those for the surface underfoot, falling back to the
standing ones where the posture names none for it. `climb` is a body holding
a ladder: its `ladder_up` and `ladder_down` clips (played at the rate the
body gains height, `ground_speeds` `ladder_up` and `ladder_down` in units a
second, so one holding still holds its pose) mark their steps like any
clip, and they sound on the surface kind the map's climb volume names
(`surface`), say `"posture_steps": {"climb": {"surfaces": {"wood":
["your-id:ladder_wood1", "your-id:ladder_wood2"], "metal": [...]}}}`; a ladder
that names none sounds like the floor under the foot. Grabbing a ladder
sounds the `postures` entry for `climb`.

`takeoff_on_surface: true` makes the take-off of a jump sound once like a
footfall on the surface underfoot, as `land_on_surface` does the landing.
`contacts` makes each footfall (a mark, the timer's step, a landing or
a take-off that sounds as a footfall) leave something on the surface it
lands on: `"contacts": {"sand": {"effect": "your-id:dust", "decal":
{"texture": "your-id:print", "size": 0.4, "seconds": 20, "color": [1, 1, 1,
0.6]}}, "*": {...}}`. Keys are surface kinds, `*` for any other (up to 17); a
kind listed nowhere leaves nothing. `effect` is an effect burst at the foot,
`decal` a mark on the ground (size 0.02 to 4, 0.5 to 600 seconds, colour 0 to
1; its texture is a texture of the Add-On's weapon pack, and one that did
not load leaves none).

`clips` ties a sound to a moment of one of the body's clips, by the clip's
name: `"clips": {"h_kill_back_gut": {"sound": "your-id:body_fall",
"seconds": 0.57}}` plays the sound where the body is once the clip has run
`seconds` (0 to 60; up to 32 clips), when the body dies and starts it (the archetype's
`deaths`, or the `death` role); other clips do not play one yet. A body-fall
is one. Test: `crates/client/src/clip_sounds.rs`.

An archetype with `"adjusts": "v20.player.<datablock>"` (and no `base` or
`name`) is no new body: it changes the constants it names on one of v20's
own player types while the Add-On is on, as a v20 Add-On's
`PlayerNoJet.maxStepHeight = 1.2;` did. Players of that type move by them,
clients predict with them, and every other type stays v20's. Two Add-Ons
setting one constant: the later id wins. Test:
`crates/sim/tests/archetype_adjust.rs`.

**Bots you write.** v20 gives the player objects a Vehicle Spawn brick
makes no brain; the engine's bots walk, find their way round and over
builds, and fight inside their builder's minigame. A `bots` Add-On's
`assets/bots.json` lists kinds (`{"schema_version": 1, "bots": [...]}`);
each appears on the Vehicle Spawn list under its `name`. Every field but
`id` and `name` is optional:

| Field | Default | Meaning |
|---|---|---|
| `sight` | 80 | how far it sees other players |
| `wander_radius` | 12 | how far from its brick it strolls when nothing is going on |
| `return_home` | true | whether a brick bot returns home when idle; independent of combat pursuit |
| `chase_radius` | 48 | home allowance for idle interactions and explicit follow pursuit; ordinary combat has no spawn-distance cutoff |
| `reaction_seconds` | 0.35 | from first seeing an enemy to its first shot |
| `turn_degrees` | 300 | how fast its aim turns, per second |
| `aim_error_degrees` | 5 | aim error when a fight starts; it narrows to a third while it keeps sight |
| `memory_seconds` | 8 | how long it searches where it last saw, or was hurt by, an enemy |
| `fights_bots` | true | whether it fights other builders' bots too (one builder's bots are always one side) |

How far it keeps from its enemy comes from the weapon it holds: melee
weapons close in, explosive ones keep clear of their blast, and arcing shots
aim high for the drop. A bot is a player without a connection, so health,
damage, `onBotTouch` events and the Gravity Gun treat it as one.

**Vehicles you write.** A vehicle is any loose physics body: a `vehicles`
Add-On's `assets/vehicles.json` holds definitions (the format Import Add-On
writes; `tools/make_steel_ball_assets.py` writes the Steel Ball's). The
`Ball` family is a true sphere of the definition's size; a definition with
no seats cannot be mounted. These fields exist for Add-Ons:

- `"smash": { "speed", "radius", "max_volume", "force" }` breaks bricks it
  strikes at `speed` or faster, under the same rules a rocket's hit follows
  (a minigame's brick damage, ownership outside minigames). With
  `"energy_per_volume"` it punches through instead: its kinetic energy
  (½mv²) pays that much per unit of brick volume, nearest brick first, and
  it keeps what is left as speed, so a heavy fast ball goes through a wall
  and a slow one stops at it. With `"wreck_speed"` it damages vehicles it
  hits too, from nothing at `speed` to their whole health at `wreck_speed`
  (the closing speed of the two, under the minigame's vehicle damage rule).
- `"shove": true` bowls players over into a tumble instead of stopping
  against them.
- `"harms_only_in_minigames": true` keeps all of that inside minigames:
  outside one the vehicle breaks nothing, damages nothing and pushes
  players aside as any vehicle does, and it never harms its own owner.
- `"per_player": 3` lets each player have at most that many of this
  vehicle at once, on top of the server's vehicle limits. A spawn brick
  past it tells its builder "You already have 3 Steel Balls".
- `"return_seconds": 30` sends a vehicle from a spawn brick back to it
  as a fresh one after it has sat empty that long away from the brick (more
  than 3 units). A mode's `vehicle_return_seconds` (or the game's setting)
  overrides it; 0 or none never returns it.
- `"blast_scale": 3.0` makes rockets, tank shells and other blasts and
  shots push it that many times as hard as v20's rule (the impulse over
  its mass) would. The Steel Ball weighs 900 and sets 3, so a rocket
  still knocks it about. Contacts and a click's flip go by its mass alone.
- A seat's `"hands": { "right": [x, y, z], "left": [x, y, z] }` puts the
  rider's hands there: a steering wheel, a gun's handles. The points are in
  the vehicle's frame, as the seat's `transform` is. Every body reaches
  them the same way, with no per-body offsets: after the seat's clip pose,
  each arm (the hand socket's node and its ancestors below the chest)
  bends at its elbow and swings from its shoulder so the hand node lands
  on the point, or points straight at it when it is out of reach. An arm
  with no even elbow (a Blockhead's rigid arm) only swings. A gunner's
  holds turn with the turret and, when the gun model pitches with a
  `look` clip, tip with it about the weapon's `pivot`. A hand left out
  keeps its clip pose. The Playground buggy's driver and gunner use it.
- `"engine_sound": { "sound": "your-id:engine" }` gives it a running
  engine. The sound is a key of the `sounds` in the same Add-On's
  `assets/weapons.json`, declared with `"looping": true` (a `.wav` or
  `.ogg` of up to 30 seconds; a whole number of cycles loops without a
  click). It plays at the vehicle, heard in 3D like any world sound, while
  someone sits in the seat that drives it (any seat, for a vehicle no
  seat drives), and fades out when they get out or it is destroyed. Its
  pitch and volume follow the vehicle's speed in a straight line, as
  multiples of the sound's own: `idle_pitch` (0.8) and `idle_volume`
  (0.6) standing still, `full_pitch` (1.6) and `full_volume` (1.0) at
  `full_speed` units a second (the vehicle's `max_speed` when absent) and
  past it. Pitch is 0.25 to 4, volume 0 to 4. Every player hears it from
  the vehicle's motion as they see it, the driver included; nothing is
  sent for it. The Playground buggy's chug
  (`tools/make_playground_assets.py`) is the example:

  ```json
  "engine_sound": { "sound": "playground-buggy:engine",
                    "idle_pitch": 0.8, "full_pitch": 1.6, "full_speed": 34 }
  ```

  `bri-addon-check` fails an engine sound no weapons pack declares, and
  warns when it does not loop.
- `"lights": [{ "node": "headlight_l", "offset": [0, 0, -0.5],
  "color": [1, 0.95, 0.85], "radius": 30 }]` gives it lamps (up to 16):
  each a point light at a node of its model, moved `offset` along the
  node's axes so it lights the road ahead rather than the lens. The light
  key in the driving seat (`controls`) turns them all on or off for
  everyone; they go out when the vehicle is destroyed. Colour is linear
  RGB, 0 to 8 (above 1 is brighter); radius above 0 up to 100 units.

- `"skid": { "emitter": "your-id:skid", "sound": "your-id:squeal" }`
  makes its tyres skid. A grounded wheel whose contact slides sideways
  (the vehicle's motion across the wheel's steered axle) faster than
  `min_slip` units a second (3) runs `emitter` at the contact, harder the
  harder it slides, and the vehicle plays `sound` (a looping sound, as for
  `engine_sound`) louder and a little higher up to `full_slip` (12). Name
  either or both; the emitter is one of the vehicle's `effects` or a base
  game emitter. Every player sees and hears it from the vehicle's motion
  as they see it; nothing is sent for it, and v20's vehicles have none.
  `bri-addon-check` checks the sound as it does an engine sound.

Every vehicle can be placed from a vehicle spawn brick and spawned by a
rule (`spawn_vehicle`).

**Bare metal.** A package model's material (`*.shape.json`) may carry
`"metal": { "color", "roughness", "detail", "detail_scale",
"detail_strength" }`: the game then draws it as physically based metal that
reflects the world around it (a reflection probe placed at the nearest
metal object with Mirrors on, drawing the bricks, map, players, vehicles,
particles, plants, weather and mirrors a mirror would; the map's sky
otherwise) and takes sun and
lamp highlights in every Lighting mode. `color` is the reflectance (linear
RGB, steel about 0.62), `roughness` 0 is a mirror and 1 matte. The
material's own texture tints the colour; `detail` names another material
whose texture holds fine surface detail, repeated `detail_scale` times:
red scales the roughness (128 keeps it), green darkens (255 keeps it), blue
and alpha tilt the surface (128 flat). The Steel Ball's
(`tools/make_steel_ball_assets.py`) is the example.

Imported or written, any field can be edited and a new vehicle never needs
engine changes. The fields that decide how it flies and looks:

| Field | Meaning | v20 source |
|---|---|---|
| `family` | `Wheeled` (a car, or a plane when `wheeled_flight` is set) or `Flying` (hovers; `flight` holds its forces) | the datablock class |
| `wheeled_flight` | Blockland's flying forces on a wheeled vehicle: `max_forward_vel`, `max_reverse_vel`, `horizontal_surface_force`, `vertical_surface_force`, `stall_speed`, `sled`. `null` for a car | `maxForwardVel`, `maxReverseVel`, `horizontalSurfaceForce`, `verticalSurfaceForce`, `stallSpeed`, `isSled` |
| `thrust`, `reverse_thrust`, `lift` | push along the nose; lift along the roof, speed × `lift`, capped at 4000 | `forwardThrust`, `reverseThrust`, `lift`; `maneuveringForce` for `Flying` |
| `pitch_force`, `yaw_force`, `roll_force` | how hard the mouse and strafe keys turn it in the air | `pitchForce`, `yawForce`, `rollForce` |
| `flight` | a hovering vehicle's hover height, drag, auto-levelling, damping surfaces and steering | the `FlyingVehicleData` fields |
| `strafe_steering` | the strafe keys steer; otherwise the mouse steers and pitches | `steeringUseStrafeSteering` |
| `steering` | `strafe_rate`, and `auto_return`, `auto_return_rate`, `auto_return_max_speed`: whether steering drifts back to straight | `steeringStrafeSteeringRate`, `steeringUseAutoReturn`, `steeringAutoReturnRate`, `steeringAutoReturnMaxSpeed` |
| `wheels[].steering`, `wheels[].powered` | how far each wheel turns (1 fully, negative the other way) and whether it drives | `setWheelSteering`/`setWheelPowered` in `onAdd`, else v20's table by wheel count |
| `steering.toward_look` | the driver steers by looking: the wheels turn the hull toward the driver's free look and ease off to arrive (Halo's Warthog); a `Flying` vehicle also pitches its nose to the look, so it flies where the driver looks, and its strafe keys slide it sideways. A bot drives it the same way. Off: v20's mouse or strafe steering | none (new) |
| `camera.orbit` | the driver's third-person camera orbits the vehicle with the mouse: `min_pitch` and `max_pitch` (degrees of elevation, -89 to 89; looking down raises it), `height` (the pivot over the box centre) and `lag` (seconds it trails the vehicle's motion). The ground and wall check keeps it out of both | none (new) |
| `turret_sway` | the turret (`attachment_model`) tips on a spring against the vehicle's acceleration: `response` (radians per unit/s²), `frequency` (Hz, 3), `damping` (0.3), `max_degrees` (4). Drawn only | none (new) |
| `steering.speed_lock` | the share (0 to 1) of `max_steering` left at `max_speed`, narrowing in a straight line from full lock at rest (1, the default, never narrows) | none (new) |
| `engine_curve` | the engine's torque by speed: up to 16 `[speed share, torque share]` points (speed share 0 to 1 of `max_speed` by rising speed, torque share 0 to 4 of `engine_force`), linear between points, flat before the first and past the last, none past `max_speed`. Empty: v20's torque falling straight from full at rest to none at `max_speed` | none (new) |
| `wheels[].rebound_damping` | damping while the spring extends, apart from `damping` while it compresses: long, soft springs that soak a landing without bouncing. 0, the default: `damping` both ways, as v20 | none (new) |
| `gravity_scale` | gravity on this vehicle as a share (0 to 4) of the game's (0.85 floats its jumps). Absent: the game's | none (new) |
| `wheels[].tire.lateral_grip` | the tyre's grip across as a share (0.05 to 20) of its grip along (`static_friction`, `kinetic_friction`), stretching the friction circle into an ellipse: Halo's Warthog front tyres 0.6, rear 1.44. 1, the default: v20's circle | none (new) |
| `board_on_use` | the use key (the empty-hand click) on the vehicle within 6 units seats the player in the free seat nearest them; an upturned one still flips. Off: riders board only by landing on it, as in v20 | none (new) |
| `throttle_brakes` | the throttle against the way the vehicle rolls brakes it with `brake_force` until it stops, then drives the other way (Halo's hard stop when you pull back). Off: v20's reversed engine, which does next to nothing near top speed | none (new) |
| `reverse_limited` | a wheeled vehicle's engine stops pushing it backward at `reverse_speed`. Off: v20's wheeled vehicles back up as fast as they go forward | none (new) |
| `weapon.automatic` | the gun fires for as long as the trigger is held: `rate` [first, last] rounds a second rising over `spin_up_ticks`, and `spread` [first, last] radians each round may stray from the aim, rising over `spread_ticks` (120 ticks a second); letting go starts both again from the first. Absent: one shot a pull, at most every `cooldown_ticks` | none (new; Halo's `weap` rate of fire and error angle) |
| `weapon.recoil` | each shot's kick, as a speed change in units a second back from the shot and up (the impulse is that times `mass`). Absent: v20's 5, and 0 for a cannon | the fixed kick in v20's vehicle gun |
| `threads` | animations the model plays by itself, like a propeller | `playThread` and `setThreadDir` in `onAdd` and the functions it calls |
| `trails`, `effects` | emitters run at the model's nodes within a speed range, like wing-tip contrails; `effects` holds the vehicle's own particles and emitters | `mountImage` of an image whose state holds a `stateEmitter`, in `onAdd` and the functions it calls |

A Warthog is a wheeled vehicle with these set, as the Playground buggy has
them from Halo's own Warthog tags at 4.12 units to a Halo unit
(`tools/make_playground_assets.py`; the tag numbers, the reasons and the
measurements are in `docs/audits/vehicle-handling.md`): `max_speed` 34,
`reverse_speed` 14.8 with `reverse_limited`, `max_steering` 0.5236 (30°),
all four wheels `powered`, `rest_length` 0.62, the weight 57% forward with
each axle's `spring` carrying its share (10300 front, 7700 rear, so the deck
sits level and clears the ground), each axle's grip along and
across from the tags, a flat `engine_curve` with `engine_force` set so it
gains about 10 units a second each second, `throttle_brakes`, the
chaingun's rate of fire and spread on its gun, and:

```json
"engine_curve": [[0.0, 1.0], [0.9, 1.0], [1.0, 0.0]],
"throttle_brakes": true,
"reverse_limited": true,
"weapon": { "automatic": { "rate": [8.0, 15.0], "spin_up_ticks": 120,
                           "spread": [0.0175, 0.0349], "spread_ticks": 360 },
            "recoil": 0.0, "...": "..." },
"gravity_scale": 0.85,
"steering": { "strafe_rate": 0.25, "toward_look": true, "speed_lock": 0.6 },
"mass_center": [0.0, 0.05, -0.2],
"wheels": [{ "rest_length": 0.62, "spring": 10300, "damping": 2600,
             "rebound_damping": 2400, "anti_sway": 6000,
             "tire": { "static_friction": 3.0, "lateral_grip": 0.6, "...": "..." },
             "...": "..." }]
```

Every force and turn acts along the vehicle's own axes, so a flying vehicle
climbs where its nose points. A thread plays one of the model's sequences
on a slot from 0 to 3. `rate` scales its speed (1 when left out, 2 twice as
fast, negative backwards). Of the threads on one slot, the first whose
`min_speed`/`max_speed` range holds the vehicle's speed plays, so a
propeller can idle below speed 5 and race above it:

```json
"threads": [
  { "slot": 0, "sequence": "propslow", "max_speed": 5 },
  { "slot": 0, "sequence": "propfast", "min_speed": 5 }
]
```

A trail runs an emitter at a node of the model while the vehicle's speed is
in its range, drawn by each player's own game from the vehicle's motion.
Its `transform` is the emitter's place and turn in the model (local up is
the direction it ejects). The emitter is one of the base game's
(`v20/emitter/<name>`) or one listed in the vehicle's `effects`, whose
particles draw the base game's textures. The Stunt Plane's contrails:

```json
"trails": [
  { "node": "mount3", "transform": { "position": [4.4985, 0.6337, -0.5048], "rotation": [0, 0, 0, 1] },
    "emitter": "vehicle_stunt_plane:emitter/contrailemitter", "min_speed": 30 }
]
```

**Client code.** An Add-On may also carry code that runs on players'
machines: a WebAssembly module and WGSL shaders, declared in a `client`
section of its `package.json` and run in a sandbox, for presentation only.
Start from [`spinning-cube`](../../packages/samples/spinning-cube), which
draws a cube with its own shader, then [`steel-ball-fx`](../../packages/showcase/steel-ball-fx)
(sounds where every vehicle of a kind hits something) and
[`gravity-gun-fx`](../../packages/showcase/gravity-gun-fx) (beams, a force
field and GPU particle systems driven by a rule's public state). With
`world.read`, code sees what the player's own screen shows: where players,
vehicles and creatures are drawn and the server's public Add-On state; `draw_with`
gives a draw its own shader parameters and `material_blend` makes glowing
(additive) or see-through layers; `material_space` draws a material in
the world (0), in view space (1, in front of everything and following the
camera: a gun in first person) or in screen space (2, flat on the screen: a
scope, a hit marker); `view` reports the field of view, screen size,
first person, aiming and alive; `players()`
includes each player's archetype and held weapon as kinds you name with
`archetype_kind`/`image_kind`; `held` tells where a player's weapon is drawn
this frame and its muzzle (so a beam leaves the gun, in first person too),
and `image_mesh` gives you a held weapon's own model to draw with your
shader: a reskin. The
[Commando look](../../packages/samples/sample-commando-look/client/main.wat)
draws its rifle and scope this way. With `audio`, `sound_at` plays one of
its own `.wav` or `.ogg` files where something happens. Its capabilities (`render.layer`,
`render.shader`, `audio`, `input.focused`, `net.message`, `world.read`)
need the player to trust the server once (not when they installed the same
code themselves); `net.http` and `files.addon_folder` need a
separate, stronger choice per Add-On. **Who turns client code on.** The host does. When a server runs your
Add-On, everyone who joins downloads the host's copy and runs it for that
game; on a server that does not run it, nobody does, even players who
turned it on themselves. So an effect in the world, like the
[Ragdoll](../../packages/showcase/ragdoll), looks the same for everyone,
while each screen still draws it on its own with no network traffic. Code
for one player's own screen only (a HUD, a crosshair, a colour filter) sets
`"personal": true` in its `client` section: each player turns it on for
themselves, it runs on every server they join, and it is never sent to
anyone. `bri-addon-check` says which one an Add-On is ("Runs on"). Code
cannot ride in an Add-On with server rules (`behaviour`, `script`, ...),
which players never download: put it in its own Add-On that depends on
the rules, as `gravity-gun-fx` does.

The format is in
[packages.md](../architecture/packages.md) ("Client code"), and the
sandbox's host API, budgets and checks in
[client-sandbox.md](../architecture/client-sandbox.md). From a checkout,
`cargo run -p bri-client-sandbox --bin bri-addon-preview -- <add-on folder>
<output folder>` renders an Add-On's client code offscreen to PNG frames.


### Vehicle weapon grip nodes

A weapon seat may name `hands.right_node` and `hands.left_node` in its weapon
attachment model. These take precedence over `hands.right`/`hands.left` point
holds. The client samples the same attachment animation, yaw/pitch, sway and
vehicle scale used by its muzzle/ejection sockets. A named node that is missing
is reported when loading; it does not silently substitute a point.

```json
{"hands":{"right_node":"grip_right","left_node":"grip_left"}}
```

Point holds remain useful for steering wheels and other fixed grips. On a gun
whose whole attachment pitches, gunner point holds pitch with that attachment;
barrel-animation models retain their pivot-based point fallback. Named nodes are
preferable for grips that must follow an authored barrel animation exactly.
These targets position hands; they do not manufacture a new body stance or wrist
animation. Author the rider's clip separately.

Weapon attachment shapes may embed a `look` animation. Embedded clips take
precedence over matching external `look.dsq` clips. Rigid objects retain instanced
joint transforms; a shape with skin weights uses independently posed geometry
for each vehicle and shares its uploaded materials and textures. Existing native
shape skin weights, inverse bind matrices and animation channels apply unchanged.

For clips ordered up, neutral, down, `weapon.look_animation_pitch` optionally
declares the source `[down, up]` endpoints in native up-positive radians. For
example `[-0.25, 0.6]` maps those endpoints to clip phases 1 and 0, keeping zero
pitch at phase 0.5. Each side interpolates independently. Endpoints must straddle
zero within +/-pi/2. Omission retains v20's +/-pi/2 mapping. This is animation
mapping; `look_pitch` still bounds player aiming. Geometry, named sockets and
authoritative muzzle tracks share the mapping.

### One-shot sound variations

A sound profile can choose among up to 64 single-clip profiles in the same pack:

```json
{"file":"sounds/hit-a.wav","volume":1.0,"variants":[
  {"profile":"example:sound/hit-a","weight":1.0},
  {"profile":"example:sound/hit-b","weight":2.0}
]}
```

Each referenced profile keeps its own file and gain. The selected clip's volume
is multiplied by the containing profile's volume. Selection happens once when
playback starts and shares the prepared clip data. Weights must be positive and
at most 1000. Choices must be distinct, nonrecursive, nonlooping, nonmusic and
have the same `local` placement policy as their containing profile.

Missing clips are diagnosed and omitted from selection. If none loads, `file`
is tried as the fallback. Reloading a pack replaces its prepared choices.
This mechanism does not model sequential audio continuations or layered loops.

### Particle illumination

Particles default to `lit: false`, preserving their authored texture/tint brightness.
`lit: true` opts into native diffuse sprite illumination. The host adapter supplies
ambient, travelling sun direction/color and up to 256 point lights through
`EffectsRenderer::set_lighting`. The game client uses the scene camera and the same
current point lights as world geometry. Thin sprite normals face the viewer;
lighting uses the native vertex-lit model's quadratic point attenuation and color
convention, with alpha, fog and blend selection unchanged.

Standalone adapters default to neutral lighting until explicitly supplied. This
path does not receive geometry shadows, sky-band ambient or baked irradiance;
opting into it does not establish fidelity to another game's lighting.

### Particle tint interpolation

`ParticleVariation.color_interpolation` accepts `rgb` (default), `hsv_short` or
`hsv_long`. It controls the single tint sampled between `color` endpoints when
each particle is born. HSV interpolates saturation/value while choosing the short
or long hue path. Alpha always interpolates linearly. Grey/black endpoints borrow
the other hue instead of introducing an arbitrary red hue. This changes neither
texture pixels nor lifetime color keys.

### Authored particle orientation

Particles optionally declare `orientation` as `screen`, `axis` or `plane`.
Omission preserves the emitter's legacy `orient` behavior. Screen sprites face
the camera; axis sprites rotate toward it around their emission axis. Plane
sprites preserve the emitter frame's right axis and are perpendicular to their
emission direction (or velocity when `orient_on_velocity` requests it).

Planes retain authored roll and particle spin without turning toward the camera.
Attached particles and portal crossings carry the entire frame. Texture layout,
pivot, depth testing, blending and fog retain their existing meanings.
