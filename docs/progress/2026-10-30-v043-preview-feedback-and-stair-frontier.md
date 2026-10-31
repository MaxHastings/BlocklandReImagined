# Preview feedback and the stair-search frontier

Maxwell tried preview.1, reported it "seems fine so far", asked for continued
autonomous execution and explicitly said he does not need any more playtests.
The plan now supersedes future manual-preview/combined-playtest gates with
integrated automated, source-comparison and offscreen validation. No further
interactive test requests are needed. This does not authorize agent-controlled
interactive gameplay or claim human acceptance of unplayed future features.

Preserved the actual preview session log privately: `session-20261031-035412.log`,
26,415 bytes, SHA256
`40ac8741201b676e3c3d042552ea63d45119fb58ab74c9f9de643f1446f71ebf`.
Its ten movement-health samples show peak pending/acknowledgement trail 16,
maximum sampled pose age 0.051 seconds, reconciliation peak 1.093 units and no
bounded/skipped movement-correction warnings. Presentation error is zero at those
sample instants. This is a short normal-play baseline, not proof that a symptom
previously appearing after minutes or hours can never recur. Preserve the
local-feel uncertainty separately from the confirmed admission regression.

## Static stair reachability

Extended the nonshipping bot probe with an optional bounded static-route audit.
It uses an actual grounded zombie's motor-derived body and the loaded world's
existing `Ground`, `Nav` and `Search`, with no jets, moving-body shortcuts or
engine state changes. It records each soft-limit result while using the existing
`Search::resume` capability. Moving bodies are deliberately excluded, so a
successful geometric route still needs ordinary integrated control verification.

Actual `ZombiesHaveTroubleGettingMeStairCase` audit, at the supplied target:

| Visited nodes | Result | Best endpoint |
| ---: | --- | --- |
| 5,607 | Partial, retained frontier | near Y=286.4 under the target |
| 11,237 | Partial, retained frontier | still near Y=286.4 |
| 16,601 | Partial, retained frontier | near Y=321.0 on the ascent |
| 19,329 | Complete route | near Y=332.2 at the target |

The same graph and body can find the route. Current bot consumption discards the
frontier after the first 6,000-expansion soft limit when the early endpoint makes
horizontal progress, then marks the target unreachable and filters it out of
behavior scoring. Partial progress below a tall target is not proof that its
staircase cannot be reached. This is a supported correction in the existing
planner; it does not justify a replacement navigation system or larger per-tick
budgets.

A cheap authoritative bot-path regression uses a tall wall with a walkable detour
and an intentionally small soft limit. It fails against the original consumer:
the early partial endpoint makes headway but prematurely sets `out_of_reach`.
An initial fixture placed feet exactly on the ground plane and failed spawn
clearance; corrected to the established 0.05-unit spawn lift before obtaining
the meaningful red failure. No acceptance assertion was relaxed.

The correction retains a pedestrian chase/search frontier across
that limit, using existing per-tick budgets and the existing hard node ceiling.
Conservative vehicle segmentation and other objective policies remain on their
existing paths. `cargo clippy --locked --profile playtest -p bri-sim -p bri-net
--all-targets -- -D warnings` passes. The navigation-body regressions pass (8),
as do the direct navigation tests (43, one existing ignored case), navigation
invalidation (1), NPC profiles (7) and replay integration (11). These are scoped
checks, not a release gate.

Actual ordinary-control 2,400-tick runs now kill the target in the staircase save
and reduce target health to 10 in the mattress-house save. Reports are private
`diagnostics/v043-{stairs,mattress}-retained-frontier.json`. No jets or target
damage shortcuts are used. The tower still fails after 7,200 ticks: the graph
can reach its target (static audit completes after 40,458 visited nodes), but
the live pursuit retains side destinations and loses planning work across
behavior changes. That is still under investigation, not a passed case.
`BotThought.search_nodes` and the probe's dated-memory fields are host-only
diagnostics, with no new network representation or player capability.

Genuine unreachable-target persistence, search continuity, natural stair
clearance, house egress and physical stacking remain distinct open work.
