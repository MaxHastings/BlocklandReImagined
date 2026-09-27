# What v20 does to a player in water

Audit of 2026-09-27. Sources: `Brick_Large_Cubes/server.cs` in the vanilla
reference (the water brick zone), `allGameScripts.cs` (the player datablock),
the OpenMBU-family Torque `player.cc`, and a read-only capstone disassembly of
`blocklandv20.exe` (image base 0x400000). The water brick surface shader is
out of scope here; the brick texture audit owns it.

## Water bricks are `PhysicalZone`s

`fxDTSBrick::createWaterZone` makes a `PhysicalZone` with Blockland's water
fields (`isWater` +0x208, `waterViscosity` 40 +0x20c, `waterDensity` 1 +0x210,
`waterColor` +0x214). River and rapids add `appliedForce` of 1000 and 3000
along the brick's forward vector. Two quirks now match:

- The zone is the brick's box grown 0.1 in height and centred 0.1 low, so it
  reaches 0.15 below the brick and its surface is 0.05 under the brick's top.
- `onColorChange` sets `waterColor` to the paint colour with alpha × 0.75.

`ShapeBase::updateContainer` (0x5beA60) copies the zone's density, viscosity
and colour into the shape; the motor already used density and viscosity.

## Screen tint (`GameRenderFilters`, 0x590360)

After the damage flash and whiteout, v20 searches the client container for
`PhysicalZone`s around the camera (a 0.2 box). The first active water zone
whose box holds the camera draws a fullscreen quad in its `waterColor`, alpha
clamped to [0, 0.9]. Then, if a map `WaterBlock` reported the camera
submerged, it draws that block's `waterColor` too. Every stock mission leaves
it at the default (0.2, 0.6, 0.6, 0.3). It is a flat tint: no fog, no
`submergeTexture`, no distortion. This is the camera position, so a
third-person camera can be dry while the player is under.

## Splash (`Player::updateSplash`, 0x5ab5f0)

Blockland rewrote the stock check. A splash needs speed ≥ `splashVelocity` 4,
not mounted, a moved position, coverage in [0.01, 0.99], and an armed flag
(+0x8ad) that is set whenever coverage is 0.01 or less. There is no
`splashAngle` test and no ray, so wading in sideways splashes. The splash sits
at `feet + height × coverage` (the surface). `createSplash` plays
`impactWaterEasy/Medium/Hard` (all `Splash1Sound`) by speed 10 and 20, and
spawns the `PlayerSplash` ring with the liquid colour. `mBubbleEmitterTime`
resets to 0.

## Froth and bubbles (`Player::updateFroth`, 0x5a4610)

- Bubbles: while `mBubbleEmitterTime` < `bubbleEmitTime` 0.1, the bubble
  emitter runs at the body for `dt × 1000` ms per frame.
- Froth: while coverage is in [0.01, 0.99], the foam droplets and foam
  emitters run at the surface point for `speed × splashFreqMod(300) × dt` ms,
  zero below `splashVelEpsilon` 0.6. No mounted or death check.
- All three splash emitters take the liquid's `waterColor` as three keys
  (alpha 1, 1, 0) and blend normally when `alpha × 2 > 0.95`, additively
  otherwise (emitter flag +0x2d9, read at 0x55a888).

## Sounds

- Exit: the ghost's `inLiquid` sets at coverage ≥ 1.0 and clears below 0.8,
  playing `exitingWater` when speed ≥ `exitSplashSoundVelocity` 5 and not
  mounted (0x5b04db). A player who never went fully under makes no exit sound.
- `ArmorMoveBubblesSound` and `WaterBreathMaleSound` exist but
  `PlayerStandardArmor` never assigns `movingBubblesSound` or
  `waterBreathSound`, so v20 plays no underwater loop. Footstep sounds are not
  assigned either. The `UNDERWATER` string is only the EAX preset table.
- `Armor::onEnterLiquid` and `onLeaveLiquid` are empty.

## Movement and animation

Already ported before this audit and unchanged: underwater speeds at 0.9
coverage, `drag × viscosity`, density buoyancy, swim rise and dive, currents.

New from this audit:

- **Forced crouch.** `updateMove` (0x5ae2ea) crouches the player when the
  crouch trigger is held *or* coverage is at least 1.0 and the player is not
  mounted. A fully submerged swimmer therefore uses the crouch box and pose,
  stands again once the crouched box breaks the surface, and jets flat like a
  crouched player.
- **Animation** (`pickActionAnimation`, 0x5a3308). Over 60% coverage the
  action is always root. With any coverage above 0.01, being off the ground,
  sinking faster than 0.1 or holding jump (trigger 2, +0x78b) also gives root,
  so there is no fall animation in water. The jump clause is not ported,
  because remote players' triggers are not replicated.

Jumping and fall damage have no other water-specific code in v20; drag and
buoyancy slow falls, and impact damage uses the collision speed as before.

## Gaps

- `onFakeDeath` and `disappear` deactivate the zone in v20. Here a fake-killed
  or disappeared water brick stays swimmable: the brick state has no flag that
  tells a deactivated zone from a water brick with rendering turned off.
