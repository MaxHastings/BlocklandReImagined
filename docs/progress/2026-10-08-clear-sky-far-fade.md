# 2026-10-08 Clear sky and floor, far things fade into the sky

Max's report: the fog hides the sky and Skylands' floor; original Slate and
Skylands showed both clearly. He suspected the fog was thickened to hide far
bricks popping out.

Cause: the 2026-10-01 change (`2026-10-01-sky-fog.md`, v0.1.12 "New fog,
sky and horizon") answered pale cut-outs of fogged buildings against a clear
sky by fogging the sky the same way as the world: an exponential fog
thinning 60 units per e-fold above the eye, integrated to infinity. Its
density comes from the fog range, so with fog from 0 to 500 the sky was
about 38% fog straight up, 62% at 30 degrees and 94% at 10 degrees, and the
mirrored floor the same below. In v20 the same 60 units were only a fog band
at the horizon of a sky drawn at the visible distance, a few degrees tall.

Now the two jobs are separate (one definition, `fog.wgsl`, CPU twin
`bri_content::environment::Fog`; no new constants):

1. The sky is the backdrop at the world's edge and takes the fog of the air
   there: `1 - exp(-FOG_DEPTH * exp(-height_at_edge / FOG_HEIGHT))`. Its
   horizon is exactly the fog of level geometry at the visible distance;
   above an edge a few `FOG_HEIGHT` up it is clear. A near edge (thick fog)
   covers more of the sky, as the v20 band did.
2. Geometry no longer fogs toward the sky's colour at the edge. Over the
   last quarter of the fog range it fades out into whatever is behind it,
   and past the visible distance it is not drawn: opaque surfaces
   (`scene.wgsl`, mirrors) leave that share of their pixels in interleaved
   gradient noise (`faded_out`), particles lose that share of their alpha.
   A far brick becomes the sky pixel behind it, so a clear sky can't cut
   it out. Sky faces, clouds and the fog backdrop are never faded.

Fog on the world itself (fog distance, visible distance, colour, the height
falloff) is unchanged. Foliage keeps its own fade. Shadows are unchanged.

Tradeoff: the fade is a screen-door dither with no temporal smoothing, so
the farthest quarter of the range shows a fine stipple on things fading out,
instead of a blend. Blending would need every opaque pipeline sorted and
blended; the dither needs neither.

## Evidence

Lavapipe (Mesa software Vulkan) in a cloud container:

- `cargo test -p bri-render --test persistent_scene`: 14 pass, 3 ignored
  (need v20 content). `sky_fades_into_the_world_fog_at_the_horizon` now
  checks the horizon equals level fog at the edge, the sky overhead and the
  floor straight down are under 1% fog, and thick fog still covers most of
  the sky. The fog test adds a triangle past the visible distance: the sky
  shows through it.
- `cargo test -p bri-fx-runtime --test gpu_contract`: pass. Fogged sprite
  cases now sit inside the range (87% fog, values from the formula) and new
  cases past it show the background.
- `cargo test -p bri-render --test shader_validation`, `-p bri-content`: pass.

Not checked: in-game look on real maps (Max's playtest).
