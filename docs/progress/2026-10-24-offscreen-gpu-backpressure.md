# 2026-10-24 Bound offscreen comparison work on software GPUs

PR304's first Windows run (40070538755) rejected the new viewer-region ambient
regression: GPU submission wait timed out before pixel comparison, including
its isolated retry. Other shards passed. The local hardware gate had passed
at production commit 558792d5d.

The test frame loop submitted more GPU work while host/pipelines settled,
without waiting for completed frames. Bound each test frame with a submission
fence using the existing 60-second wait limit. The ambient/fog fixture also
disables sun-shadow atlases, which it does not exercise. This is test-only:
production rendering and the delivered playtest binaries are unchanged.

The same three lighting modes, exact pixel equality, independent actor-light
assertion, fog negative control and exit recovery remain required. Temporarily
restoring the old production ambient override still fails pixel equality
(45 changed pixels in Classic). Correct production behavior passes on forced
DX12 Microsoft Basic Render Driver (CPU/WARP), 69.59 seconds. The existing
water lifecycle case using the shared frame helper also passes there, 60.20
seconds. The temporary adapter selector is removed before committing; normal
adapter selection remains in the retained test.

Focused client checks pass (77 seconds): formatting, all-target strict clippy,
and 668 unit tests, including both retained offscreen cases. The full gate and
fresh Windows PR run follow this commit; their receipts remain separate.

Private retained evidence: bounded-render-negative.log,
bounded-render-software.log, bounded-water-software.log and the archived probe
diff under interdiction-jail-report-20261023. This harness correction does not
establish additional gameplay fidelity or replace Maxwell's playtest.
