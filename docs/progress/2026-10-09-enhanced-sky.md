# 2026-10-09 Enhanced sky (opt-in, Options > Graphics > Sky)

## What changed
- The host chooses the sky: `atmosphere::Settings::enhanced_sky` (`Option<bool>`,
  unset = the map's own sky), set in Admin > Environment > Advanced "Enhanced
  Sky" or by `set_environment(#{ enhanced_sky: true })`. It rides the existing
  environment sync (one protocol-changes file) and is read back by
  `environment()`. `Live::enhanced_sky` carries it to the renderer.
- Players keep one local option, Options > Graphics > Sky: Server's (default)
  or Original (`$pref::Video::Sky` = 1). It can only turn the generated sky
  off. Not part of the Quality presets.
- `Live` also gains `sun_toward`: the unit vector toward the real sun, kept
  when `sun_direction` turns to the moon at night. The camera uniform gains
  `sky_sun` (xyz that vector, w 1 while the generated sky is on), appended
  after `sky_bands`.
- `scene.wgsl` `enhanced_sky()`: closed-form single scattering (Rayleigh +
  ozone absorption + Mie, Kasten-Young air mass) per ray, with a twilight
  afterglow term and night floor, encoded as display colour like the rest of
  the pipeline. No lookup tables, no extra passes: it reuses the
  camera-relative sky cube.
- Fog: `fog_target()` and the sky/cloud fog edge use the same procedural sky
  and its horizon colour when on, so far terrain still fades into what is
  drawn behind it (the October 1 silhouette fix). Off keeps the authored
  `sky_bands`. `fogged()` returns early where fog is 0. There is no map
  special case: a host that wants Skylands' own sky leaves the setting off.
- The host's `sky_color` tints the generated sky too (multiplied in display
  colour, like the texture sky; the day's own dimming is not applied twice).
- Sparse steady stars fade in with the night.
- Sun and moon discs are drawn by the shader; clouds, where a map has them,
  keep their textures and fog toward the procedural horizon.

## Not done
- No change to ambient_at(), AO or the HDR pipeline (the brick-shading thread
  owns those). Metal reflections still sample their own gradient sky.

## Evidence
- `cargo test -p bri-render --test shader_validation`, `-p bri-content atmosphere`
  (real sun above and under the horizon, moon flip), `-p bri-ui options`
  (Sky menu defaults to Original, saves, not part of Quality presets).
- `cargo test -p bri-render --test lighting_environment sky` on a software (lavapipe) GPU:
  Original unchanged by the sun; Enhanced blue noon, red sunset horizon, dark
  navy night; a far wall in fog leaves no silhouette against the sky.
  `BRI_SKY_DUMP=<dir>` writes frames; see project file `sky/enhanced-sky-day.png`.
- `cargo clippy --workspace --all-targets -D warnings` clean.
- Not run here: real maps (no v20 content in the cloud), GPU timing.

## Next
- Look at the church (midday, golden hour, sunset, twilight, night), Skylands
  and an indoor map on the PC, and compare Classic before/after; time the sky.
