# Native item/weapon presentation handoff — 2026-09-26

`bri_client::items` loads original native item, mounted image and projectile models and exposes original icons, typed tint, animation sampling and mount helpers. Runtime code reads no Torque fields and opens no original-installation files. Host gameplay, authoritative entity IDs, animation state clocks, view selection and GPU lifecycle remain outside this adapter.

## Delivered content

Canonical runtime input: ignored `content/item-presentation-pack-003/presentation.json`, schema 2, stable ID `v20.item-presentation.001`. It contains 29 model bindings, 71 texture/icon bindings, 21 item presentations, 35 mounted image presentations and 25 projectile presentations. Deduplicated output is 73 files / 1,401,707 bytes. Independently generated pack-004 is byte-identical to pack-003. Packs 001/002 are historical schema-1 outputs; the current loader requires schema 2.

Manifest SHA-256: `75e7d203ff912ad02dce565fdc3ece2786832e4e9b279ce0661ef88c1a9aca20`.

Item-physics SHA-256: `08f7642781c70b67f353225d7a30d04be576e0d70401745f71c5a30673271788`, pinned by the manifest's required `item_physics_sha256`.

The 17 weapon-pack inventory items are joined by all four core tools; none is silently dropped:

| Core item ID | Image ID | Original model |
| --- | --- | --- |
| `v20.weapon.hammeritem` | `v20.image.hammerimage` | `base/data/shapes/Hammer.dts` |
| `v20.weapon.wrenchitem` | `v20.image.wrenchimage` | `base/data/shapes/wrench.dts` |
| `v20.weapon.printgun` | `v20.image.printgunimage` | `base/data/shapes/printGun.dts` |
| `v20.weapon.wanditem` | `v20.image.wandimage` | `base/data/shapes/wand.dts` |

All four models were already converted in maps-pass-006. Core icons come from ui-pack-003. Weapon assets come from weapons-pack-003; common native textures also come from avatar-pack-001. The assembler may copy an explicitly referenced loose base PNG from the read-only original root when absent from those packs. No original resource is changed. Original PNG/JPEG bytes and native shape JSON bytes are retained rather than recompressed or reconstructed.

The four sports ItemData definitions have no authored icon path. Their icon API returns `None` and the manifest records this fact; host UI may choose a model thumbnail. No arbitrary replacement icon is supplied. Eleven projectile definitions are intentionally model-less and yield empty geometry for the host effects system. The horse brick mounted image likewise has no model in the weapon pack. These are distinct from missing referenced files, which fail loading.

## Hashes, validation and reproducibility

Weapon `Resource.sha256` identifies original DTS bytes. It is **not** the checksum of converted shape JSON. The presentation manifest stores both `ModelResource.source_sha256` and native `ModelResource.sha256`; the latter is checked before deserialization. The manifest also pins the complete native weapons.json checksum. Runtime checks weapon item/image/projectile identities, model bindings, native image offsets/mount IDs, and original model source hashes against that weapon pack. Root should include this presentation manifest in its content identity/configuration.

Loader limits: 8 MiB manifest; 32 MiB weapon manifest/model file; 16 MiB image file; 256 models; 1,024 textures/items; 4,096 images/projectiles; each image at most 4,096x4,096; 256 MiB aggregate decoded image storage; 256 MiB aggregate encoded resource reads; 2 million aggregate model vertices. The shared bounded resource reader rejects traversal, unsafe paths, canonical-path escape and file growth. Native checksums, dimensions, schema, tint, transforms, binding counts and geometry validate before an `ItemAssets` is published. Unknown models, icons and requested animation sequences return errors rather than hidden placeholders.

Offline regeneration requires Python 3 and Pillow:

```powershell
python docs/research/item-rendering/build_presentation.py --self-test
python docs/research/item-rendering/build_presentation.py --output content/item-presentation-pack-005
```

Use a fresh output directory. The assembler refuses an existing output or a destination outside workspace `content`, and rejects original-root containment. It reads native models/resources plus narrowly selected recovered literal core definitions. It evaluates only the observed numeric tint fractions, `SPC` concatenation and named color references; it never executes script. Core definition provenance records recovered-file hash and exact declaration line. The script is research/build tooling, outside the game dependency graph.

## Authored item bounds

Schema 2 requires `bounds_min` and `bounds_max` on every `ModelResource`. They preserve the original DTS object box in native coordinates, with its model pivot unchanged. They are not fitted to visible triangles or recentered. All 29 models have verified bounds; all 21 inventory item IDs have entries in `item-physics.json`:

```json
{"schema_version":1,"items":{"v20.weapon.gunitem":{"min":[-0.17005619406700134,-0.4932105243206024,-0.901964545249939],"max":[0.17005625367164612,0.38021767139434814,0.211960569024086]}}}
```

The example shows one entry; the actual catalog includes all 21. It intentionally contains no model path or extra checksum fields. The presentation joins each item ID to its model resource and pins the catalog checksum. `ItemAssets::item_physics.items` exposes `bri_weapons::ItemBounds`; `ModelResource::bounds()` exposes the same type. Host spawner placement applies scale, rotation and source-backed pivot correction to these local bounds. This adapter does not implement placement or drop policy.

The offline reader checks the original DTS checksum against the previously converted model's source identity before extracting metadata. DTS24 has a 16-byte header with dword stream offsets, 17 count words, two smallest-visible values, then guard 0. The 11 float32 values after that guard are radius, tube radius, center and bounds. Absolute byte offsets are radius 96, center 104, minimum 116, maximum 128 and guard 1 at 140. Both guards must match across all three typed streams. This layout is independently corroborated by the pinned [DTS24 engine-family reader and writer](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/ts/tsShape.cc#L576); [Box3F](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/math/mBox.h#L20) stores minimum before maximum. The existing native converter corroborates the same layout but previously discarded these floats.

Native basis conversion is `[x,z,-y]`: minimum becomes `[min.x,min.z,-max.y]`, maximum `[max.x,max.z,-min.y]`. No numerical normalization or padding is applied. `bounds-evidence.json` records each original checksum, exact source float32 bits, stream/header offsets, radii, center and source/native boxes. It is build evidence, not a runtime input. The source geometry and native JSON checksums remain distinct; the additional bounds metadata does not alter native model or image bytes.

The loader reads the catalog with a 1 MiB limit, verifies its checksum and schema, requires exactly the presentation's item IDs, validates finite ordered bounds and requires bit-identical catalog/model coordinates before decoding geometry. Original DTS reading remains exclusively offline. Synthetic header tests independently exercise fixed offsets and all-eight-corner basis conversion; nine corrupt header cases reject. Runtime tests independently pin the original pistol float bits, verify all 21 joins, prove authored bounds can differ from visible mesh bounds, and reject seven malformed catalog/manifest cases before resource loading.

## Host API

```rust
let assets = bri_client::items::ItemAssets::load(presentation_root, weapons_root)?;
let icon = assets.icon("v20.weapon.gunitem")?; // Option<&SceneImage>, RGBA bytes
let dropped = assets.item_scene("v20.weapon.gunitem", item_world)?;
let projectile = assets.projectile_scene("v20.projectile.gunprojectile", projectile_world)?;

let image_id = "v20.image.gunimage";
let transform = assets.mount_transform(image_id, first_person, eye_world, |number| {
    host_avatar_mount_world(number) // actual posed mountN; None is an error
})?;
let image = &assets.presentation.images[image_id];
let mut mesh = assets.mesh(&image.model, image.tint)?;
mesh.pose(&assets, transform, Some("ready"), state_elapsed_seconds)?;
let mut gpu_mesh = renderer.upload(device, queue, &mesh.data)?;

// Later frames: model resources stay bound; caller owns renderer/device.
let topology_changed = mesh.pose(&assets, next_transform, Some("fire"), state_elapsed_seconds)?;
if topology_changed {
    gpu_mesh = renderer.upload(device, queue, &mesh.data)?;
} else {
    let centers: Vec<_> = mesh.data.batches.iter().map(|b| b.center).collect();
    gpu_mesh.update_vertices(queue, &mesh.data.vertices, &centers)?;
}
```

`pose(model, sequence, seconds)` and `node_transform(model, pose, instance, name)` expose native muzzle/eject/mount nodes without manufacturing a gameplay muzzle direction. The host supplies sequence/state time from weapon presentation events; the adapter does not run a second state machine. A missing named clip is explicit. Rigid transforms and pose changes are atomic with respect to a persistent `ItemMesh`; a failed sample leaves its scene intact. Shapes with no visible detail remain empty and diagnostic.

`ItemAssets` is intended to be shared as immutable loaded data. Its public presentation metadata is for lookup/inspection, not mutation after validation. Node transforms require a pose for the same native model. Native instance transforms must be finite, affine, positive-determinant transforms; no implicit left-hand reflection is accepted.

## Mount and tint evidence

Recovered v20 core item/image declaration lines: Hammer 10408/10425, Wrench 10748/10765, Printer 23935/23952, Wand 11900/12002. Their separate item and image tints remain separate; Push Broom's fractional tint expression and Skis' named ItemData tint reference are resolved offline. All positions in the manifest use native `[source.x, source.z, -source.y]` coordinates. Rotations retain source Euler degrees as typed numbers, not source expressions.

The pinned engine-family [shapeImage implementation](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/shapeImage.cc#L229) selects a nonidentity eye offset for first-person, otherwise composing the host mount, authored offset and inverse bind `mountPoint`. Its [render transform path](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/shapeImage.cc#L1025) corroborates that distinction. The helper follows this rule; zero eye offset does not mean mounting at the eye. Akimbo's left image requests mount1 separately from right mount0, preserving the original unreflected model.

Recovered `eulerToMatrix` at line 2889 converts degrees then calls `MatrixCreateFromEuler`. The engine-family [Euler matrix implementation](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/math/mMath_C.cc#L278) corresponds to `Ry(-y) * Rx(-x) * Rz(-z)`. Native code composes those rotations then changes basis. This replaced a provisional ordinary XYZ order before handoff. An independent Ski eye-rotation test checks that source forward +Y, hence native -Z, becomes -X for authored `90 -90 0`. This is qualified engine-family corroboration, not a claim that the exact v20 engine build was recovered or that Maxwell has accepted the first-person pose.

The host must provide the actual posed avatar mount transforms, including intended player scale and ready/attack arm pose. Camera bob, retraction, recoil, aiming correction, view clipping and whether an image should be visible in a particular camera are host policy; no invented constants are hidden in this adapter.

## Renderer extension

Original opaque color-shift textures such as blank alpha 0 and black25 alpha 64 compose coverage over typed tint, using the same display-space overlay mechanism as the avatar. Transparent/additive shape materials use texture coverage. New `MaterialKind::UnlitOverlay` and `Unlit` bypass sunlight and point lights while retaining world fog. This preserves fullbright wand/projectile parts without using brick FX metadata.

`AlphaMode::Additive` uses straight-alpha-weighted source RGB plus destination RGB, retains destination alpha, tests scene depth and does not write depth. It shares back-to-front transparent batch sorting. Ordinary opaque/masked/blended behavior, the camera uniform and vertex formats remain unchanged. Additive color accumulation follows the native attachment's linear blending; exact original framebuffer transfer parity is not asserted. The current corpus has no unsupported material blend mode.

Remaining model fidelity: Pinball's authored billboard flag is retained but the generic posed-shape path does not rotate its model toward the camera automatically; its scene reports that omission. Detail/bump/environment mapping, differing U/V wrap modes, mip behavior and dynamic LOD are not implemented by this adapter; those fields remain in native shapes, and applicable unsupported composition/wrap fields are reported. The current delivered item/projectile gallery reports only the Pinball billboard gap. View/material parity and mount calibration remain subject to Maxwell's interactive comparison.

In addition to per-material shape flags, some recovered ItemData/ImageData (including core tool ItemData) enable `emap`. This adapter has no datablock-level environment-reflection binding; those declarations must not be considered visually implemented merely because the native DTS material flag is false. They remain source provenance in the weapon pack/core evidence, with this explicit fidelity gap. The gallery's per-shape omission list alone is therefore not a complete original-behavior acceptance checklist.

## Verification

```powershell
cargo test -p bri-client --lib items::bounds_tests --locked
cargo test -p bri-client --test item_rendering --locked
cargo test -p bri-client --test brick_fx --locked
cargo test -p bri-render --test persistent_scene --locked
cargo test -p bri-render --test persistent_scene original_avatar_layers_update_one_persistent_gpu_scene --locked -- --ignored
cargo clippy -p bri-client -p bri-render --all-targets --locked -- -D warnings
```

Four item tests pass: actual native geometry/icons/tints/mounts and authored fire animation; seven corruption/budget/binding rejection cases; independently calculated additive/unlit/ordinary-alpha/depth pixels; all stock items/projectiles plus persistent GPU vertex update. Brick FX regression: five pass. Existing persistent renderer: eight pass, two ignored; original-avatar ignored test explicitly run and passed. Clippy with warnings denied passes after correcting a test fixture initializer warning.

The schema-2 follow-up reran the four item tests against pack-003, the two new authored-bounds unit tests, `cargo check -p bri-client --locked`, and client all-target Clippy with warnings denied: all passed. Independent pack-003/004 generation compared all 73 files byte for byte. Earlier renderer/avatar gates above belong to the preceding renderer change; this follow-up changed no shader, material composition or mount code.

The final client library run also passed: 47 tests passed and 10 integration/content tests remained intentionally ignored. Unlit item modes explicitly reject the reserved brick FX marker, protecting the metadata boundary after extending material kinds.

`artifacts/native-items/items.png`, `projectiles.png` and `report.json` contain offscreen evidence on NVIDIA GeForce RTX 4070 SUPER. Gallery cells follow the report's stable sorted ID/index mapping. All 21 items have visible pixels; 14 modeled projectiles render, and the eleven authored model-less projectiles occupy black cells identified in the report. This verifies native bindings and rendering, not subjective original parity. No visible window, gameplay input, audio playback or original-installation write occurred.

Root integration: add this pack to native content paths/identity; map inventory core tools to the IDs above; retain per-instance scene/GPU caches; supply host pose/mount transforms and image sequence/time. Runtime App/session/network edits were deliberately left to root. All files are ready for ownership transfer after the reported scoped gates.
