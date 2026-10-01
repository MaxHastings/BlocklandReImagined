# 2026-10-01 Bedroom bulb: no strip above the dresser after the break

Follow-up to the 2026-10-01 broken-bulb entry in `progress.md` (bake
formats 12 and 13). In both, the Gate's bulb13 crop still showed a faint
lighter strip at the top of the window wall after the break.

- Evidence: `bulb13-pixels-log.txt` (`BRI_PIXELS` on the spawn view,
  pixels 1177,132 to 1207,124). The strip is lightmap column 80 of
  `bedroom.dif/224/86`, the edge of light 9's patch. Column 79 is lit by
  light 9 (lightmap 169), column 81 is not (87), and column 80 holds about
  half of light 9 (rest 28..34). The rays see light 9 two columns further
  in, so column 80 relied on the "light its neighbours most likely hold"
  fallback. Column 79 picked light 6 for that: it is brighter there (83
  against 62), but no ray sees it, because it is behind the wall. So
  column 80's remainder was split over every hidden light, and lights 2,
  5, 6 and 8 kept 85% of it when the bulb broke: drawn [120..132] against
  [94] and [102] beside it. This is the bake's split, not the compiler's
  lightmap.
- Fix: a texel's held light is now chosen only among the hidden lights
  the rays see within two texels of it. Column 79 holds light 9, and
  column 80 gives its remainder to light 9. Bake format 14.
- Test: `a_patch_edge_two_texels_out_goes_dark_beside_a_brighter_hidden_light`
  (the patch-edge test with the compiler's shadow two texels past the rays'
  and a brighter light behind the wall). It fails on format 13: texel 20
  draws 50 levels with the patch light off. The patch-edge, shade, strip,
  thin-ray and slab-edge tests still pass. Command: `cargo test -p bri-render`.
