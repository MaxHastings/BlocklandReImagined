# 2026-10-12 Fractional UI scales draw pixel art evenly

Maxwell reported that UI Size 100%, 200% and 300% look right but 125%, 150%,
250% and any value between look bad.

Cause: glyphs from the Torque font caches and 1:1 pixel art (`Filter::Nearest`)
were sampled nearest-neighbour onto quads at fractional physical positions. At
1.25x or 1.5x a one-texel stroke rasterised as one or two pixels depending on
where it fell, so text and button pieces looked ragged. Whole scales are
unaffected because every texel lands on a whole number of pixels.

Fix (`crates/ui/src/gpu.rs`): Nearest quads are now sampled "sharp" in the
fragment shader. Inside a texel the sample stays on its centre; only the pixel
a texel edge crosses blends the two texels, by the share of the pixel each
covers. The blend is premultiplied (`textureLoad` of the four texels), so
transparent texels do not darken glyph edges. At whole scales the result is
exactly nearest sampling. Below 1x it falls back to bilinear. Linear art and
fills are unchanged; a 1-logical-pixel fill line still rounds to one or two
pixels at fractional scales, which reads as a consistent line per edge.

Evidence: new GPU test `gpu::tests::nearest_art_keeps_even_strokes_at_fractional_scales`
renders an alternating 8-texel stripe at 1, 1.25, 1.5, 2, 2.5 and 3x and checks
each pixel against its exact coverage (within 3/255), and that whole scales
stay pure 0/255. With the shader path disabled it fails at 1.25x pixel 1 (got 0,
want 64). Ran on lavapipe in the cloud container:
`cargo test -p bri-ui --lib gpu::tests`, `cargo clippy -p bri-ui --all-targets -- -D warnings`.

Next: Maxwell to look at the menus at 125%, 150% and 250% in the packaged build.
