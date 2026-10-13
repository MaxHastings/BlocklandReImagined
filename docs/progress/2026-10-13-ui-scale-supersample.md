# 2026-10-13 Fractional UI scales draw at the next whole scale and shrink

Maxwell reported that after the 2026-10-12 fix (sharp sampling of Nearest art)
UI Size 125% and 150% still looked a little off.

Cause: that fix only covered `Filter::Nearest` quads. Solid fills (window and
field borders, list rules, one-logical-pixel frames) still rasterised at
fractional edges, so a one-pixel line came out one or two screen pixels thick
depending on where it fell. Bilinear (`Filter::Linear`) art, the stretched
bitmaps and nine-slice pieces, blurred differently on each side for the same
reason. 200% and 300% have none of this because every logical pixel lands on
whole screen pixels.

Fix (`crates/ui/src/gpu.rs`): at a fractional scale above 1, `UiRenderer::render`
draws the UI at the next whole scale (200% for 125% and 150%, 300% for 250%)
into an offscreen `Rgba8Unorm` texture cleared to transparent, then shrinks it
onto the target in one pass with exact area weights (at most 3x3 texels per
pixel, since the ratio is between 1 and 2), blended premultiplied. This is how
macOS draws its scaled resolutions. Each logical pixel then covers exactly 1.25
or 1.5 screen pixels everywhere, and the picture is the 200% UI made smaller.
Whole scales and scales below 1 draw straight, as before. The public API is
unchanged.

Evidence: new GPU test `gpu::tests::fractional_scale_lines_cover_their_exact_share`
draws one-logical-pixel fill lines at 1.25, 1.5 and 2.5 and checks every pixel
against its exact coverage, plus a translucent fill blending over the
background. With the offscreen path disabled it fails (1.25x pixel 1: got 0,
want 64). The earlier glyph stripe test still passes through the new path.
`cargo test -p bri-ui` and `cargo clippy -p bri-ui --all-targets -D warnings`
pass on lavapipe in the cloud container. `bri-client` could not be checked
there (no ALSA); its calls are unchanged.

Next: Maxwell to look at 125% and 150% in the v0.3.1 build.
