# 2026-10-11 Perf waste fix: bot relations once per tick, goof look-around every tenth second

Findings 5 and 6 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files), and the measurement
that left finding 4 alone.

## What was wrong

- `bot_allies` (who sides with whom) was derived from scratch on every ask:
  two peer lookups, two minigame player lookups, two brain lookups and two
  brick-owner lookups. Every stage of every bot's step asks it for the same
  pairs: `team_intents` walks every live intent through it, `bot_enemy`
  calls it per peer, `bot_walk_direction` and the harm bodies again. In the
  16-bot cloud profile it was 9.2% of all samples (6.4% self, 2.8% in
  `bot_brick_owner`) plus 3.7% in `bot_enemy` and 4.6% in `team_intents`.
- `surprise_pause` (what a bot could do idly, such as waving at a player)
  cast a sight ray to every live player within 24 units on every tick of
  every bot: 9.4% of samples, 6.6% of it in that iterator chain.
- Finding 4 (the shot re-validation every tick) measured at 1.2%
  (`validate_intent`) in the same profile: under the fix bar, so it is left
  as it is.

## Fix

- `Bots::relations`, a per-tick map of (bot, other) to allied, cleared at
  the start of `step_bots` after the minigame sync and bot reconciliation;
  `bot_allies` answers from it and computes a missing pair once
  (`bot_allies_now`, the old body). Cadence and caching only: the answer
  for a pair is the one the live state gave the first time it was asked
  that tick. A relation that changes in the middle of a tick (a script
  moving a player between teams during the tick) is seen next tick, which
  is already how the 30-tick minigame sync behaves.
- `surprise_pause` takes the tick and looks around for a player every
  `LOOK_TICKS` (12, a tenth of a second); the caller keeps the answer in
  `Brain::surprise_player`. Between looks the last player stands while
  still alive and within 24 units (no ray). A goof is flavour; a tenth of a
  second is below what a player can see.

## Evidence

- Work count: `cargo test -p bri-chaos --test bot_work_counts` (8 bots and
  a human idling for 240 ticks: relations derived 15,360 times, exactly
  once per bot pair per tick, for 37,220 asks; 160 goof look-arounds,
  under the 168 the tenth-second cadence allows).
- `cargo test -p bri-sim`, `cargo clippy -p bri-sim -p bri-chaos --tests --
  -D warnings`: pass.
- `bot_combined_perf` (content-free, release, cloud box with nothing else
  running, 3600 ticks, two rounds each, origin/main `01ca563` against the
  whole audit branch, so these numbers are the sum of findings 5, 6, 10, 14,
  15 and 17 on the server tick, not this change alone), step ms p50 / p95:
  mixed-combat-16 1.32, 1.36 / 3.77, 3.86 -> 1.22, 1.21 / 3.71, 3.64;
  mixed-combat-8 1.24, 1.21 / 2.33, 2.70 -> 1.06, 1.15 / 1.88, 2.04;
  independent-16 0.68, 0.73 / 2.99, 3.09 -> 0.55, 0.55 / 2.90, 2.87;
  independent-8 0.70, 0.64 / 1.18, 1.17 -> 0.59, 0.53 / 1.05, 0.99;
  mixed-combat-4 1.10, 1.10 / 2.01, 1.89 -> 1.04, 0.98 / 1.64, 1.58;
  independent-4 0.61, 0.63 / 0.99, 1.00 -> 0.51, 0.52 / 0.86, 0.95.
  About 10% off the p50 of a 16-bot fight tick and 15-25% off an idle
  crowd's; the 8-bot fight's p95 fell about 20%.

## Next

Finding 4 stays a plausible hotspot with its 1.2% measurement recorded in
the report.
