# 2026-10-18 Environment lens flare

A generic seam from the Halo lane (`codex/halo-sun-flare`), for Blood Gulch's
sun. Inert for an environment without one.

Before: a map's sun was a glow in the sky shader. A converted map with a
multi-picture lens flare (Halo's `lens` tag) had nowhere to put it.

After: an `Environment` may carry `lens_flare` (`bri_content::environment`): a
cutoff angle in degrees and 1 to 32 elements, each a picture (hash-named PNG,
validated like the sky faces), a position on the axis from the sun through the
screen's centre (0 the sun, 0.5 the centre, 1 mirrored), a radius in half
screen heights, a brightness and a tint.

How: `bri_render::lens_flare::LensFlareRenderer` draws the elements additively
in the world pass, no depth test, modelled on the vignette pass. Pictures come
through `SceneData.lens_flare` (`environment_choices::append_sky`). The client's
`sun_flare::SunFlare` decides each frame how much shows: the sun behind the view
or past the cutoff angle shows nothing, otherwise it fades toward the view's
centre; a ray to the sun against the scene's drawn opaque and cut-out triangles
(built once per map, not the map collision, which holds invisible walls) and a
brick check hide it; the level eases over 0.2 s. Both lighting shaders are
unaffected. Tests: environment validation, the angle fade, occlusion by drawn
geometry and by bricks, no flare. No wire change.
