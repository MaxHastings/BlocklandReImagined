# NPC design review for v0.4.3

Maxwell asked for a step back across perception, decisions, navigation, motion,
and teamwork before more stacking work. This is the current integration handoff;
the acceptance list in `docs/plans/2026-10-30-connected-sessions-bots-and-lighting.md`
remains authoritative and is not reduced by these decisions.

## Existing owners

- `bots/sight`, `perception`, `hearing`, and `search_memory` own observed,
  dated evidence. Allies share observations, not a target's unseen live pose.
  The explicit exact-location event is a separate authored exception.
- `behaviour`, `team`, and the `Brain` choose goals using scores, holds and
  expiring teammate intents. `claims` currently reserves vehicle bodies and
  seats; it is not already a general living-actor support reservation.
- `nav::Search` plans on a shared body-specific, invalidated static-world
  grid with time-sliced budgets. `Ground` separately considers moving players
  as obstacles. `reach::Leaps`, `Ground::sweep_arc`, and `route::LeapLeg`
  measure, check and execute ordinary jumps; an observed failed leg feeds
  existing recovery. The static arc sweep alone does not validate a moving
  actor, other moving actors, or a future support's safety.
- `bots/act` arbitrates movement and buttons, then fall/blast/portal safety
  judges the result. `bri-motor` owns actual player movement and contact;
  host simulation owns authoritative state. Client prediction mirrors other
  actors' latest network poses; `set_others` does not simulate their velocity.
  This matters for any support velocity or synchronized jump: it needs a
  defensible host/prediction/reconciliation contract, not a bot-only impulse.

## Mismatches and implications

1. The cube trace confirms one actual 384-tick support contact. The grid has
   no dynamic teammate landing and the ordinary hop safety probe ignores
   actors. Existing no-world-floor recovery can also step a bot down from a
   body. None of those alone is a complete explanation of the cube outcome.
2. Team intents and crowd avoidance help ordinary movement but do not negotiate
   a base, climber, jump or landing. A pile needs bounded commitments that
   revalidate liveness, alliance, collision group, geometry and target evidence;
   it cannot depend on processing order, a zombie name, or an omniscient target.
   Ordinary usable routes must remain available and retain their own acceptance
   tests. Cooperation is a default-enabled general bot capability, subject to
   existing kind policy, not a special zombie motor.
3. `search_memory` currently tries the last observation and four nearby probes,
   bounded by evidence expiry. That is useful local search but is not proof of
   the requested connected-building search. A continuing search needs to track
   visited reachable space and entrances/stairs, budget its work, and distinguish
   a genuinely exhausted search from a search still waiting on navigation. It
   must not read an unseen actor's current coordinates.
4. The saved cube showed repeated partial searches with tens of thousands of
   explored nodes. Search resumes in bounded slices and a large frontier helps
   find distant stairs; neither rejecting all partial routes nor removing the
   limits is justified. Measure repeated work and cache invalidation, then
   improve reuse/fairness without treating a soft limit as an unreachable proof.
5. The current trapped/self-respawn path still needs an explicit policy audit.
   `auto_respawn` alone is not proof that a kind intended suicide when navigation
   fails. The cube's observed respawn remains unexplained. Do not treat that
   respawn or the vertical-progress correction as a general fix for the house.
6. The corrected paired-jump experiment gives no boost for its tested timings.
   It does not establish the physics design for every moving support. A real
   transfer mechanism must not add hidden lifts, jets, teleportation, or reach;
   test the same physical behavior for human and bot actors and prediction.

## Implementation order and evidence

First make the pursuit/search and recovery contract explicit across short sight
loss, long connected interiors, inaccessible visible targets, an unseen spawn,
and deliberate exact tracking. In parallel with that work, implement a bounded
cooperation lifecycle under the existing coordination and act owners: propose,
agree on a live support and climb, execute measured ordinary controls, verify
actual contact, and release/replan on success, loss of support, danger, changed
intent, reset or timeout. Integrate route and local movement deliberately;
do not retain dynamic actors in the static geometry cache. Extend shared motor
support motion only with separately validated authoritative and prediction
behavior. Avoid a separate zombie-specific planner or per-tick forced actions.

Re-run all five supplied saves, including the tower/stair routes independently
of stacking and the cube with one bot and sufficient allies. Require actual
contact and a legitimate attack; also cover moving/dead supports, pass groups,
different sizes, low ceilings, interruption, reset, and a crowd/performance
budget. Report headless, rendering and user-playtest evidence separately. The
user has waived further interactive playtests; do not infer one occurred.

Sources inspected: `docs/architecture/bots.md`, `crates/sim/src/session/bots/`
(`act`, `team`, `claims`, `sight`, `hearing`, `search_memory`), `nav.rs`,
`route.rs`, `prediction.rs`, `crates/motor/src/player.rs`, and the private
cube contact / synchronized-jump reports recorded in
`2026-10-31-v043-bot-teamwork-defaults.md`.
