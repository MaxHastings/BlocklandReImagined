> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-capabilities-loop3.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: capabilities loop 3 (plan row 4), claude/project-thread-uoh3ap @ f8115830 (range c753eada..f8115830)

This is a read-only review of the whole range (52 files, +1437/-655), judged against my `review-design-capabilities.md` decisions. Nothing was run here. The parity, digest and content-test evidence is the lane's.

## Verdict: NOT YET. One small blocker, plus three merge steps for the PC thread.

The core is right and matches the decisions:
- `stock.rs` is gone.
- `OnFire` wraps `HostTool`, so PR #29's `host_tool` callers are unchanged.
- Sports checks read `Image::sport` / `ProjectileDef::sport_hit`.
- The client `Equipment` match on item ids is gone, replaced by `Tool(HostTool)` from the item's image.
- Arms and left hands come from one script reader that both importers share.
- The per-tick digest tour of all 21 stock items, identical between main and the branch, is a strong proof of "no behaviour change". The dodgeball edit shows the tour can notice a change.

## Blocker: two name rules survive in the runtime, and the guard test misses them by a line break

`runtime/sports.rs` (in this PR's diff) still decides things from datablock names:
1. **The horse variant** (`mount_ball`, `sports.rs:32-38`): it builds `native_id("image", "horse" + <ball image name>)` and uses it if the pack has it. That's a behaviour picked by name.
   - The guard test `no_datablock_names.rs` bans `native_id("image"`. It passes only because the call is written `native_id(` with a newline before `"image"`.
2. **The released ball** for pass, lateral and pop (`sports.rs:158-177, 258, 318`): it spawns `"basketballProjectile"`, `"footballProjectile"` and `"soccerBallProjectile"` by name.
   - The lane calls these "references, not a match on a name". But they're the runtime naming stock content. An Add-On ball with its own projectile can't pass or pop.

The project rule was that datablock-name checks live only in `runtime/stock.rs`, and that file is deleted. So these have no allowed home. This PR adds `Sport` and bumps the schema, so declaring them now is cheap. Doing it later costs a second schema bump and a second content rebuild.

**Fix:**
- Add `horse: Option<String>` (the image to hold on a horse) and `released: Option<String>` (the projectile a pass, lateral or pop spawns) to `Sport`. The importer fills them from the same table it already uses for the vanilla import.
- Make `mount_ball` and the sport actions read them.
- Tighten the guard: ban `native_id(` outright in the three runtime files, since a call split across lines slips past a pattern match. The digest tour then shows nothing changed.

## Merge steps for the PC thread (not blockers, but the merge breaks things without them)

1. **rustfmt the touched files** before the gate. The lane couldn't, and CI checks fmt.
2. **Rebuild the main checkout's content *before* the gate run:** `python tools/bootstrap.py --rebuild weapons --rebuild item_presentation`.
   - That checkout's weapons pack is unstamped (copied in), so bootstrap keeps it.
   - The new runtime refuses a schema-3 base pack: `Pack::validate` → "Unknown weapon schema" (`weapons/src/lib.rs:2721`). So the gate's `bri-client --check` and Max running from the checkout both fail until it's rebuilt.
   - It fails loudly, not silently. The message should say what to do, which is a cheap change I'd take in this PR: "weapons pack schema 3, this build reads 4: run bootstrap --rebuild weapons --rebuild item_presentation".
   - Then take the three "v0.2.6 bot work" entries off `gate-known-failures.toml`, as the lane says.
3. **Before the next preview build**, re-upload `ci-content` (`python tools/ci_content.py upload`) from the rebuilt checkout, and re-upload `addon-bundle` (`python tools/addon_bundle.py upload`). Preview builds take content from those draft releases, not from the PC.
   - A stale `ci-content` would fail the release's `--check` with the same schema error.
   - A stale `addon-bundle` loads, since Add-On weapon parts are merged without a per-part schema check (`weapons/src/merge.rs`), but it lacks the arm cues and akimbo left hands the new importer reads.

**Max's real installs:**
- A **preview or release folder** carries its own content from those uploads. It's fine once they're re-uploaded.
- **Older release folders** keep their own binaries and content, so nothing changes for them.
- **Add-Ons Max imported in an older build** still load. Their missing arm and left-hand fields only drop presentation cues. Re-importing fixes that, and the alpha has no back-compat promise.

## Scope: both extras are in, under the finish-line rule

- **onStopFire/onAbortCharge plays root only when the state has no arm of its own.** That's a direct consequence of this PR reading arms from scripts. Without it, a script's own `playThread(2, root)` would play twice. It's in code being changed, so it's in.
- **The admin wand showing non-rendering bricks** is a consequence of `Equipment::Tool(_)` replacing the four hard-coded variants: the wand is now a tool like the others, as v20's `AdminWandImage` (`showBricks = 1`) was.
  - Keeping the old behaviour would need a wand special case, which is exactly what this PR removes. So it's in.
  - It's player-visible. Put it in the release "Fixes" list ("the admin wand shows non-rendering bricks, as in v20").
- **`OnFire::Mount`** (the basketball swap) goes beyond the design note's three variants, but it replaces a name lookup the runtime had in two places. That's exactly the job, so it's in.

## No datablock-name checks outside the allowed place

Apart from the blocker:
- `bri_weapons::scripted_arm_pose` (`weapons/src/lib.rs:79-90`) still picks the thread-1 ready pose by image name. It's client presentation, pre-existing and untouched here, and the lane recorded it as the next read-from-script case. It's OK for v0.2.7, but record it as a known exception, since the rule's home (`stock.rs`) is gone.
- `session/weapons.rs` `PlayerBurnImage` / `PlayerTeleportImage` are engine status images the host mounts, not weapon behaviour checks. Fine.
- The remaining `v20.weapon.*` ids are in tests, a probe binary and the synthetic pack only.
- The importer's own name table (`weapons-import/src/stock.rs`) is conversion tooling, which is exactly where the decision put it.

## Smaller notes (v0.2.7)

- `HAMMER_DAMAGE` and `TOOL_RANGE` are named constants. That beats bare literals, but they duplicate data the pack already holds (`hammerProjectile.directDamage`; the range is in `hammerImage::onFire`'s script). The next loop could read the damage from the hammer image's projectile.
- `stock_parity` needs the v20 install and `.research`, so CI never runs it. That's fine, since it's a one-off proof. Say in the test's doc that the digest tour is the ongoing guard.

---

# Re-check: 7a09cfab (a724900a harm reader + 7a09cfab review fixes, on f8115830)

## Verdict: the blocker is closed and the harm reader is right. Merge after one tiny commit that does only rustfmt plus the error-string fix below. Send me that sha, or the PC thread confirms its diff is formatting plus that one string.

## The blocker: closed

**The horse variant:**
- `Image::riding_image` is declared, validated (another image of the same pack), and filled by the importer from `passBallCheck`'s `"horse" @ %image` rule (`weapons-import/src/stock.rs:83-85`), which is conversion tooling where it belongs.
- `mount_ball` reads it. No name is built in the runtime.

**The released ball:**
- Pass, lateral, pop, drop, steal and fumble all spawn the held image's own `projectile`. No stock projectile name is left in `sports.rs`, and an Add-On ball now passes as its own projectile.
- On a horse the held image is the riding variant, so its `projectile` is what's thrown. v20's horse images are declared `: basketballImage`-style copies, so that's the same projectile as before. That's inferred: the digest tour doesn't ride a horse. Low risk.

**The guard test:**
- It now strips whitespace before matching, bans `native_id(` outright, `image.rsplit(` and `format!("horse` in the runtime files. A split call can't slip past it any more.

## Harm reader (a724900a): approve

- `fall_damage` is the one formula. It covers:
  - min impact speed × scale;
  - the admin wand;
  - the mini-game Falling rule or the host setting;
  - `SPEED_DAMAGE_SCALE`;
  - the guard share.
- `step_combat` calls it, then plays the clang and applies damage. `fall_harm` is the read-only face of it.
- Behaviour is identical to the old loop: same order of checks; the clang plays only when a guard takes a share, as before.
- `guard_fall` is split into `guard_fall_share(&self)` and `guard_fall_clang(&mut self)`, as asked.
- There's no world bound for players, as the plan says.
- The test is real: soft = 0, hard > 0 with Falling Damage on, 0 with it off, and nobody is hurt by asking.
- `Capability::push` is now `(along, up)` with `pushes()`, and its truthiness is unchanged. `Context.push_harm` is validated finite and non-negative.
  - Callers pass `PUSH_WORTH`, so play is unchanged. The doc on `PUSH_WORTH` says the fighting lane deletes it.
  - Per my ledge-push review, that lane moves the value to the contest worth rather than deleting it outright. Same constant either way.
- `fall_harm(owner, impact)` takes a `Vec3`, which matches `shove_landing`'s returned impact (2fd8ced). The two hand-offs fit together.

## The case-insensitive test change is not a loosening

- `spear_short_charge_cancels_long_charge_releases` now accepts `spearthrow` for `spearThrow`.
- The client lowercases every action name before it looks up the clip (`client/src/avatar.rs:1467`, `action.sequence.to_ascii_lowercase()`), so case carries no meaning in playback.
- The test still requires the throw animation event. Only the spelling's case is relaxed. Fine.

## The one thing to fix: the schema error string

- The message has long runs of spaces inside the literal (`weapons/src/lib.rs:2728`, "…packs with              `python tools/…`, which              also…"). It looks like a line break that turned into spaces.
- It's the text Max sees when the checkout's content is stale. Use `\` line continuations, or shorten it to one line.
- rustfmt won't fix a string literal, so it needs a manual edit alongside the pending rustfmt pass.

## Still for the PC thread at merge (unchanged)

- Run rustfmt.
- Run `bootstrap.py --rebuild weapons --rebuild item_presentation` on the main checkout *before* the gate.
- Take the three "v0.2.6 bot work" known failures off the list.
- Before the next preview, re-upload `ci-content` and `addon-bundle`.
- Release notes: the admin wand shows non-rendering bricks.

---

# db900cd1 (on 7a09cfab)

The change is clean. It touches one line, the error message, which now has single spaces. Merge db900cd1, plus the merge thread's formatting-only commit.

---

# Gate fixes 498d7f58 (on the format-only 45df79e6, main df1198cc merged)

All three are test fixtures catching up with declared data. None is a loosening. Merge exactly 498d7f58.

1. **`vehicle_trails.rs`:** the inline weapons pack now uses `bri_weapons::SCHEMA` instead of a hard-coded 3. Nothing it asserts changed.
2. **`loopback.rs` `tool_pack`:** the stand-in core tools now declare `OnFire::Tool(Break/Inspect/Print/Destroy)`, as real content does. The wrench claim is right:
   - the importer maps `wrenchimage` to `OnFire::Tool(HostTool::Inspect)` (`weapons-import/src/stock.rs:106`), with its own assertion (`:183-184`);
   - the 21-item digest tour includes `v20.weapon.wrenchitem` (`sim/tests/stock_digest.rs:48`).
   
   So players' wrenches still open bricks. The test still asserts the wrench opens the brick.
3. **`minigame_screens.rs`:** the count goes from 65 to 66 for the `ffa_bots` setting that 88c4704 (10-05, "Slayer: free-for-all bots in Deathmatch") added. It surfaced only once content was regenerated. That's a real setting, now counted, with a comment.
