# Game rules

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
runs (its `commands`, [Weapons](weapons.md)): typed in chat or sent from a HUD it is
refused, so nobody types a gun's `/fire` or `/reload`. Players send a
command by typing it in chat,
`/sell coal`, or with a HUD panel's keys ([HUD panels](hud.md)). Typed words become the
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
| `minigames()`, `minigame(id)`, `setting(game, key)`, `team_setting(game, team, key)`, `server_setting(key)`, `pref(global)`, `bricks(kind)`, `brick(id)`, `palette()`, `drops()` | | `set_teams(game, teams, options)`, `set_team(p, team)`, `set_score(p, n)`, `add_score(p, n)`, `reset_minigame(game)`, `set_setting(game, key, v)`, `set_team_setting(game, team, key, v)`, `hold_respawn(p, held)`, `end_round(game, winners)`, `report_column(game, key, title, cells)`: `minigame`; `show_report(p, report)`, `hide_report(p)`: `chat`; `watch(p, target)`, `follow_path(p, knots)`, `free_camera(p)`, `orbit_point(p, at, distance)`: `player`; `set_brick_item(brick, item)`, `set_brick_color(brick, c)`, `set_brick_shown(brick, rendering, colliding, raycasting)`: `world.edit`; `fire_brick_input(brick, input, p)`, `fire_game_input(game, input, p, killer)`, `set_brick_field(brick, key, v)`: `brick_events`; `brick_field(brick, key)` reads |
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
`true` gives it back. A gun's magazine is image data ([Weapons](weapons.md)); rules
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
owner, game, name, item, ui_name }`, `ui_name` its kind's name in the build
menu (v20's `uiName`), `game` being the mini-game whose bricks it is (its owner's, as
v20's `minigameCanUse`), or `()`. `set_brick_item(brick, item)` sets the
item a brick holds out (`setItem`), or `()` for none: the world's bricks,
a mini-game's, or ones the calling player may build on. An item whose image
has `paint_tint` shows in its brick's colour. `set_brick_color(brick, c)`
repaints one of those bricks in palette colour `c` (`setColor`),
`set_brick_shown(brick, rendering, colliding, raycasting)` sets whether one
draws, collides and stops rays (`setRendering`, `setColliding`,
`setRayCasting`), and `palette()` lists the palette as `[r, g, b, a]` from 0 to 1
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
`#{ id, kind, x, y, z, turns, min, max, color, owner, game, name, item,
ui_name }` (the same map `bricks(kind)` lists), `kind` being its
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
