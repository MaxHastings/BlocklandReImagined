# 2026-10-05 Ragdoll vanish puff

Max (v0.2.4): with the Ragdoll Add-On on, the puff of smoke a corpse
vanishes in came out somewhere else, where the body died, not where the
ragdoll lay.

Cause: the server spawns `deathProjectile` (`Player::RemoveBody`) at its
corpse's feet. The ragdoll is client-only drawing (`avatar.pose`), so it
lies wherever it slid; no second corpse is kept, the puff just never knew
where the body was drawn.

Fix (client only, no wire change):
- `avatar::follow_drawn_bodies` moves an unmounted effect a player's body
  makes at itself (source is that player, position on its game body) by
  the body's drawn offset; a sound at the same tick and place goes with
  it. Other players' blasts and shots landing elsewhere stay put. Runs on
  every cue batch in `drain_events`.
- `AvatarMesh::drawn_offset` is now remembered each frame and kept once
  the corpse is hidden (5 s), until a new body spawns: a late puff and the
  death camera stay on the body where it was last seen.
- A hidden corpse no longer offers Add-On code a skeleton, so the Ragdoll
  drops its limbs then, as its own comment says, instead of keeping
  invisible bodies in local physics until respawn (debris and the Gravity
  Gun could hit them).

Evidence: `cargo test -p bri-client --lib a_corpse_vanishes` passes
(synthetic); `cargo clippy -p bri-client --all-targets -D warnings` clean;
`cargo test -p bri-client`: 473 passed, 5 failed only for "no wgpu
adapter" in the cloud container (GPU tests). Needs Max's eye in a playtest
with the Ragdoll on.
