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
