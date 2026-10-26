# Remote posture: climbing

`RemotePosture` (inside `RemoteController::posture`) gains `climbing` (holding a ladder), left out when false. A pose of a body on a ladder now carries a posture, so others can draw its ladder clips and sound its steps on the rungs.

`Sounds` also accepts `climb` as a posture name in `postures` and `posture_steps`, and a map's climb volume may name the surface kind it sounds like (`surface`).
