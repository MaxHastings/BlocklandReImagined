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
| `archetype` | host | a playable body: movement, collision `box` or `ball`, steering, health, riding, model, camera distance, `camera` profile and `sounds` | `crates/sim/tests/unlike_modes.rs` |
| `controller` | host | a movement controller: a WebAssembly module an archetype names as its `controller`; clients receive it with the archetype table ([Movement controllers](../architecture/movement-controllers.md)) | `packages/samples/sample-wall-kick`, `packages/samples/sample-dive`, `packages/samples/sample-trooper` |
| `mode` | host | a Start Game game mode: name, the Add-Ons it runs, a map, and its own mini-game | `packages/stresslab/stresslab-mode`, `crates/sim/tests/mode_and_voxels.rs` |
| `model` | each player | a box model for an entity | `packages/stresslab/stresslab-creeper-model` |
| `hud` | each player | a HUD panel, or a layout that claims a HUD region and draws textured elements in it ([HUD panels and regions](hud.md)) | `packages/samples/sample-points-hud` |
| `binds` | each player | keys for an Add-On's commands, under its own heading in Controls (below) | `crates/addon-import/ports/tool_newduplicator/files/binds.json` |
| `weapons` | everyone | weapons ([Weapons](weapons.md)) | `packages/samples/sample-bubble-blaster` |
| `bricks`, `vehicles` | everyone | written by Import Add-On ([Old v20 Add-Ons and new bricks](old-addons.md)), or a `vehicles.json` you write (fields below) | `packages/showcase/steel-ball-kit` |
| `bots` | everyone | bots a Vehicle Spawn brick can hold: name and how they play (fields below) | `packages/blockhead_bot` |
| `texture`, `block` | everyone | a PNG for block faces (up to 1024 px a side); textures or flipbooks per face with named states | `crates/sim/tests/blocks.rs` |

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

Up to 32 binds go under `division` in Options → Controls, where players
can rebind them. A bind sends its package's command as if typed: one with
no arguments when pressed, or with `"hold": true` a command declared with
`"args": ["bool"]`, sent `true` as the key goes down and `false` as it
comes up. Commands a bind sends may not be `tool_only`. `key` (and
`mac_key` on a Mac) is bound by default when neither the bind nor the key
is taken, as the New Duplicator's own keys were; a key the player binds it to
instead is kept. Binds show only while the host
runs their package.

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
crosshair, falling back to the eye when the barrel is inside a wall.
`look.animations` maps roles to the body's own clips: `idle`, `run`,
`back`, `side`, `crouch`, `crouch_run`, `crouch_back`, `crouch_side`,
`jump`, `fall`, `sit`, `aim_right`, `aim_left`, `aim_both`,
`look`, `head_up`, `head_side`, `swim` and `death`. Unnamed roles use a clip of the
Blockhead's name if the model has one, then a fallback (`back` runs `run`
reversed, `fall` plays `jump`, `sit` plays `idle`), and a role with none
leaves the body in its bind pose for that layer. The client names what a
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
  bob against the camera by `bob` times the camera's bob. Client code
  drawing its own first-person model reads the same sway from `view`.
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
| `chase_radius` | 48 | how far from its brick it follows a fight before heading back |
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
no seats cannot be mounted. Three fields exist for Add-Ons:

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
- `"blast_scale": 3.0` makes rockets, tank shells and other blasts and
  shots push it that many times as hard as v20's rule (the impulse over
  its mass) would. The Steel Ball weighs 900 and sets 3, so a rocket
  still knocks it about. Contacts and a click's flip go by its mass alone.

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
| `steering.toward_look` | the driver steers by looking: the wheels turn the hull toward the driver's free look and ease off to arrive (Halo's Warthog); a bot drives it the same way. Off: v20's mouse or strafe steering | none (new) |
| `steering.speed_lock` | the share (0 to 1) of `max_steering` left at `max_speed`, narrowing in a straight line from full lock at rest (1, the default, never narrows) | none (new) |
| `engine_curve` | the engine's torque by speed: up to 16 `[speed share, torque share]` points (speed share 0 to 1 of `max_speed` by rising speed, torque share 0 to 4 of `engine_force`), linear between points, flat before the first and past the last, none past `max_speed`. Empty: v20's torque falling straight from full at rest to none at `max_speed` | none (new) |
| `wheels[].rebound_damping` | damping while the spring extends, apart from `damping` while it compresses: long, soft springs that soak a landing without bouncing. 0, the default: `damping` both ways, as v20 | none (new) |
| `gravity_scale` | gravity on this vehicle as a share (0 to 4) of the game's (0.85 floats its jumps). Absent: the game's | none (new) |
| `wheels[].tire.lateral_grip` | the tyre's grip across as a share (0.05 to 20) of its grip along (`static_friction`, `kinetic_friction`), stretching the friction circle into an ellipse: Halo's Warthog front tyres 0.6, rear 1.44. 1, the default: v20's circle | none (new) |
| `throttle_brakes` | the throttle against the way the vehicle rolls brakes it with `brake_force` until it stops, then drives the other way (Halo's hard stop when you pull back). Off: v20's reversed engine, which does next to nothing near top speed | none (new) |
| `reverse_limited` | a wheeled vehicle's engine stops pushing it backward at `reverse_speed`. Off: v20's wheeled vehicles back up as fast as they go forward | none (new) |
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
gains about 10 units a second each second, `throttle_brakes`, and:

```json
"engine_curve": [[0.0, 1.0], [0.9, 1.0], [1.0, 0.0]],
"throttle_brakes": true,
"reverse_limited": true,
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
