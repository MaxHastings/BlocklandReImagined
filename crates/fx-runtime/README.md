# Native effects runtime

This Rust 1.93 workspace crate owns cosmetic particle simulation, original-texture
billboards, flare presentation and light snapshots. It has no Torque reader,
window, audio device, network authority or gameplay clock. The host supplies time,
attachment transforms, visibility and effect intents.

The default local pack is `content/effects-runtime-pack-001`: 119 particle
definitions, 120 emitters, the 13 original fxLight definitions, 21 explosion
flashlights, 41 explosion groups, 208 literal resource relationships and 18
original image files. All images and 75 original add-on script inputs were checked
against the user-designated E: installation; the additional core input is the
previously recovered and hashed script. `source-proof.json` retains the evidence.

## Host API

```rust,no_run
use bri_fx_runtime::*;
use glam::Vec3;

# fn example() -> anyhow::Result<()> {
let pack = EffectsPack::load("content/effects-runtime-pack-001")?;
let mut fx = EffectsWorld::new(pack.clone(), EffectsLimits::default(), 1234)?;
let jet = fx.start_emitter(
    "v20/emitter/playerjetemitter",
    SourceTransform::default(),
    SourceOptions::default(),
)?;
// Once per presentation frame, after the host updates/interpolates actor transforms:
fx.update_source(jet, SourceTransform {
    position: Vec3::new(2., 3., 4.),
    velocity: Vec3::new(1., 0., 0.),
    ..Default::default()
})?;
fx.advance(1. / 60., Vec3::new(0.2, 0., 0.))?;
// Retain existing particles when a jet/projectile trail ends.
fx.stop(jet, StopMode::Drain);
// One-shot composed spawn effect. Host independently dispatches its sound.
let group = fx.play_composite(
    "v20/explosion/spawnexplosion",
    SourceTransform::default(), SourceOptions::default(),
)?;
// Immediate stop is useful on map/session cancellation.
for handle in group { fx.stop(handle, StopMode::Immediate); }
fx.teardown();
# Ok(()) }
```

- `SourceTransform` contains Y-up world `position`, normalized `rotation` and
  inherited `velocity`. Local +Y is the emitter axis. Emissions interpolate
  between the previous and current source transform. Existing particles stay in
  world space. Update transforms before `advance`; teleport by stopping/restarting
  a source if a connecting trail is undesirable.
- `SourceOptions` controls emission time scale, scaled placement volume, local
  wind, four override color/size keys, emission enable, visibility, first-person
  owner status and host-computed flare line-of-sight fraction. Existing attached
  particles receive updated wind/visibility/override keys on the next advance.
  After drain-stop, particles keep their last source parameters.
- `brick_source(pack,id,&BrickAttachment)` applies original world-box thresholds,
  tiny-brick node choice, paint override and six direction codes. `fake_dead`
  pauses emission. Plain render invisibility is a separate host option; hidden
  bricks can intentionally retain effects. Script directions are converted from
  Torque `(x,y,z)` to native `(x,z,-y)`.
- `burst` returns a removable particle-tail handle without retaining a live
  source. `is_active` refers to a live emitter/light, not a draining particle tail.
  `stop(Immediate)` removes tail particles even when it returns false because the
  source has already ended. Handles never recycle, including after `teardown`.
- `play_composite` starts authored explosion emitters and optional flashlights,
  limits their source lifetimes, and generates the authored density burst. The
  returned handles form a host-owned group. Damage, sound, camera shake, debris
  meshes and gameplay transitions remain host responsibilities.
- `pack.bindings_for("v20/projectiledata/gunprojectile")` exposes literal fields
  such as `explosion`. Owners intentionally retain their source datablock class
  (`projectiledata`, `playerdata`, `shapebaseimagedata`, `wheeledvehicledata`);
  resource values are native emitter/light/explosion IDs. Hosts with shorter
  gameplay namespaces should resolve by source symbol once during catalog load.

## GPU integration

```rust,ignore
let mut renderer = bri_fx_runtime::gpu::EffectsRenderer::new(
    device, queue, &pack, target_format, depth_format, sample_count,
    limits.particles + limits.lights,
)?;
let camera = Camera {
    view_projection, position: eye, right: camera_right, up: camera_up,
};
let frame = fx.snapshot(&camera);
renderer.prepare(queue, &camera, &frame)?;
// After opaque geometry, with the host depth attachment loaded:
renderer.render(&mut render_pass);
```

The adapter uses the host's wgpu **30.0.1** device/queue and render pass. Its
native features match bri-render. It has six pipelines: three blend modes, each
with particle depth testing or flare depth bypass. No effect writes depth.
Original PNG/JPEG pixels are uploaded as sRGB textures. Vertex colors remain
linear; original fixed-function display-space comparison is still required.
Camera-facing particles spin; oriented particles stretch their axes toward the
camera while retaining velocity/emission direction. Alpha sprites are sorted
back-to-front; only adjacent identical texture/mode runs are batched, so texture
batching does not reorder overlapping alpha sprites. Mixed-texture depth order
can produce many draw calls; `RenderStats` reports this explicitly.

Flare dimensions use the authored radius, near/far curve, animated luminance
link and size fade. Like the inspected engine-family implementation, flares do
not use scene depth directly: set `flare_visibility` from the host's line-of-sight
query and `first_person_owner` appropriately. Leaving visibility at its default
1 means the host has declared the flare unobstructed.

`FrameEffects::lights` contains world position, brightness-scaled RGB and radius.
`GpuLight::from(snapshot)` is a 32-byte POD storage-buffer record. A concrete
SceneRenderer helper is in `examples/scene_lights.wgsl`; a GPU test compiles that
layout. The host binds its buffer, active range/count and call the helper in the
scene material shader. The sample attenuation is explicitly native and
unshadowed; original falloff and light occlusion remain integration acceptance.

## Limits and errors

Default budgets: 4,096 sources, 65,536 particles, 256 lights, 32,768 emission
attempts per advance. Particle overflow drops new particles, not gameplay work.
Source/light overflow returns an error. Emission-overdue work is capped and
reported; the skip count is an estimate based on mean period when variance is
enabled. Large elapsed times expire old particles at full elapsed time rather
than freezing effects or forcing gameplay to a cosmetic step. Linear drag uses
closed-form integration instead of the inspected engine's frame-dependent Euler
step. This is stable and tested, but exact original trajectories need comparison.

Load validates schema, canonical containment, references, hashes, image headers,
dimensions and limits before decoding. Native images cap at 4,096 per axis,
32 MiB encoded each, 256 MiB decoded collectively. Unknown effect IDs and stale
update handles return errors. IDs are not replaced with white placeholder effects.

## Reproduction

Run from the repository root, without a visible game window:

```powershell
cargo test --manifest-path crates/fx-runtime/Cargo.toml
cargo test --manifest-path crates/fx-runtime/Cargo.toml --test runtime -- --ignored
cargo test --manifest-path crates/fx-runtime/Cargo.toml --test gpu_contract -- --ignored
cargo clippy --manifest-path crates/fx-runtime/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path crates/fx-runtime/Cargo.toml --example offscreen_gallery -- content/effects-runtime-pack-001 artifacts/native-effects-runtime/release
```

The runtime and importer are root workspace members.
The native client reconciles replicated brick lights/emitters, draws particles and
occluded flares, and feeds animated point lights to the shared scene renderer.
Source budgets select nearby attachments and expose deferred counts. Player and
vehicle effects run from `crates/client/src/actor_effects.rs`, weapon effects
from `crates/client/src/weapon_effects.rs`. Point lights
are currently unshadowed with a native smooth falloff; this is not final fidelity.

See `docs/research/effects-runtime/coverage.md` for evidence and remaining fidelity
work. This subsystem and its gallery are not full-alpha or integrated-playtest
acceptance.
