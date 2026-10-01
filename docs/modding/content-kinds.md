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
| `archetype` | host | a playable body: movement, collision `box` or `ball`, steering, health, riding, model, camera distance | `crates/sim/tests/unlike_modes.rs` |
| `mode` | host | a Start Game game mode: name, the Add-Ons it runs, a map, and its own mini-game | `packages/stresslab/stresslab-mode`, `crates/sim/tests/mode_and_voxels.rs` |
| `model` | each player | a box model for an entity | `packages/stresslab/stresslab-creeper-model` |
| `hud` | each player | a HUD panel ([HUD panels](hud.md)) | `packages/samples/sample-points-hud` |
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
switches a player between bodies at any time. `push_archetype(p, a)` lays
another body of the same model over theirs for a while (a machine gunner
slowed as they fire; v20's `pushDatablock`), keeping their damage, and
`pop_archetype(p, a)` lifts it; `set_archetype` meanwhile changes the body
underneath, and death lifts them all. An archetype with
`"first_person_only": true` keeps its player's view in the eye whatever
their camera toggle says (v20's `firstPersonOnly`: Tier 2's slowed
machine gunner), and the toggle takes over again once the body changes.

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
| `threads` | animations the model plays by itself, like a propeller | `playThread` and `setThreadDir` in `onAdd` and the functions it calls |
| `trails`, `effects` | emitters run at the model's nodes within a speed range, like wing-tip contrails; `effects` holds the vehicle's own particles and emitters | `mountImage` of an image whose state holds a `stateEmitter`, in `onAdd` and the functions it calls |

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
