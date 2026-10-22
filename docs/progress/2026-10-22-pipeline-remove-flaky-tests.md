# 2026-10-22 Remove the two flaky Pipeline integration gate cases

The integrated candidate `d4c763867e6b931854db60bb624fd0d949758734`
passed the local gate, but its concurrent run failed two tests that passed
when automatically retried alone. Maxwell explicitly requested removing
flaky tests, then authorized merging without further approval and treating
older user notes as historical context.

Removed `net/crossover_soak::soak_crossover_four_players_and_sixteen_mixed_bots`
with its private scaffolding (the entire test target), and
`package-runtime/hardening_sandbox::many_small_allocations_are_bounded`.
The latter combined a small-allocation workload with a wall-clock limit.
Other sandbox budget/limit cases and the remaining networking tests remain.
This deliberately removes these checks rather than hiding their failures
with ignores or retry rules; it does not establish either behavior's fidelity.

No runtime code or content changed. The prior Pipeline content check and
offscreen input evidence still describe the same runtime implementation.
`python tools/gate.py --push` is the integration path for the updated
candidate; its exact outcome and logs belong in the final integration receipt.
No extra interactive testing or permission pause is required. Full Pipeline
objective/round-transition fidelity and Maxwell's interactive acceptance remain
open work after this source integration.
