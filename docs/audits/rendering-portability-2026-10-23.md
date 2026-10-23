# Rendering portability audit — 2026-10-23

Maxwell authorized a full world-rendering audit, corrections for older/newer
supported GPUs, and verification of an actual packaged candidate. Lizzy's
v0.3.4 report is black gameplay world; a supplied pre-test screenshot shows
working menu controls and logo. Missing menu background art is an independently
confirmed content defect, not a reported inability to use the menu. The two
startup-only backend tests do not establish Vulkan world rendering failure.
No interactive gameplay is automated. Original source installs remain read-only.

## Acceptance and audit map

Each row follows source data through GPU output. A passing log or shader compile
is not a pixel oracle. Changes must retain working paths and be checked against
independent pixels, negative controls and actual packaged inputs. No supported
GPU family is inferred solely from its age, vendor or VRAM. Native targets are
explicitly SDR RGBA8/BGRA8; true HDR output is not implemented or claimed. All physical GPU
models cannot be validated on this PC; record actual devices/backends tested.

| Stage | Ownership and invariants | Current finding / required evidence |
| --- | --- | --- |
| Released bytes | Official archive, exact source, selected packages, file hashes | v0.3.4 SHA256 matches published sums; all 13,161 manifest payloads match. Repeat on candidate ZIP, not development tree. |
| CPU map preparation | Worker task names reach the loading UI; timings are recorded at the worker, including between-frame work | The old Loading Map interval includes animation, world/collision, surfaces, lighting and verification. A release-derived debug host spends 14.0 s on world/collision and 18.25 s hashing a cold 499 MB; these are not optimized-build performance numbers. Split the world/catalog phases and measure the packaged optimized candidate. |
| Resource identity / admission | Package-relative IDs; containment; optional failures do not discard stock resources | Confirmed assets/assets root bug in animation admission. Regression includes a real indexed animated shape and an escaping path. Verify ordinary packaged hosting. |
| UI art conversion | Curated original inputs reach importer; arbitrary personal files excluded | Ten approved menu JPGs were absent from sanitized input view. Add exact hashes, identity and view coverage, version UI pack; reject missing/tampered art in content snapshots and packages. |
| Adapter / compiler / device | Surface-compatible adapter; only supported optional features; default required limits; fallbacks tested | Inspect DX12/DXC, forced FXC, Vulkan and software where available. Device limits and downlevel capabilities now logged. Existing compiler-only startup probe did not submit pixels. Correct Auto diagnostics: DLL presence does not prove whether wgpu selected DXC or FXC internally. |
| Device smoke pixels | Upload, shader, depth, raster, copy, bounded map; drain all error scopes | New checkerboard readback probe. Midtones catch double encoding; clear-only output fails. Must pass real device/backend matrix. This alone does not certify a world. |
| Asset upload | Validated finite geometry, indices/material references, dimensions and buffer limits | SceneData validates structure; upload checks device texture/buffer limits. Audit actual release map images and use packaged content captures. |
| Camera / transforms / visibility | Reversed WebGPU 0..1 depth; consistent planes, winding, nonuniform scale; independent per-view bounds | Existing perspective and depth constants agree. Frustum/retained-view and shadow tests must pass; no map- or vendor-name special case. |
| World geometry / pipelines | Actual textured, lit, unlit, alpha-tested, blended and instanced draws; lazy pipeline completion | Existing scene/persistent tests and real Bedroom/Tutorial captures must produce nonblack world pixels. Do not count HUD as world success. |
| Lights / shadows / fog / AO | Finite inputs, valid depth use, balanced pass load/store, no undefined sampled attachment | Follow world -> split opaque -> AO -> blend -> effects -> local view -> resolve. Lighting modes, MSAA off/on, shadows and exposure must be exercised. |
| Native target / UI composition | Canonical display-space output, matching pipeline/view formats, compatible sRGB aliases | Confirmed fallback used an sRGB surface view directly although GUI requires unorm. Use declared unorm view consistently at creation, ready/recovery and draw; prefer a compatible SDR target over float/HDR formats and opaque composition where available. RGBA/BGRA storage with sRGB/unorm views passes pixel checks. |
| Resize / recovery / diagnostics | Zero-sized surfaces skipped, lost/outdated surface recreated, device loss rebuilds adapters | Existing minimized/occluded handling and recovery retained. Verify code/tests; native display acceptance remains human work. Unsupported modes must fail clearly, not silently leave a black world. |
| Delivery | Optimized matching binaries; current instructions; licenses/credits; semantic and byte checks | Remove old v0.2.x handoffs and internal creator design/test cards. Put optional guides under docs/, notices under licenses/, current README at root. Verify actual ZIP and links. |

## Primary API contract

Audit against the pinned wgpu 30.0.1 API and its local source, not guessed
hardware tables. Optional features must be explicitly requested; texture limits
and sample/usage capabilities constrain resources. Compatible sRGB/unorm view
formats must be declared. Error scopes and device loss are separate from
successful final pixel output.

- https://docs.rs/wgpu/30.0.1/wgpu/struct.DeviceDescriptor.html
- https://docs.rs/wgpu/30.0.1/wgpu/struct.SurfaceConfiguration.html
- https://docs.rs/wgpu/30.0.1/wgpu/struct.TextureDescriptor.html
- https://docs.rs/wgpu/30.0.1/wgpu/struct.TextureFormatFeatures.html
- https://docs.rs/wgpu/30.0.1/wgpu/struct.Features.html
- https://gpuweb.github.io/gpuweb/

## Open report

Black world on RX550 is not reproduced or fixed by attribution. Original logs
show nonzero world GPU work and no shader/device error. Follow-up Vulkan/FXC
logs contain no map load. Missing animation metadata is a confirmed independent
loader defect; menu background omission is a confirmed independent conversion
defect. Neither establishes the black-world cause. Retain this distinction after
candidate checks pass; request actual world pixels/log on the affected device.

Validation results and exact candidate receipts will be recorded in a dated
progress entry after tests and package verification complete.
