# 2026-10-12 Bots: the hull as an affordance (v0.2.10.1)

Left over from v0.2.10 (`2026-10-11-bots-vehicles-and-calls.md`): a bot
that fights with its body could not do anything useful against an enemy in
a seat. It pressed against the hull and bit air, or, against a rider sitting
high, gave the enemy up as out of reach and wandered. A chassis an enemy
drove at a bot was also not something it got out of the way of.

## What changed

- **The hull is the attacked thing.** A seated enemy is reached at the hull
  it sits in when the bite's damage type wears hulls (`vehicle_scale` above
  0; no type counts as 1, as `damage_vehicle` already does). The bot goes to
  the hull's nearest point, looks at it, and measures its reach to it. A
  bite that reaches the rider still lands on the rider; one that does not
  lands on the hull through `damage_vehicle`, scaled like any weapon's hit.
  `Session::bite_hull` is the one place that answers "where does this bite
  meet that enemy". A bite type with `vehicle_scale` 0 leaves everything
  as before.
- **An enemy's chassis is a blow coming.** `bot_incoming` now weighs a
  chassis that runs people over, driven by an enemy that can hurt the bot,
  by the same prediction as a projectile: its path over `LOOKAHEAD`, the
  body radius plus half the hull's width, and harm equal to `runover_damage`
  over health. It is judged again every `ASK_AGAIN`, and the only way out is
  a step aside. A hop or a jet comes down in its way: with hops allowed,
  own-driver run-overs in the jeep gauntlet went from 6 to 13.

Nothing names a vehicle, and no new timer or weight was added.

## Evidence

All runs compare against main f4ce0ae on the same seeds.

Jeep gauntlet, seeds 0 to 15, 3x rounds:

| | stuck mean (worst) | kills | team kills | own falls |
|---|---|---|---|---|
| main | 2.6% (5.8%) | 700 | 26 | 0 |
| this change | 1.7% (4.5%) | 680 | 28 | 0 |

Every team kill in both was an own driver running over an own walker. Over
seeds 0 to 7 this change had 15 against main's 6, and over seeds 8 to 15 it
had 13 against 20, so the totals are noise.

Hill gauntlet, seeds 0 to 5: all pass on both. Stuck averages 1.2% against
2.8%, pressing 15.9% against 16.6%. An earlier version, before biters went for
the hull, failed seed 4 on the fight band (fight dormant at 2.1%). Biters now
fight instead of idling beside a jeep.

Team deathmatch, seeds 0 to 3, 3x: all four pass, where main failed seed 1 on
the known `bad_plans` race. Kills are 198 against 243. A 12-seed 1x run with
the chassis dodge on and off had 141 and 131 kills, team kills 5 and 4, and
12 and 11 of 12 passing. The 3x drop is variance between runs.

New test: `bot_interactions::a_bot_that_bites_wears_down_the_hull_its_seated_enemy_sits_out_of_reach_in`.
A biter faces a hull of 30 health whose rider sits 6 units up. The rider is
never bitten, and the hull is wrecked. On main the biter wanders.

Commands:

- `cargo clippy --release -p bri-sim -p bri-chaos --all-targets -- -D warnings`
- `python tools/check.py focused`
- scratch seed sweeps of `a_jeep_on_each_side`, `a_jeep_on_a_hill` and
  `vehicles_are_driven_and_crewed_in_a_team_deathmatch`, not committed.

## Remaining constants

- `0.9 * scale` body radius: the one the projectile dodge uses.
- Half the hull's width: a physical size.
- `LOOKAHEAD` 0.75 s, `ASK_AGAIN` 2 s, `INCOMING`: the existing dodge
  reflex's.
- The bite's own `reach` to the hull: the kind's.

## Next

Max's playtest: a brawler against a seated player, and a jeep driven at bots.
