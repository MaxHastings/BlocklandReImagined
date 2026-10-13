# 2026-10-13 v0.3.0 W5: imported bodies take the world's light

Branch `claude/gltf-importer-boc77k`, into `v0.3.0-experimental`. Max's
playtest: Mario seemed on a different rendering pipeline, unaffected by
the world's lighting.

## Cause

Every Add-On body's material was a `BrickOverlay`
(`crates/client/src/avatar.rs` `body_mesh`): v20's decal, lit paint with the
raw texture mixed over it by the texture's alpha. Mario's textures are
opaque, so his whole surface was the raw texture: the sun, ambient,
shadows and the dark of a night map never reached him. The made-up
Blockhead in tests has opaque placeholder textures, so it hid the same
way; real v20 Blockhead textures are clear but for prints.

## What changed

- **Bodies draw as held items and vehicles do** (`items::native_shape_scene`'s
  v20 rule): a texture with a translucent texel over its colour, one with
  none the texture times the light, untinted. `KHR_materials_unlit`
  materials stay unlit and `alpha` ones blend. The horse keeps its decal
  over its chest colour.
- **The importer** writes an OPAQUE glTF material's texture fully opaque
  (glTF ignores its alpha; exporters leave masks there, which would turn it
  into a decal on a weapon or vehicle too) and draws MASK cut-outs
  blended.
- `gltf_body.rs`'s render helper had its sun pointing up, so it lit
  nothing; it now shines from above and in front.

## Evidence

- `cargo test -p bri-client --test gltf_body`:
  `an_imported_body_takes_the_worlds_light_as_a_blockhead_does` renders a
  painted Blockhead and the imported figure in sun and in the dark: both
  keep 0.10 of their brightness. Before the change the figure kept 1.00.
- Max's Mario (not committed), `BRI_GLTF=mario.glb BRI_GLTF_HEIGHT=1.6
  cargo test -p bri-client --test gltf_body a_gltf_model_from_disk_renders
  -- --ignored`: Blockhead 0.10, Mario 0.10 (before: Mario 1.00). Sheet in
  project files, `v0.3.0/assets/mario-renders/lighting/`.

## Next

- Bodies, items or vehicles imported before this change keep their old
  PNGs: reimport them for the opaque-texture fix (bodies are lit either
  way by the client change).
