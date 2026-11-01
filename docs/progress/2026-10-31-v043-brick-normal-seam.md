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
