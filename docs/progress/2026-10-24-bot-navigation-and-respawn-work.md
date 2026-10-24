# 2026-10-24 Bot navigation and respawn policy latency candidate

The assembled Interdiction encounter exposed 323 ms tick peaks and multi-second
network replies. Repeated floor posture placement now shares exact hit-height
results within each body grid, including failed placements. Existing collision
invalidation clears affected columns. The existing navigation budgets remain.

Profiling also found repeated navigation-suicide policy calls constructing live
script snapshots. Route failure and closed rooms must not kill bots, per Maxwell.
Remove that timer and action; retain the retired chooser slot at zero score to
preserve deterministic indices. Ordinary death and explicit respawns remain.
Dead bots now wait for authoritative respawn holds before polling policy. The
held-bot regression failed with 141 calls before and passes with zero while held,
then one successful request after release.

A prototype additionally capped posture attempts globally. Although its quick
checks passed, focused checks exposed seven integration failures. Discard that
new cap and restore stable chooser indices. Do not waive those failures. The
simplified candidate passed the targeted Slayer soccer case; its remaining
integration rerun was interrupted for immediate delivery at Maxwell's request.

Prototype two-client measurements improved sustained multi-second replies to
52 ms control replies and 87 ms movement acknowledgements, but retained a
127 ms startup hitch and seven dropped ticks. Those measurements predate the
final simplification and do not certify this candidate. Logs and prototype
sources remain archived outside disposable worktrees in
interdiction-ui-lighting-20261024/server-lag-report.

Maxwell explicitly requested skipping remaining tests and delivering a warm
client-only playtest build now. This is an unmerged private candidate, with
normal integration gates still pending. Preserve the existing mission content
and companion tool source pins; the updated client also contains its local
host server. No interactive playtest acceptance is claimed.