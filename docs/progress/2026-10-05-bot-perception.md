# 2026-10-05 Bots notice things: glances and reaction delay

Lane `fix/bot-perception`, approved by Max: a small, general "what a bot
notices" layer. Base `claude/project-thread-pt64ji`.

## What changed

- `crates/sim/src/session/bots/perception.rs` (new). Two mechanisms:
  - **Glances.** An idle bot (Wander or Return; no enemy in sight, no
    objective at hand, nothing held, not seated or driving) turns its
    ordinary aim for about 0.8 s at the most salient of: a projectile
    blast (reach per unit of blast radius), a weapon sound (reach at full
    volume), a 1.5 s stare from someone within 8 degrees and in plain view,
    a body over 14 u/s. Salience is 1 at the source falling to 0 at its
    reach and is the chance of a glance; a 6 s cooldown follows. The walk
    goes on; only the look turns.
  - **Reaction.** A newly seen target, or an attacker it was not fighting,
    gets a delay of `reaction_seconds` x `combat_scale` (fighting or
    hunting) or `relaxed_scale` (strolling, interacting), x `away_scale`
    outside its `view_degrees` cone, +-`jitter`, from its own seeded RNG.
    The same scale multiplies the starting aim error, which narrows over
    the existing two seconds of tracking. Outside the cone it also turns
    `away_scale` times slower toward the target until it has reacted (no
    instant 180-degree snaps). A spawn-protected target is watched but not
    reacted to; the clock starts when it can be hurt (edge case I8/A7: no
    instant kill the tick protection ends). Damage still interrupts at once
    through the existing threat path; only the return fire waits.
  - `delay_ticks` and `Brain::switch_delay` expose the same delay for the
    surprise chooser's tell (audit part 10: "merge into the reaction").
- `BotKind::perception` (`bot_kind.rs`), 12 tunables, validated, on by
  default for every kind (Max: no "off" stage). The Blockhead's
  `bots.json` lists them. The audit counted 21 in the first cut; merged to
  one reach per source, the wobble folded into the existing aim-error
  narrowing, gaze cone/time and the "fast" speed made documented constants.
- Weapons runtime: a `Blast { source, position, radius }` event from
  `explode` (engine data for bystanders). The sim feeds it and weapon
  `Sound` events (with the sound's `volume`) to `Bots::notice`.
- `BotThought::noticed` (`BotNotice { why, since, until }`): the last glance
  or reaction and why (`glance: blast`, `reacting: relaxed, from behind`).
- Hooks in `bots.rs` only (a few lines each): target acquisition and hurt
  start a reaction; the fire gate reads it; aim error and turn rate read
  its scales; the aim chain takes a glance before the walk direction.
  No edits to `behaviour.rs`, `claims.rs`, `planning.rs` or the chooser.

## Decisions

- Alertness comes from the previous tick's behaviour: Fight, Chase, Fly,
  Search are combat; Wander and Interact relaxed; the rest ordinary. A
  flavour/goof option from the surprise lane belongs in the relaxed arm of
  `Alertness::of` when it lands (non-exhaustive match, so a new variant
  compiles meanwhile).
- Glances only in Wander/Return, so search sweeps, objectives and combat
  aim are untouched.
- The spawn-protection fix does not add its own fire block: the existing
  native withhold keeps aim and proven holds; perception only restarts the
  reaction clock when protection ends.

## Evidence

- `cargo test -p bri-sim --lib perception`: 11 passed (glance in range,
  not when ineligible, cooldown, stare, fast-motion threshold, goofing vs
  combat delay and error, cone turn cap, hurt-then-seen keeps one
  reaction, spawn protection restarts the clock, seeded variation, weights
  at 0 off).
- `cargo test -p bri-chaos --test bot_perception`: 3 passed (a real stare
  turns an idle bot's look to within 12 degrees of the watcher; none at
  reach 0; an armed relaxed bot's first wound comes at least 100 ticks
  later than plain, with a 180-tick `reacting: relaxed` readout).
- Full suites: see the lane report (bri-sim, bri-weapons, bri-chaos bot
  tests with perception on).

## Next

- Surprise lane: call `Brain::switch_delay` for its tell and put its
  flavour option in `Alertness::of`'s relaxed arm.
- Audit Pf3: the watcher poll's rays are not yet under a shared sight
  budget (they run every 12 ticks per idle bot).
