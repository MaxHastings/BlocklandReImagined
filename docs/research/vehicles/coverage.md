# Vehicle implementation coverage — 2026-09-26

This is engineering evidence, not alpha acceptance. Read `crates/vehicles/README.md` for integration and fidelity limits.

| Package / native datablock | Native behavior and resources | Verification |
|---|---|---|
| Vehicle_Jeep / JeepVehicle | Original body/tire nodes and authored convex collision; seven seats (sit/root); front steering, rear powered wheels; 300 mass, 12,000 engine, 50,000 brake, 30 wheel limit; damage 200, four-second burn | `native_catalog_assets_and_authored_values`, `wheels_drive_brake_and_world_wall`, `seats_authority_and_serialization`, lifecycle tests |
| Vehicle_Flying_Wheeled_Jeep / FlyingWheeledJeepVehicle | Jeep model dependency/seven seats/four wheels; 200 mass, 3,000 forward and 2,000 reverse thrust, lift and pitch/yaw/roll controls | `flight_and_water_families`; exact aerodynamic/energy fidelity remains open |
| Vehicle_Tank / TankVehicle | Three hull mounts; composed turret rider; four powered wheels, rear steering -0.8; 25,000 engine; 300 hull/250 turret damage; separate turret hit collider; 140-speed shell on press, 300-tick cooldown | `tank_gunner_and_turret`, `turret_damage_removes_weapon_and_preserves_hull`, wheel/lifecycle/tip tests |
| Vehicle_Tank / TankTurretPlayer | Independently spawnable actor-like turret with one seat, cannon controls, protected passenger metadata, original turret model | `tank_gunner_and_turret`; direct player-type/self-controlled turret integration still host work |
| Vehicle_Magic_Carpet / MagicCarpetVehicle | Original carpet model/collision/seven seats; flying movement, yaw and upright correction; 100 damage, 60-tick burn | `flight_and_water_families`; source-family hover/resource tests pass; closed-engine defaults remain unverified |
| Vehicle_Horse / HorseArmor | Original horse/DSQ resources; mount2 rider, root pose; actor upright collision, 12/6/1 ground limits, 28 acceleration, 17 jump impulse, ground/water/animation state, 250 damage | `horse_run_jump_and_collision`, parent-transform importer test; step/slope/autonomous AI policy not complete |
| Vehicle_Ball / BallVehicle | Original model; sphere contact/rolling, no seats, 200 mass, 0.6 restitution, 999,999 damage | `ball_rolls_without_mounts`, `original_shapes_collide_tip_and_slope` |
| Vehicle_Pirate_Cannon / CannonTurret | Original cannon/ball/debris assets; one seat, immobile 200,000-mass actor; charge 1..10 every 24 ticks and release speed 5.5*charge; 300-tick fire gate; 300 damage | `cannon_authored_charge_and_cooldown`; rig pitch and visual fuse need adapters |
| Vehicle_Rowboat / RowBoatArmor | Original rowboat model/three seats; density 0.6 buoyancy, land movement disabled, inherited swim movement 8.4/7.8, 260 damage | `flight_and_water_families`; real authored water volumes and feel need host/playtest |

Shared headless coverage includes eleven definitions/all assets SHA verification, duplicate occupancy, wrong-owner and passenger controls, NaN controls, invalid data/tick rate, blocked and forced dismount, transferred velocity, collision-to-runover intents, attached-turret weapon removal, destruction/projectile intents, physics cleanup, spawn cancellation, respawn request timing, all families and finite-state slope/tipping/drop checks. Snapshot JSON round-trips occupied seats. The slope/tipping test proves finite contact behavior, not exact original incline handling. The wheel test's historical name mentions a wall; actual wall stopping is covered by the Horse test.

Converter evidence names declarations in the exact source ZIP member, normalized source line and SHA-256. Numeric multiplication is evaluated as literal factors only; no script execution. Native node transforms compose all parent rotations/translations before mount/hub extraction. Required model resolution fails loudly. Native collision details are used where present. Actor shapes use the authored PlayerData box dimensions after the existing engine-family quarter-unit convention; those adaptations are recorded in each definition.

Recovered core evidence used for native callbacks/defaults (read-only `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`): VehicleInvulnerabilityTime line 2860 (100 ms), PlayerStandardArmor line 8738 and underwater speeds 8771–8772, Armor::doDismount 8954, vehicle wheel defaults near 18573, runover behavior 18773–18807, WheeledVehicleData::Damage 18821, FlyingVehicleData::Damage 18910, Vehicle::finalExplosion 18988. Package callback evidence lives in source-hashed declarations and ignored inspection copies under artifacts/native-vehicles. Code-derived meaning is documented, original script text is not committed.

Numerical adaptations/fidelity work are mandatory open items. This matrix does not check off the alpha vehicle acceptance item. Root still must bind shared host/network/player/world/render/effects/audio/minigame/event/save state, provide a packaged build and obtain Maxwell's interactive testing.

## Item_Skis dependencies added before handoff

| Hidden native datablock | Implemented behavior | Verification |
|---|---|---|
| skiVehicle | Original blank helper model/collision and four hub contacts; NothingTire zero powered traction; source skiSpring, 0.21 body friction, sled thrust/pitch/yaw/roll; forced root seat; simple dismount and empty-body cleanup; onWreck duration + typed tumble request | `skis_drive_simple_dismount_and_wreck_transition` |
| deathVehicle | Original blank tumbling helper model/collision, 90 mass, 0.7 restitution, zero control forces; forced root seat; manual dismount rejection; checks at 240-tick intervals for speed<1 or water coverage>0.3; 5,400-tick hard cleanup | `tumble_blocks_controls_and_releases_on_water_at_two_seconds` |

Latest pack is vehicles-pack-007, with 11 definitions, 20 native models, 21 native clips, 36 original texture records and 39 explicit Horse animation aliases. Twenty-five runtime tests and two importer tests pass. Numerical/API limits remain in the README. The water lifecycle regression initially failed because a buoyant body left the finite test water surface before the check; the corrected fixture keeps it submerged at the actual check tick rather than changing release logic. Final gates and provenance are saved under artifacts/native-vehicles.


## Save/scale/flight hardening

Pack schema2, snapshot schema2, checkpoint schema1. Additional headless tests cover mid-charge release event equivalence; pending respawn and tumble deadline equivalence; corrupted checkpoint rejection without mutating existing/unrelated bodies; completed tick/consumed intent boundary; uniform collision/seat scaling and restored motion; scaled wheel hubs/radii/rest lengths with unchanged authored mass; restored turret collider yaw and angular velocity; Carpet restoring hover forces and no free jets; exact resource cadence and its saved phase. Runtime total25, importer total2. Source-family assumptions and unsupported solver rollback/nonuniform scale/flight ceiling are documented in the README. The original flight test initially failed because it assumed unlimited jet energy; it now explicitly grants authored energy to the FlyingWheeled fixture, while a separate test verifies zero-energy Carpet behavior.
