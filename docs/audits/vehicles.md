# Vehicle audit against v20 and Torque

Date: 2026-09-27. Scope: everything that rides or is ridden in v20. That covers
WheeledVehicle (Jeep, Tank, Flying Wheeled Jeep, Ball, skis and the tumble
body), FlyingVehicle (Magic Carpet), HoverVehicle (none ship in v20),
players mounted on vehicles and turrets, and rideable PlayerData (Horse,
Rowboat, Pirate Cannon, Tank Turret).

## Sources

- v20 add-on scripts, read from the designated read-only reference
  (`Vehicle_*`, `Item_Skis`, `Weapon_Horse_Ray` ZIPs).
- Recovered v20 core scripts: `.research/bl-decompiled/v20/server/scripts/allGameScripts.cs`
  (`Armor::onMount`, `Armor::doDismount`, `Armor::onCollision`,
  `WheeledVehicleData::onCollision`, `WheeledVehicleData::Damage`,
  `Armor::Damage`, `Vehicle::onActivate`, `ProjectileData::radiusImpulse`,
  `fxDTSBrick::vehicleMinigameEject`, `MiniGameSO::addMember`) and the v20
  client defaults (`$pref::Input::UseStrafeSteering = 1`,
  `$pref::Input::UseAutoReturnSteering = 1`, `$Pref::Input::VehicleMouseInvert = 1`).
- Torque3D at the pinned `.research/torque-reference` commit (d0de864) for the
  engine side v20 inherits: `vehicle.cpp` (`updateMove`, inertia),
  `flyingVehicle.cpp` (`updateMove`, `updateForces`, `getHeight`),
  `wheeledVehicle.cpp`, `hoverVehicle.cpp`, `player.cpp` (mounted look and
  `setPosition`), `rigid.cpp` (inertia, integration).

Blockland's own engine changes (strafe steering, the "flying wheeled" forces
on WheeledVehicle, `onWreck`) are closed. Where the audit relies on them it
says so and names the evidence.

## Findings

Status: **Fixed** is on main with a test or a stated reason; **Handed off**
belongs to another thread by coordinator decision; **Open** is not done.

| # | Area | v20 behavior | What we had | Status |
|---|------|--------------|-------------|--------|
| 1 | Seat facing | Every mounted player takes the mount node's transform, so the body and first-person yaw stay fixed to the seat for drivers and passengers alike; the mouse only tilts the view, and free look turns the head within `maxFreelookAngle` | The local driver's body followed the camera, so it spun in the seat; later passengers could still turn in their seat | Fixed (2026-09-27 correction: passengers are locked too, per Maxwell's v20 check) |
| 2 | View while riding | The vehicle camera sits behind the vehicle and turns with it | The camera stayed at its world yaw while the vehicle turned | Fixed: the view carries the vehicle's turn; mouse-steered vehicles are followed |
| 3 | Tank gunner aim | The gunner controls the TankTurretPlayer: its yaw is relative to the hull and follows the mouse; barrel pitch limited by `minLookAngle -1.5708` / `maxLookAngle 0.5` | Aim yaw was the absolute camera yaw with the wrong sign, so the turret swung the opposite way and drifted as the hull turned; no pitch limit | Fixed |
| 4 | Tank Turret on its own | `TankTurretPlayer` has `uiName "Tank Turret"` and `rideable`, so it is in the wrench vehicle list | Filtered out of the list | Fixed |
| 5 | Standalone turret and cannon turning | The gunner's mouse turns the whole PlayerData (`mRot.z`) | Only the seat pose rotated; the model never turned | Fixed: the body faces the look |
| 6 | Horse movement | PlayerData: player movement with `runForce 28*90`, speeds 12/6/1, `jumpForce 17*90`, step-up, `runSurfaceAngle 85`, mouse turns it | A friction box with no step-up, turned by A/D; any plate edge stopped it | Fixed (kinematic player motor), then handed off: the Gameplay leftovers thread owns per-datablock player types |
| 7 | Rowboat, cannon, turret bodies | PlayerData like the horse | Dynamic boxes with locked rotation | Fixed with the same motor as 6 |
| 8 | Boarding | You board only by landing on a vehicle: feet 0.2 above its origin (`colZ > objZ + 0.2`), then the first free mount node, whatever you touched | Any touch boarded, and seats were chosen by distance, so jumping on a jeep's back put you in the rear seat | Fixed |
| 9 | Magic Carpet steering | FlyingVehicle: `mSteering` accumulates mouse turns up to `maxSteeringAngle 0.785`, damped by `autoInputDamping` below `maxAutoSpeed`; A/D is maneuvering thrust; inertia is Vehicle's unit sphere (0.4 mass) because FlyingVehicle ignores `massBox` | A/D fed steering; inertia came from the shape box, about ten times too large, and with `rotationalDrag 20` it could barely turn: it flew straight | Fixed |
| 10 | Mouse vs strafe steering | Wheeled vehicles steer with the strafe keys by default; `steeringUseStrafeSteering = false` (Flying Wheeled Jeep, skis) makes the mouse steer and pitch | Every vehicle used A/D; pitch control was never sent | Fixed: new `strafe_steering` field, mouse steering for FW Jeep, skis and carpet |
| 11 | Steering auto-return | `UseAutoReturnSteering` defaults on | Not modeled for mouse steering | Fixed; the return rate (halves every quarter second) is inferred, the engine value is closed |
| 12 | Skis | Using the item spawns an invisible `skiVehicle` at the feet with the player's velocity and mounts it 250 ms later; firing again steps off; `LSki`/`RSki` nodes show | The ski events were dropped ("Weapon integration pending") | Fixed |
| 13 | Ski crash and tumble | `onWreck` throws the skier onto a `deathVehicle` under the corpse camera until it settles | Not wired | Fixed; the wreck trigger (contact with a 20 m/s change or ending on its side) is inferred, `onWreck` is engine-side |
| 14 | Tackle tumble | The football `Tumble` uses the same `tumble()` | Dropped | Fixed |
| 15 | Run over | Touching a player without boarding: faster than `minRunOverSpeed` (clamped to at least 2, plus 2 with no driver) does speed × `runOverDamageScale` damage; the player's velocity is set to the vehicle's × `runOverPushScale`, if the minigame allows damage | `player_contact` existed but nothing called it | Fixed |
| 16 | Click to flip | `Vehicle::onActivate`: a vehicle slower than 2 you may use gets an impulse of 5 × mass along your look plus up | Missing | Fixed |
| 17 | Turret hits | The Tank's turret takes hits with its own 250 health; losing it drops the gunner to the hull | `damage_turret` existed but hits always went to the hull | Fixed: hits nearer the turret collider go to it |
| 18 | Damage type scaling | `$Damage::VehicleDamageScale` (Gun 0.2, Akimbo 0.05, Arrow 0.5, Sword 0.75, Hammer 0) | Ignored | Fixed: weapons-pack-008 carries `vehicle_scale` |
| 19 | Passenger protection | `protectPassengersDirect/Radius/Burn` (Jeep radius; Tank all three) | Flags imported, never applied | Fixed |
| 20 | Explosion upward kick | `impulseVertical` (2000 on tank, jeep and cannon explosions) | Not converted | Fixed: weapons-pack-008 `impulse_vertical`, applied in radius impulse |
| 21 | Minigame cleanup | Joining, leaving or resetting off LAN runs `ClearEventSchedules` and `resetVehicles`; the owner's spawn bricks eject riders who may no longer use them | Both effects ignored | Fixed: event schedules, vehicle reset, ejection and the owner's event-spawned projectiles |
| 22 | Horse animation | Run, back, side, jump, root DSQ threads | Server emits them; the client draws the horse's rest pose | Handed off |
| 23 | Horse Ray | Turns a player into a rideable horse others can board | Event dropped | Handed off |
| 24 | Barrel pitch visual | Turret and cannon barrels tilt with the gunner's look | Aim pitch was replicated but no node was posed | Fixed: the authored `look` clip poses the barrel |
| 25 | Mounted look limits | `setLookLimits(up, down)` stores two fractions in [0, 1] (Player +0x898/+0x89c, down clamped to at most up). `Player::updateLookAnimation` (0x5a53b0) clamps only the arm `look` thread position, `(mHead.x + pi/2) / pi`, to `[down, up]`; the view pitch keeps the full `minLookAngle`/`maxLookAngle` range | First pass clamped the camera pitch to the band, so riders could barely look up or down | Fixed: the arms' look pose is clamped, the view is free; the Tank gunner takes TankTurretPlayer's limits |
| 31 | Tools while seated | `Player::processTick` (0x5b2cad) splits a mounted controller's move: the rider keeps fire (trigger 0), jet (trigger 4) and pitch; the vehicle gets the rest minus fire, crouch and jet. Passengers keep full control of themselves. So every rider uses tools; only the Tank turret and pirate cannon packages turn fire into the gun and `ServerCmdUnUseTool` | Seated fire always went to the vehicle weapon, so tools did nothing | Fixed |
| 32 | Leaving and braking | The rider's jet calls `doDismount` (0x5b03d8; the Tutorial says "get out of the Jeep by pressing Jet"); jump reaches the vehicle, where WheeledVehicle brakes on trigger 2 and the horse jumps; crouch reaches neither | Jump left the vehicle, crouch braked, crouch left the horse, jet reached vehicles | Fixed |
| 33 | Dismount sound | `Armor::onMount` plays `playerMountSound`; `doDismount` and `onUnMount` play nothing, and v20 ships no dismount sound file | Silent | Matches v20 |
| 34 | Field of view | Torque's FOV is horizontal (`GuiTSCtrl::processCameraQuery`) | The renderer used 90 degrees as the vertical FOV, about 121 degrees across on 16:9, which swam when turning | Fixed |
| 26 | Vehicle camera detail | `cameraMaxDist`, `cameraOffset`, `cameraTilt`, `cameraLag` | Only `cameraMaxDist` | Fixed: pivot height, tilt and lag from vehicles-pack-011 |
| 27 | Whiteout on ski crash | `setWhiteout(time/7000)` | Not drawn | Fixed: white flash of time/7 when a tumble starts, fading one unit per second (the fade rate is inferred) |
| 28 | Tire forces | Torque lateral/longitudinal tire springs, relaxation, anti-sway | Rapier raycast vehicle with the authored spring and friction | Accepted adaptation, feel for Maxwell's playtest |
| 29 | Flying Wheeled lift and surfaces | Blockland engine code | Approximation with the authored thrust, lift and torques | Accepted adaptation |
| 30 | HoverVehicle | No v20 content uses it | Not implemented | Not needed |

## How the fixes work

**Seat roles.** `Definition::seat_role` classifies every seat as Passenger,
StrafeDriver, MouseDriver, Actor (rider of a player-type mount) or Gunner.
The server maps input by role, every rider's body faces the seat, and the
client camera follows the role: Passenger and StrafeDriver views face the seat,
MouseDriver views follow the vehicle, a Gunner's view turns with the hull, and
an Actor's mount follows the look. Nothing in the network protocol changed.

**Mouse steering.** For a MouseDriver seat the client keeps sending the raw
mouse turn in `MoveInput.yaw/pitch` (pitch wraps every half turn instead of
clamping) and shows a view that follows the vehicle. The server turns the
difference between inputs into `Controls::look_delta`, and the vehicle
accumulates it exactly as `Vehicle::updateMove` does. With v20's default
vehicle mouse invert, moving the mouse up dips the nose.

**Player-type mounts.** Kinematic bodies moved by a character controller with
the datablock's speeds, run force, jump, step height, slope limit, drag and
buoyancy. Impulses change their velocity by impulse / mass, like
`Player::applyImpulse`.

**Data.** vehicles-pack-011 (schema 5) adds the chase camera and seated look
limits on top of vehicles-pack-010 (schema 4), which added `strafe_steering`, `look_pitch`
and `underwater_speeds` and the FlyingVehicle sphere inertia.
weapons-pack-008 (schema 3) adds `Explosion::impulse_vertical` and
`DamageType::vehicle_scale`; apart from those two fields it is identical to
weapons-pack-007.

## Verification

- `cargo test -p bri-vehicles`: 28 native tests, including the run-over
  rule, mouse-steered turning for the Flying Wheeled Jeep and two Rapier
  island guards (a spawn before an unrelated collision pass, a restore).
- `cargo test -p bri-sim --test vehicles -- --ignored`: boarding by jumping,
  horse running, jumping and dismounting, tank gunner aim relative to the
  hull, the Tank Turret on the spawn list, skis on and off, and the jeep
  drive and respawn test, and the bot-brick test (a minigame vehicle reset
  leaves bots alone, as v20's `resetVehicles` predates bots).
- `seated_riders_face_the_seat_and_use_tools_but_gunners_fire_the_gun`:
  a seated driver stays facing the seat against mouse yaw and shoots a held
  gun; the gunner's fire puts the gun away.
- None of this replaces Maxwell's interactive playtest of feel.
