# 2026-10-01 Tier+Tactical: every Tier 1 to 2A behaviour run, and a report that says so

The Gate's real-copy reports counted Tier 1 at 14/88 ported with 91
unsupported. Each leftover was checked against Kai's script; what the game
did not do became a seam, and the report now counts what the port carries
out.

Report:

- `Port::handles`: a port names, per function (`WeaponImage::TT_canFire`)
  or top-level call (`call:TT_registerAmmoType`), the engine seam or host
  rule that does it. Readers record what they read too: shots, script
  rules, tables, and a magazine state script that only works its rounds
  (`TT_reload`, `setImageLoaded`, `TT_displayAmmo`; an `onFire` may also
  take its rounds and fire the image's own shot). A behaviour counts as
  ported when covered or handled; a handled top-level call moves from
  `unsupported` to the report's `ported` list ("Carried out by the port").
  A failed port clears what it claimed.

Engine, each a generic seam:

- Magazines are kept per tool slot (`slot_key`), as Tier+Tactical's
  `%obj.TT_toolAmmo[%slot]` and the hl2 system's `toolAmmo[%slot]`: two of
  one gun each keep their rounds, a thrown gun takes its slot's rounds,
  drawing another copy of the gun in hand switches to that slot's rounds.
- `Magazine::display_ticks` (Kai's four-second `bottomPrint`) and
  `display_scripts` (a dry pull's `TT_onEmptyFire`); the light key with
  nothing to load shows the ammo too (`TT_onUseLight`).
- A reload a scripted magazine's states run stops as a tool is drawn or put
  away (`SniperCarbineItem::onUse` clearing `TT_forceToolReload`).
- `Item::rotate`: an item whose `onAdd` set `%obj.rotate = true` turns once
  every three seconds where it lies (Torque `Item`'s `sRotationSpeed`).
- `Look::first_person_only`, imported from `firstPersonOnly`: Tier 2's
  `LMGArmor` keeps the gunner's view in the eye while laid on.
  `isSurvivor` is a mark only other Add-Ons read and is no longer a gap.
- Script rules read `playThread(1, armreadyboth)` in an `onMount` as
  `both_arms` (the akimbo pistol's and PDW's left guns) and the three-
  argument `TT_reload(%obj, %slot, sound)`.
- Host rules: bots neither drop an ammo bag nor pick up ammo (Kai's
  `BotAmmoDrop`, `BotNadeDrop`, `BotAmmoPickUp` defaults).

Real copies (`bri-import-addon` with Tier 1 beside each), behaviours
ported: Tier 1 88/88, Tier 1A 19/19, Tier 2 52/52, Tier 2A 50/50.
Still unsupported, honestly: `RTB_registerPref` (38 in Tier 1, 1 in
Tier 2) and Tier 2's `isFunction(registerPreferenceAddon)` wait for the
settings seam on the Slayer branch; Tier 1's `isFile` picks Sound_Blockland
or base click sounds for the reload clicks, which depends on the base audio
bank naming `Block_MoveBrick_Sound`.

Checks: `cargo test -p bri-addon-import -p bri-weapons -p bri-motor -p
bri-sim -p bri-net -p bri-package-runtime`, `cargo test -p bri-client
--lib first_person`, clippy `-D warnings` on the changed crates. The guard
for the stopped reload fails on the old code (`tier_port.rs` "no rounds
moved").
