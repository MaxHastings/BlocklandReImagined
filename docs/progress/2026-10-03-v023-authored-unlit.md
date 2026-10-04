# 2026-10-03 Dynamic authored unlit material correction

Maxwell reports dull bulb faces in Kitchen under Dynamic lighting while Classic
and Unified look acceptable. The original model is preserved. This follow-up
does not alter the published v0.2.2 archives or tag.

The converted bulb and fluorescent models preserve their authored unlit flag
(DTS flag 32), but the static scene loader assigned those faces a white-lightmap
`Surface`. The Dynamic shader shades that material with live external lighting.
Its existing `Unlit` material path already preserves the face's texture without
external lighting. The modern loader now selects that existing path for any
authored unlit static material, independent of asset or texture names. The
compatibility loader retains its existing material assignment; lit housing
continues to use `VertexLit`. Opaque texture-alpha handling is retained.

## Evidence

Added `modern_static_unlit_keeps_its_texture_without_lighting_the_housing` to
the existing offscreen `bri-render` lighting suite. It loads a synthetic native
map bundle with renamed unlit/lit faces, both using an opaque white texture
whose alpha is zero. Before the correction it fails at the actual loaded
material kind (`Surface` instead of `Unlit`):
`/tmp/bri-v023-unlit-before.log`.

After the correction it passes with external sun and ambient disabled: the
authored face is bright (at least 250/255), the housing remains dark (at most
2/255). A counterfactual changes only the face back to the previous `Surface`
assignment and renders it dark. Increasing ambient lights the housing while
leaving the unlit face's brightness unchanged. The compatibility face remains
`Surface`, and its housing material/texture matches the modern housing after
resolving loader-local image indices.

Command: `CARGO_BUILD_JOBS=2 cargo test -p bri-render --test unified_lighting
modern_static_unlit_keeps_its_texture_without_lighting_the_housing -- --exact
--nocapture`. Result: 1 passed, 18 filtered; `/tmp/bri-v023-unlit-after.log`.
No visible window or interactive play session was used.

The initial after-fix fixture incorrectly compared loader-local image indices;
the compatibility loader loads additional lightmaps. Corrected that assertion
to compare actual texture bytes/dimensions and material properties. A brief
compile error from comparing `SceneImage` (which has no equality implementation)
was corrected in the test, without adding a production equality contract.

## Limits

Independent review identified a second required correction: changing a face
from `Surface` to `Unlit` removed it from the Dynamic map-occluder whitelist.
Added `Unlit` inside the existing `light_cubes` branch, retaining the blend,
background and masked-material exclusions. A second offscreen test reproduced
light leaking through the unlit blocker before that change and now compares
Surface/Unlit coverage for both sun and lamp shadows. It includes opaque
zero-alpha textures, solid/cutout masks, and nonblocking blend/sky/water cases.
The initial Water fixture omitted required native uniforms; it was repaired
using the existing water-scene constructor, without changing validation.

Full command: `CARGO_BUILD_JOBS=2 cargo test -p bri-render --test unified_lighting
-- --nocapture --test-threads=1`. Result: **19 passed, 0 failed, 1 ignored**,
6.61 s, `/tmp/bri-v023-lighting-suite-final.log`. The ignored check is an
existing generated-real-map inspection. Two agents independently reviewed the
material/occlusion paths; compatibility behavior and shader validation remain
intact.

This proves surface visibility, not Kitchen's complete room illumination or
subjective glow/bloom. It adds no emissive lighting, bulb mesh replacement,
invented lamp cluster or global illumination. Recovered lamp placement and
window-shadow aliasing remain separate fidelity questions. Full affected
lint/build checks and Maxwell's in-game comparison remain pending integration.
