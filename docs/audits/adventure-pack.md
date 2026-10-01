# Adventure Pack seam audit

Date: 2026-10-01. Branch `claude/adventure-pack-n3spj2`, for v0.1.11.

Max's ask (2026-09-30): Bushido's Adventure Pack should be another Add-On
that comes with the game, and a test that the engine has every seam such a
pack needs.

The repo is public, so nothing of the original ships: no scripts, models,
textures, sounds or icons, and nothing derived from them. Our Adventure
Pack is our own code and assets, credited to Bushido's design (re-released
by Conan) in each package's `authors`. Shipping the original files would
need Bushido's permission.

## What the pack is

A set of modern guns for Blockland v20 minigames, played like an
adventure or survival shooter: rifles, pistols, shotguns, sniper rifles and
a taser, with magazines and reloads, ammo reserves fed by ammo boxes and
spare guns, headshots, casings and muzzle flashes, and an ammo counter on
screen.

## Seams, piece by piece

| Piece | v20 did it with | Here | Status |
|---|---|---|---|
| Guns, rounds, damage types, kill messages | datablocks | `weapons.json` | present |
| Shotgun spreads | `onFire` loops | image `shot` | present |
| Magazines and reserves | `%client` fields, `setImageAmmo` | player state, `set_image_ammo`, `commands.states` | present |
| A shot both fires and counts | `onFire` calling `Parent::onFire` | an `onfire` command on an image with a projectile now runs and the round still flies | **new** |
| Reload key | packaged `serverCmdLight` | image `commands.light` | present |
| Shell-by-shell reload stopped by the trigger | state machine | states with `down` out of the reload | present |
| Ammo boxes, spare guns as ammo | `onPickup` | `on_pickup` answering `"take"` or `false` | present |
| A dropped gun keeps its magazine | item fields | `on_drop` data | present |
| Headshots | `getDamageLocation` in `Damage` | `info.region` in `on_damage`, `region` on rays and `on_projectile_hit`, `hit_region()` | **new** |
| Hitscan revolver and sniper rifle | `containerRayCast` | `raycast`, `beam`, `damage` with a type | present |
| Damage type names in hooks | `%damageType` | `info.type` is the type's name without `$DamageType::`, so a round and a script's `damage` read alike | **new** |
| Taser stun | knocking the player over | `tumble` (`physics`) | present |
| Casings, muzzle flash | `stateEjectShell`, emitters | image state `eject_shell`, `emitter` (stock `gunShellDebris`, `gunFlashEmitter`) | present |
| Ammo counter | `bottomPrint` per shot | a HUD panel bound to an owner-visible key | present |
| Counter only while a gun is out | `bottomPrint` cleared on unmount | HUD `holding` | **new** |
| Its own models | `.dts` files | `<name>.shape.json` beside the weapons, with its textures | **new** |
| Icons in the stock style | PNGs | drawn from the model on each machine (`render.json`), `"frame": "model"` to fit long guns | **new** (frame) |
| Sounds | `.wav` | pack `sounds` | present |
| Scopes | `zoom` and overlays | image `zoom`; the Sniper Rifle thread adds overlays, levels, sway | elsewhere |

## Left out on purpose

- **Per-shot arm recoil animation and camera kick.** Cues are budgeted at
  64 a second per Add-On, and an automatic gun would spend it. The image
  states already play the stock fire sequence. Camera kick is a feel
  change that belongs in the weapon format if Max wants it.
- **Scope overlays, zoom levels and sway.** The Sniper Rifle thread owns
  them; the pack's scoped guns use plain `zoom` until that lands.

## How it is built

- `tools/make_adventure_pack.py` writes the weapons pack (23 guns, 8 ammo
  pickups), every model (boxes and cylinders on one palette texture),
  icon requests and synthesized sounds, and the generated tables at the
  top of the rules script.
- `packages/adventure/adventure-pack` (weapons, shared),
  `adventure-pack-rules` (the rules, host) and `adventure-pack-hud` (the
  ammo panel). All three ship turned off in `packages/default-addons.json`.
- Tests: `crates/sim/tests/adventure_pack.rs` plays it through the session
  (magazines, light-key and shell reloads, ammo boxes, spare guns, a
  dropped gun's magazine, revolver and pistol headshots);
  `crates/sim/tests/script_api.rs` covers rays' `region` and `hit_region`;
  `crates/package-runtime/tests/check.rs` checks the three packages clean
  and names a model with a missing texture; `crates/client/src/items.rs`
  presents its own models.
