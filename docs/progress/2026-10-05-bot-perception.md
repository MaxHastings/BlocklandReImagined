# 2026-10-05 Bots notice things: glances, reactions, rough hurt, human turns, one sight budget

Lane `fix/bot-perception`, approved by Max: a small, general "what a bot
notices" layer. Base `claude/project-thread-pt64ji`.

## What changed

- `crates/sim/src/session/bots/perception.rs` (new):
  - **Glances.** An idle bot (Wander or Return; no enemy in sight, no
    objective at hand, nothing held, not seated or driving) turns its
    ordinary aim for about `glance_seconds` at the most salient of: a
    projectile blast (10 units of reach per unit of blast radius), a weapon
    sound (12 units at full volume), a 1.5 s stare from someone within 8
    degrees and in plain view, a body faster than twice the bot's own
    running speed (both within 0.3 of the kind's `sight`), and, at a low
    chance, anyone of any team close by in plain view. `salience` scales
    every reach. Salience is 1 at the source falling to 0 at its reach and
    is the chance of a glance; `cooldown_seconds` follows. The look-round
    runs on the bot's own `cadence` beat. The walk goes on; only the look
    turns.
  - **Reaction.** A newly seen target, or an attacker it was not fighting,
    gets a delay of `reaction_seconds` (fighting or hunting), times
    `relaxed_scale` (strolling, interacting), times `away_scale` outside its
    `view_degrees` cone, +-30%, from its own seeded RNG. The same scale
    multiplies the starting aim error, which narrows over the existing two
    seconds of tracking. Outside the cone it also turns `away_scale` times
    slower toward the target until it has reacted. A spawn-protected target
    is watched but not reacted to; the clock starts when it can be hurt.
    Damage still interrupts at once through the existing threat path; only
    the return fire waits.
  - **Hurt from out of sight.** A hit from someone the bot cannot see
    stores the incoming direction to within 25 degrees and the distance to
    within 40%, never nearer than a unit to the truth (`perception::guess`,
    seeded). Its look holds until its reaction, then turns. Its warning to
    allies carries the guess. Seeing the attacker gives the exact spot, and
    a hit from someone in sight is placed exactly.
  - **Warnings.** `hear_alerts` no longer writes memory directly: each ally
    acts on the warning a seeded 0.25-1 s later (per ally and warning via
    `cadence::spread`, scaled like a reaction) at a spot up to 1.5 units
    off.
  - **Turning.** `State::turn` replaces the linear turn (not the carry
    swing): it accelerates into a big turn (top speed 1.3 x `turn_degrees`,
    acceleration 1.8 x rate squared, so a half turn takes about the plain
    time), eases out, overshoots a fast flick by a few degrees and settles.
    A strolling bot's look drifts up to 7 degrees (two slow swings on its
    own phase). Pitch stays linear.
  - `delay_ticks` and `Brain::switch_delay` expose the same delay for the
    surprise chooser's tell.
- `crates/sim/src/session/bots/sightlines.rs` (new): one sight-ray budget
  per tick. `Session::bot_sees(viewer, subject, from, to, reach, urgency)`
  and `Session::bot_sees_player(viewer, owner, from, reach, urgency)`.
  Ordinary queries share 128 rays; each bot has 4 of its own (sized for 32
  bots, 256 in all) for its current target or attacker, so those always
  run. Ordinary answers are cached per (viewer, subject) for 6 ticks while
  neither end moves half a unit; with the ordinary share spent, the last
  answer stands in. A player is seen at the eye or else at the chest, and
  vehicles occlude except the viewer's and the subject's own mounts.
  Routed through it here: combat sight (`bot_sight`; threat and current
  target as `Target`, the candidate scan as `Ordinary`), the perception
  watcher, nearby-player and fast-motion polls, and arming's item checks.
- `crates/sim/src/session/bots/cadence.rs`: the bots lane's helper from
  `fix/bots-ball-games-2` (37c290a7), copied as is; perception uses its own
  salts (101-104).
- `BotKind::perception` (`bot_kind.rs`), validated, on by default for
  every kind (Max: no "off" stage). Seven knobs: `salience`,
  `glance_seconds`, `cooldown_seconds`, `alertness`, `relaxed_scale`,
  `away_scale`, `view_degrees`. `alertness` (0 to 1) is the one dial that
  scales reaction delay, aim error, view-cone delay and turn cap, warning
  delay, turn overshoot and idle drift together. Everything else is a
  commented constant in `perception.rs` or engine data.
- Weapons runtime: a `Blast { source, position, radius }` event from
  `explode`. The sim feeds it and weapon `Sound` events (with the sound's
  `volume`) to `Bots::notice`.
- `BotThought::noticed` (`BotNotice { why, since, until }`).
- Hooks in `bots.rs` (a few lines each), one swap in `arming.rs`. No edits
  to `behaviour.rs`, `claims.rs`, `planning.rs` or the chooser.

## Decisions

- Combat alertness uses the kind's plain numbers (no combat discount): a
  0.6 combat scale doubled behaviour switches in the deathmatch gauntlet.
- Alertness comes from the previous tick's behaviour: Fight, Chase, Fly,
  Search are combat; Wander and Interact relaxed; the rest ordinary. A
  goof option from the surprise lane belongs in the relaxed arm of
  `Alertness::of` (non-exhaustive match).
- Hurt uncertainty does not depend on `alertness`: it is what the bot can
  know, not how alert it is.
- Other lanes' sight checks are not edited here; their one-line swaps are
  in the lane report.

## Evidence

RESULTS

## Next

- Team and surprise lanes: swap their rays to `bot_sees` /
  `bot_sees_player`; surprise lane: `Brain::switch_delay` for its tell
  and its goof option in `Alertness::of`'s relaxed arm.
