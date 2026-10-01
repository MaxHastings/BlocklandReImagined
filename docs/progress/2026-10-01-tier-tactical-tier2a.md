# 2026-10-01 Tier+Tactical group 4: Tier 2A

Kai's Tier 2A ports on `_shared/tier-tactical` and applies on his real copy
(`bri-import-addon` against a scratch reference holding Tier 1 and Tier 2;
Tier 1, 1A and 2 still apply). Status `partial`, like the other Tier packs,
until all 26 are in.

Engine and importer seams, each generic:

- Magazine `checks` entries gain `spend` (the check takes the shot's rounds
  as it loads one: the bullpup's later burst rounds) and `keeps_reload` (a
  reload under way is not cut short: the carbine's forced reload).
- Image commands gain `mount`, beside `unmount`; the scoped carbine pushes
  its slow player type on mount and pops it on unmount.
- Script rules target `check`, and `method` combines `*` with named methods
  (`*|onMount`).
- Packs no longer copy a dependency's projectiles under `v20:` ids, which
  collided as soon as two packs used the same one. They name them by the id
  the dependency's own package gives (`v20.projectile.<name>` for the base
  game, `<namespace>:projectile/<name>` for another import) and list them in
  `Pack::external_projectiles`; `Pack::merge` resolves them and drops an
  image whose projectile nothing provides. `WeaponsWorld::new` refuses a
  pack with unresolved ones. Ports read the dependency's projectiles beside
  the import's own.
- When every `exec` reached from `server.cs` names a plain path, scripts
  none reaches are left out (v20 never ran them). Tier 2A ships one with
  values the weapons pack refuses.
- A table row whose archetype filter names an unknown player type is left
  out instead of failing the port.

Protocol: `crates/net/protocol-changes/tier-weapons-pack.md` for the
weapons pack fields (Shot.free, the hitscan fields, rested, Check.spend and
keeps_reload, commands.mount, external_projectiles).

Checks: `cargo test -p bri-addon-import -p bri-weapons` (tier_port's five
tests pass; weapons' content-needing runtime tests need `content/`),
`cargo clippy -p bri-weapons -p bri-addon-import --tests --no-deps -D warnings`.

Next: Explosive 1/2, Medic 1, Melee I/II, the Skins packs, Frogs and WWII,
Impact Rifle, ShortRifleKai, Event_AddAmmoTT.
