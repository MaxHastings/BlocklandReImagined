# Making Add-Ons

This guide is for anyone who wants to make an Add-On for Blockland
ReImagined. It covers what works today; anything still being built is in
the last section, so you never write against something that does not exist
yet.

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

**Play** it: put the folder in the game's `content/addons/` folder, open
**Start Game > Add-Ons**, and turn it on. The screen turns on what it
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
| `aim()`, `me()`, `entities()` | `add_player(p, key, amount)` | `damage(p, amount)`, `explode(...)`: `damage` |
| `noise(seed, x, z)`, `hash3(seed, x, y, z)` | `entity_get(e, key)`, `entity_set(e, key, v)` | `spawn_entity`, `remove_entity`, `steer`, `label`: `entity` |
| `object(ref)`, `objects()`, `objects_near(x, y, z, r)`, `held(p)` | | `teleport`, `respawn`, `set_archetype`, `control(p, entity)`, `release(p)`, `give_item(p, item, equip)`: `player` |
| | | `copy_build(p, brick, limit, above_only, tool)`: `build` |
| | | `push`, `tumble`, `hold`, `let_go`, `spawn_vehicle`, `remove_vehicle`: `physics` |

A value from `players()` is a map with `id`, `name`, `x`, `y`, `z` (the
feet), `alive`, `admin`, `ex`, `ey`, `ez` (the eye), `lx`, `ly`, `lz` (the
unit direction they look), `vx`, `vy`, `vz` and `item` (the id of the item
in their hand, or `""`).

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
| `hold(player, ref, distance)` | Keeps `ref` floating `distance` (0.5 to 32) ahead of the player's eye, where they look, every tick until let go. Heavy things lag and sag. One hold per player; taking something another player holds ends their hold. |
| `let_go(player)`, `held(player)` | Ends the hold; what they hold, or `()`. |
| `spawn_vehicle(def, x, y, z, yaw, [vx, vy, vz], owner)` | A vehicle of this Add-On or one it depends on, belonging to `owner` (or `()`). Counts toward the server's vehicle limits; at most 64 per Add-On. |
| `remove_vehicle(ref)` | Removes a vehicle this Add-On spawned. |

The engine, not the script, decides who may move what: a player may move
another player when their minigame lets them hurt that player, or,
outside minigames, when that player trusts them to build; a vehicle when
its minigame lets them damage it, or, outside minigames, when they could
ride it; entities always. A hold is checked again as it goes and ends when
the rules stop allowing it, the holder dies, sits down or leaves, or the
object is dragged too far. What a push, tumble or hold moves is credited to
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
you use. Scripts run inside budgets (operations per call, sizes, call
depth): a runaway loop stops with a problem report instead of freezing the
server. Scripts cannot read files, open sockets or run `eval`.

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

A tool rather than a gun: give its image `"command": "your-rule:command"`
and no projectile. Its `onFire` state then runs that command of your rule
Add-On for the holder, with `aim()` resolved where they look (declare
`aim_reach` on the command). The Duplicator's `duplicator-tool` does this.

More moments can run commands through the image's `commands`:

```json
"commands": {
  "states": { "oncharge": "gravity-gun:charge", "onfire": "gravity-gun:fire" },
  "jet": "gravity-gun:grab"
}
```

`states` maps a state's `script` (lowercase) to a command, run as the
image enters that state: a state with `"down"` to a charging state whose
`"up"` leads to the firing state gives a press-and-hold charge.
`jet` runs when the holder presses jet (the right mouse button) with the
tool in hand, as v20's `onTrigger` slot 4 did; players who can jet still
jet. The Gravity Gun's `gravity-gun-tool` uses all three.

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
| `bricks`, `vehicles` | everyone | written by Import Add-On (section 7), or a `vehicles.json` you write | `packages/showcase/steel-ball-kit` |
| `texture`, `block` | everyone | a PNG for block faces (up to 1024 px a side); textures or flipbooks per face with named states | `crates/sim/tests/blocks.rs` |

Entities may spawn only their own Add-On's entity kinds.

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

**Client code.** An Add-On may also carry code that runs on players'
machines: a WebAssembly module and WGSL shaders, declared in a `client`
section of its `package.json` and run in a sandbox, for presentation only.
Start from [`spinning-cube`](../../packages/samples/spinning-cube), which
draws a cube with its own shader, then [`steel-ball-fx`](../../packages/showcase/steel-ball-fx)
(one shader drawn over every vehicle of a kind) and
[`gravity-gun-fx`](../../packages/showcase/gravity-gun-fx) (beams, a force
field and GPU particle systems driven by a rule's public state). With
`world.read`, code sees what the player's own screen shows: where players
and vehicles are drawn and the server's public Add-On state; `draw_with`
gives a draw its own shader parameters and `material_blend` makes glowing
(additive) or see-through layers; with `audio`, `sound_at` plays one of
its own `.wav` or `.ogg` files where something happens. Its capabilities (`render.layer`,
`render.shader`, `audio`, `input.focused`, `net.message`, `world.read`)
need the player to trust the server once; `net.http` and `files.addon_folder` need a
separate, stronger choice per Add-On. The format is in
[packages.md](../architecture/packages.md) ("Client code"), and the
sandbox's host API, budgets and checks in
[client-sandbox.md](../architecture/client-sandbox.md).

## 7. Old v20 Add-Ons and new bricks

Put an old Blockland Add-On (a `.zip` or a folder with `server.cs`) in the
game's `content/Add-Ons/` folder, open **Start Game > Add-Ons**, pick it
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

## 8. Still being built

This guide changes in the same change as these land.

- **Brick authoring without v20 files**: a native brick format you write
  directly.
- **Drawing blocks**: `block` and `texture` content (per-face textures,
  flipbooks and states a script switches with `set_block_state`) load,
  save and replicate, but the renderer does not draw block faces yet.
- **Elevated client code**: joining asks "Trust and join" before a
  server's sandboxed client code runs (section 6), but code asking for
  `net.http` or `files.addon_folder` is not offered to joiners yet.
