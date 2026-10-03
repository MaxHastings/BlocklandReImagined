# v0.2.1 integration and creator handoff

Root continues the authorized combined release in `fix/v0.2.1-playtest`.
Active subagents are GPT-6.1 Sol High. Original content remains read-only and
Maxwell alone performs interactive gameplay tests. No release/merge is claimed
by this entry; candidate/platform verification remains pending.

The three lanes cover stability/GUI/independent review, measured performance
and inventory tactics, and grounded objective integration. Independent review
found named-target variable-change collateral and pre-grounding fanout costs;
the objective owner added conservative reaction rejection and aggregate limits
before per-target clones. The combat lane and integrator moved supported hand
fire authorization to after movement/frame updates, sharing one per-step budget
and preserving charge holding without premature release.

Root prepared `docs/rule-workshop/V0.2.1-PLAYTEST.md`, added it to all platforms'
existing `tools/package_guides.py` path, corrected the Soccer guide to describe
two identical balls with an exact named-spawner condition, and documented NPC
context and conservative prediction choices in DESIGN. A temporary-directory
copy check passed for all four guides and packaged relative links.

The release must describe limits honestly: the original Windows firefight NaN
crash is not causally fixed, full Shark behavior/collision parity is unfinished,
and generalized ball transport, tool routing and cooperative actor stacking are
not delivered. Ordinary actor support/climbing is physically feasible in tested
controls; autonomous discovery/coordination is a separate mechanism gap. These
findings justify a grounded playable iteration rather than more speculative
architecture. Matching Windows symbols and new macOS/Linux symbol retention
support actionable crash investigation.

Next: finish combined actual-control combat/objective negatives, independent
review, optimized performance, warnings-denied checks and full gate; merge the
reviewed commit into current main; build/verify all three platform artifacts;
publish v0.2.1 only after the candidate meets the release checks. Record exact
commits, runs, artifact hashes and cleanup here when completed.

## Save corpus inputs available on this Mac

Root searched existing local originals without modifying them. Five canonical
cases were available and copied byte-for-byte into
`/tmp/bri-v021-save-corpus-inputs`: Violin (5018 bricks), A.T.C. Fort (6275),
TESTING1231 (expected empty-build refusal), 10,000 Bricks to GOD (10036), and
Afghanistan DM (14091). The compiled candidate `save_corpus` test passed these
five in 5.09 seconds with the normal generated main content. Input source paths
and SHA-256 hashes are recorded in `/tmp/bri-v021-save-corpus-provenance.json`.
The full gate will use these inputs too. **Coverage is 5/23, not 23/23**;
eighteen canonical inputs from the Windows collection are absent here.

An initial sixth input named `cool 5.bls` failed its expected 16599 count.
Inspection established this historical copy's authored Linecount 7947, and the
candidate placed all 7947. It cannot substitute for the canonical Windows file,
so root excluded the wrong-version temporary copy, retaining its provenance
and initial failure log. No original was changed and no test expectation was
weakened. This is an input-version limitation, not evidence of a repaired or
introduced loading regression. Logs: `/tmp/bri-v021-save-corpus.log` (initial),
`/tmp/bri-v021-save-corpus-canonical.log` (five canonical cases passed).
