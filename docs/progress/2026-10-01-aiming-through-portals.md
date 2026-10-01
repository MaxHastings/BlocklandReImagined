# 2026-10-01 Aiming through portals (branch `claude/project-thread-9cbbpg`)

Max placed a ghost brick while looking through a portal and nothing showed
on the far side. Bodies, shots, the camera and the Gravity Gun already went
through openings, but every other aim stopped at them: the brick in hand,
the hammer, wrench, printer and wands, the empty-hand click, Add-On ray
casts (duplicators aim with them), and build reach (TooFarDistance).

One mechanism now serves them all:
- `Passages::cast` (bri-content) walks a sight leg by leg and asks the
  caller what it meets on each; `Passages::shortest` is the shortest
  straight way between two points, through one opening when nearer
  (`bridge` is now that).
- Host: `Simulation::target_through` (clicks), `tool_ray` (stock tools,
  each hit keeps the direction it arrived with for its puff and pushes),
  the Gravity Gun's `sight` and Add-On `raycast` all cast through `cast`;
  plant and copy reach are measured along `shortest`.
- Client: `Building::aim` places the ghost where it is seen; it faces the
  player's way as seen through the portal, and its shift and turn keys
  move it that way (`Building::ghost_facing`). `App` hands the building
  the openings each frame, as it does the effects.

Not changed: flipping a vehicle with a click stays straight on.

Tests: `bri-sim` portals `aiming_into_a_portal_reaches_what_stands_past_its_partner`,
`bri-client` building `a_brick_aimed_through_a_portal_lands_and_turns_as_it_is_seen`;
`cargo test -p bri-sim -p bri-content`, `cargo test -p bri-client --lib
--test world_items --test app_flow`, clippy `-D warnings` on the three crates.
