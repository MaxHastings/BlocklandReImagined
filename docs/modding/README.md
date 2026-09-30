# Making Add-Ons

This guide is for anyone who wants to make an Add-On for Blockland
ReImagined. It covers what works today; anything still being built is in
the last section, so you never write against something that does not exist
yet.

Read it in order: make an Add-On (sections 1 to 6), bring in an old v20
Add-On and port what its scripts did (section 7), then what players are
asked to trust when your Add-On runs code on their PC (section 8).

## What you can make

| You want | Start from | It is |
|---|---|---|
| A game rule: points, rounds, commands | [`sample-survival-points`](../../packages/samples/sample-survival-points) | a `behaviour` file and a Rhai script, run by the host |
| A HUD panel for a rule | [`sample-points-hud`](../../packages/samples/sample-points-hud) | a JSON panel each player draws |
| A weapon | [`sample-bubble-blaster`](../../packages/samples/sample-bubble-blaster) | an `assets/weapons.json` file |
| A tool that acts where it is clicked | [`duplicator`](../../packages/duplicator) | a weapon whose image runs a rule's command (section 5) |
| A tool that grabs, holds and throws players and vehicles | [`gravity-gun`](../../packages/showcase/gravity-gun) | a rule using the `physics` operations (section 3), its tool, and client effects |
| A new vehicle or loose physics object | [`steel-ball-kit`](../../packages/showcase/steel-ball-kit) | an `assets/vehicles.json` you write (section 6) |
| Effects drawn on every player's screen | [`gravity-gun-fx`](../../packages/showcase/gravity-gun-fx) | WebAssembly and WGSL shaders reading what the game shows (section 6) |
| New bricks | a v20-style brick Add-On you import (section 7) | a brick catalog the importer writes |
| A game mode in Start Game | [`stresslab-mode`](../../packages/stresslab/stresslab-mode) | a `mode` file naming Add-Ons and a map |
| Worlds, creatures, bodies, blocks | [`packages/stresslab`](../../packages/stresslab) | see section 6 |
| A whole new game on top: bodies, scoped guns, creatures that shoot back, scoring | [`sample-commando`](../../packages/samples/sample-commando) and its four siblings | five Add-Ons: a weapon, a look with client code, rules, a HUD and a mode ([total-conversion.md](../audits/total-conversion.md)) |

Old Blockland v20 Add-Ons (`.zip` files) also work: see section 7.

## 1. Make one

1. Copy the sample closest to what you want and give the folder a new name.
2. Pick an id for your Add-On: lowercase letters, digits, `-` and `_`,
   starting with a letter, for example `coin-rain`. It never changes once
   people use your Add-On. Replace the sample's id **everywhere** in the
   folder: in `package.json` (`id` and every `provides` id), and in any
   other file that names it (a HUD's `bind` and `package`, the ids inside
   `weapons.json`).
3. Change `name`, `description` and `authors`, then make it your own.

A weapon only needs a new id. A rule needs its commands and state changed in
`behaviour.json` and its script rewritten (section 3).

## 2. Check it and try it

You need a checkout of this repository and Rust (see the main
[README](../../README.md)); neither the game nor v20 content is needed.

**Check** that the game can load it. Put the Add-Ons it depends on in the
same parent folder as it (for a HUD, next to its rule):

```sh
cargo run -p bri-package-runtime --bin bri-addon-check -- path/to/coin-rain
```

It prints who needs the Add-On (the host only, each player, or everyone),
what it provides and may do, and every mistake with its file, line and a
hint: a misspelt field, a missing hook function, a script syntax error, a
HUD row bound to a key players never receive, a broken weapons file.

**Try** a rule without opening the game. `bri-addon-run` runs it with the
Add-Ons it needs in an empty world with two players, `Host` (an admin) and
`Guest`, sends the commands you give it one second apart, and prints the
chat, the state players receive and any script problem:

```sh
cargo run -p bri-sim --bin bri-addon-run -- path/to/coin-rain \
  --send coins --wait 20 --send "gift 1" --send "Guest: coins"
```

`--wait N` waits N more seconds before the next command, `--seconds N` runs
at least that long (10 by default), `"Guest: ..."` sends as the Guest and
`other-add-on:command` sends another Add-On's command.

**Play** it: put the folder in the game's `content/addons/` folder
(`%LOCALAPPDATA%\BlocklandReImagined\Game\content` when you run
`BlocklandReImagined.exe`, or `content` beside `Launch.cmd` in a release
folder), open **Start Game > Add-Ons**, and turn it on. The screen turns on what it
depends on and shows players what it may do. See
[mod-manager.md](../architecture/mod-manager.md).

## 3. Game rules

A rule is a `behaviour` file and a Rhai `script`, both listed in
`provides`. The sample's `behaviour.json`:

```json
{
  "schema_version": 1,
  "script": "points.rhai",
  "commands": [
    { "name": "top", "cooldown_ticks": 240 },
    { "name": "reset", "admin": true }
  ],
  "state": {
    "player": {
      "points": { "default": 0, "visible": "everyone" },
      "best":   { "default": 0, "visible": "everyone" }
    },
    "global": {
      "awarded": { "default": 0, "visible": "everyone" },
      "every":   { "default": 1, "persist": false }
    }
  },
  "on_join": true,
  "tick_interval": 600
}
```

**Hooks.** A script may only define functions; top-level statements are
refused. The engine calls:

| Function | When |
|---|---|
| `on_join(player)` | a player joins, when `"on_join": true` |
| `on_tick()` | every `tick_interval` ticks (120 ticks = 1 second) |
| `on_death(victim, killer)` | any player dies, when `"on_death": true` (`killer` is `()` for none) |
| `on_loadout(player)` | a player's items were set afresh (spawn, respawn, joining or leaving a minigame), when `"on_loadout": true`: the place to hand out your Add-On's items |
| `on_spawn(player)` | a player comes to life (joining, respawning), after `on_loadout`, when `"on_spawn": true` |
| `on_leave(player)` | a player leaves, while their state can still be read, when `"on_leave": true` |
| `on_damage(victim, attacker, amount, info)` | before a player is hurt, when `"on_damage": true`: return the amount to take (0 prevents it) or `()` to leave it. `info` is `#{ kind, type, direct }`, `kind` being `weapon`, `fall`, `package` and so on |
| `on_entity_damage(entity, attacker, amount, info)` | before one of your creatures is hurt by a shot, a blast or `explode`, when `"on_entity_damage": true`: answered like `on_damage` |
| `on_entity_death(entity, killer, info)` | one of your creatures ran out of health, just before it is removed, when `"on_entity_death": true` |
| `cmd_<name>(player, args...)` | a player sends a command listed in `commands` |

`player` is the player's id: pass it straight to `tell`, `get_player` and
the rest. Loading checks that each hook exists with the right number of
parameters, so a typo shows up in `bri-addon-check`, not mid-game.

**Commands** are the only thing a player can ask of your script. Each has a
`name`, and optionally `args` (a list of `"int"`, `"float"`, `"string"` or
`"bool"`, for example `"args": ["int"]` for `cmd_gift(player, amount)`),
`cooldown_ticks` per player, `admin: true` to refuse non-administrators,
and `aim_reach` to have the engine resolve what the player is aiming at
(read it with `aim()`). Players send a command by typing it in chat,
`/sell coal`, or with a HUD panel's keys (section 4). Typed words become the
declared arguments in order; a final `string` argument takes the rest of the
line. Two Add-Ons declaring the same command name make the typed form
ambiguous, and the host says so.

**State** is declared up front with defaults. `player` keys exist for every
player; `global` keys once per server. `visible` says who receives the
value: `server` (the default; it never leaves the server), `owner` (a
player key sent only to that player, like a hand of cards) or `everyone`.
HUD panels can only show keys the viewer receives. `persist` (default
`true`) saves the key with the host's world and restores it after a restart.

**Script functions:**

| Read | Change state | Act on the world (needs capability) |
|---|---|---|
| `tick()`, `seed()`, `caller()` | `get(key)`, `set(key, value)` | `tell(player, text)`, `broadcast(text)`: `chat` |
| `players()`, `player(id)` | `get_player(p, key)`, `set_player(p, key, v)` | `remove_brick`, `place_brick`, `set_block_state(brick, state)`: `world.edit` |
| `aim()`, `me()`, `entities()` | `add_player(p, key, amount)` | `damage(target, amount[, by[, type]])`, `explode(...)`: `damage` |
| `noise(seed, x, z)`, `hash3(seed, x, y, z)` | `entity_get(e, key)`, `entity_set(e, key, v)` | `spawn_entity`, `remove_entity`, `steer`, `label`: `entity` |
| `object(ref)`, `objects()`, `objects_near(x, y, z, r)`, `held(p)` | | `teleport`, `respawn`, `set_archetype`, `control(p, entity)`, `release(p)`, `give_item(p, item, equip)`: `player` |
| `raycast(from, dir, range[, ignore])`, `can_damage(by, target)` | | `set_fov(p, fov)`, `set_image_ammo(p, ammo)`, `mount_image(p, image)`: `player` |
| | | `copy_build(p, brick, limit, above_only, tool)`: `build` |
| | | `push`, `tumble`, `hold`, `hold_distance`, `let_go`, `spawn_vehicle`, `remove_vehicle`: `physics` |
| | | `heal(p, amount)`, `fire(...)`: `damage` |
| | | `center_print(p, text, seconds)`, `bottom_print(p, text, seconds)` (`()` for everyone): `chat` |
| | | `play_sound(p, sound)` at a player's ears, `sound_at(sound, x, y, z)`, `beam(from, to[, options])`, `play_thread(p, thread, sequence)`: `effects` |
| | | `set_map_lights([x, y, z], radius, options)`: `lighting` |

Coming from TorqueScript? [torque-equivalents.md](torque-equivalents.md)
lists what each v20 call you know became here, and what is not here yet.

A value from `players()` is a map with `id`, `name`, `x`, `y`, `z` (the
feet), `alive`, `admin`, `ex`, `ey`, `ez` (the eye), `lx`, `ly`, `lz` (the
unit direction they look), `vx`, `vy`, `vz`, `item` (the id of the item
in their hand, or `""`), `minigame` (its id, or `()` outside one),
`health`, `max_health`, `archetype`, `crouched`, `mounted` (seated on a
vehicle or riding a player), `scale`, `cx`, `cy`, `cz` (the middle of the
body, `getWorldBoxCenter`), `slot` (the selected tool slot from 0, or
`()`), `image` (the image in their hand, or `""`) and `image_state` (the
name of that image's state, such as `"Ready"`).

**Rays and damage.** `raycast([x, y, z], [dx, dy, dz], range)` returns the
first thing a ray meets, now, as the script runs: a map with `kind`
(`"player"`, `"vehicle"`, `"entity"`, `"brick"` or `"map"`), `id` (`()` for
the map), `ref` (an object like `"player:3"`, for players, vehicles and
entities), `x`, `y`, `z`, the surface normal `nx`, `ny`, `nz`, and
`distance`; or `()` when it meets nothing. Rays reach up to 2000 units and
a call casts at most 64. A fourth argument names a player whose body the
ray passes through, usually the shooter: `raycast(eye, look, 200.0, p.id)`.
Rays see the world as it was when the call began: a brick the same call
removes still stops them. `can_damage(by, target)` asks the minigame
rules whether player `by` may hurt `target` (a player id or an object),
the same answer the engine's own weapons get. `damage` hurts whatever you
name, a player id or an object, whatever the rules say, so ask first when
a player's shot should obey them. Give `damage` a type from your weapons
pack (`"CommandoRifle"` or `"$DamageType::CommandoRifle"`) to use its kill
message, vehicle scale and whether it is a direct hit, as a projectile of that
type would; without one the damage is your Add-On's own.

**Held images.** `mount_image(p, image)` puts another image of your
weapons (or a dependency's) in the player's hand and keeps their tool
slot: a scope over a rifle, a second fire mode. `mount_image(p, ())` puts
the selected tool's own image back. `set_image_ammo(p, false)` tells the
held image it is out of ammo, so its states' `no_ammo` transitions run;
`true` gives it back. A magazine is then a player state key your rule
counts down. `set_fov(p, fov)` sets the player's field of view (5 to 120
degrees), and `set_fov(p, ())` hands it back to their own setting; aiming
and the zoom key still work on top of it.

**Effects** (`effects`) change nothing in the game and are sent once, like
a sound. `beam(from, to)` draws a straight beam for a moment: a tracer, a
laser, a bolt. Options go in a map, `beam(from, to, #{ color: [1.0, 0.8,
0.4], width: 0.05, seconds: 0.1, muzzle: p.id })`: `color` is `[r, g, b]`
or `[r, g, b, a]` from 0 to 1, `width` up to 16 units, `seconds` up to 10,
and `muzzle` starts it at that player's gun muzzle as each player draws it.
The beam thins and fades out over its life. `play_thread(p, thread,
sequence)` plays one of the body's animations: thread 3 a gesture any time
(`"activate2"`, `"root"` to stop), thread 2 the arms with what they hold.
Prints, sounds, beams and animations share one allowance of 64 a second
per Add-On.

**Map lights** (`lighting`) switch, dim and recolour the lamps, bulbs and
tubes baked into the map, for everyone on the server, while they play.
`set_map_lights([x, y, z], radius, #{ on: false })` switches off every map
light within `radius` units of the point (up to 2000);
`#{ color: [1.0, 0.3, 0.2], brightness: 0.5 }` recolours and dims them
(`color` from 0 to 4 per channel, `brightness` from 0 to 4, both default 1).
Calling again with the same point and radius replaces that setting, a
later setting wins where spheres overlap, and `#{}` puts those lights back
as the map made them. A map keeps up to 256 settings; a new map starts
with none. A broken light bulb or tube stays dark whatever a script sets.
Bricks' own lights are not map lights.

`fire(projectile, x, y, z, vx, vy, vz)` launches a projectile of your
Add-On's weapons, or of an Add-On it depends on, from a point at a
velocity: a creature's gun, a turret, a fireball. Add a player as a last
argument to make it their shot, hurting whom their shots may; without one
it is your Add-On's own, which hurts any living player (as `damage` could)
and credits nobody. Start it clear of the shooter's body. 240 a second per
Add-On. The Commando's sentry does this from its think.

**Moving things** (`physics`). Players, vehicles (every loose physics body:
jeeps, balls, the tumble of a knocked-down player) and package entities
are *objects*, named by a string: `"player:3"`, `"vehicle:12"`,
`"entity:7"`. `object(ref)` is a map with `ref`, `kind`, `id`,
`definition`, `x`, `y`, `z` (its middle), `vx`, `vy`, `vz`, `speed`,
`mass`, `radius`, `owner` and `spawner` (the Add-On that spawned it);
`objects()` lists every vehicle and `objects_near(x, y, z, r)` everything
near a point. A command with `aim_reach` also reports the nearest object in
front of the brick it hit: `aim().object`, `aim().object_distance` and
`aim().movable`, whether the caller may move it.

| Operation | Does |
|---|---|
| `push(ref, vx, vy, vz)`, `push(ref, vx, vy, vz, by)` | Adds to its velocity (units a second, at most 200). |
| `tumble(player, vx, vy, vz, by)` | Knocks a player off their feet into a tumble, flying at that velocity. |
| `hold(player, ref, distance)`, `hold(player, ref, distance, #{at, force, turn})` | Keeps `ref` floating `distance` (0.5 to 64) ahead of the player's eye, where they look, every tick until let go, carried at the velocity the aim point moves so it keeps up as they turn and walk. `at` (`[x, y, z]`, default its middle) is the spot on it that is held there, as a physics gun grabs where it points. `force` (default 36000, at most 10,000,000) is how hard it may pull: things up to `force / 450` in mass answer at once, heavier ones swing in slower and very heavy ones can only be dragged. With `turn`, it keeps the angle it had to the player as they turn. A living player held goes limp until let go and landed; a corpse (a player who died) can be held too. One hold per player; taking something another player holds ends their hold. |
| `hold_distance(player, distance)` | Moves what they hold nearer or farther (0.5 to 64): a reel. |
| `let_go(player)`, `held(player)` | Ends the hold; what they hold, or `()`. |
| `spawn_vehicle(def, x, y, z, yaw, [vx, vy, vz], owner)` | A vehicle of this Add-On or one it depends on, belonging to `owner` (or `()`). Counts toward the server's vehicle limits; at most 64 per Add-On. |
| `remove_vehicle(ref)` | Removes a vehicle this Add-On spawned. |

The engine, not the script, decides who may move what: a player may move
another player when their minigame lets them hurt that player, or,
outside minigames, when that player trusts them to build; a vehicle when
its minigame lets them damage it, or, outside minigames, when they could
ride it; a corpse by those who could move that player when they died
(same minigame, or trusted outside minigames); entities always; and
outside minigames, a server administrator anything. A hold is
checked again as it goes and ends when the rules stop allowing it, the
holder dies, sits down or leaves, the held player dies or revives, the
holder stands on what they hold, or it snags on something and is dragged
too far from where it should be. What a push, tumble or hold moves is credited to
`by` (the caller, by default) for five seconds: a vehicle that then runs
someone over, or smashes bricks, does it as that player.

`give_item(p, item, equip)` puts a weapon or tool in the player's tool list
(unless they carry it already) and, with `equip`, in their hand.
`copy_build(p, brick, limit, above_only, tool)` copies the build at `brick`
for the player whose command asked: the brick and every brick joined to it
through studs that the player may build on, with `above_only` none below
the brick, refused past `limit` bricks (at most 10000). The player sees the
copy as a ghost while `tool` is in hand, moves and turns it with the brick
keys, and plants it with the plant key; planting follows the server's plant
rules, plants all of it or none, and one Ctrl+Z takes it back. The
Duplicator is the worked example.

**Capabilities** in `package.json` are the only permission gate. If your
script calls `tell` without `"chat"` in `capabilities`, the call is refused
with a message saying what to add. The Add-Ons screen shows players the
capabilities in plain words ("send chat messages"), so ask for only what
you use. The `sound` capability is now called `effects`; a manifest that
still says `sound` is refused with that message. Scripts run inside budgets (operations per call, sizes, call
depth): a runaway loop stops with a problem report instead of freezing the
server. Keep arrays and maps under 1,000 levels deep: a value nested
tens of thousands deep (`a = [a]` in a loop) overflows the host's stack
and stops the server, because the script engine has no nesting limit.
Scripts cannot read files, open sockets or run `eval`.

## 4. HUD panels

A HUD panel is a JSON file each player's game draws from state it
receives. A rule's Add-On runs only on the host, so a HUD is always its own
Add-On that depends on the rule, as `sample-points-hud` depends on
`sample-survival-points`.

```json
{
  "schema_version": 1,
  "slot": "hud.overlay",
  "anchor": "top_left",
  "title": "SURVIVAL POINTS",
  "background": [0.05, 0.08, 0.12, 0.8],
  "accent": [0.35, 0.85, 0.45, 1.0],
  "text": [0.92, 0.95, 1.0, 1.0],
  "rows": [
    { "label": "Points", "bind": "sample-survival-points:player/points" },
    { "label": "Awarded to everyone", "bind": "sample-survival-points:global/awarded" }
  ],
  "keys": [
    { "key": "N", "label": "Leaderboard", "package": "sample-survival-points", "command": "top" }
  ]
}
```

- `bind` is `rule-id:player/key` (the viewing player's value),
  `rule-id:players/key` (every player's value, one line each) or
  `rule-id:global/key`. The viewer must receive the key (`owner` or
  `everyone` for its own player value, `everyone` otherwise), or the Add-On
  is refused at load.
- `anchor` is `top_left`, `top_right`, `bottom_left` or `bottom_right`.
- Up to 16 rows and 8 keys. A key is one letter `A`-`Z` that sends the
  rule's command (`package` is the rule's id) with no arguments. The game
  refuses letters it already uses.
- Colors are RGBA from 0 to 1.

## 5. Weapons

A weapon Add-On is an `assets/weapons.json` file, listed in `provides` as
`{ "kind": "weapons", "id": "your-id:weapons/main", "file":
"assets/weapons.json" }`. It is the same format **Import Add-On** writes
for v20 weapons: `items` (what players hold), `images` (the held model and
its firing states), `projectiles`, `damage_types` and `explosions`. Model
and icon paths may point at base game files; the sample reuses
`Add-Ons/Weapon_Gun/pistol.dts`.

The fields you are most likely to change:

| Where | Field | Meaning |
|---|---|---|
| projectile | `speed`, `gravity`, `lifetime_ticks` | how fast, how much it drops, how long it lives (120 ticks = 1 s) |
| projectile | `damage`, `impulse`, `vertical` | hurt, and how hard it shoves |
| projectile | `ballistic`, `elasticity` | bounces, and how much |
| image state | `ticks` | how long a state (`Fire` is the reload time) lasts |
| image | `shot` | several projectiles per shot, their spread and the recoil ([porting.md](porting.md#the-image-shot-field)) |
| item | `ui_name` | the name players see |
| image | `zoom` | `{ "fov": 20, "on_jet": true, "crosshair": false, "first_person": true }`: aim with the zoom key (and the right mouse button with `on_jet`), hide the crosshair, force first person while aiming |
| image | `eye_offset`, `eye_rotation` | where the weapon sits in first person |
| pack | `sounds` | `{ "your-id:shot": { "file": "sounds/shot.wav", "volume": 0.8 } }`: your own `.wav`/`.ogg` files, named by a state's `sound` and by rules; `local` for sounds only the holder hears, `looping` for a state-long hum |

The engine has no idea of clips, magazines or reloads: a rule builds them
from a player state key, `set_image_ammo` and image commands (section 3,
"Held images"), and the light key can reload (below). The
[Commando rifle](../../packages/samples/sample-commando-rifle/assets/weapons.json)
is a plain scoped rifle: raise it, fire, let go, fire again.

Shots hit players, vehicles, bricks and Add-On creatures. A creature's
own rule decides what the hit does (`on_entity_damage`).

A tool rather than a gun: give its image `"command": "your-rule:command"`
and no projectile. Its `onFire` state then runs that command of your rule
Add-On for the holder, with `aim()` resolved where they look (declare
`aim_reach` on the command). The Duplicator's `duplicator-tool` does this.

More moments can run commands through the image's `commands`:

```json
"commands": {
  "states": { "ongrab": "gravity-gun:grab", "onrelease": "gravity-gun:release" },
  "wheel": "gravity-gun:reel"
}
```

`"light": "your-rule:reload"` there runs when the holder presses the light
key with the image in hand, instead of turning on their light, as v20
Add-Ons did by packaging `serverCmdLight`.

`states` maps a state's `script` (lowercase) to a command, run as the
image enters that state: a state with `"down"` to a charging state whose
`"up"` leads to the firing state gives a press-and-hold charge.
`jet` runs when the holder presses jet (the right mouse button) with the
tool in hand, as v20's `onTrigger` slot 4 did; players who can jet still
jet. `wheel` runs while the trigger is held with the mouse wheel's notches
as its one `int` argument (positive rolled forward, away from you), and
the wheel then does not change tools; declare `"args": ["int"]` on that
command. The Gravity Gun's `gravity-gun-tool` uses `states` and `wheel`:
hold left click to grab, roll to reel, let go to drop or fling.

Keys of `damage_types` and `explosions` are their `name` in lowercase, and
a projectile names its damage type as `$DamageType::<name>`. Everyone in a
game needs the same weapons, so give the Add-On to the people you play
with.

## 6. Other content kinds

Each file in `provides` has a `kind`. Which kinds an Add-On provides decides
who needs it: only host kinds means the host only; only `model` and `hud`
means each player; anything else (weapons, bricks, blocks) means everyone.
An Add-On cannot mix host kinds with `model` or `hud`: split it in two, the
visuals depending on the rules. `bri-addon-check` tells you which it is.

| Kind | Needed by | What it is | Example |
|---|---|---|---|
| `behaviour`, `script` | host | a game rule (section 3) | `packages/samples/sample-survival-points` |
| `world` | host | a generated chunk world: materials, a `generate(cx, cz)` function | `packages/stresslab/stresslab-world` |
| `entity` | host | a scripted creature: model, `think` function, speed, health | `packages/stresslab/stresslab-creeper` |
| `archetype` | host | a playable body: movement, collision `box` or `ball`, steering, health, riding, model, camera distance | `crates/sim/tests/unlike_modes.rs` |
| `mode` | host | a Start Game game mode: name, the Add-Ons it runs, a map | `packages/stresslab/stresslab-mode` |
| `model` | each player | a box model for an entity | `packages/stresslab/stresslab-creeper-model` |
| `hud` | each player | a HUD panel (section 4) | `packages/samples/sample-points-hud` |
| `weapons` | everyone | weapons (section 5) | `packages/samples/sample-bubble-blaster` |
| `bricks`, `vehicles` | everyone | written by Import Add-On (section 7), or a `vehicles.json` you write (fields below) | `packages/showcase/steel-ball-kit` |
| `texture`, `block` | everyone | a PNG for block faces (up to 1024 px a side); textures or flipbooks per face with named states | `crates/sim/tests/blocks.rs` |

Entities may spawn only their own Add-On's entity kinds.

An archetype's `model` may be a package model, a v20 shape, or `"none"`
for no drawn body (client code can then draw its own). Package models draw
at the body's scale, and an entity takes a `scale` from 0.2 to 4. The
Commando's [archetype](../../packages/samples/sample-commando/commando-archetype.json)
is a whole new body in a dozen lines: no jet, 150 health and faster feet.
`movement` accepts any of the motor's constants by name (`gravity`,
`jump_speed`, `air_control`, `step_height` and the rest); `set_archetype`
switches a player between bodies at any time.

**Vehicles you write.** A vehicle is any loose physics body: a `vehicles`
Add-On's `assets/vehicles.json` holds definitions (the format Import Add-On
writes; `tools/make_steel_ball_assets.py` writes the Steel Ball's). The
`Ball` family is a true sphere of the definition's size; a definition with
no seats cannot be mounted. Two fields exist for Add-Ons: `"smash": {
"speed", "radius", "max_volume", "force" }` breaks bricks it strikes at
`speed` or faster, under the same rules a rocket's hit follows (a minigame's
brick damage, ownership outside minigames); `"shove": true` bowls players
over into a tumble instead of stopping against them. Every vehicle can be
placed from a vehicle spawn brick and spawned by a rule (`spawn_vehicle`).

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
(one shader drawn over every vehicle of a kind) and
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

## 7. Old v20 Add-Ons and new bricks

Put an old Blockland Add-On (a `.zip` or a folder with `server.cs`) in the
game's `content/Add-Ons/` folder (inside `content`, found as in section
2), open **Start Game > Add-Ons**, pick it
and press **Import**. Its scripts are never run: datablocks for bricks,
weapons and vehicles become data, and `IMPORT-REPORT.md` in the new Add-On
lists what came across and what did not. An Add-On without a licence file
is imported as `proprietary`; for your own work, set `license` in the new
`package.json`. From a checkout the same importer runs as:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- Weapon_Example.zip out/weapon_example
```

Many v20 Add-Ons keep part of what they do in scripts: a shotgun's spread,
a slash command. The report lists each such function under **Needs
behaviour**. When someone has made a native **port** of that Add-On, the
importer applies it and the report says **Ported**. The ports so far, and the
recipe for making one (or having your agent make one), are in
[porting.md](porting.md).

That is also how you make **new bricks** today: write a small v20-style
brick Add-On and import it. A folder `Brick_Tall` holding:

```text
server.cs         datablock fxDTSBrickData(brick3x3x2Data)
                  {
                      brickFile = "./3x3x2.blb";
                      category = "Bricks";
                      subCategory = "Tall";
                      uiName = "3x3x2 Block";
                  };
3x3x2.blb         3 3 6
                  BRICK
description.txt   Title: Tall Bricks
                  Author: You
```

imports into an Add-On with one 3x3 brick, two bricks tall. A `.blb`'s
first line is its size in studs, studs and plates (three plates to a
brick); `BRICK` gives a plain box with studs. v20's own brick Add-Ons show
the longer form for other shapes.

**Mirrors.** Any brick can have mirror sides. A brick Add-On can reuse the
game's window as a mirror with no model of its own (a brick that inherits a
base game brick's `brickFile` is imported without a copy of that shape; the
game lends it the base brick's shape and menu icon). The default **Mirror**
Add-On (`packages/brick_mirror`) is this brick:

```text
server.cs         datablock fxDTSBrickData(brickMirror1x4x5Data : brick4x1x5windowData)
                  {
                      uiName = "1x4x5 Mirror";
                      reflectionFaces = "north south";
                      reflectionDepth = 0.5;
                  };
```

The mirror spans the whole side and the window's frame, drawn in front of
it, hides its edges; the window's see-through glass is not drawn.

| Field | Meaning | Default |
|---|---|---|
| `reflectionFaces` | Which sides mirror: `north south east west top bottom` | required |
| `reflectionDepth` | How far in the mirror sits, from that side (0) toward the opposite side (1); 0.5 is the middle of the brick | 0 |
| `reflectionInset` | How far in from the side's edges the glass stops, in world units (leaves a frame) | 0 |
| `reflectionTint` | Colour the reflection is multiplied by, `"r g b"` from 0 to 1 (a real mirror is about `"0.95 0.95 0.95"`) | `"1 1 1"` |
| `reflectionStrength` | 1 is a full mirror, which also stops drawing the brick's own see-through surfaces across its mirrored sides (a window's glass); below 1 the painted brick shows through | 1 |

Mirrors are drawn only on each player's computer, never sent over the
network. Players pick how many mirrors show live reflections with
**Options > Graphics > Mirrors** (Off, Low, Medium, High: 0 to 3 at once,
the biggest on screen first, counting mirrors seen inside another
mirror's reflection); a mirror seen deeper than that repeats what it last
showed, so two facing mirrors make an endless tunnel, each bounce dimmed
by `reflectionTint`. The rest, and all of them with Mirrors off, show plain
silver. Mirrors facing the same way in one flat wall count as
one. Reflections show everything the world draws: bricks, the map and its
sky and water, players, vehicles, items, particles, plants, weather and
Add-On code's world-space layers (not its view- or screen-space ones, which
belong to the player's screen). Name tags and hidden-brick outlines are
screen aids and stay out of mirrors.

**Portals (linked bricks).** A brick can also be a window onto another
brick: each of its `linkFaces` shows the view out of its partner, and with
`linkPass` players, vehicles, items and projectiles that go in come out of
the partner, turned the way the partner faces. The optional **Portal**
Add-On (`packages/brick_portal`, off until a player turns it on) is the
game's window again:

```text
server.cs         datablock fxDTSBrickData(brickPortal1x4x5Data : brick4x1x5windowData)
                  {
                      uiName = "1x4x5 Portal";
                      linkFaces = "north south";
                      linkName = "Portal";
                      linkDepth = 0.5;
                      linkPass = 1;
                      linkFrame = "0.05 0.05 0.2";
                  };
```

Two bricks of one kind, placed by one player, with the same brick **Name**
(the wrench's Name box every brick has; case does not matter) are a pair.
Placing two in a row names them to match (`Portal_1a2b3`), as Teledoors do.
Three or more of one name form a ring, each leading to the next in the
order they were placed. A brick with no partner shows its own glass and,
with `linkPass`, is shut. Going in through one side comes out of the
partner's opposite side when that side is open too (a doorway), else out of
the same side (a wall portal). Pairing follows from the bricks themselves,
so nothing extra is sent; each player's game draws the views, and the host
decides who goes through.

| Field | Meaning | Default |
|---|---|---|
| `linkFaces` | The open sides: `north south east west top bottom` | required |
| `linkName` | Stem of the names placing a pair gives: up to 16 letters, digits or underscores, starting with a letter | required |
| `linkDepth` | How far in the opening sits, as `reflectionDepth` | 0 |
| `linkInset` | Frame left around each view, in world units | 0 |
| `linkTint` | Colour the view is multiplied by, `"r g b"` | `"1 1 1"` |
| `linkIdle` | Colour a linked side shows when its view is not drawn live | `"0.35 0.42 0.55"` |
| `linkPass` | Whether things pass through; the brick's collision becomes a frame around each opening | 0 |
| `linkFrame` | Width of that frame, in world units: one number for every edge, or `"sides top bottom"` (the bottom is a sill bodies step over) | 0 |

Views share the mirrors' **Options > Graphics > Mirrors** budget, and a
portal seen through a portal repeats what it last showed, like facing
mirrors.

## 8. What players are asked to trust

Players download a server's Add-Ons when they join. What they are asked
depends on the most powerful thing an Add-On does:

| Tier | What the Add-On has | What the player sees |
|---|---|---|
| Data | rules, HUD panels, weapons, bricks, models, sounds | nothing: it downloads and runs |
| Sandboxed code | a `client` section: WebAssembly and WGSL run in the sandbox | "Trust and join" or "Leave", once per server, and again when the code changes; nothing when the player installed the same code themselves |
| Elevated code | `net.http` or `files.addon_folder` | a separate, stronger prompt per Add-On (not offered to joiners yet, see section 9) |

Rules always run on the host, never on players' PCs, so they need no
trust. Players can take any trust back with **Forget Trust** on the
Add-Ons screen. Ask for the smallest tier that does the job: most Add-Ons
are data only. The details are in
[client-sandbox.md](../architecture/client-sandbox.md).

## 9. Still being built

This guide changes in the same change as these land. Everything a total
conversion can and cannot change yet, area by area, is in
[total-conversion.md](../audits/total-conversion.md).

- **Brick authoring without v20 files**: a native brick format you write
  directly.
- **Drawing blocks**: `block` and `texture` content (per-face textures,
  flipbooks and states a script switches with `set_block_state`) load,
  save and replicate, but the renderer does not draw block faces yet.
- **Elevated client code**: joining asks "Trust and join" before a
  server's sandboxed client code runs (section 8), but code asking for
  `net.http` or `files.addon_folder` is not offered to joiners yet.
