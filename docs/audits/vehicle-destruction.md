# Vehicle destruction presentation: v20 audit

Max, 2026-09-30: "when vehicles exploded like tank jeep stuntplane they would
turn black right away when on fire ... right now they don't do that".

Evidence: the recovered v20 core scripts
(`.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`, read-only on
Max's PC) and the vehicle Add-Ons in the v20 reference install, read in
place. v20 was never run. No original script text is committed here.

The stock v20 vehicle Add-Ons are Ball, Flying Wheeled Jeep, Horse, Jeep,
Magic Carpet, Pirate Cannon, Rowboat and Tank. The Stunt Plane is not in
the v20 reference; ours (`packages/imported/vehicle_stunt_plane`) is a
`WheeledVehicleData`, so it follows the wheeled rules below.

| # | Step | v20 | Ours before | Now |
|---|---|---|---|---|
| 1 | Paint at death | `WheeledVehicleData::Damage` (line 18821) and `FlyingVehicleData::Damage` (18910) call `setNodeColor("ALL", "0 0 0 1")` once damage reaches `maxDamage`. `Vehicle_Tank.cs` has its own destroy path that does the same and forwards `setNodeColor` to the turret | Kept the spawn brick's colour (`crates/client/src/vehicles.rs` `prepare`) | **Fixed**: `Definition::wreck_color` (schema.rs) is black for every Wheeled, Flying and Ball vehicle, Add-Ons included; `vehicles::body_tint` paints the body, turret and animated parts with it while `destroyed` |
| 2 | Tires | The same functions swap every wheel to `emptyTire`/`emptySpring` | The host cleared the wheels, but the client still drew them at rest | **Fixed**: no wheels drawn on a wreck |
| 3 | First explosion | `initialExplosionProjectile` (jeep, tank, default `vehicleExplosionProjectile`) | Same (`world.rs` `damage`) | Pass |
| 4 | Fire | `damageEmitter` `VehicleBurnEmitter` past `damageLevelTolerance` 0.99 | Burns from destruction until removal | Pass, with one accepted gap: v20 also burns in the last 1% of health. Health is not replicated and a cosmetic gets no bandwidth, so ours starts at destruction |
| 5 | Final explosion and removal | After `burnTime` (Jeep, Tank, Ball 4 s; Magic Carpet 0.5 s) `finalExplosionProjectile` (black smoke cloud) and the wreck is deleted | Same (`world.rs` `post_step`) | Pass |
| 6 | Tank turret destroyed | `TankTurretExplosionProjectile` | Same | Pass |
| 7 | PlayerData mounts (horse, rowboat, cannon) | Die like players (`Armor::onDisabled`), no repaint | No repaint | Pass |

Cost: the wreck look is drawn from the replicated `destroyed` flag, so late
joiners see it and it adds nothing on the wire or to the brick render path.

Tests: `bri-client` `vehicles::tests::a_destroyed_vehicle_is_drawn_black_without_its_tires`
(loads the committed stunt plane: live = paint and 3 tires, wreck = black,
0 tires) and `only_vehicle_classes_char_and_player_mounts_keep_their_colour`
(every family, live and destroyed).
