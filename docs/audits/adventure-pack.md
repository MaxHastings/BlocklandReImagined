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

Adventurer's Weapons: 9 guns, a baton, a machete and a frag grenade, on
Jack's hl2 ammo system (a magazine per gun, a reserve per ammo type, 11
ammo boxes), with projectile headshots and hitscan crits. Glass 1019 is
Conan's re-release with more guns, built on it; its import is next.

## Seams, piece by piece

The middle column is how the pack does each job, from the Gate's notes on
`Weapon_ModernWarbattles.zip` (Adventurer's Weapons, the core of the full
pack).

| Piece | Usually done in v20 with | Here | Status |
|---|---|---|---|
| Guns, rounds, damage types, kill messages | datablocks | Import Add-On to `weapons.json` | present |
| Shotgun spreads | `onFire` loops of `new Projectile` | image `shot` | present |
| Magazines, reserves, reload, ammo display | hl2 ammo system: item `maxmag`, `ammotype`, `toolMag`, `toolAmmo`, `bottomPrint` | the port's `magazines` (item fields to Tier's `magazine`) | **ported** |
| A shot that fires and counts | `onFire` calling `Parent::onFire` | an image with a projectile and an `onfire` command does both | **new** |
| Reload key | packaged `serverCmdLight` | image `commands.light` | present |
| Ammo boxes | packaged `Armor::onCollision` | the port's rules: `on_pickup`, `give_ammo` | **ported** |
| A dropped gun keeps its rounds | item fields | `on_drop` data, or Tier's `magazine` | present |
| Headshots | `getHitbox` head nodes, crouched counts as head, `headshotMultiplier` | `info.region` in `on_damage` (Torque's bands, not the head nodes), the port's rules | **ported** |
| Damage type names in hooks | `%damageType` | `info.type` is the type's name without `$DamageType::` | **new** |
| Hitscan guns | Support_RaycastingWeapons, crit ×3 with a shove | Tier's `shot.hitscan`; the crit is a rule | to port |
| Casings, muzzle flash | `stateEjectShell`, emitters | image state `eject_shell`, `emitter` | present |
| Ammo panel only while a gun is out | `bottomPrint` cleared on unmount | HUD panel `holding` | **new** |
| Icons in the stock style | PNGs | the original PNGs, or drawn from the model (`render.json`, a stock pose fitted once for all icons) | present |
| Scopes | `zoom`, overlays | image `zoom`; overlays, levels and sway are the Sniper Rifle thread's | elsewhere |

## Left out on purpose

- **Per-shot recoil animation and camera kick.** Cues are budgeted at 64 a
  second per Add-On, and an automatic gun would spend it. Camera kick is
  a weapon-format change (Tier plans an image state `shake`).

## Tests

- `crates/addon-import/tests/adventure_port.rs`: the port on a CC0
  stand-in: magazines from item fields, one reload through the image's
  states, ammo boxes (typed twice, capped) and headshots in a hosted game.

- `crates/sim/tests/hit_regions.rs`: a round in the chest and in the head
  reaches `on_damage` with its region, point and damage type, and a rule
  triples the headshot.
- `crates/sim/tests/script_api.rs`: `region` on rays, `hit_region`.
- `crates/sim/src/player.rs`: the Torque bands.
- `crates/package-runtime/src/content.rs`: HUD `holding`.
- `crates/weapons/tests/addon_seams.rs`: an `onfire` command and the round.
