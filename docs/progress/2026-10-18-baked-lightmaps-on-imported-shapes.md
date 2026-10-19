# 2026-10-18 Baked lightmaps on imported shapes

Before: an imported shape (glTF map, prop or model) had one UV set and no way
to carry baked light, so a map compiled elsewhere with lightmaps drew flatter
than its source. Only `.dif` interiors took lightmaps.

After: a mesh may carry a second UV set (`Mesh::uv1`, glTF `TEXCOORD_1`) and a
material's `Surface` may name a lightmap (`lightmap` plus `lightmap_strength`,
glTF `extras.bri_lightmap`: `index`, `strength`). The importer reads both; the
lightmap becomes a helper material the surface names, like the normal and
detail maps.

How it draws:

- Dynamic (Physical materials): the lightmap stands for the sun and ambient
  light on the surface, shadows included, so it replaces them; the map and
  point lights, highlights and moving shadows (`shadowed_lightmap`) still
  apply. `Material::baked_lightmap` (strength) sets material flag 16, the image
  is slot 8, the strength rides in `material[7].w`. Both `scene.wgsl` (Soft
  Shading on) and `scene_original.wgsl` (off) carry it.
- Unified and Classic: the loader makes the material an interior-style
  `Surface` with the lightmap, so the existing lightmap shading applies. The
  strength is not applied there.

Limits: lightmaps are linear RGB in one image per material; a map with several
pages packs them into an atlas (the Blood Gulch lane does). Imported materials
other than Physical ones in Dynamic keep their old look; shapes without a
lightmap are untouched.

Evidence: `render/tests/baked_lightmap.rs` (baked light shows, the live sun and
ambient change nothing on it, strength scales it, without it the live light
applies; both shaders), the importer test
`a_baked_lightmap_and_its_second_uv_set_survive_import`, and the private Blood
Gulch renders.
