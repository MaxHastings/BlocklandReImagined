# Native world item presentation adapter

`crates/client/src/world_items.rs` is a read-only projection from the host's
`WeaponView`, shared tool inventory and an avatar pose sampled from the same
pose used by visible avatar geometry. It owns no gameplay state and does not
invent mount offsets: a missing requested mount suppresses that item and adds a
bounded diagnostic. The callback is sampled at most once per owner in a sync.

Static items and drops use the converted item presentation table. Core tool
inventories fill a missing mounted core image from the same table; weapon images
continue to come from the authoritative mounted-image state. Pack-003 marks every stock ItemData rotation flag false; static models retain
the authored placement orientation without invented spinning. Football uses the mounted image's authored color
shift when active. Projectile appearance, tint, lifetime and animation sequence
come from the native presentation and weapon packs.

Models are cached by model and tint. Each distinct sampled animation pose gets a
reusable geometry slot; equal model/pose candidates share one GPU scene and one
instance buffer. Textures/material bindings upload once per model/tint, while
posed geometry reuses those bindings. Capacity exhaustion is counted as deferred
work. Active models retain their uploads; models and pose slots absent from the
current projection are released, so old equipment cannot permanently consume
capacity. Local held-item groups take priority over nearby world-item groups. `reset` releases session caches; `clear_gpu` drops device resources while
retaining CPU models and pose state for recreation.

Mounted animation state follows the weapon pack's named sequence. Positive state
timeouts map the authored sequence duration onto the timeout. When a new state
has no sequence, the adapter retains the last sampled sequence, matching the
audited engine-family behavior. The available source corpus is not a complete
closed-v20 engine implementation, so these semantics remain qualified. No
static-item spin is added because all 17 shipped ItemData declarations omit or
disable `rotate`.

Drop fade uses the source-recovered final five 24-tick alpha steps multiplied by
the linear 1000 ms fade request. This product is an explicit family-based
adaptation: the closed v20 interaction between node alpha and fade is not
verified. Projectile opacity follows the authored lifetime/fade ticks. Emitter
orientation converts the model's native -Z forward vector to the effects
runtime's +Y axis and uses the authored muzzle node, falling back only to the
source engine's `muzzlePoint` rule for a missing requested effect node.

The API is `WorldItems::new`, `sync`, `upload`, `draws`, `mounted_node`,
`effect_pose`, `clear_gpu`, and `reset`. The application should call `sync`
after replica/host state and avatar sampling are current, upload on the render
device, then pass `draws()` into `SceneRenderer::render_with_instances` alongside
normal world geometry. The adapter expects the caller to own the model-to-avatar
pose association and session reset boundary.

Focused evidence is in `crates/client/tests/world_items.rs`; native model and
texture provenance is documented in `../item-rendering/README.md`. These tests
are technical adapter checks, not interactive feel or original-engine parity
acceptance.

Root integrated this adapter into normal App tick/render/reset. Avatar sampling
now runs before item projection and includes the invisible local body in first
person. Held-arm/action animation integration is tracked separately; mounted
geometry alone does not close that fidelity requirement.

Verification (root, 2026-09-26): `cargo test -p bri-client --test world_items --
--include-ignored` passes all five tests. The added capacity test changes models
under a one-model/one-slot limit and checks held-item priority and reclamation.
The added actual draw test compares original Gun geometry under two placements
against independently CPU-transformed geometry (pixel tolerance one), verifies
nonempty pixels, reproduces the same image after GPU reset, and checks a cleared
session draws nothing. Asset-dependent tests are ignored by default because the
converted game content is local and intentionally excluded from Git.
