> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-ledge-pushes.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Ledge pushes: claude/v0.2.6-fighting-pushes-ki8k6d at 107dd555

Reviewed 024146e7 (the push code, which I had only seen as a design before)
and the new 32262346, 2e448233, 47b0c5ec and 107dd555. The merges of
608cf674, d233e7fc and 2fd8ced are of reviewed shas.

**Verdict: FAIL. Two must-fixes and one small fix, all local. No sha to merge yet.**

## 1. Push worth: sound in shape, one bug
- push_harm is max(fall harm at the measured landing, contest worth), capped
  at the target's health.
  - The fall harm comes from `Session::fall_harm`: the game's own rule,
    read-only, with the hardest impact on the way down.
  - The contest worth is `CONTEST_SHOVE_WORTH`, the old PUSH_WORTH moved to
    its one job.
- `worth` adds it to the attack's own damage, capped at health.
- No weapon names. Velocity is impulse over PLAYER_MASS along the shot and
  up. The flight is on the solver budget (≤ PATH_TICKS a candidate) and kept
  off-turn per target and slot.
- Falling damage off, into nothing, and along the deck are each worth 0
  (tested). These match the design review.

**MUST FIX: the target's size is scaled twice.**
- `shove.rs:64-68` does `peer.player.tuning().clone().scaled(state.scale)`
  when scale ≠ 1.
- But `Player` already stores its tuning scaled (motor/player.rs:1104,
  `self.tuning = tuning.scaled(scale)`).
- So a scaled target is flown at scale², and the landing and impact are
  wrong for any setScale'd player.
- That is exactly the "nobody scales it twice" warning in
  review-shove-landing.md.
- Fix: `let tuning = peer.player.tuning().clone();`. Add a unit test: scale 2
  gives the same tuning width as the live player's.

Note for v0.2.7: `tactics::rate` no longer counts push at all, so a
push-only threat (a broom) now costs a place nothing. A bot near a ledge
won't mind an enemy broom. Its consequence would need the same landing
read. Not a blocker.

## 2. 47b0c5ec: honest fixture, but the bound is now mostly idle
- **Honest.** `bot_best_attack` gives a pusher `push = health`, because
  where a push lands is known only per place. damage + health, capped,
  equals health. That is admissible. The test's claim ("only here is
  weighed") is about the bound, and holds for an attack that doesn't push.
  So changing the fixture's gun is not a loosening.
- **Correct.** Pushers really must weigh every place, given the bound uses
  only weapon data.
- **Cost bounded.** The work runs inside `Budget::share` (78a400e6), each
  flight is ≤ PATH_TICKS, and a place it can't afford gets no shot. The lane's
  release think time is 595 µs/tick against a 2000 µs bar.
- **But say it plainly:** v20's guns carry an impact and vertical impulse
  (the synthetic gun does too), so most real weapons push. With real
  content, the "a bot holding a good spot weighs one" saving rarely happens.
  The budget share is what bounds cost. Don't claim the saving in release
  notes.
- v0.2.7: a tighter push bound, e.g. no fall harm is possible from a place
  with no drop within the push's reach.

## 3. knocked_off: CAN hide a self-caused fall. MUST FIX
- "Struck" here means an enemy's projectile was *fired* on a straight line
  passing within 1 unit of the body's centre (where the body stood at fire
  time). It isn't a hit or a push.
  - A dodged shot counts.
  - A shot from a weapon that doesn't push counts.
- On the rooftop, bots are shot at all the time, and strafing happens under
  fire. A bot that strafes itself off the deck while a missed bullet is in
  flight is filed as knocked_off. That is exactly the fall `fell == 0` is
  meant to catch.
- The run's knocked_off=1 can't be told apart from such a fall.
- **Fix: use what the engine did.** Count knocked_off only when the faller
  received an enemy push since it last stood: a weapon
  `Event::Impulse { source, target: Actor(faller) }` with an enemy source
  (session/weapons.rs:742 applies it via push_player). If the scorer can't
  see weapon events, record "last pushed by, tick" in the session where
  push_player is applied, and expose it read-only for tests.
- Then the claim is exact: self-caused falls stay 0, and a fall after a real
  enemy shove is knocked_off.

## 4. clear_cone: general, no new number, one geometry gap (small fix)
- **General.** It keys on `Shot::projectiles > 1` (data), not a weapon name.
  SPREAD_CLEAR existed before.
- **The test is real:** an ally at 0.8 of the spread was outside the old
  0.6 cone.
- **Gap.** `image_spread` is v20's per-axis turn ("up to 5π·spread about
  each axis"). A pellet turned the full amount about both cross axes lands
  √2·spread off the line, so "the whole cone" is still only the inscribed
  circle. An ally on a diagonal between 1 and 1.41 of the spread can still
  be grazed.
- **Fix:** `spread * std::f32::consts::SQRT_2` for several pellets, with a
  comment that it's the corner of the per-axis box. That is geometry, not a
  magic number. Extend the test with an ally at 1.2·spread on the diagonal.

## Not run on this stack
Broom soccer: contest worth now goes through `contest_body`/`shove_worth`.
The PC gate's two_against_two... run (with BRI_SOCCER_KIT=broom if it's
cheap) is the check that brooms still shove opponents off the ball.

## To pass
Push 1 (tuning), 3 (knocked_off from real impulses) and 4 (√2). I'll
re-review the new sha against this file only.

---
# Re-review: ec7db637 (f5b4d9e3, 7c79f816, 48a79fbd, ec7db637 on 107dd555)

**Verdict: PASS. Merge ec7db637e2d8d994a5304a844c070d8d3474df06.**

1. **Tuning, f5b4d9e3: fixed.** It flies `peer.player.tuning()` as stored,
   and the comment says why. The test flies the same shove with the stored
   tuning at scale 2 and gets the same landing. Good.
2. **knocked_off, 7c79f816: fixed, from engine truth.**
   - `Combat.pushed` is set where `WeaponEvent::Impulse` on an actor is
     applied, with its source. The scorer requires an enemy push at or after
     the tick the faller last stood. The near-miss guess is gone.
   - `Combat` is plain runtime state (Debug, Clone; not serialized), so no
     save or wire format changes.
   - Remaining by design: any enemy push counts, including a bullet's small
     impulse. A bot nudged by an enemy hit and then strafing off itself
     still reads as knocked off. Telling those apart needs a counterfactual;
     fine for a sanity absolute.
3. **clear_cone, 48a79fbd: fixed.** SQRT_2 is the diagonal of the per-axis
   box, and the test puts an ally at 0.85·spread about both axes (≈1.2·spread
   off the line). Superseded soon by the ally-harm build, which deletes it.
4. **ec7db637:** the v0.2.7 note for the tighter pusher bound. Good.
