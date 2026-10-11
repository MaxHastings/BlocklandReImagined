# 2026-10-11 Bots in vehicles, and two calls

Max's v0.2.10 item 2: bots fight well but barely use vehicles. In Afghan DM
they got into a jeep, barely moved and got out. He asked for bots that
drive about, explore and run people over, and that leave only when wrecked,
stuck or under fire; for two bots crewing a tank more often; and for two
calls a bot can decline: "get out of the way" (an ally's blast would hit
it) and "get in" (a driver invites allies). He also asked for a long-term
design rather than tuning to pass a test. The design note is
`/mnt/project-files/bots/vehicles-and-calls-design.md` in the project
folder.

## Decisions

- No authored vehicle profile. A seat stays an opportunity scored by
  `route::drive_serves`, and nothing in bot code names a vehicle.
- A seat is a body the bot plays from, not a leg it takes. An untethered
  bot has no leash in a seat either. A driver with no fight known explores
  from the seat, using its chassis as the body, and boards a seat to
  explore (`Want::Explore`) as well as to fight or play.
- What was deleted rather than tuned:
  - the crew wait's time window: the driver now waits while an ally's
    seat claim lives, so the claim's lease bounds the wait;
  - the "offer only while parked" rule: the offer lasts while a seat is
    free;
  - the passenger's timed give-up: it gets out when no driver is in the
    seat or coming to it;
  - the drive leash for untethered bots.
- Every dismount names its reason (`interactions::Leave`):
  - `wrecked` and `bail_out` (the next hit like the last would wreck the
    hull);
  - `stalled`, `walk_the_rest` and `got_off_to_fight`;
  - `no_driver`, `other_way` (a passenger whose driver turned away) and
    `gun_lost`;
  - `nothing_to_do`.

  `Session::bot_seat_leaves` counts the reasons, and the gauntlet reports
  them.
- Calls are the visible half of intents allies already read, never orders:
  - "Get in!" (`offer`) is said as a driver starts offering seats. An
    offered seat is seen as far as the bot sees and boarded while the
    vehicle moves.
  - "Move!" (`blocked`) is said while a bot holds fire for an ally in its
    shot. Allies judge an ally's planned harm with the same dodge reflex
    they use for a shot coming: keep or dodge, chosen by surprise. They
    step aside out of a blast, else off the line, and are asked again
    every 2 s.
  - A driver whose chassis runs people over publishes what it sweeps
    before it can stop as its harm, the same as a shot's.
- Two time bounds stay, and neither is a behaviour timer:
  - the 15 s claim lease, which is how a claim nobody works lapses;
  - the 30 s callout cadence, which is chat volume.

## The stop measurement fix

`reach::Handling` measured a chassis's stop by braking from top speed in
one run, which left out the tyres' brake build-up. Jeeps chasing a pushed
bot drove off the map edge: the 3x jeep gauntlet had 2 falls. Each speed is
now reached from rest and braked fresh.

| Speed (units/s) | Rolled in game | Old measure | New measure |
|---|---|---|---|
| 12 | 12.1 | 2.3 | 11.1 |
| 21 | 19.4 | 8.0 | 18.8 |
| 27 | 24.2 | 16.4 | 24.4 |

Driving changes that rely on the honest stop:

- A driver brakes where a ray finds no floor from its leading end, one
  stop plus half its width ahead (a slanting edge meets a corner first),
  and carries the stop through to rest.
- Arrival braking applies at the route's end, not at every lookahead
  point, and never for an enemy it runs over.
- A stalled driver backs out only when the way behind is clear of allies,
  things and drops.

## Scoring changes (the metric followed the game)

Max's rule for this lane: judge by whether play is better, and fix a metric
that disagrees with good play. The gauntlet scorer changed in two places.

- A fall is attributed to whoever last pushed the walker, if that was
  after it last stood on something or within 2 s of it while it was still
  on the floor. A jeep's contact registers once, as it begins, and the
  chassis then presses the walker along. Three columns result:
  - `fell`: the bot's own doing;
  - `knocked_off`: an enemy's shove or run-over, which is good play;
  - `knocked_off_by_ally`: an own-side kill the rules do not count.

  A run-over now records the driver as the pusher (`pushed_by`), the same
  way a weapon's impulse does.
- A walker pressed against the vehicle its enemy sits in is `pressing`
  (pushing in to hit them through the hull, as a player does), not
  `stuck`. Before this, 91% of the jeep gauntlet's extra stuck time was
  this. Two attempts to stop walkers short of the hull made play worse,
  so I reverted both:
  - out of reach: idle went past 50%;
  - beside the hull: the fight band went dormant and gate kills fell from
    47 to 15.

## Evidence

Every gauntlet number below is a 3x run (`BRI_GAUNTLET_ROUNDS=3`), on the
same seeds for main (cf9c761) and the branch, with the scoring above.

The jeep gauntlet over seeds 0 to 7:

| | stuck mean (worst) | own falls | knocked off by an enemy | team kills | kills |
|---|---|---|---|---|---|
| main | 2.0% (5.8%) | 0 | 0 | 24 | 380 |
| branch | 2.2% (4.5%) | 0 | 3 | 25 | 338 |

The team deathmatch (a jeep and a tank a side, friendly fire on) over
seeds 0 to 3. This is the headline for "gets in, does nothing, gets out":

| | rides | rides under 5 s | median ride (mean of seeds) | crewed | gunner at a gun | kills per bot-minute | team kills | own falls |
|---|---|---|---|---|---|---|---|---|
| main | 163 | 31 | 13.4 s | 544 s | 366 s | 2.74 | 5 | 3 |
| branch | 132 | 11 | 24.4 s | 1426 s | 1219 s | 3.12 | 2 | 3 |

The branch's exits across those seeds, by reason:

- stalled 39, walk_the_rest 34, wrecked 15;
- bail_out 10, gun_lost 4, no_driver 4;
- nothing_to_do 1.

Main reports no reasons. Seeds 0 to 2 run about 10% fewer kills per
minute on the branch, and seed 3 runs 78% more.

"Move!" on against off, measured by `heeding_allies_harm_against_ignoring_it`
over the same team deathmatch, seeds 0 to 3:

| | team kills | team damage | enemy kills |
|---|---|---|---|
| heeding | 2 | 585 | 171 |
| deaf | 4 | 531 | 194 |

Own-side kills do not move measurably: 2 against 4 over four matches is
within noise, and team damage is level. I did not tune toward a number.

The stop fix together with the calls is what keeps the gate's jeep run
under the bound. Without the calls, the parked chassis publishes no sweep,
allies stand in a driver's way, and the gate seed's stuck share rose
(5.8% under the old scoring). So the work ships as one PR with separate
commits.

## Maps with no vehicles

Three things change there:

- Bots step out of an ally's planned shot or blast, judged by the dodge
  reflex, and a bot holding fire for an ally says "Move!".
- An objective reservation left when the objective ended is the
  opportunity's to judge, where before it was released at once.
- The scorer changes above.

The driving, stopping and seat changes apply only in a seat. Over seeds
0 to 3, 3x, branch against main:

| scenario | kills or captures | stuck |
|---|---|---|
| deathmatch_open_field | 458 vs 462 | 0% vs 0% |
| deathmatch_mixed_arsenal | 433 vs 424 | 0% vs 0% |
| rooftop_brawl_without_rails | 559 vs 552 | 0% vs 0% |
| stairs_to_a_deck | 235 vs 235 (identical runs) | 0.8% vs 0.8% |
| water_between_the_sides | 257 vs 257 (identical runs) | 1.0% vs 1.0% |
| capture_the_flag (captures) | 75 vs 64 | 0% vs 0% |

Team kills were 0 everywhere on both. One of main's rooftop runs (seed 0)
failed its own check ("no bot strafed off the deck"), and none of the
branch's did.

## Commands

```sh
cargo test --release -p bri-sim -p bri-chaos --lib --test bot_gauntlet ... (every bot_* and vehicle_* test binary)
BRI_GAUNTLET_ROUNDS=3 cargo test --release -p bri-chaos --test bot_gauntlet -- a_jeep_on_each_side vehicles_are_driven_and_crewed_in_a_team_deathmatch --nocapture
BRI_GAUNTLET_ROUNDS=3 cargo test --release -p bri-chaos --test bot_gauntlet heeding_allies -- --ignored --nocapture
BRI_STRESS_TEAMS=2 BRI_STRESS_VEHICLE=jeepvehicle=v20.vehicle.tankvehicle (Afghan DM tank team match, on Max's PC)
```

## Next (v0.2.11)

- A walker whose enemy sits in a vehicle presses against the hull to get
  at them. Melee through a hull should be a real affordance: hit the
  hull's own health, or treat the chassis as the threat and get clear.
- A rare `bad_plans` hit on a tank gunner's planned shot (own 6.36, net
  None). It looks like a scorer race: health drops after planning and
  `why.rs` nets with current health. It is not caused by this work.
- The Afghan DM tank team match on Max's PC (the stress knobs above). It
  needs the content packs, which are not in the cloud checkout.
