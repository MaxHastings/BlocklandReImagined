> **Historical note. Written 2026-10-08 (the file's date in the cloud project folder, `v0.2.6/review-fog-sky.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: fog and sky (claude/fog-sky-floor-jjyotf at 4571b9c, on stack-plain)

**Verdict: it is correct, the change is contained, and it is safe for v0.2.6. There are no code must-fixes.** Max has to see two visual tradeoffs before it ships, both in the last section. The bigger one is that the Oct 1 pale cut-outs come back in a milder form, and only for tall things. The cause is fog height, not fog thickness.

## What was run here (cloud)
- `cargo test --release -p bri-render --test shader_validation` gave 2 passed: every WGSL module, with the fog prepended, still parses and validates.
- `persistent_scene` and `gpu_contract` couldn't run here because this container has no Vulkan driver ("No suitable graphics adapter found"). That is the container, not the code. I relied on the lane's lavapipe runs for those.
- No CPU code depends on the old behaviour: `Fog::amount` / `sky_amount` / `edge` are used only by render tests (grep across `crates`). No gameplay or bot sight reads fog.

## The model
- **Sky:** `1 - exp(-FOG_DEPTH * exp(-rise * end))`. That is the fog density at the world's edge, applied as the full fog depth. At the horizon it equals level geometry at the visible distance (98%), and it is clear a few `FOG_HEIGHT` up. The CPU twin and the WGSL match line for line.
- **Geometry:** fog is unchanged (`fog_along`). The old `max(fog, edge * sky_haze)` is replaced by a fade-out over the last quarter of the range: dithered discard for opaque surfaces, alpha for particles.
- No new constants. 0.75 / 0.25 and the smoothstep are the existing edge band, moved. The interleaved gradient noise constants are the published Jimenez 2014 ones.

## Every world shader against the fade and the cutoff

| Shader | Draws | Fog | Fades out | Notes |
|---|---|---|---|---|
| scene.wgsl | bricks, terrain, interiors, players, vehicles, water (material 6) | ✅ | ✅ everything except sky faces, clouds and the fog backdrop (materials 4/5) | The sky is drawn first and never writes depth (scene.rs:209, order at ~4250), so a discarded pixel shows the sky. The shader already discarded (`v.clip<0`), so early-Z cost doesn't change. Water out to the horizon dithers into the fog backdrop, which it already nearly matches (≥94% fog there). |
| mirror.wgsl | mirrors | ✅ | ✅ | New discard, uses its own frame's eye. |
| particles.wgsl | all fx sprites | ✅ | ✅ by alpha, all three blend modes | Additive-colour (mode ≥1.5) multiplies by `kept`; the alpha modes carry it in `a`. |
| foliage.wgsl | grass, plants | own linear fade to `fog_end` (existing) | ✅ its own | Unchanged; it ends at the visible distance. |
| weather.wgsl | rain, snow | none | none | Drawn near the eye; nothing reaches the far quarter. Unchanged. |
| lines.wgsl, world_shapes.wgsl | non-rendering brick outlines, Add-On boxes | none, on purpose | none | Overlays, deliberately unfogged since Oct 1, and not new. A duplicator box far out stays drawn. Fine. |
| shadow.wgsl | shadow depth | — | — | A faded caster still casts, but only onto things just as far, which fade too. Unchanged. |
| textured.wgsl, brick_preview.wgsl, vignette, output | preview tool, UI preview, screen-space | — | — | Not world geometry. |

Nothing drawn in the world misses the fade except by design.

## Does it bring back the Oct 1 pale cut-outs? Partly, for tall things, not because of thick fog.
- A cut-out is geometry paler than the sky directly behind it.
  - **Before** this change, the sky's fog was integrated to infinity, so it was always at least the fog of any geometry in front of it. Cut-outs were impossible, and that is also why the sky was washed out.
  - **Now** the sky takes the density at the edge. Along any ray, geometry's fog is the *average* density out to that point, which is always more than the density at the edge. So **far geometry is always a bit paler than the sky behind it.**
- The gap depends only on how high the geometry is above (or below) the eye where the fade starts. It does not depend on the fog distance. Computed from the shipped formulas, at 0.75 of the range:

| Height above the eye | Geometry fog | Sky behind it |
|---|---|---|
| 10 | 0.94 | 0.96 (no cut-out) |
| 30 | 0.91 | 0.87 |
| 60 | 0.85 | 0.65 |
| 120 | 0.73 | 0.24 |

- So level ground, low buildings and the horizon are seamless, and the hard edge where the world ends is gone (the dither). A mountain or tower rising 60+ units above the eye near the far range reads as a pale shape against a clearer sky. That is milder than v20, whose distance-only fog made them ~94% fog-coloured against blue, and milder than v0.1.11. It applies under every fog setting, Thick Fog included, wherever terrain rises that high within its range. Skylands' islands far below the eye do the same against the floor.
- **This is the price of a clear sky with height fog. It isn't a bug in this change.** The one way to have both is to fog geometry toward the *sky's own colour in its direction* (aerial perspective toward the sky), not toward a flat fog colour. That needs the sky's colour per direction in scene.wgsl, which is a v0.2.7 change, not a v0.2.6 one.

## Safe for v0.2.6
- It is limited to rendering, it removes code (`sky_haze`), and it adds no pass or pipeline. It fixes exactly what Max reported.
- **For Max's playtest, two things to look at** (both inferred from the code; nobody has seen them in game):
  1. **The far quarter stipples.** The dither has no temporal smoothing, and the noise is fixed to the screen, so it crawls as you turn. On big terrain maps that can be a large part of the screen. Check the Slate horizon and a tall-terrain map while walking and turning.
  2. **Tall far terrain looks pale** against the now-clear sky, as above. Check a map with high mountains under default fog and under Thick Fog, and Skylands looking down.
- If either looks wrong to Max, the fallback is aerial perspective toward the sky colour (v0.2.7), not putting the washed-out sky back.

## v0.2.7
- Aerial perspective toward the sky's colour, which removes the cut-out gap at any height.
- Temporal smoothing for the edge fade, or a sorted blend, if the stipple bothers Max.
