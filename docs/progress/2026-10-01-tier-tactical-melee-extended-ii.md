# 2026-10-01 Tier+Tactical: Melee Extended II

Kai's Melee Extended II (five more melee weapons, the chainsaw, the riot
shield and three hidden easter-egg weapons) gets its port,
`weapon_melee_extended_ii`, pinned to the Gate's real copy (sha256
cae20f11…c4e2). On that copy, imported beside Melee Extended, the port
applies with all 47 cover patterns matching. The 7 calls it leaves
unsupported are all preferences (`isFunction`, 3 `RTB_registerPref`,
3 `TT_defaultIfUnset`) waiting for the Add-On settings seam. A CC0
stand-in (`tests/fixtures/ports/Weapon_Melee_Extended_II`, our own
numbers and sounds) runs the shield in a hosted game.

Engine, each a generic seam:

- `Image::guard`: a shield raised while the holder's right hand is in one
  of its states. It covers what strikes the side they face, or with
  `front` (Kai's up/down limits) what lands above or below set heights
  when they look steeply up or down. A projectile it stops does
  `projectile_damage` of its damage and `push` of its push, plays the
  clang burst and one of its sounds, counts down durability and, with
  `reflect`, flies back along the holder's look as theirs (not again
  within 60 ticks). A blast from a stopped shot spares the holder its
  damage and scales its push. A stopped ray keeps `projectile_damage` and
  is not sent back. Other covered harm (melee, blasts from elsewhere) takes
  `damage`, except hits over 5000 or at the feet, as Kai's
  `ShapeBase::damage` override. At zero durability the shield breaks:
  the break burst goes off and its item leaves the holder's tools.
- `DamageType::special`: Support_SpecialKills' `addSpecialDamageMsg`. The
  kill message names the special type ("Reflected") with the killing
  weapon's icon in `%3`. Weapon damage carries the special type through
  crit and radius renames.

Importer:

- `addSpecialDamageMsg(name, murder, suicide)` becomes a special damage
  type.
- A script rule `into: state` on `onMount` targets the image's first
  state, so the chainsaw's revving arm cue plays as it is drawn.
- The five melee script rules and the melee crit handle move to
  `_shared/tier-tactical-melee`, shared by Melee Extended and II.

Not ported, and why:

- The shield's and easter eggs' preferences wait for the settings seam.
  The easter-egg weapons import hidden (`hidden: true`) until then.

Checks:

- `cargo test -p bri-weapons --test guard`
- `cargo test -p bri-addon-import --test tier_port melee_extended_ii`
- `cargo test -p bri-weapons-import special_kills`
- the wider addon-import, weapons, sim and net runs, and clippy.
