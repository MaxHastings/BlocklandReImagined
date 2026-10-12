# 2026-10-11 Bots stepping round an ally no longer walk off a deck

v0.2.10 bot hardening pass, slice 1 (navigation safety), branch
`claude/v0210-bot-hardening-kgf5ku`.

## How it was found

The bot gauntlet passes at its default one round. Played three times as long
(`BRI_GAUNTLET_ROUNDS=3 cargo test -p bri-chaos --test bot_gauntlet`), every
scenario passed except `rooftop_brawl_without_rails`: one bot went off the
deck by its own doing (`fell 1`, the test says "no bot strafed off the deck").
Tracing that bot (`BRI_GAUNTLET_TRACE=rooftop`, with the act readout) showed
it was not strafing. It was walking its route, `walk route`, and an allied bot
stood on the route a step ahead. `bot_walk_direction` bends the walk round a
body in the way, and it passes on the side that is toward the deck's edge as
readily as the other.

## Cause

The bent walk was the route mover's proposal, so `act::resolve` recorded it as
`routed`. The edge check (`bot_fall_ahead`) is more lenient on the route: a
route plans its own drops, so on it a drop is refused only when it would hurt
(`bot_safe_drop`, 22.5 units for the stock body). A 6-unit deck is no hurt, so
the sidestep walked off it, and the bot spent the next few seconds back on the
ground floor.

## Change

- `act::Proposal` gets `plan`: where the route alone would walk. The route's
  proposal sets it to the direction to its next waypoint before any bend
  round a body (`bots.rs`).
- `resolve` records `plan` (else the walk) as `routed`.
- `bot_safe_walk` judges "off the route" by direction (`off_the_route`), not
  by exact equality, so a walk slowed to follow an ally through a gap is still
  the route's own, and a bend round a body is not.

A step round a body is now judged like a strafe or a goof's walk: it keeps to
floor it can step back up from.

## Tests

- `act::tests::a_step_round_an_ally_does_not_go_over_the_edge` (unit): a bent
  route walk at a deck's edge is held, the route's own walk over the same
  edge is not.
- `bot_hardening::allies_stepping_round_each_other_stay_on_a_deck`: four
  allied brawlers in a file along a 6-unit deck's edge fight one enemy for 40
  seconds, in four layouts. Before the fix each layout lost a bot (1, 1, 1 and
  2 falls); after, none.
- `bot_gauntlet rooftop_brawl_without_rails` at `BRI_GAUNTLET_ROUNDS=3` now
  passes (before: `fell 1`).

## Not fixed

A layout with a bot starting on an ally's head still loses one (spacing 2.0,
z 55.5 in the same scenario): a bot that steps off a head carries its
horizontal speed into the air and lands past the edge. That needs the
step-off to account for momentum near an edge, a separate change.
