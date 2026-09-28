# v20 datablock fields no importer reads

Final-touches sweep, 2026-09-28. Checked against v20's own files, not our
code.

## Method

```
python tools/audit_v20_fields.py --v20 "E:/Downloads/B4v21Launcher/versions/Blockland v20"
python tools/audit_v20_poses.py  --v20 "E:/.../Blockland v20" --content content
python tools/audit_v20_prefs.py
```

`audit_v20_fields.py` lists every field the stock datablocks set and the
importer files that name it. The sweep started at 99 unread of 561 fields
across 578 datablocks. It now finds 97: `jetGroundEmitter` and
`jetGroundDistance` are wired in.

Where a field's behaviour lives only in the engine, it was recovered from
`blocklandv20.exe` with capstone. The addresses below are virtual addresses
in that image.

## Wired in this sweep

| Field(s) | What v20 does | Where |
|---|---|---|
| `PlayerData.jetGroundEmitter`, `jetGroundDistance` (4) | `Player::updateJetEffects` (0x5ad1b0). While jetting, each foot (`LFoot`/`RFoot`, 0x5a9652) casts 4 units along the jet axis. On a hit, the ground emitter runs at the hit lifted 0.1 along the normal (0x711fb0), ejecting along the normal, for `dt x (4 - d) / 4` of emitter time. | `actor_effects::jet_dust`, de7b9c4 |
| `WheeledVehicleData.steeringUseAutoReturn` (default true), `steeringAutoReturnRate`, `steeringAutoReturnMaxSpeed` | `WheeledVehicle::updateMove` (0x570c4a). A move with no yaw scales the steering by `1 - rate x min(|throttle|, max) / max`, gated by the rider's `$pref::Input::UseAutoReturnSteering`. The Jeep's misspelled `steeringAutoReturn` leaves the default in place. | `bri_vehicles` steering, ebffe9ce |
| `steeringUseStrafeSteering`, `steeringStrafeSteeringRate` (0.1) | `Player::updateMove` (0x5b2e89). A held strafe key adds +-rate of move yaw per 32 ms tick, gated by `$pref::Input::UseStrafeSteering`. | ebffe9ce |

## Re-verified

- **Ported as constants.** `bri_motor::player` defaults match
  `PlayerStandardArmor`: horizontal max, resist speed and resist factor are
  68, 33 and 0.35. Crouch speeds are 3/2/2 and the crouch box is
  1.25 x 1.25 x 1.00. The audit misses these only because of the spelling.
- **Vehicle `cameraRoll = false`** (every stock vehicle). No vehicle camera
  rolls.
- **Seat `mountThread[n]`** for the Jeep, Tank, carpet, rowboat, horse,
  cannon and turret matches `vehicles-pack-011` seat poses.
- **Every stock `ShapeBaseImageData`** (54 images) matches the weapons and
  presentation packs field for field. That covers mountPoint, offset,
  eyeOffset, rotation, eyeRotation, armReady, melee and correctMuzzle,
  plus every state's name, timeout, transitions, script, sequence, sound,
  emitter, emitter node and time, eject shell and allowImageChange. See
  `tools/audit_v20_poses.py` and `crates/client/tests/v20_poses.rs`.
- **State sequences missing from the model** (for example the hammer's
  `StopFire`, or the keys' `Fire` and `StopFire`) are missing from v20's
  DTS files too. Torque then keeps the previous sequence, as ours does.
- The rest of the list keeps the review in `v20-fidelity.md`: fields that
  do nothing in stock v20 (misspellings, AI and script tags, LOD hints,
  emitters no stock datablock sets) and the tire and spring coefficients
  adapted to Rapier.

## Still open

- `PlayerData.pickupRadius` (0.625). Pickups are contact-driven, and
  whether v20 widens the pickup box by this radius is not verified.
- `$pref::Player::renderMyJets` (0, not a datablock field).
  `updateJetEffects` skips the nozzle jets on one branch (0x5ad1f9) that
  tests a connection float and a global. That looks like "your own jets in
  first person" but is not yet proven.
