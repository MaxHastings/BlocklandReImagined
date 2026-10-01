# 2026-10-01 Bots through portals

Branch `claude/project-thread-iyl0uk`, on main d2ae52670 plus the Gravity
Gun lane's add81b33 (its `Passages::sight` is the one portal sight query).
Max: "we should make it so Blockhead bots know how follow and see player
through a portal so they can still go after them".

## What changed

- Sight: `Passages::ways(a, b, reach)` lists the straight ways from a bot's
  eye to a player: across, and in through each one opening whose sight
  (`Passages::sight`) comes out at the player. `Simulation::sight` checks
  each way's legs with the targeting ray, shortest first, and with no
  portals in the world it is the old single ray with no search or
  allocation. Bots use it for every enemy they look at.
- Aim: a bot aims at the player as seen through the opening (the way's
  `aim`, with lead from the player's velocity turned back through the
  carry). Shots already go through portals, so they land.
- Paths: the walk grid (`crate::nav`) treats each opening as a link. A step
  whose body middle goes in through an opening (the motor's rule) leads to
  the cell it comes out at by the partner, costing one step; the A*
  estimate goes via each opening too, and the search bound counts from
  where openings let out. A waypoint through an opening carries the point
  to walk toward on the near side; the bot pops it when it is carried.
- Following: chases head for where the enemy really stands and the path
  finds the way. An enemy that goes through an opening as it drops out of
  sight is remembered where it came out. A bot carried through turns its
  heading, keeps its path and carries its leash (home as reached through
  the openings it took), so chase and wander distances are walked ones.
- One crossings record (`session/crossings.rs`): every player, vehicle and
  entity trip through an opening, numbered, kept 120 ticks. Gravity Gun
  holds and bots each read it from their own last-seen number; the holds'
  private list is gone.
- `bri_sim::testing::PORTAL` / `portal()`: the made-up doorway portal moved
  out of `tests/portals.rs` so bot tests and portal tests share it.

## Evidence

- `crates/chaos/tests/bot_brain.rs`
  `a_bot_sees_its_enemy_through_a_portal_and_walks_through_after_them`
  (the bot leaves a closed room through the portal at once and reaches the
  builder) and `an_armed_bot_shoots_through_a_portal_from_where_it_stands`
  both fail on main d2ae52670 (the bot never leaves; no hit) and pass now.
- `nav::tests::a_portal_is_a_way_through_a_wall_with_no_way_round` and
  `passage::tests::a_point_beyond_the_partner_is_seen_through_the_opening`.
- Cost: with no portals the bot's sight and grid steps take the same rays
  as before (one early-out check each). With portals, each sight check adds
  a plane test per opening and rays only for ways that line up; grid
  expansions add one plane test per opening per step.

## Next

- Bots only follow one opening deep by sight (a way through two portals is
  found by paths, not by eyes).
