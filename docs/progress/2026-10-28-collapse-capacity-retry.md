# 2026-10-28 Collapse breakup after earlier debris

Maxwell recalled a deleted older pirate-ship playtest: a first hit partially
damaged the mast/sail, then a later severing hit produced one strangely falling
chunk with no breakup or player pushing until restoration. A first-hit full
detach behaved normally. He clarified that this was **not his v0.4.0 log**;
the current log must not be presented as reproduction evidence. No exact old
save/build or interactive reproduction is available. Existing published code
already contains the earlier mast/deck/gap corrections.

An independent, CPU-only regression against current main f891e5aa8 demonstrates
one still-present cause of the same interaction loss. Older ordinary debris
owns the available rigid-body slot when a later complete section arrives.
`Section::partition` calls permanent `keep_shared` at zero available slots,
conflating temporary occupancy with deliberate Physics Quality Off. The older
piece expires, but the section never retries breakup or gains physical bodies.
The failing-before log is artifacts/two-hit-collapse-regression.log: the
assertion rejects permanent shared-only state after old debris has expired.

Wait for capacity without changing the section's complete visible material.
If occupancy changes during flight/preparation, retain the completed exact
proxy plan and admit its bodies together when capacity permits. Deliberate
quality Off, intrinsic configured body limits, disconnected-group ceilings and
irreducible proxy limits keep their existing fallback. No increased body/part
budgets, invented bounding hulls, changed authority/respawn, wire/save format,
new effects or game-rule changes.

The first regression now passes: older debris expires, the later landed
section receives a real body within its existing limit, and an ordinary moving
player pusher visibly displaces it. A second regression covers capacity becoming
occupied after planning but before landing, retaining the prepared proxy and
eventually receiving its body. Its initial fixture observed only 0.5 seconds,
before the section reached the floor; the corrected lifetime/observation window
still asserts that landing happens while older debris occupies capacity. The
landing and budget assertions were retained, not weakened.

All 18 section tests pass, including the broad sail/narrow deck, collision-gap,
sleeping-projectile target, coherent 1,500-member mast, delayed delivery and
material-budget cases. Logs: artifacts/two-hit-collapse-sections-final.log and
artifacts/two-hit-collapse-fixed.log. Broader debris checks, scoped clippy,
history/platform CI and a private startup-verified preview remain to complete.
This does not establish that the deleted pirate-ship preview had the same cause.
Maxwell alone performs interactive mast/sail acceptance. No GPU suite, benchmark,
full workspace gate or interactive game automation was run.

The separate future-release-speed helper drafts remain untracked and excluded
from this correction. The other working thread owns voice-policy/HUD work in a
different checkout; no destruction edits overlap.

Follow-up validation: the complete `brick_debris::` CPU unit selection passed
53 tests in68.21s, including its existing continuous-world-edit stress case.
That existing case accounts for most of the elapsed time; it was not a full
workspace/GPU/performance gate. Documentation checks pass75 maintained documents
and six generated guide layouts. Scoped all-target client clippy is running.
Five absent derived lighting-volume files in this worktree's generated content
were copied from the already-verified v0.4.0 base snapshot for preview packaging;
the other base-pack bytes match. Source installations/live playtest are unchanged.
