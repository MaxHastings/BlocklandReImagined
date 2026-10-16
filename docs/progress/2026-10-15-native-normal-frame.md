# 2026-10-15 Native sampled normal frame

Root assigned `codex/normal-map-frame` at integrated claim/crouch candidate
`ab10374f4a593e7eeb9fc1c4d3365046936c5bd8`, using the existing
`shared-engine-seams` checkout. Root's gate uses its separate immutable checkout.
Root authored the accompanying new seam-registry row and requested inclusion.

## Defect and scope

A synthetic one-pixel tangent-space normal texture should retain its direction
when positive UV scale changes. Source's private probe found that doubling U
changed the green-axis light response. Both native shader routes form derivative
axes and scale both by their maximum length; increasing one UV axis therefore
reduces the other normal component. This is separate from private source normal
storage, authored tangents and Source lighting equations.

The assigned correction is limited to sampled Physical normal maps. Procedural
metal detail keeps its existing helper and call sites. No format, importer,
authored-tangent schema, custom shader capability or game policy changes belong
to this slice.

## Correction

Both shaders now use a separate sampled-normal helper. It constructs +u/+v
directions from the UV Jacobian, projects/normalizes the tangent against the
geometric normal and constructs a unit bitangent with matching handedness.
Red remains +u and green decreasing v. Singular UV or position derivatives
return the geometric normal. Common UV/position derivative magnitudes are removed
before the conditioning test to avoid scale-dependent products.

The existing `detailed_normal` helper and metal call sites remain unchanged.
Sampled normal strength, default maps and the existing z floor are preserved.

## Verification

The public test `normal_map_frame.rs` uses synthetic geometry/texels and explicit
vertex-normal references. The unchanged shader failed
`positive_uv_scales_keep_constant_normal_directions_in_both_renderers`: mode 0,
texel `[190,128,240,255]`, UV scale `(1,2)`, maximum interior RGB discrepancy
28 bytes against the fixed 2-byte tolerance. This expected regression failure
occurred before shader edits.

Commands leave `CARGO_TARGET_DIR` unset and use `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_INCREMENTAL=0`, and process-local `CARGO_BUILD_JOBS=8` alongside root's
independent gate.

- `cargo test -p bri-render --test normal_map_frame positive_uv_scales --locked`
  before the correction: expected failure above, after a 2m 07s compile.
- `cargo test -p bri-render --test normal_map_frame --locked` after correction:
  all three tests pass in 20.92s. They cover X/Y/diagonal texels under positive
  nonuniform scales, mirrored/rotated/skewed UVs, rotated geometry, degenerate
  fallback, zero strength and the default map. Mode 0 uses the original shader;
  mode 3 with shading enabled uses the modern shader. No tolerance was widened.
- `python tools/check.py quick --crates bri-render`: formatting and all 70
  renderer unit tests pass, two steps in 202s. The compatibility-lighting case
  completed successfully after its ordinary long-running notice.
- Read-only extraction/hash comparison against `ab10374f` confirms
  `detailed_normal` and `metal_surface` are unchanged in both shaders.
- `python tools/check.py focused --crates bri-render --tests
  bri-render:normal_map_frame --tests bri-render:physical_surface --tests
  bri-render:metal`: all 15 steps pass in 1546s, including warnings-denied
  all-target renderer Clippy, dependent importer/scenario/client/foliage/effect/
  weather checks and all three explicitly requested offscreen binaries. The new
  normal-frame, existing Physical and Metal binaries each pass three tests.
  Existing generated-content/long-case ignores keep their ordinary disposition;
  this slice introduces no ignored tests. No unexpected failure occurred.
- `git diff --cached --check`: passed.

Build output is retained for active integration under root's explicit instruction;
no cleanup retry was attempted. Root owns the final integrated candidate gate.

The broader movement-boundary design was saved outside this checkout during
root integration. It remains a future assignment; this renderer slice does not
implement movement or close private material or interactive acceptance.
