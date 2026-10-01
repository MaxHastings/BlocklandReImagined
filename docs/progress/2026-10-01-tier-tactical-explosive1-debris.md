# 2026-10-01 Tier+Tactical: Explosive 1's last datablocks

The Gate's report on Explosive 1 left 4 datablocks recognised but not
converted. Both causes are now generic importer fixes.

- Debris trails. `tierFireDebris` trails `tierFireBomb2Emitter`, but a
  debris piece's trail always resolved to the base game's emitter by that
  name, and the importer never converted an emitter only debris names.
  Now the importer converts the Add-On's own emitters that the debris of
  its explosions and casings trail. `bri_weapons::debris` resolves each
  trail to the pack's emitter by name, else to the base game's, from
  `emitters = "a b"` or `emitters[0]`/`emitters[1]` (Torque's 2 slots).
  The client draws the resolved id.
- A fifth explosion emitter. The stick grenade's explosion writes
  `emitter[4]`; Torque's `ExplosionData` has 4 slots and refused it as it
  loaded, so v20 never drew that emitter. Mentions in array slots past the
  engine's (4 explosion emitters, 2 debris emitters) no longer count as
  uses, so that emitter and its particle are reported consumed.

Melee Extended II's pin now names the zip that will be bundled (sha256
cae20f11…c4e2). The earlier hash was of a scripts-only folder. As
`porting.md` says, a copy whose hash is not pinned still gets a port when
every cover matches, and the report calls it `unlisted`.

Checks:

- `cargo test -p bri-addon-import --test import a_gun_add_on_brings_its_sounds_effects_debris_and_odd_items`
- `cargo test -p bri-client --lib explosion_debris`
- the wider weapons and addon-import runs, and clippy.

Explosive 1 and Melee Extended were pinned to scripts-only folders too;
they now pin the bundled zips (855a8cbb…b64e and be6a6478…2803). With
that, all nine bundled Tier packs ported so far show `listed`.
