# 2026-10-09 Preview stabilization

Max authorized a small stabilization pass after receiving the first preview:
fix Options clipping; preserve mirror/portal images from main-camera AO;
check a mixed scene offscreen; ship one updated preview. He requested short,
consistent existing GUI labels and no verbose explanations in the panel.
Branch codex/sky-shading-preview, draft PR #35. Main remains untouched;
Gravity Gun integration remains excluded. Generated clouds, broader lighting
and per-secondary-view AO remain deferred for this preview.

Changes:
- Graphics menu rows have a two-pixel gap rather than six, fitting Render
  Scale above Done. Ambient Occlusion's checkbox is wide enough for its text.
  Existing groups, labels, style and behavior are kept.
- The split world pass records its after_opaque compositor after AO and before
  blended world geometry. With AO off the original ordering is unchanged.
  Mirrors/portals show their own sky/shading/geometry image without a
  main-camera AO overlay; they still show through transparent geometry.
- Regression check exercises the actual Reflections compositor for reflected
  and through-portal views with MSAA 1/4, checking 900 interior pixels per
  case while proving AO changes the surrounding opaque world.
- Bounded mixed-scene check uses the native posed Blockhead and instanced
  Jeep, procedural Glow/glass/ordinary blocks and the real particle renderer.
  It renders off / Soft Shading / Soft Shading+AO, in Unified/Dynamic at
  MSAA 1/4, with native and synthetic fixtures. Glow and particle samples must
  remain unchanged when AO is added, and ordinary geometry must be shaded.

Evidence on Max's PC, with no visible game or interactive input:
- Both previously failing Options layout checks pass: UI unit
  apply_and_render_scale_fit_display_settings (1) and authored_buttons
  options_tabs_fit_short_and_wide_windows (content+synthetic, 2).
- native_graphics_panel_offscreen passes. Actual authored Graphics panel
  PNGs at 800x450 and 1280x720 visually inspected: labels fit, all eight menus
  including Render Scale sit above Done, no explanatory text was added.
- main_camera_ao_preserves_mirror_and_portal_images passed (51.92 seconds).
- sky_shading_scene passed both native and synthetic cases (28.18 seconds).
  Native montage inspected: character, Jeep, transparent pane, green Glow,
  magenta particle; ordinary faces/contacts shade while emissive colours stay.
- Focused suites passed: lighting_environment 12; mirrors 12 (one existing
  benchmark ignored by this non-benchmark command); shader_validation 3.
- `cargo clippy -p bri-client -p bri-render -p bri-ui --all-targets --locked -- -D warnings` passed.
- The earlier full gate's two Options failures are fixed and rechecked; a
  full gate rerun is not claimed by these focused checks. No test or watchdog
  was disabled, and no new known-failure exception was introduced.

Runtime and verification source a4318d758:
https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37979972281
Release dispatched with version preview-2026-10-09-sky-shading-r2 and
publish=false. Artifact only, no GitHub Release/tag. First preview run
37974800673 has now succeeded on Windows, Mac and Linux.

Local evidence: artifacts/ui-native-graphics/ and artifacts/sky-shading-mixed/;
these original-content renders are ignored and must never be committed.
No new worktree or shared Cargo target was created. Original installation
remains read-only. Earlier strict 4x MSAA exterior parity finding is still open;
all interactive visual acceptance and frame-rate judgment remain Max's.
