# Bots crawl through low gaps

Bot redesign, step 3: a richer walk map.

## What changed

- `nav::Body` has `crouch_height`. A cell with no room to stand but room
  to crouch, walked in to (never jumped up to), is a crawlspace floor
  (`nav::crawl`), costing 1.5 more per cell than walking.
- `Waypoint::crouch` marks those cells; `simplify` keeps them. A bot
  presses crouch on the way into one; the motor keeps it down until it
  has room to stand.
- `sample` no longer gives up when its downward ray runs out; it falls
  back to the crawl check.
- `a_low_ceiling_blocks_and_changes_are_forgotten_locally` now uses a
  slab at knee height (0.8), since a 1.6 slab is now crawled under.

## Evidence

- `nav` unit `a_crawlspace_is_crawled_unless_a_door_is_near`: crawls
  under a 1.6 slab with crouch waypoints, walks a nearby upright gap.
- chaos `a_bot_crawls_through_a_low_gap`: a long see-through wall with
  a 1.6-high gap; the bot crosses under it at the gap and reaches the
  builder. With the crouch input disabled it fails ("the bot got past
  the wall").
- chaos bot_brain 16, sim lib 70, sim showcase 30, vehicles pass;
  clippy clean.
