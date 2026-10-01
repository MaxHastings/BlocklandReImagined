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
| A tool that grabs, holds and throws players and vehicles | [`gravity-gun`](../../packages/showcase/gravity-gun) | a rule using the `physics` operations (section 3), its tool, and client effects |
| A new vehicle or loose physics object | [`steel-ball-kit`](../../packages/showcase/steel-ball-kit) | an `assets/vehicles.json` you write (section 6) |
| A bot for the Vehicle Spawn brick | [`blockhead_bot`](../../packages/blockhead_bot) | an `assets/bots.json` you write (section 6) |
| Effects drawn on every player's screen | [`gravity-gun-fx`](../../packages/showcase/gravity-gun-fx) | WebAssembly and WGSL shaders reading what the game shows (section 6) |
| New bricks | a v20-style brick Add-On you import (section 7) | a brick catalog the importer writes |
| A game mode in Start Game | [`stresslab-mode`](../../packages/stresslab/stresslab-mode) | a `mode` file naming Add-Ons and a map |
| A game mode on a world players dig into, with its own mini-game | [`crates/sim/tests/mode_and_voxels.rs`](../../crates/sim/tests/mode_and_voxels.rs) | rules with a generated world, a tool and a mode whose `minigame` block runs the game (section 6) |
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
| `on_damage(victim, attacker, amount, info)` | before a player is hurt, when `"on_damage": true`: return the amount to take (0 prevents it), `()` to leave it, or `#{ amount, type }` to change either and the damage type (a weapons pack's `$DamageType::<name>`, whose kill message a death shows: a crit's own). `info` is `#{ kind, type, direct }`, `kind` being `weapon`, `fall`, `package` and so on, `type` the damage type's name without `$DamageType::`. A shot or blast also gives `region` (`"head"`, `"torso"` or `"legs"`, where it struck) and its point `x`, `y`, `z`, and a weapon's hit `dx, dy, dz` (the unit direction it travelled, outward from the centre for a blast), so a shield can block hits from the front. A projectile's hit also gives `projectile`, its definition id (`"ns:projectile/name"`), so rules can tell shots apart when guns share a damage type, and `bounces`, the turns a ricocheting ray made before it (0 for a straight hit) |
| `on_entity_damage(entity, attacker, amount, info)` | before one of your creatures is hurt by a shot, a blast or `explode`, when `"on_entity_damage": true`: answered like `on_damage` |
| `on_vehicle_damage(vehicle, attacker, amount, info)` | before a vehicle is hurt by a shot, a blast, a smashing vehicle or a package's `damage`, when `"on_vehicle_damage": true`: answered like `on_damage`. `info` is `#{ kind, type, part, max_health, x, y, z }` plus `projectile` for a shot; `kind` is `weapon`, `package` or `smash`, `part` is `chassis` or an attached `turret`, and `max_health` is what that part takes to be destroyed |
| `on_entity_death(entity, killer, info)` | one of your creatures ran out of health, just before it is removed, when `"on_entity_death": true` |
| `on_pickup(player, item, info)` | a living player touches an item of your Add-On (or one it depends on) lying in the world, before they pick it up, whether or not they have room, when `"on_pickup": true`: return `false` to leave it, `"take"` to use it up without giving it (a spawn brick's item then starts its respawn), `()` for the usual pickup, or `#{ rounds }` to set what a dropped gun's magazine holds and then pick it up as usual (it keeps that count if they have no room). `info` is `#{ drop, spawner, data, rounds }`: the dropped item's id or the spawn brick's, what `on_drop` kept with it, and the rounds in a dropped gun's magazine (`()` for one that has none). Called as it happens, so keep it quick |
| `on_drop(player, item, slot)` | a player drops a tool of your Add-On (or one it depends on), when `"on_drop": true`. What it returns (a number, a map such as `#{ rounds: 7 }`) is kept with the dropped item and handed to `on_pickup` as `info.data` |
| `on_projectile_hit(hit)` | a projectile of your weapons (or a dependency's) struck something, delivered at the start of the next tick, when `"on_projectile_hit": true`. `hit` is `#{ projectile, by, kind, id, ref, x, y, z, nx, ny, nz, vx, vy, vz }`: `kind` is `player`, `vehicle`, `entity`, `brick` or `map`, `by` the shooter or `()`; a player hit also has `region` |
| `on_activate(player)` | a living player clicks with nothing in their hand (v20's `Player::activateStuff`), when `"on_activate": true`: return `true` to take the click, or anything else to pass it on. Add-Ons are asked in load order, and a click nobody takes does the usual thing (opens doors, presses buttons, flips vehicles) |
| `on_path_node(player, knot)` | a camera path the rules gave (`follow_path`) reaches knot `knot`, from 0, when `"on_path_node": true` |
| `on_observer(player, button)` | a spectator (dead with their respawn held, or under a rules camera) presses `"fire"`, `"jump"`, `"jet"` or `"light"`, when `"on_observer": true`: return `true` to take it |
| `on_minigame(event)` | something happened to a mini-game, delivered at the start of the next tick, when `"on_minigame": true`. `event` is `#{ kind, game, player, team }`: `kind` is `created`, `configured`, `reset`, `ended`, `joined`, `left`, `team` (`player`'s team changed to `team`, or `()`), `teams` (the game's team list changed) or `loaded` (a loaded build set the game up again, `per_minigame` state included) |
| `on_pick_spawn(player)` | a player is about to spawn or respawn, when `"on_pick_spawn": true`: return a brick id to appear on that brick, `[x, y, z]` to appear there, or `()` to leave it to the engine (spawn bricks, then the map). The first Add-On to answer decides. Called as it happens, so keep it quick |
| `on_brick_output(output, target, params, info)` | a builder's wrench row ran one of the outputs in `brick_outputs` (see **Brick events**) |
| `on_brick_input(input, brick, player)` | a player set off an engine input that some of your `brick_inputs` follow, on a brick with rows on one of them: return the name of one to run it too, or `()` (see **Brick events**) |
| `on_event_row(player, brick, row)` | a player sends a row of wrench events for a brick, before it is saved, when `"on_event_row": true`: return `false` or a reason to leave it out (see **Brick events**) |
| `on_zone(player, brick, event)` | a living player enters (`"enter"`), stays in (`"tick"`, with `"ticks": true`) or leaves (`"leave"`) the space over a brick of a kind listed in `zones` (a Torque trigger made with `createTrigger`) |
| `on_trigger(player, trigger, down)` | a living player with nothing in their hand presses (`down` true) or lets go of a trigger (v20's `Armor::onTrigger`), when `"on_trigger": true`. Trigger `0` is fire: the empty-hand click, whose press comes before `on_activate`. Return `true` to take it, anything else to pass it on: Add-Ons are asked in load order, presses and releases alike, and a press nobody takes goes on to `on_activate` and the usual click. A charged throw starts on the press and lets fly on the release |
| `on_drop_key(player)` | a living player with nothing in their hand presses the Drop Tool key (v20's `serverCmdDropTool` while `currTool` is -1), when `"on_drop_key": true`. Return `true` to take it; Add-Ons are asked in load order. Capture the Flag drops a carried flag this way |
| `cmd_<name>(player, args...)` | a player sends a command listed in `commands` |

`player` is the player's id: pass it straight to `tell`, `get_player` and
the rest. Loading checks that each hook exists with the right number of
parameters, so a typo shows up in `bri-addon-check`, not mid-game.

**Policies** are decisions the engine owns and asks you about. List them in
`"policies"` and define `allow_<policy>(player)`: `true` allows, `false` or
a reason string refuses. `respawn` is a dead player asking to come back,
`build` any command that builds, and `equip` taking a tool, the spray can,
the FX can or bricks into the hand or putting a tool away (v20 Add-Ons packaged
`serverCmdUseTool` and friends for this). Every Add-On that lists a policy
must allow it.

**Commands** are the only thing a player can ask of your script. Each has a
`name`, and optionally `args` (a list of `"int"`, `"float"`, `"string"` or
`"bool"`, for example `"args": ["int"]` for `cmd_gift(player, amount)`),
`cooldown_ticks` per player, `admin: true` to refuse non-administrators,
`aim_reach` (up to 1000 units) to have the engine resolve what the player is aiming at
(read it with `aim()`), and `tool_only: true` for a command only an image
runs (its `commands`, section 5): typed in chat or sent from a HUD it is
refused, so nobody types a gun's `/fire` or `/reload`. Players send a
command by typing it in chat,
`/sell coal`, or with a HUD panel's keys (section 4). Typed words become the
declared arguments in order; a final `string` argument takes the rest of the
line. When two Add-Ons declare the same command name, the typed form runs
the one whose package id comes last by name, as v20 ran Add-Ons in name
order and the last one's command won (both duplicators' `/dup`); a HUD key
names its package and always reaches its own.

**State** is declared up front with defaults. `player` keys exist for every
player; `global` keys once per server. `visible` says who receives the
value: `server` (the default; it never leaves the server), `owner` (a
player key sent only to that player, like a hand of cards) or `everyone`.
HUD panels can only show keys the viewer receives. `persist` (default
`true`) saves the key with the host's world and restores it after a restart.
A `global` key with `"per_minigame": true` (default `{}`) is a map from a
mini-game's id, as text, to that game's value, such as Slayer's fly-through
path. A game's entry travels with the build: saving a build keeps the
mini-game its saver runs (its settings, Add-On settings, teams and these
entries), and loading the build sets that up again in the game its loader
runs, or a new one of theirs, under that game's id, then sends
`on_minigame` a `loaded` event. Settings of Add-Ons the server does not
run are left out. Your rules remove a game's entry when it ends.

**Script functions:**

| Read | Change state | Act on the world (needs capability) |
|---|---|---|
| `tick()`, `seed()`, `caller()` | `get(key)`, `set(key, value)` | `tell(player, text)`, `broadcast(text)`: `chat` |
| `players()`, `bots()`, `player(id)` | `get_player(p, key)`, `set_player(p, key, v)` | `remove_brick`, `place_brick`, `set_block_state(brick, state)`: `world.edit` |
| `aim()`, `me()`, `entities()` | `add_player(p, key, amount)` | `damage(target, amount[, by[, type]])`, `explode(...)`: `damage` |
| `noise(seed, x, z)`, `hash3(seed, x, y, z)` | `entity_get(e, key)`, `entity_set(e, key, v)` | `spawn_entity`, `remove_entity`, `steer`, `label`: `entity` |
| `object(ref)`, `objects()`, `objects_near(x, y, z, r)`, `held(p)`, `tethered(p)` | | `teleport`, `respawn`, `set_archetype`, `push_archetype`, `pop_archetype`, `control(p, entity)`, `release(p)`, `give_item(p, item, equip)`, `take_item(p, item)`, `drop_item(item, x, y, z[, vx, vy, vz[, data]])`, `drop_item(item, #{ ... })`, `remove_drop(id)`, `name_drop(id, text, c)`: `player` |
| `raycast(from, dir, range[, ignore])`, `can_damage(by, target)`, `enabled(add_on)`, `lan()` | | `set_fov(p, fov)`, `set_speed_scale(p, scale)`, `set_image_ammo(p, ammo)`, `set_image_loaded(p, loaded)`, `mount_image(p, image)`, `mount_image(p, image, slot[, paint or #{ paint, keep }])`, `unmount_image(p)`, `emote(p, image[, skip_spam])`, `set_scale(p, scale)`, `set_look_limits(p, up, down)`, `orbit_camera(p, target[, nearest, farthest], distance[, body])`: `player` |
| | | `give_ammo(p, ammo, rounds)`, `set_reserve(p, ammo, rounds)`, `set_rounds(p, item, rounds)`, `reload(p)`: `player` |
| `minigames()`, `minigame(id)`, `setting(game, key)`, `team_setting(game, team, key)`, `server_setting(key)`, `pref(global)`, `bricks(kind)`, `brick(id)`, `palette()`, `drops()` | | `set_teams(game, teams, options)`, `set_team(p, team)`, `set_score(p, n)`, `add_score(p, n)`, `reset_minigame(game)`, `set_setting(game, key, v)`, `set_team_setting(game, team, key, v)`, `hold_respawn(p, held)`, `end_round(game, winners)`, `report_column(game, key, title, cells)`: `minigame`; `show_report(p, report)`, `hide_report(p)`: `chat`; `watch(p, target)`, `follow_path(p, knots)`, `free_camera(p)`, `orbit_point(p, at, distance)`: `player`; `set_brick_item(brick, item)`, `set_brick_color(brick, c)`: `world.edit`; `fire_brick_input(brick, input, p)`, `fire_game_input(game, input, p, killer)`, `set_brick_field(brick, key, v)`: `brick_events`; `brick_field(brick, key)` reads |
| `brick_box(brick)`, `voxel(brick)`, `can_place_voxel(x, y, z)` | | `place_voxel(x, y, z, material)`: `world.edit`; `set_avatar_colors(p, colors)`, `temp_look(p, look, seconds)`: `player` |
| `brick(id)`, `bricks_in(min, max)`, `can_plant(kind, [x, y, z], turns)`, `can_edit(brick)` | | `plant_brick(kind, [x, y, z], turns, color, owner)`: `world.edit` |
| | | `copy_build(p, brick, limit, way, tool[, options])`, `copy_box(p, min, max, limit, tool[, options])`, `mirror_copy(p, axis)`, `mirror_ghost(p, axis, asymmetric)`, `highlight_copy(p, rgba, seconds)`, `save_copy(p, name[, options])`, `load_copy(p, name, limit, tool[, options])`, `list_copies(p, filter)`, `plant_wait(p, seconds)`, `pivot_copy(p, pivot)`, `plant_as(p, target, admin)`: `build` |
| | | `cut_copy(p)`, `paint_copy(p, color)`, `paint_copy(p, paint)`, `wrench_copy(p)`, `super_cut(p, min, max)`, `fill_box(p, min, max, color)`, `paint_fill(p, brick, paint, options)`, `paint_vehicle(p, vehicle, paint, options)`: `world.edit` |
| | | `push`, `tumble`, `hold`, `reach`, `hold_distance`, `let_go`, `tether`, `tether_length`, `untether`, `spawn_vehicle`, `remove_vehicle`, `mount_object(mount, rider, node, can_dismount[, turn])`, `unmount_object(rider)`: `physics` |
| | | `heal(p, amount)`, `fire(...)`, `spawn_explosion(p, projectile, scale)`: `damage` |
| | | `play_sound(p, sound)` at a player's ears, `sound_at(sound, x, y, z)`, `beam(from, to[, options])`, `play_thread(p, thread, sequence[, after])`, `show_box(p, min, max, tool)`, `hide_box(p)`, `show_shapes(owner, key, shapes)`, `hide_shapes(owner, key)`: `effects` |
| | | `center_print(p, text, seconds)`, `bottom_print(p, text, seconds[, hide_bar])` (`()` for everyone), `tell_minigame(game, text[, except])`, `center_print_minigame(game, text, seconds)`, `bottom_print_minigame(game, text, seconds)` (a mini-game's members, counted once), `ask(p, title, text, command)` (a yes/no box; yes sends the package's own argument-less `command` as if typed, as v20's `MessageBoxYesNo` did), `plant_error(p, error)`, `message_box(p, title, text)` (an OK box, v20's `MessageBoxOK`): `chat` |
| `bot_kinds()` (each `#{ id, name, first_names }`), `bot_limit()` | | `add_bot(game, #{ kind, name[, team] })`, `remove_bot(bot)`, `rest_bot(bot, rest)`, `bot_tool(bot, slot or ())`: `bots` |
| | | `set_map_lights([x, y, z], radius, options)`: `lighting` |
| `environment()` | | `set_environment(#{ ... })`, `reset_environment()`: `environment` |

A string literal in your rules may colour its text as v20's did: `\c0`
to `\c9` pick the profile's colours, `\cr`, `\cp` and `\co` reset, push
and pop (`` `\c3Done \c6now` ``, or `"\\c3Done"` in double quotes).
They become colour codes when the rules compile, so text a player typed
or named never does.

Coming from TorqueScript? [torque-equivalents.md](torque-equivalents.md)
lists what each v20 call you know became here, and what is not here yet.

**Another Add-On's content.** A rule fires projectiles, mounts images and
spawns vehicles of its own package and of each Add-On in its
`dependencies`. One it uses only when that Add-On happens to be on, as a
v20 script tested `isObject(critProjectile)` for a datablock another
Add-On made, goes in `optional_dependencies` instead
(`"optional_dependencies": { "emote_critical": "*" }`): the game runs
without it, an enabled one must meet the requirement, and
`enabled("emote_critical")` says whether it is on in this game.

A value from `players()` is a map with `id`, `name`, `x`, `y`, `z` (the
feet), `alive`, `admin`, `ex`, `ey`, `ez` (the eye), `lx`, `ly`, `lz` (the
unit direction they look), `vx`, `vy`, `vz`, `item` (the id of the item
in their hand, or `""`), `minigame` (its id, or `()` outside one),
`health`, `max_health`, `archetype`, `crouched`, `mounted` (seated on a
vehicle or riding a player), `scale`, `cx`, `cy`, `cz` (the middle of the
body, `getWorldBoxCenter`), `slot` (the selected tool slot from 0, or
`()`), `image` (the image in their hand, or `""`), `image_state` (the
name of that image's state, such as `"Ready"`), `paint` (the palette
index their spray can last picked), `fx_can` (the FX can picked since, as
`serverCmdUseFXCan` numbers them from 0, or `()`), `may_paint` (their
minigame lets them paint, `enablePainting`), `mx`, `my`, `mz` (where the host fires
the held image's shots from, `getMuzzlePoint`; the eye when nothing is
held), `tools` (each tool slot's item id, `""` for an empty slot, as
`%obj.tool[%i]`), `team` (their team's id in their mini-game, or `()`),
`score` (their mini-game score), `magazine`: for a gun with a magazine in
hand, a map with `item`, `rounds`, `size`, `ammo` (its ammo type),
`reserve` (rounds of that ammo left to load, `()` when it never runs out)
and `reloading`; `()` otherwise; `reserves`, a map of every ammo type they
hold a reserve of (they drew a gun of it, or a rule set it) to its rounds
(`()` when it never runs out; a type missing from it gets its starting
reserve with the first gun of it they draw); `riding` and `seat` (the
player this one rides and on which mount point, or `()`), `bot` (`true`
for a bot) and `bot_owner` (for a bot from a bot brick, the brick owner's
id, as `%bot.spawnBrick.getGroup().bl_id`; else `()`), `spawner` (for a bot
a package added with `add_bot`, that package's id; else `()`) and `emote` (the
image worn in the emote slot, `""` for none). `lan()` says whether this is
a single-player or LAN game (`$Server::LAN`).

**Bots** are players without a connection. `players()` lists only people;
`bots()` lists the bots, as the same maps, and `player(id)` reads either.
Bots from bot bricks hear no player hooks. Bots a mini-game's rules add
(`add_bot`, below) are members like players: `on_spawn`, `on_loadout`,
`on_leave`, `on_pick_spawn`, zones, `on_death` and `on_minigame` hear them,
and they keep player state keys while they play (gone when they leave).
`on_join`, input hooks and policies stay for people.

**Bots for a mini-game** (the `bots` capability, v20 Slayer's
`addBotToGame`). `bot_kinds()` lists the bot kinds the enabled Add-Ons
provide, each `#{ id, name, first_names }` (`first_names` is the kind's own
name list, maybe empty). `add_bot(game, #{ kind, name, team })` adds a bot
of that kind to the game, on `team` when given; it joins when the
operations run, and the rules hear it join. Its brain is the engine's: it
spawns where members spawn, roams from wherever it stands, fights whoever
the damage rules let it hurt with the first weapon it carries (or the one
`bot_tool(bot, slot)` put in its hand; `bot_tool(bot, ())` puts its tools
away), and respawns as soon as its game lets it. `rest_bot(bot, true)`
holds it still with its fire held, `rest_bot(bot, false)` lets it go.
`remove_bot(bot)` takes it away; it also leaves when its game ends or it
is put out of it. A package can only move, arm or remove its own bots.
Spawn-brick bots and these share `bot_limit()`, 16 at once.

**Rays and damage.** `raycast([x, y, z], [dx, dy, dz], range)` returns the
first thing a ray meets, now, as the script runs: a map with `kind`
(`"player"`, `"vehicle"`, `"entity"`, `"brick"` or `"map"`), `id` (`()` for
the map), `ref` (an object like `"player:3"`, for players, vehicles and
entities), `x`, `y`, `z`, the surface normal `nx`, `ny`, `nz`, and
`distance`, and for a player `region` (`"head"`, `"torso"` or `"legs"`);
or `()` when it meets nothing. Rays reach up to 2000 units and
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
type would; without one the damage is your Add-On's own. `explode(x, y, z, radius, damage,
brick_radius)` hurts everyone within `radius` and knocks out bricks within
`brick_radius`, with the rocket's blast; a seventh argument names another
explosion of the weapons pack (`"rocketExplosion"`, or an imported
Add-On's own datablock name), whose particles, light, camera shake and
sound it then shows.

**Where a hit lands.** `hit_region(player, x, y, z)` names the part of a
living player's body at a point: the top 15% of their box is the `"head"`,
the next 30% the `"torso"`, the rest the `"legs"` (Torque's
`getDamageLocation` with the player defaults v20 never changed); `()` for
anyone else. The engine measures the same for `raycast`, `on_damage` and
`on_projectile_hit`, so a headshot rule reads `info.region == "head"`
instead of working it out.

**Held images.** `mount_image(p, image)` puts another image of your
weapons (or a dependency's) in the player's hand and keeps their tool
slot: a scope over a rifle, a second fire mode. `mount_image(p, ())` puts
the selected tool's own image back. `set_image_ammo(p, false)` tells the
held image it is out of ammo, so its states' `no_ammo` transitions run;
`true` gives it back. A gun's magazine is image data (section 5); rules
change it with `give_ammo(p, ammo, rounds)` (an ammo box, up to the most a
magazine of that ammo carries), `set_reserve(p, ammo, rounds)` (`()` for
ammo that never runs out), `set_rounds(p, item, rounds)` (the one in hand, else the first they
carry) and `reload(p)`.
`set_image_loaded(p, false)` unloads the held image (`setImageLoaded`),
so its states' `not_loaded` transitions run, and `true` runs their
`loaded` ones; putting an image in hand loads it. A state's `spin`
(`stop`, `spin_up`, `spin_down`, `full_speed`, v20's `stateSpinThread`)
turns the image's `spin` sequence while it is in that state: the New
Duplicator spins while a job runs this way.
`set_speed_scale(p, scale)` moves a player at that share of their running,
crouching and swimming speeds (0 to 4) until they respawn: a heavy gun's
slowdown. `set_fov(p, fov)` sets the player's field of view (5 to 120
degrees), and `set_fov(p, ())` hands it back to their own setting; aiming
and the zoom key still work on top of it. `unmount_image(p)` empties the
player's hand (`unMountImage(0)`): a tool, a spray can or bricks, which the
player's client puts away too.

**Worn images.** `mount_image(p, image, slot)` puts an image in one of a
player's two worn slots, `2` or `3` (`mountImage(%image, 3)`), beside
whatever they hold: a flag on the back, a hat. A fourth argument tints it
with a palette colour, for an image whose `paint_tint` is set, so one image
serves every team. `mount_image(p, (), slot)` takes it off; dying takes
every worn image off. The fourth argument may instead be `#{ paint, keep }`:
with `keep: true` no other Add-On replaces or takes off that image while it
is worn (Slayer CTF's `Player::mountImage` and `unMountImage` overrides
guarding a carried flag); its own Add-On still may, and once it is off the
slot is anyone's.

**Mini-games and teams.** `minigames()` lists the mini-games and
`minigame(id)` reads one: `#{ id, title, owner, members, round, teams,
friendly_fire, ally_same_color, round_over, player_type, loadout,
points_kill_player }`, each team `#{ id, name, color }`; `player_type`,
`loadout` (five item ids, `""` for an empty slot) and `points_kill_player`
are the game's own (`playerDatablock`, `startEquip`, `Points_KillPlayer`).
`set_teams(game, teams, #{ friendly_fire, ally_same_color })` sets a game's
teams: a team map with an `id` keeps that team and its members, one
without is new, and a team left out is removed. `set_team(p, team)` puts a
member on a team (or `()` for none), `set_score` and `add_score` change
their score, and `reset_minigame(game)` resets the game. The engine keeps
teammates from hurting each other while `friendly_fire` is off, sends team
chat to the team, and tells `on_minigame` about every change. These need
the `minigame` capability. A rule set like Slayer's sorts players with
`on_minigame` and spawns them on their team's bricks with `on_pick_spawn`.

**Add-On settings.** A rule declares the settings a mini-game's owner picks
in its behaviour's `settings`, and the host shows them in the Mini-Game
window's Add-On Settings, grouped by Add-On and `category`:

```json
"settings": [
  { "key": "mode", "title": "Game Mode", "category": "Minigame", "type": "list",
    "default": "dm", "items": [ { "value": "dm", "name": "Deathmatch" } ] },
  { "key": "lives", "title": "Lives", "category": "Victory Method",
    "type": "int", "default": 0, "min": 0, "max": 99 },
  { "key": "team_lives", "title": "Lives", "scope": "team",
    "type": "int", "default": -1, "min": -1, "max": 999 },
  { "key": "rule", "title": "Custom Rule", "type": "text", "default": "",
    "max_length": 150, "editor": "admin",
    "shown_when": { "setting": "mode", "is": ["dm"] } }
]
```

`type` is `bool`, `int` (with `min` and `max`), `list` (a pick from
`items`), `text` (up to `max_length`, at most 256), `item` (one of the
server's items, by id, or `""` for none, as Slayer's team start tools) or
`player_type` (one of the server's player types, by id, or `""`); the
window lists the server's choices for those two, and an id the server
lacks reads as `""`. `scope: "team"` gives
every team of the game its own value, edited beside the team's name and
colour in the same window, which also adds and removes teams.
`scope: "server"` gives the whole server one value, as RTB's
`$Pref::Server::*` preferences were: only the host changes it, in the Admin
menu's Add-On Settings, and it is kept with the host's Server Settings
(saved as `$Pref::Server::AddOn::<namespace>::<key>`, and kept for an
Add-On that is off until it runs again). A server setting may name the v20
global it stands for (`"global": "$Pref::Server::TT::Ammo"`). One with
`"restart": true` is read only as the server starts or loads a map (RTB's
needsRestart): it keeps the value it had then, and a host's change waits
for the next start, marked in the window.
`editor: "admin"` lets only an admin change it. `shown_when` hides a
setting unless another (`key`, or `ns:key` of an Add-On this one depends on)
holds one of the listed values; `"is_not": [...]` in place of `"is"` shows it
while that setting holds none of them (exactly one of the two lists values).
A dependent Add-On adds its own items to a
list setting with `setting_items` (`[{ "setting": "slayer:mode", "items":
[...] }]`), as Slayer's game modes do. The host checks every change against
the declaration and keeps only values that differ from the default, on the
mini-game and its teams.

`setting(game, key)` and `team_setting(game, team, key)` read a value (the
default when unset), `server_setting(key)` a server-wide one, and
`pref("$Pref::Server::TT::Ammo")` the server setting standing for that
global whichever running Add-On declares it, or `()` when none does (an
unset global), as every script read the one global; `set_setting(game, key, value)` and
`set_team_setting(game, team, key, value)` change it, `()` putting the
default back (`minigame` capability). `on_minigame` gets `kind ==
"settings"` with the changed `keys` when anyone changes them.

**Rounds.** `end_round(game, #{ teams: [...], players: [...] })` ends a
mini-game's round, won by those teams and players (or by nobody, with
neither): the game's `round_over` is `true` until its next reset, and every
rule hears `on_minigame` with `kind == "round_end"` and the winners'
`teams` and `players`. Slayer announces the winner and resets after its
time between rounds; a mode built on it (Capture the Flag) only calls
`end_round` (`minigame` capability). `hold_respawn(p, true)` keeps a dead mini-game member from
respawning (out of lives, the round over; v20 Slayer's `setDead`): their
client shows no respawn prompt, and a reset of the game or
`hold_respawn(p, false)` lets them go (`minigame` capability).
`watch(p, other)` turns `p`'s camera to orbit `other`'s body, `watch(p, p)`
their own body or corpse, and `watch(p, ())` hands control back, as a
respawn also does (`player` capability). It is the frozen kind of
`orbit_camera` below, 8 units out: a watching body does not fire, use
tools or click, and its keys go to `on_observer`. An admin's free camera
and a rule's `control` are left alone.

**Cameras.** `player(p).camera` is where `p`'s camera is and looks
(`getControlObject().getTransform()`): `#{ at: [x, y, z], yaw, pitch }`.
`follow_path(p, knots)` flies it along a path, as Torque's `PathCamera`,
while the body stands still: each knot is `#{ at, yaw, pitch, speed, type,
path, jump }`, `speed` units a second to the next knot (default 7),
`type` `"normal"`, `"kink"` or `"position"` (the next knot's view), `path`
`"spline"` or `"linear"`, and `jump: true` cutting straight to it. Up to 20
knots. With `"on_path_node": true` the rules hear `on_path_node(p, knot)`
as it reaches each one, from 0 (`PathCameraData::onNode`).
`follow_path(p, ())` hands control back. `free_camera(p)` lets a spectator
fly freely from where their camera is (`Camera::setMode("Observer")`, no
orb, no drop), and `orbit_point(p, [x, y, z], distance)` circles a point
0.5 to 100 units out (`setOrbitPointMode`); `watch(p, ())` ends either. All
are `player` capability and take only the body or another rules camera.
With `"on_observer": true`, `on_observer(p, button)` hears the keys of a
spectator, a dead player whose respawn a rule holds or one under a rules
camera: `"fire"`, `"jump"`, `"jet"` (`Observer::onTrigger`'s triggers 0, 2
and 4) or `"light"`. Return `true` to take the key; the next Add-On is
asked otherwise. Slayer's spectating and fly-through camera are these.

**Bricks.** `bricks(kind)` lists the bricks of one kind
(`"pkg:brick/flagstand"`, `"v20/brick/brickspawnpointdata"`), and
`brick(id)` reads one: `#{ id, kind, x, y, z, turns, min, max, color,
owner, game, name, item }`, `game` being the mini-game whose bricks it is (its owner's, as
v20's `minigameCanUse`), or `()`. `set_brick_item(brick, item)` sets the
item a brick holds out (`setItem`), or `()` for none: the world's bricks,
a mini-game's, or ones the calling player may build on. An item whose image
has `paint_tint` shows in its brick's colour. `set_brick_color(brick, c)`
repaints one of those bricks in palette colour `c` (`setColor`), and
`palette()` lists the palette as `[r, g, b, a]` from 0 to 1
(`getColorIDTable`). `set_brick_field(brick, key, v)` keeps a value on a
brick as your Add-On's `key` (letters, digits and `_`, up to 32), as a v20
script kept a dynamic field on a brick object, and `()` clears it; any
Add-On reads it with `brick_field(brick, key)` (your own key) or
`brick_field(brick, "namespace:key")` (another's). A value is at most 256
bytes and goes with its brick. Slayer keeps a brick's locked team colours
(`setTeamControlLocked`) as `locked`, and Capture the Flag refuses a locked
flag from it.

**Score reports.** `show_report(p, #{ title, banner, columns, sections })`
opens a window of its own for player `p` with a score table, as Slayer's End
of Round Report did: `banner` is large text over it ("VICTORY") or `()`,
`columns` is `[#{ key, title }]` after each row's name, and `sections` is
`[#{ title, rows: [#{ key, name, color, cells: #{ column: value } }] }]`,
names in palette colour `color` or `()` for the window's own. Cells are
written as text, `()` blank. `hide_report(p)` closes it. Another Add-On
changes a game's report with `report_column(game, key, title, cells)`: the
column `key` is retitled and filled by row `key` (`team:<id>`,
`player:<id>` by convention), or added at the end when the report has no
such column; `report_column(game, key, ())` takes it out. Changes hold for
the game until changed again, at most 8, and are put in when the report is
sent at the end of the tick, so the order Add-Ons' hooks run in does not
matter (Slayer's modes did this with `scoreListInit` and `scoreListAdd`).
Up to 11 columns, 4 sections, 256 rows and 64 characters a text.

**Brick events.** Rules may add inputs to the wrench's event list
(`registerInputEvent`), which builders wire to outputs like the engine's
own:

```json
"brick_inputs": [ { "name": "onFlagPickedUp", "targets": ["Player", "Client", "MiniGame"] } ]
```

Every input targets its brick (`Self`); `targets` adds any of `Player`,
`Client`, `MiniGame`, `OwnerPlayer` and `OwnerClient` (the brick
owner's, while they are on), and `Player(Killer)` and `Client(Killer)`. `fire_brick_input(brick, "onFlagPickedUp", p)`
(`processInputEvent`; capability `brick_events`) runs the rows on `brick`
wired to it, with `p` filling those targets (or leave `p` out). The rows
run as the brick owner's, under the same budgets and trust as any other.
A package fires only its own inputs, and a name the engine or another
Add-On already uses is refused when the Add-Ons start. Up to 32 per
package. Fired from inside `on_brick_output`, an input's rows run once
the tick's event rows are done.

`fire_game_input(game, "onMinigameDeath", p, killer)`
(`processMultiSourceInputEvent`) runs an input on every brick of the
mini-game `game` with rows on it, `MiniGame` being that game, `p` filling
`Player` and `Client` and `killer` filling `Player(Killer)` and
`Client(Killer)` (both may be left out).

An input may follow one of the engine's (an `onPlayerTouch` or
`onActivate` override, as Slayer's `onActivate(Team2)`):

```json
"brick_inputs": [ { "name": "onActivate(Team1)", "targets": ["Player", "Client"], "follows": "onActivate" } ]
```

When a player sets `onActivate` off on a brick with rows on any input
that follows it, the rules' `on_brick_input("onActivate", brick, p)` may
answer with one of them, which runs too, set off by the same player.

With `"on_event_row": true`, `on_event_row(p, brick, row)` reviews each
row a player sends from the wrench (`serverCmdAddEvent`), `row` being
`#{ index, input, target, class, output, package }` (`class` the
target's, as `MiniGame`). Return `false` (they hear "You may not use the
… event.") or a reason to tell them, and the row is left out; anything
else keeps it.

Rules may add outputs too (`registerOutputEvent`; capability
`brick_events`), up to 32, each acting on a `fxDTSBrick`, `Player`,
`GameConnection` or `MiniGame` with up to four parameters the wrench
shows:

```json
"brick_outputs": [ { "name": "setTeamControl", "class": "fxDTSBrick",
                     "params": [ { "type": "paint_color", "default": 0 } ] } ]
```

A parameter is `int` (`min`, `max`, `default`), `float` (`min`, `max`,
`step`, `default`), `bool`, `string` (`max_length`, `width`),
`paint_color` (`default`), `list` (`items`: `[name, number]` pairs, the
rules getting the number) or `vector` (`max_length`). A port writes them
from the original's own text with `{{name|event_params}}`, which reads a
`registerOutputEvent` parameter string (`"int 0 200 1" TAB "bool"`). A row
that runs one calls `on_brick_output(output, target, params, info)`:
`target` is the brick's id, the player's or the mini-game's,
`params` the row's values, and `info` is `#{ brick, owner, client, class,
target, base, input, row }`, `client` being whoever set the row off, or
`()`, and `target` the row's own target (below), or `()`. Return
`()`, or one of the package's own inputs to run next on the brick, as
`"onTeamCheckTrue"` or `#{ input: "onTeamCheckTrue", rows: [1, 4] }` to
run only rows 1 to 4 (Slayer's `checkTeam`). Its targets are filled from
whoever set the row off.

Rules may add targets as well (`registerEventTarget`; capability
`brick_events`), up to 8, each standing for something only the rules
know, found from one of an input's targets:

```json
"brick_targets": [ { "name": "Team(Client)", "class": "Slayer_TeamSO", "from": "Client" } ]
```

Every input with the `from` target (`Self`, the brick, or `Player`,
`Client`, `MiniGame`, `OwnerPlayer`, `OwnerClient`) lists it, the engine's
inputs and every Add-On's alike. Its `class` is the package's own, not one
of the engine's, and the package's `brick_outputs` of that class are its
outputs. A row aimed at it calls `on_brick_output` with `target` the
`from` entity (the client, the brick), `info.target` the target's name,
`info.class` its class and `info.base` the class of what `target` is; the
rules find what it stands for (Slayer: the client's team, or every team
of the brick's colour).

**Zones** are spaces over bricks that notice players, as Torque's triggers:

```json
"zones": [ { "bricks": ["pkg:brick/flagstand"], "above": 0.2, "period_ms": 150 } ]
```

Every `period_ms` (10 to 10000, default 100) the engine checks each living
player against the box of every brick of those kinds, raised by `above`,
and calls `on_zone(player, brick, "enter")` or `"leave"` as they come and
go (and `"tick"` while they stay, with `"ticks": true`).
`set_zone_period(zone, ms)` (`minigame`) changes how often zone `zone` (its
index in `zones`) is checked, as a script setting the trigger datablock's
`tickPeriodMS` did.

**Dropped items with data.** `drop_item(item, #{ at, velocity, paint, data,
seconds })` drops an item at `at` (`[x, y, z]`), thrown with `velocity`,
tinted with `paint`, carrying `data` (any value, up to 1 KB, handed to
`on_pickup` as `info.data`) and gone after `seconds` (at most 600).
`drops()` lists your Add-On's drops still lying in the world (`#{ id, item,
x, y, z, data }`) and `remove_drop(id)` takes one away. `name_drop(id,
text, c)` floats `text` (up to 32 characters) over one of them in palette
colour `c`, as `setShapeName` with `setShapeNameColor` did, and
`name_drop(id, ())` takes it away. A flag dropped where its carrier died is
this: `on_pickup` answers `false` and decides what touching it means, and
the seconds until it goes home count down over it.

**The emote slot.** `emote(p, image)` puts an image of your weapons (or a
dependency's) on a living player's body in the emote slot, as v20's
`%obj.emote(%image)` mounted it in image slot 3: every client plays its
states' emitters there, and it replaces whatever the slot wore, so an
emote, pain from a hit, flames or a teleport replace it in turn.
`emote(p, ())` takes it off. Its states follow their timeouts on the host,
and a state whose script has a command in the image's `commands.states`
runs that command for the wearer: a heal over time is an image whose
`onHeal` state heals a little each pass and, after enough passes, calls
`emote(p, ())`. Dying takes off an image that runs commands. Like v20's,
emotes are spam-checked: one under a second after the last counts, ten
quiet seconds forgive them, and past five counted an emote is dropped.
`emote(p, image, true)` skips the check (`%skipSpam`, as pain does).

**Bodies.** `set_scale(p, scale)` resizes a player's body, from 0.2 to 5
(`setScale`); a respawn puts it back to 1. `set_tools(p, [item, (), ...])`
fills a living player's tool slots in order (`forceEquip`, Slayer's team
start tools): an id per slot, `()` or `""` for an empty one, slots past the
list emptied, and items the server lacks left out (`player`, as
`give_item`). `set_respawn_time(p, ms)` makes their next deaths wait `ms`
(at most 999999) before they may respawn, over the mini-game's own
(`setRespawnTime`); `set_respawn_time(p, ())` and leaving the mini-game
give the game's back (`minigame`). `set_look_limits(p, up, down)`
bounds how far their arms and head follow their look, each from 0 (looking
straight up) to 1 (straight down), as v20's `setLookLimits`: `(0.5, 0.5)`
holds them level. `set_look_limits(p, ())` lifts it; a respawn does too.

**Cameras.** `orbit_camera(p, target, distance)` hands player `p` a camera
circling player `target`, 1 to 20 units out, which their mouse turns
(v20's `%client.camera.setOrbitMode(%target, ...)` then
`setControlObject(%client.camera)`).
`orbit_camera(p, target, nearest, farthest, distance)` lets their mouse
wheel zoom it a unit a notch between `nearest` and `farthest`
(`setOrbitMode(%target, %transform, %min, %max, %cur)`); it starts at
`distance`. Their body takes no moves meanwhile.
Their clicks do not end it: they still reach Add-Ons as the empty-hand
trigger (`on_trigger`, then `on_activate`), so a held player clicks to
struggle. `orbit_camera(p, ())` gives the body back; so does the target
leaving, and death and respawn as always. A player flying an admin camera
or driving an entity keeps it.
A last argument says what the body does: `"acts"`, the default above, or
`"frozen"` (`orbit_camera(p, target, nearest, farthest, distance,
"frozen")`), v20's plain `setControlObject(%client.camera)`: the body
takes no actions, every key, the click too, goes to the rules'
`on_observer` as a spectator's does, and it may circle the player
themselves or be given to the dead, from the body or any rules camera.
That is `watch`. `orbit_camera(p, (), "frozen")` ends any rules camera, as
`watch(p, ())` does; `orbit_camera(p, ())` ends only the acting kind, so
neither Add-On takes the other's camera.

**Riding players.** `mount_object(mount, rider, node, can_dismount)` seats
player `rider` on player `mount` at mount point `node` (`mountObject`).
`mount_object(mount, rider, node, can_dismount, turn)` also turns the
rider's body `turn` degrees (clockwise seen from above) on the mount point,
as a `setTransform` right after `mountObject` does in v20; their own turn
moves it from there, unless a camera has their moves.
Mount points are the body model's `mount0` to `mount7` nodes; on the
Blockhead `1` is the left hand. The rider rides along, turns with their own
mouse and drops what the gravity gun or a hold had of them; with
`can_dismount` `false` jumping does not get them off. They stay seated if
either body changes archetype or size. Both must be alive and not seated,
the mount carrying no rider on that point. A command's player may seat
someone only on themselves, and only someone they may move (the same rules
as `hold` and `push`). `unmount_object(rider)` lets them off in place,
moving as the mount was, so a throw is `unmount_object(t)` then
`push("player:" + t, ...)`. It also takes a player out of a vehicle seat
(`dismount()`), as a grab does in v20. A rule's command or hook may unmount
the rider themselves, their own rider, or a player it may move. Landing on a Blockhead still does not seat you:
only rideable bodies, such as the horse, take riders by touch.

**Effects** (`effects`) change nothing in the game and are sent once, like
a sound. `beam(from, to)` draws a straight beam for a moment: a tracer, a
laser, a bolt. Options go in a map, `beam(from, to, #{ color: [1.0, 0.8,
0.4], width: 0.05, seconds: 0.1, muzzle: p.id })`: `color` is `[r, g, b]`
or `[r, g, b, a]` from 0 to 1, `width` up to 16 units, `seconds` up to 10,
and `muzzle` starts it at that player's gun muzzle as each player draws it.
The beam thins and fades out over its life. `play_thread(p, thread,
sequence)` plays an animation on one of the body's four threads, as
`playThread` did: 0 and 1 the body (a hit's flinch, `"jump"`), 2 the arms
with what they hold, 3 a gesture (`"activate2"`). Each holds until the next
animation on its thread, and `"root"` stops it. An arm pose started with
empty hands (`"armreadyboth"`) keeps playing until the hand changes, and
whole-body sequences such as `"death1"` play over the walk and look as in
v20. `play_thread(p, thread, sequence, after)` plays it `after` seconds
later (up to 60), on the first tick at or past that time, as
`%player.schedule(ms, "playThread", ...)` did; a new body (a respawn) drops
what was still waiting, and one player holds at most 64 waiting.
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

**Environment** (`environment`) changes the sun, sky and fog for everyone
on the server, the same settings as the Admin Menu's Environment window.
`set_environment(#{ sun_elevation: 10.0, direct_light: [1.0, 0.6, 0.3] })`
sets only the keys it names; `()` puts a key back to the map's own
(`#{ fog_color: () }`), and `reset_environment()` puts them all back. A new
map starts with none set.

| Key | Value |
|---|---|
| `day_length` | seconds for a whole day and night, 10 to 86400; starts a day/night cycle (`()` stops it) |
| `time_of_day` | 0 midnight, 0.25 sunrise, 0.5 noon, 0.75 sunset; needs a cycle |
| `day_cycle` | `false` stops the cycle; `true` starts one of 300 seconds |
| `sun_azimuth`, `sun_elevation` | degrees, 0 to 360 and -90 to 90; with a cycle, where the sun is at noon |
| `direct_light`, `ambient_light` | `[r, g, b]` from 0 to 1: the sun's light and the light everywhere |
| `shadow_color` | `[r, g, b]`: the light where the sun does not reach, in place of the ambient light |
| `sun_flare_color`, `sun_flare_size` | `[r, g, b, a]` (a is how strong) and 0.1 to 4: a glow around the sun |
| `visible_distance`, `fog_distance` | units: where fog is complete (20 to 1000) and where it starts (0 to 1000) |
| `fog_color`, `sky_color` | `[r, g, b]`: the fog, and a tint over the map's sky |
| `vignette_color`, `vignette_multiply` | `[r, g, b, a]` darkening the screen's edges; `true` multiplies instead of blending |

`environment()` reads what is set, as the same keys; unset keys are absent,
and `time_of_day` is where a cycle is now. A cycle runs on every player's
clock from the server's tick, so a turning sun sends nothing after it
starts: use `day_length` for a moving sun rather than setting
`sun_azimuth` again and again. An Add-On gets 8 changes a second; each is
sent to every player. Players' own Visible Distance option still caps how
far they see.

`fire(projectile, x, y, z, vx, vy, vz)` launches a projectile of your
Add-On's weapons, or of an Add-On it depends on, from a point at a
velocity: a creature's gun, a turret, a fireball. Add a player as a last
argument to make it their shot, hurting whom their shots may; without one
it is your Add-On's own, which hurts any living player (as `damage` could)
and credits nobody. Start it clear of the shooter's body. 240 a second per
Add-On. The Commando's sentry does this from its think.

**Bricks by kind** (`world.edit`). `brick(id)` reads a placed brick:
`#{ id, kind, x, y, z, turns, min, max, color, owner, game, name, item }`
(the same map `bricks(kind)` lists), `kind` being its
brick catalog id (`v20/brick/brick2x4data`, or an imported Add-On's
`<ns>:brick/<datablock>`), `x, y, z` its centre, `turns` its clockwise
quarter turns, `color` its palette index and `owner` the build it is in (0
for the world's own). `bricks_in(min, max)` lists up to 1024 bricks
overlapping a box (`InitContainerBoxSearch`). `plant_brick(kind, [x, y, z],
turns, color, owner)` plants a brick of any loaded kind into build `owner`,
centred as near the point as the stud and plate grid allows, as v20 rules
did with `new fxDTSBrick(...).plant()`; where it does not fit (a brick, a
player or the map in the way) nothing is planted, so ask `can_plant` first
when it matters. A rule may plant into, or `remove_brick` from, a build its
caller has full trust on, or, inside a minigame, a build that minigame plays
with (its owner's bricks, or everyone's with Use All Players' Bricks); hooks
with no caller touch only the world's own bricks. `can_edit(brick)` asks
that before a rule changes anything. `aim()` also has `nx, ny,
nz`, the face the aim met. The Trench Digging port's rules
(`crates/addon-import/ports/gamemode_trenchdigging/rules`) are built on these.

**Moving things** (`physics`). Players, vehicles (every loose physics body:
jeeps, balls, the tumble of a knocked-down player) and package entities
are *objects*, named by a string: `"player:3"`, `"vehicle:12"`,
`"entity:7"`. `object(ref)` is a map with `ref`, `kind`, `id`,
`definition`, `x`, `y`, `z` (its middle), `vx`, `vy`, `vz`, `speed`,
`mass`, `radius`, `owner` and `spawner` (the Add-On that spawned it);
`objects()` lists every vehicle and bot and `objects_near(x, y, z, r)`
everything near a point. A bot (a Blockhead Bot) is a `player:` object
that is not among `players()`: its `definition` is its kind
(`bot.blockhead`) and its `owner` is whoever owns its spawn brick, who
decides, outside minigames, who may move it. A command with `aim_reach` also reports the nearest object in
front of the brick it hit: `aim().object`, `aim().object_distance`,
`aim().object_at` (`[x, y, z]`, where the aim met it) and
`aim().movable`, whether the caller may move it. The aim looks through
portals (the openings of linked bricks) as players see through them: its
`x, y, z` and `object_at` are where things are on the far side, and its
distances run along the sight, so take positions from these rather than
from the eye and the look. A `hold` or `reach` through a portal holds the
object on its side, and one carried through a portal stays held.

| Operation | Does |
|---|---|
| `push(ref, vx, vy, vz)`, `push(ref, vx, vy, vz, by)` | Adds to its velocity (units a second, at most 200). A living player may always be pushed by themselves (a shot turned back on its shooter). |
| `tumble(player, vx, vy, vz, by)`, `tumble(player, vx, vy, vz, by, seconds)` | Knocks a player off their feet into a tumble, flying at that velocity; for `seconds` (0.1 to 60) when given, else until it settles. |
| `hold(player, ref, distance)`, `hold(player, ref, distance, #{at, force, turn})` | Keeps `ref` floating `distance` (0.5 to 64) ahead of the player's eye, where they look, every tick until let go, carried at the velocity the aim point moves so it keeps up as they turn and walk. `at` (`[x, y, z]`, default its middle) is the spot on it that is held there, as a physics gun grabs where it points. `force` (default 36000, at most 10,000,000) is how hard it may pull: things up to `force / 450` in mass answer at once, heavier ones swing in slower and very heavy ones can only be dragged. With `turn`, it keeps the angle it had to the player as they turn. A living player held goes limp until let go and landed; a corpse (a player who died) can be held too. One hold per player; taking something another player holds ends their hold. |
| `hold_distance(player, distance)` | Moves what they hold nearer or farther (0.5 to 64): a reel. |
| `reach(player, distance, #{near, force, turn})` | While they hold nothing, the engine looks where they look every tick, up to `distance`, and holds the first thing they may move, by the spot it met, at least `near` off (`force`, `turn` as `hold`). Ends once it holds something, on `let_go`, or when they die. A trigger held at something out of range catches it when it comes in range. |
| `let_go(player)`, `held(player)`, `held_distance(player)` | Ends the hold and any reach; what they hold, or `()`; how far off it is carried, or `()`. |
| `tether(player, [x, y, z], length)`, `tether(player, [x, y, z], length, #{brick, object, reel, swing, keys, straight})` | Ropes the player to that point: they move freely within `length` (1 to 1000) of it and no farther, so they swing on it like a pendulum and keep their speed, and it goes slack when they come closer. `()` for `length` makes it as long as it spans now (at most 4 more than `length` otherwise). The rope holds them at their raised hands. `reel` (default 24, at most 80) is how fast `tether_length` winds it, braking to a stop at the end so a hard pull arrives gently; `swing` (default 7, at most 60) is how hard the movement keys pump a swing while the rope is taut and they are off the ground. With `brick`, the rope breaks when that brick goes. With `object` (a ref, `"player:3"` or `"vehicle:7"`; not the player themselves, not with `brick`), it is tied to that spot on it and the anchor rides along as it moves and turns, carrying the player too, and it breaks when that goes. `keys` (`[shortest, longest]`) lets the player's jump key reel it in and crouch let it out while held, stopping when let go. With `straight: true`, reeling in draws the player straight along the rope instead of letting them swing: a grappling hook's pull. Only the player whose command asked may be roped, one rope each; the player's own prediction runs the same rope (keys and moving anchors included), so it feels the same on their screen. |
| `tether_length(player, length)` | Reels the rope in or out toward that length (1 to 1000): a winch. It takes over from the winch keys until they are pressed again. |
| `untether(player)`, `untether(player, #{keep})`, `tethered(player)` | Cuts the rope; with `keep` (0 to 1) the player keeps only that fraction of their speed relative to what the rope was tied to, as a winch's grip slows them letting go. `tethered` gives the rope (`#{x, y, z, length, target, brick, object}`), or `()`. |
| `spawn_vehicle(def, x, y, z, yaw, [vx, vy, vz], owner)` | A vehicle of this Add-On or one it depends on, belonging to `owner` (or `()`). Counts toward the server's vehicle limits; at most 64 per Add-On. |
| `remove_vehicle(ref)` | Removes a vehicle this Add-On spawned, or, when a player's command asks, one of this Add-On's (or a dependency's) kinds that the player owns (an administrator: anyone's). A spawn-brick vehicle removed this way stays away until the brick's wrench asks again. |

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
too far from where it should be. A rope breaks when its player dies,
teleports, respawns, sits down, tumbles or is held, or is carried more
than 12 past its length, and when its brick or object goes; `tethered(p)` turns
`()` and the rule sees it. What a push, tumble or hold moves is credited to
`by` (the caller, by default) for five seconds: a vehicle that then runs
someone over, or smashes bricks, does it as that player.

`give_item(p, item, equip)` puts a weapon or tool in the player's tool list
(unless they carry it already) and, with `equip`, in their hand.
`take_item(p, item)` takes one back: the held slot if it holds that item,
else the first slot that does, putting it away if it is in hand (a thrown
axe leaves the thrower's tools). `drop_item(item, x, y, z)` puts an item in
the world as a pickup anyone may take at once, popping after ten seconds
like a dropped tool; add `vx, vy, vz` (at most 200 units a second) to throw
it, and an eighth argument, `data` (a map or number, as `on_drop` keeps),
to hand `on_pickup` as `info.data`: a dead player's bag holding their
ammo. An Add-On has at most 64 of these lying about at once. With
`on_pickup` and `on_drop` they make ammo boxes, magazines that stay with a
dropped gun, and thrown weapons that land as pickups.
`copy_build(p, brick, limit, way, tool)` copies the stack at `brick` for
the player whose command asked, as v20's duplicators selected one: the
brick, then every brick joined by studs to one already taken, going only
`"up"` (what is built on it) or `"down"` (what it is built on) from the
clicked brick itself and both ways from the rest, nearest first and cut
short at `limit` bricks (at most 10000). The player sees the copy as a
ghost while `tool` is in hand, moves and turns it with the brick keys, and
plants it with the plant key; planting follows the server's plant rules,
plants all of it or none, and one Ctrl+Z takes it back.

A last `options` map changes what a copy may take and how it plants:
`trust` (the trust the player needs in a brick's owner: `"build"`, the
default, `"full"`, `"none"` for anyone's bricks or `"self"` for only
their own), `public_bricks` (public bricks may be copied; default
true), `admin` (administrators may copy any brick; default true),
`partial` (planting plants each brick that fits and skips the rest, as
v20's Duplorcator did; default false), `stack` (the same trust in the
owner of a brick's stack also counts: whoever owns the bricks it was built
on, v20's `stackBL_ID`, so the copy's player may also cut, paint or wrench
through the copy what others built on their stack with their full trust;
default false) and, for a stack, `limited` (keep
the stack on its side of the clicked brick: going up, nothing reaching
below its bottom). `may_copy(p, brick, options)` says whether a copy with
those options would take a brick, before the player picks it. `highlight_copy(p, [r, g, b, a], seconds)` then lights
the copied bricks in the palette colour nearest that one (`()` for each
brick's own), glowing, for everyone to see, and gives them their own
colours back after (a negative `seconds` keeps them lit until the copy is
let go, lit again or taken up to place; 0 puts them out now); a copy
takes a lit brick as it is underneath. An Add-On with `on_copy` in its
behaviour hears `on_copy(player, #{ action, name, bricks, total,
limit_reached, refused, error, message, size, names, queued, searched })` instead of the player getting
the engine's message (`action` is `"select"`, `"save"`, `"load"`,
`"list"` or `"plant_as"`, below;
`size` is the held copy's `[studs, plates, studs]` along x, up and z, or
`()` when it holds none), and
with `on_place`, `on_place(player, #{ planted, bricks, error, message,
failed, wait, mirror_errors, float_refused })` after the player plants its copy (`failed` counts the bricks
each plant error kept out, `#{ float: 2, overlap: 1 }`; a partial plant
tries a floating brick again once the rest are in). `mirror_errors` is
`#{ side, upside_down }`, the catalog names (`Category/Group/Name`) of
the bricks a mirrored plant had no exact mirror image for, across and
upside down. `plant_wait(p, seconds)` makes each of the player's copy
plants wait that long after the last (0 to 60; 0, the default, none): one
sooner is refused and `on_place` hears `error` `"wait"` with the seconds
left in `wait`. `float_copy(p, float)` lets the copy's plants float in
mid air (v20's Force Plant); with `#{ admin_only: true }` that is checked
again at each plant, and a plant by a player no longer an administrator
goes in as a normal one, with `float_refused` true in `on_place` and the
float turned off. Only the player sees the bricks of the copy they place;
with `on_copy_ghost`, `on_copy_ghost(player, #{ box })` hears the box
round it (`#{ min, max }`, world units) each time they move, turn, mirror
or flip it, and `()` once it is put away or let go, for the Add-On to show
the others where it stands (the New Duplicator's blue box).

Copies can be kept by name on the host. `save_copy(p, name)` keeps the copy
the player holds, replacing one saved under that name; with `#{ overwrite:
false }` it keeps the old one and reports `error` `"exists"`.
`list_copies(p, filter)` gives `on_copy` the saved names containing
`filter` (any case; all when empty) in order (`action` `"list"`,
`names`); `load_copy(p, name, limit, tool[, #{ partial }])` gives
them the copy saved under that name, its first `limit` bricks, in this
world's nearest colours. Saved copies are the host's, whichever duplicator
saved them, and loading also finds v20 duplication files (Plornt's
Duplorcator and Zeblote's New Duplicator wrote them) that the host's
player dropped in its `saves/Duplications` folder, moved onto the grid. The
game never looks for them in a Blockland install. The host reads and writes
them while the game runs on, so the answer comes to `on_copy` a tick or
more later; `error` is then also `missing` (no copy by that name),
`unavailable` (this host keeps none), `busy` (the player's last one is
still going) or `failed`. `copy_name(text)` turns what a player typed into
a name a copy may have (the file name only, without `.bls`), or `()`.

`copy_box(p, [x, y, z], [x, y, z], limit, tool)` copies instead every
brick lying wholly inside a box (world units, grown out to whole studs
and plates, at most 1024 units a side) that the player may build on,
lowest first, with the same options; `limited: false` takes every brick
reaching into the box as well.
`brick_box(brick)` gives the box a brick fills, `#{ min: [x, y, z], max:
[x, y, z] }`, to build boxes from clicked bricks. `mirror_copy(p, axis)`
mirrors the copy the player holds, across `"x"` or `"z"` (the world's
axes) or `"view"` (left and right as they face): each brick crosses to the
other side and becomes its mirror image, the same brick turned or its
twin in the catalog (a left wedge for a right one), found from the bricks'
own shapes; a brick with no twin keeps its shape (`on_place` names those
in `mirror_errors`). The mirror is part of
where the player puts the copy, like its turn. `mirror_ghost(p, axis,
asymmetric)` does the same to the player's ghost brick where it stands
(`player(p).ghost` says whether they have one out, bricks in hand); a
brick with no exact image in that mirror stays as it is and the player is
told `asymmetric`. `move_copy(p, point,
normal)` puts the copy against the surface at `point` whose outward
`normal` is given (a `raycast` hit's), as a ghost brick goes where it is
aimed: the middle of the copy's box half its size out along the normal,
its pivot on the grid. `pivot_copy(p, "start")` makes the copy turn about,
and go against what is clicked by, the brick it was taken from first (the
clicked brick of a stack) instead of its whole box (`"whole"`, the
default); the player's game is told, as the turn keys are theirs.
`plant_as(p, target, admin)` plants the player's copies into another brick
group: `target` is a player's name or part of it, or a BL_ID with bricks in
the world; `""` plants into their own again. They need build trust with
that group, or `admin` and to be an administrator, when choosing and at
each plant (`on_place` hears `error` `"group"` once that is gone).
`on_copy` hears `action` `"plant_as"` with the group's `name`, or `error`
`"missing"` or `"trust"`. Undoing such a plant takes back the group's
bricks.

`drop_copy(p)` takes the copy away from the player. A copy remembers the
bricks it was taken from. `cut_copy(p)` removes them
and `paint_copy(p, color)` paints them, all or none, with the player's own
full trust (the hammer's and spray can's), each as one Ctrl+Z step; the
undo of a cut puts every brick back exactly as it was, events, lights and
owner included; an Add-On with `on_copy` hears how a cut went there
(`action` `"cut"`, `error` `"empty"` or `"refused"`). `cut_copy(p, #{ each:
true })` cuts each brick the player may and leaves the rest, counted in
`refused` (the New Duplicator's cut). With
`"undo_confirm_over": n` in its behaviour, a player's Ctrl+Z of one of
these steps (a plant, paint, wrench, cut or fill) changing more than `n`
bricks is held the first time: `on_copy` hears `action` `"undo"` with the
`bricks` it would change, and the next Ctrl+Z goes ahead (any other undo
between starts over), as the New Duplicator asked before a big undo. `show_box(p, min, max, tool)` outlines a box on that
player's screen while `tool` is in their hand (a selection, a zone being
marked) and `hide_box(p)` takes it away. The port of the New Duplicator
uses them all.

`show_shapes(owner, key, shapes)` draws boxes in the world that every
player sees, joiners too, as Torque Add-Ons did with scaled
`StaticShape`s: each shape is `#{ min, max, color, inside, sides, label }`,
colours RGBA from 0 to 1. Its faces are `color` seen from outside and
`inside` seen from within (alpha 0, the default, draws none), `sides` gives
the faces across x, y and z their own outside colours (a shaded cube), and
`label` is drawn over its top centre like a player's name, in `color`.
The set replaces the one the package last showed under `key` (64 shapes
at most, each side up to 1,040 units); `hide_shapes(owner, key)` takes it
away. With a player as `owner` the set is theirs and goes when they
leave; `()` shows one nobody owns. The New Duplicator's port draws its
selection box and the edges round a selection this way.
`plant_error(p, error)` shows v20's plant error to a player (`"overlap"`,
`"float"`, `"stuck"`, `"buried"`, `"too_far"`, `"limit"` or `"flood"`):
the icon and sound `MsgPlantError_…` gave. `player(p).copy` is the copy
they hold, `#{ addon, bricks }` (the Add-On that took it), or `()`.

Big copy work goes on over several ticks, a slice each tick, so a copy
of up to 1,000,000 bricks (the most any `limit` may be) never holds the
server up. Selecting, planting, cutting, painting, wrenching, loading and
undoing a copy all work this way, and so do `super_cut` and `fill_box`
(each `on_copy` report has `placed` and `refused`; a fill stops at the
server's brick limit with `limit_reached`); one that fits in the tick's slice
finishes at once as before. While a player's job runs,
`player(p).copy_working` is true, their other copy work and undo are
refused as busy, and `on_copy` hears `working: true` with the `action`,
`bricks` done and `total` four times a second (a stack selection also
gives the bricks still `queued` to look around; a box selection, and a
plant's later passes for bricks that now have something under them, how
far they have `searched` in percent, otherwise -1) (the engine shows
"Working... (N%)" for an Add-On without `on_copy`). `cancel_copy(p)`
stops it: what it did stays done, as one undo step, and the job's report
comes as usual (`on_place` with `canceled: true`; any other cancelled
job's `on_copy` has `error` `"canceled"`, with what it did). An undo done over several ticks ends with `on_copy`
`action` `"undone"`. A held copy's player sees at most 10,000 of its
bricks as the ghost; `on_copy` gives how many as `ghosted`.

**Digging and filling a generated world.** In a `world` Add-On's world
every cube is a brick, so `remove_brick` digs one out. `voxel(brick)`
says whether a brick is one of the world's cubes: `#{ x, y, z, material }`
in cube coordinates, or `()` for any other brick. `place_voxel(x, y, z,
material)` puts a cube of one of the world's materials back (`world.edit`,
out of the same share of 2,048 edits a second as `remove_brick`), and
`can_place_voxel(x, y, z)` says whether it would fit now: inside the
world, its chunk generated, and no brick, player or vehicle in the way.
Dug and placed cubes are saved with the world. The spade in
[`crates/sim/tests/mode_and_voxels.rs`](../../crates/sim/tests/mode_and_voxels.rs)
digs dirt out and piles it back up this way.

**Fills.** `paint_fill(p, brick, paint, options)` paints `brick` and every
brick of its colour joined to it as `p`'s spray cans would (their full
trust, brick by brick; the minigame's paint rule), as one Ctrl+Z.
`paint` is `#{ color: n }`, `#{ color_effect: n }` (0 to 6) or
`#{ shape_effect: n }` (0 to 2). `options`: `limit` (bricks, 1 to 128000),
`reach: [sideways, vertical]` (join bricks whose boxes overlap a brick's box
grown by that much, as v20's `containerBoxSearch` did; without it bricks
join through shared faces), `stop_at_limit` (paint the first `limit` and
stop, as v20's Fill Can did, instead of refusing), `limit_message: [text,
seconds]` (shown when it stops there), `limit_error: true` (the plant-limit
error icon, and its sound where the player turned it on, when it stops
there: v20's `MsgPlantError_Limit`) and `refusal_seconds` (how long "does
not trust you enough" shows). Only the player whose command or shot asked
may be filled for. Undo puts back only bricks still as the fill left them.

**Vehicles.** `paint_vehicle(p, vehicle, paint, options)` paints a vehicle
(`"vehicle:3"` or its id) as `p` (full trust from its spawn brick's build,
or its owner's for one no brick spawned; the minigame's paint rule), as one
Ctrl+Z. `paint` is `#{ color: n }`, a palette colour, which a vehicle its
spawn brick recolours takes through the brick (the brick is painted too,
as `fxDTSBrick::colorVehicle` reads it), or `#{ rgb: [r, g, b] }` on the
vehicle alone until it respawns. `options`: `riders_seconds` (its riders
take the colour that long, as `setTempColor`) and `refusal_seconds`. Only
the player whose command or shot asked may paint.

**For a moment.** `temp_look(p, #{ color: [r, g, b, a] }, seconds)` or
`#{ paint: n }` colours every part and hides the decal (`SetTempColor`);
`#{ face: "smileyEvil1", alpha: #{ accent: 0.7 } }` changes the face and
fades worn parts (`setFaceName`, a visor's `setNodeColor`). A colour and a
face each last their own time and end when the player respawns.

**Paint pickers.** An image with `paint_picker: true` stays in hand when its
holder picks a colour or FX can (v20 Add-Ons packaged `serverCmdUseSprayCan`
and `serverCmdUseFXCan` to remount theirs); the pick shows in
`player(p).paint` and `fx_can`.

**Uniforms.** `set_avatar_colors(p, #{ torso: [0.8, 0.1, 0.1], rarm: [...] })`
paints parts of a player's own look with the rule's colours (`player`),
as v20's `setNodeColor` did for team games; `set_avatar_colors(p, ())`
gives them back their own. The parts are `head`, `torso`, `hat`,
`accent`, `pack`, `secondpack`, `hip`, `rarm`, `larm`, `rhand`, `lhand`,
`rleg` and `lleg`; colours are 0 to 1, with an optional alpha. Everyone
sees the change with the player's look: it costs nothing beyond it.
`set_avatar_parts(p, #{ hat: "copHat", pack: "none", face: "smiley",
decal: "AAA-None" })` dresses them in parts too (`hideAllNodes` and
`unHideNode`, a full team uniform), per part slot (`hat`, `accent`, `pack`,
`secondpack`, `chest`, `hip`, `rarm`, `larm`, `rhand`, `lhand`, `rleg`,
`lleg`) with a face and decal; a part the server's avatar pack lacks stays
theirs, and an accent their hat cannot wear comes off.
`set_avatar_parts(p, ())` gives them their own. `avatar_choices()` lists
the pack's choices in v20's order, a slot's list (`hat`, `face`, `decal`)
and each hat's accents (`accents.helmet`), so a rule reading v20's list
positions (`$pref::Avatar::Hat` 6) finds the part. Both last across
respawns until the rule changes them.

**Filling.** `paint_fill(p, brick, color, limit)` paints `brick` and every
brick of the same colour joined to it through shared faces (side by side,
stacked or hanging under, joined by studs or not; bricks meeting only along
an edge are not joined) in palette colour `color`, as player `p`'s spray
can would paint each one, as one Ctrl+Z step. It needs the player's full
trust on `brick`; beyond it the fill flows around bricks they may not paint
and never through them. More than `limit` bricks (at most 10000) is refused
rather than cut short, so a fill never stops half way across a wall. The
player sees how many bricks turned, and how many touching ones were not
theirs to paint. A fill tool is one command around it; the test's own
([`crates/sim/tests/fixtures/fill-can`](../../crates/sim/tests/fixtures/fill-can))
shows one.

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
- `holding` (optional, up to 32) shows the panel only while the viewer
  holds one of these: an Add-On id (any of its images) or an image id.
  An ammo panel listing `["my-guns"]` appears with one of that Add-On's
  guns out and nowhere else.

## 5. Weapons

A weapon Add-On is an `assets/weapons.json` file, listed in `provides` as
`{ "kind": "weapons", "id": "your-id:weapons/main", "file":
"assets/weapons.json" }`. It is the same format **Import Add-On** writes
for v20 weapons: `items` (what players hold), `images` (the held model and
its firing states), `projectiles`, `damage_types` and `explosions`. Model
and icon paths may point at base game files; the sample reuses
`Add-Ons/Weapon_Gun/pistol.dts`. An item's `icon` may also be your own
PNG (up to 512 pixels a side), named without `.png` relative to
`assets/`: the Gravity Gun's `"icon": "icons/gravity_gun"` is
`assets/icons/gravity_gun.png`. With neither, the item shows its first
letter.

An item, image or projectile `model` may also be your own model: a native
model file (`*.shape.json`, the format of `bri_content::shape`: x right, y
up, -z forward) relative to `assets/`, such as
`"model": "models/pick.shape.json"`. Nodes `muzzlePoint` and
`ejectPoint` say where a gun fires and throws its casings. Each material names a PNG
beside the model (`pick_wood` draws `pick_wood.png`, up to 1,024 pixels a
side), as a vehicle model's do. A node called `mountPoint` is where the hand
holds it. A `detail9999` detail is what the holder sees in first person
and the lower details what everyone else sees, so an image state's
`sequence` (the pick's `"fire"`) can swing the first-person copy alone.
Its box is its bounds for dropping; `bri-addon-check` names a model or
texture it cannot find. `bri-client`'s `own_model_tool` test fixture writes a
small example.

An item has one look wherever it is: in a hand (first or third person),
dropped, on a spawn brick, in a mirror and as its icon. The look is its
image's: the image's `model`, its `color` when `color_shift` is on, and
its skin. An item whose image has no model draws the item's own `model`.

A skin is your own shader drawn over every copy of one of your images,
puffed over its model. Put `looks.json` in `assets/`:

```json
{ "schema_version": 1,
  "images": { "gravity-gun-tool:image/gravitygun":
    { "skin": { "shader": "skins/alien.wgsl", "color": [0.3, 0.95, 1.0],
                "energy_states": ["Grab"] } } } }
```

The shader is a WGSL file in your Add-On, written and limited like an
Add-On's own client shaders (see
[client-sandbox.md](../architecture/client-sandbox.md)): the game
draws it in world space with your image's model at rest, and gives each
copy `params[0]` = `color` and its energy (1 while its holder's image is
in one of `energy_states`, else 0), `params[1]` = the direction sunlight
travels and a seed for that copy, `params[2]` = the sun's colour and
`params[3]` = the ambient light. Skins share an Add-On's GPU budget; one
that runs far over it stops, and items draw their plain models. A skin
that names an image that is not yours, a colour outside 0 to 1, or a
shader that is missing or does not compile is logged, and the item draws
without it. A skin is WGSL, so it is held to the same trust as Add-On code
(section 8): a server's skins draw on a joiner's screen only once they
trust that server's code, and the item draws plain until then.
`skins/alien.wgsl` in `gravity-gun-tool` is the Gravity Gun's.

A tool whose image has `"paint_tint": true` (below) is dropped in the
colour it was held in.

An icon can instead be drawn from the item's own model on each player's
machine, so it matches the stock icons without shipping a picture of
base game art. Put `<icon>.render.json` beside it:

```json
{ "schema_version": 1, "pose_like": "v20.weapon.printgun",
  "look": { "skin": {} } }
```

`pose_like` names a stock item: its model is fitted to its own icon's
outline to find the side profile it was drawn in (which way its nose
points across the picture, and how far it is tipped and turned). Your model
is drawn in that profile on its own axes: forward is +Y and up is +Z, as
item models are held, or from `mountPoint` towards `muzzlePoint` when it has
both. So its nose and grip point the way the stock item's do. It is sized
to its own bounds to fill the box the stock drawing fills, with a clear
border on every side, on a clear background. The model is the item's in
play. `look.base` is its colour, by default the item's colour in play
(its image's tint); `"textured": true` draws the model's own
textures and colours instead (times `base`), so a tool of wood and iron
shows both. The
optional `skin` is the Gravity Gun's alien shell: a dark `shell` with glowing
`veins` (by default the colour of the item's skin in `looks.json`), puffed
out by `puff` (default 0.012) as it is in play. If the icon
cannot be drawn, the item keeps its PNG or letter and the log says why.
The icon is drawn once on a background thread while the game loads (the
PNG or letter shows until it is ready) and kept in the client state folder
under `item-icons/`, named by a hash of the models, the stock icon and the
request, so later runs show it at once.

An image with `"paint_tint": true` is held in its holder's spray colour,
the palette colour they last picked with the paint keys, as a colour spray
can is: a tool that paints with that colour shows it.

An image with `"light": { "radius": 20, "color": [1, 1, 1] }` lights the
world around it while a player holds or wears it, as a v20 image's
`hasLight` with `lightType = ConstantLight` did; the importer reads those
fields (`lightRadius` up to 100, `lightColor`). Worn in a paint colour it
lights in that colour: Capture the Flag's flag glows in its team's colour on
the carrier's back. Other light types are noted, not drawn yet.

The fields you are most likely to change:

| Where | Field | Meaning |
|---|---|---|
| projectile | `speed`, `gravity`, `lifetime_ticks` | how fast, how much it drops, how long it lives (120 ticks = 1 s) |
| projectile | `damage`, `impulse`, `vertical` | hurt, and how hard it shoves |
| projectile | `ballistic`, `elasticity` | bounces, and how much |
| image state | `ticks` | how long a state (`Fire` is the reload time) lasts |
| image | `shot` | several projectiles per shot, their spread and the recoil ([porting.md](porting.md#the-image-shot-field)); `scale` sizes the projectiles against their holder (0.1 to 10) |
| image | `volleys` | up to 4 more sets each shot fires after its own: `[{ "projectile": "...", "projectiles": 1, "spread": 0.0005 }]`, a shotgun's slug after its pellets |
| image | `last_shot` | `{ "shot": {...}, "volleys": [...] }`: what the magazine's last `last_rounds` rounds fire instead (a two-barrel gun's single barrel) |
| image | `state_shots` | `{ "onfire2": {...} }`: a shot fired on entering a state with that script, as onFire's is, so a gun can spread wider as it keeps firing |
| image state | `arm`, `gesture` | the holder's animation on entering it: `arm` on thread 2 (`shiftright`), `gesture` on thread 3, the other hand; `"arm_once": true` does not play `arm` again as the state times out into itself (an arm raised while a throw waits) |
| image state | `cues` | up to 16 things a while after it is entered, as scripts scheduled them: `[{ "after_ms": 450, "thread": 2, "sequence": "plant", "sound": "your-id:sound/pump" }]`, each played even if the state has moved on |
| projectile | `children` | smaller projectiles it throws out as it flies, bounces or explodes: one set or a list of up to 4, each its own `projectile`, `count`, `speed`; `fuse_ticks: [0, 48]` sets each child off after a random time in that range, as cluster bomblets; `max_count` throws a random number from `count` to it; `steps` (`low`, `high`, `offset`, `step`, each `[x, up, back]`) sets each axis of a child's velocity to a whole number from `low` to `high`, plus `offset`, times `step`, as a script that threw embers with `getRandom`; `on_hit` throws them at each thing it hits, bouncing or bursting, never as it dies in the air; `angles` flings each along `(cos a, cos b, sin a)` at `speed`, `a` and `b` whole degrees at random (Tier+Tactical's `PrjLoop_emitPrj`); `redraw` draws the count's limit again before each child past `count`, as a `for(%i = 0; %i < getRandom(3, 5); %i++)` loop; `max_times` stops `every_ticks` after so many |
| projectile | `aura` | hurts what is within `radius` every `every_ticks` as it flies: `damage`, `players_only` to pass vehicles by, `effect` played on each one hurt at its scale, `target_sound` heard by that player alone, `max_pulses` to stop after so many (it flies on), `max_targets` to hurt only the first so many each pulse, `ally_damage` (0 to 100) dealt instead to a teammate or ally of its thrower in a mini-game with weapon damage on, friendly fire or not |
| image | `shot.lob` | `{ "speed": 18.25, "range": 300, "otherwise": 100, "distance_divisor": 3.75, "jitter_steps": [3, 3], "jitter_divisor": [6, 6] }`: lobbed to come down about where the holder looks, as Tier+Tactical's mortar: `speed` along the aim, plus upward the distance from their feet to where the look lands (`otherwise` past `range` times their scale) over `distance_divisor`, plus a little along the world's x and -z |
| image | `cook` | a grenade whose fuse burns from a state in the hand (below) |
| image | `guard` | a shield raised in some states (below) |
| projectile | `fixed_damage` | its direct damage stays as authored at any scale |
| item | `ui_name` | the name players see |
| image | `zoom` | `{ "fov": 20, "on_jet": true, "crosshair": false, "first_person": true }`: aim with the zoom key (and the right mouse button with `on_jet`), hide the crosshair, force first person while aiming. More under "Scopes" below |
| image | `eye_offset`, `eye_rotation` | where the weapon sits in first person: exactly there, relative to the camera, as Torque places it, so a scope whose sight is on the eye line stays centred at any zoom |
| image | `hide_nodes` | the holder's body nodes hidden while the image is held, shown again when it goes: `["lhand", "rhand", "lhook", "rhook"]` for a model that draws its own hands (v20's `hideNode` in `onMount`). Up to 16 |
| image | `both_arms` | `true` holds it up with both arms (`armReadyBoth`), not the mount hand's arm alone |
| image | `scripts` | what each state `script` does, by lower-case name: `{ "onfire": { "arm": "spearThrow", "fire": true, "use_up": true } }` swings the arm, launches the image's projectile (or the entry's own `projectile`) and uses the item up, as a thrown grenade ([torque-equivalents.md](torque-equivalents.md#image-state-scripts-as-data)) |
| image | `follow_arm` | `true` also moves a first-person `eye_offset` image with the arm's actions (shift, plant, swing), as the base game's brick, hammer and spray cans do; off by default |
| image | `rope` | `{ "projectile": "your-id:projectile/chain", "speed": 160 }`: while the holder hangs on a rope (`tether`), every player's game draws it with that projectile's trail, swept from the muzzle to the rope's end as densely as the projectile flying it at `speed` would lay it, as v20 rope tools did by firing a stream of them; nothing is sent for it |
| pack | `sounds` | `{ "your-id:shot": { "file": "sounds/shot.wav", "volume": 0.8 } }`: your own `.wav`/`.ogg` files, named by a state's `sound` and by rules; `local` for sounds only the holder hears, `looping` for a state-long hum |

**Scopes.** `zoom` takes more for a proper scope:

```json
"zoom": { "fov": 22, "on_jet": true, "jets": false, "crosshair": false,
          "first_person": true, "levels": [10],
          "overlay": "scope/scope",
          "sway": { "degrees": 0.35, "seconds": 4.5, "crouched": 0.3, "moving": 2.5 } }
```

| Field | Meaning | Limits |
|---|---|---|
| `jets` | `false`: with `on_jet`, the right mouse button only aims and the player does not jet while holding it | default `true` |
| `levels` | further fields of view the mouse wheel steps through while aiming (back on rolling the other way); each narrower than the last. The step resets when the aim ends | up to 8, 5 to 85 degrees |
| `sensitivity` | look speed while aimed, on top of the usual slowing with the field of view | 0.1 to 4, default 1 |
| `overlay` | your own PNG (named without `.png`, relative to `assets/`) drawn over the whole view while aimed in first person, fitted to the screen's height with black either side, under the HUD. Its transparent middle is the lens; the held model is hidden meanwhile | up to 2048 px a side and 4 MB |
| `sway` | the aim drifts on a figure of eight, `degrees` to each side, once every `seconds`; `crouched` and `moving` scale it. Only on foot, only on the holder's screen, and the mouse turns freely under it; it eases in and out | 0 to 5 degrees, 0.5 to 30 seconds, crouched 0 to 1, moving 1 to 4 |

Sway changes only where the holder looks, so other players see nothing
new and it costs no bandwidth.

**Your own model.** A weapons pack may bring its models and textures in
`assets/presentation.json` (beside `weapons.json`, the same format the
base game's item presentation uses: `models` and `textures` keyed by your
own ids, each with its file and sha256) and `assets/item-physics.json`
(each item's box). An item's or image's `model` in `weapons.json` then
names one of those keys, such as `your-add-on:model/rifle`, and a
particle's `texture` may name a texture key. Leave `items`, `images` and
`projectiles` empty in it: the game presents them from `weapons.json`.
Models are the native `shape.json` format (nodes, meshes, materials,
animations); name a `mountPoint` node where the hand holds it and a
`muzzlePoint` where shots leave, and an image state's `sequence` plays one
of its animations (a rifle's `Bolt`).

**Magazines.** An image's `magazine` gives it rounds, a reload and a
reserve, with nothing in a rule:

```json
"magazine": { "size": 30, "ammo": "rifle", "reload_ticks": 240, "reserve": 90,
              "max_reserve": 180, "reload_sequence": "shiftDown",
              "reload_sound": "mag:reload", "empty_sound": "mag:click",
              "display": "Rifle Rounds" }
```

Each shot takes `per_shot` rounds (1 when left out); a shot without them
clicks with `empty_sound` and reloads. The last round, the light key and
`reload(p)` start a reload that lasts `reload_ticks` (120 a second, up to
1200) and fills the magazine from the holder's reserve of its `ammo`.
Every gun loading the same `ammo` shares that reserve; each gun keeps its
own rounds, by the tool slot it sits in (two of one gun each keep theirs),
also when thrown and picked up by someone else. `one_by_one`
loads a round per `reload_ticks` (`per_load` rounds with a two-barrel
gun), as a shotgun's shells, and a pull of the
trigger stops the loading and fires. The first gun of an ammo type a
player draws brings `reserve` rounds, never above `max_reserve` (100000 at
most); a new life brings full magazines and starting reserves again. The
holder sees `display  rounds / reserve` at the bottom of their screen,
sent to them alone and only when it changes; with `display_ticks` (up to
7200) it stays up that long each time, and the states whose scripts
`display_scripts` names show it again (a dry pull), as does the light key
when there is nothing to load. A size is 1 to 1000 rounds;
an ammo name is 1 to 32 letters, digits, `.`, `_` or `-`.

A magazine with `"from_reserve": true` (size 1) has no rounds of its own:
each throw takes `per_shot` straight from the reserve and nothing
reloads, as Tier+Tactical's counted grenades. With none left the image
leaves the hand while its tool stays selected; more ammo of its kind
(a grenade bag) puts it back, or with `"clear_when_out": true` its tool
goes too. The display shows the reserve alone, and the light key works the
light.

`"remount": true` draws the gun afresh (out of the hand and back, its
draw states again) when the holder picks another copy of it from another
slot; without it the gun stays up and takes that copy's rounds.

A magazine's `supply` says where its rounds come from, as Tier+Tactical's
ammo systems did: `reserve` (the default: shots take the magazine's rounds,
a reload fills it from the reserve), `endless` (a reload fills it from
nothing, the reserve untouched; the display shows `rounds / size`),
`unlimited` (nothing is used, no display), `counted` (shots take the
reserve, no reloads; the display shows the reserve) or `both` (shots take
the magazine and the reserve, a reload fills the magazine free while there
is reserve). `"hide_display": true` shows no display at all.

**Fields from server settings.** A pack's `bindings` let a server setting
decide any field of its items, images and projectiles, as v20 scripts read
a `$Pref::Server::*` global where the field was used:

```json
"bindings": [
  { "setting": "$Pref::Server::TT::Recoil",
    "field": ["images", "mag:image/rifle", "shot", "kick"],
    "values": { "false": null } },
  { "setting": "tier-rules:tt_displaytime",
    "field": ["images", "mag:image/rifle", "magazine", "display_ticks"],
    "scale": 120 },
  { "setting": "$Pref::Server::TT::Ammo",
    "field": ["images", "mag:image/rifle", "magazine", "supply"],
    "values": { "2": "endless", "3": "both" },
    "when": { "$Pref::Server::TT::AlwaysReloadEx": "true" } }
]
```

`setting` is `<package>:<key>` or the v20 global of a server setting
(section 3's `scope: "server"`), whichever running Add-On declares it.
`field` is the kind, one of the pack's own ids and the path in it (not its
`id`, `states`, `image` or `item`). `values` maps the setting's value, as
text, to the field's (`null` leaves an optional field out; a value not
listed leaves the field as authored), or `scale` multiplies a number
setting. `when` applies a binding only while other settings have those
values; a later binding of the same field wins. The host plays the pack
with the settings applied and derives it again when the host changes one;
players get the values with the world and derive the same pack. The
result is checked as any pack, so a value that takes a field out of its
range is refused when the host sets it. A gun keeps its rounds and
reserve across a change; new shots, reloads and spawns follow the new
fields. A restart setting changes them at the next start. At most 8192
bindings, 10 steps deep.

A magazine can instead follow the image's own states, as Tier+Tactical's
guns did with their check scripts: `checks` names the flags each state
script sets on entering its state (`"TT_onFireCheck": { "loaded": ["shot"],
"ammo": ["reserve"] }`, each flag `true`, `false` or true when any listed
fact holds: `shot`, `empty`, `full`, `not_full`, `reserve`, `no_reserve`),
`on_reload` and `on_loaded` the flags as a reload starts and as its rounds
arrive, and `reload_state` the state script the rounds arrive with; its
`light_states` (below) are then also the only states a reload starts in.
A check with `"spend": true` also takes the shot's rounds as it loads one
(a burst's later rounds, fired by states that do not run `onFire`), and
one with `"keeps_reload": true` leaves a reload under way unloaded, so a
forced reload is not cut short by the next check. A reload these states
run stops when a tool is drawn or put away.

A pack may fire projectiles of a package it depends on: list their ids in
`external_projectiles` (`"external_projectiles": ["tier1:projectile/tracer"]`)
and name them as usual. They are found when the game loads the packs
together; an image whose projectile no loaded package declares is left
out, with a note.

An image's `shot` can say more of how it fires. `hitscan` (`range`, and
`moving_range`) lands each projectile at once along a ray, with its
damage and push; `explosion` names another projectile exploded there in
place of its own, `player_sound` and `other_sound` play there by what it
hit, `flown` names a projectile flown from the muzzle to that point, and
`tracer` (`color`, `width`, `seconds`) draws a streak to it. A ray from
the muzzle (`from_eye` false) starts at the eye instead when something
stands within `eye_within` units in front of it, so a muzzle poking
through a wall does not shoot past it; with `converge` it heads for the
point the eye looks at rather than along the muzzle. `sounds` (up to 8
pairs of `player` and `other`) has each shot draw one pair, as melee
scripts picked one of two hit sounds per swing; a side a pair leaves out
keeps `player_sound` or `other_sound`. `damage` (-100 to 100) is what
each ray deals in place of its projectile's. `ricochet` (`times` 1 to 8,
`damage`, `shooter`) turns the ray off whatever it meets, mirrored about
the face it hit, up to `times` more landings over the range it has left:
each landing deals `damage` (-100 to 100) more for every one before it,
and once turned it can come back into its shooter, who takes `shooter`
(0 to 1, default 1) of its damage. Every turn is drawn as a streak in the
tracer's look, and `on_damage`'s `info.bounces` says how many turns came
before the hit.
`moving_spread` and `moving_projectile` replace the spread and the
projectile while the shooter moves faster than `moving_speed`; `rested`
(`after_ticks`, `spread`, and optionally `still` and `projectile`) is
the truer first shot after a pause. An image's `state_shots` fire on
entering a state whose script is not `onFire` (`"onfire2": { ... }`),
each a shot of its own, hitscan or not (a knife's weaker stab beside its
slash); a shot with `"free": true` takes no rounds. `recoil` pushes the shooter back along
the aim as they fire (units a second); `recoil_vertical` sets the push
along the aim's vertical part apart, so a machine gun can push only up or
down. `kick` shakes the holder's view
with each shot (`amplitude` 0 to 1, `frequency`, `seconds`); with a
`radius` (up to 100) other players within it feel it too, weaker with
distance, as a v20 recoil blast's camera shake. The image's
`volleys` fire more projectiles after its own (a shotgun's close blast),
and `left_image` holds a second image in the left hand that shares the
holder's ammo; a state script `onFireAkimbo` pulls its trigger. A
projectile's `slow` (`{ "divisor": 2 }`) slows the player it hits for a
moment, more with each hit down to a floor. An item with `"hidden": true`
is put in the world only by rules (`drop_item`): no spawn list, loadout or
`/give` offers it.

The light key reloads a gun with a magazine unless its image gives the key
its own command (below). With `"light_states": ["Ready", "Empty"]` it
reloads only from those states, and works the light as usual whenever it
cannot reload (a full magazine, no reserve), as the hl2 ammo system did.

**Cooked grenades.** An image's `cook` lights a fuse in the hand as a
state script runs, and the shot it fires next carries what is left:

```json
"cook": { "script": "onpindrop", "fuse_ticks": 480, "burst_height": 2.0,
          "print": "{seconds} second{s} left", "print_ticks": 12, "print_seconds": 0.15 }
```

The holder reads `print` in the middle of their screen every
`print_ticks` while it burns (`{seconds}` the time left to a tenth, `{s}`
an `s` unless it is exactly 1; `first_print` replaces the first). Held for
the whole `fuse_ticks`, the image's projectile goes off `burst_height`
above their feet and they put it away, keeping the grenade. Putting it
away first puts the fuse out. The
[Commando rifle](../../packages/samples/sample-commando-rifle/assets/weapons.json)
is a plain scoped rifle: raise it, fire, let go, fire again.

**Shields.** An image's `guard` protects its holder while their right
hand's image is in one of its `states`:

```json
"guard": { "states": ["Ready"], "front": { "up": 0.7, "above": 3, "down": 0.8, "below": 4 },
           "projectile_damage": 0.1, "damage": 0.25, "push": 0.5,
           "reflect": true, "reflect_kill": "Reflected", "hit_explosion": "ns:explosion/clang",
           "sounds": ["ns:sound/bing"], "durability": 20, "break_explosion": "ns:projectile/pieces" }
```

It covers what strikes the side the holder faces. With `front`, looking up
past `up` it covers hits landing higher than `above` (times their scale)
below the middle of their body, and looking down past `down` those lower
than `below`. A projectile it stops does `projectile_damage` of its damage
and `push` of its push, plays `hit_explosion` and one of up to 8 `sounds`
at the holder, and with `reflect` flies back the way they look as theirs
(a kill by it reads as the damage type `reflect_kill` when the pack has
one). A ray it stops does `projectile_damage` and is not sent back; any
other harm it covers does `damage`. After `durability` stops the shield
breaks: `break_explosion` goes off and the item leaves their tools; with
`"bots_keep": true` a bot's never wears out. With `fall_damage` (0 to 1),
a fall or crash the holder meets looking the way they were going (down,
for a fall) does that share, with `hit_explosion` at twice their scale.

Shots hit players, vehicles, bricks and Add-On creatures. A creature's
own rule decides what the hit does (`on_entity_damage`).

An image with a projectile and an `onfire` command (in `commands.states`)
does both: the round flies and the command runs, as a v20 gun's `onFire`
that called `Parent::onFire` did. That is how a rule counts a magazine.

A tool rather than a gun: give its image `"command": "your-rule:command"`
and no projectile. Its `onFire` state then runs that command of your rule
Add-On for the holder, with `aim()` resolved where they look (declare
`aim_reach` on the command). The Duplicator's `duplicator-tool` does this.
A state's `"arm"` swings the holder's arm as the image enters it
(`"armattack"` to strike, `"root"` to rest); v20 chose the swing from the
image's name in script, so give your own tools this instead, for example
a swing on `PreFire` and a rest on `StopFire`.

More moments can run commands through the image's `commands`:

```json
"commands": {
  "states": { "ongrab": "gravity-gun:grab", "onrelease": "gravity-gun:release" },
  "wheel": "gravity-gun:reel"
}
```

`"light": "your-rule:reload"` there runs when the holder presses the light
key with the image in hand, instead of turning on their light, as v20
Add-Ons did by packaging `serverCmdLight`. `"cancel": "your-rule:mode"`
runs when the holder presses the cancel key (v20 Add-Ons packaged
`serverCmdCancelBrick` for a rifle's grenade launcher or the next kind of
round); the key still clears their ghost brick. Declare such commands
`tool_only` (section 3).

`states` maps a state's `script` (lowercase) to a command, run as the
image enters that state: a state with `"down"` to a charging state whose
`"up"` leads to the firing state gives a press-and-hold charge.
`jet` runs when the holder presses jet (the right mouse button) with the
tool in hand, as v20's `onTrigger` slot 4 did; players who can jet still
jet. `wheel` runs while the trigger is held with the mouse wheel's notches
as its one `int` argument (positive rolled forward, away from you), and
the wheel then does not change tools; declare `"args": ["int"]` on that
command. `mount` runs as the image goes into the holder's hand and
`unmount` as it leaves (v20's `onMount` and `onUnMount`), so a scope that
slows its holder can push a slower player type and pop it again. The
Gravity Gun's `gravity-gun-tool` uses `states` and `wheel`: hold left
click to grab, roll to reel, let go to drop or fling.

`light` and `cancel` take those keys. `shift`, `rotate` and `plant` take
the brick keys whenever the player has no ghost brick out and holds no
copy to place with the tool (a copy moves and plants with them as
always), with v20's `serverCmdShiftBrick` arguments: declare `"args":
["int", "int", "int", "bool"]` (studs away from and to the left of the
player's facing, plates up, and whether it was the super shift),
`["int"]` for `rotate` (1 clockwise seen from above, or -1), and none for
`plant`. `seat` takes the next and previous seat keys on foot, with 1 or
-1 (`["int"]`). A duplicator's selection box uses them. `mount` and
`unmount` run as the image comes into and leaves the holder's hand
(v20's `onMount` and `onUnMount`), however it happens; declare the
`unmount` command `while_dead`.

An item with no `image` is picked up but held by nobody: an ammo box or a
health pack whose `on_pickup` answers `"take"`. An item's `label` (up to
32 characters) shows above it where it lies, as v20's `setShapeName` on an
item: an ammo box's round count. One with `"rotate": true` turns slowly
where it lies, once every three seconds, as a v20 item whose `onAdd` set
`%obj.rotate`. Every item needs a
`ui_name`, the name players pick it by.

`effects` holds the pack's own particles, emitters and lights in the base
game's effects library format (ids in your namespace, such as
`your-id:emitter/flash`; a particle's `texture` is the base game's, such
as `base/data/particles/cloud`, or a key of your item presentation's
`textures`), and
`explosions`, each explosion's effect: `{ "id": "your-id:explosion/boom",
"lifetime": 0.3, "emitters": [...], "light": ..., "burst": [emitter,
count, radius] }`. An image state's `emitter` and a projectile's `trail`
name an emitter by id; an explosion's effect is found by its explosion's
name (`boom`), as the base game's are. Import Add-On writes these from a
v20 Add-On's datablocks.

**Worked example: a rifle with a burst mode.** Two Add-Ons, as the
Commando sample splits them: `mag` provides the weapons pack (the rifle
`mag:weapon/rifle`, whose image has a `magazine` of `"ammo": "rifle"` and
fires `mag:projectile/round`, an ammo box `mag:weapon/ammo` with no
`image`, and a sound `mag:ping`), and `mag-rules` provides the rule, lists
`mag` in its `dependencies` (so the item hooks hear `mag`'s items and
rounds) and asks for the `player`, `damage`, `chat` and `effects`
capabilities. The magazine counts, reloads and shows its rounds itself;
the rule adds what it does not. Its `behaviour.json`:

```json
{
  "schema_version": 1,
  "script": "mag.rhai",
  "on_pickup": true,
  "on_projectile_hit": true,
  "commands": [
    { "name": "fired", "tool_only": true },
    { "name": "mode", "tool_only": true }
  ],
  "state": { "player": { "burst": { "default": false } } }
}
```

The rifle's image sends its moments to the rule: `"commands": { "states":
{ "onfire": "mag-rules:fired" }, "cancel": "mag-rules:mode" }`.

```rhai
fn cmd_fired(p) {                      // the image fired one round
    let me = player(p);
    let m = me.magazine;
    if get_player(p, "burst") && m != () && m.rounds > 0 {
        // A second round from the muzzle, out of the same magazine.
        fire("mag:projectile/round", me.mx, me.my, me.mz,
             me.lx * 200.0, me.ly * 200.0, me.lz * 200.0, p);
        set_rounds(p, m.item, m.rounds - 1);
    }
}
fn cmd_mode(p) {                       // the cancel key
    set_player(p, "burst", !get_player(p, "burst"));
    center_print(p, if get_player(p, "burst") { "Burst" } else { "Single" }, 1.0);
}
fn on_pickup(p, item, info) {
    if item == "mag:weapon/ammo" {     // used up, never held
        give_ammo(p, "rifle", 30);
        return "take";
    }
    ()
}
fn on_projectile_hit(hit) {
    if hit.kind == "brick" { sound_at("mag:ping", hit.x, hit.y, hit.z); }
}
```

`take_item(p, "mag:weapon/rifle")` takes the rifle back (a thrown weapon),
`drop_item("mag:weapon/ammo", x, y, z)` leaves a box in the world, and
`player(p).tools` lists what someone carries by slot, for a rule that
refuses a second rifle. Typing `/mode` in chat is refused because the
commands are `tool_only`; the image still runs them.

Keys of `damage_types` and `explosions` are their `name` in lowercase, and
a projectile names its damage type as `$DamageType::<name>`. A damage type
with `"special": true` is a special kill (Support_SpecialKills): its
message's `%3` is replaced by the killing weapon's icon, so `"%2 [sent
back]%3%1"` shows both. Two Add-Ons may declare the same name: each
keeps its own. The pack loaded later has its declaration kept as
`<its id>:<Name>`, and its own projectiles, images and rules that name the
bare `<Name>` (`damage`, an `on_damage` answer) get that one. The earlier
one is replaced for everyone only when the later Add-On depends on its
package, as v20's re-declaration did; the same declaration twice is one.
Everyone in a
game needs the same weapons, so give the Add-On to the people you play
with.

## 6. Other content kinds

Each file in `provides` has a `kind`. Which kinds an Add-On provides decides
who needs it: only host kinds means the host only; only `model`, `hud`
and `binds` means each player; anything else (weapons, bricks, blocks) means everyone.
An Add-On cannot mix host kinds with `model` or `hud`: split it in two, the
visuals depending on the rules. `bri-addon-check` tells you which it is.

| Kind | Needed by | What it is | Example |
|---|---|---|---|
| `behaviour`, `script` | host | a game rule (section 3) | `packages/samples/sample-survival-points` |
| `world` | host | a generated chunk world: materials, a `generate(cx, cz)` function | `packages/stresslab/stresslab-world` |
| `entity` | host | a scripted creature: model, `think` function, speed, health | `packages/stresslab/stresslab-creeper` |
| `archetype` | host | a playable body: movement, collision `box` or `ball`, steering, health, riding, model, camera distance | `crates/sim/tests/unlike_modes.rs` |
| `mode` | host | a Start Game game mode: name, the Add-Ons it runs, a map, and its own mini-game | `packages/stresslab/stresslab-mode`, `crates/sim/tests/mode_and_voxels.rs` |
| `model` | each player | a box model for an entity | `packages/stresslab/stresslab-creeper-model` |
| `hud` | each player | a HUD panel (section 4) | `packages/samples/sample-points-hud` |
| `binds` | each player | keys for an Add-On's commands, under its own heading in Controls (below) | `crates/addon-import/ports/tool_newduplicator/files/binds.json` |
| `weapons` | everyone | weapons (section 5) | `packages/samples/sample-bubble-blaster` |
| `bricks`, `vehicles` | everyone | written by Import Add-On (section 7), or a `vehicles.json` you write (fields below) | `packages/showcase/steel-ball-kit` |
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
cargo run -p bri-addon-import --bin bri-import-addon -- Weapon_Example.zip out/weapon_example --installed content
```

`--installed` names the game's content folder. An Add-On builds on the base
game: a dirt brick declared `brick1x1DirtData : brick1x1Data` inherits the
stock 1x1's icon and fields, an image names `weaponSwitchSound`. The
importer reads those base datablocks' names and fields from the installed
game's brick catalog, weapons, sounds and effects (Import in the Add-Ons
screen always passes it); without it they are unknown and the report says so.

Many v20 Add-Ons keep part of what they do in scripts: a shotgun's spread,
a slash command. The report lists each such function under **Needs
behaviour**. When someone has made a native **port** of that Add-On, the
importer applies it and the report says **Ported**. The ports so far, and the
recipe for making one (or having your agent make one), are in
[porting.md](porting.md).

For weapons the importer brings across items, images, projectiles,
explosions, damage types and the Add-On's own look and sound: the particle
emitters its image states, projectile trails and explosions use (with an
explosion's burst and light), its `AudioProfile`s (a sound whose
description is not 3D is heard by its holder alone), and its `DebrisData`
with its model, so its casings and explosion debris are its own. v20 datablocks the engine
would have corrected on load (an emitter's period or angles) are corrected
the same way and noted. An `ItemData` with no `uiName` is hidden in v20,
so it is left out and its image kept for rules to mount; a
`ParticleEmitterData` with a `uiName` is offered in the wrench's emitter
list, as v20 listed it, and so is a named light; one with a
`uiName` and no image becomes a pickup nobody holds. A kill icon the
Add-On forgot to ship is left out of its messages. Its particles may draw
its own textures; players load at most 64 of them from all Add-Ons, and
fit each within 256 pixels a side.

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
                  datablock fxDTSBrickData(brickMirror1x14x10Data : brickMirror1x4x5Data)
                  {
                      uiName = "1x14x10 Mirror";
                      stretchSize = "14 1 30";
                  };
```

`stretchSize = "width depth height"` (studs, studs, plates) makes another
size of the shape, as a nine-slice picture stretches: the frame, sill and
stud edges keep their size and the middle grows. Its mirror, collision,
portal openings and pairing all follow the new size, so a 1x14x10 Mirror
is the same window brick, big.

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
                  datablock fxDTSBrickData(brickPortal1x14x10Data : brickPortal1x4x5Data)
                  {
                      uiName = "1x14x10 Portal";
                      stretchSize = "14 1 30";
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

Bots know portals too, with nothing for a bot kind to set: a bot sees and
shoots through an opening at whoever stands beyond its partner, its paths
lead through openings where walking through is the way, and it follows an
enemy it watched go in.

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

The Add-On also has a 1x14x10 (6.9 by 5.75 inside: a tank or a jeep
drives through with room to spare) and a 1x20x12 (the Stunt Plane, wings
and all). Each size of portal is its own kind, so a 1x4x5 never pairs
with a 1x14x10 of the same name. A big one costs no more to draw than a small one
the same size on screen: each view is drawn only over the part of the
screen its opening covers.

**Another size of a brick (`stretchSize`).** Any brick can be its
`brickFile`'s shape at another size, `"width depth height"` in studs,
studs and plates. Half a stud of every edge keeps its size and moves out
with the edge while the middle stretches, as a nine-slice picture does:
a window's frame stays as thin round a bigger pane, studs on top stay one
stud each (there are more of them), and the brick's attachment grid and
collision boxes grow to match. A shape with no collision boxes of its
own (and no `linkPass` frame) needs the Add-On to give it collision.

Views share the mirrors' **Options > Graphics > Mirrors** budget, and a
portal seen through a portal repeats what it last showed, like facing
mirrors.

## 8. What players are asked to trust

Players download a server's Add-Ons when they join. What they are asked
depends on the most powerful thing an Add-On does:

| Tier | What the Add-On has | What the player sees |
|---|---|---|
| Data | rules, HUD panels, weapons, bricks, models, sounds | nothing: it downloads and runs |
| Sandboxed code | a `client` section, or skins in `looks.json`: WebAssembly and WGSL run in the sandbox | "Trust and join" or "Leave", once per server, and again when the code changes; nothing when the player installed the same code themselves |
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
