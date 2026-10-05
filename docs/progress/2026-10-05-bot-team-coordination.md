# 2026-10-05 Bot team coordination

Lane `fix/bot-team-roles`. Bots read what their teammates are doing
through score terms in the one chooser (`docs/architecture/bots.md`,
"Coordination"; code in `crates/sim/src/session/bots/team.rs`).

## What changed

- Every bot publishes an intent beside the claims (`claims::Intent`): the
  place its option takes it or holds, its target, the vehicle whose free
  seats it offers while it waits for crew, its weapon space
  (`claims::Space`, also used now by the hold-fire check), the vehicle it
  rides and a seated ally's sightline. Intents lapse three ticks after
  they stop being renewed, so dead, departed and switched bots drop out.
- Terms: overlap (an earlier ally on the same target or doing the same
  option at the same spot), interaction (a seat offer or a driver place
  giving a seated ally line of sight pays; a fight's stance in an ally's
  line of fire costs and steps out), objective pressure from the canonical
  team score, mood (the share of the players a bot sees goofing, less those
  it sees playing, a person counting more, capped), and copy (an option
  seen working for a teammate scores a little more, capped, fading over
  `effectiveness_seconds`). Each visible enemy and each item to arm with
  is its own option, so crowding moves bots to another one.
- Crew of one vehicle neither crowd nor endanger each other; seats stay the
  claim's to arbitrate. An ally ahead in a gap too narrow to step round is
  followed at its pace rather than sidestepped.
- Dials (`bots.json` `team`, all on): `teamwork` 0.5, `mood` 16, `mood_cap`
  10, `mood_human` 3, `pressure` 0.3, `copy` 0.15, plus callout templates.
  Term ratios, crowd radius and callout rate are constants in code; mood
  radius is the kind's sight.

## Evidence

Built with `CARGO_TARGET_DIR=/home/claude/bri-target CARGO_PROFILE_DEV_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo --config cgu.toml`; each suite
run with `--include-ignored --test-threads 2`.

- `cargo test -p bri-sim --lib`: 233 passed (team tests: overlap, seat and
  harm from data, driver sightline, better option still wins, band and
  hold apply, mood rises/capped/sight-only/person > bot/play counts
  against, waves irregular and flat at mood 0, copy lifts/occluded/fades/
  capped, no names in the module, callouts).
- Mood waves (12 minds, one hour): spread 0.142 against 0.068 with mood 0;
  108 s at half or more against 0; mean 0.137; gaps irregular.
- Chaos suites, team off (base) and team on (this branch):
  bot_brain 18 to 19 (new: four allies through one 1.5-unit doorway, all
  through by tick 1088; that doorway does not trip the both-sides gap test,
  so the sidestep alone carries them and the follow rule is not exercised
  there),
  bot_interactions 18 to 19 (new: seat offer read from seat data),
  bot_knowledge 8/8, bot_soccer_match 5/5, bot_soccer_teams 5/5,
  bot_tactics 13/13, bot_team 1/1 (new),
  bot_gauntlet 10/12 to 11/12.
- Gauntlet, base then team on: zombie_survival failed (34.3 switches/min
  > 32) and now passes (25.7); a_jeep_on_each_side failed on base
  (19.0 switches/min > 14, stuck 9.9%) and still fails on switches, at
  15.0, stuck 4.6%, circling 1.0%. Idle 0.0-0.2% everywhere but
  runners_cross_head_on (11.7%, unchanged). CTF 46.5 to 43.9 switches/min;
  deathmatch_open_field 18.2 to 12.0.
- Kickoff spread (soccer 2v2 hands, seed 1): identical with teamwork 0.5
  and 0 (nearest within 3.5 m at +5 s: 1.67 bots; second-nearest 0.95 m
  behind the first). The soccer objective's own roles set the kickoff; the
  overlap term does not reach it because the ball-contest choice carries no
  shared target or spot. Eye-lock is perception's.

## Failures met and fixed

With the terms first on, idle rose (CTF 8.1%) because harm pushed fights
and objectives to Wander: harm now costs only a fight's stance, which
steps out. Place crowding kept a second bot off a passenger seat: seats are
the claim's. A gunner's weapon space covered its own driver: crew are
exempt. Bots piled on one item: an item an ally went first for is left to
it. Following in every case slowed a race (circling 17.5%) and broke 3v3
soccer (seed 2: ball ignored 278 bot-s): only where solid stands close on
both sides. Seat offers from a vehicle under way pulled bots into chasing
it (jeep circling 3.4%): an offer stands only while the driver waits.

## After the merge

- `stairs_to_a_deck` failed on the merged tip with circling 4.5% (bound
  4%); teamwork 0 gave 2.1%. Bisecting the terms (overlap, harm, follow,
  sight crowding) moved it but none was the cause: the overlap only changed
  which enemy a bot chased. The cause was in the chase itself: when the
  path search to a chase goal came back empty, the bot settled, and the
  goal was refreshed only once the enemy moved 2.5 m from it, so a bot
  short of an unreachable goal paced in place. A chase now searches again
  whenever it has settled with no plan short of its goal. With teamwork
  0.5: circling 1.7%, stuck 5.5%.
- Two correctness fixes met on the way: a bot's seniority on an enemy or
  place now counts from when it took that option and target (not any
  intent), and enemy crowding counts only allies who took the enemy first,
  so two bots no longer push each other off the same pick.
- The mood looked round (a sight ray to every peer in range) for every
  bot, every tick. It is now looked at afresh on each bot's own `cadence`
  beat (salt `MOOD`, about once a second) whenever no enemy threatens it,
  kept between and under threat, and cached in the brain's team state, so
  a match keeps a mood for flavour to be scored by once flavour is an
  ordinary option (the bots lane's work); the natural pause reads it as
  before. Team sight (the mood's look at each peer, an ally watching an
  option work) now asks perception's shared `bot_sees_player` instead of
  casting its own rays, and enemy crowding tests an intent's target
  before asking whether its owner is an ally. In `bot_think_time_16` (all
  dials on, 16 bots, 30 s, debug, merged with a00626eb) the mood's sight
  queries by tick 4200 fell from 608,161 to 3,684, and bot think is 3515
  and 3597 us/tick against 4307 and 4957 with the per-tick mood and 5900
  and 4101 on the release tip a00626eb itself.

## Not done

A goal to defend, passing; humans publish no intents beyond what is seen;
the audit's free-for-all brick bots (T1) was reverted here, because
`bots_of_one_builder_are_on_one_side` asserts the current rule and a
lane's test cannot be inverted without a decision; callouts keep their
own per-bot deadline rather than a cadence beat.
