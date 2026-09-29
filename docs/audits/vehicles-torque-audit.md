# Vehicles and riders against the Torque engine

Date: 2026-09-29. Branch `claude/stunt-plane-controls`. This walks the Torque
engine behaviour that touches vehicles and their riders, and checks each item
against Blockland v20 where Blockland changed the engine.

It was written after Maxwell's v0.1.2 test:
- The Stunt Plane felt as if "two systems are conflicting".
- A Jeep passenger was "frozen in place".
- The driver could not look up and down without Z.

The per-vehicle checklist is `vehicles-v20-checklist.md`.

## Sources and how items are marked

- **Torque** is Torque3D at the pinned research commit d0de864 (MIT). Files
  under `Engine/source/T3D/` were read through the GitHub API into a scratch
  folder: `player.cpp`, `vehicles/vehicle.cpp`, `vehicles/flyingVehicle.cpp`,
  `vehicles/wheeledVehicle.cpp`, `shapeBase.cpp` and
  `gameBase/gameConnection.cpp`. Torque3D descends from TGE 1.x, which v20
  is built on. Line numbers refer to that commit.
- **Exe** means blocklandv20.exe from the designated reference install,
  disassembled for this audit. Blockland changed several of the functions
  below, and where the two disagree the exe wins.
- **Datablock** values come from the reference install's Add-On scripts. The
  Stunt Plane's come from Maxwell's archive copy.
- Nothing from these sources is committed. Only addresses, line numbers and
  our own descriptions are recorded here.

Every row says how it was established:
- **Confirmed**: an exe address, a Torque line or a datablock value, plus the
  test that pins our side.
- **Inferred**: reasoned from those sources without a direct reading.

v20 was not measured headless. The disputed behaviour needs a connected
client: the first-person camera, the control object and client prediction.
A dedicated server with bots cannot exercise any of these, so those items
stand on the exe and the Torque source.

## Field offsets that matter

The earlier audit read two Player fields the wrong way round. Offsets are
from `ShapeBase::mountObject` (0x5bf330) and `Player::getCameraTransform`
(0x5ab7d0):

| Offset | Field | Evidence |
|---|---|---|
| +0x658 | `mMount.object`, what the player sits on | written by `ShapeBase::mountObject` (0x5bf379), with +0x660 the node and +0x65c the list link |
| +0x864 | `Player::mControlObject`, what the player drives | compared with +0x658 in `getCameraTransform` (0x5ab880); the move split in `processTick` (0x5b2c81) is gated by it |
| 0x4000 / 0x10000 | `PlayerObjectType` / `VehicleObjectType` | `$TypeMasks` registration (0x59b65c) |

`Armor::onMount` calls `setControlObject(%vehicle)` for the driver and
`setControlObject(%obj)` for everyone else. Torque's
`Player::setControlObject` (player.cpp:1972) stores nothing when given the
player itself, so **passengers have no control object**. That is the fact
the earlier audit missed.

## Walkthrough

### Control objects and the move

| Behaviour | Torque | Blockland v20 | Ours | Verdict |
|---|---|---|---|---|
| Who gets the move | `Player::processTick` (player.cpp:2029): with a control object, the object gets the move, and the player gets a null move plus yaw/pitch/roll only while free looking | Same gate (0x5b2c81). The rider also keeps fire, jet and pitch (0x5b2cd4), and a mounted rider always keeps trigger 0 | Server `Session` step: seated riders' moves go to `vehicle_input` by seat role | Matches. Confirmed (exe, `vehicles.md` 31) |
| Strafe-steered driver | Not in Torque | 0x5b2d15 to 0x5b2d7a: with `$pref::Input::UseStrafeSteering` (connection +0x368), a vehicle with `steeringUseStrafeSteering` (default on, 0x5716ec) and a control object of that vehicle, the rider's move goes down the free-look path: the head takes yaw and pitch, and the vehicle gets none but the strafe-key rate (0x5b2e97). With the pref off the vehicle gets the mouse (Torque `Vehicle::updateMove`) and the strafe keys do nothing | `SeatLook::StrafeDriver` with the pref on; mouse driver with it off, the default now (the reference install's) | **Fixed**. Confirmed (exe; session test `the_jeep_steers_by_the_mouse...`) |
| Moves per tick | `GameConnection` hands each player one move per tick | Inherited | The host ran every queued move of a seated player in one tick | **Fixed**: one per tick, as for walkers. Confirmed (session test: prediction corrections gone) |
| Head update | `Player::updateMove` (player.cpp:2523): pitch is added and clamped to `min/maxLookAngle`. Free look (mounted on node 0, or third person) turns `mHead.z` up to `maxFreelookAngle`. Otherwise `mHead.z` halves each tick, and `mHead.x` halves only with a control object | 0x5ae972: free look is allowed for any mounted player (0x5aea73 tests `mMount.object`). The `mHead.x` halving is narrowed to a control object of `VehicleObjectType` **in first person** (0x5aeae3 to 0x5aeb13). `mHead.z` halves in the else branch, but not for a vehicle's driver in third person | `Controls::look` / `advance_head` per `SeatLook` | **Fixed** (was: every seated head sprang back in first person). Confirmed (exe, Torque) |
| Passenger | No control object: the whole move reaches the player, so pitch moves `mHead.x` and yaw moves `mRot.z`. A mounted player's transform is the mount node's times `rotZ(mRot.z)` (player.cpp `setPosition`) | Same: `updateMove` adds the turn to `mRot.z` (0x5aeacd) and `Player::setPosition` (0x5a6bc0) turns the mount transform by it | `SeatLook::Passenger`: the mouse turns the body on the seat (sent relative to it; the host's `follow_seats` adds the seat's heading) and pitches the head; Free Look turns only the head | **Fixed** (third pass; the second pass had locked the body). Confirmed (exe, Torque) |
| Mouse-steered driver | The vehicle accumulates `move->yaw/pitch` into `mSteering`, clamped to `maxSteeringAngle` (vehicle.cpp:1058) | Same (`Vehicle::updateMove` 0x56b590). The rider keeps the move's pitch (0x5b2cd4) and its head halves it back every tick in first person (0x5aeae3): a slight tip of the view | `SeatLook::MouseDriver`: the mouse steers, and the head tips and springs back | Matches. Confirmed (`flying_jeep.rs`, `a_mouse_driver_steers...`). The tip fought the plane only while the plane answered a round trip late; it is predicted now |
| Rider's body pitch on the host | A controlling player's `mHead.x` returns to centre (player.cpp:2544) | Only in first person for vehicles | The host keeps a mouse driver's body pitch at 0. Before, it took the steering accumulator, which wraps every half turn | **Fixed**: other players saw the pilot's arms swing and flip. Inferred for third person, where v20's arms would follow the head |
| Auto-return steering | `WheeledVehicleData` constructor defaults. Torque has no auto-return | `WheeledVehicle::updateMove` (0x570c4a) | `VehiclesWorld::pre_step` | Matches. Confirmed (`vehicles.md` 11) |

### Cameras and eyes

| Behaviour | Torque | Blockland v20 | Ours | Verdict |
|---|---|---|---|---|
| Who owns the third-person camera | `GameConnection::getControlCameraTransform` (gameConnection.cpp:590) asks the connection's camera object, the player | `Player::getCameraTransform` (0x5ab80e) hands a player **with a control object** to that object's camera | `App::view_camera`: drivers get the vehicle's chase camera; Actor and Gunner get their player-type mount's camera; passengers keep their own player camera | **Fixed** (the audit had given passengers and the gunner the vehicle's camera). Confirmed (exe) |
| Vehicle chase camera | `Vehicle::getCameraTransform` (vehicle.cpp:949) orbits by the vehicle's own eye | Blockland rewrote it (0x56cc10): the first mounted Player's `mHead` turns it (`rotX(head.x + cameraTilt)·rotZ(head.z)` about the vehicle), then it is levelled while `cameraRoll` is off | `vehicle_camera::driver_view`, swung by `Controls::driver_head_yaw`, so a Jeep driver's mouse orbits it | **Fixed** (was: only while Z was held). Confirmed (exe). Uses the local rider's head rather than the newest rider's: differs (accepted) |
| First-person eye of a vehicle rider | `Player::getRenderEyeTransform` (player.cpp:5335): the rider's transform times the head, at the `eye` node | Same (0x5aafa0). Reached for every vehicle rider (0x5ac506) | `App::rider_eye` through the seat, `Ride::Seat` rotation | Matches. Confirmed (exe; `vehicle_first_person` test) |
| First-person eye of a player-mount rider | Not in Torque | With a control object of `PlayerObjectType` that it sits on: the mount node plus the eye, in the mount's frame (0x5ab84d) | `vehicle_camera::driver_eye` for Actor seats | Matches. Confirmed (exe) |
| View rolls with the seat | Follows from `getRenderEyeTransform` | Same | `Controls::ride_view`, `rolled_view_basis` | Matches. Confirmed (tests `a_seated_first_person_view_rolls...`, `vehicle_first_person`) |
| `cameraRoll`, `cameraMaxDist`, `cameraOffset`, `cameraTilt` | Datablock fields | `cameraRoll` false on every stock vehicle and the Stunt Plane; the Stunt Plane has `cameraMaxDist` 13, offset 7.5, tilt 0.4 | Pack `camera` | Matches. Confirmed (datablocks) |
| `cameraLag`/`cameraDecay` | `Vehicle::advanceTime` (vehicle.cpp:884) trails the camera | Not read by 0x56cc10 | Not used | Matches. Confirmed (exe) |
| Look limits while seated | Not in Torque | `setLookLimits` clamps only the arms' `look` thread (0x5a53b0) | Avatar `look_position` | Matches. Confirmed (`vehicles.md` 25) |
| Free-look range | `maxFreelookAngle` | 3 rad (PlayerStandardArmor) | `MAX_FREELOOK` | Matches. Confirmed (datablock) |

### Prediction and reconciliation

| Behaviour | Torque | Blockland v20 | Ours | Verdict |
|---|---|---|---|---|
| The controlled vehicle | The client runs `Vehicle::processTick` with its own moves (vehicle.cpp:801). `writePacketData`/`readPacketData` (vehicle.cpp:1549/1565) send the control object's full state, and the client replays its moves from it | Inherited | `Predictor::drive`/`drive_pose` (bri-sim) run the host's own `VehiclesWorld` on the collision mirror, one input per 120 Hz tick. Each newer `VehiclePose` restores the body, spin, steering and wheels, then replays the moves after `driver_input` | **Fixed**: the driven vehicle used to be drawn at its last pose, a round trip late, so the plane answered the mouse late. Confirmed (`vehicle_prediction` test: within 0.00002 of the host under a 100 ms round trip) |
| Drawing between ticks | `Vehicle::interpolateTick` (vehicle.cpp:866) | Inherited | `Motion::driven_frame`: between the last two predicted ticks, with corrections fading at 14/s | Matches. Inferred rate (the player's) |
| Other vehicles | Interpolated ghosts | Inherited | Interpolated 9 ticks behind; the driven vehicle's extrapolation now also carries its spin when not predicted | Matches |
| Player-type mounts (horse, cannon, turret) | Players predicted like players | Inherited | Show the host's pose | Differs: not predicted yet. Inferred low impact (slow, ground-bound) |

### Physics, mounting, dismount

| Behaviour | Torque | Blockland v20 | Ours | Verdict |
|---|---|---|---|---|
| Rigid body inertia | `mRigid.setObjectInertia()` ignores `massBox` (vehicle.cpp:915) | FlyingVehicle keeps it | Pack `inertia_box`, sphere for FlyingVehicle | Matches. Confirmed (`vehicles.md` 9) |
| FlyingVehicle forces | `FlyingVehicle::updateForces` (flyingVehicle.cpp:479) | Stock (0x568770) | `world.rs` flight | Matches. Confirmed (`vehicles.md` 40) |
| Flying wheeled forces | Not in Torque | `WheeledVehicle::updateForces` (0x5746a0) | `wheeled_flight` | Matches. Confirmed (`flying_jeep.rs`) |
| Tire model | `WheeledVehicle::updateForces` (wheeledVehicle.cpp:849) | Inherited | Rapier raycast vehicle | Differs (accepted, `vehicles.md` 28) |
| Mounting | `ShapeBase::mountObject` | Plus `Armor::onCollision` boarding | `vehicle_contacts` | Matches (`vehicles.md` 8) |
| Dismount | Script | `Armor::doDismount` | `VehiclesWorld::dismount` | Matches. Confirmed (`native.rs` dismount tests) |
| Seat switching | Script | `serverCmdNextSeat` | `switch_seat` | Matches (`hardening_session`) |

## Protocol

- `VehiclePose` gains `angular_velocity`, `mouse_steering` and
  `driver_input`: 28 bytes per vehicle pose. Only the driver needs them, but
  poses go to everyone.
- `VehicleInfo` gains `scale` (reliable, on change).
- No new messages and no per-tick traffic for cosmetic things.
