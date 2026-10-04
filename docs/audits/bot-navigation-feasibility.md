# Bot navigation and physical feasibility — v0.2.1

Six deterministic headless tests in `crates/chaos/tests/bot_navigation_spike.rs`
separate ordinary physics capability from NPC planning. This is synthetic,
authored collision/control evidence, not native-map or human playtest acceptance.
After initialization no actor transforms, support geometry, hidden platforms,
jet input or teleportation are used. NPC cases use real session brains,
ordinary movement, MiniGames and authored event execution.

| Case | Evidence | Limit |
|---|---|---|
| Three-actor stack | Ten simulated seconds of idle input. Minimum feet heights 0.0100, 2.6722, 5.3346; every actor grounded at end. | Initial actors placed above one another; this proves support stability, not formation coordination. |
| Ordinary climber, 5-unit platform | Climber starts on ground. Matched solo run cannot reach (apex 3.6486). With one stationary actor, ordinary jump/move input mounts its head and reaches the platform (apex 6.3037; landed feet 5.0100). | The test supplies a small feedback control policy, not NPC planning. No jet or movement correction. |
| Fixed navigation negative | On the same physical actor/platform fixture, actual bounded Nav completes as `Partial([3,0,0])` while goal is `[3,5,0]`. | Ground samples only fixed colliders. Physical actor support is excluded by design; no dynamic support transition exists yet. |
| Rotated narrow interior pursuit | Fixed room walls/lintel rotate 30° around an off-grid origin (0.17,20.23); real opening 1.8 units. Full-width box projection 1.708. Ordinary human passes, then autonomous bot follows a human acquired outside into the room (closest goal distance 2.4735). | One authored opening/angle, not arbitrary interiors. The bot takes longer than the direct human approach; success is physical traversal, not an optimal-route claim. |
| Long authored objective | Bot begins near x=-75 and physically reaches the finish region near x=80 (max x=78.8296), earning 17 through onRegionEnter plus winRound. | More than 155 units exceeds the 72-unit per-search bound, proving continuation across bounded segments. A flat route, not general topology completeness. |
| Live actor obstruction | Roofed 1.8-unit corridor. A non-enemy actor holds the bot at x=-1.2500 through four seconds, then clears the approach via ordinary movement; the bot reaches the authored region and earns17. | Confirms live obstruction/retry recovery in this fixture. Nav does not cache this actor as fixed geometry. |

The first pursuit attempt put the human hidden inside the room, so the bot
wandered rather than acquiring a target. Another fixture installed a bot-owned
world before the host joined, giving the brick an unintended owner identity.
The final fixture joins the real host, loads its build through the normal
command, creates its MiniGame, then moves a previously visible human through
the doorway. Those failed fixtures are not recorded as navigation defects.
A temporary test compile error used a nonexistent mutable-values iterator on
the persistent brick map; initialization now updates actual keys explicitly.

## Mechanism boundary and proposed support affordance

The player motor already gathers other actors into contact polygons, including
jumpable top faces. Dynamic actors are not missing physical supports. In
contrast `Ground::filter` intentionally uses `only_fixed().exclude_sensors()`;
the ordinary nav cache must not silently retain moving actors as immutable
floor samples. The new negative test records this exact reachability gap.

A minimal separate mechanism should accept a bounded observation set:
`SupportSurface` with real actor identity, footprint/top, current velocity,
contact/alive state and observation tick; mover body, start and target; and an
authoritative geometry/jump-clearance callback. It should produce explicit
jump/land transitions referencing the actor, rejecting stale/moving/insufficient
support. Existing fixed Nav/cache stays the durable world path mechanism.

The central NPC adapter owns policy: filter actual allies, reserve an existing
stationary supporter and landing footprint using bounded claims, execute normal
move/jump controls, and revalidate identity, current surface pose/velocity,
clearance and claim every tick. Movement, death, team/round loss or lost contact
releases/replans; proposed outcomes are never written as world state. A holding
or ascending role must derive from the reachability job and resource claim,
not a specific puzzle/objective name.

First integration acceptance should be a real NPC traversing an existing allied
actor's support with ordinary controls, plus invalidation when it moves away.
Recruiting/moving several allies to form a new stack is additional coordination
work. This audit does not claim that feature is implemented from standalone
physics evidence. Root owns the integration decision and central hooks; this
lane has not edited navigation or actor motor code.

## Commands

- `cargo test -p bri-chaos --test bot_navigation_spike -- --nocapture`: six passed, zero failed, 0.28 seconds after the final fixture corrections.
- `rustfmt --edition 2024 crates/chaos/tests/bot_navigation_spike.rs`: completed.
- Touched-path `git diff --check`: passed.
- Warnings-denied scoped client/crash/navigation clippy is recorded in the lane progress entry when complete; concurrent NPC edits previously blocked the stability check.

No window or operating-system input. Vanilla inputs are unchanged. Full
release gate, platforms and Maxwell's interaction/fidelity review remain open.


## Independent target-child objective regression

`an_unmodelled_named_target_reaction_rejects_the_whole_objective_action` uses a
real authored build, host-created MiniGame and NPC movement. Its supported
region writer increments a named latch's Target variable, sets MiniGame
progress, and enables a guarded score/win region. It selects writer brick 2 and
scores 17. With `onRuleVariableChanged` on the actual named target resetting
progress and hiding itself, the whole writer is rejected: score 0, no selection
of writer 2, latch remains visible. This covers otherwise omitted collateral;
it never invokes projected effects as execution. The foundation owner owns the
runtime rejection fix. The test was added after that fix, so no failed-before
executable result is claimed. Full suite: seven passed, 0.26 seconds.

## Shipping Workshop race heldout

`workshop_race_recipe_finishes_three_laps_through_actual_npc_region_crossings`
creates the actual recipe through the normal `/rulelab` PackageCommand dispatch,
with a standard Blockhead NPC in an ordinary host-created MiniGame. Actual
checkpoints 2/3/4 are selected; physical crossings produce score 0/1/2/3. Bounded
rule tracing records the canonical `winRound -> Player ...: ran` action. Another
five seconds leaves score 3, matching the recipe's round-over gating. No modified
recipe, direct event firing or injected bot aim/movement/objective is used. The
first Chat-only attempt created no lab because slash commands use PackageCommand;
that was corrected harness input. Final suite: 8 passed, 0.26 seconds. This is a
shipping-recipe context/limits regression, not evidence for arbitrary modes or
unimplemented cooperative support.
