# 2026-10-08 HE Grenade wind-up loop (v0.2.7.1)

Max: bots holding the HE Grenade "wind up and rewind over and over" and
rarely throw.

## Cause
The HE Grenade is release-only: Charge (84 ticks) leads to Armed, and only
letting go throws (`charged_control::release_only`). Two places cancelled
a held wind-up whenever the target dropped out of view for a moment (a
corner, a pillar, a body stepping behind cover), and each cancel remounted
the image to Activate, so the next sighting started the pin and the charge
over:

- the post-move fire gate (`bot_hand_fire_gate`) judged a planned tick with
  no live intent as "no plan" and aborted the held button, even though
  holding a release-only charge fires nothing;
- `bot_act` kept the native chooser's wind-up only while it had a live
  choice (`native_choice.is_some()`), not under the hold rule the scripted
  path already used.

## Fix
One rule, one owner. `bot_act` now computes `attack_stands` once (enemy in
sight or remembered, within the kind's hold since it was last on target)
and both hand paths keep the wind-up by it. The gate takes `releasing` and
lets a planned tick without intent hold a release-only wind-up; the release
itself is still planned and judged, and unplanned presses keep their harm
check. No new layer: the competing cancel paths now ask the same question.

## Evidence
`bot_watch`, Close Quarters, 4 bots, all HE Grenade, 40 s:

| | Charge | Armed | Fire | Activate (cancel remounts) | kills | self kills |
|---|---|---|---|---|---|---|
| before | 155 | 55 | 49 | 139 | 7 | 0 |
| gate fix only | 158 | 71 | 60 | 124 | | |
| both | 72 | 51 | 39 | 57 | 8 | 0 |

One bot wound up 35 times for 3 throws before. After, wind-ups end in 33
throws, 18 cancels in search once the hold ran out (by design), 7 in a
fight and 5 deaths; held Armed median 90 ticks, p90 255, max 502 (no hold
forever).

Tests:
- `bot_tactics::a_target_out_of_sight_a_moment_keeps_the_windup_until_a_throw`
  drops the target behind the bot past its sight for 20 ticks mid-charge:
  without the fix the gate aborts on the first tick ("no plan") and the
  image goes back to Activate; with it the wind-up is held and thrown
  (Fire at tick ~500, a projectile from the bot).
- `bot_windup::a_bot_throws_the_imported_he_grenades_it_winds_up`
  (ignored, needs `BRI_CONTENT`): the imported HE Grenade in the duel
  harness throws every wind-up begun in a fight. The duel's `Fire` check
  now also counts a projectile the bot fired, since a thrown grenade is
  used up and its Fire state is never seen.
- `bot_watch` logs the bot's hand image state per tick (`img`).

One harness expectation widened: the duel counted a strike only when the
Fire state was seen; now a projectile the bot fired also counts (a used-up
grenade has no visible Fire state). No other test changed.

## Next
Ships as v0.2.7.1 together with the Slayer 32-bot limit (93ee6c2), one PC
gate for both.
