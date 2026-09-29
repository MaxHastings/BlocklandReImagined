# Native weather

`bri-weather` is a main-workspace Rust 1.93 / glam 0.33 / wgpu 30.0.1 library. Its runtime reads native schema-1 JSON and PNG only. `bri-weather-import` is a separate offline package; it is not a runtime dependency. No window, audio device, physics world or GPU device is created by the library.

## Host integration

```rust,ignore
use bri_weather::*;
use bri_weather::gpu::WeatherRenderer;
use glam::Vec3;

let pack = WeatherPack::load(config.weather_pack)?; // worker/startup, not each frame
let limits = WeatherLimits::default();
let mut weather = WeatherWorld::new(pack.clone(), limits, cosmetic_seed)?;
weather.set_map(&verified_map_id)?;
// For the converted maps, choose this explicit, qualified original-family wind.
// Otherwise supply the host's dynamic weather environment in native units/second.
let wind = pack.placements_for(&verified_map_id)
    .next().map_or(Vec3::ZERO, |p| Vec3::from_array(p.reference_wind_velocity));
weather.set_environment(WeatherEnvironment { wind_velocity: wind })?;
weather.set_density(settings.precipitation_density)?; // 0..=1; 1 preserves authored count
let mut renderer = WeatherRenderer::new(
    device, queue, &pack, target_format, depth_format, sample_count,
    limits.drops + limits.splashes,
)?;

// Per visual frame: no dependence on cosmetic time for gameplay simulation.
weather.advance(dt_seconds, CameraState {
    position: camera.position,
    forward: camera.forward, right: camera.right, up: camera.up,
    velocity: camera.velocity, // native units/second
}, collision_revision, &mut |ray| {
    // Return nearest segment hit across host solid geometry AND native water regions.
    // Rapier-only queries omit map water. Use normalized world-space hit normals.
    host.closest_weather_hit(ray.start, ray.end)
})?;
let frame = weather.snapshot();
renderer.prepare(queue, camera.view_projection, &frame)?;
// After opaque scene depth exists, in a compatible host pass with depth LOAD:
renderer.render(&mut pass);

// Disconnect/map replacement: clear immediately, including cached roofs/splashes.
weather.clear();
renderer.prepare(queue, camera.view_projection, &weather.snapshot())?;
```

The client loads this pack, queries native solids/water and draws through the shared host pass. See ../../docs/runtime-weather.md for current evidence and remaining gaps. CPU-only users set `default-features = false`. `Arc<WeatherPack>` and runtime state are Send-friendly; the collision closure executes synchronously on the calling thread, with no independent physics.

`set_map` replaces all systems and resets diagnostics/time/cache. A verified dry map legitimately returns zero systems; it is not a catalog lookup error. Wind and density persist across map changes, so the host must set the destination environment. `clear` also retains those settings but removes every particle and splash. There are no global resources or background tasks.

## Collision contract and bounded work

`CollisionRay` is a world-space segment, 500 units upstream to 100 units downstream of a drop along its wind/fall trajectory. Return the nearest hit from `start`, including roofs above the camera, as `WeatherHit { position, normal, surface: Solid | Water }`. Return `None` only for a completed query with no intersection. This API does not encode asynchronous query failure; do not report an unavailable query as clear space.

Increment `collision_revision` whenever bricks, roofs, collision terrain, dynamic blockers or water surfaces change; do not increment on every unchanged frame. `invalidate_collision()` is also public. Wind changes invalidate trajectories automatically; camera teleports beyond half the smallest local volume dimension rebuild drops. Pending or invalid query results remain hidden. A low query budget can temporarily reduce weather rather than show rain through a roof. `invalid_hits`, `pending_queries` and query counts expose this behavior.

Defaults bound drops at 16,384, splashes at 8,192, queries at 8,192 per advance and catch-up at eight cosmetic ticks. Long stalls skip and record excess cosmetic time, clear stale splashes and reseed camera-local volumes. Density zero immediately clears drops and splashes. Motion is seeded per stable placement and particle slot; frame partition tests cover fixed camera/world state. Moving cameras and changing collision callbacks are inputs, so different input histories need not produce identical frames.

Snapshots contain bounded world-space billboard position/right/up, original atlas UVs, texture index, color and splash classification. They are sorted back-to-front; the renderer preserves that order in **one instanced draw** using a texture array. Layers use original texels without resampling, with transparent padding and per-instance UV scaling. The texture array is separately bounded to 128 MiB and device limits. Alpha blends normally, discards alpha below 1/255, reads host depth and never writes it. There is no white fallback. Errors identify bad references, hashes, sizes or limits.

## Validation

```powershell
cargo test --manifest-path crates/weather/Cargo.toml
cargo test --manifest-path crates/weather/Cargo.toml --test weather -- --ignored
cargo clippy --manifest-path crates/weather/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path crates/weather/Cargo.toml --example offscreen_weather -- content/weather-pack-002 artifacts/native-weather
```

The ignored test needs the private converted pack. Ordinary tests use synthetic fixtures, including a real offscreen GPU readback that checks atlas selection, alpha blending and host depth read/no-write. The gallery uses original weather resources over labeled synthetic roof/water query planes, not original map geometry. See `docs/research/weather/evidence.md` for source distinctions, conversion commands, limitations and measured results.
