# 2026-10-28 v0.4.1 integration and release coordination

Maxwell explicitly requested orchestration of the four active working chats,
merging their completed work into main, then publishing v0.4.1. The coordinator
owns merge order and publication; each chat retains its isolated implementation
and scoped verification. Maxwell confirmed he is not playtesting and authorized
the heavier release checks. Interactive gameplay remains his responsibility.

The actual active scopes, despite older chat titles, are:

- Voice policy/routing, optional radio samples, speaker HUD and 50% received
  voice gain: `codex/voice-policy` in `plastic-destruction-playtest`.
- Collapse breakup recovery after temporary physics capacity pressure:
  `codex/two-hit-collapse-interaction` in `voip-review`, PR #392. Its separate
  unfinished build-speed experiments are excluded from this release.
- Gunner world heading independent of turning hull: `codex/tank-world-aim`
  in `tank-world-aim`.
- Human-only ordinary player/admin views, retaining bot team assignment:
  `codex/human-player-lists-isolated` in `human-player-lists`.

Integration reuses `footstep-turning-f`, whose clean prior branch was already
merged into origin/main, on `codex/v041-release`. No new build checkout was
created. The main checkout is detached on historical source with existing local
changes and untracked notes; preserve it. The release process must operate from
the coordinator checkout and use explicit content paths as needed, rather than
checking out another commit over those local changes.

Starting main: `f891e5aa8`. Published v0.4.0 source:
`37611e924d7a39ac1a38536d0491937de8aa8935`, verified through the GitHub release.
The existing STATUS release paragraph is older; refresh it after verified
v0.4.1 publication. Existing published tags/assets remain immutable.

Pending: all four scoped handoffs, combined conflict/authority review,
history/protocol validation, matching platform CI, current generated content
and imported bundle validation, final extended release gate, platform packages,
independent archive/startup verification, tag and public release verification.
This entry records authorization and setup, not passing checks or publication.
