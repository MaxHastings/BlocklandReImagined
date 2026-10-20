> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-wind-up.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: bots' wind-up loop (f884a925 on c424c8b0 + db76ec7c)

Verdict: **NOT YET. One blocker (b)**, small to fix. Everything else passes. Re-check the fix sha, then merge it.

## (a) One mechanism, by capability: yes

- `charged_control::keeps_wind_up(last_down, image, kept)` is `last_down && kept && image.charges() && release_only(image)` (charged_control.rs ~43). It reads only image data, with no names or ids.
- The native path's old `tracking_charge` was exactly that expression, so its behaviour is unchanged.
- The scripted path passes `kept` = enemy seen or still remembered, AND `behaviour::paused_hold(wind_up_on_target, tick, kind.hold())`.
- The test names the knife only to build or import it. That's fine in a test.

## (b) Goof click: BLOCKER, a new harm path

- `hands_goof` (surprise.rs ~1605) guards harm with `hand_combat::item_attacks`, which needs a **native capability** (combat.rs:290, "an unknown mechanism does not count"). A scripted wind-up weapon like the Butterfly Knife has none, so `attacks` is false. The goof then treats it as harmless and clicks it **on any close player** (`on = close.filter(|_| !attacks || ...)`), damageable or not, and into the air.
- Before this commit, the next tick cancelled that press, so no stab happened. That was an accident, but it kept people safe. Now `cancel_hand_charge = false` under a goof (bots.rs ~1404) lets the click release, so **the early stab lands on whoever the goof picked**: a teammate with friendly fire on, or an enemy hit "for fun".
- **Fix (one rule, by capability):** `hands_goof`'s "can it hurt" must use the conservative reader the fight code already has. Lift the per-slot test out of `has_possible_attack` (combat.rs ~232-283), "a native attack that damages or pushes, OR not `known_noncombat_manipulation` and `image_weapon(...)` is some", into an `item_may_attack(session, item, scale)`. Use it in both places, and in `hands_goof`.
- Then a scripted weapon is used only on someone the rules say it can't hurt, or just looked at, as the goof doc already promises ("an item that can hurt them only ever comes out to be looked at").
- **Test:** a goofing bot holding the synthetic knife, with a damageable player close by. That player's health never drops (zero, absolute).

## (c) Reusing the hold time: safe

- `paused_hold(scored, tick, rule)` is a pure function (behaviour.rs:307), the existing "hold an option through a pause" rule. The weapon chooser already uses it the same way: hold the weapon in hand through a spell it can't attack (combat.rs ~770).
- Reading it for wind-ups changes no other reader. `hold_seconds` isn't written anywhere new. It is the same mechanism for the same job, holding through a brief pause, which follows the one-mechanism rule.
- A behaviour note, fine: a bot that switches to Flee with the enemy still in sight keeps its wind-up held for up to the hold time, then puts it away (`cancel_hand_charge` once `winding` ends). It never releases it off target.

## (d) The test is an absolute: yes

- "Every wind-up begun while fighting ends in a full strike within charge ticks + hold, unless one of them dies", plus "at least one strike lands". The bound comes from the data (charge ticks) and the kind (hold), not a tuned band, and the exception is explicit. Sanity-only rules respected.

## (e) Merging with fighting

- This commit itself merges cleanly with fighting's tip 6de797e in `bots.rs`. The new field and the trigger block don't overlap fighting's changes.
- **Heads-up for the merger:** the capabilities stack and fighting **already conflict in `spots.rs`**. This comes from timed throws 3e5ae24, not from this commit (`git merge-tree 6de797e c424c8b0`, so it's already there between the two branches). Fighting's new `bot_capability(owner, empty_hand)` closure needs the extra `&self.weapons.pack.projectiles` argument that timed throws added to `hand_combat::capability`. It's a one-line resolution: keep fighting's version and add that argument inside `of`.

## Blocker fix 1bfaba28 (on f884a925): passes, merge exactly 1bfaba28

- **One copy.** `hand_combat::attack_of(session, image, scale)` (combat.rs ~280-298) is the old per-slot body of `has_possible_attack`, moved out unchanged:
  - a native capability that damages or pushes gives `Some(Some(cap))`;
  - a native capability that does neither gives `None`;
  - otherwise `!known_noncombat_manipulation && image_weapon(...)` gives `Some(None)`.
  
  `has_possible_attack`, `hands_goof` and the goof tool swap all call it. `known_noncombat_manipulation` is now called only from it and from the existing descriptor check (~377). No second copy is left.
- **Still by capability:** it reads image data and the bot's own fire reader. No names.
- **Harmless tools aren't suppressed wrongly.** A tool the bot reads natively with zero damage and no push (a spray can, a wrench or hammer if their imported data gives a capability) returns `None` before the scripted branch, so it's harmless as before. An unknown scripted tool with no manipulation descriptor now counts as "can hurt". For a goof that only means it is used on someone the rules say it can't hurt, or just looked at. The error is on the safe side, and the cost is a little goof flavour for odd scripted tools.
  - **Ask (one line, no new test):** print `attack_of` for the stock hammer, wrench, printer and spray can once on the PC, to confirm they stay `None`. If any reads `Some(None)`, its import is missing a manipulation descriptor. That's a data fix, not a code fix.
- **The remaining `item_attacks` callers are "worth" readings** (pickup, spare weapon for a teammate), which is the right use. Two are really **threat** readings and keep the native-only view: `contest.rs` ~80 (is an enemy armed?) and `team.rs` ~471 (is a player attacking?). A player stabbing with the scripted knife reads as unarmed or not attacking there. That's not a harm path and not changed by this PR. **v0.2.7 note:** switch those two to `attack_of`.
- **Reproducing the old bad click** (so the chaos guard can pin it):
  - Calm map, no enemies, friendly fire on.
  - A bot holding only the synthetic knife, and a still teammate inside `HAND_REACH` in front of it.
  - Goof strength high, with every flavour weight except Tool at 0 (`flavour_weight`), so every pause becomes a hands goof on the close teammate.
  - On f884a925 the teammate's health should drop within a few click cycles (`CLICK_TICKS`); on 1bfaba28 it stays at 100.
  - If it still doesn't reproduce, check that `on` picks the teammate (`close` and the `clear` aim gate).
