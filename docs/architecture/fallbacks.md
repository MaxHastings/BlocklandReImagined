# Counted fallbacks

A loader that meets bad or missing input and carries on with something else
(a stand-in texture, a default value, an item left out) must say so.
`bri_console::fallback::report(kind, addon, item, fallback)` records it,
counted once per distinct (kind, Add-On, item, fallback); the first of each is
a console warning. Every Add-On health problem (`bri_package::health`) is also
reported here, so one list covers both.

`bri-client --check` loads every listed map's visual bundle and native
collision, then writes the list to `logs/fallbacks.json`:

```json
{"schema": 1, "overflow": 0, "fallbacks": [
  {"kind": "missing_texture", "addon": "<package id or map id>",
   "item": "<asset or field>", "fallback": "<what was used instead>", "count": 3}
]}
```

The check prints `Fallbacks: N (path)` and exits normally; the push gate and
the packager read the file and refuse any entry that is not on the stock
Add-On allow list the integrator keeps in `tools/`.

## What counts, and what does not
- **Counts:** input that was present and unusable (a colour that does not
  parse, a texture that is not there), or required and missing, where the
  game carried on anyway; an object, node, element or layer left out.
- **Does not count:** an absent optional key with a documented default.
- **Not for hot loops:** report at load, or once per rare event. A report
  takes a lock and four small strings.

Kinds: `missing_texture`, `sound`, `missing_model`,
`missing_reference`, `dropped_particle_group`, `unknown_role`,
`defaulted_field`, `ignored_key`, `dropped_item`, `other`
(`bri_console::fallback::kind`).

## Sites covered so far
Add-On health (all kinds), scene loader (malformed sun, ambient, sky and
water colours and angles; retained-but-undrawn nodes; movers without an asset;
unbound reflection/detail/bump effects, including soft-cutout materials that
used to skip the note; lightmaps replaced by the embedded one), native map
(no surface table, materials without a surface kind, colliders that lost
their triangle kinds, nodes without collision), map content (a missing
tutorial or environment pack, with the real error), effects (particle
textures replaced by the cloud, dropped groups with no contact, every
`WeaponEffects`/`ActorEffects`/`WorldItems` "missing" counter), animation
(clips an archetype maps that the body does not have, including optional
roles), bodies and HUD (packages or manifests that cannot be read, layouts
skipped, textures past the HUD limit).

## Unknown keys
`bri_content::strict::from_slice` / `from_value` read JSON into a struct and
report every key the struct did not read (kind `ignored_key`, item
`<Struct>: <path>`). The loader names whose content it is once with
`bri_console::fallback::with_owner(map_or_addon_id, || ...)`; reports that
name no Add-On take it. Used for the map's scene, interiors, shapes and
terrain (`NativeMap::load`, the scene loader) and the weapons pack.

## Not yet covered (audit follow-ups)
- `strict` is not yet used for effects libraries, bricks, the avatar pack and
  the Add-On manifests (`bri_package::library`), nor `fx-import` datablocks.
- Per-frame HUD binding failures, and the muzzle/grip warnings in
  `bri_sim::session::grips` and `bri_weapons::muzzle`.
