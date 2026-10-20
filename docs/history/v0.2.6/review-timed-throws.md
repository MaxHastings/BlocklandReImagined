> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-timed-throws.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: timed throws (grenades), claude/project-thread-uoh3ap @ a942ba48 (range 66a286b..a942ba48)

`66a286b` is the merge of fighting b6280a24 into loop 3 498d7f58. The range is 4 commits, 10 files, +1139/−167. Read against `review-design-grenades.md` and the runtime. Nothing was run here. The chaos, agreement and think-time numbers are the lane's.

## Verdict: NOT YET. One small blocker (the ray-budget formula is off by the tick rate), then one think-time measure with a grenade.

Everything else is right:
- My four design conditions are met.
- The runtime extraction is exact.
- The planned flight follows the host's own rules.
- `release_only` isn't a back door to cooking.
- The `dealt`/`push_harm` merge matches the approved ledge-push plan.
- No item names anywhere.

## Blocker: `chord_ticks` uses the wrong units, so it spends about 11× the rays it should

- `fall_per_tick` is a *velocity* change per tick: the host does `velocity.y -= fall`, then `position += velocity / 120` (`weapons/src/runtime.rs` `coast`, `TICK_HZ = 120`). The client turns it into an acceleration with `× TICK_HZ` (`app/frame.rs`).
- So an arc of k ticks sags `fall · k² / (8 · TICK_HZ)` off its chord, not `fall · k² / 8`.
- `chord_ticks` (`combat.rs`) solves the second, giving chords √120 ≈ 11× shorter than the plate rule allows.
- **Example:** gravity 20 u/s² gives fall ≈ 0.167 per tick. That's chords of 3 ticks instead of about 34. A 3 s fuse becomes ~120 rays instead of ~11, per walk, with up to 6 walks per weapon per turn.
- It's not wrong, just conservative. But it burns the shared budget, pushes walks into `pending`, and slows every bot's decision while a grenade is in hand.
- **Fix:** `((8.0 * plate * TICK_HZ / fall_per_tick).sqrt() as u32).clamp(1, PATH_TICKS)`, with the doc comment's formula corrected to match. Add a unit test checking that the sag at the returned k is under a plate and at k+1 is over, computed with `flight_position`, not the formula.

**Then measure:** the `bot_think_time_16` arsenal has no grenade, so its 672 µs says nothing about this PR.
- Run it once with a timed weapon in the arsenal: the chaos test's synthetic canister is enough. Use a one-off env switch or a second scenario, under the 10-minute rule.
- Report the number. It must stay under the 2000 µs bar.

## The four design conditions: met

1. **One bounce rule.** `bounce_velocity` is used by the host (`rebound`), the bot walk and the client ghosts (`ghosts.rs`, whose copy is deleted). The ghost keeps its own rest-speed stop and contact cap, which is prediction policy. ✓
2. **Stick and roll.** `Rebound::Stuck`, and a bounce whose `off.y ≤ fall_per_tick` on an upward-facing surface, both wait for the clock. The threshold comes from the projectile's own data. ✓
3. **Host order.** `expires` (fuse, then end of life) runs before moving. At a hit: `hit_bursts`, then `rebound` (stick, then bounce, with `bursts` on the last allowed bounce). The host's own `projectile_step` now calls the same three functions, so the order can't drift. ✓
   - The agreement test (`a_planned_burst_matches_the_host_flying_it`) flies six cases through both and checks the tick exactly: armed hit, last bounce, cooked fuse, end of life, stuck, rolled.
   - Position must match within 0.01, except "rolled". There the plan rests at the last bounce while the host rolls on, so the tolerance is launch speed × elasticity, 2 units against a 3-unit blast. That's honest, and the doc says so.
4. **Plate from `bri_content::brick::PLATE`.** ✓ (k ≥ 1 ✓; see the blocker for the units.) Refused:
   - fragments of fragments;
   - auras;
   - never-expiring fragments;
   - fragments that hurt in flight (`every_ticks`, `on_bounce`, `on_hit`).
   
   ✓

## `release_only` is not a back door to cooking

- The new rule is `!charges() || (no cook && no onFire entry reachable without trigger-up)`.
- `charges()` is "some state fires on release" (`weapons/src/lib.rs:816`).
- So a cooked image is admitted only if *nothing* fires on release, which means it throws on the press or a timeout and holding the trigger can't delay the throw. A cooked image that's held and thrown on release still has `charges()` true and a cook, so it's still refused.
- `fuse_left` then charges the throw for the ticks its states take from lighting to firing, or reads the real lit tick (`Actor::fuse_lit`) if it's already burning.
- No hold-to-cook path exists. ✓

## The `dealt`/`push_harm` merge matches the ledge-push plan

- `dealt(weapon, health, push_harm)`: `suitability` passes `context.push_harm`, which every caller sets to `PUSH_WORTH`. `rate` (threat cost) passes `PUSH_WORTH`.
- The values are identical to both parents, so there's no play change.
- The fighting lane later swaps the context value for the landing harm (with the contest worth as the floor), and decides what `rate` should read for a threat's shove. That's fighting's wiring, as agreed.

## Smaller findings (fix now if cheap, else v0.2.7)

- **Harmless fragments widen the danger zone.**
  - `fragment_reach` adds every child set's flight plus blast, including fragments that do no damage (sparks, smoke puffs). A grenade with a long-lived harmless trail child gets a huge danger zone, and bots would refuse to throw it.
  - "Danger" means where it can hurt, so only children with `damage > 0` or `explosion.damage > 0` should add reach. That's a one-line `filter(hurts)`. I'd take it in this PR, since it's in new code and decides whether a weapon is usable.
- **Allies along the throw.** Timed throws skip `clear_path` in both `choose` and `validate_fire`; the walk replaces it.
  - The walk sweeps against current bodies and bounces off them (`ally: false`). Allies near the *burst* are still kept clear by `safe_blast`.
  - But a grenade with direct `damage` that bounces off an ally on the way does hit that ally (the host's `direct_hit` runs on every hit).
  - Most grenades have 0 direct damage, so it's rare. Either refuse a walk whose sweep hits an ally body, or note it as a known gap.
- **Duplicated push-off.** The `0.001` push-off after a bounce (`normal * 0.001`) now appears in both the host and the walk. Make it a shared constant next to `bounce_velocity`, so the "one rule" stays one.
- **Timed throws aim at the feet** (`FEET_AIM`, now named alongside `SPLASH_REACH`; both were literals before). That's sensible: a grenade bounces off a body. ✓
- **The vanilla claim** ("no stock weapon becomes timed; sports balls bounce before arming but have no blast") fits the `timed && splash` rule.
  - Also confirm the other half I asked for: did any **stock** weapon go from refused to usable because the children refusal is gone?
  - Any stock projectile with cosmetic children now gets a capability (with a danger zone) where it used to be refused. List them, or say "none".

## Also flag for the merge thread

The lane reports **two `bot_tactics` failures that come from fighting b6280a24**. Fighting is cleared to merge at b6280a24, so those failures will stop its gate. Find out which tests they are and whether they fail on main too, *before* fighting goes to the gate. If they're fighting's, they go back to the fighting lane, not here.

## Re-check c424c8b0 (commits 0591238 + c424c8b)

Verdict: **passes. Merge exactly c424c8b0, after fighting b6280a24 lands** (c424c8b0 contains it).

Closed:
1. Chord units (blocker). `combat.rs` now uses `((8.0 * plate * hz / fall_per_tick).sqrt() as u32).clamp(1, PATH_TICKS)`, which matches `coast` (velocity.y -= fall; position += velocity/HZ). The doc comment is corrected. The new test `a_timed_throw_spends_one_ray_a_chord` pins sag(chord) <= PLATE and sag(chord+2) > PLATE through `flight_position`. Using chord+2 is fine because of the integer midpoint. It also checks that a 360-tick open-air throw spends `(lifetime-1).div_ceil(chord)` rays.
2. Think time with a grenade. The lane measured 507 us/tick on bot_think_time_16, under the 2000 us bar. This was a local edit and is not committed, which is OK as evidence.
3. Harmless fragments. `fragment_reach` skips a child that does not hurt, has no children of its own and has no aura. A tactics test pins spark danger = 0 next to a hurting shard.
4. Stock weapons and children. The lane reports that no stock v20 projectile has children, so refusing nested children changes nothing for stock content.

Still open (v0.2.7, not blockers):
- Allies along a timed throw: clear_path is skipped for timed deliveries.
- The 0.001 push-off constant is still duplicated; it should be one shared constant.

For the merge thread: the two bot_tactics failures from fighting b6280a24's gate (low-roof back-out, charged-equipment cancel) must be green on fighting before it merges. This branch inherits them.
