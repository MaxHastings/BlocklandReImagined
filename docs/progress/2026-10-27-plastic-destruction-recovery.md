# 2026-10-27 Plastic destruction recovery and playtest continuation

Recovered Claude session `session_0138CxzBn7MpBhHhW5GnizvW`, branch
`origin/claude/project-thread-auaahm`, remote tip verified with `git ls-remote`:

- `a29ede5b2`: direct-hit plain-brick shards and cue type.
- `a0b59930b`: plastic debris tuning and complementary-shard checks.
- `f2839d018`: Cracked Bricks options checkbox.
- `0575b63b7`: exact stud-grid partition planner/tests, fragment bookkeeping,
  `Simulation::insert_pieces` and `rests_on_map`; no blast integration.

No later branch commit, merged PR, destruction package or promised Afghanistan
captures was found. Main and the installed footsteps/landing candidate did not
contain these commits. The primary checkout is an older detached snapshot with
pre-existing user changes; it remains untouched.

Created `../BlocklandReImagined-worktrees/plastic-destruction-playtest` on
`codex/plastic-destruction-playtest`, based on installed engine candidate
`07f6897cc`, and cherry-picked the four commits. Maxwell then explicitly
requested the full experience without switches: reverted the checkbox commit
and removed the proposed free-build toggle before continuing implementation.
The user's pasted conversation and drawing are the design evidence; no separate
complete original design note was recovered. Current requirements and explicit
limits are consolidated in [the design](../architecture/brick-destruction.md).

Implementation/verification and the Windows handoff are in progress. No
interactive session, mouse or gameplay input automation is authorized; Maxwell
performs all playtests. No public release or main push is requested by this task.

## Three-way review requested before further scenarios

Maxwell explicitly redirected work to a conversation/code/edge-case review,
using reasoning and code inspection before more real-world runs. The resulting
[review](../audits/brick-destruction-three-way-review.md) records the consolidated
intent, the relevant code chains, 25 concrete gaps and a hard-case coverage
catalog. Scenario probes/captures and packaging remain held. In particular,
ordinary stud connectivity does not preserve material continuity across newly
partitioned cells of one original slab; vertex-count budgets do not bound
footprint raycasts; the global support epoch can starve a pending search;
respawn can refresh collisions per original; event/admin/copy/undo paths bypass
family cleanup; and intact section physics/query cost is not bounded by body
count alone. The first pass is not ready to hand over.

Earlier narrow evidence: the three integrated server tests passed before this
review; an initial client unit run passed 706 tests with 78 ignored, including
the local section checks. These do not close the review, later modifications,
or the pending candidate gate. No new real-content scenarios were launched for
the review. The dormant `destruction_probe` draft is uncompiled, not evidence,
and needs its material-bundle load corrected before it is used later.

The already-running quick tier subsequently exited 0:
`python tools/check.py quick --crates bri-sim,bri-client,bri-net` (610 s,
mostly first-build compilation). Its client run reported 706 passed/78 ignored
and simulation 437 passed/4 ignored. No fresh real-content scenarios or visible
game session were launched for the inspection. This remains narrow unit/format
evidence, not closure of the review or validation of the dormant probe binary.
