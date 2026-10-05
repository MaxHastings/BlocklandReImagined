# 2026-10-05 v0.2.4 preview: the last bot lanes merged as they are

Maxwell asked for everything finished to be merged and handed to him as a
preview, with no further bot test runs: his own playtesting finds the gaps.

## Merged
- `fix/bot-route-planner` (9d19371c): one route planner with string-pulled
  walks and jet legs (the Fly behaviour is gone), runover kill credit and
  the teleport/portal fling fixes.
- `fix/bots-ball-games-2` (1e9c0283): one hold rule for every choice
  (`bots.json` `hold`), goofing and weapon upgrades as scored options, melee
  footwork. Resolved onto the planner: scores are `[f32; Behaviour::COUNT]`,
  the Fight score fades past its band and keeps the planner's reach-up and
  out-of-reach memory, a stacked melee fighter still steps off sideways, a
  strafing fighter in the air still steers back from an overshoot, and the
  hold rule replaces the old tell pause.
- `fix/bot-team-roles` (628ab2cf): the mood is looked at on each bot's own
  beat (`cadence` salt `MOOD`) and cached in its team state; team sight uses
  line of sight.

## Dials, by judgement (no sweep)
- `surprise.strength` 0.5 to 0.6 and `team.teamwork` 0.5 to 0.6: a little
  more variety and more visible teamwork for the showcase. Everything else
  stays as the lanes left it (`hold` 0.5 s / 0.1, `extras.strength` 1,
  `perception.strength` 1, mood 16 capped at 10, pressure 0.3, copy 0.15).
  Everything is on by default.

## Checks
- `cargo clippy -p bri-sim -p bri-chaos --tests`: clean.
- Bot suites and the gauntlet were not run, as asked. Known open from the
  lanes: `all_dials_on` (water_between_the_sides switches), soccer line-up
  and contested-ball bars, `a_jeep_on_each_side` switches.

## Next
Windows preview from main for Maxwell; publish after his OK.
