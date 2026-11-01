# Release-profile bot stress first pass

With an isolated copy of the hybrid Windows package's content and the prior
Afghan scenario's enabled Add-Ons, `python tools/gate.py --perf-only` ran both
32-bot, 3,600-tick release workloads. Afghan averaged 4.88 ms against 5.50 ms
(p99 9.2 ms). Crazy Collage Madness averaged 7.68 ms against 7.00 ms (p99
19.3 ms), so the performance gate failed. A separate same-workload diagnostic
run averaged 7.21 ms (p99 15.81 ms); it also exceeded the unchanged budget.
These are not the earlier 5.741 ms playtest-profile Afghan result Maxwell
accepted, and they are not a full-gate pass.

The second collage run reported 5.40 ms/tick bot thinking, 348,126 floor
samples and no bot-step errors. Tested avoiding redundant edge-support queries
when a sampled floor was already present; the nav unit suite passed 47 cases,
but its release collage run averaged 7.12 ms (p99 15.60 ms). That small change
does not establish a material improvement amid observed run variation and
still fails the gate, so it was removed. The unchanged 7.00 ms budget is not
being waived or labeled passed. Private full outputs for the latter two runs
are `diagnostics/collage-release-recheck.log` and
`diagnostics/collage-release-no-duplicate.log`. The initial command's visible
summary and tail are in the session tool output; its full output was not saved.

Further release validation may run independently, but this measured performance
gap and its potential gameplay tradeoffs remain open. A later pass alone after
an unchanged retry would be reported as flaky, not accepted as proof of a fix.
