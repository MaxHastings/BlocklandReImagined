# 2026-11-01 Dynamic Map PCSS

Dynamic player shadows already used `pcss_lit`, but map/interior shadows in
the active `scene_original.wgsl` shader always used four fixed comparison taps.
Route `map_cascade_lit` through blocker-search PCSS when Soft Sun Shadows is
enabled. Map PCSS uses a core blocker sample, a weighted search across the full
filter radius, and 32 comparison taps. The filter is wired in both scene
shaders and in the far-map fallback; PCSS cascades blend before their filter
footprint reaches the map edge. Player/object PCSS remains at its existing
8-search/16-filter settings.

The Dynamic-mode offscreen regression covers Bedroom-scale cascade and far-map
window shadows. It checks both penumbra width and reduction of the largest
adjacent-pixel intensity step. Maxwell reported the visual issue fixed after
rebuilding the client with this implementation.

Validation:

- `cargo test -p bri-render --test unified_lighting dynamic_window_shadow_softens_when_pcss_is_enabled -- --nocapture`
- `cargo test -p bri-render --test pcss_sun_shadows --test unified_lighting`
- `cargo test -p bri-render --test shadow_occluders cascade_splits_do_not_cut_a_shadow -- --nocapture`
- `cargo fmt --all -- --check`
- `cargo clippy -p bri-render --tests -- -D warnings`
- `git diff --check`