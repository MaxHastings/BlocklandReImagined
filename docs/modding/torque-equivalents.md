# TorqueScript equivalents

Add-On rules are Rhai scripts that run on the host (see
[README.md](README.md), section 3). They are not TorqueScript and there is
no TorqueScript VM, by design. This page lists the v20 calls modders
reach for and the call that does the same job here, so a port starts from
something familiar. "Equivalent" means the same job, not the same
signature: arguments are plain values (player ids, `[x, y, z]` arrays,
objects like `"vehicle:3"`), and every change to the world is an
operation that needs a capability.

## Reading the world

| TorqueScript | Here | Notes |
|---|---|---|
| `ClientGroup` loop, `%client.player` | `players()`, `player(id)` | A player map, below. |
| `AIPlayer`s in `MissionCleanup`, `%bot.spawnBrick.getGroup().bl_id` | `bots()`, `player(id)`, `p.bot`, `p.bot_owner` | A bot reads like a player. |
| `%obj.getObjectMount()`, `getMountNodeObject` | `p.riding`, `p.seat`, `p.mounted` | |
| `%obj.getScale()` | `p.scale` | |
| `%obj.getPosition()`, `getTransform()` | `p.x`, `p.y`, `p.z` | The feet. |
| `%obj.getWorldBoxCenter()` | `p.cx`, `p.cy`, `p.cz` | The middle of the body, crouch and scale included. |
| `%obj.getEyePoint()`, `getEyeVector()` | `p.ex`..`p.ez`, `p.lx`..`p.lz` | The look is a unit vector. |
| `%obj.getVelocity()` | `p.vx`, `p.vy`, `p.vz` | |
| `%obj.getDamageLevel()`, `getDataBlock().maxDamage` | `p.health`, `p.max_health` | Health counts down, not damage up. |
| `%obj.isCrouched()` | `p.crouched` | |
| `isObject(%obj.getObjectMount())` | `p.mounted` | On a vehicle or riding a player. |
| `%obj.getScale()` | `p.scale` | |
| `%client.currTool`, `getMountedImage(0)`, `getImageState(0)` | `p.slot`, `p.image`, `p.image_state` | `image_state` is the state's name, like `"Ready"`. |
| `%client.minigame` | `p.minigame` | |
| `registerInputEvent(fxDTSBrick, "onX", targets)`, `%brick.processInputEvent("onX", %client)` | `brick_inputs` in behaviour.json, `fire_brick_input(brick, "onX", p)` | `$InputTarget_[...]` is filled from `p`: Player, Client and MiniGame; OwnerPlayer and OwnerClient are the brick owner's. |
| `registerEventTarget("Name Class", "BaseClass", "%client.findCode")` | `brick_targets` in behaviour.json: `{ "name", "class", "from" }`, with `brick_outputs` of that class | `from` names the slot the find code starts from (`Client` for `%client`, `Self` for `%this`); the rules find what the target stands for in `on_brick_output` from `target` and `info.target`. |
| `registerMultiSourceInputEvent(fxDTSBrick, "onX", targets)`, `processMultiSourceInputEvent("onX", %client, %mini)` | `brick_inputs` in behaviour.json, `fire_game_input(game, "onX", p, killer)` | Runs on every brick of the mini-game with rows on it. `$InputTarget_["Player(Killer)"]` and `["Client(Killer)"]` are filled from `killer`. |
| A package of `FxDtsBrick::onPlayerTouch` or `onActivate` that calls `processInputEvent("onX(...)")` | a `brick_inputs` entry with `"follows": "onPlayerTouch"`, and `on_brick_input(input, brick, p)` answering its name | Asked only for bricks with rows on an input that follows it. |
| A package of `serverCmdAddEvent` that refuses rows | `"on_event_row": true` and `on_event_row(p, brick, row)` returning `false` or a reason | The refused row is left out and the player told why. |
| `registerOutputEvent(Class, "doX", params)`, `function Class::doX(%this, ..., %client)` | `brick_outputs` in behaviour.json, `on_brick_output(output, target, params, info)` | `info.client` is the appended `%client`. An output that fires its brick's own input (`%this.onX(%client)`) returns it instead, with a row range if it limits the rows. |
| `%obj.getMuzzlePoint(0)` | `p.mx`, `p.my`, `p.mz` | The held image's muzzle, or the eye with empty hands. |
| `%obj.tool[%i]` | `p.tools` | Item ids by slot, `""` for an empty one. |
| `%client.currentColor` | `p.paint` | The palette colour last picked with the paint keys. |
| `containerRayCast(%start, %end, %mask, %exempt)` | `raycast(from, dir, range, ignore)` | Answers at once. A map with `kind`, `id`, `ref`, `x`, `y`, `z`, `nx`, `ny`, `nz`, `distance` (and `region` for a player), or `()`. No type mask: check `kind`. |
| `%obj.getDamageLocation(%pos)` | `hit_region(p, x, y, z)`, `info.region` | `"head"`, `"torso"` or `"legs"`, by Torque's default bands. `on_damage`, `on_projectile_hit` and `raycast` carry it already. |
| `initContainerRadiusSearch` | `objects_near(x, y, z, r)` | Players, vehicles and entities. |
| `minigameCanDamage(%a, %b)` | `can_damage(by, target)` | Players, vehicles and entities. |
| `getRandom()` | `hash3(tick(), a, b, c)`, `noise(...)` | Deterministic, so a replay agrees. |
| `getSimTime()` | `tick()` | 120 ticks a second. |

## Changing the world

| TorqueScript | Here | Capability |
|---|---|---|
| `%obj.damage(%src, %pos, %amt, %type)` | `damage(target, amount, by, type)` | `damage` |
| `%obj.addHealth(%amt)` | `heal(p, amount)` | `damage` |
| `new Projectile() { ... }` | `fire(projectile, x, y, z, vx, vy, vz, by)` | `damage` |
| `new Explosion()`, `radiusDamage` | `explode(x, y, z, radius, damage, brick_radius)` | `damage` |
| `%obj.mountImage(%img, 0)` | `mount_image(p, image)`, `mount_image(p, ())` | `player` |
| `%obj.unMountImage(0)` | `unmount_image(p)`: tools, cans and bricks in hand | `player` |
| `%client.camera.setOrbitMode(%target, ...)`, `setControlObject(%client.camera)`, back with `setControlObject(%player)` | `orbit_camera(p, target, distance)`, `orbit_camera(p, target, min, max, distance)` (the wheel zooms between), `orbit_camera(p, ())` | `player` |
| `%obj.setScale("s s s")` | `set_scale(p, s)` | `player`; 0.2 to 5, one number |
| `%obj.setLookLimits(%up, %down)` | `set_look_limits(p, up, down)`, `set_look_limits(p, ())` | `player` |
| `%client.setControlObject(%client.camera)`, `%camera.setOrbitMode(%target, ...)` | `watch(p, target)`, `watch(p, ())` | `player`; orbits a body, the body takes no actions |
| Slayer's `%client.setDead(1)` (no respawn) | `hold_respawn(p, true)` | `minigame`; a reset lets go |
| `$Pref::Server::...` prefs a game mode's GUI edits (Slayer's `Slayer_PrefSO`) | `settings` in the behaviour; `setting(game, key)`, `set_setting(game, key, v)` | `minigame` to change |
| `%obj.mountObject(%rider, %node)`, `%rider.canDismount = 0` | `mount_object(mount, rider, node, can_dismount)` | `physics`; node is a `mount<N>` of the body model |
| `%rider.setTransform(...)` after `mountObject`, to turn them on the mount | `mount_object(mount, rider, node, can_dismount, turn)`, `turn` in degrees clockwise from above | `physics`; Torque's angle is radians |
| `%rider.unMountObject()`, `dismount()` | `unmount_object(rider)` | `physics`; keeps the mount's velocity |
| `%obj.setImageAmmo(0, %x)` | `set_image_ammo(p, ammo)` | `player` |
| `%client.setControlCameraFov(%fov)` | `set_fov(p, fov)`, `set_fov(p, ())` | `player` |
| `%obj.setTransform`, `%client.spawnPlayer()` | `teleport(p, x, y, z)`, `respawn(p)` | `player` |
| `%obj.setVelocity`, `addVelocity` | `push(ref, vx, vy, vz, by)` | `physics` |
| A rope or grappling hook scripted from a schedule that re-aims `setVelocity` toward a point each tick | `tether(p, point, length, #{brick, object, reel, swing, keys, straight})`, `tether_length`, `untether` | `physics` |
| `%player.tool[%i] = ...` | `give_item(p, item, equip)` | `player` |
| `%player.tool[%i] = 0`, `serverCmdDropTool` | `take_item(p, item)` | `player` |
| `new Item() { ... }` at a point | `drop_item(item, x, y, z)`, `drop_item(item, x, y, z, vx, vy, vz)` | `player` |
| `centerPrint`, `bottomPrint` | `center_print(p, text, s)`, `bottom_print(p, text, s)` | `chat` |
| `messageClient`, `messageAll` | `tell(p, text)`, `broadcast(text)` | `chat` |
| `%mini.messageAll`, `messageAllExcept`, `centerPrintAll`, `bottomPrintAll` | `tell_minigame(game, text[, except])`, `center_print_minigame(game, text, s)`, `bottom_print_minigame(game, text, s)` | `chat`; one line of the share for the whole game |
| Slayer's `%mini.endRound(%winner)` | `end_round(game, #{ teams, players })`, then `on_minigame` `round_end` | `minigame` |
| `serverPlay3D(%profile, %pos)`, `%client.play2D` | `sound_at(profile, x, y, z)`, `play_sound(p, profile)` | `effects` |
| `%obj.playThread(%slot, %seq)` | `play_thread(p, thread, sequence)` | `effects`; whole-body sequences (`death1`) override by priority, empty-hand arm poses (`armReadyBoth`) hold |
| A stretched `StaticShape` tracer | `beam(from, to, #{ color, width, seconds, muzzle })` | `effects` |
| Mission lights baked into the map (v20 scripts could not change them) | `set_map_lights([x, y, z], radius, #{ on, color, brightness })` | `lighting` |
| The mission `Sun`'s `azimuth`, `elevation`, `color`, `ambient` and the `Sky`'s `fogColor`, `fogDistance`, `visibleDistance` (fixed in v20; changed live here) | `set_environment(#{ sun_azimuth, direct_light, fog_color, visible_distance, day_length, ... })`, `environment()` | `environment` |
| `%brick.setColor(%c)` over a hand-written search of touching bricks | `paint_fill(p, brick, color, limit)` | `world.edit` |
| `%client.score`, dynamic fields | `get_player`/`set_player` on declared state | none |
| `%player.setNodeColor(%node, %color)` for team uniforms | `set_avatar_colors(p, #{ torso: [r, g, b] })`, `set_avatar_colors(p, ())` | `player` |
| `hideAllNodes(%player)`, `%player.unHideNode(%node)`, `setFaceName`, `setDecalName` for team uniforms | `set_avatar_parts(p, #{ hat: "copHat", face: "smiley" })`, `set_avatar_parts(p, ())`; `avatar_choices()` for `$hat[%i]`, `$accentsAllowed[%hat]` | `player` |
| `%client.forceEquip(%slot, %item)` for every slot | `set_tools(p, [item, (), ...])` | `player` |
| `%client.setRespawnTime(%ms)`, `resetRespawnTime()` | `set_respawn_time(p, ms)`, `set_respawn_time(p, ())` | `minigame` |
| `%mini.playerDatablock`, `%mini.startEquip[%i]` | `minigame(game).player_type`, `.loadout` | none |
| A pref of `type = "object"` (`object_class = "ItemData"` or `"PlayerData"`) | a setting of `type` `item` or `player_type` | none |
| Digging a terrain of bricks: `%brick.delete()`, `new fxDTSBrick()` of a dirt cube | `remove_brick(id)`, `place_voxel(x, y, z, material)`, with `voxel(brick)` and `can_place_voxel(x, y, z)` to read | `world.edit` |

## Hooks

| TorqueScript | Here |
|---|---|
| `Image::onFire`, `onReload` and other state scripts | The image's `commands.states`: entering a state with that script runs a rule command. |
| `Image::onTrigger` slot 4 (right mouse) | `commands.jet` |
| `serverCmdLight` packaged for a reload key | `commands.light` |
| `schedule(%ms, ...)` | `on_tick` with a tick counter in state |
| `CreateMiniGameSO` in a game mode's `server.cs`, with `$MiniGame::...` settings | A `mode` file's `minigame` block: the host runs that one game and everyone joins it |
| `playThread(0, armattack)` from a tool's `onFire`, chosen by image name | A state's `"arm": "armattack"` |
| `GameConnection::onClientEnterGame`, `onDeath`, `Armor::damage` | `on_join`, `on_death`, `on_damage` |
| `serverCmdSomething` | A declared command, `cmd_something` |
| A `serverCmd` only the gun calls | A command with `tool_only: true`: typing it is refused, the held image still runs it |
| `serverCmdCancelBrick` packaged for a gun | `commands.cancel` |
| `ItemData::onPickup` | `on_pickup(p, item, info)`: answer `false` to leave it, `"take"` to use it up |
| `ItemData::onDrop`, dynamic fields on the dropped `Item` | `on_drop(p, item, slot)`: the value it returns rides the drop to whoever picks it up |
| `%brick.isLocked[%color] = 1` and other dynamic fields on a brick | `set_brick_field(brick, key, v)`; any Add-On reads it with `brick_field(brick, "namespace:key")` |
| `%item.setShapeName(%text)` with `setShapeNameColor` on a dropped item | `name_drop(id, text, c)` |
| `commandToClient(%cl, 'Slayer_ctrDisplayAdd', ...)` lines of an ML score list, `Slayer_ForceGUI` | `show_report(p, report)`, `hide_report(p)` |
| A Slayer mode's `scoreListInit` / `scoreListAdd` replacing columns | `report_column(game, key, title, cells)` |
| `serverCmdDropTool` packaged for empty hands (`currTool == -1`) | `on_drop_key(p)` |
| `hasLight`, `lightType = ConstantLight`, `lightRadius`, `lightColor` on a `ShapeBaseImageData` | The image's `light` (the importer reads them) |
| `ProjectileData::onCollision` | `on_projectile_hit(hit)` |
| A flood fill over `InitContainerBoxSearch` with `setColor`, `setColorFX` or `setShapeFX`, pushed as one undo | `paint_fill(p, brick, paint, #{ limit, reach, stop_at_limit })` |
| `Player::SetTempColor(%color, %ms)` with no position; `setFaceName` with a reset `schedule` | `temp_look(p, #{ color \| paint, face, alpha }, seconds)` |
| `%vehicle.color = …; %vehicle.spawnBrick.colorVehicle()`, its `COLORGENERIC` undo, and `setTempColor` on its mounted riders | `paint_vehicle(p, vehicle, #{ color \| rgb }, #{ riders_seconds })` |
| `messageClient(%client, 'MsgPlantError_Limit')` when a fill stops at its limit | `paint_fill`'s `limit_error: true` |
| `serverCmdUseSprayCan` / `serverCmdUseFXCan` packaged to remount a tool, `%client.currentFXcan` | The image's `paint_picker`; `player(p).fx_can` |
| `%client.minigame.enablePainting` | `player(p).may_paint` |
| `Player::activateStuff` packaged (an empty-hand click) | `on_activate(p)`: answer `true` to take the click |
| `Observer::onTrigger` packaged for an Add-On's camera mode | The player's click in an `orbit_camera` reaches `on_trigger` and `on_activate` |
| `serverCmdUseInventory`, `serverCmdInstantUseBrick` packaged to refuse bricks | The `equip` policy: taking bricks in hand puts the tool slot away, which it refuses |
| `Armor::onTrigger` packaged (fire with an empty hand, pressed and let go) | `on_trigger(p, trigger, down)`: answer `true` to take a press |
| `serverCmdUseTool`, `serverCmdUnUseTool`, `serverCmdUseSprayCan`, `serverCmdUseFXCan` packaged to refuse | The `equip` policy, `allow_equip(p)` |

## Not here yet

- **Client-side prints of your own layout**: HUD panels and client code
  carry text instead.
- **Changing one motor constant for one player**: swap archetypes.

## Asking for a missing function

Name the TorqueScript call you would have used and what your Add-On needs
it for. We add a general version of it that any Add-On can use, never one
shaped for a single mod, and it gets a capability and hard limits like
the rest. [The design notes](#design-notes) show how requests have been
weighed.

## Design notes

The calls above come from one modder's gun port (September 2026). Each
request was kept only as a general building block; how each was judged:

| Asked for | Verdict | Why |
|---|---|---|
| `spawn_projectile` | Already here as `fire` | Same job. |
| `center_print`, `bottom_print`, `play_sound` at a point | Already here | Same names, or `sound_at`. |
| `raycast` with a callback run later | Changed: answers at once | Every Add-On that aims wants the answer where it asked, as `containerRayCast` gave it. A callback splits one shot's logic across two calls and needs a second budget. The ray reads the live world through the weapons' own sweep. |
| Minigame checks inside `damage` | Changed: `can_damage` is its own read | Scripts have the last word on damage everywhere else (`on_damage`, creatures, traps). A separate question lets one rule obey the minigame for players' shots and not for a trap. |
| `damage` for vehicles, with a type | Kept, widened to entities | The same call hurts anything an object reference names. |
| Player facts `crouched` | Already here | |
| Player facts `mounted`, `scale`, `cx`/`cy`/`cz`, `slot`, `image`, `image_state` | Kept | Plain reads with no cost to anything else. The body centre accounts for crouching and scale. |
| `mount_image`, `set_image_ammo` | Kept | They are v20's image seams, which total-conversion.md already named as the way to build magazines and scopes without magazine code in the engine. |
| `set_fov` | Kept | Scopes, cameras in cutscenes, sprint effects. |
| `play_thread` | Kept, threads 2 and 3 | Those are the threads the body animates. |
| Light key on images | Kept | The same kind of key hook as `jet`. |
| `beam` drawing one Add-On's tracer model | Changed: a coloured beam | A colour, width and fade covers tracers, lasers and bolts with no model to ship or convert. A model-drawn beam can come later if someone needs one. The beam starts at the shooter's muzzle as each player draws it, and costs one cue. |
| A `sound` capability and a new `effects` capability | Merged into `effects` | Sounds, beams and animations are all presentation; players read one line. |
| Converting an Add-On's own particles, explosions and sounds on import | Kept (second port) | Emitters named by image states, trails and explosions, explosion lights and bursts, and AudioProfiles become the pack's own `effects` and `sounds`; particles draw the Add-On's own textures. |
| Client fallback to an Add-On's own `sounds.json` by profile name | Not needed | The importer now rewrites each profile the gun names to its pack sound key, so nothing looks up by profile name. |
| Shape converter: `-1` starts for unused animation pools | Kept (a converter fix) | Torque writes `-1` for a pool a sequence does not use; the converter now reads it as "none" instead of refusing the model. Any Add-On model can have it. |
| Importer: items with no `uiName` fail the package | Real, fixed | A nameless item made the whole weapons pack fail its checks. The item is now left out with a note and its image kept, as v20 lists no nameless item in the inventory. |
| Import Add-On's test shot at 2 s instead of 0.5 s | Changed: it fires when the gun is ready | A fixed time is wrong for some gun either way. The check now presses as soon as the image reaches a state the trigger leaves and holds until the image takes it, as a player's click does, so neither a slow draw nor a ready state that plays out its timeout first is reported as a gun that fires nothing. |
| The gun mod's own rules, models and sounds | Not in the base game | Third-party content stays out of the repository. |

The same modder's second write-up (September 2026), judged the same way:

| Asked for | Verdict | Why |
|---|---|---|
| The next gun mounts with no ammo | Real, fixed | `mountImage` mounts loaded and `WeaponImage::onMount` sets ammo on; the runtime kept the last gun's empty flag. |
| Scoped view off-centre | Already fixed on main | |
| Add-On `AudioProfile`s never play | Real, fixed | The importer did not convert them. |
| Missing kill icon drops the damage type | Real, fixed | The icon is left out of the kill message and the damage type kept. |
| Emitter values the engine corrects on load | Real, fixed | The converter applies `ParticleEmitterData::onAdd`'s clamps (period, variance, theta). |
| `take_item`, `drop_item` | Kept | Magazines, ammo pickups and gun swaps without inventory code in the engine. `drop_item` is capped at 64 live drops per package. |
| `on_pickup`, `on_drop`, `on_projectile_hit` | Kept | Scoped to the package's own items and projectiles, or a dependency's. |
| Commands only the gun may run | Kept as `tool_only` | A general flag on any command. |
| The cancel key reaching the held gun | Kept as `commands.cancel` | The same kind of key hook as `jet` and `light`. |
| Muzzle point and tools in the player map | Kept | Plain reads. |
| First-person scopes off centre (a third write-up) | Real, fixed | Since 2026-09-29 every first-person `eyeOffset` image moved with the arm's actions, so a scope drifted off the eye line. Torque places it at eye × eyeOffset alone; that is the default again, and `follow_arm` on an image opts into the arm's motion, which the base game's tools keep. The image already hangs from the drawn camera (since v0.1.9). |
| Add-On `eyeRotation` written as axis-angle or `eulerToMatrix` | Real, fixed | Import Add-On dropped it to no turn; it now reads it as the base game's images are read, and any axis-angle axis converts, not only x, y or z. |
| Clearing a finished one-shot arm action | Left as is | Torque holds a finished non-cyclic thread at its last frame until something replaces it, and v20's arm clips end at rest; with scopes no longer riding the arm it cannot move them. |
| Custom casing and debris models | Kept | An image's `casing` `DebrisData` flies as its fields and the image's `shellExit*` fields say and draws its own model; explosion debris draws the Add-On's model too. The base game's `gunShellDebris` still throws the stock brass. At most 256 casing kinds and 512 loose models drawn at once. |
| Capping `stateEmitterTime` at 300 s | Left out | v20 does not cap it and the effects runtime already limits live particles. |
| A sound that is not 3D | Heard by its holder only | A sound with no position has no place for other players to hear it from, so it stays with the player who fired. |

## Image state scripts as data

A v20 image's states name script functions (`stateScript[2] = "onCharge"`)
and the Add-On's `Image::onCharge` did the work. Here an image's `scripts`
lists, by lower-case script name, what each one does, so a melee or thrown
weapon needs no rule. Entering a state whose script is listed does that
instead of the game's built-in handling of the name. The
[Butterfly Knife and HE Grenade ports](../../crates/addon-import/ports) are
written this way.

| TorqueScript in the function | `scripts` entry field | Notes |
|---|---|---|
| `%obj.playThread(2, spearReady)` (`spearThrow`, `armattack`, `root`) | `arm` | The holder's arm animation (thread 2), started first. Letters, digits and `_`, up to 64. |
| `Parent::onFire(%this, %obj, %slot)` | `fire: true` | Launches the image's projectile as a plain `onFire` does: from the muzzle along the aim, with the image's `shot`, after the arm. |
| A second `ProjectileData` spawned in the function (`%p = new Projectile() { dataBlock = jabProjectile; ... }`) | `projectile`, with `fire: true` | Launched instead of the image's. It must be in the pack (or a merged one); an image whose script projectile nobody provides is dropped like one missing its own. |
| `%obj.tool[%slot] = 0; serverCmdUnUseTool(%client)` after a throw | `use_up: true` | The held item leaves the holder's tools and the hand empties once the function has run. |

Up to 16 entries per image. A state with no listed script keeps the
built-in handling (`onAbortCharge` and `onStopFire` lower the arm).
