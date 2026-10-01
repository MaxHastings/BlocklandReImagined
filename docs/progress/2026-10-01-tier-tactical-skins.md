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
