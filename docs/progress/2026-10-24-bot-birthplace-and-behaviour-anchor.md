# 2026-10-24 Separate bot birthplace from behaviour anchor

The private invasion consumer exposed a real lifecycle defect before delivery:
a killed follower returned at the living target's exact position, despite the
mission supplying distinct approach spawns. The retained negative is
invasion-spawn-diagnostic-12.log in external server-lag-report. This is production
behaviour, separate from earlier contact-fixture assumptions.

Session::spawn_choice asks bot_home before package on_pick_spawn. Bots::home
accepted any tethered brain, and a Follow or Guard intent is tethered. Follow
updates that home to the target every tick. The moving behaviour anchor thus
bypassed the mission's spawn hook. This was neither a source coordinate error
nor a need to reconstruct another game engine.

Native birthplaces now come only from an actual spawn brick's authoritative
position. Rules-added bots continue through the ordinary game spawn policy.
Idle homes, pursuit anchors, leashes and navigation remain unchanged. This also
prevents a brick follower's next life from inheriting its leader's position.

The focused native regression forces actual new lives under Follow and Guard
and observes their accepted positions at the member spawn. It passes after the
correction; warm dev compilation took 58.74 seconds. The same private negative
now passes through the assembled mode in 71.82 seconds
(invasion-spawn-after-13.log): unchanged inputs return the actor to its approach,
rather than its follow target. The full invasion lifecycle/population comparison subsequently passes in
148.76 seconds (invasion-lifecycle-after-spawn-fix-14.log). Required broader
integration checks remain pending. No updated client delivery or main merge
is claimed yet. The existing ServerRecovery package remains unchanged.

The retained corrected private consumer subsequently passes four functional
checks together in invasion-functional-candidate-24.log (250 seconds), including
authored spawn return and original relay-driven zombie life/population. Scoped
quick and required focused validation pass; the latter takes 935 seconds with
compilation. Full gate/main integration remain separate.
