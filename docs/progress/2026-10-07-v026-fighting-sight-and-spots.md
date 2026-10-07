# 2026-10-07 v0.2.6 fighting: one visibility question and where to stand

Branch `claude/v0.2.6-fighting-ki8k6d`, PR 3 of `docs/plans/v0.2.6-bots.md`,
built to the reviewed `docs/plans/v0.2.6-design-fighting.md`.

## What changed

**One visibility question.** Bot sight now asks what an eye sees, from
engine data only:

- Paint: `Simulation::eyes_see` looks through a brick painted under full
  alpha, the client's own see-through test (`brick_cover`). Bot sight
  (`bot_sees`) uses it. `Simulation::sight` is unchanged for hands and
  vehicle boarding, and shots (`clear_path`) still stop on glass.
- Fog: `Session::bot_sight_reach` caps a kind's sight at the server's
  visible distance (the live environment). Every bot eye check uses it.
  Known gap: a map's own authored fog is scene data the host does not
  load, so only a server-set distance caps bot sight.
- Target size: `Seen::aim` (`sightlines::aim_point`) drops from the eye by
  half a unit times the target's scale, for the shot chooser, the
  clear-fire check and the harm line. The aim error stays an angle.

**Where to stand** (`bots/spots.rs`). A ranged fighter on its feet weighs
seven stable options on its planning turn (here, left, right, in, out a
body's width; hop onto a ledge ahead; jet onto one higher), from the walk
grid and measured reach. One score in harm per second, no new weight:
`dealt / (travel + fire) x (1 - taken)`, with dealt and fire from the shot
chooser run from that place (`combat::choose`'s origin parameter,
`tactics::worth`), and taken the share of its health the known threats
seeing the place and allies' lines across it would take
(`tactics::rate`). With no shot anywhere, the least exposed place wins.
`Domain::Spot` in the surprise chooser picks with the hold rule; the
place is a route goal (`Goal::Stand`), and once reached it is "here"
again, held from then (`Mind::arrived`). F3 shows each place's three
quantities.

**Deleted:** the Team mover (`act::Mover::Team`, `team::exit`; an ally's
line of fire is now a cost of a place), the stance's step back inside the
band's near edge (`back_off`) and the strafe's lean in (`LEAN`,
`LEAN_TICKS`, `LEAN_ROOM`, `cadence::salt::LEAN`).

**Not in this PR: the cover case.** The design's second case (covering a
body a teammate holds by scoring the places round it, deleting
`contest::cover` and the swap) needs a different worth (reaching the body
when the claim frees, standing in the push line) and the soccer "ignoring
ball" share measured before and after on full matches, which the lane's
iteration rules keep out of the lane. Per the design it comes out whole:
`contest::cover` and its swap are untouched, and the case stays queued.

## Decisions

- In and out step as far as the weapon's own band (`Weapon::band`) takes
  the fighter back inside it, a body's width at least, and a place nearer
  the enemy than the band's near edge deals nothing: so a rocket or gun bot
  with an enemy hugging it steps back out, the job `back_off` did. The
  shot chooser now puts the shooter's own body at the origin it weighs
  (its blast clearance), not where it stands.
- The ledge probes step by two body steps from a body step up; `Nav::node_at`
  finds a floor within more than a step of where it is asked, so no height
  is skipped and no nav tolerance is copied as a number.
- Spots are for ranged fighters on foot only. A melee fighter closes and
  keeps its footwork; a swimmer weaves; a rider rides. Their old step
  back went with `back_off` (a melee band's near edge is 0, so it never
  fired for melee).
- Hop and jet probe the column one body width toward the enemy, hop up to
  the measured jump height and jet up to one jump above the enemy's floor.
  A ledge straight overhead is not offered.
- A melee fighter standing in an ally's line of fire is no longer walked
  out of it (the Team mover is gone and spots are ranged only); the ally
  still holds fire across it (`bot_fire_clear`). The `team` harm term,
  which cost a Fight standing in a line (dropping a sword bot's Fight below
  Chase once nothing walked it out), is deleted with the `harm` callout key
  (`bot_kind::TERMS`, the Blockhead's `bots.json`): an ally's line is a
  cost of a place now, so the gunner steps aside. Melee line of fire waits
  for v0.2.7.
- With no shot from a place, its fire time is the weapon in hand's cycle,
  so standing in sight still costs something and a bot with nothing to
  shoot keeps out of sight.
- Exposure rays use the bot's own fog-capped sight reach for every threat
  (it sees them that far; they see it that far), cached per (threat,
  bot, option) like other sight answers.

## Evidence

- `cargo test -p bri-sim --test building eyes_see_through`, `--lib
  fog_caps_how_far_a_bot_sees a_shot_aims_lower`: pass.
- `cargo test -p bri-sim --lib spots`: seven pass, among them
  `a_ranged_bot_with_an_enemy_inside_its_band_steps_back_out_of_it` (a
  rocket and a gun bot two units from an enemy each choose *out*, to the
  band's near edge),
  `a_bot_behind_a_wall_steps_aside_to_the_open_side_every_time` (strength
  0, four seeds) and `a_bot_never_weighs_a_place_with_no_floor`.
- `cargo test -p bri-sim --lib -- session::bots bot_kind`: 165 pass, 1 ignored.
- `cargo clippy -p bri-sim --tests -- -D warnings`: clean.
- `cargo test --release -p bri-chaos --test bot_gauntlet bot_think_time_16
  -- --ignored --nocapture`: 1441 us/tick for 16 bots, all dials on (90 us
  a bot), under the 2000 us release bar; passes.

## Next

- The cover case as its own change, with the soccer share before and after.
- Replay of one of Max's saves on this branch for reproduction.

For v0.2.7:

- Bots no longer call out "Moving!" when an ally's fire moves them: the
  `harm` callout went with the team harm term, and a step out of an ally's
  line is now a choice of place, which has no callout yet.
- Melee line of fire: a melee bot in an ally's line is not walked out of it.
- A strafe limit round the place a bot fights from (tried as `Brain::stand`
  during the gate fixes, taken out): measure first; it changes how far
  every ranged bot holding position strafes.
- A tighter bound for weapons that push when weighing places to stand:
  today a pusher's bound is the target's whole health, so with v20 guns
  (which push) every place is weighed on each planning turn.
- The `all_dials_on` interference with tests running beside it is CPU
  contention (one worker per core), not shared dials.
