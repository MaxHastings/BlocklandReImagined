# 2026-11-04 Zombie tower pursuit

Maxwell reported that all three zombies see the player at the saved spawn in
ZombiesCantGetMeBeta16CityTower, but stare for several seconds and only one
successfully pursues upstairs. His latest clarification explicitly uses the
save's player spawn; the screenshot camera is not a required input.

## Reproduction and causes

The private native save SHA256 is
`468c390587b333fbd93acc506139fa95265457d0a679dc2404615afc4728b37b`.
It contains 8,528 bricks on Bedroom. The existing headless bot watch loads the
saved minigame, installed Add-On choices and normal spawn; host movement is
zero ordinary input. No route, actor transform or bot decision is injected.
Original files remain unchanged outside Git.

Baseline traces show target visibility on the first recorded tick for all
three zombies. Their first walking inputs arrive at ticks 1,564/1,598/1,598
of the recorded interval (about 13 seconds at 120 Hz), after searches retain
over 50,000 nodes each. This is planning delay, not slow perception.

The other failure is purpose arbitration: return-home can beat search after
personality/boredom terms, even with a valid route and a recent observation.
One captured return occurs while the host is still alive, so target death
does not explain every abandonment. The first zombie later killing the host
does correctly end the remaining pursuits; traces distinguish those cases.

## Correction

- Existing general route planning remains cost-based. Cross-floor chase/search
  can prioritize admitted elevation progress to find a feasible approach sooner.
  This trades minimum traversal cost for response time; it is not a shortest-
  path guarantee. Every directed walk, jump, crawl and other movement edge still
  comes from the existing body/collision graph.
- The existing partial-progress rule only recognized distant horizontal
  progress. A whole body's progress toward the target floor now counts too.
  Partial routes explicitly distinguish a limited search from an exhausted
  graph. A budget is not a disconnection proof or permission to breach bricks.
- Pursuit continues after useful limited segments. Unproductive detours keep
  their frontier. Expansion windows resume at their original size; per-tick
  sampling, expansion and arc limits are unchanged. Search/retained-state caps
  remain bounded.
- Idle return-home is ineligible while an enabled valid pursuit/search has
  work. Add-On behavior weights, dated evidence, dead targets, anchored intents
  and exhausted-route handling remain authoritative inputs.

## Evidence and remaining checks

The incremental-window run starts walking at ticks 249/142/141 (about
2.1/1.2/1.2 seconds), versus the baseline's roughly 13 seconds. All three reach
the target floor, reacquire the live host and enter combat before its death.
The response is improved, not instantaneous. This is headless technical
evidence; Maxwell's integrated playtest acceptance is still outstanding.

`cargo test --locked -p bri-sim --lib`: 520 passed, 5 ignored. Coverage includes
elevation ordering with ascent/descent and an unreachable floor under normal
work limits; forward/backward stair progress; and limited/exhausted segment
continuation, preserving the old exhausted-route expectations.

`bot_watch::saved_tower_zombies_pursue_the_stationary_host` is an explicit
external-save regression. It requires all three original zombies, pursuit
before each initial observation's expiry, reacquisition on the live host's
floor, and no failed bot steps. It uses BRI_CONTENT, BRI_WATCH_SAVE,
BRI_WATCH_CHOICES, BRI_WATCH_OUT, BRI_WATCH_SECONDS=60, BRI_WATCH_WARMUP=0 and
BRI_WATCH_TICKS=1. It passes on the private save without changing its rules or
giving the host immunity. `cargo test --locked -p bri-sim --test nav_invalidation`
passes its case; `cargo test --locked -p bri-chaos --test bot_navigation_spike`
passes all eight cases. Changed sim/chaos all-target Clippy with `-D warnings`,
`python tools/check_format.py` and `python tools/check_docs.py` pass. The final
sim unit run after distinguishing ordinary limited progress in diagnostics
again passes 520 cases with 5 ignored. Platform CI and merge remain pending.

Rejected experiments remain in private logs, not production: goal-distance
priority alone and goal-side reverse-walk probes did not solve the delay.
Reverse probes were too restrictive around directed step/jump edges and added
work. One prototype also exposed an expansion-accounting underflow, corrected
during that experiment; that entire prototype was then removed. No retry-only
success is being represented as a clean pass of unchanged code.

Private captures/logs are under
E:/Downloads/BlocklandReImagined-Release-Evidence/zombie-tower-2026-11-04.
Three unrelated enabled choices are absent from this content root:
blockhead_footsteps_f, mario and mario-rules; the watch reports them. Required
bot_hole, bot_zombie and blockhead_bot packages are present. This is not a full
content-fidelity claim or a completed alpha/release handoff.
