# Adventure Pack seam audit

Date: 2026-10-01. Branch `claude/adventure-pack-n3spj2`.

Max's ask (2026-09-30): Bushido's Adventure Pack should be another Add-On
that comes with the game, and a test that the engine has every seam such a
pack needs. Max's later decisions for every classic Add-On: originals only,
and bundled in the download. The release zip carries the original pack,
imported from Max's copy and credited to Bushido; this repository holds
only our port (`crates/addon-import/ports/weapon_modernwarbattles`), which
Import Add-On applies to the player's copy. The Tier Tactical thread owns
the shared `magazine` seam.

An earlier cut of this branch built our own remake (models, sounds and a
rules script). It was removed before landing. What stays is the engine
seams it showed were missing.

## What the pack is

Adventurer's Weapons (`Weapon_ModernWarbattles`): 9 guns, a baton, a
machete and a frag grenade, on Jack's hl2 ammo system (a magazine per gun,
a reserve per ammo type, 11 ammo boxes), with projectile headshots and
hitscan crits. The Glass 1019 release (`Weapon_AdventurePack`, Conan's)
is the bundled one: the same ammo system with its own reserves, a Paired
Shotgun, a taser and its own raycast support script, without crits. Both
ports share one rules script (`weapon_modernwarbattles/rules`), each with
its own values.

## Seams, piece by piece

| Piece | Usually done in v20 with | Here | Status |
|---|---|---|---|
| Guns, rounds, damage types, kill messages | datablocks | Import Add-On to `weapons.json` | present |
| Spread shots and their volleys | `onFire` loops of `new Projectile` | image `shot`, `volleys`, read by the `shots` reader | **ported** |
| A gun whose fire states each run their own script | `onFire2`, `onFire3` with wider spreads | image `state_shots` | **new** |
| Bigger rounds | `scale = "1.5 1.5 1.5"` on the projectile | shot `scale`; projectile `fixed_damage` where the pack's own `damage` ignored scale | **new** |
| A magazine's last rounds fire another shot | `if(toolMag <= 2)` branch | image `last_shot`, magazine `last_rounds` | **new** |
| Magazines, reserves, reload, ammo display | hl2 ammo system: `maxmag`, `ammotype`, `toolMag`, `toolAmmo` | the port's `magazines` to Tier's `magazine`; `one_by_one` from `onReloadSingle` | **ported** |
| Ammo boxes, spare guns | packaged `Armor::onCollision` | rules `on_pickup`, `give_ammo`; a dropped gun's rounds in `info.rounds`, `#{ rounds }` answer | **ported** |
| Picking up a second of a gun | per-slot `toolMag` | the holder keeps their magazine | **new** |
| State sounds, arm moves, both hands | `serverPlay3d`, `playThread(2/3, ...)` in state scripts | state `sound`, `arm`, `gesture` | **new** (`gesture`) |
| Camera kick | `spawnExplosion` of a shaking recoil blast | shot `kick` | ported |
| Headshots | `getHitbox`, crouched counts as head, `headshotMultiplier` | `info.region`, rules | **ported** |
| Hitscan guns | Support_RaycastingWeapons image fields | `shot.hitscan`, a ray projectile per gun, the `hitscans` reader | **ported** |
| Crits and their kill message | `isRaycastCritical`, ×3, `raycastCritDirectDamageType`, only `if(isObject(CritProjectile))` | rules, `on_damage` answers `#{ amount, type }`; manifest `optional_dependencies`, script `enabled` | **ported** |
| Melee swings kill players and vehicles | `onRaycastDamage`: twice `maxDamage`, own type | rules `on_damage`, new `on_vehicle_damage` | **ported** |
| Swing hit sounds | random pair set in `onFire` | rules `on_projectile_hit`, `sound_at` | **ported** |
| Grenade shrapnel and trails | `onExplode` loops | projectile `children` as a list | **ported** |
| Taser tumble | `tumble(%col, ms)` | `tumble(..., seconds)` | **ported** |
| Crit effects (explosion, crit sounds) | `Emote_Critical`'s `critProjectile` and sounds, `spawnExplosion`, `play2d`, `serverPlay3d` | script `spawn_explosion`, `play_sound`, `sound_at`; Emote_Critical imports whole (no scripts) | **ported** |
| Grenade cooking and countdown | `onPinDrop` `burnSched`, `sendCenterNade`, `burnedIt`, thrown fuse `4000 - chargeTime` | image `cook`, projectile fuses | **new** |
| Bomblet fuses | `projectile::onAdd` `400 - getRandom(...)` | children `fuse_ticks` | **new** |
| Light key reloads | hl2 `serverCmdLight` | magazine `light_states` | **new** |
| Head-hit flinch | hitbox `playThread(0/2, jump)`, `plant` after 50 ms | needs a client body thread 0 and a timer | **not yet** |
| Hitmarker | `commandToClient('hitmarker')` | nothing in either pack draws it | none in the original |

## Approximations

- Shrapnel directions are uniform; the script's random throw leaned
  upward. Each set's speed is the script's mean.
- The engine keeps one magazine per gun per holder, where v20 kept one per
  tool slot: two of the same gun share theirs.
- The hitbox head hit's `goremodHitProjectile` and `gamedamage2Sound` are
  defined nowhere, so the original never showed them; the port has none.

## Tests

- `crates/addon-import/tests/adventure_port.rs`: both ports on CC0
  stand-ins: magazines and reloads, shots, volleys, last shot, state shots
  and both hands' moves, hitscans and shrapnel from the scripts; boxes,
  headshots, crits with their kill message only beside Emote_Critical's stand-in and
  its effects and sounds, grenade cooking and bomblet fuses, melee kills with their own
  message and hit sound, vehicle wrecks, dropped spare guns and the taser
  in hosted games.
- `crates/weapons/tests/addon_seams.rs`: volleys, the last shot, state
  shots with their gestures, children lists, an `onfire` command.
- `crates/sim/tests/hit_regions.rs`, `script_api.rs`: hit regions.
