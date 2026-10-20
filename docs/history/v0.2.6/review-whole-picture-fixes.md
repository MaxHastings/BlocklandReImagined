> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-whole-picture-fixes.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: whole-picture fixes (capabilities lane)

Branch origin/claude/project-thread-uoh3ap-v026-fixes = **1418df5b629d19aa97e3507f6615347228b2ddff** (3 fixes merged onto main fd719c94).

## Verdict: PASS. Merge 1418df5b.

| Commit | What | Result |
|---|---|---|
| aaddb4f4 | `holds_attack(session, owner)` (combat.rs ~346) over `hand_combat::attack_of`; contest.rs, team.rs and surprise.rs now use it instead of inline `item_attacks` | ✅ An enemy holding a scripted Add-On weapon now counts as armed (test an_enemy_holding_a_scripted_weapon_is_armed) |
| 6eeb91aa | Deletes client `scripted_arm_pose` name table; importer reads `playThread(1, armReady*)` into `Image::both_arms/arm_ready` and `State::raised_arms`, propagated along state transitions until a script lowers them; client frame.rs reads the data; SCHEMA 4→5; name-ban test widened to every runtime crate | ✅ Parity test checks the old table's arms against the new data; scripts_raise_arms_for_their_states covers propagation |
| 39db2bda | Deletes dead `route::landing` | ✅ trivial |

Checked in the cloud: `cargo test -p bri-weapons --test no_datablock_names` passes on 1418df5b. The test now walks every runtime crate (importers/test tools excepted), lists 6 known uses with reasons, and fails if a known use disappears so the list can't go stale. Two entries (actor_effects.rs, sim session/weapons.rs: emote, pain, burn, teleport images) are old name tables already on the v0.2.7 list.

Not blocking (v0.2.7): `bot_unarmed` (extras.rs ~690) still uses `item_attacks`, so a teammate holding only a scripted weapon is treated as unarmed when bots decide whether to hand it a spare. Pickup/hand-off semantics, not threat; harmless extra hand-off.

## Player question: do old imported Add-Ons re-import after the 4→5 bump?

**No. This is a v0.2.6 player bug (it already existed with the 3→4 bump).**

- `crates/package/src/classic.rs` `plan()` (~193-225) re-imports a dropped Add-On only when its file `Stamp` changes. `Record` stores name, stamp, id, dir, error and included; there is no importer or schema version.
- So a player who upgrades in place, or copies their old `content` folder into the new build, keeps conversions made at schema 3 (v0.2.5) or 4. Every one fails to load with "Unknown weapon schema N (this build reads 5): regenerate the packs with `python tools/bootstrap.py ...`" (weapons/src/lib.rs:2719) and is left out. That hint is a developer command shown to players.
- A fresh folder plus re-dropped zips works, which is why our tests don't see it.

**Right fix (import plan, not this branch):**
1. `classic::Record` records the importer version (the weapon SCHEMA, or one importer stamp covering every schema the classic importer writes).
2. `plan()` emits `Step::Import` with `replaces` when the recorded version differs from the current one, same as a stamp change. Records without a version count as stale.
3. If the Adopt path (existing import dir with no record) can meet an old conversion, it re-imports too.
4. The player-facing message for a schema mismatch says the Add-On will be re-imported (or to re-drop it), never the bootstrap command.
5. Test: a record written with an old version and an unchanged stamp produces an Import step; then a load of the result succeeds.

The release Add-On check (95e9d469) only covers the bundled set, so it won't catch this; the test in step 5 is the guard.
