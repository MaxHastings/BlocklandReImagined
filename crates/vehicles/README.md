# Native vehicles

`bri-vehicles` implements server-side vehicle state on the **host's shared Rapier world**. It never creates or steps an invisible world. `bri-vehicles-import` is a separate one-time ZIP/datablock/native-model converter; no runtime dependency points back to it. Both crates are root-workspace members. The host session adapter is `crates/sim/src/session/vehicles.rs`; the client renders vehicles in `crates/client/src/vehicles.rs`.

Generated content: the vehicles pack `crates/package/base-packages.json` lists (currently `content/vehicles-pack-011/vehicles.json`; the schema is `SCHEMA_VERSION` in `src/schema.rs`). Earlier packs are superseded development iterations. Eleven definitions cover the eight vehicle packages, the independently spawnable Tank Turret and hidden Item_Skis ski/tumble dependencies. All 20 package DTS models and 21 DSQ native clip assets are copied from the geometry pack; original package images are copied byte-for-byte into the indexed texture directory. The pack has 77 model/clip/texture asset records, source file SHA-256 and declaration-line evidence for 80 source datablocks. It validates/hash-checks without opening the reference installation at runtime. No required vehicle model is unresolved. Material names, animation node names and source authored metadata remain in native JSON.

## Reproduce

From repository root, with the original installation read-only:

```powershell
cargo run --manifest-path crates/vehicles-import/Cargo.toml -- "E:\Downloads\B4v21Launcher\versions\Blockland v20" content/maps-pass-008 content/vehicles-pack-011
cargo test --manifest-path crates/vehicles/Cargo.toml
cargo test --manifest-path crates/vehicles-import/Cargo.toml
cargo clippy --manifest-path crates/vehicles/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path crates/vehicles-import/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path crates/vehicles/Cargo.toml --bin vehicle_probe -- content/vehicles-pack-011/vehicles.json artifacts/native-vehicles/probe.json
```

The converter refuses an existing destination. Tests deliberately load `vehicles-pack-011` and do not silently skip missing content. Tests need no original installation, graphics device, visible window, input automation or audio device. Original assets/recovered scripts remain ignored and must not be committed.

## Shared host integration

```rust,ignore
use bri_vehicles::*;
let pack = Pack::load("content/vehicles-pack-011/vehicles.json")?;
pack.verify_assets("content/vehicles-pack-011")?;
let mut vehicles = VehiclesWorld::new(pack)?;
// physics below is bri_sim's existing bri_physics::new_world(), not a second world.
vehicles.spawn(&mut physics, Spawn {
    id: VehicleId(host_allocated_entity_id), owner: OwnerId(authenticated_owner),
    definition: "v20.vehicle.jeepvehicle".into(),
    transform: Transform { position: [0., 2., 0.], rotation: [0.,0.,0.,1.] },
    spawn_id: Some(SpawnId(brick_entity_id)), respawn_ticks: Some(600), scale: 1.,
})?;
// Authenticate the session, resolve the actual player position and apply host policy first.
vehicles.mount(&physics, VehicleId(host_allocated_entity_id), 0,
    Occupant { id: OccupantId(player_entity_id), owner: OwnerId(authenticated_owner) },
    authoritative_player_position)?;
vehicles.set_controls(OwnerId(authenticated_owner), OccupantId(player_entity_id),
    Controls { throttle: 1., ..Default::default() })?;
// Once per fixed tick, before other adapters add persistent forces to these bodies:
vehicles.pre_step(&mut physics, |position| environment.water_surface_at(position))?;
physics.step(); // exactly one shared 120 Hz step, owned by bri-sim
vehicles.post_step(&mut physics)?;
let replicated: Snapshot = vehicles.snapshot(&physics);
for intent in vehicles.drain_intents() { host.apply_vehicle_intent(intent); }
```

`VehicleId`, `OccupantId`, `OwnerId` and `SpawnId` are distinct u64 newtypes. Physics handles never appear in network snapshots or content. The host assigns IDs; imported legacy numeric IDs do not grant authority. `mount` validates distance, empty seat and duplicate occupancy; `set_controls` verifies the authenticated owner matches the occupant and that the seat has controls. Trust/minigame/spawn permissions belong to the caller. Do not expose `damage`, `damage_turret`, `apply_impulse`, `spawn`, forced `dismount` or `cancel_spawn` directly as unchecked client commands.

`Mounted` tells the player adapter to suspend its independent motor/collider and attach the avatar using the returned pose. Continue updating mount poses from each seat snapshot. `Dismounted` restores the motor at the returned world transform and velocity, including body angular velocity and the source-like exit kick. It performs a capsule sweep and endpoint overlap check for five source-derived exit directions. Failed nonforced dismount is atomic. `disconnect` clears that owner's occupied seats. `remove` removes shared physics and emits forced player transitions; `cancel_spawn` also cancels delayed replacement. Source uses a fixed 2.5-second shot gate; Tank requires a new press and Cannon charges immediately to 1, then every 24 ticks to at most 10, firing on release. Hosts must deliver both press and release edges in ordered authoritative controls.

`RespawnDue` supplies the original spawn brick key, definition, owner and transform. The host checks that the brick and its selected vehicle still exist, allocates a **new** VehicleId and calls spawn. `respawn_ticks=None` means no automatic replacement. Wreck removal follows the authored burn interval; replacement waits at least another 100 ms. The host owns minigame rules and pending-intent consumption.

`classify_collider` maps a hit in the shared physics world to `VehiclePart::Chassis` or `Turret`. Route permitted turret damage to `damage_turret`, other permitted damage to `damage`. Attached Tank turrets have a separate 250-damage pool and collider; disabling one emits its explosion, removes its collider/weapon and moves its occupant to the hull mount. Standalone turrets remain a separate definition. All hit/damage-type scaling, damage attribution across minigames, avatar passenger hit protection and scoring need host binding. `passenger_protected` exposes the vehicle's direct/radius/burn flags. Invoke `player_contact` on a qualifying contact **begin**, after host damage policy; `RunOver` carries speed-scaled damage and a replacement velocity, matching the core script's setVelocity semantics. It is not a force to accumulate each frame.

Fire intents name stable `v20.projectile.<lowercase datablock>` identifiers and carry origin, normalized direction, fully authored launch velocity, source vehicle, optional source occupant and source owner. Weapons owns projectile simulation; do not add its datablock muzzle velocity again. Tank callback velocity is 140 even though its projectile declaration says 120. Cannon launch velocity is charge times 5.5. Destruction emits projectile intents too. Audio/effect intents preserve source datablock names for catalog binding (`TankshotSound`, `HorseJumpSound`, `fastImpactSound`, `VehicleBurnEmitter`, `vehicleSplash`, `CannonFuseImage`, `CannonSmokeImage`, `TankSmokeImage`). No audio is played by the crate.

## Environment/render inputs

The water callback receives a native **X-right, Y-up, -Z-forward** body-origin position and returns an absolute surface Y, or None when the host decides the body has no water contact. The callback must interpret the authored map volume; the vehicle crate does not assume that WaterBlock position is its top. Root's verified OpenMBG evidence says the source top is position.z + scale.z; OpenMBG ShapeBase buoyancy uses body/water world-box overlap. The host must enforce horizontal and vertical volume overlap before returning a surface; an unbounded point-submerged helper alone is insufficient. This callback only receives the origin, so the host must capture the corresponding body bounds from its shared geometry when choosing a region. Buoyancy uses the shared collider's current vertical AABB submerged fraction, authored density, gravity and water drag. This is a bounded native approximation, not exact hull-volume integration. Rowboat uses inherited underwater forward/reverse speeds 8.4/7.8; the shipped package contains no paddle script.

Models/meshes/materials are native pack assets with virtual source paths for texture matching. Bind `model`, wheel model paths and optional Tank `attachment_model`; apply each vehicle's body transform, wheel spin/suspension and steering. v20 tires are authored with the hub axis along forward; each wheel's `model_rotation` is WheeledVehicle's quarter turn that puts the axle along X with the outer face away from the chassis (`bri_client::vehicles::wheel_transform`). `turret_transform` supplies the base mount, `turret_aim` supplies yaw/pitch for native rig posing, and `turret_damage` controls attachment visibility. Seat transforms already account for attached turret yaw. All 39 Horse clip aliases map to native DSQ asset paths in `Pack::animation_aliases`; `Animation` intents select root/run/back/side/jump/death/activate. Preserve original texture/material sentinel conventions when wiring the renderer. This subsystem does not claim render acceptance from file availability.

## Current verification and remaining work

Twenty-five runtime tests pass against actual Rapier collision geometry and the real native pack; two importer tests verify literal numeric expressions and parent-chain mount transforms. `artifacts/native-vehicles/probe.json` measures eleven spawned definitions (the transient tumble vehicle cleans itself up) over 1,200 shared ticks on a synthetic floor and one water region. The debug profile is reported; this is not map-scale performance acceptance. Test names and package coverage are in `docs/research/vehicles/coverage.md`.

Versioned checkpoints and uniform scaled instances are implemented below. Horse Ray transitions and player-type/turret self-control need host adapters. Exact animation blending, material appearance, sound/effect lifetimes and mounted camera limits need their respective adapters. There is no autonomous horse AI/navigation policy in this crate.

Rapier changes the numerical solver: suspension spring/damping (per unit travel, as in Torque) divide by body mass; bodies fall at Torque's 20 m/s^2 through a gravity scale; positive steering turns right; braking force becomes a per-tick wheel impulse; drag is scaled to native damping. Authored lateral/longitudinal tire relaxation and anti-sway, exact collision response, air steering, custom FlyingWheeled lift/surface behavior, exact closed-engine flight defaults and Horse step/slope solver parity are not fully reproduced. Typed behavior exists for all families, but these are explicit fidelity gaps, not accepted equivalents. Runtime emits source-style speed/damage/burn/fire values; headless tests prove control flow and collision boundaries, not subjective feel. Maxwell's interactive playtest and the full alpha contract remain outstanding.

## Skis and tumble dependency adapters

Hidden definitions `v20.vehicle.skivehicle` and `v20.vehicle.deathvehicle` are real native models/collision/suspension bodies; omit empty names from the vehicle-spawn selector. Skis expose one forced root-pose seat, four hub-based NothingTire/skiSpring contacts, authored low friction, 500 forward/reverse sled thrust and air pitch/yaw/roll. Tumble has a freely rotating restitution-0.7 90-mass body and one forced seat, with no player controls. These models are intentionally invisible helpers; original player LSki/RSki nodes provide visible skis. Their blank materials are intentional original content.

For weapons `StartSkis`, allocate a vehicle ID, spawn the skis at actor feet plus native Y 0.3, preserve actor rotation and call `set_velocity` with actor velocity. At +30 ticks mount the actor in seat 0 if it remains eligible. Host owns that pending transaction and must remove/cancel on failed spawn, disconnect, death, item cancellation or a conflicting mount. Forward SkiNodes and current paint color to the avatar renderer. StopSkis performs simple `dismount`; the empty transient body is cleaned up in post_step. Call weapon `cancel_skis` on every external teardown.

For `Tumble`, spawn deathVehicle at the supplied actor/old ski transform and velocity, then mount seat 0; `apply_impulse` is also available for source impulses applied below the center of mass. `wreck_skis` computes source-style requested duration clamp(1 + (speed-10)/50*7,1,7) seconds and returns `TumbleRequested` **after** old Dismounted/Removed events. Host converts that to the new tumble mount, whiteout and corpse camera state. The source `%time` parameter's unmount schedule is commented out: this runtime checks every 240 ticks and releases on speed<1 or water coverage>0.3, with 5,400-tick hard cleanup. A normal manual dismount is rejected while tumbling; forced host cleanup remains available. Source-derived impact projectile IDs are `v20.projectile.skiimpactaprojectile` and `v20.projectile.tumbleimpactaprojectile`.

`wreck_skis` currently needs the host's verified wreck/damage decision; exact original WheeledVehicle engine damage thresholds are not reconstructed from a guessed impact speed. Tire radius follows `WheeledVehicleTire::preload`: half the tire shape's DTS header height, ignoring the scripted `radius` (Jeep 0.749, Tank and skis 0.66). Native sled surface forces and water skim behavior need Maxwell's feel check and original-engine calibration. These limitations remain alpha work.

## Durable checkpoints and scaled instances

At a completed shared tick, drain/apply intents and call `let checkpoint = vehicles.checkpoint(&physics)?; let bytes = checkpoint.encode()?;`. Persist these bytes atomically alongside the host actor/world checkpoint. On load, construct the same validated pack and call:

```rust,ignore
let saved = Checkpoint::decode(&bytes)?;
let snapshot = vehicles.restore_checkpoint(&mut physics, saved,
    |occupant, spawn, seat| host.actor_exists_and_may_mount(occupant, spawn, seat))?;
host.reconcile_mounted_actors(snapshot); // restore emits no duplicate gameplay intents
```

Checkpoint schema 1 stores a SHA-256 fingerprint of the exact native pack, original spawn identity/transform/scale, current rigid-body pose and linear/angular velocity, sleeping state, valid occupied seats and authoritative controls, public wheel state, charge/shot/burn/tumble/respawn deadlines, held-input edges and energy cadence. It contains no Rapier handles. Validation rejects duplicate active/pending vehicle or brick IDs, duplicate or unauthorized actors, invalid content/geometry/poses/scales, nonfinite motion, impossible seat/timer/charge/energy states and oversized payloads before touching shared state. The host callback must be a side-effect-free existence/permission check. Publication replaces only this component's bodies and rebuilds queries without advancing physics; unrelated host bodies survive. No callbacks or fallible work occur during publication. The returned snapshot requires the host to reconcile actors formerly mounted in the replaced world as well as restored occupants.

Private Rapier contact manifolds, warm-start caches and ray-wheel contact handles are deliberately rebuilt. Native suspension/contact snapshots survive the boundary, but their next contacts are queried against the restored shared geometry. This is durable gameplay save/restore, not bit-identical solver rollback. Tests compare subsequent fire, tumble and respawn events, airborne energy/motion evolution, immediate scaled/turret snapshots and atomic failure. Persist pending host `StartSkis` mount transactions in the host save, because that delay belongs to the weapons/player adapter.

`Spawn::scale` supports only finite **uniform** 0.2 through 5.0. Nonuniform scaling is intentionally not represented: spherical tires, actor controller dimensions and source callbacks assume one scale value. The renderer multiplies original model scale by snapshot scale. Collision hulls, Ball radius, tire radius/hub/rest/travel, seat/attachment offsets, mount reach and dismount search offsets scale together. Player clearance remains the ordinary player capsule; a separately scaled player needs a host clearance adapter. Mass remains the authored datablock mass at the authored `massCenter`, with the inertia of `massBox` or else the shape's header bounds, as in `WheeledVehicle::onNewDataBlock`; both scale with the geometry. Suspension stiffness is `force / length` (Torque's `force * (1 - extension)`), so a geometrically scaled suspension keeps the same support force. Source forces/speed limits remain unchanged. `apply_impulse` accepts a final world impulse/point and never scales either again.

Wheeled/Flying damage follows recovered scale division; actor damage does not. Vehicle callback projectile launch speed and muzzle/destruction offsets scale; `FireIntent::scale` also tells weapons/render/effects the projectile scale, while its velocity is already final. `RespawnDue` and `TumbleRequested` preserve scale. Root must propagate scale through these adapters exactly once. No realistic mass-volume scaling or structural destruction is introduced.

## Flight evidence and limits

Pinned OpenMBG engine-family [FlyingVehicle force implementation](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/vehicles/flyingVehicle.cc) supplies the hover/automatic stabilization/surface-force algorithm; [Vehicle resource/mass implementation](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/vehicles/vehicle.cc) supplies energy thresholds/drain and authored mass assignment. These are evidence from the engine family, not the unavailable exact Blockland binary source. Shipped datablock values are retained in typed `EnergySettings` and `FlightSettings` as well as source metadata. Jeep default powered wheels are rear 2/3 per recovered core; Tank explicitly powers all four.

Magic Carpet casts down into shared geometry for hover height. It applies local-up gravity support reduced above the authored hover height and extra restoring jet force below it; low-speed auto-linear/gyro forces, lateral/vertical surfaces, maneuvering thrust, signed-square steering and source input damping are implemented. The support uses **host gravity magnitude**, replacing the engine-family constant20 so it balances this shared world's gravity. Hover ray range/height scale with the instance. Native angular damping replaces the original angular-momentum drag term. Terrain clearance and surface tests are real shared Rapier queries; source mission-area flight ceiling remains a host integration gap.

`Controls::jet` (or positive vertical) requests jets. Initial energy is zero following ShapeBase engine-family initialization; `set_energy` is trusted host grant/reset only. Recharge/drain run on an exact rational 32ms cadence within 120Hz (25/96 source tick per native tick), with the phase checkpointed. Carpet source omits maxEnergy, so the native engine-family default0 prevents resource jets while automatic hover still works. Closed Blockland defaults need original-engine verification; no invented infinite energy is supplied. FlyingWheeled uses authored jetForce with the same energy gate, but its custom lift/surface equations remain a documented native approximation because the pinned older engine does not implement that Blockland class. Energy and hover tests establish this implementation, not subjective flight parity.
