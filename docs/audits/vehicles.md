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
| 1 | Seat facing | Every mounted player takes the mount node's transform, so the body and first-person yaw stay fixed to the seat for drivers and passengers alike; in first person the head springs back to the seat and free look turns it within `maxFreelookAngle` (item 42) | The local driver's body followed the camera, so it spun in the seat; later passengers could still turn in their seat | Fixed (2026-09-27 correction: passengers are locked too, per Maxwell's v20 check) |
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
| 23 | Horse Ray | Turns a player into a rideable horse others can board | Event dropped | Fixed: players ride rideable players by `Armor::onCollision`'s rules (2026-09-28, `session/riding.rs`) |
| 24 | Barrel pitch visual | Turret and cannon barrels tilt with the gunner's look | Aim pitch was replicated but no node was posed | Fixed: the authored `look` clip poses the barrel |
| 25 | Mounted look limits | `setLookLimits(up, down)` stores two fractions in [0, 1] (Player +0x898/+0x89c, down clamped to at most up). `Player::updateLookAnimation` (0x5a53b0) clamps only the arm `look` thread position, `(mHead.x + pi/2) / pi`, to `[down, up]`; the view pitch keeps the full `minLookAngle`/`maxLookAngle` range | First pass clamped the camera pitch to the band, so riders could barely look up or down | Fixed: the arms' look pose is clamped, the view is free; the Tank gunner takes TankTurretPlayer's limits |
| 31 | Tools while seated | `Player::processTick` (0x5b2cad) splits a mounted controller's move: the rider keeps fire (trigger 0), jet (trigger 4) and pitch; the vehicle gets the rest minus fire, crouch and jet. Passengers keep full control of themselves. So every rider uses tools; only the Tank turret and pirate cannon packages turn fire into the gun and `ServerCmdUnUseTool` | Seated fire always went to the vehicle weapon, so tools did nothing | Fixed |
| 32 | Leaving and braking | The rider's jet calls `doDismount` (0x5b03d8; the Tutorial says "get out of the Jeep by pressing Jet"); jump reaches the vehicle, where WheeledVehicle brakes on trigger 2 and the horse jumps; crouch reaches neither | Jump left the vehicle, crouch braked, crouch left the horse, jet reached vehicles | Fixed |
| 33 | Dismount sound | `Armor::onMount` plays `playerMountSound`; `doDismount` and `onUnMount` play nothing, and v20 ships no dismount sound file | Silent | Matches v20 |
| 35 | Rider on a slope | A mounted player's transform is the mount node's, so the rider pitches and rolls with the vehicle | Riders stayed upright and floated off a tilted seat | Fixed: the body, and the first-person eye height, follow the seat rotation; the view direction stays the rider's look |
| 34 | Field of view | Torque's FOV is horizontal (`GuiTSCtrl::processCameraQuery`) | The renderer used 90 degrees as the vertical FOV, about 121 degrees across on 16:9, which swam when turning | Fixed |
| 26 | Vehicle camera detail | `cameraMaxDist`, `cameraOffset`, `cameraTilt`, `cameraLag` | Only `cameraMaxDist` | Fixed: pivot height, tilt and lag from vehicles-pack-011 |
| 27 | Whiteout on ski crash | `setWhiteout(time/7000)` | Not drawn | Fixed: white flash of time/7 when a tumble starts, fading one unit per second (the fade rate is inferred) |
| 28 | Tire forces | Torque lateral/longitudinal tire springs, relaxation, anti-sway | Rapier raycast vehicle with the authored spring and friction | Accepted adaptation, feel for Maxwell's playtest |
| 29 | Flying Wheeled lift and surfaces | Blockland code in `WheeledVehicle::updateForces`, decoded from blocklandv20.exe (see below) | Invented model: lift grew without limit, jets pushed straight up, surfaces and stall ignored | Fixed |
| 30 | HoverVehicle | No v20 content uses it | Not implemented | Not needed |
| 35 | Barrel pitch motion | The `look` thread is set every frame to `(mHead.x + pi/2) / pi` (`Player::updateLookAnimation`, 0x5A53B0), independent of `min/maxLookAngle`, so the barrel points exactly where the gunner looks and moves continuously; the controlling client poses it from its own head pitch | Model baked at 13 poses spread over the look limits, so the barrel moved in ~10 degree steps, didn't match the aim, and waited for the server | Fixed: the barrel is drawn as its own part posed from the clip at any pitch; the local gunner's barrel follows their own look |
| 36 | Shell spawn point | Tank: `getSlotTransform(1)`, the posed `mount1` node on the barrel; Cannon: `getEyeTransform()`, the posed `eye` node. Direction is `getMuzzleVector`: with no image in the slot, Player::getMuzzleTransform (0x5A6EE0) returns the body facing tipped by the head pitch | Rest-pose muzzle turned about the turret's yaw pivot (Tank) or the model origin (Turret, Cannon), so shells left from beside or below the barrel | Fixed: `Pack::load` samples the muzzle node along the look clip and shots start at the posed barrel mouth |
| 37 | Flying wheeled Add-Ons | Blockland's thrust, lift and wing forces run in every `WheeledVehicle::updateForces` (0x5746a0); a datablock flies by setting `forwardThrust`, `lift`, `maxForwardVel` and the surfaces (the Stunt Plane, Kaje and Ephialtes) | Only the stock Flying Wheeled Jeep's own family got them; an imported plane drove as a car | Fixed: schema 6 `wheeled_flight`, set by the importer when any flying field is nonzero |
| 38 | Wheel steering and drive | `WheeledVehicleData::onAdd` (recovered core scripts) picks steering and driven wheels by count: 3 wheels steer the nose and drive the rear pair; a vehicle's own `onAdd` overrides with `setWheelSteering`/`setWheelPowered` | Imports used the Jeep's front-two-steer rule for every count | Fixed: the table, plus the Add-On's own `onAdd` calls |
| 39 | Model animations | `playThread(slot, sequence)` and `setThreadDir` from script, such as the Stunt Plane's propeller switching `propslow`/`propfast` at speed 5 | Vehicles drew their rest pose; no way to say a sequence plays | Fixed: schema 6 `threads` with rate and speed range; the client poses the moved parts from the server tick |
| 40 | FlyingVehicle surfaces | `FlyingVehicle::updateForces` (0x568770) is stock Torque: `horizontalSurfaceForce`/`verticalSurfaceForce` damp the sideways and roof velocity directly | Multiplied by speed as well, so the carpet stiffened with speed | Fixed |
| 41 | DTS sequences | An empty trigger list may keep a stale start index | The reader rejected it, so the Stunt Plane model failed to convert | Fixed: empty ranges skip the bounds check |
| 42 | First-person view in a vehicle seat | `getRenderEyeTransform` (0x5aafa0): the seat's rotation × head, so it rolls and pitches with the vehicle; `updateMove` halves the head every tick in first person unless free looking (0x5aeaed) | Yaw and pitch only, level through a loop; the mouse tilted a seated view freely | Fixed (2026-09-29, `vehicles-v20-checklist.md`) |
| 43 | Invert Mouse In Vehicles default | Stock v20 1; the reference install and v21 0 | 1: mouse up dipped a plane's nose, reported as inverted | Fixed: default 0 |
| 44 | Free look while mouse steering | The vehicle gets no yaw or pitch | Free look steered | Fixed |
| 45 | Third person for passengers and the gunner | The vehicle's chase camera (0x5ab80e) | An orbit round the seat or turret | Fixed |
| 46 | Dismount | 2.2 up the tilted seat first; never refused; the vehicle's velocity without its spin | World up first; refused when blocked; spin added | Fixed |
| 47 | Next/Prev Seat on foot or with no free seat | Silent | An error message | Fixed |
| 48 | Seat look by seat (second pass) | Passengers have no control object: the mouse pitches the head freely, Free Look turns it; a strafe-steered driver's mouse turns and pitches the head without Z (0x5b2d7a); only a mouse driver's head springs back, in first person | Every seat sprang back (item 42 was wrong: it read +0x658/+0x864 swapped) | Fixed (`vehicles-torque-audit.md`) |
| 49 | Third person for passengers and the gunner (second pass) | Their own camera, and the turret's for the gunner (0x5ab80e hands off only a controlling player's camera) | Item 45 gave them the vehicle's chase camera | Fixed |
| 50 | Chase camera and the driver's head | `Vehicle::getCameraTransform` swings by the rider's `mHead` whenever it is turned | Only while Z was held | Fixed |
| 51 | Driven vehicle prediction | Torque runs the controlled vehicle's moves on the client and corrects it | Drawn at the last pose, a round trip late | Fixed: `Predictor::drive` |
| 52 | Mouse driver's arms | Head pitch centred in first person | Posed from the steering accumulator, flipping every half turn | Fixed |

## How the fixes work

**Seat roles.** `Definition::seat_role` classifies every seat as Passenger,
StrafeDriver, MouseDriver, Actor (rider of a player-type mount) or Gunner.
The server maps input by role, every rider's body faces the seat, and the
client camera follows the role. In first person every rider of a vehicle sees
through the seat, rolled and pitched with it, with a head that springs back; a
Gunner's view rides the hull; an Actor's mount follows the look. In third
person every rider of a vehicle sees its chase camera. Nothing in the network
protocol changed.

**Mouse steering.** For a MouseDriver seat the client keeps sending the raw
mouse turn in `MoveInput.yaw/pitch` (pitch wraps every half turn instead of
clamping) and shows a view that follows the vehicle. The server turns the
difference between inputs into `Controls::look_delta`, and the vehicle
accumulates it exactly as `Vehicle::updateMove` does. With the default
Vehicle Mouse Invert off (item 43), moving the mouse up raises the nose.

**Player-type mounts.** Kinematic bodies moved by the v20 player motor (its
`updatePos` box sweep, see `docs/player-simulation.md`) with the datablock's speeds, run force, jump, step height, slope limit, drag and
buoyancy. Impulses change their velocity by impulse / mass, like
`Player::applyImpulse`.

**Flying Wheeled Jeep.** It is a `WheeledVehicle`; Blockland added flying
forces to `WheeledVehicle::updateForces` (blocklandv20.exe 0x5746a0, fields
registered at 0x5703ea). Each tick, after the stock tire and jet forces:

- `speed` is the size of the velocity along the nose, forward or back.
- Thrust: `forwardThrust` × throttle while throttling forward below
  `maxForwardVel`, or `reverseThrust` × throttle backward below
  `maxReverseVel`, along the nose.
- Lift: `lift` × speed along the roof, truncated to a whole number and capped
  at 4000 (hard-coded). The jeep weighs 200 × 20, so it flies level at 40.
- Bite: `clamp((speed - stallSpeed) / maxForwardVel, 0, 1)`. Pitch, yaw and
  roll torques (`pitchForce`, `yawForce`, `rollForce`) and both surface forces
  scale with it, so below `stallSpeed` the controls do nothing.
- Pitch and yaw use the squared mouse steering over `maxSteeringAngle`; roll
  uses the strafe keys unsquared. Mouse up (with the default
  `$Pref::Input::VehicleMouseInvert = 1`) dips the nose.
- Surfaces: minus the sideways and roof-wise velocity components × |velocity|
  × `horizontalSurfaceForce` / `verticalSurfaceForce` × bite. This is what
  makes a raised nose climb.
- Drag: angular momentum × (`rotationalDrag` + `drag`), velocity × `drag`;
  speed over 200 is cut to 199.
- Steering return (`WheeledVehicle::updateMove` 0x570be0, defaults from the
  data constructor): on a move with no mouse yaw, both steering axes are
  multiplied by 1 - 0.9 × min(|throttle|, 10) / 10. Holding a climb or turn
  therefore takes continuous mouse motion while the throttle is held, and
  steering does not return with the throttle released.
- No jets: `Player::processTick` (0x5b2cad) clears the rider's jet and crouch
  triggers before the vehicle sees the move, so `jetForce` never applies.
  Space brakes (trigger 2). The datablock sets no engine or jet sound.

Schema 6 carries these fields as `wheeled_flight` and `steering`; a vehicle with `wheeled_flight` gets them whatever its name, so imported Add-Ons such as the Stunt Plane fly the same way.
`tests/flying_jeep.rs` covers takeoff at 40, climbing on mouse down, level
flight, turning and rolling right, stall and landing on the wheels.

**Data.** Schema 6 types the wheeled flying and steering fields and adds animation `threads`; `Pack::load` upgrades schema 5 packs. vehicles-pack-011 (schema 5) adds the chase camera and seated look
limits on top of vehicles-pack-010 (schema 4), which added `strafe_steering`, `look_pitch`
and `underwater_speeds` and the FlyingVehicle sphere inertia.
weapons-pack-008 (schema 3) adds `Explosion::impulse_vertical` and
`DamageType::vehicle_scale`; apart from those two fields it is identical to
weapons-pack-007.

**Barrel pitch and muzzle.** `bri_vehicles::muzzle` samples each gunner's
muzzle node (`mount1`, or `eye` for the cannon) at 65 points along its look
clip when the pack loads; `Definition::muzzle` interpolates it for the server's
shots and the client's muzzle smoke. The client splits gunner models into the
fixed part and the clip-moved barrel and poses the barrel per frame.

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
- `cargo test -p bri-vehicles --lib muzzle -- --ignored`: the shot origin stays
  at the barrel mouth and the barrel turns one-for-one with the aim.
- `cargo test -p bri-client --lib vehicles -- --include-ignored`: the drawn
  barrel mouth matches the shot origin at 41 pitches and moves at every step.
- None of this replaces Maxwell's interactive playtest of feel.
