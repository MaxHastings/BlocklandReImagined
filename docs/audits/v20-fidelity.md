# v20 fidelity audit: weapons, vehicles, explosions and player feel

Date: 2026-09-28. Branch `claude/v20-fidelity`, from main 1932015. Rows 18 to 25
came from Max's a16 play reports the same day.
Question: which sounds, effects, numbers and messages that v20's stock
datablocks and scripts define does the game drop or get wrong? Max's
report: small, inconsistent integration gaps, such as a weapon or vehicle
missing a sound or particle, or an experience that is "not quite right".

## Method

Three passes, each repeatable:

1. **Field coverage.** `tools/audit_v20_fields.py` reads every literal
   datablock in the recovered core script and every stock add-on ZIP of the
   designated reference (`E:\Downloads\B4v21Launcher\versions\Blockland v20`,
   read only). It covers 578 datablocks of the gameplay classes (items,
   images, projectiles, explosions, debris, emitters, particles, sounds,
   vehicles, tires, springs, players, splashes and static shapes) and 561
   class and field pairs. For each field it lists the source files that read
   it or cite it. Before this audit, 112 fields had no reader; after it, 99.
   Every one of the 99 is reviewed below.

       python tools/audit_v20_fields.py --v20 "<v20 folder>" --md fields.md

2. **Server cue harness.** `crates/weapons/tests/v20_fidelity.rs` fires
   every stock item (21) at a player 4 units ahead: equip, hold, release,
   hold, release. It compares the emitted cues with the original literal
   fields kept in the weapons pack (`Pack::definitions`), not with the
   lowered states. Every state the image visits must play its
   `stateSound`, emit its `stateEmitter`, play its `stateSequence` and eject
   its shell. Every projectile fired must be the image's `projectile`. A hit
   must deal `directDamage` and push with `impactImpulse`, and the explosion
   must show, play its `soundProfile` and deal `radiusDamage`. The record is
   written to `target/tmp/v20-fidelity-weapons.json`.

       cargo test -p bri-weapons --test v20_fidelity -- --ignored

3. **Client resolution harness.** `crates/client/tests/v20_fidelity.rs`
   checks that every cue named in the packs resolves in the converted audio
   and effects packs: state sounds and emitters (spray cans through their
   palette copies), projectile loops, trails and explosions (with their
   sounds and bounce, stick and blood effects), explosion debris trails,
   vehicle gun sounds and firing images, and every audio trigger the client's
   cue mapping uses. A cue the server emits but the client cannot resolve is
   the missing sound or particle a player notices.

       cargo test -p bri-client --test v20_fidelity -- --ignored

Both harnesses pass. The only weapon exception is the Horse Ray.
`HorseRayProjectile::Damage` replaces the hit with the horse transform
(`Weapon_HorseRay.cs:375`), so its `directDamage = 30` never applies in v20
either. For behaviour that lives in engine code rather than datablocks, the
audit reads Torque3D 1.1 (`d0de864`, the TGE lineage v20 inherits) for
`Player`, `Explosion`, `Debris` and `CameraShake`. Where it relies on that
lineage instead of the v20 binary, the row says "inherited".

## Results

"Before" is main at 1932015. **Fixed** rows are on this branch with a test.

| # | Area | v20 | Before | Status |
|---|------|-----|--------|--------|
| 1 | Weapon state cues (21 items) | Each visited state's sound, emitter, arm/image sequence and shell | All emitted | Pass (harness 2) |
| 2 | Projectiles fired, direct damage, impulse | Datablock `projectile`, `directDamage`, `impactImpulse` | Match; the dodgeball's scripted 50000 `CannonBallDirect` also matches `dodgeball.cs:113` | Pass |
| 3 | Explosion effect, sound, radius damage | `explosion`, `soundProfile`, `radiusDamage` | Match | Pass |
| 4 | Cue resolution | Every cued sound and effect exists | All resolve | Pass (harness 3) |
| 5 | Explosion debris | Six explosions throw `DebrisData` (`Explosion::launchDebris`, client side): Jeep, four tires then its wreckage; Tank, turret and hull; Pirate Cannon, barrel; the tank shell, 30 ± 10 invisible pieces at 140 ± 50 whose `rocketTrailEmitter` trails are its spark streaks | Not drawn. The effects importer listed `debris` as "adapter pending" | **Fixed** 7b88b4c: `bri_weapons::debris` lowers them from the current pack, and `client::explosion_debris` launches pieces (theta from the hit normal, phi around it, half a unit up). Pieces move per `Debris::advanceTime` (9.81 × `gravModifier`, spin, reflecting bounces, friction, elasticity, static on the last bounce, fade over the last second). Models come from vehicles-pack-011, which already held all six, and trails follow each piece |
| 6 | Vehicle explosion sound | Only the `initialExplosionProjectile`'s explosion sound (`vehicleExplosionSound`) | Played twice: an extra curated `vehicle.explosion` cue on destruction. Horses and rowboats, which have no explosion, played it too | **Fixed** f767c0e |
| 7 | Pirate cannon charge print | `CannonStrengthLoop` bottom-prints "Fire! :" and a 20-bar meter, at the press and every 200 ms to full power | Charge worked, no print | **Fixed** 019bd6e (1 s print per step, existing `Notice::Bottom`) |
| 8 | Turret and cannon firing animation | `playThread(0, activate)` on the gunner object | Intent dropped | Pass: `tank_turret.dts` has no `activate` sequence, and `Cannon.dts`'s animates no nodes, so v20 shows nothing |
| 9 | Hard landing camera shake | `Player::updatePos` (inherited): past `groundImpactMinSpeed` 10, shake frequency 4, amplitude 1 × (speed − 10) / `minImpactSpeed`, 0.8 s, falloff 10, controlling client only | None | **Fixed** 04390b0 |
| 10 | Falling damage threshold | Engine calls `Armor::onImpact` past the datablock's `minImpactSpeed` (Horse 250, others 30). The script also wants `minImpactSpeed` × height scale and spares a player holding the admin wand | 30 for every type, no scale, no wand rule. Horses took falling damage | **Fixed** 3429f0b |
| 11 | Vehicle tire spray, impact sounds, crash damage | See `skis-v20.md` rows 10, 20 and 21 | Landed before this audit | Pass |
| 12 | Vehicle burning | `damageEmitter` past `damageLevelTolerance` 0.99: only a destroyed vehicle burns | Burns while destroyed | Pass |
| 13 | Vehicle water splash | `splash = vehicleSplash`, `splashEmitter` | Drawn (`actor_effects` `VEHICLE_SPLASH`) | Pass |
| 14 | Tank gun | 140 × scale, 2.5 s cooldown, `TankshotSound`, `TankSmokeImage`, hull impulse −(look + up) × mass × 5 | Same (`vehicles-import` weapon, `world.rs` `weapon_step`) | Pass |
| 15 | Cannon gun | Power 1–10 at 200 ms steps, speed 5.5 × power × scale, `CannonSmokeImage`, fuse image while charging | Same | Pass |
| 16 | Jet ground dust | `jetGroundEmitter` within `jetGroundDistance` 4 of the ground while jetting | Not drawn | **Fixed** on main by the final-touches thread (de7b9c4, recovered from the executable) |
| 17 | Bottom print bar | `bottomPrint(..., hideBar = 1)` for the cannon meter | Server bottom prints always keep the bar (`Notice::Bottom` has no flag) | **Fixed** 2cee4fcb (protocol 41): `Notice::Bottom::hide_bar`. v20's global `bottomPrint` passes its line count as `hideBar`, so only the cannon hides the bar; `commandToClient` and `GameConnection::BottomPrint` prints keep it |
| 18 | Image `rotation`/`eyeRotation` from `eulerToMatrix` | `MatrixCreateFromEuler` builds `QuatF(EulerF)`, and `TypeMatrixRotation` rebuilds the matrix through it. The result is the transpose of `MatrixF(EulerF)` (TGE lineage, OpenMBG `mathTypes.cc`, `mQuat.cc`) | Stored as `MatrixF(EulerF)`: the skis (−90 90 0 / 90 −90 0) were held sideways, and the bow's tilt, football, horse brick and cannon smoke were mirrored | **Fixed** 5228401: `bri_weapons::rotation`, applied when the client loads item presentation and image emitters |
| 19 | Deploying a brick | Clicking with bricks in hand fires `brickImage`: the Fire swing, a `brickTrailEmitter` stream and `brickDeployProjectile`'s `brickDeployExplosion` (blue chunks, flash) where the ghost lands. Its `onCollision` never raises `onProjectileHit` | Ghost placed locally only, no image fire | **Fixed** 936ebe6 |
| 20 | Held melee in first person | `setImageState` restarts the state's sequence on every entry. A held hammer, wand, sword or broom loops Fire, CheckFire (0 ticks), Fire | View model swung once. The replicated state never left "Fire" | **Fixed** 4521d7f: the image-thread `WeaponAnimation` cue restarts the clip |
| 21 | Gun casing collision | Cosmetic | A ray starting inside a brick returned a zero normal. Casing debris raised an error that closed the game (a16 multiplayer crash) | **Fixed** d08cab4: the casing is dropped, counted and logged once. 40bf926 does the same for every per-frame presentation subsystem (`CosmeticFaults`) |
| 22 | Brick break sound | `BrickBreak` on `AudioClientClose3d` (3D, 10/60). The client schedules a `BrickBreakSoundEvent` for a dying ghost brick only when its death is at least 80 ms from the last one scheduled for any brick (`blocklandv20.exe` 0x539c10-0x539c57), and plays it at that brick, culled past `maxDistance` (0x53a130) | One full-volume copy per killed brick; then one per blast origin (41b0dcf), which still stacked a copy per brick on a Destructo Wand chain kill, since each popped brick has its own origin | **Fixed** (wand polish): one per 80 ms at the first brick, for blasts and chain kills alike. `audio.rs` test pins it |
| 23 | Looking straight up or down | Look limits exactly ±90° (`minLookAngle`/`maxLookAngle`). The eye is yaw then pitch; `getCameraTransform` composes `cameraTilt` past vertical. m.dts's look sequences never move the Eye node | The render camera switched to a fixed +Z up within 0.8° of vertical, so the view snapped roll, stopped turning with yaw and disagreed with the held tool. The chase camera's pitch was clamped at 89.4°, a 15° dead zone | **Fixed** e308f4b: one yaw-then-pitch basis (`Camera::oriented`) for the camera, effects, weather and listener |
| 24 | Throwing the spear | spear.dts's `fire` sequence hides both spear objects while it is thrown | The empty posed image got zero-size GPU buffers and the shadow pass bound them: a16 crashed ("buffer slice can not be empty") | **Fixed** 93678a0a: posed geometry gets placeholder buffers and empty scenes are skipped. Only the spear hides every object; the bow hides just its arrow |
| 25 | Stuck arrows | A stuck projectile keeps its last render transform | Sticking zeroes the velocity and the model was oriented from velocity, so stuck arrows pointed straight up | **Fixed** 9d016bfe: the client keeps each projectile's last flight direction. d5b92266 replicates the heading (`Projectile::heading`, protocol 41), so late joiners see it too |

### Player feel constants

Compared with `PlayerStandardArmor` and the stock Player_* add-ons
(`crates/motor/src/player.rs`, `player_types.rs`, `crates/client/src/app.rs`).
All match:

| Field | v20 | Ours |
|-------|-----|------|
| Run / back / side speed | 7 / 4 / 6 | same |
| Crouch forward / back / side | 3 / 2 / 2 | same |
| Underwater forward / back / side | 8.4 / 7.8 / 7.8 | same |
| `runForce` / mass (acceleration) | 48 | 48 |
| `airControl` | 0.1 | 0.1 |
| `jumpForce` / mass | 12 | 12 |
| `jumpDelay` | 3 ticks (96 ms) | 12 × 120 Hz |
| `minJumpSpeed` / `maxJumpSpeed` | 20 / 30 | same |
| `runSurfaceAngle` / `jumpSurfaceAngle` | 70 / 80 | same |
| `horizMaxSpeed`, `horizResistSpeed`, `horizResistFactor` | 68, 33, 0.35 | same |
| `upMaxSpeed`, `upResistSpeed`, `upResistFactor` | 80, 25, 0.3 | same |
| `drag`, `rechargeRate`, `maxEnergy` | 0.1, 0.8, 100 | same |
| Camera `cameraMaxDist` / `cameraVerticalOffset` / `cameraTilt` | 8 / 0.75 / 0.261 | same (`PLAYER_CAMERA`) |
| `cameraDefaultFov` | 90, horizontal | 90, horizontal (`vertical_fov`) |
| `maxFreelookAngle` | 3 | 3 |
| Tool slots (`maxTools`) | 5 | 5 |
| Fuel/Jump/Leap/No-Jet, Quake, Horse | per add-on | per add-on (`PlayerType::tuning`) |

Tool switching sound (`weaponSwitchSound` as `stateSound[0]`) is covered
by harness 2.

## Fields no importer reads (99), reviewed

The script finds readers by name. The 99 below have none. Each is either
handled under another name, has no effect in v20, or is an open gap.

- **Ported as named constants** (checked by hand against `player.rs`,
  `player_types.rs`, `combat.rs` and `actor_effects.rs`):
  `maxForward/Backward/SideCrouchSpeed`, `horizMaxSpeed`,
  `horizResistSpeed/Factor`, `crouchBoundingBox`, `cameraDefaultFov`,
  `maxTools/maxWeapons`, `groundImpactShake*` (row 9) and
  `minRunEnergy/runEnergyDrain/minJumpEnergy/jumpEnergyDrain` (all 0 on
  stock jet types).
- **No effect in stock v20.**
  - Walk and prone speeds are Torque fields that Blockland never uses.
  - `footPuffNumParts/Radius`: no stock player sets `footPuffEmitter`.
  - `footstepSplashHeight`: no `footstepSplash` datablock is set.
  - `repairRate`: the core script sets it to 0 (`setRepairRate(0)`).
  - `emap`: the environment map is a render hint with no stock reflections.
  - `renderFirstPerson`, `decalOffset`, `aiAvoidThis`,
    `cameraMin/MaxFov` (the zoom limits are the prefs'), `isSportPlayer`,
    `isTurboSportPlayer`, `isBasketball`, `isSoccerBall`,
    `isChargeWeapon` (script tags; the sports port keys on the item
    instead).
  - `useCustomPainEffects`, with its empty pain fields, silences turret and
    rowboat pain.
  - `stateFire` marks the firing state for AI.
  - `doRetraction` is 0 on every stock image.
  - `statetimoutvalue` is a typo in `WandImage` that v20 also ignores.
  - `activateSeq/maintainSeq` belong to the football, whose DTS has no such
    thread.
  - `hasWaterLight/waterLightColor` belong to `clockProjectile`, which is
    never underwater.
  - `texWrap` is `PlayerSplash`'s ring texture wrap.
  - `dragCoeffiecient` is misspelled in `ClockExplosionSmoke`, so v20
    ignores it too.
  - `doDetail/doFalloff/overrideAdvances` are emitter LOD hints.
  - `explodeOnMaxBounce` is false on the one debris that sets it.
  - `camerashakefalloff` is `pushBroomExplosion`'s misspelling of
    `camShakeFalloff`.
  - Vehicle `collisionTol`, `integration`, `justCollided`, `useEyePoint`,
    `cameraRoll` (false), `displayName`, `numDmgEmitterAreas`,
    `damageEmitterOffset/LevelTolerance` (row 12), `destroyedLevel`,
    `maxDrag`, `minRollSpeed`, `softSplashSoundVelocity` (no vehicle sets a
    water sound), `dustHeight/triggerDustHeight` and `minTrailSpeed` (no
    stock vehicle sets `dustEmitter` or a trail emitter).
  - `doSimpleDismount`: the skis and the tumble body dismount simply by
    family (skis audit row 15).
  - `steeringAutoReturn` is the Jeep's misnamed field; the engine's is
    `steeringUseAutoReturn`.
  - Glass `StaticShapeData` fields `deployedObject`, `disabledLevel`,
    `doesRepair` and `expDamage/Radius/Impulse`: the breakable glass is
    scripted in `breakables.rs`.
- **Adapted, documented elsewhere.** Tire and spring coefficients
  (`lateral/longitudinal*`, `kineticFriction`, `antiSwayForce`) are mapped
  to Rapier's ray suspension (`vehicles.md`, "Adaptations").
- **Verified.** `jetGroundEmitter/jetGroundDistance` landed on main (row
  16). `pickupRadius` 0.625: `PlayerData::preload` raises it to the box's
  larger XY side (1.25) and adds only the excess (`pickupDelta`, here 0) to
  the contact box, so v20 picks up an item exactly when it overlaps the
  player's own box, as ours does (`sim/tests/items.rs`
  `pickups_need_the_player_box_itself_to_touch_the_item`).
