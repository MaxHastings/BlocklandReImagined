# 2026-10-01 Tier+Tactical: the twelve Skins packs

Kai's Skins packs (Pistol, Dualies, Rifles, SMG, Shotgun, LMG, Magnum,
Rifles T2, Shotgun T2, Sniper, Bullpup, Machine Pistol) are not skins
alone. Each is a set of full guns built on a host pack's datablocks
(`ClassicPistolItem : PistolItem`), with copies of the host's gun scripts
under its own names. Each gets a port that includes the shared
`tier-tactical-core` and `tier-tactical` rules, so the same script rules
that port the hosts port the skins. On the scripts of each real copy, with
its hosts as the reference, every port applies and every script function
is ported, with nothing unsupported. The Gate's real copies give the pins.

Importer, each generic:

- Datablock fields written as arithmetic on other datablocks' fields
  (`TT_maxAmmo = classicPistolItem.TT_maxAmmo*2`,
  `TT_raycastDirectDamage = classicPistolImage.TT_raycastDirectDamage`)
  are evaluated as Torque did when the datablock loaded
  (`Datablocks::number`: `+ - * /`, brackets, literals and `name.field`).
  The magazine and hitscan readers use it.
- An `onMount` that plays an arm, gesture or root thread plays it as the
  gun is drawn. This rule moved from Melee Extended II's port to
  `tier-tactical-core`, so the Bolt Rifle skin and the chainsaw share it.

Never-run functions, recorded as handled:

- The Retro Magnum's `onReloadWait` is named by none of its own states.
- The Silenced dualies' `onLoadCheck` is not what the inherited Akimbo
  states name (`TT_onLoadCheck`).
- Kai's server.cs leaves the Covert Pistol (Pistol skins) and the Covert
  dualies unloaded.

Each pack has a CC0 stand-in
(`tests/fixtures/ports/Weapon_Skins_*`). These hold the datablock and
method names the covers check, built on the host stand-ins, with our own
names and numbers.

Checks:

- `cargo test -p bri-addon-import --test tier_port skins_are_their_hosts_guns_under_their_own_names`
- the wider addon-import run, and clippy.

## On the real copies

The Gate's real copies (pinned now) left LMG 16/20, Rifles T2 26/32 and
SMG 15/25. Their reference also holds Weapon_ModernWarbattles, which
declares `SubmachineGunImage`, `LightMachinegunImage` and the battle
rifle under the same names as Tier 1 and 2. The reference kept the first
declaration it read (Modern Warbattles, by name order), whose states
name no `onReloadWait` or `onReloaded`. In v20 the declaration that holds
is the last one to load before the Add-On. Add-Ons load in name order,
each after the ones it requires, and declaring a name again sets its
fields on the same datablock. `Reference::settle_for` now settles each
shared name that way for the Add-On being imported (later fields win,
the rest stay), so the skins inherit their required pack's states.
Guarded by `import a_name_two_add_ons_declare_is_the_one_loaded_last_before_this_one`.

## Impact Rifle

Kai's Impact Rifle gets `weapon_impact_rifle`. Its spread check is
`still && (getSimTime() - %obj.lastShotTime) > 500`. Only Tier 2's
Assault Rifle and Light MG set `lastShotTime`, so the port reads it as
spreading 0.0002 when still and 0 on the move. A pack rule reads this
shape. CC0 stand-in and
`tier_port the_impact_rifle_spreads_by_whether_its_holder_stands_still`.

## Gate re-check of 3809ecea2

With MWB still in the reference: LMG 20/20 and Rifles T2 32/32, both
listed; MWB 132/132, Tier 1 88/88 and Tier 2 52/52 unchanged. The Impact
Rifle applies 3/3 with datablocks 14/14 and nothing unsupported; its port
is pinned to that copy (sha256 65a13321…e01b).

SMG stayed 20/25: each skin's `onReloadWait` (Micro, Modern, Naval,
Classic, Silenced) is a method none of its states names, so v20 never
called it, as with the Retro Magnum. The port says so in `handles`.
