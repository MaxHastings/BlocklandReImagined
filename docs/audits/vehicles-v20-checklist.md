# Vehicle behaviour against v20: checklist

Date: 2026-09-29. Branch `claude/vehicle-v20-audit-mg3f1h`. Scope: every stock
vehicle (Jeep, Flying Wheeled Jeep, Tank and its turret, Magic Carpet, Ball,
Horse, Rowboat, Pirate Cannon, Skis and the tumble body) and the Stunt Plane
Add-On. Maxwell reported three things: the mouse is inverted in vehicles, the
Stunt Plane's first-person view stays level while the plane loops, and "many
other little things". He tested the Tank's driver seat and says it is good.

This builds on `vehicles.md` (items 1 to 41), `skis-v20.md` and
`v20-behaviour.md`. Items those audits verified with tests are listed with
their number rather than re-derived.

## Sources

- The designated reference install (`E:\...\Blockland v20`, read only): its
  `base/client/defaults.cs`, `config/client/prefs.cs` and Add-On ZIPs. The
  Stunt Plane is not in that install; its script came from Maxwell's archive
  copy of `Vehicle_Stunt_Plane.zip`.
- Recovered v20 scripts in `.research/bl-decompiled/v20` (stock
  `client/defaults.cs`, `allClientScripts.cs` `pitch()`, `allGameScripts.cs`
  `Armor::onMount`, `Armor::doDismount`, `serverCmdNextSeat`) and v21's
  `client/defaults.cs`.
- blocklandv20.exe, disassembled from the reference install for this audit:
  - `Player::getCameraTransform` 0x5ab7d0
  - `Player::getRenderEyeTransform` 0x5aafa0 and the matrix it builds, 0x5ab0d0
  - `Vehicle::getCameraTransform` 0x56cc10
  - `Player::processTick`'s move split, 0x5b2cad
  - `Player::updateMove`'s head update, 0x5ae972 to 0x5aeb27
  - `GameConnection::isFirstPerson` 0x526d60
  - the `$TypeMasks` registration at 0x59b65c: PlayerObjectType is 0x4000 and
    VehicleObjectType is 0x10000.

Nothing from these sources is committed. Addresses and the rules read from
them are recorded here instead.

## What the exe says about riding cameras

- **First person.** `Player::getCameraTransform` at `pos` 0 checks the mount:
  - A mount of type 0x4000 (a Player: horse, rowboat, cannon, tank turret)
    whose rider controls it: the eye is the mount node plus the rider's posed
    `eye` node, in the mount's frame.
  - Everyone else, including every rider of a vehicle (0x10000):
    `getRenderEyeTransform`, which is the rider's own transform (the mount
    node's world transform) times `rotZ(mHead.z)·rotX(mHead.x)`, at the posed
    `eye` node.

  The first-person view therefore rolls and pitches with the vehicle. The
  last camera fix (06ed4ce7b) had the two type masks swapped: it gave vehicle
  drivers the player-mount rule. The positions agree on every stock seat
  because their mount nodes are unrotated.
- **The head in a vehicle.** `processTick` gives the rider fire, jet and
  pitch, and gives the vehicle the whole move minus fire, crouch and jet.
  While free looking, the rider takes yaw and pitch and the vehicle gets
  neither. `updateMove` adds the pitch to `mHead.x`, clamped to the look
  angles. Then, for a rider of a vehicle in first person who is not free
  looking, it halves `mHead.x` and `mHead.z` every tick. In third person it
  leaves them as they are. Free look turns `mHead.z` up to `maxFreelookAngle`
  (3 rad) and is allowed because every mounted player has a control object
  (`Armor::onMount` sets one for passengers too).
- **Third person.** A mounted player at `pos` above 0 asks its mount for the
  camera. Every rider of a vehicle therefore sees the vehicle's chase camera,
  and so does the Tank's gunner, whose turret is itself mounted on the Tank.
  Riders of a player-type mount see that mount's player camera.
  `Vehicle::getCameraTransform` builds its free-look swing from the head of
  the first Player in the vehicle's mount list, which is the newest rider.

## Checklist

Verdicts:
- **Matches**: our behaviour already agrees with v20.
- **Fixed**: changed on this branch, with a test.
- **Differs (accepted)**: an accepted default that differs from v20.
- **Not fixable**: explained in the row.

### Controls

| Item | v20 | Verdict |
|---|---|---|
| Invert Mouse In Vehicles default | Stock v20 ships `VehicleMouseInvert = 1`. The designated reference install and stock v21 ship 0. `pitch()` uses it only while driving a mouse-steered vehicle without free look | **Fixed**: default 0 (`NATIVE_DEFAULTS` replaces the pack's stock value), so the mouse moves a plane's nose the same way it moves the view. This was Maxwell's inverted mouse. A saved choice is kept; the option was only saved when changed |
| Invert sign per vehicle kind | Mouse drivers (Stunt Plane, Flying Wheeled Jeep, Magic Carpet, skis) use `VehicleMouseInvert`. Strafe-steered drivers (Jeep, Tank), passengers, gunners and free look use `MouseInvert` | Matches (`controls.rs` test `vehicle_mouse_invert_replaces_invert_mouse_while_mouse_steering`) |
| Mouse steering | `mSteering` accumulates move yaw and pitch, clamped to `maxSteeringAngle` | Matches (vehicles.md 9, 10) |
| Strafe steering | A held strafe key adds `steeringStrafeSteeringRate` each tick, gated by `$pref::Input::UseStrafeSteering` and the datablock | Matches (vehicles.md 10) |
| Auto-return steering | `steeringUseAutoReturn` and the pref: with no move yaw, both axes shrink by rate × throttle share. The Stunt Plane's `steeringAutoReturn = false` is not a real field, so the default (on) applies | Matches |
| Free look while mouse steering | The vehicle gets no yaw or pitch while free looking | **Fixed**: free look used to feed the steering, so looking round the cockpit turned the plane |
| Brake, jet, crouch | Jump brakes (trigger 2), jet leaves (`doDismount`), crouch reaches neither | Matches (vehicles.md 32) |
| Tools while seated | The rider keeps fire; the vehicle never gets it | Matches (vehicles.md 31) |
| Next and previous seat | `serverCmdNextSeat`/`PrevSeat`: the next free seat round the vehicle; on foot, on a one-seat mount or with no free seat, nothing happens | **Fixed**: those cases used to answer "You are not in a vehicle" or "No free seat" |
| `UseStrafeSteering` and `UseAutoReturnSteering` defaults | Stock v20 and v21: 1 and 1. The reference install: 0 and 0 | Differs (accepted): kept at 1. Maxwell rated the strafe-steered Tank good, and v21 keeps 1 |

### Cameras, in every seat

| Item | v20 | Verdict |
|---|---|---|
| First-person orientation, vehicle seats (every Jeep and Flying Wheeled Jeep seat, Tank driver and passenger, Carpet seats, skis, Stunt Plane pilot and wing riders) | Seat rotation × head: rolls and pitches with the vehicle | **Fixed**: the view was yaw and pitch only and stayed level through a loop. `Ride::Seat` in `controls.rs`, rolled basis in `app.rs` |
| First-person head return | The head halves every 32 ms tick in first person unless free looking; nothing returns in third person | **Fixed**: seated riders used to tilt the view freely and snap free look back on release |
| First-person free look | Up to 3 rad of turn, with pitch free; springs back after release | **Fixed** (same change) |
| Tank gunner first person | The turret is a Player on the Tank: the view is the hull's rotation turned by the aim, with pitch free and no spring | **Fixed**: now rolls and pitches with the hull (`Ride::Hull`) |
| Horse, Rowboat, Cannon, Tank Turret first person | A Player mount: upright, no spring | Matches (unchanged) |
| First-person eye position | Vehicle riders: the eye through the seat. Riders controlling a player mount: the mount node plus the eye, in the mount frame | **Fixed** rule (see "What the exe says"); positions were already right on stock seats. `vehicle_first_person` checks every Tank seat |
| Driver third person | Level chase camera behind the heading: `cameraMaxDist`, `cameraOffset` rise, `cameraTilt`, ray back-off | Matches (vehicles.md 2, 26) |
| Passenger third person | The vehicle's chase camera | **Fixed**: passengers orbited their own seat. `vehicle_first_person` checks the Tank passenger against `driver_view` |
| Gunner third person | The Tank's chase camera (turret mounted on the Tank) | **Fixed**: the gunner orbited the turret |
| Third-person free look | Swings the chase camera round by the head's turn | Matches for the driver. Differs (accepted): v20 uses the newest rider's head for everyone in the vehicle, so a passenger's free look moved the driver's camera. Here each rider's own free look swings their own view, and the gunner's never does |
| Camera roll in third person | `cameraRoll` is off on every stock vehicle and the Stunt Plane | Matches: the chase camera stays level |
| Field of view | Horizontal FOV | Matches (vehicles.md 34) |

### Mounting and dismounting

| Item | v20 | Verdict |
|---|---|---|
| Boarding | Land on top (feet 0.2 above the origin); take the first free mount node | Matches (vehicles.md 8) |
| Seat facing | The body takes the mount transform | Matches (vehicles.md 1, 35) |
| Look limits | `setLookLimits` clamps only the arms' look pose | Matches (vehicles.md 25) |
| Mount sound | `playerMountSound`; no dismount sound | Matches (vehicles.md 33) |
| Dismount points | 2.2 up the rider's own (tilted) transform, then 3 up, 3 down, 3 along world +X and -X, times the vehicle's scale | **Fixed**: the first point was world up, so a rider of a vehicle on its side went up through the floor pan instead of out the side |
| Dismount with every point blocked | Always gets out, at the last point tried (world -X) with no push. Forced: stays on the seat | **Fixed**: we refused to let the rider out |
| Velocity carried | `setVelocity(vehicle.getVelocity())` plus the offset as an impulse per unit of mass | **Fixed**: we also added the vehicle's spin at the exit point |
| `doSimpleDismount` | Skis and the tumble body get out in place with the vehicle's velocity | Matches for stock (by family). An imported vehicle's own `doSimpleDismount` is not read yet; no stock or known Add-On sets it outside the skis |
| Switching seat | The next free seat; a turret seat mounts its player | Matches (the Tank's gunner seat is seat 2) |

### Respawn, damage and wrecks

| Item | v20 | Verdict |
|---|---|---|
| Respawn time | Minigame `VehicleRespawnTime` (`$Game::MinVehicleRespawnTime` 0) | Differs (accepted): 1 s floor |
| Crash damage | `collDamage*` are networked but never applied | Matches: no crash damage (progress.md, protocol 39) |
| Damage scaling and passenger protection | `VehicleDamageScale`, `protectPassengers*` | Matches (vehicles.md 18, 19) |
| Explosions and final explosion | Initial and final explosions with `impulseVertical` | Matches (vehicles.md 20) |
| Burn emitter on a destroyed vehicle | `damageEmitter` until removed | Matches (`actor_effects.rs`) |
| Ski wreck and tumble | `onWreck` → `deathVehicle` | Matches (vehicles.md 13; skis-v20.md) |
| Run over and click to flip | `minRunOverSpeed`; `Vehicle::onActivate` impulse | Matches (vehicles.md 15, 16) |
| Vehicle limits | `MaxPhysVehicles_Total` 10, `MaxPlayerVehicles_Total` 150 | Matches: they are v20's server defaults |
| Map vehicle spawns on internet hosts | No such cap | Differs (accepted): capped at 5 |

### Wheels, flight and water

| Item | v20 | Verdict |
|---|---|---|
| Suspension and tire friction | Torque tire springs and relaxation | Differs (accepted): a Rapier raycast vehicle with the authored spring and friction (vehicles.md 28) |
| Steered wheels | `WheeledVehicleData::onAdd`: with 3 wheels only wheel 0 steers; a vehicle's own `onAdd` overrides it (the Tank steers all four) | Matches (vehicles.md 38; progress.md). The accepted "only wheel 0 steers" is v20's rule for three wheels |
| Flying wheeled forces (Flying Wheeled Jeep, Stunt Plane) | Thrust, capped lift, bite, surfaces, drag | Matches (vehicles.md 29, 37) |
| FlyingVehicle (Magic Carpet) | Stock Torque forces and sphere inertia | Matches (vehicles.md 9, 40) |
| Jets | The rider's jet leaves instead; `jetForce` never reaches a vehicle | Matches (vehicles.md 32) |
| Water | Buoyancy and drag by coverage; sinking | Matches (`jeeps_sink_and_stop_spinning_in_water`) |
| Splash, tire dust, impact sounds | `vehicleSplash`, `tireEmitter`, `soft/hardImpactSound` by speed | Matches (`actor_effects.rs`, `world.rs`) |
| Engine sounds | Every stock datablock leaves them commented out | Matches: silent |
| Propeller and contrails (Stunt Plane) | `playThread` by speed; contrail images above `minContrailSpeed` | Matches (vehicles.md 39; trails) |

### HUD

| Item | v20 | Verdict |
|---|---|---|
| Vehicle HUD | None: no speed, health or seat display | Matches |
| Whiteout on a ski crash | `setWhiteout` | Matches (vehicles.md 27) |

## Noticed outside vehicles (not changed)

On foot, v20 only allows free look in third person, or in first person once
the player has a control object. `doDismount` gives the player one, so after a
first dismount free look works in first person on foot too. On foot we always
allow it. This is player behaviour, outside this audit.

## Feel checks only Maxwell can make

1. Stunt Plane in first person: loop and roll. The view should turn over
   with the cockpit. Moving the mouse up should now raise the nose.
2. In any seat in first person, move the mouse up and down. The view nudges
   and springs back to the seat within about a tenth of a second. Hold free
   look (Z) to look round and up, then let go: it eases back.
3. Tilt a Jeep on a slope in first person, as driver and as a side passenger:
   the horizon tilts with the seat.
4. As a passenger in third person, and as the Tank's gunner in third person:
   the camera should sit behind the vehicle like the driver's.
5. Get out of a Jeep lying on its side: you should step out of the upper
   side. Under a low ceiling you get out to one side instead of being stuck.
6. Press Next Seat on foot: nothing should appear in the chat.
