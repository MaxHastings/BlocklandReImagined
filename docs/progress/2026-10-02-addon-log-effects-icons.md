# Max's v0.1.12 log: effects, kill messages, icons

Follows 2026-10-02-shared-addon-sounds.md. Checked against the original
scripts and zips in the project files.

Fixed:
- Fill Can's icon (`icon_fillcan`) showed its first letter. The importer
  copies `iconName`'s PNG to `textures/<hash>.png` and lists the original
  path in the pack's `resources`. The client looked only for the original
  path, so every imported Add-On icon was missed. `imported_picture` in
  crates/client/src/items.rs maps the path through `resources`. Guard:
  `an_imported_add_on_item_shows_the_icon_it_shipped`, which fails without
  the mapping.
- `tierFragPortBounce` and `hegrenadePortBounce` "show nothing". They are
  our ports' bounce explosions, which play only a sound, so the health
  check now checks their sound instead (references.rs
  `sound_only_explosion`). Guard:
  `a_sound_only_bounce_is_checked_for_its_sound_not_an_effect`.

Same as v20, left alone:
- `advHugeBulletFireEmitter`: Adventure's adventure_Effects.cs names
  particle `advHugeulletFireParticle` (a typo), so the emitter never
  existed in v20 either.
- $DamageType machinePistol, butterflyknifeRadius, ImpactLauncherDirect,
  SmokeGrenadeRadius, Laserguide, L4MachStil and SniperCZoomed: no
  AddDamageType for them exists in any original. v20 shows the default
  kill message too.
- Dualies' `classicpistolakimbo`: should be fixed by the same icon mapping
  if the zip ships the PNG. Not checked; no Dualies zip here.
