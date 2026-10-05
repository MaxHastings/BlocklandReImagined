# 2026-10-05 Bot route planner: jet, swim and drive legs

Branch `fix/bot-route-planner`. The design is the "Routes" section of
[../architecture/bots.md](../architecture/bots.md), with a row in
[../architecture/seams.md](../architecture/seams.md).

## What changed and why

The bots had a separate behaviour or shortcut for each way of getting
about. Flying was a behaviour that hovered under whatever it could not get
round, and in Max's Slayer soccer with empty loadouts every bot flew
upward. Walkers floated in deep water with no route. Drivers chased
targets inside their turning circles round and round, and boarded any
vehicle in sight, so 8 bots piled onto a tank mid-soccer. Now one search
plans every way a bot can get about, and each leg runs on ordinary
controls:

- **Jet legs.** The walk grid gains jet edges (`nav::Search::jet_edge`):
  - a takeoff only where the sky above is clear and no body stands over
    it;
  - a climb, a crossing at the apex and a descent, each swept against
    fixed collision;
  - a cost from the body's own jet thrust, gravity and energy
    (`route::Jets`).

  `route::JetLeg` and `route::jet` fly the leg. It is given up, and the
  bot plans again, when the climb stalls, the leg runs out of time, or the
  bot lands short. The **Fly behaviour is removed**, and the kind's `fly`
  weight is now a leg weight (`bot_kind::LEG_WEIGHTS`). A ranged weapon
  Fights an enemy above it that its band reaches (`Situation.reach_up`).
- **Swim legs.** Deep water is swum at the surface, no longer walked on
  the bottom (`Mode::Swim`, `route::Swim`). A floating bot plans from
  there, and banks higher than the water are climbed out of with jump.
  `moves: swim` kinds keep their own water roaming.
- **Drive legs.**
  - `route::Chassis` gives a turning radius from the wheelbase and the
    lock.
  - `route::gear` handles a point inside the circle: the driver backs out
    or pulls out first. A point behind is backed onto when that is sooner
    than turning round.
  - `route::pace` sets speed: no faster than the tyres hold the
    pure-pursuit arc, and manoeuvring speed while backing out.
  - Waypoints and search probes count as reached within half the
    chassis's footprint.
- **Board legs serve the goal.**
  - A seat is an opportunity only when walking to it, boarding and driving
    gets there sooner than walking (`route::drive_serves`), or when the
    vehicle is armed.
  - A bot with a grounded objective of its own takes no seats at all.
  - Once seated, the seat claim measures the drive's progress afresh
    (`claims::leg`).
- **Leave legs.** A chassis that cannot hurt the enemy it chases (no gun,
  and no runover for someone not on foot) stops and gets out about where
  the bot fights from on foot. A wreck, or a wheeled hull on its side or
  roof, ends the drive at once (`route::upright`). A bot on a vehicle roof
  plans from the floor beneath.
- **Out of reach.** When the best route to an enemy ends where the bot
  cannot hurt them, chasing them, or searching where they stand, is worth
  nothing until they move.

## Guards

All are in `crates/chaos/tests/bot_routes.rs`. The first five, plus the
roof test, fail on the old code (checked by building them against the old
`crates/sim`):

| Test | Old code |
|---|---|
| `a_bot_jets_from_open_sky_onto_a_high_platform_instead_of_hovering_under_it` | 2330 ticks hovering under the platform |
| `a_walker_swims_across_deep_water_to_reach_its_enemy` | floats at (-12.2, 4.1, 40.5) with no route |
| `unarmed_bots_with_only_a_ball_to_play_never_take_off` | the Fly behaviour jets at tick 325 |
| `a_vehicle_that_does_not_serve_the_ball_game_attracts_no_bot` | a bot goes for the jeep at tick 9 |
| `two_drivers_who_cannot_hurt_each_other_get_out_and_fight` | the gauntlet's jeep duel: nobody hit in 40 s |
| `a_melee_bot_gives_up_an_enemy_it_cannot_reach_on_a_roof` | chases for the whole 20 s |
| `a_driver_searches_round_a_wall_from_an_allys_sighting` | property test: the ally's sighting leads the driver round the wall until it sees the spot |
| `a_driver_runs_down_an_enemy_who_sidesteps_inside_its_turning_circle` | passes on both (smoke) |
| `a_jeep_drives_straight_to_a_point_ahead_and_stops_there` | passes on both (smoke) |

Unit tests in `route` cover:

- the chassis radius, gears and pace;
- jet climb and crossing, and flights the energy cannot afford;
- swim cost and the jet-leg controls;
- `drive_serves` and `upright`.

They also cover `claims::leg` and the scaled reach in `search_memory`.

Changed tests:

- `bot_tactics`: the close-flyer test became
  `a_close_ranged_bot_backs_out_from_under_a_low_roof_and_delivers_a_safe_blast`.
  The bot no longer flies under the roof; it backs out to its band and
  delivers a safe blast.
- `bot_knowledge`: in `removing_a_rules_bot_releases_its_reservation_before_the_lease_expires`,
  the target now stands 36 away, far enough that the cart serves the
  chase. Its assertions are unchanged.

## Evidence

The environment for every command was `CARGO_TARGET_DIR=/home/claude/bri-target`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`.

- `cargo test -p bri-sim --lib`: 209 passed.
- `cargo clippy -p bri-sim -p bri-chaos --all-targets -- -D warnings`:
  clean.
- Every `crates/chaos/tests/bot_*` target passes, before and after:

| Target | Base | Branch |
|---|---|---|
| `bot_brain` | 18 | 18 |
| `bot_knowledge` | 8 | 8 |
| `bot_navigation_spike` | 8 | 8 |
| `bot_physical_objectives` | 14 | 14 |
| `bot_soccer_teams` | 3 | 3 |
| `bot_tactics` | 13 | 13 |
| `bot_interactions` | 17 | 17 |
| `bot_carryable_objectives` | 7 | 7 |
| `bot_creator_acceptance` | 8 | 8 |
| `bot_creator_adversarial` | 5 | 5 |
| `bot_creator_heldout` | 1 | 1 |
| `bot_objective_rest` | 2 | 2 |
| `bot_objectives` | 11 | 11 |
| `bot_physics_interactions` | 11 | 11 |
| `bot_search_objectives` | 4 | 4 |
| `bot_routes` | (new) | 9 |

The ignored perf, firefight and CTF targets need generated content
(`BRI_CONTENT`), which this machine does not have.

## Not done / next

- **Gauntlet numbers.** The gauntlet (`bot_gauntlet`, `a_jeep_on_each_side`)
  is on the newer integration branch, which this branch could not merge
  from here. The root cause of the jeep duel was found by reproducing it
  in `two_drivers_who_cannot_hurt_each_other_get_out_and_fight`. Run the
  gauntlet after the merge.
- **Merge conflicts to expect** in `bots.rs`, `behaviour.rs` and
  `interactions.rs`:
  - The newer base's `reverses()` and `mounted.reverse_*` tunables
    should fold into `route::gear`, keeping `reverse_distance` as its
    pursuit limit, so one reversing policy remains.
  - The ball-games lane's `fly_rise`, `fly_drop` and `fly_give_up_seconds`
    go away with the Fly behaviour.
  - The team lane's ally-in-harm-volume helper should replace
    `bot_ally_corridor` in the drive leg's stopping path.
- **Not built yet:**
  - board/drive legs for goals other than an enemy (V1);
  - k-best alternative routes for the chooser;
  - one progress monitor per leg in place of the separate stuck timers;
  - a session-level guard for a wrecked or flipped vehicle (no test hook
    wrecks or flips one).
