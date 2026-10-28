# 2026-10-28 Destruction lifecycle and total-cost review

Maxwell requested a fresh conversation/diff/edge-case cross-check before
another playtest, using reading and reasoning rather than another scenario
suite. He then designated shared lifecycle consistency and total pipeline cost
as the two issues to address before merging.

Reviewed destruction branch HEAD c9c6d3f64 against merge base
b927779dc2aa9b05552f0c2509fc0a1b5c3f0096: 48 changed files, 5,528 additions,
212 deletions. Read authoritative damage/support scheduling, common Simulation
mutation/family hooks, events/respawn/save/tool paths, wire cues/replication,
client section admission/contact/planning/proxy construction, local physics,
projectile presentation/hit paths and rendering/budgets. Reused existing
preview receipts and locally inspected video evidence from the prior pass.

The current prioritized report is
[destruction-lifecycle-and-budget-review.md](../audits/destruction-lifecycle-and-budget-review.md).
It distills the latest design, distinguishes corrected bugs from current gaps,
maps costs across the pipeline and enumerates 60 edge cases. The original
three-way audit remains historical rather than a list of newly rediscovered
unfixed bugs.

Main findings: group membership/counts do not supply an explicit shared
commit/cancel lifecycle; client motion and hit targets depend on collision
preparation; body/search counters exclude several potentially large bursts.
Current whole-group visual-cap rejection is not a satisfactory overload policy.
The displayed projectile path also differs from the raw view used for rubble
hits, and birth/terminal sweeps are absent.

The supplied Claude conversation and recovered commits are available evidence.
No complete independent Claude transcript was recovered. Accessible material in
the related Review gameplay value chat was inspected; opaque assistant content
references were not treated as readable transcript evidence.

Commands were read-only git status/log/merge-base/diff and rg/Get-Content source
inspection, followed by creation of the audit and this entry. No production code,
executable, package or PR state changed. No new tests, scenarios, benchmarks or
builds were run. Existing preview.3 feedback remains tentative and the full
integration gate remains incomplete.

Next correction: define one authoritative section lifecycle at the shared
mutation/query boundary and resource admission before destructive commitment;
budget preparation, finalization, transport and presentation separately and
fairly. Integrate motion/hit corrections through that representation. This
entry records the review and implementation priorities, not completion of
those fixes or permission to merge.
