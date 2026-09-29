# Import report: Vehicle_Stunt_Plane

Stunt Plane by Kaje and Ephialtes. Licence: unknown. Package `vehicle_stunt_plane` 1.0.0.

**Verdict: converted with gaps.**

| | Count |
|---|---|
| Files | 17 |
| Assets converted or copied | 8 |
| Assets failed | 0 |
| Datablocks | 13 |
| Datablocks converted | 11 |
| Datablocks recognised only | 2 |
| Datablocks unsupported | 0 |
| Ids assigned | 15 |
| Dependencies (missing) | 2 (0) |
| Unsupported | 1 |
| Ambiguous | 5 |
| Needs behaviour | 4 |
| Needs behaviour, ported | 0 |

## Dependencies

- `base` (reference, reference): uses VehicleBurnEmitter, VehicleTireEmitter, base/data/particles/cloud.png, base/data/shapes/empty.dts, vehicleBubbleEmitter, vehicleExplosion (inherited), vehicleExplosionProjectile (inherited), vehicleFinalExplosion (inherited), vehicleFinalExplosionProjectile (inherited), vehicleFoamDropletsEmitter, vehicleFoamEmitter
- `Vehicle_Jeep` (ForceRequiredAddOn, reference, v20-vehicles): uses JeepVehicle, jeepDebrisTrailEmitter, jeepTireDebrisTrailEmitter

## Needs behaviour

- `ContrailImage1::onDone` at Add-Ons/Vehicle_Stunt_Plane/stuntplane_Contrail.cs:56: runs on image state script `onDone` of `ContrailImage1`; does change inventory.
- `ContrailImage2::onDone` at Add-Ons/Vehicle_Stunt_Plane/stuntplane_Contrail.cs:82: runs on image state script `onDone` of `ContrailImage2`; does change inventory.
- `contrailCheck` at Add-Ons/Vehicle_Stunt_Plane/stuntplane_Contrail.cs:87: helper called by other script; does change inventory, play animation, read velocity, schedule.
- `stuntplanevehicle::onadd` at Add-Ons/Vehicle_Stunt_Plane/Vehicle_StuntPlane.cs:185: callback on `stuntplanevehicle (WheeledVehicleData)`; does call original, play animation.

No port is listed for the rest yet. To make one, follow `docs/modding/porting.md`.

## Unsupported

- JeepVehicle.uiName = "" (Add-Ons/Vehicle_Stunt_Plane/server.cs:15): changes Vehicle_Jeep's datablock at load; a package cannot edit another package's content

## Ambiguous

- stuntplaneVehicle.flatspring = stuntplaneFlatSpring (Add-Ons/Vehicle_Stunt_Plane/Vehicle_StuntPlane.cs:1): `stuntplaneFlatSpring` is not declared by this Add-On, the reference install or the core scripts; Torque leaves the field empty
- stuntplaneVehicle.flattire = stuntplaneFlatTire (Add-Ons/Vehicle_Stunt_Plane/Vehicle_StuntPlane.cs:1): `stuntplaneFlatTire` is not declared by this Add-On, the reference install or the core scripts; Torque leaves the field empty
- stuntplaneVehicle.hardimpactsound = fastImpactSound (Add-Ons/Vehicle_Stunt_Plane/Vehicle_StuntPlane.cs:1): `fastImpactSound` is not declared by this Add-On, the reference install or the core scripts; Torque leaves the field empty
- stuntplaneVehicle.softimpactsound = slowImpactSound (Add-Ons/Vehicle_Stunt_Plane/Vehicle_StuntPlane.cs:1): `slowImpactSound` is not declared by this Add-On, the reference install or the core scripts; Torque leaves the field empty
- stuntplaneVehicle.splash = vehicleSplash (Add-Ons/Vehicle_Stunt_Plane/Vehicle_StuntPlane.cs:1): `vehicleSplash` is not declared by this Add-On, the reference install or the core scripts; Torque leaves the field empty

Full detail: `import-report.json`.
