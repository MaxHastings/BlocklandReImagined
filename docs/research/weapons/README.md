# Native vanilla weapons and items

`bri-weapons` is an authoritative fixed-tick subsystem with no Torque reader or physics-engine dependency in its runtime graph. `bri-weapons-import` reads the designated installation without modifying it and reuses the existing offline DTS converter. Both packages now belong to the root workspace and share its lockfile. Gameplay/client integration is still pending; the importer remains outside the client/server runtime graph.

Current pack: `content/weapons-pack-003/weapons.json`, schema 1, 17 visible items, 31 image definitions, 25 projectile definitions, 270 source declarations and 117 model/texture/icon resources. Model meshes, hierarchy, muzzle/mount nodes and embedded animation are native `bri_content::shape::Shape` JSON. All referenced models and texture/icon dependencies converted without diagnostics. Raw source declarations retain source path, line, SHA-256 and fields; they are evidence, not executable runtime scripts. Original byte resources and intermediate review files remain ignored.

## Reproduce and verify

From repository root in PowerShell; choose a fresh output directory:

```powershell
cargo run --manifest-path crates/weapons-import/Cargo.toml -- 'E:/Downloads/B4v21Launcher/versions/Blockland v20' '.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs' 'content/weapons-pack-004'
$env:BRI_WEAPONS_PACK = 'C:/Users/Maxwell/Desktop/Games/BlocklandReImagined/content/weapons-pack-004/weapons.json'
cargo test --manifest-path crates/weapons/Cargo.toml -- --include-ignored
cargo test --manifest-path crates/weapons-import/Cargo.toml
cargo clippy --manifest-path crates/weapons/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path crates/weapons-import/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path crates/weapons/Cargo.toml --example headless_probe -- $env:BRI_WEAPONS_PACK artifacts/native-weapons/probe.json
```

The pack-dependent tests are explicitly ignored by default, rather than silently succeeding without assets. `--include-ignored` runs the entire suite. One test also reads `content/map-bundle-006` and collides with its actual authored Bedroom interior. Another uses the shared native collision-recipe adapter with a thin brick, proving swept bullets do not tunnel through it. This is headless validation: no window, OS input, audio device or game launch.

## Host integration

```rust,ignore
use bri_weapons::{ActorId, Frame, Pack, WeaponsWorld, native_id};
let pack = Pack::from_json(&std::fs::read(pack_path)?)?;
let mut weapons = WeaponsWorld::new(pack)?;
let actor = ActorId(server_owned_actor_id);
weapons.add_actor(actor, 5)?;
let slot = weapons.give(actor, &native_id("weapon", "GunItem"))?;
weapons.equip(actor, Some(slot))?;
weapons.set_frame(actor, authoritative_frame)?;
weapons.trigger(actor, pressed)?;
let events = weapons.step(&mut authoritative_queries); // exactly once per 120 Hz tick
```

`Frame` carries feet position, eye position, two world-space muzzle positions, muzzle/aim direction, world velocity, scale, grounded state, mount state, horse state, first-person state and `can_jet`. All coordinates are X-right/Y-up/-Z-forward. The host derives positions from the native player/model animation, handles correct-muzzle aiming, and updates frames before stepping. Actor frame inputs reject nonfinite and excessive values. Image offsets are converted to native coordinates. `source_rotation_degrees` retains original Euler axes; original non-Euler rotation/eye-rotation declarations are retained in `definitions` for presentation integration. Those non-Euler hidden-effect mount rotations still require presentation binding.

`TargetId::{Actor(ActorId), Vehicle(u64), Brick(u64), Map(u64)}` intentionally separates identity domains. Never cast a client number into authority. The host validates requests and supplies minigame/trust policy through `Query::can_affect` and `can_catch`. `can_catch` means same eligible sports/minigame context. The host also validates proximity and item ownership before `pickup`/`use_sport_item`.

Implement `Query` using the existing shared `PhysicsWorld` and the same native brick collision query used by Simulation. `sweep` covers the full segment and returns the nearest valid surface; respect player/world filters and the source actor. `radius` supplies deterministic unique target order and closest bounding-volume distance, bounded to the supplied limit. `visible` implements explosion occlusion. The runtime performs no physics step. `Filter::projectile_age_ticks` supplies birth age for projectile sweeps (None for aim/tool queries). The adapter owns source-exclusion policy; use a brief projectile birth exclusion rather than permanently excluding an owner if self-catching/ricochet behavior is required. This source-exclusion detail needs final host fidelity verification.

`Query::on_contact(&ProjectileContact) -> ContactResponse` is an optional synchronous hook for the brick event scheduler. Evaluate authorized zero-delay projectile outputs before default impact. `Continue`, `Delete`, `Bounce(factor)` and `Redirect { vector, normalized }` are executable native outputs. Bounce uses incident velocity/normal, normalized redirect preserves incident speed, and both cap speed at 200 as the recovered core does. Native projectile identity is preserved through redirect and lifetime restarts; the original destroys/recreates a Torque object. `Event::Contact` and `BallHit` are observation records if the same input was already dispatched in this hook; do not dispatch it twice. Delayed commands must validate the target still exists. Arbitrary TorqueScript is never evaluated.

Normal state changes are driven by the imported state graph, including trigger-up/down, ammo/no-ammo, timeout, wait-for-timeout and allow-image-change. Positive durations round up to 1/120-second ticks. Zero-duration transitions can chain within the tick and have a 16-transition admission bound. `set_ammo` controls Rocket Launcher's no-ammo state. `image_state` exposes current image and state for presentation. `projectiles` and `drops` expose authoritative positions/velocities for replication/interpolation; events alone are not movement snapshots.

## Event adapters

- `Mounted`/`Unmounted` and `ImageState`: attach the corresponding original native shape at its authored mount, apply colors/offsets, and play image sequences. `Animation` selects original player threads. Akimbo also mounts the original left-hand image at hand 1.
- `Spawned`/`Removed`/`Bounced`: create/update/remove projectile presentation, using the referenced definition for original model, trail, looping sound, light and fade lifetime. These are data-driven references; weapon runtime opens no audio device. Bind sound profiles to Opus's native audio pack and effect names to the effects pack, case-insensitively.
- `Sound`, `Effect`, `Shell`: start original profile/emitter/casing presentation at the supplied source and node. Casing direction/offset/variance/velocity remain in source-backed image `definitions`; root presentation must bind them.
- `Damage`, `Impulse`, `Burn`: apply to the already-authorized actor/vehicle world state, retaining source identity for minigame scoring/last-pusher rules. Direct damage is clamped to 100 before source-scale multiplication, matching the recovered core. Radius damage/impulse uses imported explosion parameters, distance falloff and occlusion.
- `BrickImpact`: root applies source force/radius/max-volume/max-floating-volume to native fake-kill/respawn behavior, including per-brick permission checks. This never requests structural fracture.
- `HorseTransform`: set `v20.player.horsearmor`, reapply the actor's colors, and dismount. Its nominal projectile directDamage does not produce a damage event. Feed horse state back through `Frame`.
- `Key`: dispatch `onKeyMatch` or `onKeyMismatch` with normal Self/Player/Client/Minigame targets. Keys use a ten-unit eye ray and original hue-distance/greyscale rule, rather than exact RGBA equality.
- `StartSkis`: spawn `v20.vehicle.skivehicle` at the supplied feet position plus native Y 0.3 and preserve velocity; mount seat 0 after 30 ticks. `StopSkis` dismounts. `SkiNodes` shows/hides original LSki/RSki nodes; host supplies current paint color. `cancel_skis` clears pending/active ski state on failed spawn or external dismount. Feed `Mount::{None,Skis,Other}` back through `set_frame`.
- `Tumble`: spawn `v20.vehicle.deathvehicle`, force seat 0, and apply the supplied velocity. Source tackle applies a mass-scaled impulse two units below the vehicle center; root should use the vehicle agent's offset `apply_impulse` for matching angular response. `ticks: 360` is the requested duration, not a release deadline: the original active implementation polls every two seconds for speed < 1 or water coverage > .3, with a 45-second failsafe. The vehicle subsystem owns that lifecycle.

## Sports integration

`use_sport_item(actor, item_id)` mounts a loose/brick sports item directly, without a tool slot. Host consumes or respawns its item only after success. `give`/`equip` also supports inventory-based loadout adaptation; a thrown ball is consumed from its occupied slot. Catches mount the correct normal/horse image and enforce a 300 ms release cooldown. Dodgeballs eliminate eligible players for the source's 50,000 damage only before their first bounce; a bounced ball can be caught. Football/soccer rest emits `BallRest`; host creates the corresponding native loose item once and schedules its cleanup. Basketball and dodgeball continue bouncing to their source lifetimes.

Main trigger uses `trigger`. Movement/jet triggers 2–4 use `sport_trigger`: basketball switches from dribble to shooting, non-jet release passes, football non-jet trigger laterals, and soccer movement/pop drops use source velocities and pickup cooldowns. `SportAction::{BasketballPass, FootballLateral, SoccerPop, SoccerDrop}` is the explicit command API. Basketball shoot lobs use source distance/grounded/player-target rules. `SportMovement { locked }` requests/restores the original BallShootPlayer no-movement behavior for non-jet players. Character turbo/energy behavior remains root's player-type adapter, not an inert weapon selection.

`steal_basketball(actor, host_random_word, queries)` performs the five-unit ray, eligibility check and source 1-in-6 chance; jitter comes deterministically from that host PRNG word. `tackle(victim, attacker, host_random_word, queries)` performs eligibility, five-second immunity, football fumble and tumble intent. Root calls it only on the source-eligible player collision. `FootballCatch` supplies source/catcher, rounded horizontal feet (`distance * 1.875`) and whether the ball was thrown; root owns server record/preferences and original rewards/messages. `touchdown` checks any held sports ball, as the source does, and emits the brick input. `BallHit` carries the projectile identity required by event targets.

Call `drop_ball` before death/tool/brick/spray switching when a held ball should enter the world. `remove_actor` is final subsystem teardown and explicitly removes that actor's active projectiles; do not use it as the ordinary death animation transition if the dropped ball should remain. Minigame start-ball configuration, dead-player eligibility, player speed/energy variants, soccer heading/spin/continuous dribble cosmetics and source football celebration/message UI still require host adapters or further fidelity work. Native helper coverage does not establish complete Item_Sports acceptance.

## Saving, limits and verification status

`save()` returns `WeaponsSave` schema 1, including pack ID, actor inventory, held image clocks/triggers, sports cooldowns, projectile trajectory/lifetime and drops. `WeaponsWorld::restore(pack, bytes)` checks the schema, pack ID, IDs, sizes, timestamps, positions and definitions before returning a world. Root embeds this in the versioned native world save and replicates authorized snapshots. No rendering or Rapier handles are saved. The test resumes a bow mid-cycle and obtains identical future events and state.

Current explicit operating bounds: 128 actors, 16 slots per actor, 1,024 projectiles, 1,024 drops, 8,192 queued command events, four collision iterations per projectile tick and 128 radial targets per explosion query. Admission failure is returned; invalid state cycles and collision saturation emit diagnostics. A pathological fifth collision does not simulate the remaining substep. Radial target admission/overflow is a host concern: detect and report any query truncation; the current isolated adapter contract does not implement paged mass explosions. These are measurable working limits, not unlimited-content promises. Converter limits include 8 MiB scripts, 4,096 definitions/member counts, 32 MiB members and 256 MiB aggregate archive bytes; inheritance has cycle/depth bounds. Runtime packs and saves have byte limits and independent finite/index validation.

Twenty-one runtime tests and three importer tests pass. All-target Clippy with warnings denied passes. Tests cover all requested families, state cadence/charge/release, inventory, collision/explosion/occlusion, transformation/keys/skis, sports ownership/actions, redirect limits, cleanup, schema attacks and native save replay. Native Bedroom/brick tests prove collision integration with actual shared adapters. No visual/interactive feel acceptance, final shared host/client integration, LAN replication agreement, full sports player behavior or packaged alpha completion is claimed. Maxwell remains the only interactive playtester.
