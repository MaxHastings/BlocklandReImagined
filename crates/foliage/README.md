# Native original map foliage

`bri-foliage` places and draws the original Bedroom grass/beargrass through host collision queries and the host wgpu30 device/render pass. It owns no physics world, GPU device, window, clock or input. The independent `bri-foliage-import` performs resource discovery and converts preserved legacy map fields into a typed native schema. No runtime dependency points to the importer. Both crates are root-workspace members and the client connects native map classification, background placement and the shared GPU lifecycle. See [client integration](../../docs/runtime-foliage.md).

Current pack: the foliage pack `crates/package/base-packages.json` lists (currently `content/foliage-pack-003/foliage.json`), schema 1. Exact scene ID `v20/add-ons/map_bedroom/bedroom.mis` has nodes18/20: grassComp40,000 and bearGrass1,000. The two PNG files retain original bytes and embedded alpha; there are no separate alpha companions in the reference foliage folder. Hash verification is mandatory on native image load. No TSStatic trees are duplicated. All legacy fields, exact source member/line/hash and explicit adaptation notes remain in definition evidence.

## Host API

```rust,ignore
use bri_foliage::*;
let pack = FoliagePack::load("content/foliage-pack-003/foliage.json")?;
let images = pack.images("content/foliage-pack-003")?;
let mut builders = pack.definitions.iter().filter(|d| d.scene == current_scene_id)
    .map(|d| PlacementBuilder::new(d.clone())).collect::<anyhow::Result<Vec<_>>>()?;
// Spread work across loading ticks. The closure performs at most the specified
// number of queries per call, including water fallback queries on later calls.
for builder in &mut builders {
    builder.advance(1024, |ray| host.nearest_static_surface(ray))?;
}
// After each builder reports completed (do not call finish early):
let fields = builders.into_iter().map(PlacementBuilder::finish)
    .collect::<anyhow::Result<Vec<_>>>()?;
let mut foliage = FoliageRenderer::new(&device, &queue, &pack, &images, fields,
    RenderConfig { target: color_format, depth: depth_format, samples: sample_count })?;
let stats = foliage.prepare(&queue, &Camera {
    position: camera_position, right: camera_right,
    view_projection, visible_distance: map_visible_distance,
}, seconds_since_map_load, fog_start, fog_end)?;
// In the host pass, after opaque scenery, with its ordinary nonreversed depth:
foliage.render(&mut render_pass);
```

The example's loading loop must repeat until **all** builders finish. `PlacementStats` reports requested/placed/rejected/query totals and completion; rejected placements are reported, never silently replaced with a placeholder. Dropping a builder cancels without mutating the world. Accepted `Plant::id` is the original requested slot within its stable definition ID. Immutable `FoliageField` exposes `definition()`, `plants()` and independent `visible(camera, indices)` for hosts with another adapter.

`PlacementRay` is native X-right/Y-up/-Z-forward, startY2000/endY-2000. `SurfaceHit` must be the nearest solid Terrain/Interior/Static/Water result, with world position and unit normal. Include prohibited classes in the trace: a roof or tree blocks terrain underneath instead of letting foliage grow through it. Ignore dynamic actors, projectiles and sensors. When `include_water=false`, omit water but keep other blockers. Water placement and underneath-water retries are explicit; current Bedroom definitions disallow water. Current tests provide all original Bedroom terrain/interior/TSStatic collision, with shared query refresh before placement. Player bricks do not trigger regeneration; this immutable module does not resample existing plants every frame. Rebuild from the same static-world snapshot for deterministic load/restore.

## Behavior and bounds

Engine-family seeded LCG drives independent X/Y radial samples and angle, bounded relocation retries, size and flip. Width equals chosen height when fixAspectRatio is enabled, matching beargrass fields. Offset, accepted surface flags and normal/slope limits are applied. PlacementAreaHeight only draws the original editor band; it does not bound placement rays. A bounded deterministic grid replaces the original quadtree. Cells conservatively include plane dimensions/sway; visible cells then receive distance rejection. Diagnostics expose source/tested/visible cells and plants. No frame scans all41,000 plants in the measured view.

The GPU holds128bytes per plant once; frames update visible u32 indices plus112bytes of camera/time/fog uniforms. Two material groups produce at most two draws for Bedroom. Source sinusoidal sway and luminance use719/seconds phase rates and quantized720-step angles; lower vertices stay anchored, upper vertices sway. Fades are full at1..70units, near0..1 and far70..90 for both shipped definitions. Ground alpha0.9 and cutoff0.5 combine with original texture alpha. Native two-sided planes use alpha test, standard source-over blending, depth test LessEqual and depth writes. Draw after opaque scenery and before effects requiring foliage occlusion. No GPU mesh/image is reuploaded for time animation.

Limits are explicit errors:128 definitions/textures,100,000 requested plants per definition,1..1000 retries,0..65,536 queries per advance call,500,000 plants per renderer,128MiB decoded/GPU image budget,4096-square maximum image, native pack/image-file size limits and finite parameter ranges. GPU storage limits are checked. Camera matrix uses wgpu0..1 depth; right must be normalized. `prepare_elapsed` accepts f64 map-relative seconds and rebases per-instance sway/light phases every ten minutes; the shader receives a small time offset. A rebase uploads the instance buffer once (reported in upload_bytes), preserving distinct authored rates beyond 24 hours. Ordinary frames still update only indices/uniforms. The f32 `prepare` wrapper remains available. Fog is a supplied linear alpha interval, not the full Torque layered fog system. Errors in prepare retain the previous GPU draw state.

## Evidence and remaining fidelity work

Pinned [OpenMBG foliage implementation](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/fx/fxFoliageReplicator.cc) and [LCG](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/math/mRandom.cc) provide engine-family behavior, not exact closed Blockland source. The native beargrass quad keeps world-up height with camera-right width, following the fixed-function source's preserved vertical column. `fxGrassReplicator` is absent from that source: its authored random rotation, square-area option and misspelled foilageColour fields are preserved and interpreted as fixed vertical planes, square sampling and top/bottom color. This is a documented native adaptation; original grass plane count/layout and exact closed-engine rendering remain unverified.

Absolute host time advances hidden plants; the source only advanced unsynchronized phases while processed for rendering. This deliberate deterministic modernization avoids camera-dependent simulation state. Native grid bounds, two-sided grass, GPU precision/color-space, lack of foliage mip chains, ground blending and simple fog remain visual calibration concerns. No scaled replicator, non-Any surface material filter or placement-editor visualizer is claimed. TSStatic meshes belong to the shared scene renderer. Rendering the original resources does not establish Maxwell's subjective fidelity acceptance.

## Reproduce

```powershell
cargo run --manifest-path crates/foliage-import/Cargo.toml -- "E:\Downloads\B4v21Launcher\versions\Blockland v20" content/map-bundle-017 content/foliage-pack-003
cargo test -p bri-foliage -- --include-ignored
cargo clippy --manifest-path crates/foliage/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path crates/foliage-import/Cargo.toml --all-targets -- -D warnings
```

The converter refuses existing output. Tests require the current foliage pack and map bundle, use actual Rapier geometry and a headless device, and never silently skip a missing adapter/content. Nine tests include seeded generation/chunk equivalence, embedded alpha, retry caps, occlusion/slope/water second-query behavior, real-map placement, culling/fade/sway/light and actual GPU pixel/depth checks. `artifacts/native-foliage` contains final logs, hashes, placement/render counters, debug timings and the inspected offscreen image. These are bounded subsystem probes, not full-map performance or playtest acceptance.
