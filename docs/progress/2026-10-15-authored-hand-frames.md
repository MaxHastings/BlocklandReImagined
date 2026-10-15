# 2026-10-15 Authored hand frames and hold-role parity

Imported bodies may preserve authored hand sockets and bone animation instead of
receiving generic Blockhead hand fitting/support IK. The default stays fitted;
first-person stock-tool fallback remains available. Host caches separate the two
policies even when archetypes share a model. A protocol change note records the
new replicated socket setting.

The inspection then exposed a separate shared mismatch: the client selects the
two-handed ready role for a both-arms image, while the host cached only each
hand's one-handed role. The source rig has materially different one- and two-hand
right-arm rotations. Host caches now retain both variants and select using the
actual image's both_arms property. Both stock shape and imported body providers
supply the variants; older caller-supplied one-hand maps retain their fallback.
This corrects standing, level static shot origins. Seated animated muzzle/aim
parity remains open, as do crouch/pitch provenance and original-view comparison.

Evidence: authored-hand fallback/client-host regression passed. A distinct
one/two-handed synthetic rig regression passed (1 test, 405 filtered, 74 s build).
No original assets or private fixtures enter this public commit. Private original
idle frame-zero channels match all 19 source bones; two-handed arm alias preserves
that pose, and raw native hand markers match original source markers. Diagnostic
A/B captures show that bypassing generic fitting changes the rifle height. Source
foregrip closure is about 0.055 native units, not exact; this is evidence, not a
visual acceptance claim or permission to add a guessed compensation.

All-target clippy and final checks are pending. Nothing was installed, delivered
as the all-eight build, merged, tagged or published by this checkpoint.

Updated evidence: all-target clippy for content/importer/render/client/net/sim
passed (48.62 s). Quick tier passed all 12 steps (316 s). The ignored private
original-asset integration check also passed: the rendered raw right/left rifle
frames match the host's two-handed cache, and the original one-hand pose remains
distinct (0.96 s). This is standing/level technical parity, not fidelity acceptance.
Focused integration tier is in progress; final gate is not run for this candidate.

Focused tier completed: all 20 steps passed in 948 seconds, including selected
crate integration suites, actual imported-body rendering and the animated UV GPU
check. Crossover/sized-join retained their existing thresholds. Final assembled
candidate gate and Windows CI are still required before release.

A follow-up found that legacy/stock set_body_grips callers supply already authored
frames but only registered the default cache policy. A stock-model archetype opting
into preservation would otherwise lose its host grip and fire from its eye.
The legacy entry point now registers that same map under both policies; imported
models still use the explicit per-policy entry point. A stock-body regression and
the distinct-role regression pass. Sim-only focused tier passed all three steps
(153 s, unchanged integration cases); no broad network/GPU suite was repeated.
