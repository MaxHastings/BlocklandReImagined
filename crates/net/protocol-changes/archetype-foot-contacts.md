# Archetype foot contacts

`Sounds` in the archetype table gains `takeoff_on_surface` and `contacts` (surface kind to `FootContact { effect, decal: FootDecal { texture, size, seconds, color } }`). Absent fields leave footfalls as before.

`Sounds` also gains `postures` (posture name to sound) and `StepEvent` gains `sound`, `volume`, `radius`.

`Sounds` also gains `step_noise` and `land_noise`, and `StepEvent` gains `noise`.

`Sounds` also gains `posture_steps` (posture name to `PostureSteps { footsteps, surfaces }`).
