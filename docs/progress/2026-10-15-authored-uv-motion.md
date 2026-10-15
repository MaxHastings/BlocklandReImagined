# 2026-10-15 Authored texture coordinate motion

Engine development checkpoint eef1d969 preserves full material contact profiles,
swept cosmetic contacts and the ordered impairment relay. The remaining focused
run passed net (including sized join/crossover), package runtime, physics and render
unit targets, then failed sim/script_api: a newly added stowed-ammunition fixture
omitted required Magazine.reload_ticks. The fixture now supplies 120; runtime
validation was not relaxed. Resume that test and subsequent targets, not all prior
successful imports/bot/physics runs. Final integrated gate is still required.

## Candidate after the checkpoint

Native shape material uv_motion supplies scale/offset, linear scroll and rotation
about an authored center. The render clock drives it without moving the geometry
or lightmap coordinates. Both renderer shader paths must implement it: the first
GPU check caught an unchanged red texture because only scene.wgsl had been changed;
scene_original.wgsl also needs the feature. After correcting both, the bounded GPU
regression passed in Classic and Dynamic, with a quarter rotation changing the
sampled quadrant and a complete rotation restoring it.

Model/vehicle/actor item material binding and static map binding carry the motion.
glTF extras.bri_uv_motion imports with strict finite/range validation. Map export
retains it, keeping distinct motions over the same image separate. Source material
constructors retain None so existing content serialization remains unchanged.
A mechanical constructor edit initially touched two unrelated foliage alpha-cutoff
fields and split a multiline glTF initializer; corrected without changing foliage
behavior. Client/tests cargo check now passes (31.14 s). GPU check passes (22.91 s).
Importer and corrected script API checks are in progress at this entry.

## Private binding / rendered inspection

The original Warthog sensor is a single Chicago stage, not the engine/speed meter
shader. Its decoded bitmap, 2 s / -360 degree rotation, offsets and declared 2.5
center are now bound privately. Imported UVs span approximately 1.08..1.92; no
arbitrary center compensation was introduced. Actual vehicle material adapter
captures at 0/.25/.5 seconds are in artifacts/warthog-sensor-inspection in the main
checkout. The green spot moves over the blue dashboard grid. These use a diagnostic
backdrop/camera, not Blood Gulch or a matched original view. Native operation and
visible movement are verified; original UV equation/blend comparison remains open.

The large engine/speedometer rectangles still render as gray source-encoded meter
textures. They are a separate, obvious missing meter evaluation/live-input feature,
not considered finished by the sensor result. Multi-layer teleporter/glass/shield
materials, source fog/lightmap/sun completeness, authored grip/seat shot parity,
particle contact sounds, expansion and exact packaged playtest remain open. Nothing
was installed or delivered as the completed all-eight request.

Dedicated cached shadow-mask passes do not yet animate their UVs. The current
source sensor is additive/unlit and does not cast a shadow. Animated cutout
shadow parity is an open capability; do not claim it from the sensor check.
The all-target clippy run also found a terrain test material initializer missing
the new optional uv_motion field; supplied None, preserving terrain behavior.

Focused tier completed: all 20 steps passed in 948 seconds, including selected
crate integration suites, actual imported-body rendering and the animated UV GPU
check. Crossover/sized-join retained their existing thresholds. Final assembled
candidate gate and Windows CI are still required before release.
