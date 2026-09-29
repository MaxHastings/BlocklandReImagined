# Vehicle behaviour against v20: checklist

Scope: every stock vehicle (Jeep, Flying Wheeled Jeep, Tank and its turret,
Magic Carpet, Ball, Horse, Rowboat, Pirate Cannon, Skis and the tumble body)
and the Stunt Plane Add-On.

This builds on `vehicles.md` (items 1 to 52), `skis-v20.md` and
`v20-behaviour.md`, and on the Torque walkthrough in
`vehicles-torque-audit.md`, which also lists the sources.

v20 was not measured headless. The disputed behaviour needs a connected
client: the first-person camera, the control object and client prediction.
A dedicated server with bots cannot exercise any of these, so those items
stand on the exe and the Torque source.

## History

- **First pass, 2026-09-29, branch `claude/vehicle-v20-audit-mg3f1h`.**
  Maxwell reported three things:
  - The mouse is inverted in vehicles.
  - The Stunt Plane's first-person view stays level through a loop.
  - "Many other little things."
- **Second pass, 2026-09-29, branch `claude/stunt-plane-controls`.**
  Maxwell tested v0.1.2 and reported three more:
  - The Stunt Plane felt like "two systems conflicting".
  - A Jeep passenger was frozen in place.
  - The driver could not look up and down without Z.

- **Third pass, 2026-09-29, branch `claude/vehicle-controls-r3`.**
  Maxwell tested v0.1.3 and reported four things:
  - The plane's mouse up and down felt inverted.
  - The first-person camera shook while steering the plane.
  - A Jeep driver's mouse should steer like A/D.
  - Passengers could not turn left or right.

  Findings:
  - **Pitch.** Measured through the app, mouse up raised the nose with the
    default then (invert off). That is not a sign bug but the default:
    - v0.1.1 and earlier had invert on (stock v20).
    - v0.1.2 and v0.1.3 had it off (the reference install).
    - This pass turns it back on.

    Options' Invert Mouse In Vehicles flips it both ways (app test).
  - **Shake.** The host ran all queued moves of a seated player in one tick,
    so moves arriving two to a datagram put the host's vehicle a tick out
    of step with the client's prediction, and every pose corrected the
    view. Seated moves now run one per tick.
  - **Jeep driver.** The mouse steers with `UseStrafeSteering` off, which
    is the reference install's and Maxwell's saved v20 value; that is now
    the default. The Tank steers by the mouse too.
  - **Passengers.** A passenger's mouse turns the whole body on the seat
    (`mRot.z`; `Player::setPosition` 0x5a6bc0). The second pass had it
    locked.

  The second pass found the first pass had read two Player fields the wrong
  way round. It took +0x658 (`mMount.object`) to be the control object and
  +0x864 (`Player::mControlObject`) to be the mount (see the Torque audit).
  That gave passengers a control object they never have. The rows marked
  **Corrected** below were wrong in the first pass, and each says why.

## Marks

- **Verdict:**
  - **Matches**: already agrees with v20.
  - **Fixed**: changed, with a test.
  - **Corrected**: the first pass's verdict was wrong and is fixed now.
  - **Differs (accepted)**: an accepted difference from v20.
- **Basis:**
  - **Confirmed** names what settles it: an exe address, a Torque line, a
    datablock value or a test.
  - **Inferred** is reasoning from those without a direct reading.

## Seats: how the mouse and camera work

| Seat | Mouse pitch | Mouse turn | Returns | Third person |
|---|---|---|---|---|
| Passenger (every non-driving seat of a vehicle, sitting or standing) | pitches the head freely | turns the whole body on the seat, a full circle; Free Look turns only the head, up to 3 rad | the head's turn eases back after Free Look, halving every 32 ms; the pitch and the body's turn stay | own player camera round the seat |
| Jeep and Tank driver, `UseStrafeSteering` off (the default) | steers the vehicle's pitch (nothing on a car) and tips the head, which springs back in first person | steers; A/D do nothing | as a mouse driver | the vehicle's chase camera |
| Jeep and Tank driver, `UseStrafeSteering` on | pitches the head freely | turns the head up to 3 rad, no Z needed; A/D steer | nothing returns | the Jeep's chase camera, swung by the head's turn |
| Stunt Plane, Flying Wheeled Jeep, Magic Carpet and skis driver (mouse steering) | steers, and tips the head slightly | steers; Free Look moves the head | in first person the head springs back (the tip within about a tenth of a second, Free Look after release); in third it stays | the vehicle's chase camera, swung by the head's turn |
| Tank gunner | pitches the barrel and head | turns the turret | nothing | the turret's own player camera |
| Horse, Rowboat, Cannon rider | pitches | turns the mount | nothing | the mount's own player camera |
| Rowboat passenger | as a passenger | as a passenger | as a passenger | own player camera |

The basis is `Player::processTick` 0x5b2c81 and 0x5b2d7a, `Player::updateMove`
0x5ae972 to 0x5aeb27, `Player::getCameraTransform` 0x5ab80e, Torque's
player.cpp:1972 and :2523, and `Armor::onMount`. All of these are confirmed
(exe and Torque).

## Checklist

### Controls

| Item | v20 | Verdict | Basis |
|---|---|---|---|
| Invert Mouse In Vehicles default | Stock v20 ships `VehicleMouseInvert = 1`; the reference install and v21 ship 0. `pitch()` uses it only while driving a mouse-steered vehicle without Free Look | On, as stock v20 (third pass; the first pass had turned it off, and Maxwell's v0.1.3 test called that inverted). The toggle works both ways | Confirmed: `pitch()`, defaults files; app test `invert_mouse_in_vehicles_turns_the_nose_both_ways...` |
| Invert sign per seat | `VehicleMouseInvert` for mouse drivers; `MouseInvert` for everyone else and during Free Look | Matches | Confirmed: `pitch()`; `controls.rs` test |
| Mouse steering | `mSteering` accumulates move yaw and pitch, clamped to `maxSteeringAngle` | Matches | Confirmed: vehicle.cpp:1058, 0x56b590; `flying_jeep.rs` |
| Strafe steering | A held strafe key adds `steeringStrafeSteeringRate` per tick | Matches | Confirmed: 0x5b2e97; `vehicles.md` 10 |
| Auto-return steering | Default on; the Stunt Plane's `steeringAutoReturn` is not a real field | Matches | Confirmed: 0x570c4a; exe field names |
| Free look while mouse steering | The vehicle gets no yaw or pitch | Fixed (first pass) | Confirmed: 0x5b2df7; test `free_look_in_a_mouse_steered_vehicle...` |
| Mouse look as a passenger | The whole move reaches the player: pitch moves the head freely; the turn moves `mRot.z`, which turns the whole body on the seat (`Player::setPosition` 0x5a6bc0, Torque `setPosition`); Free Look turns only the head | **Corrected** twice: the first pass sprang the head back; the second locked the body's turn. No per-seat rule exists: sitting and standing (bumper) seats behave alike | Confirmed: 0x5b2c81, 0x5aeacd, 0x5a6bc0; tests `a_passenger_turns_on_the_seat...` (controls and session). The absence of a per-seat rule is inferred |
| Mouse look as a Jeep or Tank driver | The move goes down the free-look path: the mouse turns and pitches the head without Z | **Corrected**: the first pass needed Z and sprang back | Confirmed: 0x5b2d15 to 0x5b2d7a; test `a_strafe_driver_looks_round...` |
| Mouse look as a mouse driver | The mouse steers; the head also takes the move's pitch (0x5b2cd4) and halves it back every tick in first person (0x5aeae3): a slight tip of the view with each mouse move | Matches: the tip is kept. It fought the plane only while the plane answered a round trip late; the driven vehicle is now predicted and answers on the same tick | Confirmed: 0x5b2cd4, 0x5aeae3; test `a_mouse_driver_steers...` |
| Rider's body pitch seen by others | A mouse driver's head pitch returns to centre in first person | Fixed: the host posed the pilot's arms from the steering accumulator, which flips every half turn | Confirmed (first person, 0x5aeaed); inferred for third person |
| Brake, jet, crouch | Jump brakes, jet leaves, crouch reaches neither | Matches | Confirmed: 0x5b03d8; `vehicles.md` 32 |
| Tools while seated | The rider keeps fire | Matches | Confirmed: 0x5b2cd4; `vehicles.md` 31 |
| Next and previous seat | Nothing happens on foot or with no free seat | Fixed (first pass) | Confirmed: `serverCmdNextSeat`; `hardening_session` |
| `UseStrafeSteering`, `UseAutoReturnSteering` defaults | Stock v20 and v21: 1; the reference install and Maxwell's saved v20 prefs: 0 | 0 (third pass): the Jeep's and Tank's drivers steer with the mouse, as Maxwell expects | Confirmed: defaults files, `config/client/prefs.cs`; session test `the_jeep_steers_by_the_mouse...` |
| Seated moves per tick | The engine runs one move per tick for every player | Fixed (third pass): the host drained a seated player's whole queue each tick, so a predicting driver was corrected every pose (the first-person shake) | Confirmed: session test `a_predicted_driver_needs_no_corrections_when_moves_arrive_in_pairs` (before: 0.33 units and 0.009 rad per pose; after: none) |

### Cameras

| Item | v20 | Verdict | Basis |
|---|---|---|---|
| First-person view in a vehicle seat | The seat's rotation times the head: rolls and pitches with the vehicle | Fixed (first pass) | Confirmed: 0x5aafa0, 0x5ac506; `vehicle_first_person` test |
| First-person head return | Passengers: the turn returns after Free Look. Mouse drivers: the head returns in first person only. Strafe drivers: never | **Corrected**: the first pass sprang every seat back | Confirmed: 0x5aeae3 to 0x5aeb27; `controls.rs` tests |
| Tank gunner first person | Rides the hull, turned by the aim, pitch free | Fixed (first pass) | Confirmed: 0x5ab0d0 (own transform times head); test |
| Player-type mount riders, first person | Upright, no spring | Matches | Confirmed: mount type 0x4000 skips the halving |
| First-person eye position | Vehicle riders: through the seat. Player-mount controllers: the mount node plus the eye | Fixed (first pass) | Confirmed: 0x5ab84d; `vehicle_first_person` |
| Driver third person | Level chase camera; `cameraMaxDist`, `cameraOffset`, `cameraTilt`; swung by the head's turn | Fixed: it swung only while Z was held; now a Jeep driver's mouse orbits it | Confirmed: 0x56cc10; datablocks |
| Passenger third person | Own player camera (no control object) | **Corrected**: the first pass gave them the vehicle's chase camera | Confirmed: 0x5ab80e; `vehicle_first_person` checks it is not the chase camera |
| Gunner third person | The turret's own player camera | **Corrected**: the first pass gave the Tank's chase camera (the turret has no control object of its own) | Confirmed: 0x5ab80e |
| Chase camera free look source | v20 uses the newest rider's head for the vehicle's camera | Differs (accepted): each driver's own head | Confirmed: 0x56cc10 mount-list loop |
| Camera roll in third person | `cameraRoll` off everywhere | Matches | Confirmed: datablocks |
| Field of view | Horizontal | Matches | Confirmed: `vehicles.md` 34 |

### Prediction and replication

| Item | v20 | Verdict | Basis |
|---|---|---|---|
| The vehicle a client drives | Predicted: its moves run on the client and are corrected from the server | Fixed: the host's own vehicle code runs in the client's collision mirror and replays after each pose; the plane answers the mouse on the tick | Confirmed: vehicle.cpp:801/1549/1565; `vehicle_prediction` test |
| Drawn between ticks | Interpolated between ticks | Matches | Confirmed: vehicle.cpp:866; inferred correction rate |
| A driven vehicle that cannot be predicted | Not applicable | Its rotation is now extrapolated by its spin, as its position is | Inferred |
| Player-type mounts | Predicted as players | Differs: not predicted yet | Inferred low impact |

### Mounting and dismounting

| Item | v20 | Verdict | Basis |
|---|---|---|---|
| Boarding | Land on top; first free mount node | Matches | Confirmed: `Armor::onCollision`; `vehicles.md` 8 |
| Seat facing | The body takes the mount transform | Matches | Confirmed: `vehicles.md` 1, 35 |
| Look limits | `setLookLimits` clamps only the arms' pose | Matches | Confirmed: 0x5a53b0 |
| Mount sound | `playerMountSound`; no dismount sound | Matches | Confirmed: `Armor::onMount` |
| Dismount points | 2.2 up the tilted seat first, then the world axes | Fixed (first pass) | Confirmed: `Armor::doDismount`; `native.rs` |
| Dismount when blocked | Out at the last point tried, no push | Fixed (first pass) | Confirmed: `Armor::doDismount`; `native.rs` |
| Velocity carried | The vehicle's velocity plus the push, no spin | Fixed (first pass) | Confirmed: `Armor::doDismount`; `native.rs` |
| `doSimpleDismount` | Skis and the tumble body | Matches for stock | Confirmed: datablocks |
| Switching seat | The next free seat | Matches | Confirmed: `serverCmdNextSeat` |

### Respawn, damage and wrecks

| Item | v20 | Verdict | Basis |
|---|---|---|---|
| Respawn time | `$Game::MinVehicleRespawnTime` 0 | Differs (accepted): 1 s floor | Confirmed: scripts |
| Crash damage | `collDamage*` never applied | Matches | Confirmed: progress.md (protocol 39) |
| Damage scaling, passenger protection | `VehicleDamageScale`, `protectPassengers*` | Matches | Confirmed: `vehicles.md` 18, 19 |
| Explosions | Initial and final, `impulseVertical` | Matches | Confirmed: `vehicles.md` 20 |
| Burn emitter on a wreck | `damageEmitter` | Matches | Confirmed: datablocks |
| Ski wreck and tumble | `onWreck` | Matches | Inferred trigger (`vehicles.md` 13) |
| Run over, click to flip | `minRunOverSpeed`, `Vehicle::onActivate` | Matches | Confirmed: `vehicles.md` 15, 16 |
| Vehicle limits | 10 physics, 150 player | Matches | Confirmed: `server/defaults.cs` |
| Map vehicle spawns on internet hosts | No cap | Differs (accepted): 5 | Confirmed |

### Wheels, flight and water

| Item | v20 | Verdict | Basis |
|---|---|---|---|
| Tire model | Torque tire springs and relaxation | Differs (accepted): Rapier raycast vehicle | Confirmed: wheeledVehicle.cpp:849 |
| Steered wheels | 3 wheels: only wheel 0; the Tank's `onAdd` steers four | Matches | Confirmed: `WheeledVehicleData::onAdd`, 0x571e34 |
| Flying wheeled forces | Thrust, capped lift, bite, surfaces | Matches | Confirmed: 0x5746a0; `flying_jeep.rs` |
| FlyingVehicle | Stock Torque, sphere inertia | Matches | Confirmed: flyingVehicle.cpp:479, vehicle.cpp:915 |
| Jets | The rider's jet leaves | Matches | Confirmed: 0x5b2cad |
| Water | Buoyancy and drag by coverage | Matches | Confirmed: `native.rs` |
| Splash, dust, impact sounds | By datablock | Matches | Confirmed: datablocks |
| Engine sounds | None on stock datablocks | Matches | Confirmed: datablocks |
| Propeller and contrails | `playThread` by speed; contrail images | Matches | Confirmed: Stunt Plane script |

### HUD

| Item | v20 | Verdict | Basis |
|---|---|---|---|
| Vehicle HUD | None | Matches | Confirmed: client scripts |
| Whiteout on a ski crash | `setWhiteout` | Matches | Inferred fade rate |

## Noticed outside vehicles (not changed)

On foot, v20 allows free look only in third person. The free-look test is
"mounted, or not first person" (0x5aea73), and a player on foot has no mount.
We allow it in first person on foot too. This is player behaviour, outside
this audit.
