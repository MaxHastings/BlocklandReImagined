# Native content conversion

To rebuild every pack the client loads from a v20 install in one step, see
[content-regeneration.md](content-regeneration.md).

Runtime dependencies must consume `bri-content`, never `bri-convert`. Native
schema versions reject unknown layouts. Conversion outputs are editable local
JSON for now; runtime packaging/compression comes after correctness.

The current executable implements TER v3, BLB, DTS v24, DSQ v24, DIF resource44/interior0
and literal MIS conversion. It scans base/
and Add-Ons/, reads loose files and ZIP members without extracting or executing
them, and writes a new output directory outside the original installation.
It does not read user configuration. Each source has a SHA-256 fingerprint,
physical origin, virtual path, output location and diagnostics. Duplicate virtual
paths never silently overwrite each other. Archive failures remain visible and
make the command exit unsuccessfully after writing its manifest.
RAR archives are detected by signature even when named .zip. The optional
offline 7z fallback reads selected members into bounded stdout pipes, never
extracting files into the source installation. Set BRI_7Z or put 7z on PATH.
The game runtime does not depend on 7z.

Terrain elevations convert losslessly from u16 samples to native f32 world
units using the original 1/32 height unit. One native world unit currently equals
one original world unit; any future unit conversion must apply consistently to
all content and behavior. Map instance spacing/translation/repetition belongs
to the mission conversion, not terrain storage. Each of eight possible material
slots retains its own blend weights and original primary indices. Material
references are unresolved pending the material pass. Unknown high flag bits and
authoring script bytes survive in separate provenance JSON; scripts are never run.

Generated source provenance is separate from runtime content. Conversion does
not prove geometry, materials or collision are correct in-game. No full map is
considered imported until dependent assets and behavior are accounted for.

Example (output parent must exist; output directory must be new):

```powershell
cargo run -p bri-convert -- "C:\path\to\Blockland v20" "content\conversion-001"
```

Bounds: 16 MiB per terrain, 4 MiB per stored authoring script, exact end-of-input
validation, schema sample-count validation. Only v3 is accepted at this stage.
Malformed archive paths, truncated data, trailing unknown sections and unresolved
primary-layer references fail explicitly instead of manufacturing replacements.

## Bricks and catalog

BLB meshes become native counterclockwise quads with explicit top/side/bottom/
ramp/print channels. Colors remain optional (paint inheritance is different from
authored white), and transparency/additive sentinels are preserved for material
integration. Attachment grids, coverage records and collision boxes survive.
Zero-box bricks require resolving their datablock's external collision shape;
visual meshes are not silently substituted. Seven old SPECIALBRICK files lack
attachment grids; synthesized solid grids are explicitly flagged for review.

Ordinary BRICK files become meshes with separate side/top/bottom-edge/bottom-loop
regions. Texture/print lookup and faithful material shading remain unfinished.
Legacy geometry uses half-unit studs and 0.2-unit plates. Its authored normals
already encode world proportions: rotate axes and normalize, without inverse
stud/plate scaling. A corpus comparison across 135 ramp faces gives mean
normal/geometric alignment 0.99782 with rotation versus 0.97973 with inverse
scaling. This deliberately corrects the old Blockland2 importer's assumption.

Malformed stock lines use `crates/convert/data/blb-repairs.json`: exact source
hash, line precondition, replacement and explanation. The complete original text
is retained only in ignored provenance output. Unknown variants are not repaired
blindly. UV/grid adaptations requiring fidelity review are identified as such.

`stock_catalog` extracts static fxDTSBrickData declarations from an already
recovered original stock script. It handles literal properties and inheritance,
rejects dynamic expressions in required fields, and preserves other expressions
without executing them. The catalog keeps original names/categories, print
ratios, orientation corrections, special behavior tags and asset references.
It currently targets the stock catalog, not arbitrary add-on script execution.

```powershell
cargo run -p bri-convert --bin stock_catalog -- <recovered-stock-script> <converted-content-dir> <new-output-dir>
cargo run -p bri-render --bin brick_preview -- <converted-content-dir> <output-dir>
python tools/verify_brick_conversion.py <v20-root> <converted-content-dir>
```

The brick preview loads native JSON only and creates an offscreen contact sheet.
It checks geometry loading/rendering, not original materials, sorted glass,
print selection, player building rules or collisions. Pixel occupancy and visual
inspection are bounded checks, not a substitute for Maxwell's playtest.

BLB format reference: https://github.com/DemianWright/io_scene_blb (documentation;
no exporter implementation copied). Original content is the primary evidence.

## Models and animation

DTS models become native nodes, named objects, LOD/collision details, materials,
triangle meshes, optional skin influences/inverse binds and embedded clips.
DSQ files become named animation tracks with timing, interpolation channels,
ground motion and triggers. Runtime code consumes only these native schemas.
Original mesh part names remain available for avatar customization. Coordinates
rotate from Z-up to Y-up without changing world units. Torque quaternions need
conjugation as well as the basis change; this is covered by a regression test.
Shared skin arrays and explicit empty mesh slots are supported.

The reference reader layout is Torque3D v1.1 at commit
`d0de864ea26293e5e905c6ec1768f985376af3de` (Engine/source/ts), cross-checked against
the actual v20 corpus and the earlier Blockland2 importer. It is not assumed to
be the exact Blockland engine. Split-buffer guards, bounded counts, index ranges
and complete input consumption are checked. Unsupported versions/mesh types fail.
Material flags and certain legacy decal/IFL records survive in provenance with
diagnostics; their runtime rendering is not implemented. Auto-billboard details,
sorted meshes, material animation and complete source metadata preservation need
additional work. Successful conversion does not certify material fidelity.

The native sampler handles hierarchy, node rotation/translation/scale, additive
tracks, object frames/visibility and CPU skinning for validation. The production
animation mixer, root-motion policy, GPU skinning and gameplay state selection
remain work. Forty player clips are sampled at six times each. Four obsolete
files have unbound old node names: armready, boot, jump and visorup. The original
mDts constructor does not reference those files; all 36 files it actually uses
bind successfully. Its 39 sequence aliases must still become native player setup.

```powershell
cargo run -p bri-render --bin model_preview -- <converted-content-dir> <output-dir>
```

The six-cell offscreen proof renders the player bind/run/crouch poses and Jeep
body/tire with debug colors. It does not establish original texture fidelity,
correct blended gameplay animation or vehicle assembly.

## Native collision

`stock_catalog` also writes `native-collisions.json`, a library keyed by stable
brick IDs. All 136 stock entries resolve. Ninety entries use authored BLB boxes;
46 resolve their external model collision details. Each detail/object convex
piece remains separate, transformed through its bind hierarchy. Faces that cut
through a purported convex piece are rejected rather than silently filled by a
hull. This preserves tree trunk/canopy and multi-part corner shapes.

Rapier builds cached solver shapes from this portable recipe. Precise building
selection uses native convex-plane ray clipping: GJK hull queries have measurable
edge tolerances, and direct triangle queries can miss shared seams. These are
query algorithm differences, not reasons to change the original geometry.
World-space placement/broad-phase integration and caching still belong to the
gameplay implementation. The current adapter is a correctness baseline.

```powershell
cargo run -p bri-physics --bin content_probe -- <native-collisions.json> <report.json>
```

The probe compares 49,368 native plane queries against an independent triangle
intersection oracle and exercises solver contact on an original round brick.
It reports GJK discrepancies separately. It is not a movement or performance
acceptance test, and no interactive input or visible game window is involved.

## Map architecture and placements

DIF conversion preserves render surfaces and authored collision separately.
Render windings are triangle strips; collision uses reordered fan masks and
explicit hull references, including invisible null surfaces. Creating collision
from all render faces would close intentional openings. Native geometry includes
all detail levels, subobjects, convex pieces and specialized vehicle collision.
Original texture and lightmap coordinates remain attached to vertices. Embedded
PNG lightmaps are retained exactly. BSP/zone acceleration and animated-light
records remain indexed in a separate copy of the source for future adaptation;
they are not yet implemented native runtime features.

The two historical interior0 layouts are tried against an entire bounded detail,
not selected from the surface header alone. Unexpected extensions and nonempty
resource entities needing adaptation fail visibly. No trailing data is silently
discarded. Format reference: [pinned interiorIO.cpp](https://github.com/GarageGames/Torque3D/blob/d0de864ea26293e5e905c6ec1768f985376af3de/Engine/source/interior/interiorIO.cpp).

MIS conversion reads literal nested declarations without running scripts. Group
membership stays separate from transforms: these placements are absolute, not
scene-graph transform inheritance. Native matrices include the Torque rotation
conjugation and axis change. Environment settings, unadapted classes and datablock
references remain explicit properties; parsing them is not implementing them.

```powershell
cargo run -p bri-convert --bin map_bundle -- <v20-root> <converted-dir> <new-bundle-dir> Add-Ons/Map_Bedroom/bedroom.mis Add-Ons/Map_Kitchen/kitchen.mis Add-Ons/Map_Slopes/slopes.mis
cargo run -p bri-render --bin interior_preview -- <native-bundle-dir> <output-dir>
cargo run -p bri-physics --bin map_collision_probe -- <native-bundle-dir> <report.json>
```

The map bundle currently resolves direct geometry and original interior/terrain
images. Static-model materials, sky, datablock decorations and native environment
behaviors still need integration. Mission lighting is baked offline from the
originals (see below); terrain lightmaps become ordinary RGB modulation textures.
The renderer preserves the authored display-domain modulation response and returns
linear color to its sRGB attachment. Final visual fidelity remains unaccepted.

Architectural physics uses welded native triangle meshes with internal-edge
handling. The spawn-floor test caught and eliminated spurious sideways drift on
Bedroom's flat floor. Maxwell still performs interactive traversal and feel tests.

## Mission lighting bake

`map_bundle` bakes every mission's sun lighting itself (`scene_lighting.rs`);
it never reads `.ml` caches, which the reference install does not ship. The bake
ports the classic engine's `SceneLighting` pass from
[pinned sceneLighting.cc](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/sceneGraph/sceneLighting.cc):

- The sun direction comes from the Sun's `azimuth`/`elevation`
  (`getVectorFromAngles`), not its `direction` field. Slopes and Tutorial author
  no direction, and the dark maps' direction points up; only the angles match.
- Terrain: the heightfield shadow sweep, corner-weighted normals, ambient plus
  N.L diffuse, 5-bit packing and the blender's six-bit expansion, 512x512 per
  block. Interior shadows on terrain are ray-sampled per lexel.
- Interiors: outside-visible surfaces add the sun to their embedded lightmaps
  (ambient only when facing away), saturating per byte. Each surface's stored
  lightmap rectangle is lit together with a 10-texel border. Shadows come from
  light-facing outside-visible interior surfaces and terrain. The engine measured
  lit lexel area with a shadow-volume BSP; the bake samples rays instead (corners
  and centre, then 4x4 where they disagree).

`lighting_compare` rebakes a bundle and compares it with lighting that came from
the engine's own caches. Against map-bundle-015 (six missions lit from a
secondary install's caches) the mean absolute channel difference is 0.07
(Bedroom), 0.08 (Kitchen), 0.8 (Tutorial) and 4.5 (Slopes) for terrain, and
3.3 (Bedroom) and 2.8 (Kitchen) for interiors. Known gaps: Tutorial's build
platform is lit where the cache has it shadowed (one lightmap), and the dark
maps' caches carry small coloured patches the stock Sun cannot produce. Vertex
lighting is not baked; the native renderer uses lightmaps only.

```powershell
cargo run --release -p bri-convert --bin lighting_compare -- <v20-root> content/maps-pass-006 content/map-bundle-015 <new-scratch-dir>
```

Terrain placement follows the legacy centered 256-cell period using squareSize
(default 8), retaining the serialized position in provenance properties. At the
Slopes spawn, the centered interpretation gives floor Y=569.4233 and spawn
Y=571.371; origin zero incorrectly leaves the spawn hundreds of units above ground.
Native rendering, height queries and Rapier collision share checkerboard triangle
splits and periodic borders. Height interpolation is triangular, not bilinear.
Terrain holes/emptySquares and production streaming/LOD remain pending.

```powershell
cargo run -p bri-render --bin terrain_preview -- <native-bundle-dir> <output-dir>
cargo run -p bri-physics --bin terrain_collision_probe -- <native-bundle-dir> <report.json>
```

The Slopes probe compares 3,721 rays, including negative/repeated border positions,
against native height interpolation and tests constrained vertical solver contact.
It does not simulate player traversal or certify the full map environment.
