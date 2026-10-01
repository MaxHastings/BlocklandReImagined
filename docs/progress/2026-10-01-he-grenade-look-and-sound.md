# 2026-10-01 HE Grenade look, fireball and explosion sound

Max tested the bundled HE Grenade in the test build of main b5d99c948:
throwing works, but the grenade drew as a flat green blob, the explosion
looked off and it made no sound. All three were engine gaps any Add-On could
hit; none is fixed by special-casing the grenade. The original's files were
read from Max's PC (private project folder only, never the repo); the v20
exe was disassembled here with capstone (sha256 d9e5a593…77ac).

## Silent explosion: a sound naming the game's own file

`hegrenadeExplosionSound` is `filename = "base/data/sound/vehicleExplosion.wav"`,
a file of the game, not the Add-On. The importer only kept `AudioProfile`s
whose file it converted, so the explosion named an unknown profile and
played nothing.

- `bri_weapons::SoundDef::stock`: the file is the installed game's, by its
  v20 path. The importer sets it when the profile's file is not the Add-On's
  but the installed game has it (`Reference::stock_sound`; installed audio
  clips now list their source paths). Volume and looping still come from the
  Add-On's `AudioDescription`.
- `SoundBank::clip_at` finds a loaded clip by v20 path; the client plays a
  stock sound from that clip (`SoundAsset::world`). A file the game lacks is a
  warning and a silent sound, as before.
- Content identity skips stock sound files (the audio pack covers them);
  merged packs give them no package folder.

## Flat model: plain textures were drawn unlit

The grenade's textures (`green`, `gray50`, `gray75`) are 16x16 RGB with no
alpha. Every opaque item material was drawn as an overlay (the brick
`GL_DECAL` rule), where a fully opaque texel replaces the lit colour, so the
faceted model came out one flat colour. An offscreen render (lavapipe) of the
converted projectile showed exactly Max's flat blob before, and lit facets
after.

What v20 does, from the exe:

- The texture manager flags a texture as translucent when any texel it
  samples (every 16th along each axis, both walks bounded by the width) has
  alpha under 255 (0x509d79-0x509dc2, `TextureObject+0x34`).
- `TSMesh` with a colour shift: a translucent texture goes `GL_DECAL` over the
  lit shift colour (0x63ac1a); any other `GL_MODULATE` under a white colour
  (0x63ac97), i.e. texture times light, the shift colour unused. Without a
  colour shift the default `GL_MODULATE` (0x6362c1) stays.

Now `native_shape_scene` keeps the overlay only for a texture v20 counts as
translucent (`items::translucent_texel`); an opaque material with any other
texture is `VertexLit` with the new `Material::untinted` (shader flag 4:
texture times light, the vertex colour's RGB unused). Stock textures with
alpha (`blank` alpha 0, `black25` alpha 64) keep their overlay. Item icons
drawn on the CPU honour `untinted` too.

## Fireball: a missing explosion shape shows the rocket's sphere

`hegrenadeExplosion.explosionShape` is `Add-Ons/Weapon_Rocket Launcher/explosionSphere1.dts`
(a space; the game's folder is `Weapon_Rocket_Launcher`). 9c63917b made a
missing shape mean no shape, on the inference that Torque found nothing
there. The exe says otherwise: `ExplosionData::preload` (0x52c520-0x52c5e4)
loads `add-ons/Weapon_Rocket_Launcher/explosionsphere1.dts` instead whenever
the named shape does not load and `$Pref::Net::DownloadExplosions` is off,
which is its stock default (`docs/research/ui-ux/data/client-defaults-stock-vs-installed.tsv`).
So the original HE Grenade showed the rocket's expanding sphere.

`ExplosionShapes::load` now does the same: a shape no package provides, or an
Add-On's that fails to load, draws `explosion_shapes::MISSING_SHAPE` when the
game has it, with the explosion's own lifetime and scale. The import report's
note for such a path says so. The rest of the explosion (three emitters, the
smoke burst, camera shake, no light: its light radii are 0) was already
converted.

The exe has the same download-off fallbacks for item and projectile shapes,
by name keywords (`grenade`, `rocket`, `hegrenade.dts` and others, 0x5c13c9,
0x5b6a7c). They only apply to a shape the client could not load; imported
Add-Ons always carry theirs, so nothing needs them yet.

## Guard tests (each fails on b5d99c948)

- `bri_client::audio::tests::an_add_on_sound_naming_the_games_own_file_plays_the_banks_copy`
- `bri_addon_import` `installed::a_sound_naming_the_games_own_file_plays_that_file`
- `bri_net::content_identity::tests::test_weapons_pack_identity_and_every_choice`
  (a stock sound in the pack; fails without the identity skip)
- `bri_client::items::texture_rule_tests::only_a_texture_with_a_translucent_texel_is_laid_over_the_colour`
- `bri_client::explosion_shapes::tests::a_shape_no_package_provides_shows_the_rockets_sphere`

## Next

Max re-tests the HE Grenade in the next test build: a lit green pineapple
grenade, the rocket's sphere on the blast, and the vehicle explosion sound.
Other bundled originals with plain RGB textures now draw lit as well.
