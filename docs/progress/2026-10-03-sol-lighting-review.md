# 2026-10-03 Sol High independent rendering source review

Read-only review of isolated `content-reload` commits `9f38c828` and
`b89278e2`, while root coordinates integration and serialized verification.
No Cargo, GPU rendering or interactive gameplay was performed. This is source
review, not full release or subjective visual acceptance.

The copy-preview change retains the authoritative complete blueprint, caps
cosmetic source triangles at 200,000 and uses one explicit enclosing shell when
over budget. It keeps the separate world budget and 10,000 replicated ghost
limit. Dynamic's loader and GPU entry point bypass legacy illumination images,
visibility/residual volumes and per-texel shares; current geometry/shadows and
source descriptors supply its lighting. Best's documented 496 MiB depth atlas
excludes other rendering resources and establishes no general frame-rate claim.

The initial handoff had three concrete findings: requested shadow settings could
contradict retained modern shading after a failed compatibility load; repeated
mode switches dropped receivers while detached compatibility bakes continued;
and an accepted light radius equal to the fixed near plane produced a nonfinite
projection. Compatibility constructors also ran synchronously from render
preparation during source switching. Findings were reported before editing.

The followup resolves those paths at source level. Effective accepted mode drives
renderer/caster policy. One process-wide compatibility worker has one replaceable
pending input, cancellation is checked between expensive stages, the queue lock
is released before work, and one map-owned ticket/results survive same-map mode
flips. Baker/Bake construction is inside that worker. No queue deadlock or
same-map repeated-job growth was found. Runtime and generator share radius
validation, and generator size/field validation precedes atomic output replacement.
These changes were explicitly uncompiled at this review checkpoint.

One remaining admission edge was sent to root: finite position alone does not
prove finite shadow matrices. Position `[1e38, 0, 0]`, inner radius 0 and outer
radius 0.050001 pass the new scalar validation, but the approximately 50,000
projection depth coefficient times view translation overflows f32. The new
matrix test uses only position `[3, 4, 5]`. Check the resulting six matrices or
admit a documented practical coordinate/radius envelope at the common descriptor
and GPU boundary; include a large finite-position negative. This is a source
finding, not a reproduced GPU failure or a claim that prepared native maps fail.

Root retains followup fixes, unexcluded correctness/lint checks, private sidecar
regeneration, combined gate, platform verification and publication.

## Finite-face followup disposition

Read-only review of `c4cc1bdc` closes the remaining concrete overflow finding at
source level. `shadow::finite_lamp_faces` constructs the same six face matrices
as rendering and checks every coefficient. `lighting_parameters::valid_geometry`
combines that check with the strict radius contract; the sidecar reader, atomic
writer and GPU admission all use it. The fixed face axes and quality-independent
depth coefficients cover the reported translation overflow, while lower-quality
lateral scales do not exceed the checked Best scale. No arbitrary coordinate cap
was introduced.

New tests reject the exact finite `[1e38, 0, 0]` / 0.050001-radius input at both
256 and 512 resolutions, exercise writer preservation and directly written bad
JSON reader rejection, and accept the same position with radius 50. Source review
found no new lifecycle or admission blocker in this followup. These tests remain
**uncompiled/unrun** here: performance has the exclusive compute slot, and root
will integrate all three commits and run unexcluded crate checks/native-map probes.
No production, content, hash, GPU or Cargo work was performed by this review.
