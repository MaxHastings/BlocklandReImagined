# Vanilla avatar tool and weapon animation evidence

This note records the read-only source basis for native avatar held-arm and action layering. The original installation and recovered scripts are evidence only and are not copied into runtime content or Git.

## Script behavior

Recovered from `.research/v20-dso/client/scripts/allClientScripts-Vanilla.cs` (the sequence declarations around lines 12228–12249), the native player rig maps `armAttack`, `armReadyLeft`, `armReadyRight`, `armReadyBoth`, `spearready`, `spearThrow`, `wrench`, `activate`, and `activate2` to the corresponding `m_*.dsq` sequences.

Recovered from `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`, `Player::updateArm` chooses the right, left, or both ready sequence from mounted image `armReady` fields (around lines 9620–9678). Vanilla Hammer prefire plays `armattack`, and stop-fire plays `root` (around 10475–10480). Wrench prefire plays `wrench`, and stop-fire plays `root` (around 10814–10820). Thus held image state and temporary action state are independent inputs.

## Native sequence behavior

`content/avatar-rig-001/rig.json` is the runtime native package generated from the original rig and DSQs. Its declared arm-ready clips are non-additive and contain only their authored arm/hand channels. Action clips such as `armattack`, `wrench`, `activate`, `activate2`, `spearready`, `spearthrow`, and `leftrecoil` are declared additive and include authored channels across the full rig. `armattack`/`wrench`/`activate*` have priority 64 while arm-ready and spear sequences have priority 14. The action channels are therefore kept intact: stripping torso/head/leg channels by name would discard authored sequence matters and is not supported by the native evidence.

The engine-family reference `.research/torque-reference/Engine/source/ts/tsAnimate.cpp` sorts animation threads and applies per-sequence `rotationMatters`, `translationMatters`, and `scaleMatters` bitsets in priority order (`animateNodes`, around lines 86–150). The offline converter `crates/convert/src/shape.rs` maps the sequence additive flag bit (`0x8`) into native `Animation.additive`. Our native animation mixer likewise layers absolute clips before additive clips. These references establish that the tracks/flags are meaningful; exact Blockland engine equivalence still needs a runtime comparison.

## Runtime API and integration contract

`AvatarMesh::pose_with_animation(assets, player, time, &AvatarAnimationInput)` takes a per-actor `HeldToolPose` (`None`, `Right`, `Left`, `Both`) and optional `ActionAnimation { sequence, started_at }`. `started_at` shares the `time` domain. The existing `pose` method remains a compatibility wrapper with no held tool or action. Set `held_tool_pose` from the active equipped image(s)' authored arm readiness. Track reliable `CueKind::WeaponAnimation { actor, thread, sequence, ... }` per actor; send the current thread-2 sequence/time to `action`, and clear it when a `root` stop cue, image unequip/switch, or actor removal cancels the thread. Keep thread-1 emotes separate from this thread-2 input. Missing requested clips return a descriptive error rather than silently substituting an offset or pose.

Both rendered geometry and `world_node`/world-item mount sampling are produced by this same pose call. World items should continue consuming the resulting original `Mount0..7` and `Eye` nodes.

## Evidence limits

The source is recovered v20 script text plus native-converted sequence data and Torque-family engine source; it does not prove Blockland's closed engine's exact thread arbitration. In particular, the OpenMBU/Torque-family implementation is comparative evidence. Interactive playtest remains Maxwell's boundary and has not been performed here.

A small pure helper, `HeldToolPose::from_mounted_images`, accepts `(image_slot, arm_ready)` pairs and implements the source `updateArm` truth table: image slot 0 is right, 1 is left, false readiness has no effect, other slots are ignored. The vanilla Hammer, Wrench, and printGunImage declarations each explicitly set mountPoint 0 and armReady 1. Core tools that are not in the weapon presentation pack should be mapped from those verified source fields individually.



## First-person eye frame

Engine-family comparison found `Player::getEyeTransform` in `.research/openmbu-reference/mbg-player.cc` around lines 2950–2963. It explicitly reads position from the animated Eye node and supplies orientation from the player's head angles. The converted rig's Eye bind rotation is 180° around Y, but `main` and `start` each carry 90° around Y, so the neutral composite cancels to identity. The `look` clip animates Hip and descendants while main/start remain identity and Eye is not tracked; the raw Eye node orientation therefore does not represent the first-person pitch frame. `AvatarMesh::eye_transform(assets, view_yaw, view_pitch)` retains the actual sampled Eye world position and composes a view basis in native Y-up/-Z-forward axes. Local App integration should use current control view angles; remote actors should use authoritative player angles. The Torque-family source is comparative evidence only; Blockland's exact closed-engine implementation has not been verified.
