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

## Counts on the real scripts, and the Battle Rifle

Imported Max's real MWB and 1019 scripts (the Gate's
adventure-mwb-scripts.zip) with silent stand-in sounds and a stand-in
reference for Weapon_Gun and Weapon_Hammer, in /tmp only. Needs-behaviour
ported: MWB 132/132, 1019 199/199. Emote_Critical: 0 unsupported, 0
needs-behaviour.

That showed a real gap: MWB's Battle Rifle names `battleRifleProjectile`,
which nothing declares. v20 left the field empty and the raycast script
never used it, so the gun worked; our hitscan reader refused the port. An
image with no projectile now gets a bare ray projectile carrying its damage
(test: the stand-in battle rifle in `adventure_port.rs`). Its unused
`onSmoke` is named in `handles` as never run.

## Verdicts: converted

The Gate's real-copy check on 1e987af3 (adventure-port-reports-1e98.zip)
showed MWB refused over the Battle Rifle (fixed above) and both verdicts at
converted_with_gaps. The importer now follows three more Torque load rules:

- A sound file named by a path built at load (`filename = %path @ "x.wav"`
  after `if(isFile("Add-Ons/Weapon_TF2BasicMelee/..."))` / `else %path =
  "./sounds/";`) resolves to the value under which the Add-On has the file.
  MWB's baton and machete swing sounds (tf2MeleeSwingSound) were silent
  before; they play now.
- A particle named only by emitters nothing uses is consumed, as the
  emitter is (v20 never drew it).
- A subfolder's `description.txt` is skipped, not unsupported: Blockland
  reads only the Add-On's own.

On the real scripts (stand-in sounds and reference, /tmp only) both MWB
and 1019 now read `converted`: 0 unsupported, 0 recognised-only, every
behaviour ported. Test: `built_paths_idle_particles_and_folder_descriptions`
in `crates/addon-import/tests/import.rs`.
