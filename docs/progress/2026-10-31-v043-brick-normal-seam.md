# Optional normal maps for brick surfaces

The existing renderer has tangent-frame normal mapping for physical materials,
but ordinary brick overlays and batched brick surfaces did not accept it.
`Material::set_brick_normal_map` now binds a linear tangent-space image and a
bounded strength to those two material kinds. Both scene shaders use the
existing derivative-based tangent frame to alter the lighting normal; default
materials without a map keep their existing response. Scene validation rejects
an sRGB normal image. Normal maps do not change geometry or collision.

Offscreen `brick_normal_maps_modify_direct_light_without_changing_alpha` passes
for individual and batched bricks in both shader paths: the test's tilted map
increases light from an angled sun, leaves alpha unchanged, stays dark without
light, and rejects sRGB normal data. Existing ordinary-overlay darkness/local
light and paint-opacity tests also pass. `cargo fmt -p bri-render` and `cargo
clippy --locked --profile playtest -p bri-render --lib --test persistent_scene
-- -D warnings` pass. The first run correctly exposed that scene validation
disallowed all brick uniforms; validation was extended for the bounded detail
parameters, then the same test passed.

This is a renderer seam only: no stock texture is yet assigned a normal map,
and one batched material currently uses one detail image. Point-light behavior,
mirrored/rotated brick UVs, alpha/FX combinations, specular parameters, authored
material profiles, other world families, visual review and performance still
need coverage. It does not fulfill the complete material request by itself.

## Optional specular parameters

`Material::set_brick_specular` now gives these brick kinds bounded direct-light
highlight strength and exponent. It is opt-in: materials without it retain
their previous highlights, and explicitly zero strength removes the highlight.
Classic uses the sun; Unified and Dynamic also use their map-light path. This
does not add a specular term for Classic's separate vertex point-light path.
Glow FX keeps its own behavior.

The offscreen `brick_specular_parameters_control_highlights_without_emission`
test passes for both brick kinds in Classic, Unified and Dynamic: strength and
exponent change the sun highlight, darkness stays dark and opacity is retained.
The normal-map and ordinary overlay controls pass again; the input validation
case rejects invalid strength/exponent and unsupported kinds. Scoped formatting
and `cargo clippy --locked --profile playtest -p bri-render --lib --test
persistent_scene -- -D warnings` pass. The test does not verify point lamps,
shadowed/FX combinations or an authored stock material profile; no stock brick
has been assigned these parameters yet. Full material coverage remains open.

## Initial stock brick profile

The native brick-material loader now assigns a restrained direct-light
specular profile (strength 0.08, exponent 48) to its batched brick surfaces and
print overlays. It changes no diffuse image or collision. This is an initial
plastic-family profile, not a per-material visual calibration or proof that all
world textures are covered. Temporary placement ghosts retain their existing
flash parameters rather than inheriting the profile.

`cargo test --locked --profile playtest -p bri-client --lib materials::tests`
passed five cases, including the shared brick/print profile assertion; the ten
`world_scene::tests` and `cargo clippy --locked --profile playtest -p bri-client
--lib -- -D warnings` passed. The existing lighting probe was built and run
offscreen with the native Bedroom bundle and 16 synthetic bricks in Classic.
The overview image was inspected, but its bricks are too small for a meaningful
gloss assessment. The probe reported the already-known Bedroom fallback warnings
for an interior lightmap and foliage; these are not proof of a gloss regression.
Close-up comparisons across lighting modes and representative prints, local
lights, visual tuning and cost remain required; no interactive acceptance or
complete material coverage is claimed.
