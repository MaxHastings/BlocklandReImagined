# 2026-10-13 Capture the Flag bot behaviour

Max's report from "TDM Afghan" (red/blue Capture the Flag): bots ignore the
tank by their spawn and all take one opening path; flag carriers keep turning
to fight; nobody returns their own dropped flag; bots sidestep and back round
walls and stairs indoors; bots with jets fear high drops. Targets
v0.3.0-experimental (Max asked for it in v0.3.0).

## Causes found and changes

- **Carriers distracted.** Slayer CTF's `bot_objectives` offered no
  destination while the carrier's own flag was away ("Return Flag Without
  Own" off, the default), so a carrier had no objective at all and fought
  like anyone else. `return_way` now offers the carrier's home anyway; the
  return still happens only when the rules allow it (`on_zone` is
  unchanged). A carrier at a destination that has not taken the delivery
  waits there, stepping out and back in every 2 s so the zone sees it enter
  again, and its approach deadline holds meanwhile
  (`package_objectives::Action::Visit::inside_since`, `turn.rs`). Run and gun
  now shoots only enemies within about 50 degrees of the way
  (`GUN_BEHIND_COS` 0.64, was 105 degrees), so a carrier keeps running
  instead of turning side-on.
- **Own flag ignored.** Nothing offered it. `bot_objectives` now offers a
  team its own dropped flag (`recovery_described`, both Instant and Manual
  recovery), completed by a new per-player `recoveries_by` tally that
  `recover_flag` keeps. Package offers are sorted nearest first, so the bot
  beside its dropped flag takes that before the enemy's far off.
- **Sidestepping indoors.** On a feet-only objective the look was the
  objective's point, through the walls, while the walk followed the route;
  a carrier going down stairs walked sideways and backwards. It now looks
  along the route until within 4 units of the point. Any walker looking
  along its route also slows into a sharp bend (`turn_pace`) so it turns and
  walks forward.
- **Same opening path.** Every bot's A* found the same cheapest route.
  Objective searches are now seeded per bot life (`route::Costs::veer`):
  6-unit patches of the way look up to 60% slower to that search alone, so
  bots spread over comparable streets. Costs only ever rise, so the
  heuristic stays admissible; no per-tick cost.
- **Tank ignored.** Seats were never an opportunity while a bot had a
  grounded objective. Farther than 32 units from a feet-only step it has not
  picked up, a vehicle's controls are now an opportunity when the drive
  beats the walk (`Want::Objective`, `drive_serves`, armed or not); the
  driver pulls up and walks the last stretch. A known enemy still comes
  first, as before: the seat is only sought with none about.
- **Fear of drops.** A body whose jets lift it and hold 3 s of jetting gets a
  32-unit walk-grid drop (was 4 for everyone), and a new highest mover,
  `Brake`, holds the jets with no walk when `route::brake_fall` says a fall
  would otherwise land faster than the archetype's minimum impact speed.

## Evidence

- `cargo clippy -p bri-sim --all-targets -- -D warnings`
- New tests: `reach::tests::jets_held_late_break_a_fall_from_any_height`
  (the real motor dropped from 16 to 150 units lands under 30 u/s, and over
  it without jets), `nav::tests::a_body_with_a_deep_drop_walks_off_a_high_roof`,
  `nav::tests::seeded_searches_spread_over_equal_ways`, and
  `slayer::bots_are_offered_the_way_home_and_their_own_dropped_flag`
  (fixture CTF port, no original content).

- Chaos `bot_objectives::an_objective_resumes_after_an_ordinary_combat_interruption_and_disconnect`
  failed once the seeded routes moved the bot: the scripted attacker's
  misses then flew on 75 units into the passive author. The assertion now
  checks the stated intent (the bot never takes the author on) rather than
  the author's health. `bot_objectives`, `bot_interactions`, `bot_gauntlet`
  pass.

## Not verified here

The TDM Afghan save and v20 content are on Max's PC, so nothing ran on the
real map or tank. Whether the v20 tank's seat counts as a wheeled control
seat for bots, and how carried flags behave while seated (bots only board
before picking a flag up), need a playtest. Next: a headless match recording
on TDM Afghan with the match recorder.

## Considered and left out: blasting through walls, rocket jumps

Max asked whether bots could blast a shortcut through a wall (when brick
damage is on) or rocket jump onto a roof, only if either fits the existing
planner without a rework. Neither does yet:

- **Wall shortcut.** The route search (`nav::Search`) has no edge that needs
  an action first; walls are fixed colliders in the walk grid. Pricing a
  breakable wall as a way through needs a new edge kind carrying the brick,
  its volume and support (`events.rs::breakable_bricks`), the game's brick
  damage permission, and the weapon's `BrickImpact`, which bot capabilities
  (`tactics.rs::Capability`) do not carry today. The hole also refills when
  the brick respawns. The piece that would fit is narrower: the door seam
  (`extras.rs::bot_opens_way`) could treat a breakable brick across the way
  as openable when the route gives up short, but that only helps with no
  way round, not with a shorter one.
- **Rocket jump.** Only jumps and jets are measured legs (`reach.rs`). A
  blast at the feet pushes its shooter only where self damage is on (impulse
  and damage share `can_radius_damage`), so it would need a new `nav::Mode`,
  reach tables keyed by weapon as well as tuning, a leg controller and a
  health budget.

## CI follow-up

- **Delivered vehicle left behind.** The pull-up near a feet-only step
  (`bot_seated_input`, `arrived`) also fired when the step was the drive
  itself, so a bot delivering a vehicle got out beside it, could not board
  again ("Just dismounted"), gave the seat up and went for the passive
  creator. It now pulls up only when the objective's step is not the drive
  (`objectives::State::drive`). Found by
  `bot_physical_objectives::unfamiliar_ground_vehicle_uses_its_own_control_seat_and_actual_round_outcome`.
- **Broom brawls away from the ball.** A fight begun at the ball scored
  `0.8 * ARMED` (0.6) once the ball rolled off, and the play's 0.65 never
  beat that by the held choice's 10% margin, so four bots shoved each other
  harmlessly for a minute (`bot_soccer_match`, Broom seed 1; the base does
  the same for shorter spells: combat about 400 of 600 bot-s). An enemy
  that can only shove is now worth `SHOVES` (0.7) away from the play, below
  the margin; one with a damaging attack keeps `ARMED` (lowering that
  instead stopped bots ever seeing an enemy through glass in
  `acceptance_unfamiliar`).
- `bri-net/soak` (`two_players_stay_consistent_through_a_lossy_jittery_link`)
  ran 1097 of 1200 ticks on one Windows runner: a wall-clock pace check with
  no bots in it, not this change.
- **Jet drop cost on stacked builds.** The 32-unit jet drop made each
  floor sample list every storey down to 32 units below (up to
  `MAX_LAYERS`), about 1.3 ms a tick on Collage with 32 bots per the lag
  thread's measurement (`2026-10-13-collage-bot-cpu.md`). Past the walked
  drop (`nav::SHALLOW_DROP`, 4 units) a body only falls and lands on the
  first floor below, so a sample now stops after that one
  (`nav::tests::a_deep_drop_samples_only_the_first_storey_below`: 3 floors
  where it listed 8). Bodies with a 4-unit drop sample as before. Not yet
  re-measured on Collage (no generated content in this container).
