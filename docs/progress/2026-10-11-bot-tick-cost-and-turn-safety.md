# 2026-10-11 Bot tick cost, the "Empty slot" error and hooks removing bots

v0.2.9 release blocker 4 (bots), branch `claude/v029-bots-hardening-crlqsb`.
The full measurement report, with every table, is
`bots/bot-performance-2026-10-11.md` in the project files.

## Measured

A new ignored harness, `crates/chaos/tests/bot_deathmatch_stress.rs`, loads a
real save through the dedicated-server loader, enables `blockhead_bot`,
`gamemode_slayer` and `weapon_hegrenade`, fills a Slayer free-for-all with
bots, times every `Session::step` and replays what the host's client would
draw of each bot (the server's own pose stream, the client's interpolation
rules from `crates/client/src/motion.rs`, 120 Hz wakeups and 8-step
catch-up).

```sh
BRI_CONTENT=<content> BRI_STRESS_WORLD=<afghan.world> BRI_STRESS_BOTS=32 \
BRI_STRESS_SECONDS=60 cargo test --release -p bri-chaos \
  --test bot_deathmatch_stress -- --ignored --nocapture
```

On Afghan DM (14,091 bricks), release build, a 4-core 2.1 GHz cloud Xeon:

| Bots | Code | Mean ms | Worst ms | Ticks over 8.3 ms | Bot share |
|---|---|---:|---:|---:|---:|
| 16 | before | 6.01 | 91.8 | 18.0% | 89% |
| 16 | after | 4.09 | 21.5 | 6.1% | 88% |
| 32 | before | 7.92 | 86.5 | 29.8% | 86% |
| 32 | after | 5.68 | 36.9 | 11.9% | 84% |
| 63 | after | 8.12 | 27.5 | 36.7% | 81% |

- The bot share is sustained: 87-90% in every 10 s window over 3 minutes at
  16 bots, drifting up as bricks fall.
- Before the fixes, the time went to weapon choice (24.5%), sight lines
  (17.5%), path search and floor sampling (26%) and path-cache invalidation
  (5.1%). The hottest leaf was the brick lookup in brick rays. The 50-90 ms
  spikes were 77-90% `Nav::invalidate`: a rocket knocks out dozens of bricks
  and each one walked every path cache.
- Teleporting comes from the host tick, not the pose rate or interpolation.
  Replayed with zero step time, bots stay within half a unit 99% of the time
  at every count. Replayed with measured step times, 63 bots show 495 jumps a
  minute, while 32 bots after the fixes show none.

## Changed

- `Simulation::walk_bricks` slab-tests each bucket entry's bounds before the
  world lookup and the `accept` filter. Results are identical.
- `Nav::invalidate_all` forgets a tick's collision changes in one pass per
  cache, with nearby change boxes merged first. It forgets a superset of what
  per-change calls did, so routes stay correct.
- "Empty slot" (v0.2.8 bots 5 and 26, v0.2.4 bot 8). A bot's weapon choice is
  made at the start of its turn. The turn can then lose what the choice was
  for. The reproduced case is a gunner whose wheeled hull is tipped past
  upright: `bot_seated_input` dismounts it, then the fight block tried to
  equip the seat gun's sentinel slot on foot. The fight block now checks the
  choice against the live inventory. A choice whose slot is empty is dropped
  as a failed plan, and the next tick chooses again.
- PR #40's hand-off. A rules hook inside one bot's turn (`on_damage` from a
  bite) can `remove_bot` other bots or the hitter itself. `step_bot` indexed
  the removed bot's brain and panicked (`bots.rs` at the top of the turn,
  `fire.rs` after the bite). A removed bot's turn now ends at the top, after
  the goof and extras phases, after the bite and callout, and after the
  objective act. The bite no longer indexes the biter after its hooks.
- `Bots` counts failed bot steps and keeps the first 32. The count is exposed
  as `#[doc(hidden)] bot_step_failures` for tests and the harness.

## Tests

Each new test was run without its fix first, and each failed the way the
field logs did:

- `bot_interactions::a_gunner_thrown_from_a_tipped_seat_drops_its_seat_shot_without_failing`
  failed with `[(2, 239, "Empty slot")]` and now passes.
- `bot_knowledge::a_hook_inside_a_bots_turn_may_remove_the_bots_still_to_step`
  panicked at `bots.rs` "no entry found for key" and now passes.
- `bot_knowledge::a_hook_inside_a_bots_turn_may_remove_that_bot` panicked at
  `fire.rs` and now passes.

## Next

- Path search and floor sampling, then weapon-choice rays, then sight lines,
  in measured order.
- Numbers on Max's PC, where the host also renders.
- The hardening pass on navigation, weapons, communication, driving and
  getting stuck.
