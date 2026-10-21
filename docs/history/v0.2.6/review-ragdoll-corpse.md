> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-ragdoll-corpse.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: ragdoll budget + corpse collision (93fbffae, 2 commits on df1198cc)

Verdict: **passes with one small fix. Merge the sha with it.** The branch merges cleanly onto 498d7f58 (`git merge-tree`, no conflicts).
- The fix: on a map change, `Surroundings::sync` must also drop the loaded bricks and terrain (see b).
- If the lane shows Add-On physics bodies are always cleared on a map change or load, 93fbffae is fine as is.

## (a) Corpse group

- **Group logic is correct.**
  - A corpse is `memberships CORPSE, filter ALL`.
  - Player motor queries use `MOVES_AGAINST` (memberships ALL, filter ALL minus CORPSE, And mode). A corpse's memberships AND that filter is 0, so it is skipped.
  - A living body is `all()`, so it is still hit.
  - GROUP_1 is used nowhere else in the workspace (I searched `Group::`/`InteractionGroups`), so nothing else is caught by accident.
- **Both death paths set it** (combat.rs ~1209 and ~1359), and respawn clears it (~2236). The 5 s timeout's sensor switch is unchanged.
- **Server vs client:** the client's prediction (`motion.rs`) already left corpses out, so the server and the client now agree, and walking into a corpse is no longer corrected back. That is the lane's reading, consistent with the old symptom; I didn't re-read motion.rs.
- **Still solid to everything else:**
  - Weapons, vehicles and grabs use default filters, so they still hit and hold corpses.
  - A corpse's own motor query includes bricks (default groups), so it still rests on bricks.
  - Small quirk, acceptable: a corpse still lands on a living player's head (its query includes living bodies) while the living player can walk out from under it. Two corpses pass through each other. Neither is something a player would care about.
  - Resting on a moving vehicle is unchanged, since vehicles aren't filtered.
- **Bots:**
  - Nav and `shove_landing` copy only fixed colliders (`QueryFilter::only_fixed()`, reach.rs on 2fd8ced), so corpses never were nav obstacles, before or after. The copy also keeps `collision_groups`.
  - Targets and shoves go through `alive` checks.
  - `Player::clear` (spawn and landing checks) now ignores corpses, so a player can spawn in a corpse. That's correct, since corpses don't block.
  - No bot code reads collision groups. No change in either direction.
- **Test:** `living_players_walk_through_a_fresh_corpse` failed before the fix (blocked at 0.75) and passes after. Good.

## (b) Surroundings::sync: stale colliders

- **Within one map, it's correct.**
  - Every loaded brick is re-checked against `Building::solid_brick(id)` by shape `Arc` identity and pose. A removed, moved, reshaped or no-longer-solid brick gets dropped.
  - A content or Add-On reload makes new shape Arcs, so those bricks drop too, which errs on the safe side.
  - Removing anything wakes all bodies.
  - A brick new to a sleeping body's box wakes it.
  - Both tests pin it, and an id reused with the same shape and pose is the same solid, so that's fine.
- **Gap: map change and load.**
  - When `map_generation` changes, `sync` now replaces only the map colliders. Loaded **terrain chunks** stay, and before the change they were dropped by the blanket `clear` that every build change ran. Bricks are re-checked by id against the new building, but terrain isn't, so the old map's terrain stays solid and the new map's terrain chunks are never loaded, because their keys already exist.
  - Also, a fresh `Building` starts `query_generation` at 0. If `Surroundings.generation` happens to equal it, the brick check is skipped too.
  - This only matters if Add-On bodies (ragdolls) survive a map change or a save load. I couldn't confirm that they're cleared. `addon_physics` clears its surroundings only when it has no bodies.
  - **Fix (one line):** in the `map_generation` branch of `sync`, call `self.clear(world)` before reloading the map. A smashed breakable also bumps `map_generation`, which is rare, so a full reload there costs nothing that matters.
- **Minigame reset:** bricks are reloaded through the build, so the id and pose check covers it. Same map.

## (c) Multibody to impulse joints

- **Named and justified:** `JOINT_PASSES = 4` (addon_physics.rs ~61), with its measurements in the doc: 0.004 stretch with 4 passes against 0.027 with 1.
  - The 0.05 limit is the existing blast test's.
  - The note answers the original reason for multibody (0.19 stretch on a rocket landing): that number was with swept CCD, since replaced by soft CCD. The blast test's worst is now 0.004, and the real-Blockhead content checks pass.
  - No magic numbers added.
- **Cost:** 14 ragdolls average 1.06 ms (2.3 ms worst), never over the 4 ms budget, against 1 ms *each* before. That fixes the reported "ragdoll stops" directly.
- **What players might notice (inferred, not measured):**
  - PGS joints with motor friction tend to be a little softer or springier than an exact multibody. Expect slightly floppier limbs, and maybe a little jitter when a pile of ragdolls rests on each other.
  - The stretch numbers say limbs won't visibly separate (0.004 is well under a pixel at play distance).
  - `num_internal_pgs_iterations` applies to the whole Add-On physics world, contacts included, which also makes stacked bodies a bit stiffer. The 1.06 ms figure already includes that.
  - **Ask Max to look at deaths by explosion and ragdoll piles in the preview.** Joint feel is the one thing tests can't judge.
- `carried_mass` walks the impulse-joint graph with a seen-set, so it can't loop. The test holds a 3-body chain by its end.
- **Small:** brick debris still calls `wake_up_all` on *any* build change (`sync` returns true even when nothing was stale). That's harmless (debris isn't budgeted, and it matches the old behaviour), but the comment "resting debris looks again for its support" overstates it. Fine for v0.2.6.

## (d) Through the floor: v0.2.7 note, no fix now

- The probe covered the obvious cases: drops from up to 150 units onto decks, 0.2 plates and one-layer floors; 50 u/s kills into the floor; build churn. Nothing passed through. The reported fall-back to the death animation matches the budget stop.
- **The likeliest remaining causes, all inferred:**
  1. **The old build-change rebuild itself.** Before this fix, every build change dropped every brick round every body, and they were reloaded only for the reach box. A body moving faster than its reach box predicts could step once with the brick under it missing. This PR removes that path, so the bug may already be gone.
  2. **A ragdoll created overlapping a thin plate**, with the corpse pose's feet or hand inside the brick top. Depenetration can push a limb down through a 0.2 plate, and its joints drag the rest after it.
  3. **Bodies faster than `MAX_SPEED`** outrun soft CCD's lookahead. Bodies are capped at it, so this is unlikely.
- Recommendation: v0.2.7 note. Ask Max for the save and the spot if he sees it again in the preview. If it recurs, check (2) first: spawn the ragdoll with the corpse half inside a plate in the probe.
