# 2026-10-01 Unified and Unified+Shine are one mode

Max: "Unified and Unified + Shine should just be combined. they are
basically the same." The only difference was highlights on bricks,
players, items and vehicles. The map's own surfaces were the same in both.

- Options' Lighting menu is now Classic, Unified and Dynamic. Unified is
  the default and keeps Shine's look (highlights). `$pref::Video::Lighting`
  keeps its values (0 Classic, 2 Unified, 3 Dynamic). A saved 1 (the old
  Unified) reads as 2, so both old choices open as Unified, and Dynamic's
  3 is unchanged.
- The duplicate path is gone: `scene.wgsl` no longer gates highlights on
  mode 2, and `map_light_sum` lost its `specular` flag (both callers always
  wanted it). The default mode draws exactly as before, so there is no
  frame-time change. `lighting_probe` renders Classic and Unified.
- Guard: `bri-ui` `saved_unified_and_unified_shine_both_open_as_unified`
  fails on main 91ffd7e2f (a saved 1 reads as 1). The lighting menu test
  checks the three items. In `bri-client`, `graphics` maps a saved 1 to 2.
  The render tests that drew mode 1 now draw 2. One expectation
  (`map_walls_shade_objects_from_the_sun_with_a_filtered_edge`) now
  includes the sun's highlight, since the sun and the eye are straight
  above. Commands: `cargo test -p bri-ui --lib`,
  `cargo test -p bri-render --no-fail-fast`,
  `cargo test -p bri-client --lib graphics`, and clippy `-D warnings` on
  ui, render and client (all targets). All pass.
