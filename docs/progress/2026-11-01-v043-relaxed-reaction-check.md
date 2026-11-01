# Relaxed bot reaction integration check

The old perception scenario assumed a hidden opponent was forgotten and the
bot was wandering exactly three seconds later. With last-known-area search,
the bot can still be investigating then. The test now waits up to 60 seconds
for an actual unseen wander before presenting the opponent again.

Comparing first wound times from two separate runs was also confounded by
movement, aim and projectile travel: an investigative run measured 285 ticks
with the relaxed reaction and 289 without it, despite the recorded relaxed
reaction having the expected 126–234 tick interval. The integration check now
requires that actual interval, verifies the authoritative fired tick does not
precede its reaction deadline, and requires the ordinary shot to hurt the
opponent. The strength-zero control still requires no reaction notice and a
real wound. The existing `a_goofing_bot_reacts_slower_and_errs_wider_than_one_in_combat`
unit test separately compares the scheduling policy for 49 matching seeds.

All five `bot_perception` integration cases passed in the playtest profile,
and changed-target Clippy and formatting passed. The first attempt to observe
firing through active projectile objects missed projectiles that collided in
the same simulation tick; the final check uses authoritative `fired_tick`.
No bot behavior or reaction timing was changed.
