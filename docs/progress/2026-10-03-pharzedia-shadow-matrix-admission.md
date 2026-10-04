# 2026-10-03 Pharzedia shadow matrix admission

Source-only follow-up to `b89278e2`, prompted by the coordinator's independent
review. No compilation, test, offscreen render or generated content operation
was run: the exclusive NPC performance validation pause remains in force. The
passing counts recorded for `9f38c828` do not cover this or the preceding lifecycle
follow-up. Root owns integrated validation after its explicit performance release.

A finite position `[1e38, 0, 0]` and valid radius `outer=0.050001`, `inner=0`
passed the previous field/radius checks, but the shadow projection's depth scale
multiplied its view translation into infinity. Checking only finite fields did
not establish the promised finite face matrices.

`shadow::finite_lamp_faces` now checks the actual six matrices built by the same
`lamp_faces` helper used for rendering. It checks every face axis, using the
standard Best cube resolution; resolution changes only the lateral margin
(scale at most one), while the overflow-prone depth coefficients are identical
across quality levels. Shared `lighting_parameters::valid_geometry` combines
this check with the existing radius contract. Descriptor reader, validated atomic
writer and renderer light admission all use it. No arbitrary position/content
envelope, public API or content schema was added. Normal Classic/Unified/Dynamic
parameters keep the same matrices and output; extreme finite coordinates are
accepted when the actual matrix arithmetic stays finite.

Authored regressions awaiting root execution:

- `shadow::tests::finite_positions_can_overflow_shadow_depth_projection` checks
  the concrete overflow at 256/512 face resolutions, rejects it through shared
  admission, and accepts both ordinary coordinates near the clip boundary and
  the same huge finite coordinate with an ordinary radius (no invented cap).
- `scene::tests::shadowed_light_parameters_reject_finite_position_projection_overflow`
  checks the negative case at the validator shared by GPU upload entry points.
- Existing `lighting_parameters::tests::generator_validation_preserves_existing_file_on_bad_radii_or_size`
  now also verifies that a failed overflow descriptor cannot replace a valid
  file, and that directly writing hash-matching invalid JSON is rejected by
  the reader. The original valid file is restored for the other size checks.

Direct rustfmt completed and `git diff --check` passed. Existing fourteen-map
source descriptors are expected to remain valid; this is not claimed as measured
evidence until root reruns their packaged checks and renderer/client suites.
All prior architecture constraints and rendering limits remain unchanged; Dynamic
still has no baked illumination fallback. Full alpha and interactive acceptance
remain open, with Maxwell performing all interactive playtests.
