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
| `%client.currentColor` | `p.paint` | The palette colour last picked with the paint keys. |
| `containerRayCast(%start, %end, %mask, %exempt)` | `raycast(from, dir, range, ignore)` | Answers at once. A map with `kind`, `id`, `ref`, `x`, `y`, `z`, `nx`, `ny`, `nz`, `distance`, or `()`. No type mask: check `kind`. |
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
| `%obj.setImageAmmo(0, %x)` | `set_image_ammo(p, ammo)` | `player` |
| `%client.setControlCameraFov(%fov)` | `set_fov(p, fov)`, `set_fov(p, ())` | `player` |
| `%obj.setTransform`, `%client.spawnPlayer()` | `teleport(p, x, y, z)`, `respawn(p)` | `player` |
| `%obj.setVelocity`, `addVelocity` | `push(ref, vx, vy, vz, by)` | `physics` |
| `%player.tool[%i] = ...` | `give_item(p, item, equip)` | `player` |
| `centerPrint`, `bottomPrint` | `center_print(p, text, s)`, `bottom_print(p, text, s)` | `chat` |
| `messageClient`, `messageAll` | `tell(p, text)`, `broadcast(text)` | `chat` |
| `serverPlay3D(%profile, %pos)`, `%client.play2D` | `sound_at(profile, x, y, z)`, `play_sound(p, profile)` | `effects` |
| `%obj.playThread(%slot, %seq)` | `play_thread(p, thread, sequence)` | `effects` |
| A stretched `StaticShape` tracer | `beam(from, to, #{ color, width, seconds, muzzle })` | `effects` |
| Mission lights baked into the map (v20 scripts could not change them) | `set_map_lights([x, y, z], radius, #{ on, color, brightness })` | `lighting` |
| `%brick.setColor(%c)` over a hand-written search of touching bricks | `paint_fill(p, brick, color, limit)` | `world.edit` |
| `%client.score`, dynamic fields | `get_player`/`set_player` on declared state | none |

## Hooks

| TorqueScript | Here |
|---|---|
| `Image::onFire`, `onReload` and other state scripts | The image's `commands.states`: entering a state with that script runs a rule command. |
| `Image::onTrigger` slot 4 (right mouse) | `commands.jet` |
| `serverCmdLight` packaged for a reload key | `commands.light` |
| `schedule(%ms, ...)` | `on_tick` with a tick counter in state |
| `GameConnection::onClientEnterGame`, `onDeath`, `Armor::damage` | `on_join`, `on_death`, `on_damage` |
| `serverCmdSomething` | A declared command, `cmd_something` |

## Not here yet

- **Client-side prints of your own layout**: HUD panels and client code
  carry text instead.
- **Custom casing models**: `stateEjectShell` ejects the stock brass.
- **An Add-On's own particles and explosions from Import Add-On**: the
  importer does not convert them yet, so a ported gun uses the base
  game's effects. It is importer work and comes next.
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
| Converting an Add-On's own particles, explosions and sounds on import | Later | Importer work, not script API; it gets its own pass. |
| Client fallback to an Add-On's own `sounds.json` by profile name | Later, with the importer pass | Rules already play weapon-pack sound keys and v20 profiles; the fallback only matters once the importer converts AudioProfiles. |
| Shape converter: `-1` starts for unused animation pools | Kept (a converter fix) | Torque writes `-1` for a pool a sequence does not use; the converter now reads it as "none" instead of refusing the model. Any Add-On model can have it. |
| Importer: items with no `uiName` fail the package | Not reproduced | Main's importer takes an item without `uiName` (it gets an empty name). Needs the Add-On that failed to check further. |
| Import Add-On's test shot at 2 s instead of 0.5 s | Changed: it fires when the gun is ready | A fixed time is wrong for some gun either way. The check now presses as soon as the image reaches a state the trigger leaves and holds until the image takes it, as a player's click does, so neither a slow draw nor a ready state that plays out its timeout first is reported as a gun that fires nothing. |
| The gun mod's own rules, models and sounds | Not in the base game | Third-party content stays out of the repository. |
