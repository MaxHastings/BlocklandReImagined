# Prevent unplanned drops beside level bot routes

In the raised-deck regression, sampled bots carried velocity over a deck edge
while walking on level routes; two fell in four layouts. The motor's last
safety check treated every route-aligned step as a planned drop, even if the
next waypoint was level. This let ordinary route movement pass the less strict
hurt-only fall check. It was not necessary to disable stacking or impose a
universal no-drop rule.

The safety check now applies its existing stricter drop test to grounded level
routes. A waypoint that actually descends beyond a step retains the planned
non-harmful drop behavior, and existing flight/leap/climb exemptions remain.
The existing safety regression now separately asserts that an ally-avoidance
step and a level route are held at the edge, while an intentional descending
route may proceed. All four deck layouts then had zero falls (formerly two),
and all six `bot_hardening` cases passed.

Changed-area validation: six bot-action safety unit cases passed, 47 nav
cases passed (one existing measurement ignored), ten `bot_routes` cases
passed, and both real Beta16 tower CTF placements captured without jets,
including return down the tower. Formatting and changed-crate Clippy passed.
Saved-game headless comparisons used the same `content` root and five-brick
cube, 82-brick staircase and 17,518-brick balcony saves, with no set-aside
bricks. Sampled target-death ticks (control -> guard): cube 10921 -> 10921,
stairs 2401 -> 2401, balcony 3366 -> 3486. The 120-tick balcony difference is
about one second; these single deterministic replays do not establish general
route quality. An earlier attempt using the release gate content lacked enabled
zombie packages and set aside three cube bricks; it is invalid and excluded.

The release-profile bot budget, full extended gate and platform CI must be
rerun after this runtime change. No interactive playtest was performed.
