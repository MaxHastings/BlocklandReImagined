# Vanilla weapon debris adapter — 2026-09-26

This scoped adapter adds the stock Gun/Akimbo casing as native client-side
cosmetic debris. It does not modify or execute the original installation. The
only primary sources read were in Maxwell's designated E: v20 installation.

## Source inventory and coverage

`Add-Ons/Weapon_Gun.zip::server.cs` (SHA-256
`349f478c92f32224f2879d0f2adc95134151b42daa92b555533891f99bf04d7d`) declares
`gunShellDebris` at line 19. Its authored fields are lifetime 2.0 s, spin
−400..200 degrees/s, elasticity 0.5, friction 0.2, three bounces, static at the
last bounce, no snap, fade enabled and gravity multiplier 2 (lines 22–32). The
Gun image ejects on state 2 (line 360), with exit direction `(1,-1.3,1)`, zero
offset, 15-degree variance and speed 7 (lines 315–318).

`Add-Ons/Weapon_Guns_Akimbo.zip::Weapon_AkimboGun.cs` (SHA-256
`ec1e524e2b4b3e655d2ee69f11999180ccfb0933e6bf651226a1bd44c5092b2c`) contains
two Akimbo images with the same casing and shell parameters; each ejects at
state 2 (lines 104–107, 137, 205–208, 239). Both hands remain distinct in the
reliable cue, so the host resolves the exact image and hand attachment.

The original casing model is `Weapon_Gun.zip::gunshell.dts` (2776 bytes;
SHA-256 `52586e95b407c95f70923c7acbd61e00df0a3675b5eaadd3395b6ba731ec652d`).
Its native lowering has no converter warnings. The pack also retains original
`black50.png` and `yellow.png` bytes. The renderer has no authored cubemap or
reflectance-map pipeline, so applying those maps as diffuse color remains an
explicit rendering approximation to resolve against the original renderer.

The primary Rocket Launcher script
`Add-Ons/Weapon_Rocket_Launcher.zip::Weapon_Rocket Launcher.cs` (SHA-256
`f49e09872e7a60b829333bf0e6b82af5595f609dfe713f52a07afc049307169f`) references
`explosionSphere1.dts` at line 233. Its native DTS lowers without warnings and
contains the authored nonlooping `ambient` animation (0.33333436 s). The model
and hashes are converted into the same pack, but a transient animated model
renderer and its reliable explosion cue hook are not yet in this adapter.
Rocket line 415 comments out shell ejection. Vehicle Cannon/Tank/Jeep debris
declarations are outside this weapon-shell implementation; do not interpret
the converted Rocket shape or prior FX composites as vehicle debris support.

## Runtime contract

`crates/client/src/weapon_debris.rs` uses `WeaponDebrisAssets::load` and
`WeaponDebris::new`. The host passes ordered `WeaponShell` cues to
`cues(cues, pose, actor_velocity)`. The pose callback receives the authoritative
actor, image identity and hand and must return the original animated image's
shell-eject-node world transform. `None` waits up to 0.5 s; an absent or
unequipped attachment then expires with a diagnostic. No eye, muzzle or guessed
mount fallback is used. The state-2 image cue remains exact-once by reliable
cue ID; reset with the immutable connection checkpoint cue cursor and clear
with zero on disconnect.

Call `advance(dt, pose, sweep)` each client frame. Simulation runs at a fixed
120 Hz with at most 0.25 s catch-up per call. The host sweep callback receives
each casing segment and returns a hit fraction plus unit normal. These are
visual-only collisions; authoritative hits, damage and gameplay physics stay
in the host. The runtime caps active casings and unresolved poses at 512 by
default, and exposes each capacity/expiry count. `instances()` returns stable
cue IDs, world transforms and authored lifetime fade for the shared model.
Upload `assets().shell_scene` once through `SceneRenderer::upload` and update a
`GpuInstances` buffer from `instances()`; geometry and original images are not
re-uploaded per casing or frame.

Torque's proprietary random sampler is unavailable. The implementation records
its engine-family assumptions: it lowers source Z-up vectors by `[x,z,-y]`,
interprets `shellExitVariance` as symmetric azimuth spread about the authored
direction, and gives the `DebrisData` gravity multiplier the project's
9.81 m/s² base. Cue-ID seeded random values and fixed-step advancement make the
result deterministic for a given cue, pose and collision-query sequence.
These assumptions require later comparison with a v20 capture before parity
acceptance.

## Reproduction and evidence

The offline converter is separate from the runtime dependency graph and only
uses `bri-convert::shape::read_dts` for Torque DTS. It checks original Gun,
Akimbo and Rocket script declarations, records their archive-entry hashes,
extracts bounded ZIP entries read-only, preserves texture source hashes, and
refuses to overwrite an output pack. Runtime loading confines canonicalized
paths to the pack, bounds manifest/model/texture bytes and decoded image/vertex
budgets, checks hashes before parsing, validates DTS topology and rejects
invalid shell physics before decoding resources.

```powershell
cargo run --manifest-path docs/research/weapon-debris/importer/Cargo.toml --target-dir target -- 'E:/Downloads/B4v21Launcher/versions/Blockland v20' content/weapons-pack-003/weapons.json content/weapon-debris-pack-001
cargo test -p bri-client weapon_debris::tests -- --include-ignored --nocapture
cargo test -p bri-client --test weapon_debris_render -- --include-ignored --nocapture
```

The generated pack is ignored content. Native loader tests validate source
hashes, model topology and source image bindings; deterministic simulation
tests cover pending-pose resolution/expiry, duplicate delivery, reset,
collision bounce limits and repeatable transforms. The ignored offscreen test
uploads the source casing scene once, draws its `GpuInstances` buffer, verifies
foreground pixels, and saves
`artifacts/native-weapon-debris/gun-shell-offscreen.png`. This is a renderer
plumbing check, not visual parity or gameplay acceptance. The adapter has not
been wired into App. Root's App hook needs to consume
`HostRequest::Shell` exactly once, query the original eject-node pose from the
same sampled animated image used by `WorldItems`, pass actor velocity and a
world collision sweep, then submit `shell_scene` plus instance transforms to
the normal global scene pass. Before hookup, root should choose/implement an
authored collision query and confirm the source ejection node name and v20
variance/bounce sampling against a native capture. The Rocket explosion sphere
is converted and source-verified only; its animation/render cue handling and
all vehicle debris remain separate work.
