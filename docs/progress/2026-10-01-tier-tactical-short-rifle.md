# 2026-10-01 Tier+Tactical: name clash fix and Kai's Short Rifle

## What changed
- Weapons merge (`Pack::merge_with`): a damage type or explosion two
  Add-Ons declare differently is no longer "kept the earlier one". The
  later package's is kept as `<package>:<Name>`, and its own projectiles,
  auras and shield reflect kills point at it. If the later package depends
  (directly or through others) on the earlier one, its declaration replaces
  the earlier for everyone, as v20's last-loaded rule. The same declaration
  twice is one. Dependencies come from each part's package.json
  (`content_identity`). Rules: `damage` and `on_damage` answers that name a
  bare type resolve to the caller's own (or a package it uses), and
  `info.type` stays the Torque name.
- Engine hitscan `ricochet` (`times` 1..8, `damage`, `shooter`): the ray
  mirrors off what it meets over the range left, more damage per landing
  before, its shooter's share once it comes back. Each turn is an
  `Event::Ricochet` the sim draws as a beam in the tracer's look;
  `on_damage` gets `info.bounces`.
- A living player may always be pushed by themselves (`push` op), as the
  original's `setVelocity` on a shot's own shooter.
- Importer: `datablock:<Name>` handles mark a datablock the import could
  not convert as carried out by the port.
- Port `weapon_shortriflekai` (partial): hitscan from the `raycast*`
  fields, ricochet times/damage/shooter read from fireRaycast and
  onRaycastDamage, the shared rules' `ricochet_shots` (shove on every hit;
  with Emote_Critical on, crits after a turn: from below x below, else the
  head x head, under the gun's crit damage type).

## Evidence
- Real copy (`run4.sh Weapon_ShortRifleKai`): applied, behaviours 16/16,
  unsupported 0.
- `cargo test -p bri-weapons --test tactical_seams` (ricochet off the floor
  into B with +30, back into the shooter at its share, out-of-range refused).
- `cargo test -p bri-addon-import --test tier_port the_short_rifle_ricochets_and_crits_only_after_a_turn`:
  stand-in on Tier 1 with Emote_Critical and the Adventurer's Weapons
  stand-in on beside it, both declaring `StandinCrit` with different kill
  messages; each gun's crit kill shows its own.
- `weapons` merge unit test for the clash and dependency rules;
  `import.rs` merge test updated (a copied Add-On's damage type naming its
  own icon is kept apart).

## Next
- Weapon-data settings seam (datablock fields bound to server settings),
  restart-only prefs, Event_AddAmmoTT, Frogs, Frogs WWII.
