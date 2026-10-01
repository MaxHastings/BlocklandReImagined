# 2026-10-01 Tier+Tactical: every preference carried out

The coordinator's goal was no unsupported preference across the Tier packs,
reported honestly. On the real copies every Tier pack now reports 0
unsupported; three packs are not ported and move to v0.1.12 (below).

What changed:

- Item lists follow settings: an item's `hidden` may be bound. The
  session refreshes the mini-game catalog when a retune hides or shows an
  item (`MinigamesWorld::set_catalog`), and the client reinstalls its item
  choices (`WeaponContent::apply_settings` says when they changed). Tier's
  Disable Tier 1, Disable Ammo Items, Disable Explosive 1, Disable Grenade
  Pickups and the `???` easter eggs bind it; each is a restart preference
  in RTB, so it applies from the next start or map.
- Easter eggs: the Retro Magnum, Classic Shotgun and the four Tier 2 rifle
  skins were imported visible though Kai loads them only with `???`; they
  are now hidden like the others and offered while it is on.
- Magazine `remount` (Remount Duplicate Items) and `clear_when_out` (Clear
  Unusable Grenades); `hide_display` per grenade (Display Grenade Count).
- Guard `bots_keep` (Riot Shield Breaks for Bots off) and `fall_damage`
  (Riot Shield Stops Falls: a fall or crash met with the raised shield
  does an eighth, with its clang); actors know they are bots
  (`WeaponsWorld::set_bot`, set when a bot is made).
- Aura `ally_damage` and `Query::is_ally` (Molotov Friendly Fire
  Override): the fire sears the thrower's teammates and allies for 1 in a
  mini-game with weapon damage on, friendly fire or not, as Kai's did.
- Importer: bindings combine `values` with `scale` (shield durability:
  -1 never breaks, 0 is 1, else the count); `only` takes `datablock`,
  `*` globs and lists; rules with only settings need no capabilities; a
  `pref:<global>` handle reports a preference carried out with no setting.

A pivot, said plainly: Tier 1's four bug-fix preferences (Firing on Death
Bugfix, Stop Anims on Death, SetInventory Bugfix, RemoveItem Bugfix) have
no setting. The engine always puts a dead player's tools away and keeps
each slot's magazine with the gun in it, which is each preference's "on".
Their "off" only brought back the bug, so it is not reproduced; RemoveItem
Bugfix was off by default in Kai's pack, so that default differs.

Evidence on the real copies (`run4.sh`, behaviours ported / unsupported):
Tier 1 88/88, 0; Explosive 1 21/21, 0; Melee Extended II 47/47, 0; every
other Tier pack 0 unsupported. Every binding of every import derives a
valid pack at each value and each combination (scratch checker).

Tests: `bri-weapons` guard (durability values, bots keep theirs, falls
met), magazines (remount), thrown grenades (cleared when out, ally
damage); `tier_port` a restart preference hides the guns from the next
start, and the bug-fix preference is reported carried out with no setting.

After merging main 730739b14 (which carries the server settings seam this
branch's weapon settings build on): the 25 finished packs import with 0
unsupported and every binding derives a valid pack; crate tests pass
(`bri-weapons`, `bri-addon-import`, `bri-sim`, `bri-net`,
`bri-minigames`, `bri-package-runtime`, `bri-client` lib apart from its
GPU test).

Moved to v0.1.12 (Max chose "Ship what's ready" for v0.1.11): ports of
Event_AddAmmoTT (0/2 behaviours), Frogs Weaponry (0/110, 54 unsupported)
and Frogs Weaponry WWII (0/24). No port exists for them, so they stay out
of the bundle and nothing half-ported ships. Every preference of the
finished packs is carried out; none is left for v0.1.12.

Landing merge (Tier, Adventure and the bundle lane's Kai entries over
batch174): `Pack::merge` reports Add-On problems, and the dependency-aware
merge now does too. The bundle builder imports each copy on its own, so
Tier 2 could not see the Tier 1 it requires and its port failed. The
importer now reads a required Add-On it lacks from the folder beside the
copy, as its hint already told players (`Reference::add_beside`; test
`a_required_add_on_beside_the_copy_is_its_reference`). `bundled_in_game`,
`bundle`, Tier and Adventure tests pass.
