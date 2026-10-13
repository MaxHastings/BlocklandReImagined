# 2026-10-12 Bots: a lone crew takes the seat its plan needs

Max asked for a tank's first bot to stop being stuck as driver: switch to
the gun to blow up a distant enemy, back to the controls to move, stay
driving to run someone over nearby, and leave a seat for a buddy coming.

## What changed

- `Session::bot_lone_role`: in a vehicle whose controls and gun are
  separate seats, the other role is free when nobody holds it, no ally's
  claim is on it, and (for the gun) the gun can fire.
- In the seat (`bot_seated_input`): Fight, with the gun serving, takes the
  gun; Chase, Search, Return, Objective and Explore take the controls; any
  other behaviour stays put. The switch uses `switch_seat`, the player's
  seat keys.
- The gun does not serve against an enemy on foot that the chassis runs over
  from nearer than the gun's near edge. That edge is the band's, which comes
  from the shell's blast (`bot_runs_over_near`).
- Alone at the controls, the free gun is the bot's weapon for scoring a
  fight and for whether it can hit back (`bot_spare_gun`). Without that, the
  fight that sends it to the gun would never be chosen.
- `vehicle_gun` is factored out of `bot_vehicle_weapon`, so both read one
  gun.
- Scoring: a seat change inside a vehicle is the same ride, and the
  gauntlet's RIDES line counts `switches`.

There are no new constants or timers. The hold rule on behaviours keeps a
seat from flipping.

## Tests

`a_lone_driver_does_not_wait_forever_for_an_absent_gunner` asserted the old
play: a lone bot drove off with its target in the gun's reach. It is now
`a_lone_crew_takes_the_gun_for_an_enemy_in_reach_and_the_controls_for_one_beyond_it`.
New: `a_lone_crew_at_the_gun_takes_the_controls_to_run_down_an_enemy_too_near_for_its_blast`.

`a_driver_brakes_for_a_grounded_ally_before_forward_or_reverse_contact` is
about braking, and its armed rover's target was in gun reach, so it now
uses the gunless pursuer.

## Evidence (team deathmatch with a jeep and a tank a side, against main f4ce0ae)

The jeep gauntlet's runs are identical, since the jeep has no gun seat.

| | main | lone-crew seats |
|---|---|---|
| kills, 1x, seeds 0 to 17 | 192 | 178 |
| kills, 3x, seeds 0 to 5 | 319 | 258 |
| time at a tank gun, 3x | 1676 s | 2020 s |
| `no_driver` exits | 8 | 0 |
| seeds passing, 1x | 15 of 18 | 17 of 18 |

Failures on both are the known `bad_plans` race and the fight band.

Kills by the killer's seat, seeds 0 and 1 at 3x:

| killer's seat | main | lone-crew seats |
|---|---|---|
| on foot | 55 | 37 |
| driver | 26 | 15 |
| passenger | 42 | 19 |
| tank gun (or a jeep's back seat) | 21 | 26 |

Kills from the tank gun rose. The fall is in kills by everyone else. Matches
diverge early, and those totals swing widely between seeds. Max judged
total kills a weak measure of play. The behaviour itself is what he asked
for, and his playtest is the check on it.

## Next

Max's playtest of a tank team match:

- Does a lone tank bot shell from a standstill too long?
- Does it drive on when the enemy closes in?
