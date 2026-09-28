# Native brick FX and legacy color handoff — 2026-09-26

> Superseded 2026-09-27: the FX equations below were approximations. The exact
> v20 per-vertex equations recovered from `blocklandv20.exe`, now used by the
> shader, are in `docs/audits/bricks.md`.

The actual replicated brick scene now binds color/shape FX to the persistent native scene renderer. This closes the missing rendering connection. It **does not establish original v20 visual parity**: recovered scripts establish the IDs, but no exact v20 brick rendering implementation or approved interactive comparison was available. The equations below are explicitly native visual approximations. Pumpkin numeric-color interpretation and transparent-paint interaction with negative-alpha offsets remain required, open fidelity work.

## Evidence and boundary

Read-only recovered `allGameScripts-Vanilla.cs` lines 17381–17382 register color IDs `None=0, Pearl=1, Chrome=2, Glow=3, Blink=4, Swirl=5, Rainbow=6` and shape IDs `None=0, Undulo=1, Water=2`. Spray callbacks at lines 13121, 13337, 13553, 13769, 13988, 14204, 14434, 14650 and 14866 corroborate the normal paint-tool IDs. Those scripts call engine methods; they do not specify shader equations, reflection maps, phase, amplitude or frequencies.

This work edits only `crates/client/src/world_scene.rs`, `crates/render/src/scene.rs`, `crates/render/src/scene.wgsl`, the new `crates/client/tests/brick_fx.rs`, and this research directory. `materials.rs` did not require changes. No importer, schema, original content, game state, network, manifest, device creation in runtime, window or input changes were needed. Root owns the shared progress/acceptance update.

## Host API and representation

`build_world_scene_materials` passes authoritative `Brick.color_effect` and `shape_effect` to:

```rust
let fx = bri_render::scene::BrickFx::new(brick.color_effect, brick.shape_effect)?;
scene.append_brick_with_fx(mesh, transform, paint, surface_materials, fx)?;
```

`append_brick` remains the no-FX entry point for ghosts and existing callers. `SceneVertex`, material and camera layouts remain unchanged. FX uses an explicitly reserved pair in otherwise unused brick `lightmap_uv`: `[1024 + color + 8*shape, -4096]`; no FX remains `[0,0]`. `BrickFx::encode/decode` validate integral code/ranges and reject unused color code 7. `SceneData::validate` rejects marked vertices on lightmapped surfaces, terrain, sky, cloud and water. Actual lightmap UV interpolation is unchanged. Avatar callers continue to supply ordinary zero lightmap UVs; the original avatar persistent upload/update regression passes.

The vertex shader decodes metadata once and carries **flat integer** IDs to the fragment shader. Perspective interpolation of the encoded floating-point values caused visible speckling in an intermediate implementation; that was corrected, with a pixel-level regression covering the failure. Host callers must preserve one FX assignment per triangle and must not use this reserved pair as ordinary brick UV data.

Phase uses existing `Camera.atmosphere[2]` elapsed seconds. A phase change only updates the camera uniform, without reuploading geometry. The adapter applies paint/FX changes when it rebuilds from the next authoritative world view. This does not synchronize elapsed time across clients or establish original phase reset semantics.

Materials remain shared across bricks. Blink selects a deduplicated blended variant so its changing opacity works even for opaque paint. A 128-brick regression verifies material count stays bounded. Other FX require no extra material per brick. Original print/surface UVs and images remain unchanged; pigment coverage is composed with painted color before lighting. Literal source colors and geometry alpha remain distinct from image coverage.

## Implemented behavior and open fidelity limits

| IDs | Native implementation | Fidelity boundary |
| --- | --- | --- |
| Color 0 | Existing diffuse, sunlight, point lighting, paint, print and alpha path | No FX change |
| Pearl 1 | Paint-tinted Fresnel contribution and broad sunlight specular | Analytic approximation; exact reflection texture/combine unknown |
| Chrome 2 | View/normal horizon sheen with narrower specular | Analytic approximation, no original sphere map or scene reflection |
| Glow 3 | Illumination component floor of 1, retaining stronger light | Approximation; no bloom or added world light implied |
| Blink 4 | Opacity multiplier `0.5 + 0.5*cos(pi*t)`, 2-second cycle | Original curve, rate and phase unknown |
| Swirl 5 | World-space axial brightness wave, range 0.4–1 | Original texture, coordinate basis, rate and amplitude unknown |
| Rainbow 6 | 5-second HSV hue cycle, retaining maximum base RGB as value | Original hue sequence, paint influence and rate unknown |
| Shape 1 | Undulo: cyclic coordinate sine displacement, amplitude 0.1 native units, angular rate 2/s, spatial frequency 2 | Original amplitudes/space/rates unknown; analytic deformed normals |
| Shape 2 | Water: two horizontal sine waves affecting height, combined bound 0.1 | Source-backed event ID, native approximation of its visual behavior |

Shape displacement is visual only; authored collision stays unchanged. `BrickFx::displacement_bounds()` returns conservative extra world-axis bounds: Undulo `[0.1,0.1,0.1]`, Water `[0,0.1,0]`. The current scene renderer does not frustum-cull these batches, so undeformed bounds cannot currently hide displaced geometry. Future culling must expand by this bound. Transparent batch sorting remains the existing batch-center approach; intersecting transparent surfaces can still sort imperfectly.

Prints remain visible under each effect, but exact original treatment of printed versus painted regions is not established. FX equations and ordering must be calibrated against original evidence before marking the alpha FX acceptance item complete. No subjective match is claimed.

## Legacy color investigation

`resolve_brick_vertex_color` centralizes the checked interpretation without changing stored mesh data:

- No authored color inherits paint.
- Observed alpha `-1` selects signed RGB offsets from paint, clamped to the display range. Existing inherited paint alpha is retained, with a diagnostic for translucent paint.
- Ordinary literal alpha 0–1 is preserved, including zero/fractional opacity. Finite literal RGB is preserved exactly, including out-of-range values; the latter emits a diagnostic.
- Nonfinite data and unsupported negative-alpha magnitudes fail before appending geometry or material variants. No guess about their meaning is silently introduced.

The [BLB exporter's author documentation](https://github.com/DemianWright/io_scene_blb/tree/cf2c2f2d5bf7104eee764f229ca61e8d77cc6477#defining-colors) describes signed paint offsets; its [encoding site](https://github.com/DemianWright/io_scene_blb/blob/cf2c2f2d5bf7104eee764f229ca61e8d77cc6477/blb_processor.py#L2667) corroborates negative alpha. It does not prove transparent-paint composition. No exporter code was copied. The prior inventory in `artifacts/brick-color-sentinel-inventory.json` found 648 alpha-minus-one rows among 2,068 authored rows in 173 stock BLBs.

Direct read-only inspection of `Brick_Halloween.zip` confirms 136 ordinary `COLORS:` records of `[200,150,0,1]`: 60 in `pumpkin_ascii.blb`, 40 in `pumpkin_face.blb`, 36 in `pumpkin_scared.blb`. `sentinel-evidence.json` records source hashes, counts and first exact line positions. Their surrounding syntax does not identify byte normalization or a special mode. The engine-family [OpenMBG ColorF header](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/core/color.h) has float/byte color representations and explicit conversion operations, but this does not establish which path Blockland's BLB reader used. A generic color type is insufficient corroboration for a reader policy.

An intermediate pre-light RGB clamp was rejected and reverted. The final runtime preserves the prior raw finite values, so this handoff **does not change the pumpkin's previous no-FX numeric-color policy**. `sentinel-candidates.png` compares, left to right, unchanged raw values, diagnostic division by 255, and diagnostic pre-light clamping. At ambient 0.08 with sunlight disabled, each proposed conversion changes 2,055 pixels and substantially darkens the face. These are comparison candidates, not recommended interpretations. The comparison test asserts source preservation and visible differences; it does not bless either conversion. Exact sentinel fidelity remains open.

## Verification and reproduction

All checks were headless/offscreen, with original installation read-only. Tests load native `brick-materials-001`, `stock-catalog-004` and `maps-pass-003`; they do not parse Torque at runtime.

```powershell
cargo test -p bri-client --test brick_fx --locked
cargo test -p bri-client --lib --locked
cargo test -p bri-render --test persistent_scene --locked
cargo test -p bri-render --test persistent_scene original_avatar_layers_update_one_persistent_gpu_scene --locked -- --ignored
cargo clippy -p bri-render -p bri-client --all-targets --locked -- -D warnings
```

Results: 5/5 focused tests; client library 45 passed, 8 intentionally ignored; persistent scene 8 passed, 2 intentionally ignored; separately enabled original avatar test passed; Clippy passed. The default persistent suite covers point lighting, water motion/depth, cloud motion, sky/fog, alpha/depth/resize, original overlay pigment and output transfer.

A final repeated Clippy invocation after that successful gate encountered concurrent shared-schema integration errors outside this task: `tool_ui.rs:374` lacked `WrenchProperties.item_spawn`, and `tool_ui.rs:667` lacked `ToolCatalog.items`. Root was notified; this task did not edit those files. The earlier successful scoped check is the verified result for this handoff; the final integrated workspace gate belongs to root.

`offscreen-fx.png` shows the original 2x2 printed tile with Letters/A, columns None/Pearl/Chrome/Glow/Blink/Swirl/Rainbow, rows time 0 and 0.8 seconds. `offscreen-report.json` records native IDs, materials, triangles and omissions. Tests also verify both shape phases, paint/alpha changes, all 21 color/shape metadata combinations, invalid marker/material combinations, ghost isolation, rejection atomicity and the flat-metadata regression. These prove working native behavior and protect regressions, not original visual parity.

Root handoff: all owned code is stable and available for workspace gates. Keep FX equation parity, pumpkin interpretation and transparent signed-offset opacity open in shared acceptance tracking. Maxwell's interactive original comparison remains outstanding.
