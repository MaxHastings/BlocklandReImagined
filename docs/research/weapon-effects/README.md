# Native weapon effects adapter — 2026-09-26

`crates/client/src/weapon_effects.rs` is an isolated presentation adapter. It is
not normal App integration or full weapon fidelity acceptance. No interactive
playtest, visible window, input automation or audio playback was used.

## Native resources

The additive offline importer creates `content/effects-runtime-pack-002` from
pack001 and the source-hashed native weapon declarations in weapons-pack-003.
The designated E: reference is read-only; the recovered core must match its
recorded hash. The importer remains outside the runtime dependency graph.
It reuses the established `bri-convert` particle/emitter lowering and preserves
every existing native definition and original compressed texture byte.

Pack002 adds 12 particles, 12 emitters and six explosion composites: Horse Ray,
sports ball trails, cannon smoke/fuse/trail, cannon base explosion, tank smoke
and tank shell/body/turret/final explosions. The additions need no new texture:
all use the original 18 textures already present. Totals are 131 particles,
132 emitters, 40 lights and 47 composites. New particle/emitter conversion
diagnostics: zero. Every referenced trail and transient resource in all 25
native projectile definitions and image states resolves (35 unique transient
definitions). That does not mean each composite's separate mesh/debris behavior
is implemented.

Reproduction (the output directory must be fresh; never overwrite pack001):

```powershell
cargo run --manifest-path docs/research/weapon-effects/importer/Cargo.toml --target-dir target -- 'E:/Downloads/B4v21Launcher/versions/Blockland v20' content/effects-runtime-pack-001 content/weapons-pack-003/weapons.json content/effects-runtime-pack-002 .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs
python docs/research/weapon-effects/verify.py 'E:/Downloads/B4v21Launcher/versions/Blockland v20' content/effects-runtime-pack-001 content/effects-runtime-pack-002 content/weapons-pack-003/weapons.json .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs artifacts/native-weapon-effects/independent-verification.json
```

The independent Python verifier checked 27 script hashes, all 18 original texture
hashes, baseline preservation, and added particle/emitter timing and motion
fields. It is an independent conversion check, not a visual equivalence oracle.
Generated source proof/definitions, PNGs and native packs stay ignored.

## Host binding

1. Construct `WeaponEffects::new(fx_pack, Arc<weapons>, limits)`. It derives native
   point-light definitions from typed projectile radius/color fields. Textures
   retain their existing ordering, so `world().snapshot(camera)` can use the
   same native particle renderer. The private EffectsWorld owns its own source,
   particle and light budgets; merging world/weapon snapshots also needs an
   aggregate renderer budget.
2. On checkpoint/session change call `reset(checkpoint.cue_cursor)` before any
   cue dispatch. On disconnect clear with `reset(0)`. Initial synchronization
   starts current projectile trails/lights; it never fabricates impact bursts
   from disappearances or replays historical one-shots.
3. Call `sync(&WeaponView)` each frame, optionally with interpolated projectile
   positions. Stable IDs preserve emission clocks. Transform changes update the
   existing handle. Removal drains particles and removes lights. Finite authored
   trails keep a tombstone until removal and do not restart every frame. Source
   capacity rejections are observable and retried on subsequent synchronization.
4. Call `cues(&ordered_reliable_cues, pose)` once data is committed. Repeated/old
   IDs are ignored with constant-size cursor state. A whole invalid or unordered
   batch rejects before any cue side effects. Missing bindings/poses and resource
   rejection consume the cue and increment diagnostics rather than replay later.
5. Call `advance(dt, wind, pose)`. Pose lookup must resolve the cue's exact
   actor/vehicle, image ID, hand and node against the current animated mount. It
   must return `None` when that attachment no longer exists. The emitter's local
   axis is +Y; transform the authored node's forward axis accordingly. Supplying
   an actor eye position or guessing the Akimbo hand is not a valid pose binding.
   Existing particles drain when attachment lookup fails.
6. Drain all `take_host_requests()` values. Shell cues need source casing/debris
   model/ejection metadata plus an animated eject-node pose and bounded shell
   physics. Animation cues need the correct avatar/image thread, sequence and
   mount lifetime. These are exact-once host requests, not executed animations
   or shell rendering. Unconsumed queue capacity is 4096, with explicit drops.

Image/drop/static-item meshes belong to the item presentation adapter. They do
not imply particles: state entry cues own mounted emitter starts. Mounted image
duration reconstruction for a late join is not currently represented by
WeaponView (it lacks state entry tick/remaining emitter duration), so a joining
client starts trails/lights and only future state transients. Long-lived cannon
fuse/fire image effects need that checkpoint state to reconstruct mid-state.

`EffectsWorld::set_remaining_lifetime` caps sources inside their emission clock.
It never extends authored finite lifetimes. Long advances cannot emit beyond
the cue's source duration; particles retain their own lifetimes afterward.
All count limits are explicit; warning strings are capped at 128 and host
requests at 4096. These cosmetic rejections never affect server gameplay.

## Reliable pose metadata

Weapon runtime Effect events and session cues now preserve image ID and hand for
state emitters, normalized actual aim, and source scale. Collision/bounce/stick
effects carry the actual collision normal and projectile scale. Lifetime-death
effects have no fabricated normal (`direction: None`). The adapter uses received
impact normals; node poses come from the host callback. Cue validation rejects
partial image/hand identities, invalid hands, nonunit/nonfinite directions,
invalid scales and zero Actor/Vehicle/Brick IDs. Map ID zero remains valid.
The current unreleased protocol8 carries these fields; no defaults conceal an
older protocol. Replica validation remains atomic before world mutations.

Projectile emission uses an axis opposite velocity, with native +Y for zero
velocity. This is grounded in the engine-family
[GarageGames Projectile::emitParticles implementation](https://github.com/GarageGames/Torque3D/blob/development/Engine/source/T3D/projectile.cpp#L873),
which passes negative normalized velocity to the emitter. That source supports
the implementation choice but is not proof of proprietary v20 engine behavior.
The original emitter angles, speeds and inherited velocity remain unchanged.

## Evidence and open work

Eight adapter tests pass, including the explicitly enabled native-pack test.
Coverage includes move/interpolation, duplicate delivery, late join baseline,
session reset, removal/draining, finite source non-restart, capacity retry,
bounded warnings/host requests, atomic invalid batches, backward trail/received
normal orientation and exact finite lifetime across a long frame. Native sample:
25 projectiles, 35 unique transient definitions, 1254 particles and 12 sources,
zero missing bindings/capacity failures.

Additional checks: presentation unit tests 2 passed; malformed weapon cue codec/
replica atomicity test passed; all 24 weapon runtime tests including native inputs
passed; native Gun session tick/cue test passed; FX runtime 9 passed/1 explicit
private-content ignore. All-target Clippy with warnings denied passes for
client/sim/net/weapons/fx-runtime and the standalone importer.

```powershell
cargo test -p bri-client --test weapon_effects -- --include-ignored --nocapture
cargo test -p bri-sim --lib presentation::tests
cargo test -p bri-net --test replication invalid_weapon_pose
cargo run -p bri-fx-runtime --release --example offscreen_gallery -- content/effects-runtime-pack-002 artifacts/native-weapon-effects
```

The existing bounded offscreen renderer produced and visually inspected
`original-emitters.png` and `original-composites.png` in
`artifacts/native-weapon-effects`. They show original-texture output including
the added Horse Ray/sports/cannon/tank resources. GPU: NVIDIA GeForce RTX 4070
SUPER. The separate 1000-source, 240-step crowd sample measured median0.7887ms,
p95 1.0958ms; its deliberately capped 20000-particle budget dropped29123
particles and reported zero skipped emissions/source rejections. GPU preparation
0.4574ms, 960112 uploaded bytes. This is isolated FX evidence, not combined
game/network performance acceptance or subjective visual sign-off.

Open integration/fidelity requirements:

- Actual App render/cue/mount binding, disconnect/checkpoint reset and combined
  renderer budget, plus proper replication of mid-state persistent emitters and
  an offscreen render through the combined App path.
- Shell model/debris metadata and ejection poses; checkpoint restoration of
  active avatar action sequences.
- Explosion meshes: Rocket's original explosionSphere1.dts, and cannon/tank/Jeep
  debris resources/lifecycles. The existing composite representation retains
  explicit unresolved entries rather than substitutes.
- Source scale is preserved and validated, but EffectsWorld has no authored
  particle-placement/size scale. Exact v20 scale semantics, zero-normal expiry
  orientation, Pong bounce's absent lifetime default, spherical burst placement,
  and light curves/falloff remain fidelity investigation requirements.
- Network cue-age compensation, cross-client seed/phase alignment, exact authored
  muzzle/ejection poses, audio loops, camera shake and gameplay remain host work.

No alpha acceptance item was checked off by this isolated adapter.

## Normal client App integration — 2026-09-26

`ContentConfig::default` now selects `effects-runtime-pack-002`. App constructs both
brick `WorldEffects` and `WeaponEffects` from that native pack; weapon projectile
light definitions are added without changing the pack's original texture order.
Projectile trails synchronize each entered-session tick. Server weapon effect,
shell and animation cues enter a bounded App queue (4096). The immutable cue cursor
from the welcome checkpoint initializes the adapter before subsequent deltas;
the moving replica cursor is deliberately not used for resets. Ordered cues wait
for sampled mount poses for up to 0.5 seconds (two capped 0.25-second frames),
then missing poses are consumed and counted. Disconnect resets effects and cue
state. Duplicate cue IDs remain consumed once.

Both effect snapshots now share the same texture-backed renderer frame. App merges
particle instances before global back-to-front sorting, caps renderer lights at
256 nearest lights, and exposes the number deferred through `weapon_effect_backlog`.
The doubled GPU instance budget accounts for both 65,536-particle worlds plus
both light-flare allowances. Source-capacity rejections and retry counts remain
available through WeaponEffects diagnostics. Brick effects retain their existing
adapter and lifecycle.

The normal avatar sampling path consumes thread-2 animation cues as authored
avatar sequences, gates them by owner/hand, and clears them on a `root` cue,
identity switch, unequip or actor removal. Held-arm readiness comes from each
mounted weapon image's original `armReady` bit and actual hand slot. Selected
Hammer, Wrench and Printer use the separately audited vanilla `armReady=true`
values; Wand readiness is not inferred. WorldItems receives the corrected Eye
frame from `AvatarMesh::eye_transform`: local camera yaw/pitch for the local
owner and authoritative player yaw/pitch for remote owners. Shell presentation
still has no model/ejection physics/render adapter. `take_avatar_animation_requests`
transfers thread-2 actions to App by cue ID while retaining shells and other image
threads; its queue test exercises transfer and the 4096 bound. Shell output remains
pending and undrawn with observable capacity diagnostics. App acknowledges these
adapter requests after dispatch into its own bounded avatar-action queue, so the
adapter does not retain duplicate copies of already-owned thread-2 cues.

App evidence:

```powershell
cargo check -p bri-client
cargo test -p bri-client --lib app::tests::app_weapon_effect_path_consumes_cues_once_and_syncs_projectile_trails -- --ignored --nocapture
cargo test -p bri-client --lib app::tests::native_weapon_catalog_startup_and_headless_host -- --ignored --nocapture
cargo test -p bri-client --lib app::tests::world_and_weapon_effects_share_depth_order_and_nearest_light_budget
cargo test -p bri-client --test weapon_effects shell_animation_queue_is_exact_once_and_bounded
cargo clippy -p bri-client --all-targets -- -D warnings
```

All checks completed successfully on 2026-09-26. The real-pack App test verified a
nonempty native projectile trail and finite transient cue, delayed missing-mount
handling, duplicate suppression and checkpoint reset. The headless QUIC host test
verified normal core-tool mount and drop projections, zero missing pose/binding
diagnostics, and disconnect cache cleanup. The pure combined-frame test verified
cross-world particle ordering and that the nearest light survives a full cap with
one deferred light reported. These tests used no visible window, gameplay input,
GPU App frame or audible output. Prior bounded offscreen pack002 rendering
remains separate evidence; the normal App render pass has not yet been exercised
offscreen.

Remaining integration gaps include checkpoint reconstruction of mid-state image
emitters/action animations, shell models and ejection poses, exact muzzle nodes
under the newly layered avatar animations, composite debris meshes, cue-age/seed
alignment across clients, and a bounded offscreen render through the combined App
path. Weapon/source scale, several authored bounce/burst/light semantics, loops
and camera shake also remain fidelity work.
