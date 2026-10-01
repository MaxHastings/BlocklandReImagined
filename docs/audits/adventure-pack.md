# Adventure Pack seam audit

Date: 2026-10-01. Branch `claude/adventure-pack-n3spj2`.

Max's ask (2026-09-30): Bushido's Adventure Pack should be another Add-On
that comes with the game, and a test that the engine has every seam such a
pack needs. Max's later decision for every classic Add-On: originals only.
The game loads the original files from the player's own Blockland
`Add-Ons` folder, and ships none of them. A player without the files does
not get the Add-On. The Tier Tactical thread owns that loader and the
shared `magazine` seam.

An earlier cut of this branch built our own remake (models, sounds and a
rules script). It was removed before landing. What stays is the engine
seams it showed were missing.

## What the pack is

A set of modern guns for v20 minigames: rifles, pistols, shotguns, sniper
rifles and a taser. It has magazines and reloads, ammo reserves fed by ammo
boxes and spare guns, headshots, casings, muzzle flashes and an ammo
counter.

## Seams, piece by piece

The middle column is how a v20 weapon Add-On usually does each job. It was
not checked against the pack's own files, which the cloud could not
download. Check it when the Tier loader first reads the real pack.

| Piece | Usually done in v20 with | Here | Status |
|---|---|---|---|
| Guns, rounds, damage types, kill messages | datablocks | Import Add-On to `weapons.json` | present |
| Shotgun spreads | `onFire` loops of `new Projectile` | image `shot` | present |
| Magazines, reserves, reload, ammo display | client fields, `setImageAmmo`, `bottomPrint` | Tier's shared `magazine` seam | Tier thread |
| A shot that fires and counts | `onFire` calling `Parent::onFire` | an image with a projectile and an `onfire` command does both | **new** |
| Reload key | packaged `serverCmdLight` | image `commands.light` | present |
| Ammo boxes, spare guns as ammo | `onPickup` | `on_pickup` answering `"take"` or `false` | present |
| A dropped gun keeps its rounds | item fields | `on_drop` data, or Tier's `magazine` | present |
| Headshots | `getDamageLocation` in `Damage` | `info.region` in `on_damage`, `region` on rays and `on_projectile_hit`, `hit_region()` | **new** |
| Damage type names in hooks | `%damageType` | `info.type` is the type's name without `$DamageType::` | **new** |
| Hitscan guns | `containerRayCast` | `raycast`, `beam`, `damage` with a type | present |
| Taser stun | knocking the player over | `tumble` (`physics`) | present |
| Casings, muzzle flash | `stateEjectShell`, emitters | image state `eject_shell`, `emitter` | present |
| Ammo panel only while a gun is out | `bottomPrint` cleared on unmount | HUD panel `holding` | **new** |
| Icons in the stock style | PNGs | the original PNGs, or drawn from the model (`render.json`, a stock pose fitted once for all icons) | present |
| Scopes | `zoom`, overlays | image `zoom`; overlays, levels and sway are the Sniper Rifle thread's | elsewhere |

## Left out on purpose

- **Per-shot recoil animation and camera kick.** Cues are budgeted at 64 a
  second per Add-On, and an automatic gun would spend it. Camera kick is
  a weapon-format change (Tier plans an image state `shake`).

## Tests

- `crates/sim/tests/hit_regions.rs`: a round in the chest and in the head
  reaches `on_damage` with its region, point and damage type, and a rule
  triples the headshot.
- `crates/sim/tests/script_api.rs`: `region` on rays, `hit_region`.
- `crates/sim/src/player.rs`: the Torque bands.
- `crates/package-runtime/src/content.rs`: HUD `holding`.
- `crates/weapons/tests/addon_seams.rs`: an `onfire` command and the round.
