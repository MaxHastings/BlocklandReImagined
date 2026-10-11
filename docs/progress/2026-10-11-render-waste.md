# 2026-10-11 Render waste: avatars, still shadow layers, repaint fades

Thread "Avatar skinning and shadow redraw waste", branch
`claude/v0210-perf-render-7vgw3d`. This entry fixes findings 3, 8 and 9 of the
v0.2.10 performance waste audit (`perf/waste-audit-v0.2.10.md` in the project
files). Each fix is its own commit.

## What was wrong

- **Finding 3: avatars.** With shadows on (every Shadow Quality but off),
  `render_frame` skipped the frustum test for bodies, so every avatar was
  CPU-skinned (`build_pending`) and its vertices re-uploaded every frame,
  in view or not. Horses had no visibility test at all, built their mesh at
  every pose, and rebuilt a whole `Appearance` per frame to compare one
  colour. Every acting body also sampled its skeleton a second time
  (`unacted`), which only the local player's first-person image reads.
- **Finding 8: shadows.** The player's sun layers (casters, occluders, map,
  four cascades each at Best) were redrawn every frame. Texel-snapped
  cascade matrices repeat bit for bit with a still camera, so a still frame
  redrew identical depth. The same was true of the 24 lamp moving-caster
  tiles and of the skylight's moving layer (merge, nine draws and a
  9x512x512 depth copy). One kept lamp face was also redrawn round-robin
  every frame, because `kept_casters_key` could not see `hide_vertices`.
- **Finding 9: repaint fades.** A fading brick was rebuilt from its mesh
  (`rebuild_brick`) every frame of its roughly one-second ease. A paint fill
  of 512 bricks meant about 33k brick rebuilds.

## What changed

- `SceneRenderer::shadow_reach` returns every view's sun cascades, the
  casting lamps' spheres and the skylight projectors. A body or horse is
  built only when the player's frustum, a live mirror plane, a probe face or
  that reach can see it (`culling::Elsewhere`). Horses defer their mesh like
  bodies and compare their chest colour. `wants_unacted` is set for the view
  owner only. A new span, "avatar meshes", times the loop.
  `entity_counts` reports `avatar_meshes_built` and `horse_meshes_built`.
- Each whole sun layer of the player's view remembers a signature of what it
  was drawn from: its matrix, the caster, occluder or map geometry and
  instances (`casters_signature`), and any kept-layer redraw. The layer is
  skipped while the signature matches. A plane fitting its own cascades
  writes the same layers, so it clears what is known. The lamp moving tiles
  are skipped as a set, and the skylight's moving layer is skipped too,
  under the same rule. Scene revisions start unique per scene
  (`fresh_revision`) and count `hide_vertices`, `update_vertices` and
  `patch_images`. That lets `kept_casters_key` see dead bricks, and the
  round-robin refresh is gone. `RenderStats::shadow_layers_kept` counts what
  was skipped.
- A repaint at the same alpha now rewrites only the painted vertices'
  colour. Which vertices take the paint is learned once, by building the
  brick a second time in another colour of the same alpha and comparing the
  two builds. Alpha fades and any other brick change still rebuild.
  `FadeDiagnostics::recolors` counts the rewrites.
- `large_build_perf`: a new `BRI_PERF_BOTS=<n>` option places Blockhead Bot
  spawn bricks over the build. It turns the Add-On on in a linked copy of the
  content root's two Add-On lists only. Each view now also reports the
  average main-thread ms of every span.

## Correctness checks

- `still_frames_keep_their_shadow_layers` (bri-render, lavapipe): still
  frames record zero shadow draws and are pixel-identical to the first. A
  moved player and a tower hidden in place are each drawn the same frame,
  pixel-identical to a fresh renderer.
- All other bri-render GPU tests pass on lavapipe. These cover the kept
  brick layers, occluders, unified lamp updates, mirrors and lighting.
- `repaints_rewrite_colours_without_rebuilding`: every frame of a repaint
  equals a fresh build, with one upload per brick and recolours after that.
- `bodies_out_of_view_and_shadow_reach_are_not_built`: a body 1000 units
  behind the camera is not built. One behind but inside a cascade is built,
  and so is one inside a lamp's reach.

## Not done

- Finding 9's second half, the two whole-chunk rebuilds per fade (at fade
  start and at settle), is not fixed. It needs the chunk job to defer a
  painted brick's change and hide it in place instead. That is a structural
  change to change tracking, left for after the measurements.
- Mirror planes still read the player's lamp and skylight reach; a body
  only casting into a plane's own cascades is covered by those cascades.

## Measurements

Pending: `large_build_perf` before (main `cf9c761` with this benchmark file)
and after, on Max's PC. These runs are queued behind the release thread's.
