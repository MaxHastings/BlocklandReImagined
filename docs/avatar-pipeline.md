# Native Blockhead rig and animation layers

The original Blockhead now renders in the native client and avatar editor, using
original textures and stock customization rules. Reliable appearance replication
and initial movement/look animation selection are connected. Full animation timing,
equipment/emotes, interpolation and visual acceptance remain required.

## Offline assembly

The `avatar_bundle` converter reads the recovered stock `mDts` declaration at
`.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs:8417` and assembles
already-converted native geometry/clips. It never executes scripts. Literal
declarations must have contiguous indices and unique case-insensitive aliases;
unknown fields, expressions, missing clips and unbound tracks fail conversion.

```powershell
cargo run -p bri-convert --bin avatar_bundle --locked -- 'C:\Users\Maxwell\Desktop\Games\B4v21-Launcher-Release\versions\Blockland v20' content/maps-pass-003 .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs content/avatar-rig-001
```

The output directory must be new and outside the original installation and native
input package. The completed package is `content/avatar-rig-001`: 82 nodes,
44 named objects and all 39 stock sequence aliases, with source hashes, native
hashes and constructor line provenance. The manifest hashes the assembled rig
and recovered constructor source. The rig is 891,965 bytes. Files remain ignored
original-derived content. Re-running requires a new output directory.

Aliases matter: `walk` and `run` share a source clip; `jump` and `standjump` use
the stock stand-jump source. Many tool gestures have the same embedded clip name.
The constructor alias is therefore the native gameplay lookup key. Four unused
old player clips are not silently attached merely because they exist on disk.
This pass covers the stock Blockhead constructor; other vanilla player rigs and
vehicle/weapon animation bindings remain required.

## Runtime composition

`bri-content::animation::sample_layers` combines ordered absolute channel layers,
then additive local transforms. An absolute clip changes only channels it authors,
so raising an arm does not reset a running lower body. Weights are bounded to
0–1; rotation uses quaternion interpolation. Additive transforms compose locally
and do not move the authoritative physics body. Clip ground motion and trigger
records remain preserved data; their gameplay use is not implemented here.

The caller chooses layer priority, time and weight. The API rejects an absolute
layer after additive layers instead of pretending an arbitrary decomposed/sheared
transform reproduces the intended result. Visibility channels blend; mesh and
UV frame indices switch at half influence. This is a native composition contract,
not yet a claim of exact Torque transition or animation-thread behavior.

`SceneData::append_shape` uses the native pose, named-part selection, per-part
paint, caller-resolved materials and original UVs. `GpuScene::update_vertices`
updates a model with stable topology/materials and new batch centers, preserving
uploaded indices, material bindings and textures. Appearance/topology changes
require a new upload. Writes occur once per scene per submission. CPU pose/mesh
evaluation is the initial path; no crowd-performance or GPU-skinning claim.

## Evidence and remaining integration

```powershell
cargo test -p bri-render --test persistent_scene original_avatar_layers_update_one_persistent_gpu_scene --release --locked -- --ignored --nocapture
```

The offscreen test samples all 39 aliases at six times (234 poses), checks finite
posed geometry, and renders running, running with a raised right arm, and running
with both the arm and look layers. It checks that the arm layer preserves leg
poses and that the rendered frames differ. All three frames reuse one GPU scene
upload. Rejected non-finite/wrong-size updates leave the last frame unchanged.

The original layer-only evidence remains in `artifacts/native-avatar/layers-*.png`.
Those historical images use diagnostic paint. Current textured client/editor
evidence is in `artifacts/native-client-flow/`.

## Original materials, customization and client integration

```powershell
cargo run -p bri-convert --bin avatar_material_bundle --locked -- 'C:\Users\Maxwell\Desktop\Games\B4v21-Launcher-Release\versions\Blockland v20' content/avatar-rig-001 content/ui-pack-003 content/avatar-pack-001
```

The new package has 63 byte-preserved original PNGs: 27 faces, 28 decals and eight
model surface textures. Original archive/member paths, checksums and dimensions
are retained. Face/decal scope comes from the native UI catalog and retains its
earlier distribution-inventory limits. No community package is promoted to vanilla
merely by being present. Runtime loading verifies contained paths, hashes, image
dimensions and aggregate memory bounds before decoding. It never reads Torque.

Stock body rules come from `GameConnection::applyBodyParts` and `ApplyBodyColors`
at recovered server script lines 3796 and 3866: skirts replace legs, trims use leg
colors, hats restrict accents, and packs raise the head. Head/body paint is opaque
and quantized as `AvatarColorCheck`; accents preserve bounded transparency as
`AvatarColorCheckT`. Unknown/invalid choices reject atomically, including fields
currently hidden by a skirt. Surface/face/decal alpha overlays pigment; separate
blend bindings handle translucent part paint. Exact material reflection/detail
behavior and all appearance under every lighting condition remain fidelity work.

The client renders original geometry from authoritative player poses and retains
GPU resources across animation updates. Initial state selection covers root,
forward/back/side movement, crouch variants, jump/fall and look; packs add the
head-up layer. First person hides the local body. Remote poses are
interpolated (`crates/client/src/motion.rs`). Strafing right plays the side clip
backward; held tools raise the arms through the original `armReady` sequences;
dead bodies hold `death1`; the `sit` emote holds the sit sequence
(`crates/client/src/avatar.rs`). Movement-rate matching, transitions,
footsteps/triggers and other gestures remain work. CPU posing is not a claim of
acceptable eight-player/bot load performance.
Mid-session LAN-name/clan changes are not yet replicated; this command currently
changes appearance, while the established connection retains its joined name.

The editor's preview uses a separate renderer/camera, a 2:3 portrait target and
the authored light/FOV fields (`Avatar_Preview`, recovered GUI lines 13995–14010).
The 35-degree FOV is interpreted horizontally; exact orbit/FOV parity awaits
fidelity testing. The transparent target preserves the original background. It
renders to an sRGB attachment but exposes its encoded bytes through a UNORM view,
matching the UI texture convention. Previewing never publishes an outfit; Done
uses the existing acknowledgment/settings workflow. Translucent preview edges
still need compositing scrutiny.

Protocol 3 carries server-validated appearances in reliable checkpoints/deltas,
independently of movement datagrams. Another client, late join and same-process
authenticated resume see the accepted outfit. The runtime content fingerprint
includes the rig, customization tables and every declared avatar image. Changing
the catalog changes the identity; corrupt image bytes fail the hash check.

Verification includes 108 native part/accent/image binding cases, all 63 texture
hashes, skirt/hat/pack rules, hidden invalid choice rejection, actual QUIC outfit
updates/late join/resume, and App → server → third-person/editor rendering. The
App test also verifies that previewing another outfit leaves server state intact.
No visible window or OS input was used.

```powershell
cargo test -p bri-client --lib original_outfits_materials_and_customization_rules --locked -- --ignored --nocapture
cargo test -p bri-net --test loopback original_avatar_changes_replicate_late_join_reject_invalid_and_resume --locked -- --ignored --nocapture
cargo test -p bri-net --lib runtime_identity_includes_avatar_catalog_and_rejects_changed_image_bytes --locked -- --ignored --nocapture
cargo test -p bri-client --test app_flow --release --locked -- --ignored --nocapture
```
