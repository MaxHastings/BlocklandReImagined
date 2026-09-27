# Original brick materials and stock prints

`content/brick-materials-001/brick-materials.json` is a schema-1 native bundle.
It contains five original brick surface PNGs and 77 prints with their 77 original
selector icons: 159 image files, all byte-identical to their installation sources.
`artifacts/brick-materials-verification.json` independently verifies source/output
hashes, dimensions, decoded image integrity, every archive print/icon pair,
package scope, aliases and the complete output file set. There are no conversion
warnings, missing pairs or excluded installed print packages in this run.

| Verified default package | Prints |
| --- | ---: |
| Print_1x2f_BLPRemote | 2 |
| Print_1x2f_Default | 8 |
| Print_2x2f_Default | 7 |
| Print_2x2r_Default | 6 |
| Print_2x2r_Monitor3 | 1 |
| Print_Letters_Default | 53 |

The scope comes from recovered v20 `server/defaultAddOnList.cs`, lines 42–47,
cross-checked with `docs/vanilla-inventory.json`, including exact installed ZIP
hashes. It does not come from loading every installed `Print_*` package.
All six installed print ZIPs are default-enabled packages. Historical distribution
byte authenticity and completeness of any shipped-but-disabled print packages
still require independent distribution evidence; this pass does not certify
those broader claims. Original source archives and images remain unmodified.

## Reproduction

Run from the workspace root; output parent must exist and output must be new.

```powershell
cargo run -p bri-convert --bin brick_material_bundle --locked -- "C:/Users/Maxwell/Desktop/Games/B4v21-Launcher-Release/versions/Blockland v20" .research/bl-decompiled/v20/server/defaultAddOnList.cs docs/vanilla-inventory.json content/brick-materials-001
python tools/verify_brick_materials.py "C:/Users/Maxwell/Desktop/Games/B4v21-Launcher-Release/versions/Blockland v20" content/brick-materials-001 --report artifacts/brick-materials-verification.json
cargo test -p bri-content brick_materials --locked
cargo test -p bri-convert --bin brick_material_bundle --locked
```

The converter canonicalizes the original root and output parent before checking
containment; aliases and `..` cannot turn an output into an original-install write.
Existing outputs are refused. It reads ZIP members without extraction, rejects
unsafe/case-ambiguous paths and missing icons, and bounds archive entries, expanded
bytes, image bytes and decoded dimensions. It validates everything before creating
the output directory. The four focused tests cover source protection, package
allowlisting, print compatibility, aliases, schema and byte-preservation checks.

## Runtime contract

`bri_content::brick_materials::Bundle` depends on native schema crates only.
Surface keys are `top`, `side`, `bottom_edge`, `bottom_loop` and `ramp`, matching
the BLB surface channel. Sources are under `base/data/shapes/brick*.png`; stale
references to a `shapes/bricks` subdirectory must not be used.

Each image records a relative native PNG path, dimensions and SHA-256. Source
provenance records either an installation-relative loose path or exact archive
and member paths. `Source.archive = Add-Ons/Print_Letters_Default.zip` and
`Source.path = icons/A.png` reconstruct the original UI VFS icon reference.
Texture alpha must not blindly become surface coverage: these five images are
low-alpha color overlays, while brick/vertex alpha determines geometry opacity.
The current display-space overlay blend is supported by the local B4 shader and
earlier renderer research; exact v20 fixed-function equivalence still needs a
reference comparison. Asset conversion alone does not establish that equivalence.

Print IDs use `print/<lowercase package>/<lowercase filename stem>`. Original
case and punctuation survive in `name`; `aliases` contain original BLS tokens
such as `Letters/A` and `2x2r/monitor3`. `Bundle::resolve` accepts stable IDs or
case-insensitive original aliases, never original session-local numeric indices.
Native images keep authored proportions; `aspect` is the compatibility class,
not a width/height ratio calculated from the PNG.

Recovered `serverCmdSetPrint` accepts the brick's print class **or Letters**.
`Print::compatible` and `Bundle::compatible_prints` implement that rule for a
nonempty brick aspect. An empty aspect remains non-printable. The original client
uses separate aspect and Letters tabs; classes with no matching images open the
Letters tab. Recovered `serverCmdBuyBrick` defaults a new printable selection to
`Letters/A`. These facts do not imply letter images should be duplicated into
every stored aspect list. Printer execution, UI catalog binding, rendering and
save/replication integration require their own tests.

## Legacy vertex-color evidence

`artifacts/brick-color-sentinel-inventory.json` scans 173 core/default-package
BLBs and finds 2,068 authored color rows. Of these, 648 have alpha `-1`, with
positive or negative RGB offsets in the treasure chest and gravestone meshes.
Literal fractional alpha and alpha zero also occur. The Halloween pumpkin has
136 rows with out-of-range RGB `(200, 150, 0)` and alpha `1`.

The [BLB exporter's author documentation](https://github.com/DemianWright/io_scene_blb/tree/cf2c2f2d5bf7104eee764f229ca61e8d77cc6477#defining-colors)
describes additive/subtractive colors as offsets to the in-game paint color.
The [author's encoding](https://github.com/DemianWright/io_scene_blb/blob/cf2c2f2d5bf7104eee764f229ca61e8d77cc6477/blb_processor.py#L2667)
uses negative alpha for that mode, with signed RGB encoding addition/subtraction.
This corroborates the original corpus and rules out interpreting negative alpha
as invisible geometry. No exporter implementation was copied. Immutable source
URLs, source hashes and relevant lines are recorded in
`artifacts/brick-color-sentinel-evidence.json`.

The exact interaction between negative-alpha offsets and transparent paint is
not established by that documentation. The pumpkin's out-of-range values also
remain unresolved: automatic byte normalization or clamping would be an inference,
not proven original behavior. Preserve raw source values and document any checked
native adaptation separately. These remaining fidelity questions are required
alpha work, not waived acceptance items. This pass used no game or desktop input.

The native renderer now resolves alpha `-1` to signed RGB offsets from paint,
clamped to the display color range. Ordinary literal vertex colors/alpha remain
unchanged. For translucent paint, inherited paint alpha is explicitly provisional
and emits an omission; unsupported negative-alpha encodings and out-of-range
literal RGB also emit omissions. The source values remain untouched in the native
mesh. No inferred byte normalization was applied to pumpkin colors.

`bri-client::materials` verifies native image hashes, dimensions, containment and
decoded byte budget before binding original surfaces and prints. The material
shader uses raw display-space overlay pigment and separate brick opacity.
The offscreen GPU test checks coverage values 0, 46 and 255 against independently
calculated display colors and confirms opaque brick alpha remains 255.

`artifacts/brick-materials-gallery/` renders every one of the 77 prints on an
actual compatible native brick and renders chest/gravestone/pumpkin geometry
in three paint colors. The report records exact print/brick IDs and remaining
omissions. This verifies bindings/rendering, not original-engine image parity.
Run `cargo test -p bri-client --test brick_material_gallery --locked -- --ignored --nocapture`.
