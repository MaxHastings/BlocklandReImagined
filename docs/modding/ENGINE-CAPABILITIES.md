# What the engine offers a new game

Blockland ReImagined provides reusable engine systems for building entirely
different games. An Add-On defines the gameplay; the engine provides physics,
networking, rendering, movement, audio, UI and extensible scripting. This page
is a map: start from what you want to build, and it points at the guide that
says how, a working sample, and a test that runs it.

It does not repeat what each seam's status is. **[seams.md](../architecture/seams.md)
is the one place that holds status** (on main, in which release, what is still
limited). The older [total-conversion audit](../audits/total-conversion.md) is a
dated snapshot from 2026-09-29 and is not a list of current gaps.

**Reading the labels.** Everything named here is in the source on `main`. A
published release has only what was on main when it was cut; a feature newer
than your release is not in your download. [seams.md](../architecture/seams.md)
says which release first carried each seam.

**What this page does not claim.** Private conversions that other people are
building on the engine are not part of this repository and are not described
here. Only the generic engine systems they use are, and those are documented
on their own terms, whatever state any game built on them is in.

## The building blocks every game uses

| You need | Use | Guide |
|---|---|---|
| Game logic that runs on the host | Rhai rules: hooks, commands, state, capabilities | [rules.md](rules.md), [rhai.md](rhai.md) |
| A package that other packages build on | exported APIs between Add-Ons | [rules.md](rules.md) ("Add-Ons that build on each other"), `crates/sim/tests/addon_interop.rs` |
| Code that runs on each player's machine | sandboxed WebAssembly and WGSL, trusted once per server | [trust.md](trust.md), [../architecture/client-sandbox.md](../architecture/client-sandbox.md) |
| Numbers for a body, weapon or vehicle | data files, no code | [tuning.md](tuning.md), [weapons.md](weapons.md), [content-kinds.md](content-kinds.md) |
| A new body, weapon, vehicle or brick from Blender | glTF import | [gltf-models.md](gltf-models.md) |
| A level | map Add-Ons | [maps.md](maps.md), sample `packages/samples/sample-map` |
| A mode in Start Game | `mode` files naming Add-Ons and a map | [content-kinds.md](content-kinds.md), `packages/stresslab/stresslab-mode` |

## By what you want to build

### A military shooter

Weapons with magazines, reloads, scopes, sounds, spread, recoil and damage
falloff; bodies with their own health, speeds, posture and falling damage;
kill scoring, rounds and a score report; a HUD that replaces the stock one.

- **Weapons and held items:** [weapons.md](weapons.md); image states and
  `mount_image` in [rules.md](rules.md). Recoil and view kick are a shot's
  `kick` (a camera shake on the shooter's own view).
- **Bodies:** an archetype's `movement` block (every motor constant),
  `look.camera`, and per-body crouch, prone and lean; see [tuning.md](tuning.md)
  and [gltf-models.md](gltf-models.md).
- **Hit regions:** `hit_region` in [rules.md](rules.md);
  test `crates/sim/tests/hit_regions.rs`.
- **HUD:** panels and regions that can replace the crosshair, ammo, chat, tool
  bar and building interface; [hud.md](hud.md).
- **Rounds and teams:** minigames, teams, `end_round`, `show_report`;
  [rules.md](rules.md).
- **Bots:** [bot-objectives.md](bot-objectives.md),
  [../architecture/bots.md](../architecture/bots.md).
- **Sample and tests:** the Commando sample (`packages/samples/sample-commando*`),
  `crates/sim/tests/commando.rs`, `crates/client-sandbox/tests/commando.rs`.

### Vehicle combat

Driven vehicles with seats, turrets that aim independently, riders in
authored poses, and animation layered on the body.

- **Vehicles** are data (`vehicles.json`): flight, steering, threads, smash,
  shove, weapons and seats; [content-kinds.md](content-kinds.md).
- **Seats, rider poses and held images:**
  [seat-pose-composition.md](seat-pose-composition.md).
- **Prediction for the driver:** [../networking.md](../networking.md).
- **Tests:** `crates/sim/tests/vehicles.rs`, `vehicle_prediction.rs`,
  `vehicle_crashes.rs`; a buggy to copy in `packages/playground`.

### A physics sandbox

Pushing, throwing, holding and tethering players, vehicles and loose bodies;
asking the world questions; building from bricks.

- **Moving things, holding and tethers:** the `physics` capability, "Moving
  things" in [rules.md](rules.md).
- **Asking the world:** `raycast`, object queries and zones, objects of every
  kind through `on_object`; [rules.md](rules.md).
- **Pass groups, portals, tethers:** `crates/sim/tests/pass_groups.rs`,
  `portals.rs`, `tether.rs`.
- **Wiring bricks together:** `packages/samples/sample-wire-lite`,
  `crates/sim/tests/wire_lite.rs`.
- **Throwable and projectile weapons:** `packages/samples/sample-bubble-blaster`.

### A roleplay server

Money and items the host remembers, windows for players, shops and chests,
and several Add-Ons that share one economy.

- **Persistent data:** `host_data` / `set_host_data` and Add-On data on disk;
  [rules.md](rules.md) ("Kept on the host").
- **Windows:** `show_panel` and HUD panels; [rules.md](rules.md), [hud.md](hud.md).
- **A framework others extend:** `packages/samples/sample-rp-bank`, with
  `sample-rp-inventory` built on it; `crates/sim/tests/roleplay.rs`.

### A platformer or a different kind of mover

Bodies that do not move like a Blockhead, cameras that are not behind the
player, controls that are not walking.

- **Movement controllers:** WebAssembly modules that run on the host and
  predicted on the client; [../architecture/movement-controllers.md](../architecture/movement-controllers.md).
  Samples `packages/samples/sample-dive`, `sample-wall-kick`, `sample-trooper`;
  test `crates/sim/tests/movement_controller.rs`.
- **Cameras:** scripted cameras, orbit cameras and object cameras
  ([rules.md](rules.md) "Cameras"), and client code that places the camera and
  takes over a control (`packages/samples/sample-overhead-camera`);
  tests `object_camera_and_zones.rs`, `package_camera_path.rs`.
- **Body shape and scale:** box, ball or model bodies, `set_scale`;
  [tuning.md](tuning.md).
- **Opt-in motor behaviours** an archetype may set, such as getting out of
  geometry, following the floor down stairs and momentum, are in
  [gltf-models.md](gltf-models.md), section 2 (Bodies).

### A survival or world-generation game

Generated worlds players dig into, creatures, an economy, and inventories.

- **Worlds:** `world` Add-Ons with `generate`; digging and filling in
  [rules.md](rules.md); `crates/sim/tests/mode_and_voxels.rs`,
  `trench_digging.rs`.
- **Creatures:** `entity` Add-Ons with a think rule, steering and weapons;
  `packages/stresslab/stresslab-creeper`.
- **Inventory and items:** [../runtime-inventory.md](../runtime-inventory.md),
  `crates/sim/tests/items.rs`.
- **Scoring and economy rules:** `packages/samples/sample-survival-points`,
  `packages/stresslab/stresslab-economy`.

## Across all of them

- **Audio:** weapon and rule sounds, positional sources, a music bus and
  ambient beds from rules; "Effects", "Music" and "Ambient sound" in
  [rules.md](rules.md); [../runtime-audio.md](../runtime-audio.md).
- **Looks:** materials, particles, lights, fog and sky, per-body camera
  profiles; [materials.md](materials.md), "Environment" in [rules.md](rules.md).
- **Trust:** what a player is asked before joining a server with client code:
  [trust.md](trust.md).

## When something you need is not here

Look in [seams.md](../architecture/seams.md) first: it lists every seam, who
built it and where it stands. If it is not listed, it is not built, and a new
seam is added there in the commit that starts it.
