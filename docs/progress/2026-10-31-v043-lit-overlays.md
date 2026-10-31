# Ordinary overlays respond to lighting

The world shaders previously reproduced v20's fixed-function decal ordering:
they mixed the overlay into the already-lit paint. With no ambient or sunlight,
a half-coverage blue overlay still produced the offscreen pixel `[16, 64, 112,
255]`. This is the full-bright defect reported for ordinary brick and character
decals. Compose the overlay's display-space pigment with the paint before
applying illumination instead. Explicit `UnlitOverlay` remains unlit. Both the
original and modern shader variants use the same ordering.

Validation on the warm Windows worktree:

- `cargo fmt -p bri-render`
- `cargo test --locked --profile playtest -p bri-render --test persistent_scene ordinary_overlay_receives_darkness_and_local_light_but_unlit_stays_bright -- --nocapture` passed. It exercises original and modern paths, individual and batched brick surface materials, half and full overlay coverage, darkness, a local light and the explicit unlit control. The original-path dark assertion failed before the fix.
- `cargo test --locked --profile playtest -p bri-render --test persistent_scene brick_overlay_coverage_preserves_paint_opacity_and_display_space -- --nocapture` passed; the existing unity-light paint/coverage controls remain intact.
- `cargo clippy --locked --profile playtest -p bri-render --lib --test persistent_scene -- -D warnings` passed.

These are offscreen material checks, not a full visual or source-reference
comparison. Other translucent material families, projected surfaces, glow
effects, shadow combinations and the BadDream fixture still need their own
investigation and evidence before the lighting release requirement is complete.
