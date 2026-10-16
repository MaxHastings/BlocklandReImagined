# 2026-10-15 Main integration with full fidelity release hold

Maxwell authorized merging the completed v0.3.1 engine/Add-On work into main,
while withholding release publication until the full agreed Add-On fidelity
scope, combined checks and his final cross-game playtest are satisfied. He
explicitly rejected using tracked gaps as parked or waived work. Root sent
that direction to all four chats, retaining their current atomic tasks and
requiring owners/actions/closure evidence for every discrepancy.

Root created `codex/v031-main-integration` in the existing primary checkout
from committed `84da72c82f866ff27cb2c52eee17d7da30681ed2`. Previous primary
HEAD `0c877cb3` and origin/v0.3.1-experimental are ancestors. No idle clean
merged auxiliary checkout was eligible, so no new worktree/build cache was
created. All unrelated untracked notes are preserved; only this explicit
integration documentation will be staged. Active game/engine checkouts and
the delivered Halo preview remain untouched.

Refreshed both remotes. Engine PRs #170/#171 report green Windows checks on
their recorded heads; local shared seam commits still need their integrated
candidate and Windows validation. The previous `ab10374f` gate remains failed
(timeouts, nondeterministic crash test and subsequently corrected chat fixture).
No completed merge or clean full gate is claimed by this entry.

Private PRs #3/#4/#5 contain committed game-specific checkpoints and isolated
research/diagnostic outputs; their dirty working changes are not integration
inputs. Root will audit pinned heads, namespaces/output ownership and parent
coverage before merging. Older private #1/#2 must be checked for ancestry so
their content is preserved rather than blindly reapplied.

The image-origin owner is finishing its small separate slice from `84da72c8`.
Root approved quick for all five touched crates and focused depth zero for
those crates plus explicit origin/pose/actor/net/constructor coverage; one
full integrated candidate will cover the complete graph before main. The
base-loader arm override correction is in scope only for explicit origin:
legacy absent-origin behavior stays unchanged. No skipped suite is presented
as a pass, and broader fidelity acceptance is unaffected by test scheduling.

Root inspected the publishing workflow: releases require a release-tag trigger
or explicit workflow dispatch/publication, rather than an ordinary main push.
No tag, release dispatch/upload or update deployment is authorized before
Maxwell's final approval. The new readiness document records this boundary
and the full per-game completion standard.

Next: audit/merge committed private checkpoints without changing active trees,
resolve integrated validation failures, combine the next committed image-origin
slice and run required gates/Windows checks, then land the eligible main
candidate through gate.py. Continue seat/authority, material and movement/prop
work with consumer revalidation until the fidelity ledgers are actually closed.

## Private main integrated

Verified private repository visibility and audited pinned committed heads against
latest private origin/main. Halo touches addons/halo-* and halo/ plus one root
ignore rule; AA touches aa25/; Source touches source/. Pairwise path intersections
are empty. Halo #3 contains the older private #1/#2 heads by ancestry. Existing
Mario main changes are retained by ordinary merge ancestry.

Marked the three reviewed PRs ready and merged with exact head-match guards,
without deleting active branches or modifying worker checkouts:

- Halo #3 head `2d0126941a2ffcfaee6fce0555add4f3a4ae0f82`, merge
  `f4ce5a235d0e66fccdc3cf85caa375a33aca7c19`.
- AA #4 head `11eb3b385c8128ecd2c421b4b42f55a64ad9cfdd`, merge
  `cbe88752bac5055a2b0bb2c4cbcaebb383f4dd6b`.
- Source #5 head `d0125c7f16cf7e7de2e0779911ceb0a18f4fa4e8`, merge/combined main
  `a579e1c7b82c90418844f4c7bb0421a3fad32aa5`.

These are coherent source/diagnostic checkpoints, not claims of finished game
fidelity. Workers were told to preserve current work, then commit and create
follow-up branches/PRs from updated main without competing main pushes. No
private primary working files (including unrelated untracked Mario notes) were
changed. No release artifact or existing preview was replaced.

Root started an isolated rerun of the two cached timeout binaries from exact
old candidate `ab10374f`, under the gate's real lock, with no build/checkout
change. Diagnostic timeout is bounded at 1,200 seconds per binary; this is not
a full gate or a changed assertion. Session output/log is
`artifacts/gate-timeout-isolated-diagnostic.log`. New candidate gating waits for
that lock and the next atomic image-origin commit.
