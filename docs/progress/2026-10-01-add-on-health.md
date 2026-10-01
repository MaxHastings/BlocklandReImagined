# 2026-10-01 Add-On health: missing references are loud

Max's test build showed Add-Ons that pass their own tests but do nothing in
the installed game. One cause: a missing shape, sound, texture, effect,
damage type, dependency or rules companion silently became nothing. Now each
one is an Add-On health problem (`bri_package::health`), deduplicated by
Add-On, kind and reference.

## What reports a problem
- Content load (`crate::add_on_health::collecting` around the content and
  presentation loads): every `cosmetic::add_on_fault` (models, textures,
  icons, death icons, vehicle art), weapon and vehicle merge notes (duplicate
  ids, an item or image dropped for a missing image or projectile; both
  `merge`s now return structured `Problem`s), Add-On particles, emitters and
  explosions missing a texture, particle, light or burst, and casing models
  that did not load.
- After load (`App::check_add_ons`): Add-Ons left out, Add-Ons whose rules,
  HUD or modes were left out, `check_set` (a companion or manifest
  dependency not on; what `import-report.json` left unresolved: a required
  Add-On the import did not know, a port that did not fit the copy, an asset
  that failed), and every sound, effect, explosion and damage type an
  Add-On's items, images and projectiles name (`Pack::add_on_references`)
  checked with the same lookups play uses (`ClientAudio::has_sound`,
  `WeaponEffects::knows`, `Pack::has_damage_type`), plus Add-On items and
  images with a model that draw nothing.

## Where it shows
- The log: one line per problem, once per load, with the Add-On's name.
- In game: an admin entering a game gets one chat line, for example
  "3 Add-On problems: Sniper Rifle, HE Grenade. Each is listed under its
  Add-On in Add-Ons." Client-local, nothing on the wire.
- The Add-Ons screen: each problem is listed under its Add-On, and the
  notice starts with the same summary.
- `<state>/logs/add-on-health.json` (schema 1) is written on every load,
  and `bri-client --check` prints the summary, so the Gate can read a
  packaged build's report against real copies.

Nothing runs per frame; the check runs once per content load.

## Tests
- `bri-package` `health::tests` (dedup, summary, bound, `check_set` with an
  off companion, an unknown dependency, an unfitted port and a failed asset).
- `bri-weapons` `references::tests` (an Add-On image's sound and emitter are
  listed, base entries are not; an unknown damage type is not had).
- `bri-client` `add_on_health::tests` (collection inside nested loads,
  rules problems attributed to their Add-On and file).
- Import merge tests now check the structured problems.

Commands: `cargo clippy -p bri-package -p bri-weapons -p bri-vehicles
-p bri-net -p bri-client -p bri-addon-import -p bri-sim -p bri-stresslab
--all-targets -- -D warnings`; `cargo test -p bri-package -p bri-weapons
-p bri-vehicles`; `cargo test -p bri-addon-import --test import`;
`cargo test -p bri-client --lib -- add_on_health weapon_debris cosmetic
add_ons`.

## Next (v0.1.12)
- Server side: rules-script failures (`op.failed`) during play reported to
  admins once each, and a dedicated server's own health report.
- Runtime misses (cue effects, sounds) folded into the same list.
- The Gate reading `add-on-health.json` from the packaged build each batch.
