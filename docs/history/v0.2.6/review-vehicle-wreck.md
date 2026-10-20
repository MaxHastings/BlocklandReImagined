> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-vehicle-wreck.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: vehicle explosion white parts (PR #28), head a819214, base main a78b3a5

This was an independent, read-only review. Nothing was built or run.

## Verdict: passes. No blockers.

## The wire change is game state the host owns, so syncing it is fine

`VehicleInfo::turret_broken` mirrors `turret_damage >= TURRET_MAX_DAMAGE` on the host (`crates/sim/src/session/vehicles.rs:429-431`). The host's vehicle world already acts on that state:

- **The turret stops firing.** `weapon_available` is false once the turret is broken (`crates/vehicles/src/world.rs:380-386`).
- **The gunner moves.** The host seats them on the hull's fallback seat (`effective_seat_pose`, `world.rs:540-543`).
- **The hull's death changes.** It fires the turret explosion only while the turret is still on (`world.rs:1340-1346`).

The client can't work this out for itself, because vehicle health isn't replicated. It's one bool in an info message that is sent only when it changes, and it is not per tick. This doesn't break the "cosmetics stay client-side" rule. It is listed in `crates/net/protocol-changes/vehicle-turret-broken.md` and doesn't touch VERSION.

## The black.png fix is general, with no vehicle name checks

- **One lookup for every material.** `material_texture` (`crates/client/src/vehicles.rs:496-523`) tries the material name as written, then with `.png`, `.jpg` and `.jpeg`. It looks beside the model first, then anywhere in the pack. That's the same order the Add-On importer already uses.
- **The `turret_broken` gates read only data.** They use the definition's `attachment_model`, `attachment_mount` and `attachment_fallback_seat`, and the info flag (`vehicles.rs:799-807, 905-908`).
- **Names appear in comments and tests only.** The Tank and the Cannon are mentioned only there.

## Tests

- **The host test can fail for real.** `a_shot_off_turret_and_a_destroyed_hull_both_replicate_the_turret_gone` (`session/vehicles.rs:2458-2483`) checks that:
  - shooting the turret off sets `turret_broken` while the hull lives;
  - destroying the hull with its turret on sets both.

  It uses the real damage path.
- **The client test covers drawing and the gunner seat.** `a_broken_turret_leaves_the_hull_and_its_gunner_sits_on_the_hull` checks turret drawing and the gunner's seat for three states (`vehicles.rs:1289+`). It runs on the synthetic pack, and on content when present.
- **The texture lookup has a test.** `a_material_named_with_its_extension_finds_its_texture` covers `black.png`, `Black`, and a `.png` name found elsewhere in the pack.

## Not blocking (v0.2.7 notes)

- **Driver correction under overhangs.** The driver's predicted vehicle (`crates/sim/src/prediction.rs` `DriveSpawn`) keeps a turret collider after the turret is gone. A driver can get small corrections from the host under a low overhang. The new flag can be passed through to fix it. The PR's write-up already notes this.
- **Gunner camera after the turret goes.** A gunner whose turret was shot off keeps the turret seat's eye height and pitch limits. The host keeps the seat's role too, so the two agree. This is cosmetic.
- **Wrong texture from another folder.** The pack-wide fallback matches by filename suffix, so a material could pick up a same-named texture from another vehicle's folder. That behaviour isn't new. It runs at load time only.
- **Wreck no longer shows a turret.** A destroyed tank's wreck now has no turret, because it was always blown off first. That's a judgement call Max delegated, and it's recorded in `docs/audits/vehicle-destruction.md`. Max confirms in a playtest.
