# 2026-10-09 Enhanced sky (opt-in, Options > Graphics > Sky)

## What changed
- New local setting `$pref::Video::Sky`: 0 Original (default; the map's own
  sky textures, pixel-identical to before) or 1 Enhanced. Independent of
  Lighting and of the Quality presets (`ui` options screen, `Graphics::enhanced_sky`).
- `atmosphere::Live` gains `sun_toward`: the unit vector toward the real
  sun, kept when `sun_direction` turns to the moon at night. The camera
  uniform gains `sky_sun` (xyz that vector, w 1 while Enhanced is on),
  appended after `sky_bands`.
- `scene.wgsl` `enhanced_sky()`: closed-form single scattering (Rayleigh +
  ozone absorption + Mie, Kasten-Young air mass) per ray, with a twilight
  afterglow term and night floor, tone-mapped and encoded as display colour
  like the rest of the pipeline. No lookup tables and no extra passes: it
  reuses the camera-relative sky cube, so cost is a few exp/pow per sky pixel.
- Fog: `fog_target()` and the sky/cloud fog edge use the same procedural sky
  and its horizon colour when Enhanced is on, so far terrain still fades into
  what is drawn behind it (the October 1 silhouette fix). Original keeps the
  authored `sky_bands`. `fogged()` now returns early where fog is 0.
- Maps with a bottom sky face (Skylands) keep their authored sky (Enhanced is off for them); the
  shader mirrors below the horizon only for tests. Sun and moon discs are drawn by the
  shader; clouds, where a map has them, keep their textures and fog toward
  the procedural horizon.

## Not done
- No change to ambient_at(), AO or the HDR pipeline (the brick-shading thread
  owns those). Metal reflections still sample their own gradient sky.
- Admin `sky_color` tint does not apply to Enhanced (it multiplied the texture).
- No stars.

## Evidence
- `cargo test -p bri-render --test shader_validation`, `-p bri-content atmosphere`
  (real sun above and under the horizon, moon flip), `-p bri-ui options`
  (Sky menu defaults to Original, saves, not part of Quality presets).
- `cargo test -p bri-render --test enhanced_sky` on a software (lavapipe) GPU:
  Original unchanged by the sun; Enhanced blue noon, red sunset horizon, dark
  navy night; a far wall in fog leaves no silhouette against the sky.
  `BRI_SKY_DUMP=<dir>` writes frames; see project file `sky/enhanced-sky-day.png`.
- `cargo clippy --workspace --all-targets -D warnings` clean.
- Not run here: real maps (no v20 content in the cloud), GPU timing.

## Next
- Look at the church (midday, golden hour, sunset, twilight, night), Skylands
  and an indoor map on the PC, and compare Classic before/after; time the sky.
