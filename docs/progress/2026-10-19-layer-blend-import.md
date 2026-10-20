# Layer-blend import (2026-10-19)

A glTF material's `extras.bri_layer_blend` (up to four `{index, transform}` layers and a `weights` texture, see docs/modding/maps.md)
imports as `Material::layers` (`LayerBlend`: helper materials for the layers and the weight map, like a surface's maps). The scene
loader draws it as `MaterialKind::LayerBlend` (`layer_transforms` from the transforms, the weight map in image slot 9), blended over when
the material is `BLEND`. A layer-blended surface is lit by its vertex colour alone, which the shader doubles: the mesh's `COLOR_0` (kept for
`bri_layer_blend` materials only, as `Mesh::colors`) is its baked light, and a vertex without one gets the neutral 0.5.

Test: `a_layer_blend_material_becomes_a_weighted_layer_pass_the_renderer_draws` (import, shape, scene materials and vertex light).
Played: nothing, headless only.
