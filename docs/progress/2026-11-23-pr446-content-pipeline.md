# 2026-11-23 PR 446 content pipeline review

Reviewed Sirdelorus's PR 446 at 81a790327 against main de0013432. The
associated Archive.zip (SHA-256
`bfffc90328edf2419ee0cc373a188ee02be5dd45de6b22646ef374dfd7377a97`) contains
all 41 proposed base package folders and 2,704 GLBs. Its 19 migration tools
were inspected as inputs and kept out of the build. The archive is private
evidence. The vanilla v20 installation and the installed content were kept
read-only.

The first patch keeps the importer corrections independent of the content
layout. Import and BLS save loading now share the Windows-1252 fallback for
legacy text. The DTS material reader preserves decoded names so accented
texture names can still resolve. The vehicle importer uses authored hubs in
Torque's scan order and keeps the script's steering and drive policy on the
compacted indices. It reports discrepancies between the hub set and the
declared count. The source-specific Forest Tree repair needs the original BLB
and an assessment of the two missing faces.

Review evidence and acceptance for the GLB cutover:

- Build every base pack and the example Add-Ons through their checked-in
  recipes. The original PR ships a converted tree and temporary scripts,
  but bootstrap still calls the existing converters and the Playground
  asset generators still produce JSON. The attached tree shows installed
  coverage, not fresh-clone reproducibility.
- Keep package validation read-only. Refresh authoring outputs explicitly
  in the tool that produces them. The proposed global repin hook rewrites
  other manifests by matching virtual paths or previous hashes, which can
  couple independent copies whose bytes differ. Client, server, checker
  and network identity must consume the same published bytes.
- Reuse the model decode owned by the glTF importer. Extract format
  decoding from package writing with a reviewable model contract, and use
  the same verified model in host animation and client rendering. Keep
  ordinary glTF material factors authoritative. The PR sometimes lets
  private extras override Blender edits and relies on generated suffixes
  to rebind avatar paint. The common cached pose path also needs vertex
  colors, alongside the immediate renderer.
- Prove the cutover with the existing body scale and animation checks,
  Playground gameplay, deterministic sample map generation and a strict
  headless startup with regenerated content. Original CI run 45145121503
  failed format, body height, the base-list count, Playground model
  decoding, sample-map regeneration and a rocket test requiring private
  content in a content-free shard. Preserve original-asset verification
  off Git. The original PR adds seven converted community or v20 meshes
  and one DTS fixture that cannot be inherited into the review branch.

Windows verification of the importer slice passed the library suites for
`bri-content`, `bri-bls`, `bri-convert`, `bri-vehicles-import` and
`bri-addon-import`. The Add-On importer integration targets `import`,
`installed`, `borrowed_bricks` and `sample_map` passed 35 tests with two
private-content cases ignored as before. The encoding regression exercises
both byte encodings through an actual package import. No interactive
playtesting was performed.
