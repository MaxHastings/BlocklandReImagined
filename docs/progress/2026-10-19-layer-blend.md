# LayerBlend material (2026-10-19)

`MaterialKind::LayerBlend` draws up to four diffuse layers (image slots 0..=3) weighted by the red, green, blue and alpha of one
weight map (slot 9, sampled at `lightmap_uv`). Each layer's `uv` goes through its own affine (`Material::layer_transforms`,
`[a, b, c, d, tx, ty]`). The sum is multiplied by twice the vertex colour, fogged, and drawn without other scene lighting. The
material uniform grew to 16 vec4 (matrices in 10..=13, translations in 14..=15).

Why: the pixel shaders a 2003-era D3D8 terrain path embeds are a weighted sum of up to four layers times `mul_x2` of the vertex
diffuse. This is that shape, named generally.

Probe: `crates/render/tests/layer_blend.rs` renders solid layers with known weights and checks the pixel against
weight x layer x 2 x vertex light (clamped), and that a layer's translation selects the other texel of a 2x1 strip. Played: nothing, headless only.
