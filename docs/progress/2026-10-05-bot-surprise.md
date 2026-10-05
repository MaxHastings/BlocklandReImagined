# 2026-10-05 Bot surprise (off by default)

Branch `fix/bot-surprise`, based on the bot root-cause lane's
`fix/bots-ball-games` (2fcb7b89) for its arbitration changes.

## What changed

Maxwell asked for "variation among bot behavior and change over time",
with no personality or moods, general across every weapon and vehicle,
and delegated the design. Built as new files plus small hooks, so the
root-cause lane merges cleanly:

- `crates/sim/src/session/bots/surprise.rs` (new): one chooser,
  `Mind::pick`, used at four choice points (behaviour, weapon, splash aim
  point, chase route). A weighted random pick among options whose score,
  adjusted by effectiveness, is within `band` of the best; weights carry a
  per-bot drift (bounded Ornstein-Uhlenbeck walk) and boredom. Guards:
  commitment, carrying/urgent gating, a zero-score viability filter, and a
  tell (hold the old option, stand still, hold fire) before a switch the
  variation causes. Flavour interrupts at natural pauses through ordinary
  player commands (emote, spray can, drop tool, toggle light, equip) or
  ordinary controls (stare, hop, circle, detour, look, crouch). Shot
  outcomes (target lost health or died, judged after the flight) and getting
  stuck feed effectiveness. `BotThought::surprise` (`BotSurpriseView`) is
  the readout: drives and every term of the last decision at each point.
- `behaviour.rs`: `choose` split into `scores` and `best` (same result).
- `combat.rs`: the planner's weapon pick goes through the chooser on the
  planning turn; splash weapons (real `splash_radius`/`splash_damage`) also
  offer feet and nearby-surface aims, solved, `safe_blast`-checked and
  path-checked like any shot. A surface aim's path is clear when it reaches
  its surface, and the firing gate requires its impact within the splash
  radius of the body.
- `bot_kind/surprise.rs` (new): the kind's `surprise` tunables with
  validation. `BotPack::from_json` now skips `//` comments outside strings,
  so the Blockhead's `bots.json` documents each tunable. `strength` is 0.
- Docs: "Surprise" in `docs/architecture/bots.md`; a "Bot surprise" row in
  `docs/architecture/seams.md`.

## Evidence

- `cargo test -p bri-sim --lib`: 215 passed (2 ignored are pre-existing).
  New tests: `surprise::tests` (determinism for a seed; strength 0 is the
  plain pick with no random draw; commitment holds and yields when out of
  band; gating; viability filter; tell before switch; effectiveness decay
  moves picks to feet and to the weapon that lands; boredom and drift;
  no interrupts while carrying), `bot_kind` comment/validation test.
- `cargo test -p bri-chaos --test bot_brain --test bot_knowledge`: all pass.
- Gauntlet `surprise_by_strength` (mixed-arsenal fight, 6 bots 60 s; idle
  pause, 3 bots 60 s). Strength 0 reproduces `deathmatch_mixed_arsenal`
  exactly (same kills, shots, circling, switches and reversals).

| strength | fight variety /bot-min | switches off plain /bot-min | kills | reversals /min | idle goof share | longest goof |
|---|---|---|---|---|---|---|
| 0 | 17.5 | 0.0 | 67 | 11.4 | 0.00% | 0.0 s |
| 0.5 | 19.4 | 2.5 | 71 | 13.1 | 2.44% | 2.3 s |
| 1 | 22.4 | 5.1 | 69 | 18.1 | 7.50% | 3.0 s |

  Goof in the fight is 0 at every strength (no natural pauses). Idle
  variety stays 2.0 (only wandering); its variety is the interrupts.
  No team kills, self kills or shots at allies at any strength.
- `a_jeep_on_each_side` fails identically on the base commit 2fcb7b89
  (stuck 9.9%, team kills 2); it belongs to the root-cause lane.
- `cargo clippy -p bri-sim -p bri-chaos --tests -- -D warnings`: clean.

## Next

- Tune after the root-cause lane lands: raise `strength`, then put bands
  on the gauntlet's variety, goof share and longest goof.
- Watch reversals at strength 1 (11.4 to 18.1 a bot-minute): tells and
  flanks change direction more often.
- Interactive playtest by Maxwell once a strength is chosen.
