# 2026-10-01 Tier+Tactical: Melee Extended

Kai's Melee Extended (eleven melee weapons) gets its port,
`weapon_melee_extended`, pinned to the Gate's real copy (sha256
d5602d27…b989). All 50 script functions the import lists are ported on the
real copy, and every cover pattern matches it. A CC0 stand-in
(`tests/fixtures/ports/Weapon_Melee_Extended`, our own numbers and
sounds) runs the port in a hosted game.

Engine, each a generic seam:

- `Hitscan::sounds`: up to 8 pairs of landing sounds, and each shot draws
  one pair for all of its rays. Kai's `onFire` picked one of two brick
  sounds (sometimes paired with a player sound) with `getRandom(0, 1)`
  before `Parent::onFire`. A side that a pair leaves out keeps
  `player_sound` or `other_sound`.
- `Hitscan::damage`: what each ray deals in place of its projectile's
  damage, from -100 to 100. Kai's combat knife sets the damage field
  before `Parent::onFire` in two fire states: the slash sets 105 (the
  raycast script clamps it to 100) and the stab sets 55.
- State shots may hitscan; the validation for image and state-shot
  hitscans is shared (`Hitscan::validate`).

Importer:

- A fire state of its own whose script ends in `Parent::onFire` (the
  knife's `onStabFire`) casts the image's rays as a state shot; script
  rules lay its own damage on top.
- The raycast script's damage limit holds any damage a script rule sets
  (`limit_ray_damage`).
- Support_TT_Raycasting's port, together with the shared arm and sound
  state rules, moves to `_shared/tier-tactical-core`. The six gun packs
  include it before `tier-tactical`. The melee pack includes only the
  core, because it has no ammo system or host rules, and the gun packs'
  crit rule would misread its zombie-only crit.

Not ported, and why:

- Crits never happen: `TT_isMeleeRaycastCrit` crits only on a zombie, and
  this game has none.
- The Machete's horizontal swing file is never `exec`'d by the Add-On.

Checks:

- `cargo test -p bri-weapons --test raycast_guns`
- `cargo test -p bri-addon-import --test tier_port melee_extended_swings_draw_their_hit_sounds_and_the_knife_stabs`
- the wider addon-import, weapons and sim runs, and clippy.
