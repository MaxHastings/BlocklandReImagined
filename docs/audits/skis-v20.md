# Skis audit against v20

Date: 2026-09-28. Build under test: a15 (main f95d2736d, protocol 36).
Scope: everything the v20 `Item_Skis` add-on defines or relies on: the
`SkiItem` / `SkiWeaponImage` item, `Player::startSkiing` / `stopSkiing`, the
`skiVehicle` WheeledVehicle with its `NothingTire` and `skiSpring`, the
engine's WheeledVehicle forces, crash (`onWreck`) and impact handling, and the
`deathVehicle` tumble where it touches skiing.

## Sources

- `Add-Ons/Item_Skis.zip` from the designated v20 reference
  (`E:\Downloads\B4v21Launcher\versions\Blockland v20`, read only):
  `Item_Skis.cs` (line numbers below are that file's).
- `blocklandv20.exe` from the same folder, disassembled with capstone:
  - `WheeledVehicleData` field registration at 0x5703ea: `isSled` is a bool
    at datablock +0x378; `forwardThrust` +0x37c ... `stallSpeed` +0x3a8;
    `steeringUseAutoReturn` +0x3ac, `steeringAutoReturnRate` +0x3b0,
    `steeringAutoReturnMaxSpeed` +0x3b4. `jumpForce` is registered only on
    `PlayerData` (0x5a097b), so the ski datablock's `jumpForce = 100` is
    ignored, as its own comment says ("havent added this into code yet").
  - `WheeledVehicle::updateForces` (0x5746a0): spring force is
    `force × (1 − extension)` plus a push-only damper, not scaled by mass
    (0x5749be); then Blockland's flying forces (thrust, lift, bite, pitch /
    yaw / roll torques, surfaces), already decoded for the Flying Wheeled
    Jeep (see `vehicles.md`). New here: at 0x57565f the surface forces are
    skipped when `isSled` is set and wheel 0 (`mWheel[0].surface.contact`,
    object +0x2078, wheels are 0x64 apart from +0x2058) is off the ground.
    Thrust and the three torques are not gated.
  - `WheeledVehicle::updateMove` (0x570be0): with no mouse yaw on a move,
    both steering axes are multiplied by
    `1 − steeringAutoReturnRate × min(|move.y|, maxSpeed) / maxSpeed`.
  - `WheeledVehicle::updateCollision` (0x5721e0): after
    `resolveCollision` reports a hit (0x5722f6), `onWreck` (0x572348) is
    called on the server when none of wheels 0, 1 and 2 touches the ground
    (loop at 0x572303).
  - `VehicleData` constructor (0x569aa2): `minImpactSpeed` and
    `softImpactSpeed` default 25, `hardImpactSpeed` 50,
    `collDamageThresholdVel` 20, `collDamageMultiplier` 0.05.
- Torque (TGE 1.4 lineage) `Vehicle::resolveCollision` / `resolveContacts`
  / `updatePos` for what counts as a collision (approach faster than
  `contactTol`), the impact sounds and `onImpact`.

## Differences

"Ours" is main at f95d2736d. Status **Fixed** is on branch
`claude/skis-v20` with a test or a stated reason; **Open** is not done.

| # | Area | v20 value or rule | Ours before | Where (before) | Status |
|---|------|-------------------|-------------|----------------|--------|
| 1 | Forces that move the skis | `skiVehicle` is a WheeledVehicle on `NothingTire` (all friction and tire forces 0, `Item_Skis.cs:615`), so engine, brakes and tires do nothing. Only Blockland's flying forces act: `forwardThrust 500` × throttle along the nose while speed < `maxForwardVel 40`, `reverseThrust 500` below `maxReverseVel 10`, `lift 0` | An invented model: `forwardThrust` × throttle only when a ray found ground, with no speed limit | `crates/vehicles/src/world.rs:1305` | Fixed: skis use the same decoded WheeledVehicle force code as the Flying Wheeled Jeep |
| 2 | Sideways grip | `horizontalSurfaceForce 50`: minus sideways velocity × speed × 50 × bite, where bite = clamp(speed along nose / `maxForwardVel`, 0, 1); `isSled = true` applies it only while wheel 0 is on the ground | Minus sideways velocity × 50, no speed or bite factor, whenever a short ray from the origin hit ground | `world.rs:1310` | Fixed |
| 3 | Turning | Mouse steering (`steeringUseStrafeSteering = false`, `Item_Skis.cs:765`) accumulated up to `maxSteeringAngle 0.885`; yaw torque `yawForce 600` × (steer / 0.885)², pitch `pitchForce 1000` × pitch², roll `rollForce 900` × strafe, all × bite, so standing skis cannot turn and slow ones barely do | Torque linear in steering, not scaled by speed, so skis spun on the spot | `world.rs:1322` | Fixed |
| 4 | Steering return | `updateMove` returns steering only on a move with no mouse turn, by `1 − 0.9 × |throttle|/10` per 32 ms move, so it holds with the throttle released | Halved every quarter second whenever the mouse was still (the generic inferred rate) | `world.rs:1114` | Fixed: skis use the decoded rule, like the Flying Wheeled Jeep |
| 5 | Brake / jump | Jump (trigger 2) reaches the vehicle as the WheeledVehicle brake, which acts through the tires; with `NothingTire` it does nothing. `jumpForce 100` is not an engine field | Jump braked the skis with an invented horizontal impulse of `brakeTorque` / mass | `world.rs:1313` | Fixed: removed |
| 6 | Drag | Every WheeledVehicle: force −= `drag 0.8` × velocity; torque −= (`rotationalDrag 3` + `drag`) × angular momentum | Linear damping `drag` × 0.05, angular `rotationalDrag` only | `world.rs:1841`, `world.rs:1843` | Fixed: same as the Flying Wheeled Jeep (linear `drag`/mass, angular 3.8) |
| 7 | What holds the skis up | Four `skiSpring`s give at most 195 each (`Item_Skis.cs:640`), 780 against a 1800 weight (90 × 20), so the skis ride on their body, which slides with `bodyFriction 0.21` | Same springs and hull, but Rapier's contact friction at the hull's corners, blended with the ground's friction (about 0.36) | `world.rs:1879` | Fixed with an adaptation, see "Adaptations" |
| 8 | Crash (`onWreck`) | The body hits something (approach faster than `contactTol 0.01`) while none of wheels 0–2 touches the ground | Inferred: any contact with a velocity change of 20 or more, or lying past about 75° | `world.rs:58`, `world.rs:1456` | Fixed: decoded rule. Hitting a wall or landing hard with the skis under you no longer crashes; landing on your back or side does |
| 9 | Impact puff (`onImpact`) | On a body collision with velocity change > `minImpactSpeed 3`: `skiImpactAProjectile` (`Item_Skis.cs:859`) | Any active contact with change > 3, including resting slides | `world.rs:1437` | Fixed: collisions only |
| 10 | Impact sound (every vehicle) | On a body collision, `hardImpactSound` at change ≥ `hardImpactSpeed`, else `softImpactSound` at ≥ `softImpactSpeed` (defaults 50 and 25, 0x569aac). Skis: `Impact1BSound` past 10, no soft sound (`Item_Skis.cs:759`); Jeep, Tank, Ball, Flying Wheeled Jeep: `slowImpactSound` past 10, `fastImpactSound` past 15 | `fastImpactSound` past 15 for every vehicle, on any contact | `world.rs:1466` | Fixed: the datablock's sounds and speeds for every vehicle (the tumble body plays `Impact1BSound`) |
| 11 | Ski colour | `setNodeColor("LSki"/"RSki", getColorIDTable(%client.currentColor))` (`Item_Skis.cs:456`): the skier's last colour spray can, set by `serverCmdUseSprayCan` | Always SkiItem's blue colour shift | `crates/client/src/app.rs:4205` | Fixed: the server remembers the last colour can per player and colours the (invisible) ski vehicle with it; the client paints the ski nodes from that |
| 12 | "Can't use skis" | `commandToClient(..., 'CenterPrint', "\c4Can't use skis right now.", 2)` when firing the skis while on another vehicle | A server diagnostic line the player never saw | `crates/weapons/src/runtime.rs:1117` | Fixed: a two-second centre print in colour 4 |
| 13 | Boarding after 250 ms | `%newcar.schedule(250, mountObject, %obj, 0)` has no reach check | Boarding needed the feet within `minMountDist` of the seat, so a fast skier could leave the skis behind and the start was cancelled | `crates/sim/src/session/vehicles.rs:1019` | Fixed: the skier is put in the seat wherever it is |
| 14 | Starting | Spawn at the player's position + 0.3 up, the player's rotation and velocity; hide the image, `setScrollMode -1` | Same | `runtime.rs:1128` | Matches |
| 15 | Stopping | Fire the skis again while riding: `stopSkiing` and unmount; jet dismounts (`doSimpleDismount`, no free-space search); `onDriverLeave` deletes the skis | Same | `vehicles.rs:880`, `world.rs:667` | Matches |
| 16 | Crash tumble | `onWreck`: `stopSkiing`, whiteout `time/7000`, tumble for `((speed − 10)/50 × 7 + 1)` s clamped to 1–7 on a `deathVehicle` under the corpse camera | Same | `world.rs:794` | Matches (`vehicles.md` rows 13, 27) |
| 17 | Chase camera | `cameraMaxDist 11`, `cameraOffset 6.8`, `cameraTilt 0.3201`, `cameraLag 0`, `cameraDecay 1.75`, `cameraRoll false` | Imported as authored (vehicles-pack-011) | `crates/vehicles-import/src/lib.rs:525` | Matches |
| 18 | Mass and inertia | `mass 90`, `massCenter 0 0 0.5`, `massBox 1.5 1.5 1.5` | Same | vehicles-pack-011 | Matches |
| 19 | Speed cap | Every WheeledVehicle over 200 is cut to 199 (0x575bd5) | Only the Flying Wheeled Jeep | `world.rs:1254` | Fixed: skis share the code |
| 20 | Tire spray (every wheeled vehicle) | `WheeledVehicle::advanceTime` (0x571c60): above speed 1, each wheel whose `surface.contact` is set runs the datablock's `tireEmitter` at its contact point for `dt × speed / maxWheelSpeed × 1000` ms, axis straight up. Skis use `SkiEmitter` (`Item_Skis.cs:698`); Jeep, Tank and Flying Wheeled Jeep use `VehicleTireEmitter`; the Ball's is commented out | No vehicle drew tire emitters, and clients did not know which wheels touched the ground | none | Fixed (follow-up commit): `VehiclePose::wheel_contact` (protocol 39) and `actor_effects::tire_sprays` / `update_tires` |
| 21 | Collision damage (every vehicle) | None. `collDamageThresholdVel` and `collDamageMultiplier` are registered and packed for the network (0x56a346) but never read; `Vehicle::updatePos` (0x56ecb1) only raises `onImpact` past `minImpactSpeed` and plays the impact sounds, and no stock vehicle scripts `onImpact` (only skiVehicle and deathVehicle, for puffs) | Every hull contact with change over `collDamageThresholdVel` did `(change − 20) × collDamageMultiplier` damage: 0.02 per unit to the Jeep, Tank, Ball and Flying Wheeled Jeep, 0.05 to the Magic Carpet | `world.rs:1527` | Fixed (follow-up commit): removed; the audit's first version wrongly called this missing damage |
| 22 | Energy and jets | `maxEnergy 100`, `jetForce 3000`; the rider's jet never reaches a vehicle (`Player::processTick` 0x5b2cad) | Same | `vehicles.md` row 32 | Matches |

## Adaptations

- **Body collision shape.** The skis collide as the box around
  `skivehicle.dts`'s two collision hulls. The main hull is a box with a
  slightly tapered, not quite flat base; Rapier produced sideways contact
  normals on it at speed (measured: a normal of (−0.34, 0.88, −0.34) on flat
  ground), which kicked sliding skis into spins of up to 10 rad/s. The box
  differs from the hull by under 0.1 on each side.
- **Body friction.** `bodyFriction 0.21` is applied as Coulomb friction at
  the centre of mass from the previous step's contact impulses
  (`hull_friction`), with Rapier's own contact friction turned off for the
  skis. Rapier's corner friction at 0.21 made the skis yaw on their own;
  Torque's `resolveContacts` behaves differently and is not reproduced
  exactly.
- The wreck test uses the Rapier raycast wheels' contact flags for Torque's
  `mWheel[i].surface.contact`, and "collided" as any hull contact the skis
  were moving into faster than `contactTol` at the start of the tick.

## Verification

- `cargo test -p bri-vehicles --test skis` (new): thrust to 40 and no
  further on flat ground (15.6 after 5 s), coasting down a 25° slope
  (25.6 after 4 s, running straight), sideways slide removed on the ground
  but not in the air, mouse steering turns moving skis but not standing
  ones, landing upside down wrecks, a 15 m drop onto the skis does not,
  skiing into a wall at 25 puffs and plays `Impact1BSound` without wrecking.
- `cargo test -p bri-vehicles` (all 41), `cargo test -p bri-weapons` with
  `--ignored`, and `cargo test -p bri-sim --test vehicles -- --ignored`
  (8, including the ski item test, which now checks the skis take the
  colour can's paint).
- Follow-up: `cargo test -p bri-vehicles --test impacts` drives the Jeep,
  Tank, Flying Wheeled Jeep, Ball, skis and Magic Carpet into a wall at 30:
  none takes damage (before, each wheeled one did), each plays its own
  hard impact sound and the Carpet none; wheels report ground contact on
  the ground and none in the air. `cargo test -p bri-client --test
  tire_spray -- --ignored` checks every stock vehicle's tire emitter exists
  in effects-runtime-pack-004 and sprays at the tire bottom by speed over
  `maxWheelSpeed`, only above 1 and only on the ground.
- None of this replaces Maxwell's playtest of how skiing feels.
