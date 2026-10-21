> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `bots/mood-design-note.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Design note: one mood instead of many dice (v0.2.6 or later)

Status: a proposal. Max said yes to writing it up on 2026-10-07 at 00:27Z. Nothing is built.
Code references are to branch `claude/v0.2.5-s4s5bn` at 41287b0, with paths under `crates/sim/src/session/`.

## The idea in one paragraph

Everything that makes the bots feel human has the same purpose, but today each piece rolls its own randomness. Keep the three stages the code already has (what it knows, what it picks, how well it does it). Replace the separate sources of variation with **one small mood per bot**: a few numbers that change from what happens to the bot and settle back on their own. Every stage reads the mood. Humans are inconsistent in a *connected* way: a hurt, frustrated player aims worse, turns harder and doesn't goof. Today our bots vary each trait separately, which reads as random. With one mood the traits vary together, which reads as a person.

This is engineering, not a personality system. It replaces about six variation sources with one, so the knob count goes down.

## What varies today, and where

| Source | What it drives | Where |
|---|---|---|
| Drift: a slow random walk per option, which reverts over about 90 s | Which near-best option wins | `bots/surprise.rs:469` (`Mind`, `:394`) |
| Boredom, per option, with a 20 s half-life, and the lull | Goofing, switching off a long choice | `bots/surprise.rs:581` (`accrue`), `PLAY_BOREDOM` `:67`, `LULL` `:72` |
| Effectiveness, per option, over 30 s | Dropping what isn't working | `bots/surprise.rs:592` (`outcome`) |
| The ±15 % near-best band | Imperfect picks | `bots/surprise.rs:40` (`BAND`) |
| Goof roulette, with weights per flavour | Which goof | `bots/surprise.rs:701-760` |
| `reflex_rng`, a second random stream | Dodge draws | `bots/surprise.rs:399, 414, 431` |
| Cadence: seeded beats, bursts and footwork per bot | Timing of fire, footwork, alerts | `bots/cadence.rs` (stateless, pure function of bot, salt and tick) |
| Reaction jitter of ±30 %, scaled by alertness | How fast it reacts | `bots/perception.rs:307-308` |
| Aim error size and direction | How well it aims | `bots/perception.rs:570` (`steady_error`), `:593` (`drift`) |
| Alertness, *read from the chosen behaviour* | Reaction, and from round 2, turn speed | `bots/perception.rs:245, 662`: Fight, Chase or Search counts as Combat; Wander or Interact counts as Relaxed |
| Private timers in extras (dodge, door click, hand-off, crouch) | When those fire | `bots/extras.rs` |
| Strafe turn-back coin, 70 % | Footwork direction | `bots.rs:179, 574` |

Note that alertness today is a *result* of the choice: the bot is "in combat" because it chose Fight. A human is the other way round: getting shot makes them alert, and being alert changes what they choose.

## Proposed shape

**Mood**, per bot, a handful of numbers from 0 to 1:

- **Alert:** jumps on a hit, a seen enemy, a heard fight or a near miss. It settles over seconds.
- **Bored:** grows while nothing changes and while it does the same thing. A change of scene or a fight resets it. This is today's boredom, made per bot instead of per option.
- **Frustrated:** grows on misses, stalls, no progress and being killed. It shrinks on hits and progress. This is today's effectiveness, made general.
- **Confident:** follows health, winning fights and the team score. It shrinks when hurt or losing.

Each one settles back toward the kind's resting value. This built-in calming is what stops runaway loops.

**Who reads it.** Every reader is one multiply on a value that exists today:

| Stage | Reads | Replaces |
|---|---|---|
| Knows | Alert shortens reaction and widens the glance. Bored makes it notice less. | Alertness from the behaviour, and the reaction jitter dice |
| Picks | Bored raises goofs and exploring. Frustrated lowers what isn't working and raises switching. Confident raises Fight and Chase and lowers retreat. The band's width follows calm versus alert. | Per-option boredom and effectiveness, goof roulette, part of the drift |
| Does | Alert means faster turns and steadier aim after a moment. Frustrated means shakier aim and shorter bursts. Bored means a lazy head and wandering looks. | The flat turn speed, the aim error only partly tied to state, and the burst pauses |

**What stays as is:**
- **Cadence.** Seeded timing is not randomness about behaviour. It stops every bot acting on the same tick, so it stays.
- **Drift.** A small amount stays as the "never the same twice" ingredient, but it reads one stream, not several.

**The kind becomes a temperament.** A kind's resting value and settle rate for each mood number replace most of the variation knobs. "Hothead" means high resting alert, quick frustration and slow calming. "Calm" is the reverse. That's about 8 numbers per kind instead of today's surprise and perception sub-settings.

**F3.** One line, for example "Mood: alert 0.8, bored 0.1, frustrated 0.6, confident 0.3". It explains most of what the bot does, because every stage reads from it.

**Emotes and callouts** come from mood changes: a frustration spike after dying, a confidence jump after a kill, an alert spike on surprise. This answers the "emote after a kill" item without new state. The small "just won" memory dropped in the human-variation list becomes the confidence jump.

**Team.** Moods can spread a little between nearby allies, as the existing social pull already does for goofing and playing. "Goofing spreads and seriousness spreads" becomes boredom and alertness spreading.

## Why it's worth it

- **Fewer knobs.** About six variation sources and their settings become one mood with about 8 numbers per kind. This follows Max's "less knobs" rule from 10-05.
- **Connected behaviour.** Correlated variation reads as a person, while independent dice read as random. This is the main gain in human feel.
- **Diagnosis.** One line of F3 explains why the aim, the head and the choice are the way they are right now.
- **It fits the design.** It is perception, options, chooser and executors reading one input. It isn't a new stage, and nothing is keyed to content.

## Risks and how to contain them

- **Runaway loops.** For example, frustrated means it misses more, which makes it more frustrated. Contain this with:
  - every mood number settling back to rest at a fixed rate;
  - each reader's effect capped;
  - a never-invariant test: over many seeds no mood is pinned at 0 or 1 for long. This is an absolute, which fits the test bar.
- **Fairness.** Mood changes aim. Keep the aim effect inside the perception strength dial, so fairness keeps one knob.
- **Too predictable.** If mood explains everything, players can read it. Keep a little drift.
- **Determinism.** Mood is ordinary state on the brain and is fed by events, so replays stay the same. Reaction draws come from the one stream.
- **This reverses a narrowing.** On 10-04 Max narrowed away "moods and personality", and on 10-05 deferred confidence and grudges as extra loops. This proposal is different because it *removes* loops and knobs rather than adding them, but it does bring a mood back. Max decides.

## How to get there (when the time comes)

1. **Read only.** Compute the mood from today's signals (hurt, sight, boredom, effectiveness, health) and show it in F3. Nothing reads it yet. Then watch whether it matches what Max sees in play.
2. **Do stage first.** Turn speed, aim steadiness and burst pauses read the mood. It's the easiest to see on camera and the lowest risk.
3. **Picks stage.** Per-option boredom and effectiveness become mood readers, goofs become scored options that read bored, and `reflex_rng` and the goof roulette go.
4. **Knows stage.** Alertness comes from the mood, not the behaviour.
5. **Kinds become temperaments.** Old knobs are deleted, not hidden.

Each step stands alone and can stop there. Step 1 is small, steps 2 to 4 are medium each, and the whole thing is big. About 80 % of the state already exists in `surprise::Mind`, so this reshapes code rather than rewriting it.

## Not part of this

- New behaviours.
- Grudges or revenge targeting. They'd be a separate decision, though mood would make them a one-line reader if ever wanted.
- Anything for v0.2.5.
