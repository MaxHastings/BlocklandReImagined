# 2026-10-04 Mini-Nuke: a bundled original that holds up on a 75k-brick build

Max asked for Bushido's Mini-Nuke (`Weapon_Mini_nuke.zip`, his Steam
Add-Ons, sha256 7d3e6507…8ea3) as part of the game for v0.2.4: everything it
has, and the climax of his video is nuking Badspot's Christmas Block Party
(about 75k bricks) without the frame rate collapsing.

## Bundled

- `packages/default-addons.json`: `weapon_mini_nuke`, off at start, credited
  to Bushido, pinned to that copy. Files never enter the repository;
  `tools/addon_bundle.py` imports the pinned copy for releases.
- Port `crates/addon-import/ports/weapon_mini_nuke` (verified): `onMount`
  raises both arms (`armReadyBoth`), `onUnMount` plays root; the image's
  `both_arms` does both. Its `onUnMount` calls `Parent::onMount`, which only
  sets the emptied slot's ammo. Everything else (the 65 u/s ballistic
  missile, 9000 damage in 25 units, 94000 impulse in 49, bricks in 29 units
  up to volume 290, the 90-piece debris burst, the ring, overlay and smoke
  emitters, the 60-80 unit light, the camera shake, both sounds, the
  launcher and missile models and icon) is the Add-On's own data.
- The real copy now imports as `converted`: 19 of 19 datablocks, 2 of 2
  behaviours ported, no gaps.

## Engine gaps the import showed (every Add-On)

- **Missing particle texture draws the cloud.** The overlay emitter (the
  90-unit flash) names `base/data/particles/star`, which no Blockland has.
  v20's `ParticleData` preload (0x558c60 in blocklandv20.exe) loads
  `base/data/particles/cloud.png` whenever a texture does not load and
  never fails. The client left the particle, its emitter and part of the
  explosion out. Now it draws the cloud (`weapon_effects::MISSING_PARTICLE_TEXTURE`);
  the import report says so.
- **`exec` of a base Add-On's script** (`exec("add-ons/weapon_rocket_launcher/weapon_rocket launcher.cs")`)
  loads that Add-On, which the base game always has: a dependency (`how:
  exec`), not an ambiguity.

## Big-build performance

Headless: the real import in a hardlinked copy of the content, the 75,155
bricks of `Slate/Badspot's Block Party Christmas 09.bls` hosted on Slate,
a networked shooter nuking the middle of it, and a real `App` joined over
loopback rendering offscreen at 1920x1080 (RTX 4070 SUPER, Vulkan). The
probes were scratch tests, not committed.

| | Before | After |
|---|---|---|
| Bricks knocked out | 5,041 | 5,041 |
| Host blast tick | 17 ms | 17 ms |
| Client worst frame, blast | 6.0 s | 49 ms (p99 20 ms) |
| Client worst frame, respawn 30 s later | 17.6 s | 57 ms (p99 25 ms) |

Causes and fixes:

1. **Debris cues pushed out the blast's own cues.** One `BrickKill` cue per
   brick filled the 4,096-cue presentation queue; on a 55k-brick test pile
   the explosion and its sound were dropped. A blast now announces at most
   `MAX_BLAST_DEBRIS` (2048, a client's highest Physics Quality) of its
   bricks, spread over the blast; every brick is still knocked out and
   respawns (`docs/audits/brick-damage.md` rule 5).
2. **Unannounced knocked-out bricks faded out one model each.** Clients
   now take every hidden, intangible brick within a blast's reach out at
   once, as they already did announced ones (`app::fx::settle_blasted`).
3. **Fade models uploaded every brick surface image each.** A brick easing
   in or out (a respawn, `setRendering`, a repaint) built a scene with its
   own copy of the brick textures: about 10 ms each, 512 at once on a
   respawn. They now draw with the chunk palette's textures, as debris does
   (`FadeModels::upload`, `upload_palette_model`).
4. **Brick index updates shifted buckets per brick.** `grid::Index` gained
   `remove_many` and `insert_many`; the client's three building indexes
   take a whole update in one pass.

## Tests

- `crates/addon-import/tests/ports.rs mini_nuke_port_holds_it_up_with_both_arms`
  (CC0 stand-in in `tests/fixtures/ports/Weapon_Mini_nuke`).
- `crates/addon-import/tests/import.rs real_steam_mini_nuke` (the real copy
  where Steam's Add-Ons and the v20 reference exist).
- `crates/client/tests/weapon_effects.rs a_particle_whose_texture_is_missing_draws_the_cloud`.
- `crates/sim/tests/brick_damage.rs a_blast_through_thousands_of_bricks_keeps_its_explosion`.
- `bri_sim::grid::tests::batched_changes_match_one_brick_at_a_time`.
- `bri_client::app::fx::tests::bricks_a_blast_knocks_out_without_a_cue_do_not_fade`.
- `bri_client::brick_fade` tests check fade scenes carry no images of their own.

## Not done

- `brickExplosionMaxVolumeFloating` (390 for the Mini-Nuke) is still not
  modelled; see the audit's "Not verified" list.
- Default Physics Quality keeps 512 debris bodies and sheds to what the PC
  affords inside a dense build (66 at the blast's centre on the test PC),
  as for any blast.

## Next

Max turns the Mini-Nuke on in Add-Ons and nukes the Christmas Block Party
in the v0.2.4 build. The bundle needs rebuilding and uploading
(`python tools/addon_bundle.py upload`) after this merges so releases
carry it.
