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
