# Source-Faithful, Consumer-Driven Vertical Slice Development

Maxwell adopted this working approach on October 17, 2026. The name describes
our method; it is not a claim that the whole platform or every recreation is
validated. It complements the [platform principles](architecture/platform-principles.md),
[test tiers](testing-tiers.md) and existing product scope in [STATUS](STATUS.md).

> Finish one recognizable, playable behavior end to end. Establish the original
> game's observable behavior from source evidence, trace the dependencies needed
> to reproduce it, and test it through the actual Add-On gameplay path. Classify
> failures correctly: Add-On policy, asset conversion, or a missing reusable
> engine mechanism. Reuse existing capabilities before adding new ones. Validate
> with independent source expectations, meaningful negative controls, and
> integrated state transitions. Deliver a verified playable build, get player
> feedback, and then choose the next slice. Preserve useful findings across
> sessions without turning documentation or isolated tests into substitutes for
> working gameplay. Keep unknowns and deliberate adaptations explicit.

**Observe → establish source behavior → trace dependencies → reproduce the
failure → fix the correct layer → verify integrated behavior → package →
playtest.**

## Working loop

1. **Choose a visible outcome.** Describe what the player should be able to do
   and what would count as failure. Bound the assignment without silently
   dropping dependencies or agreed fidelity. A small slice is an integration
   target, not permission to special-case the engine around one weapon or game.
2. **Reuse evidence and establish expectations.** Identify the original version,
   source locations and relevant calling/state path. Separate proven behavior,
   hypotheses and unknowns. Research the connected assets, controls, animation,
   rendering, effects, audio, physics and authority that can affect the outcome;
   do not reconstruct unrelated internals before making progress. A related port
   or our implementation is not independent proof of the target game's behavior.
3. **Reproduce through the ordinary consumer.** Trace source asset and conversion
   through package binding, state/authority and client presentation as relevant.
   Keep a reproducible failing case. Do not replace ordinary input or admission
   with a privileged test path that bypasses the missing behavior.
4. **Fix the responsible layer.** Repair lost conversion data or Add-On policy
   there; add a generic engine mechanism only for a demonstrated gap. The engine
   owns reusable mechanisms; Add-Ons own game-specific behavior and values.
   Return the capability to its actual consumer before expanding dependent work.
5. **Challenge the result.** Derive expectations independently from source, not
   by reading back metadata we just wrote. Use a wrong-FOV, wrong-binding or
   other meaningful negative control where it distinguishes a plausible wrong
   result. Check consequential state combinations: aim/fire/release, sprint and
   recovery, interruption, stance/selection changes, teardown and host/remote
   agreement where relevant. Retain useful unit and regression tests; isolated
   renders or passing gates establish only the scope they actually exercise.
6. **Deliver and learn.** Use the required test tiers, review an immutable tested
   pin, and package the actual Add-On with matched binaries, load-health evidence
   and concrete instructions. Maxwell performs all interactive playtests; agents
   use headless ordinary controls/network tests and bounded offscreen rendering.
   Record what changed, what remains unknown and any explicitly authorized
   adaptation. A limited preview does not close full source-fidelity acceptance.

## Keep it efficient and durable

Reuse existing research, automation, engine mechanisms and regression coverage.
Maintain one compact consumer evidence/handoff with source references, relevant
dependencies, exact pins, reproducible commands and open gaps. Link existing
receipts instead of generating parallel reports or repeating discovery. Keep
source-game assets and recovered private material out of the public repository.

Consumers may prepare independent work in parallel. Coordinate shared engine
paths centrally, hand off tested immutable pins, and finish the affected consumer
before opening more dependent mechanisms. This guide does not authorize new
work, broaden path grants, resume paused lanes or replace required checks.
Proceed within existing authorization without routine approval requests; surface
real decisions, blockers and useful playable results.

The AA aimed-shot, AK sprint/lowering and weapon-projection work illustrate the
loop. Source calling branches, actor scale and an initially confounded render
comparison exposed assumptions that isolated successes had missed. These are
reasons to keep testing the approach, not proof that every larger integration
problem is solved. Preserve correctness, multiplayer authority, stability and
agreed fidelity while measuring progress by usable integrated behavior.
