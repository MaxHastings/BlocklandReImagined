# 2026-11-23 PR 446 compiled GLB pipeline

The second pass uses GLB as the generated model container and the existing
glTF importer as the authoring entry point. The original glTF lowering
retains its sizing, socket, animation and material rules. Its codec lives
in bri-gltf, separate from package authoring and legacy readers. Clients,
hosts and checkers read the same validated model data. Importers publish
the model and write its digests at generation time.

The versioned compiled model keeps geometry in binary glTF accessors and
serializes detail membership, object state, skins, animation tracks and
collision metadata explicitly. This preserves models with multiple authored
details, separate first-person meshes, wrap and paint policy, and the
original visibility tracks. Blender sources can be edited and imported
again. The compiled GLB's scene is a geometry preview. The native rig
manifest, brick grids, map interiors and separate material resources retain
their domain formats. Base package IDs and layout are unchanged.

Stock geometry, map models, vehicle and weapon models, item presentation,
debris and tutorial producers are connected to the compiler. The
Playground and Steel Ball models use the same compiler from their original
asset recipes. No contributor or vanilla asset data is committed.

The compiled model round-trip regression compares the complete native
shape, including more than two details and authored visibility, and checks
that a truncated binary is rejected. The existing body scale, deterministic
sample map and Playground assertions remain active. Validation targets
include an empty Windows content root, the declared v20 reference, and the
Windows/Linux PR checks. The original PR remains the contributor reference.

Local evidence before platform CI
- Using the declared read-only v20 reference, compared 124 rebuilt models
  against the existing same-source detail tables. Every name, threshold
  and range matched, including all ten levels on the Kitchen palms and oak.
  The independent geometry pass reported only the known BLB fragment and
  the unsupported DTS v18 editor marker.
- The focused Windows command was
  `cargo test --locked -p bri-addon-import --lib --test import --test installed --test sample_map -p bri-sim --test playground`.
  The first run found an additive-material assertion still parsing model
  bytes as JSON. The assertion now reads through the model codec and still
  checks both blend modes. All focused targets passed, including the
  body height, sample-map and 19 Playground regressions.
- The first clean-content bootstrap stopped because the standalone weapon
  effects converter had not picked up the light cone field from
  `51798d555`. Stock explosion lights are omnidirectional. Both standalone
  converters are now checked with their own lockfiles in Windows CI.
