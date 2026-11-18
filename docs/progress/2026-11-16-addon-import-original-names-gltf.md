# 2026-11-16 Add-On import: original names and DTS to glTF (branch `bionic/addon-import-overhaul`)

Three changes to the legacy Add-On importer (`bri-import-addon`), all approved
by the user: converted assets keep their **original file names** instead of
hash names; DTS models ship as a **glTF binary** (`.glb`) alone — no
`.shape.json` beside them — so the game draws the GLB in-game and Blender can
open it. A model glTF cannot hold later came to present as a placeholder
cube instead — legacy `.json` model support was dropped entirely (see below).
The release builds of `bri-import-addon` and `bri-client` land in
`target/release/`.

## Original names for converted assets (`crates/addon-import/src/lib.rs`)

A package-wide claim table (`Names`, a set of lowercased paths) now assigns
each converted file its source name: `{dir}/{subpath relative to the Add-On
root}/{stem}.{ext}` — `models/vehicle.dts` becomes
`assets/models/models/vehicle.glb`, a root-level `gun.dts` lands flat as
`assets/models/gun.glb` (a shape glTF cannot hold keeps its `.shape.json`
there instead); textures and sounds keep their names under
`textures/` and `sounds/`. When the original name is invalid for a package
path or already claimed, the file takes `{stem}-<digest8>.{ext}`, and only if
even that collides does it fall back to the old flat hash name. The same
table claims hashed placeholder cubes and flat textures, so an original can
never take a path a generated asset holds.

Brick geometry is the exception: it always lands **flat** under `bricks/`
(`brick_name` strips the subpath). The runtime names a brick's mesh file
beside `stock-catalog.json`, and both `bri_sim::definitions` and the package
checker refuse a filename with a folder in it — nested brick paths made the
end-to-end `bundled_in_game` tests fail until this was flattened.

Borrowed geometry from installed dependencies keeps its name in the lending
Add-On (the file of the virtual path after `add-ons/<name>/`), so re-imports
are stable: a changed native brick now invalidates by content, not by a new
hash filename (`tests/borrowed_bricks.rs` asserts that).

## DTS to glTF (`crates/addon-import/src/gltf/shape_export.rs`, new)

Each converted shape is written as `{same name}.glb` — the package's only
model file for it — through the crate's own glTF writer. The exporter covers
what a Blockland shape holds: nodes with transforms, meshes grouped by
material (vertices deduped per group), materials with their textures embedded
as images (blend modes mapped to `alphaMode`; additive-without-translucent
draws opaque as Torque does and is noted), billboard objects, skinned meshes
with joints/weights (`JOINTS_0` 8-bit or u16-vec4), collision details as
nodes with a `bri_blocks` extra (the re-import reads it back), and animation
clips from the shape's sequences — node tracks become channels, per-object
frame data is baked at each object's own frame (skinned objects at frame 0,
matching the runtime) with the other frames as morph targets so Blender can
play the frame animation. What glTF has no word for (two-sided or shot
collision details, texture-coordinate frames, unknown query types in a
detail's blocks) is dropped into the asset's notes instead of failing.

**GLB-only output, deliberately.** New imports ship the `.glb` alone:
`presentation.json`, the weapons and vehicle packs all point at it with its
real checksum, and no `.shape.json` is written beside it. Older packages that
still carry only a `.shape.json` keep loading through the runtime's JSON path
— that is the backwards compatibility; nothing about old files changes. A model
glTF cannot hold never ships as an un-lowerable file: `export_shape`
(`crates/gltf/src/shape_export.rs`) reads its own output back with the exact
call the client makes (`convert`, Kind::Item) and fails when it would not draw
there, so the importer writes the legacy `.shape.json` alone and notes why
("kept in its legacy .shape.json"). A shape with no visible mesh (collision-only)
or more than an item's 1024-node budget takes that fallback. Blender work
happens on the glTF; re-import comes back through `bri-import-addon gltf`.

## Verification

- New tests: `converted_files_keep_their_original_names` (subpath and flat
  cases over a real DTS with geometry — `tests/fixtures/models/shotgunShell.dts`,
  from Weapon_Shotgun; each model out as its `.glb`, read back by the crate's
  own glTF reader, no `.shape.json` beside it) and an additive-material test
  whose geometry-less fixture now asserts the legacy fallback (`.shape.json`
  written alone, "kept in its legacy .shape.json" noted, blends intact).
- Full `cargo test -p bri-addon-import`: all suites pass, including the two
  end-to-end `bundled_in_game` tests (import → install → host → client over
  loopback), which caught the nested-brick regression above.
- Clippy and rustfmt clean for the crate; release build of
  `bri-addon-import` only (`cargo build --release -p bri-addon-import`).
- Smoke test on real content: `.research/to port/Weapon_Shotgun.zip` imports
  with every asset under its original name, and both models come out as valid
  glTF 2.0 binaries — `shotgun.glb` (8 nodes, 2 meshes, 4 materials, a
  `reload` clip, textures embedded) and `shotgunShell.glb`.

## Runtime GLB rendering (`crates/gltf`, `crates/client/src/items.rs`)

The user then asked for the game itself to view the `.glb` in-game (keeping
the `.shape.json` as the legacy fallback). Approved as a bigger project; the
stated concern — no fidelity problems converting DTS models to glTF — is
covered by round-trip tests over real content.

**Crate split:** Torque readers and conversion tooling stay out of the
runtime game dependency graph (AGENTS.md), so the shared glTF code moved from
`bri-addon-import` into a new crate `crates/gltf` (package `bri-gltf`):
read/write/shape_export/map_scatter/map_zones/static_lighting/vertex_lights/lod
plus the core of the old `gltf/mod.rs`. `bri-addon-import` keeps its CLI entry
and package writers and re-exports; it is a dev-dependency of `bri-client`
only.

**Client loaders:** every model load site branches on the file's extension —
`.glb` lowers through `items::lower_glb_bytes` (the same `bri_gltf::convert`,
Kind::Item, that the importer's export check uses), anything else parses as
legacy JSON. Four sites: item models (`items.rs`), vehicle model assets
(`vehicles.rs`), Add-On explosion shapes (`explosion_shapes.rs`) and custom
body files (`avatar.rs`). New imports name their own `.glb` in the manifest —
its real checksum, so the read is sha-checked like every other resource;
older packages name a `.shape.json`, whose `.glb` sibling (base packs) is still
preferred at runtime when it lowers. A GLB that fails to lower is a fault line
for an Add-On model: with a JSON beside it the model draws from the JSON,
without one (new imports) it drops while the load itself succeeds; base-pack
models fall back silently. The GLB read takes the same 32 MB budget as the
JSON.

**Fidelity fixes in `bri-gltf`** (they change GLB bytes of existing packages,
never `.shape.json` output): materials are created in glTF array order with
used native materials pre-created in declaration order — 17 real shapes use
materials out of declaration order, which a first-use ordering would draw
wrong; `make_material` restores blend/environment/mipmaps/detail_scale/
reflectance/paint_overlay from `extras.bri_legacy`, and the exporter writes
`paint_overlay` into that block (a DTS `None` now round-trips as `None`, not
the old hardcoded `Some(false)`); the exporter's `unique_clip` renamed
underscore-free clip names to `{name}_3`, breaking role-matching on re-import
(weapon fire/reload clips came back dead) — fixed. A shape glTF cannot hold
exports no `.glb`: `export_shape`'s round-trip check fails it and the importer
writes its legacy `.shape.json` alone, noted in the report.

**Tests:**
- `crates/gltf/tests/round_trip.rs`: every drawable shape in the corpus
  (`.research/archive/defaults` + `content/addons/weapon_shotgun`) is exported
  and lowered again; material fields and order, geometry totals, node names,
  clips and texture bytes all come back. Documented exporter skips are
  mirrored by a `representable()` helper rather than asserted as regressions:
  additive, ≤1-frame or zero-channel clips (each noted at import), verbatim
  node names with case-insensitive `_N` dedup, and collision details coming
  back as `col_` collider nodes.
- `crates/client/tests/glb_model_loading.rs`: the base pack's gun model draws
  from a valid `.glb` sibling while its `.shape.json` is garbage (GLB
  preferred; a base model has no other fallback, so success proves the JSON
  was never read); an Add-On part with a broken GLB gets a "GLB unusable"
  fault line and still draws from its JSON, while its valid-GLB sibling model
  loads without a fault; and a manifest that names its own `.glb` (a new
  import) lowers it directly — a corrupt one whose checksum matches is a
  "GLB unusable" fault with no fallback to draw from.

**Re-import check (fresh `bri-import-addon` run on Weapon_Shotgun.zip):** under
dual output the package tree and every non-`.glb` file were byte-identical to
the earlier import; the two `.glb`s differed only in clip name (`reload_3` →
`reload`, the unique_clip fix), an explicit `"paint_overlay": null` legacy key,
and bufferView layout — all accessor payloads and embedded images verified
byte-identical. Under GLB-only output the fresh import ships `shotgun.glb` and
`shotgunShell.glb` with no `.shape.json` beside them; presentation and weapons
packs point at the GLBs' real checksums.

**Release builds:** `bri-import-addon` and `bri-client` rebuilt into
`target/release/` (the client binary is what renders the GLBs in-game); no
debug deliverable.

## Deferred items, completed this branch

All three deferrals landed on this branch; the scope notes are kept as the
design record. A fourth change rode with them — legacy `.json` model support
dropped entirely (see below).

### 1. Bind textures from the GLB's embedded images (drop duplicated PNGs) — done

Each model texture used to ship twice: embedded in its `.glb` (for Blender)
and as a PNG under `assets/textures/`, which is what the client bound at
runtime (`lower_glb_bytes` discarded `Converted.textures`). The user prefers
glTF-only packages fully self-contained. **Done:** de-duplication runs before
presentation (`crates/addon-import/src/lib.rs`): a texture embedded in an
exported `.glb` is removed from the package's standalone files unless named by
something else — UI icons, particle textures, or a model that failed to export
(those still draw their textures from files). The four load sites bind the GLB
's own images: a manifest binding may name a listed texture file or be empty,
and an empty slot draws the model's embedded bitmap positionally for that
material (`crates/client/src/items.rs`); UI icons (`shotgunIcon.png`) are not
model textures and stay as files.

### 2. Merge external `.dsq` sequences into their shape's GLB — done

A `.dts`'s own embedded sequences already export as glTF clips (the shotgun's
animations round-trip). Standalone `.dsq` files — referenced from TorqueScript
datablocks (`sequence0 = "run.dsq run"`) — are recognized (kind `animation`)
but not converted: the report lists them as unsupported and a shape that names
one loses that clip. **Done:** `plan_dsq_clips`
(`crates/addon-import/src/lib.rs`) resolves each Add-On's own `.dsq` to its
owning shapes from the datablocks the importer already parses, loads it once
with `bri_convert::shape::read_dsq`, and appends the clips to those shapes
before GLB export — exporter clip handling and client lowering work unchanged (clip names dedupe as today; tracks naming absent nodes are noted). A `.dsq` referenced by several shapes (two player types reusing one animation
file) loads once and merges into each of them — every owning GLB carries its own
copy, self-containment over dedup; the report lists all owners. `.dsq`s no shape
references get reported. A standalone "clips-only" `.glb` per DSQ was considered
and rejected: a DSQ holds only clip names and node-name-keyed
tracks — no skeleton, rest pose or mesh (those live in the DTS), glTF animations
target node indices so they cannot float free of a rig, and `convert` requires a
visible mesh anyway; merging into the owner's GLB is strictly more useful to
Blender and self-contained for the game.

### 3. Brick (`.blb`) geometry as glTF — done

Not a format limitation: brick meshes could export with most of `shape_export`
reused. What is missing is the same gap models had before this branch — the
brick render path has no glTF loader (bricks draw from native `brick.json`,
flat under `bricks/` beside `stock-catalog.json`) and bricks carry semantics a
plain shape export does not: paintable materials and their own collision
details/masks. **Done:** `bri_gltf::export_brick` (new
`crates/gltf/src/brick_export.rs`) exports BLB geometry to GLB preserving its
paintable materials and collision details/masks, and brick packages ship the
`.glb` alone under `bricks/`. The runtime lowers it with
`bri_gltf::brick_from_glb` (`crates/sim/src/definitions.rs`, the brick-side
equivalent of `lower_glb_bytes`) — and now requires it: a brick mesh that is
not a `.glb` faults ("Brick geometry must be a .glb; convert it with the Add-On
importer"). No JSON fallback, matching models.

## JSON model support dropped

The user and Max approved dropping legacy `.json` model support entirely — GLB
is now the only model format, on both sides of the importer.

- **Importer:** a shape glTF cannot hold no longer ships its legacy
  `.shape.json`; it presents as a shared placeholder cube (a small `.glb` with
  one material and an embedded 1×1 white PNG), named in the manifest with
  `"source": "placeholder"` and an ambiguous entry noting the model is "not in
  this Add-On or did not convert; presented as a placeholder cube"
  (`crates/addon-import/src/lib.rs`). The export self-check's message was
  updated to match.
- **Game:** item models load from `.glb` only — a manifest naming any other
  model file faults ("ships its legacy model file …; the game draws models from
  .glb only") and the item presents without it (`crates/client/src/items.rs`).

## Final cleanup (2026-11-17)

- The swatch-PNG skip in `convert` (`crates/gltf/src/lib.rs`) is restricted to
  `Kind::Item`: a factor-only material (no baseColorTexture) emits no PNG for
  an item's `.glb`, which lists only the bitmaps it carries — a swatch is made
  on demand where it draws. Every other kind writes its textures out as files
  beside a bundle or manifest and keeps one per material, swatches included (a
  map binds every slot to `{name}.png`). Item output is byte-identical to what
  the client suite was last verified against.
- Four stale tests in `crates/addon-import/tests/import.rs` still asserted the
  removed `.shape.json` fallback; rewritten over the real-geometry fixture
  `tests/fixtures/models/shotgunShell.dts` to keep their intent: the additive
  test byte-patches the fixture's material flags (0x43 → 0x4b/0x4f) and asserts
  GLB export plus the "drawn opaque" note; the missing-texture test ships only
  one of the two textures and asserts empty bindings, no standalone texture
  file (de-dup), the colour-shift tint and a diagnostic for the missing bitmap
  only; the path-global name and coverage tests run unchanged on the same
  fixture (unresolved textures land in notes, not `missing`). A new test pins
  the placeholder design: a geometry-less shape presents as the cube with
  `"source": "placeholder"` and coverage `unsupported`. The test helper
  `tiny_dts_flags` was folded into `tiny_dts` (flags are now per-material in
  one fixture).
- Fixed a real bug the de-dup exposed: presentation looked up material textures
  among the package's shipping files, so a bitmap that had been de-duplicated
  out fell into the "has no texture" branch; it now looks in the source plus
  the embedded set and binds empty for GLB models
  (`crates/addon-import/src/lib.rs`).
- Clippy clean (five warnings fixed: a redundant `.into_iter()`, two
  collapsible `if`s restructured with let-else, two lazy closures to
  `then_some`) and rustfmt applied across the workspace.

## Rename hash-named content back to original names (2026-11-17)

`crates/addon-import/src/bin/rename-content.rs` (`rename-content CONTENT_ROOT
[--apply]`, dry-run by default; report in `RENAME-REPORT.json`, full old→new map
in `RENAME-PLAN.json`) renamed every hash-named asset file in installed content
to the original Blockland name recorded in its package manifest, in place (same
directory — layout preserved). Run over all of `content/` on 2026-11-17:
9,026 candidates → **8,317 renamed** (import-report: 4115, sibling-field: 2231,
propagated-by-basename: 1270, sidecar prefix propagation: 385, sha-field:
102, presentation.json: 93, key-path: 86, v20-reference sha fallback: 35),
212 same-name collisions suffixed with an 8-hex tag, **709 left hash-named by
design** — derived artifacts whose original name is recorded nowhere (map-bundle-017
baked lightmaps under `lighting`, two map_creek lightmaps plus synthesized
`.scene.json`/`.world.json` files). `.downloads/` cache (5,882 files) and `.seal`
files are excluded. **No re-imports were performed** — `render-pack/` and
`surda-pack/` received only the same in-place renames as everything else.

Reference rewriting is package-scoped so identically named twins in different
packages never collide: resolution order per string value is (1) package-relative
path, (2) parent-dir+basename within the package, (3) basename unique within the
package, (4) globally unambiguous basename. 295 manifests were rewritten.
Flat conversion packs carry no anchor file (`package.json`/`addon.json`/
`import-report.json`/`inventory.json`), so for unanchored files scope entries are
stored under every ancestor directory up to the content root — a pack's manifest
and its assets then share one lookup map (this fixed stale texture refs in
`item-presentation-pack-010-v043-r2/presentation.json`,
`vehicles-pack-012/vehicles.json`).

Verification: 0 failures; a dangling-reference scan over every JSON file found no
live manifest still pointing at a renamed hash name — the only residuals are
report artifacts (`MIGRATE-REPORT.json`, `RENAME-*`), path→sha keys in
`import-generation.json` (keys, not values; importer bookkeeping) and one
provenance string `native/map-bundle/…scene.json` in weather-pack. Pre-apply
backup: `.research/backup/content-20261117-111120/`; rollback is restoring that
directory or reversing `RENAME-PLAN.json`.

## Map interiors as GLB (2026-11-17)

The last strict-JSON content is gone. Interiors now ship as one self-contained
`.interior.glb` per file — render geometry, authored collision, diffuse
textures and lightmap sheets embedded together:

- **Codec** (`crates/gltf/src/interior_export.rs`, `interior_glb.rs`): mesh
  "render" carries one primitive per non-empty surface (position, normal,
  base UV in TEXCOORD_0, lightmap UV in TEXCOORD_1); mesh "collision" is the
  authored triangle soup as position-only primitives with explicit indices.
  One glTF material per (detail, slot); textures deduped by file; lightmap
  sheets (+auxiliary) embedded under synthetic keys. A `BRI_interior`
  extension carries id, detail/subobject layout, surface-to-primitive mapping,
  lightmap ownership and ambient values so the native type reconstructs
  exactly (round-trip unit-tested byte-for-byte). Authored convex hulls and
  vehicle collision were dropped from the format — nothing at runtime reads
  them; the game collides from the triangle soup.
- **Load path**: render, sim and the dev probes dispatch on the asset file's
  suffix (`.glb` → embedded textures via `InteriorGlb`; `.json` still loads as
  a transitional fallback). Per-scene baked mission lightmaps stay external —
  one interior is placed in several scenes with different bakes, so only the
  original embedded sheets ride inside; `bundle.json` `lighting[scene].interiors[]`
  records point at them by name.
- **Installed content migrated** by a temporary tool
  (`crates/addon-import/src/bin/convert-map-interiors.rs`,
  `convert-map-interiors CONTENT_ROOT [--apply]`, dry-run by default, report in
  `INTERIOR-MIGRATION-REPORT.json`): all 7 installed interiors (5 in
  map-bundle-017 incl. the empty slate, 2 in map_creek) converted with a
  read-back verification of geometry, lightmaps and collision before each old
  JSON was deleted; every drawn material had to resolve to a texture or the
  asset is left as JSON with its reason reported (none failed). Bonus: all 709
  hash-named baked lightmap records now carry structural names
  (`{scene}-node{N}-slot{S}.png`), four shared bakes copied once — no
  hash-named files remain in installed content. Terrain files stay JSON on
  purpose (procedural data the runtime reads as such). Pre-apply backup:
  `.research/backup/content-20261117-124723/`. Verified: `map_collision_probe`
  passes map-bundle-017 (13 spawn-floor/solver checks); its map_creek failure
  ("no authored floor near original spawn") reproduces identically on the
  pre-conversion backup, so it is pre-existing.
- **Still to do:** `bri-import-addon` writes `.interior.json` for imported
  maps; switching the importer to write the GLB directly is the next turn.

## Known gaps and environmental notes

- **Content conversion: done by a temporary tool.**
  `crates/addon-import/src/bin/migrate-content.rs` (`migrate-content CONTENT_ROOT
  [--apply]`, dry-run by default, report in `MIGRATE-REPORT.json`) converted all
  of `content/` on 2026-11-17: 3815 models (186 shapes, all 3629 bricks) to GLB,
  290 manifests rewritten, 42 presentation texture-table rows pruned and 4
  now-redundant local textures deleted. Left as JSON on purpose:
  - ~566 visually-empty shape models (no visible mesh; the game drew nothing
    for them either — vehicles/avatars still accept their JSON) and
  - interior files, now handled: see "Map interiors as GLB" above.
  Backup of the pre-migration content: `.research/backup/content-20261117-010632/`.
- **Repo packages still legacy:** `packages/playground/playground-rifle/assets/models/carbine.shape.json`
  and `packages/showcase/steel-ball-kit/assets/models/steel-ball.shape.json`
  draw nothing (the "game draws models from .glb only" fault) while everything
  else about them loads; migrating `packages/` is a later turn.
- **CLI gap:** the `bri-import-addon gltf --kind item|model` path
  (`crates/addon-import/src/gltf/mod.rs`) still writes a `.shape.json`, which
  the game no longer reads for item models; it is tied to the content-conversion
  turn.
- **Environmental:** four `bri-client` test binaries fail on this machine with
  wgpu out-of-memory (little VRAM available); accepted as environmental by the
  user and excluded from this branch's verification. Everything else is green:
  full `cargo test -p bri-addon-import`, every binary ok, zero failures; clippy
  and rustfmt clean for `bri-gltf`, `bri-addon-import` and `bri-client`.
- **Vertex-coloured legacy shapes lost their colour at migration:** 20 pieces in
  `maps-pass-008` (pumpkins ×4, christmasTree, gravestone, chest/chestOpen,
  teledoor, spawnPoint, vehicleSpawn, checkpoint, 1x1music, four road tiles, a
  window, rapids/river) were per-vertex-coloured DTS meshes; the shape reader
  never parses a DTS `COLORS:` section (`colors: Vec::new()` in
  `crates/convert/src/shape.rs`) and the GLB export has no COLOR_0 path, so
  their `.glb`s are geometry-only — white in Blender, untextured fallback in
  game. The originals ride beside them as `.source.json` (nothing lost); other
  packs reference several of them (brick_halloween, brick_christmas_tree,
  brick_teledoor). Fix = parse DTS vertex colours into `COLOR_0` and teach the
  item-model render path to use them; deferred. The other 152 no-material
  pieces in that pack had no texture or colour in their sources either, so they
  are unchanged from legacy.

## Solid-colour model materials and texture backfill (`crates/content`, `crates/gltf`, `crates/client`, temporary tool)

Converted model GLBs referenced textures by name only — Blockland resolves a
model's pictures at runtime from the pack's presentation table — so 3567 of
the 3583 installed model `.glb`s carried no picture and drew white in Blender.
The system moves forward instead of faking it: one solid colour rides as
glTF's native `baseColorFactor` (a few bytes of JSON, not a swatch image),
and only patterned pictures are embedded.

- **Colour field** (`crates/content/src/shape.rs`): `Material::color:
  Option<[f32;4]>`, stored linear (glTF semantics); `None` means "no colour"
  (white). Import sets it from a non-white explicit factor with no texture;
  export writes it as `baseColorFactor`. Round-trip tested in
  `crates/gltf/tests/round_trip.rs`.
- **Render fallback** (`crates/client/src/items.rs` `model_scene`, and the
  vehicle path in `vehicles.rs`): presentation binding → embedded GLB image →
  solid colour → clear. The solid is synthesised at load as a 1×1 sRGB-encoded
  SceneImage (no renderer changes); built in two passes so no borrow of the
  texture list survives while it is filled.
- **Backfill tool** (`crates/addon-import/src/bin/backfill-glb-textures.rs`,
  `backfill-glb-textures CONTENT_ROOT [--apply]`, dry-run by default, report in
  `BACKFILL-REPORT.json`): a model material's texture reference resolves the
  same way the game does — by its presentation-table id (the full-path key of
  each pack's `presentation.json` `textures` map), local-pack-first, and
  several providers must be byte-identical or the reference is ambiguous and
  skipped with a reason. The picture is classified by decoded content: one
  solid colour becomes `baseColorFactor`, a patterned picture is embedded as
  PNG (bufferView + image + texture appended to the BIN chunk). Geometry
  buffers pass through byte-for-byte; materials that already carry a texture,
  or whose reference is empty (`""` = untextured on purpose), are untouched.

Applied on 2026-11-17 over all of `content/`: 25 models touched, 84 materials
backfilled (all solid — the stock swatch set in
`item-presentation-pack-010-v043-r2/textures/`, e.g. `whitecheck.png`,
`blank.png`, `black75.png`; several packs ship same-named but different
pictures under different ids, which id-based resolution sidesteps), 0 problems.
Verified against the pre-apply backup (`.research/backup/content-20261117-140352/`):
every BIN chunk byte-identical, every factor matches an independently decoded
swatch pixel in linear space, and no material or top-level array outside the
fixes changed. `smg.glb` (the canonical white case) now carries three
solid-colour materials and opens coloured in Blender; brick meshes stay
geometry-only by design — their pictures come from the runtime print system.

**Still to do:** 32 model entries whose tables still name `.json` files
(e.g. `item-presentation-pack-010-v043-r2/models/*.json`) are skipped by this
tool on purpose; they convert to GLB in a later turn, after which the tool and
its remaining temporary bins can go.

## 2026-11-17 — remaining legacy model JSONs converted (migrate-content, applied)

`migrate-content` (`crates/addon-import/src/bin/migrate-content.rs`, temporary
tool) was extended and **applied** over `content/`:

- **Vendored weapon shapes**: plain `.json` models named by a `weapons.json`
  resource entry (original-DTS `path` filter; sound resources excluded) are now
  discovered too — the 32 non-empty copies in `weapons-pack-009/shapes/`. A
  byte-identical copy of a shape that already exported with its material plan
  **reuses that GLB** (`twin_reused` report field; 25 here) so materials keep
  the colours/textures the first export embedded. `package_root`'s fallback now
  groups flat packs' `shapes/` folders at pack level so `weapons.json` itself
  gets rewritten (34 resources → `.glb`; their `sha256` fields stay the
  original DTS hashes — provenance compares those).
- **Cross-pack pruning/de-dup guard**: item manifests merge across packs
  (`merge_part`), so an Add-On may bind a swatch row another pack provides
  (e.g. SWeps binds `base/data/shapes/yellow.png`, provided by the base
  presentation pack). Pruning and texture de-dup now scan **every** manifest in
  the content root, not just their own package (case-insensitive, Torque-style).
  Without this, apply would have deleted rows/files still bound elsewhere.
  Result: only 4 globally unreferenced swatch rows pruned; **0 files deduped**.
- Applied after a fresh backup (`.research/backup/content-20261117-174009/`):
  71 models converted (32 base item models, 7 `maps-pass-008` trees, 32 vendored
  weapon shapes), 3 manifests rewritten (`presentation.json`, `weapons.json`,
  maps `manifest.json`), no double slashes anywhere. Left as JSON on purpose:
  map-bundle assets (scene loader reads JSON), collision-only helpers that fail
  the exporter's visible-mesh re-import (pre-existing class, 536 files), and two
  empty anchors (`empty.json`, SWeps' `eject_anchor.shape.json` — no geometry to
  hold in glTF; they draw nothing either way).
- **Post-apply verification**: a Python mirror of the startup checks over all
  presentations passes (every model `.glb` exists, safe path, sha256 matches;
  every texture row's file + dimensions match); embedded-image spot checks pass
  (football/soccerBall/pongProjectile carry their white PNGs, basketball its
  texture); vendored GLBs are byte-identical to their base twins; `render-pack`
  and `surda-pack` files untouched by the migrator.
- **Pre-existing bug found + fixed while verifying**: the earlier backfill pass
  rewrote 25 Add-On GLBs (SWeps ×21, support_render ×2, flashlight, walkie
  talkie) without refreshing their `presentation.json` model `sha256` fields.
  The runtime's `checked_read` enforces those hashes, so all 25 models faulted
  at load ("draws nothing"). The manifest hash fields were re-certified against
  the current file bytes (manifest-only change; files untouched). All
  presentations now verify clean.

State: no item/weapon/explosion model JSON remains except the two empty anchors
and collision helpers above. Deferred as before: GLB texture de-duplication via
embedding, BLB→GLB, DSQ sequences merged into their model's GLB.

## 2026-11-17 — Stale item-physics pin fixed (game-start blocker)

Game failed to start post-migration with `Item physics weapon checksum
mismatch`: the base `presentation.json` (`item-presentation-pack-010`) pins
the sha256 of the base weapons pack's `weapons.json`, and the migrator rewrote
that file in a different package without refreshing the pin.

- Re-stamped the pin to the current `weapons-pack-009/weapons.json` bytes
  (content-only change; gitignored).
- Migrator gained a final `refresh_physics_pins` pass: re-pins every
  presentation's `weapons_sha256`/`item_physics_sha256` against the files they
  bind — sibling pairing for Add-Ons, `packages.json` role pairing
  (`item_presentation` → `weapons`) for the base pack. Skips `.downloads`.
  Reported as `physics_pins_repinned`; dry-run counts without writing.
- Audited all 26 physics pins across content: valid. Full mirror of the
  startup checks in `ItemPhysicsContent::load_models` (schemas, both pins,
  budgets, coverage, per-item bounds/model consistency, every model/texture
  row's file + hash) passes against real content. Release binary rebuilt;
  dry-run over migrated content is idempotent (0 changes).

## 2026-11-17 — Stale brick geometry references fixed (game-start blocker #2)

Game failed to start again with `Expected a .glb brick geometry:
<hash>.brick.json`. Two distinct cross-package staleness bugs, both in
migrated/renamed content; our tools operate per-package and missed these.

- **170 stale bindings in `stock-catalog-004/catalog-audit.json`**: every
  `resolved_meshes[].native_mesh` still pointed at hash-named `.brick.json`
  files, while the actual geometry lives in a different package
  (`maps-pass-008/`) under real names as `.glb`. Repaired all 170 via the
  `RENAME-PLAN.json` mapping; each target verified to exist on disk (zero
  misses). Content-only change.
- **Disambiguation-suffix bug in `rename-content.rs`**: the collision suffix
  was appended *after* the extension (`pineTree.glb-d60e7be3`), which fails
  the runtime's `.ends_with(".glb")` check and image readers. Fixed to insert
  before the extension (`pineTree-d60e7be3.glb`). Then repaired content: 94
  files renamed (10 maps-pass-008 arch bricks + brick_modter_siba's 24 glb +
  icons etc.) with all references updated in raw text (formatting preserved)
  across `stock-catalog-004/catalog-audit.json`, `maps-pass-008/manifest.json`,
  `map-bundle-017/bundle.json`, `environments-001/environments.json`, and the
  brick_modter_siba manifests. One pre-existing non-UTF-8 file (`worlds-pass-006/
  Soccer 2v2.world.world.json`) skipped, as before.

Verification: zero bad-pattern files left on disk; catalog-audit strict
runtime-semantics check passes 170/170 (join against geometry dir, `.glb`
extension, no path chars); zero old-style `*.ext-hash8` refs in any content
JSON; all touched JSON parses. Release `rename-content` rebuilt.

Recurring lesson: this content has legitimate cross-package references (brick
catalog → geometry pack; base presentation → weapons pack), so any future
bulk rename/convert must audit cross-pack refs afterward. No general
cross-pack reference auditor exists yet — the sweeps here were one-off.

## 2026-11-17 — Physics pin went stale again: ordering trap (game-start blocker #3)

Third game-start failure, same error (`Item physics weapon checksum
mismatch`). Root cause was an **ordering trap in our own tooling**, not the
laptop sync or app-data folder:

- The pin re-stamp for `item-presentation-pack-010-v043-r2/presentation.json`
  (blocker #1, commit `f11d3ede7`) was done at 19:00 against the then-current
  `weapons-pack-009/weapons.json`.
- The brick-repair reference sweep (blocker #2) ran later (~19:18) and
  rewrote `weapons.json` in raw text — it contained old-style suffixed texture
  refs (`textures/black.png-f953fefc`) that were corrected to the new style
  (`black-f953fefc.png`). The file's bytes changed, invalidating the pin
  stamped an hour earlier.
- Verified on disk: all 164 `weapons.json` resource refs resolve (the renamed
  suffixed files exist under their new names), and every other package's pins
  + model/texture hashes were already consistent — this one cross-package pin
  was the only stale value in all of content.
- Re-stamped `weapons_sha256` to the current `weapons.json` bytes (content-only).
  Full re-audit of all 26 presentation packages: pins + every model/texture
  file hash consistent.

Rule going forward: **pin re-stamping must be the last content step** after any
bulk rewrite, and a full pin+hash audit should run at the end of every bulk
rename/convert session — including ones that touch no presentation files
directly. The `refresh_physics_pins` pass in `migrate-content` exists for this;
the trap is running other rewriters after it.

## 2026-11-17 — Vehicle/weapon asset pins stale (game-start blocker #4)

Fourth game-start failure: `Loading native vehicles: asset hash mismatch:
models/stuntplane.glb`. Same bug class, wider blast radius than the physics
pins:

- The 09:38 material-fix session re-exported GLBs across base packs and Add-Ons
  (white-model/solid-color repair) but only refreshed `presentation.json`
  hashes — it never touched `vehicles.json` `assets[].sha256` or `weapons.json`
  `resources[].sha256`, so every rewritten model left a stale pin behind.
- Audit found **162 stale asset/resource pins** (27 vehicle manifests incl. the
  base pack; weapon manifests across Add-Ons + base) plus, after re-stamping
  them, **22 invalidated `weapons_sha256` whole-file pins** (editing a
  weapons.json changes its bytes — ordering trap again).
- Direction verified before touching anything: stale pins matched the original
  import bytes still present under `.downloads/`; on-disk files carried the
  intended post-fix bytes. Provenance `source_sha256` fields were preserved
  (field-qualified replacement; shared-hash rows handled explicitly —
  `bullet.glb` pinned by two weapon defs, `steel-ball` pin sharing a value with
  its own provenance field).
- Final audit: **1292 checks across all of content pass** — every physics pin,
presentation model/texture hash, vehicle asset, and weapon resource matches
disk bytes.

Note: Syncthing was paused during this repair (repo folder syncs both ways with
the test laptop); resume it after the game starts cleanly.

## 2026-11-17 — Muzzle tracks parsed GLB weapon models as JSON (game-start blocker #5)

Fifth game-start failure: `Loading native vehicles: expected value at line 1
column 1`. The previous "corrupted manifest on the laptop" hypothesis was
wrong — both machines' content is byte-identical and valid; the error
reproduced deterministically locally. Root cause: `vehicles::muzzle`
(`attach_muzzle_tracks`) parsed weapon-vehicle model assets with
`serde_json::from_slice`. The GLB migration converted
`Cannon.glb`/`tank_turret.glb` (used by `cannonturret`, `tankvehicle`,
`tankturretplayer`); serde chokes on the `"glTF"` magic bytes — exactly that
error. Every launch since the migration failed here, after blockers #3/#4
were fixed.

Fix: new public `bri_gltf::lower(bytes, id)` (in-memory GLB→Shape via the
same `Kind::Item` conversion the render path uses); `items::lower_glb_bytes`
now delegates to it; `muzzle.rs` gained `shape_of(root, asset)`, which reads
+ hash-verifies the asset and lowers `.glb` (JSON fallback kept for legacy
packs). Used by both the muzzle builder and the previously-ignored real-content
test. Verified: `cargo test -p bri-vehicles --release
shots_leave_the_posed_barrel_mouth_at_every_pitch -- --ignored` passes — node
names (`barrel100`, `Cannon1`, `mount1`/`eye`) round-trip through the GLB, so
sampled muzzle nodes match rendered ones.

## 2026-11-17 — Rename pass left two aggregate pins stale (content-only)

Two more launch failures, same bug class as blockers #3/#4 but caused by the
hash-name→real-name rename session rather than a model re-export:

- `effects-runtime-pack-005-v043-r2`: the rename rewrote `effects.json`
  (texture entries `a877cfed….png` → `square.png`, …) but never re-stamped
  `manifest.json`'s `library_sha256`. All texture records + on-disk files were
  already consistent; only the pin was stale. Re-stamped to the current
  `effects.json` hash (content-only).
- `map-bundle-017`: the rename rewrote ~1000 file references inside
  `bundle.json` (terrain/texture/lighting filenames), invalidating
  `lighting-parameters.json`'s `bundle_sha256`. Light data itself untouched.
  Re-stamped (content-only).

Full pin audit after both re-stamps: every `library_sha256`, avatar
`rig_sha256`, and lighting `bundle_sha256` in content matches disk.

## 2026-11-17 — Stale dts-provenance invariant + horse parsed as JSON (blockers #6/#7)

- `Original/native model provenance mismatch`: `items.rs` compared
  `weapons.json` `resources[].sha256` against the presentation's
  `models[].source_sha256` for every `.dts` resource. That invariant only held
  in the JSON era, when resources pinned the original DTS bytes. The GLB-era
  re-import pins the converted file on both sides instead (audit: all 34 dts
  resources satisfy `model.sha256 == resource.sha256`, with both pack copies
  byte-identical and matching their pins; zero still use source-provenance
  pins). Fix: accept either recorded pin — legacy packs keep working, GLB-era
  packs validate. Content untouched (it was already consistent).
- `expected value at line 1 column 1` in `avatar::load_horse`: the horse model
  is now `vehicles-pack-012/models/horse.glb`; the loader parsed it as a JSON
  Shape. Fix: GLB branch via `items::lower_glb_bytes` (same conversion the
  vehicle renderer draws), JSON fallback kept. Its `.clips.json` sequence
  assets were left JSON by the migration and parse unchanged; avatar `rig.json`
  and weapon-debris models are still JSON on disk (verified).

Headless verification: `bri-client --check content <tmp>` now passes end to
end — startup validation (16 maps, 1446 brick definitions), save pictures
35/35, modern light parameters for all base maps, add-on health report.
Remaining warnings are pre-existing import gaps (unported scripts, unadapted
map objects, bodies referenced but not shipped) or the known GLB-era cosmetic
fallbacks (`eject_anchor.shape.json`, `empty.json` stand-ins).

Known follow-ups (not startup blockers): session-time animation registry in
`net::content_identity` still filters on `.shape.json`, so weapon/item clips
in GLB-only packs may be missing at host/join; tutorial target shapes are now
GLB and the JSON-only reader degrades to "targets will not be drawn". Both to
verify/fix during playtest.

## 2026-11-17 — Final model leftovers converted; NYC cache removed

Content audit found the only remaining `.shape.json` models with real geometry
were five in repo `packages/`, re-installed over content on every launch by
defaults.rs (byte-compare + mirror replace):

- Converted to GLB via `migrate-content packages --apply`: playground-buggy
  (`buggy`, `tire`), playground-rifle (`carbine`), steel-ball-kit
  (`steel-ball`). Manifests rewritten; texture files stay (vehicle loaders
  resolve pack textures themselves).
- `samples/sample-map/map/sample-map.shape.json` stays JSON on purpose: it is
  a map-bundle asset read by the scene loader's JSON path (no GLB branch yet),
  same category as terrain.
- Everything else left as JSON has no visible geometry in its source data
  (~407 collision-only/empty-mesh shapes, verified per detail offset) or is a
  map-bundle prop (31 files); nothing to convert there.
- Removed the unused NYC map download cache
  (`content/.downloads/packages/22a21dc8…` + `.seal`, ~2k hash-named files).
- Fixed `migrate-content` leaving stale `sha256` pins in vehicles.json-style
  asset entries: the sibling-hash refresh only recognized `file`/
  `native_mesh` reference keys, but those packs use `path`. It now also
  matches `path` and resolves the hash by new or old basename. (The three
  converted packs' pins were re-stamped to match; runtime `checked_read`
  verifies them at load.)

Verified: headless `bri-client --check content <tmp>` with
`BRI_INSTALL_DEFAULT_ADD_ONS=1` installed the three GLB packs into
content/addons and passed startup validation (hash-checked loads included);
no `.shape.json` remains in those packs, add-on health report clean for them.
