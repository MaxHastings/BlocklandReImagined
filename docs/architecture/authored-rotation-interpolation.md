# Authored node rotation interpolation

Root reserves this bounded sampler prerequisite on clean contact head
`b5a82226373fe24e2ec238507f7036c81f692ce0`. Source PR42/private4c932590 reports
that an unchanged SDK normalized-linear key sampler differs from native spherical
sampling at fractional times. This note proposes a generic primitive; original
conversion/attachment matrices remain Source's evidence and acceptance.

`Animation.rotation_interpolation: RotationInterpolation` selects
`Spherical` (default) or `NormalizedLinear`, serialized snake_case. Missing field
retains native spherical sampling; default is omitted from serialization. Unknown
enum/type is rejected. Shape and ClipSet remain schema1: a defaulted alpha format
extension, no migration and no network protocol change. Existing stored key data
is unchanged. Conversion explicitly selects policy; no content/game-name rule.

Selection affects only interpolation between `NodeTrack.rotations` keyframes.
Normalized linear aligns the incoming quaternion to the first hemisphere when
their dot product is negative, mixes components by the existing frame fraction,
then normalizes. Existing spherical code is retained verbatim for the default.
Scale-axis quaternion sampling, layer weights, frozen transition blending,
additive influence, ground motion, frame timing and object channels retain their
existing semantics. Any other selection needs separate evidence and scope.

Native tests use invented two-key 120-degree rotation: at quarter time spherical
is 30 degrees while normalized-linear differs. An independent component-space
oracle checks native matrices and a child socket; this is no original-fidelity
claim. Default/explicit spherical controls must remain exact. Further controls
cover antipodal and nearly equal keys, normalization, sparse/rest/single keys,
loop closure, endpoints, invalid values, additive/layer/transition/scale-axis
preservation and Shape/ClipSet serialization.

Production proposal sent root before edits; root granted it after the enabled
red baseline (one passed, three failed, no ignores). The typed field adds
15 mechanical literal defaults across13 existing files (the exact list is in
the progress entry). No importer changes select the new policy for existing
content. No dense key baking, new framework, manifests or runtime source reader.

Eight enabled native controls pass after implementation. Scoped quick
and focused checks precede an immutable correction for Source's independent
original consumer. Root owns integrated candidate/gate and the subsequent shared
fitting/socket, authoritative pose and generation-owned animation clocks.
