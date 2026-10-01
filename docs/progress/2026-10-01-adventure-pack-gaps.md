# 2026-10-01 Adventure Pack: every behaviour run or accounted for

Ports: `weapon_adventurepack` (Glass 1019), `weapon_modernwarbattles`, and
Emote_Critical beside them. Each leftover in the Gate's real-copy reports
was checked against Bushido's scripts; what the game did not do became a
generic seam, and the rest is named in the port's `handles` (Tier's map,
980fe7e3), the hl2 ammo system's shared entries in
`ports/_shared/bushido-hl2.json`.

Engine seams:

- `State::cues`: arm moves and sounds a state script scheduled by hand
  (`%obj.schedule(450, playThread, 2, plant)`).
- `Shot::rested`: the truer first shot after a pause.
- `Magazine::per_load`: the Paired Shotgun's two shells a load.
- `Hitscan::eye_within`: a muzzle shot starts at the eye when something
  stands within that distance (MWB `checkForObstruction`, 4.5 units).
- `Hitscan::converge`: a muzzle shot heads for where the eye looks (1019
  sniper `getLOSPoint`).
- Add-On particle emitters and lights with a `uiName` join the wrench's
  emitter and light lists (Emote_Critical's glow), as v20 listed them.

Import and ports:

- Magazines `calls` and `scripts`: the hl2 system's own functions and the
  gun methods that only work rounds count as read.
- Script rule filter `{group|seconds}`; MWB rifles draw the beam their
  `onFire` spawned (`drawRaylineRifleTracer`, deleted after its ms).
- MWB guns play their `raycastExplosion*Sound`; the Baton and Machete
  leave theirs to the rules' random pair, so a swing is heard once.
- The frag grenade plays its bounce sound (`bounce_effect`).
- `include` keeps a port's `null`s (it merged with RFC 7396, which
  dropped the 1019 port's `"critical": null` value and any patch removal).

Tests: `cargo test -p bri-addon-import` (adventure_port, ports, tier_port,
import), `cargo test -p bri-weapons --test tactical_seams`, the
content_identity and tool_ui tests. The stand-in MWB fixture gained CC0
`checkForObstruction`, `shrapGrenProjectile::onCollision`, a machete and
the revolver's beam and hit sounds.

Assumed, not read: the 1019 beam's width (0.1, from MWB's comment on the
model) and colour (white, from `setNodeColor("1 1 1 1")`).

Next: the Gate re-imports the real copies to confirm the reports.
