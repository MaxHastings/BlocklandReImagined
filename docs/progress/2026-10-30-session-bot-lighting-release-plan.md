# 2026-10-30 Session bot lighting and audio release planning

Maxwell requested audit and a full plan before approving implementation, then
added five v0.4.2 bot save reports, +6 dB hammer/wrench swishes with sword support,
+4 dB RMS indoor/outdoor beds, issue 414 and explicit independent engineering
judgment. The plan is
[connected sessions bots and lighting](../plans/2026-10-30-connected-sessions-bots-and-lighting.md).
Implementation and publication remain pending plan approval.

Read project contracts, platform/seam guidance, test policy and relevant historical
and current progress. The primary checkout is historical `591ff3c72` and dirty;
source inspection used `git show`/`git grep` on origin/main. `git fetch origin main`
advanced its tracking ref from `74c7e62fa` to `9390c39cf`; no checkout, runtime code,
original content, gameplay state or other thread's dirty work was changed.

GitHub connector searches and issue/comment reads established open issues 400,
412 and 413; subsequently read the newly supplied 414. Read original developer
announcements for selected core changes from the shader era through r2033.
The old official Change-Log stops at v20; complete later core-fix classification
remains a Phase 1 deliverable rather than a falsely claimed exhaustive audit.

Confirmed existing global minigame voice choices; both client entry gating and
server gameplay-only peers exclude loading voice. Lobby global chat exists, while
game messages follow member/private delivery. Owner departure ends a minigame and
hole-bot game membership follows the builder. Spawn code omits brick rotation.
Search uses last observation plus four local probes; trapped respawn can issue an
ordinary suicide command. Existing corpse filtering and shape normal/material
support mean the reported defects need chain-specific investigation.

Read supplied Bot_Hole/Zombie source ZIPs in memory with Python zipfile; source
hashes, event contracts and stack/spawn evidence are in the plan. GitHub connector
cannot download arbitrary public ZIP attachments, so Python HTTPS read issue 413's
two archives in memory. Confirmed the TV's five converted surfaces use lightmaps
and the DIF converter skips animated-light records. Neither fact proves the TV's
original illumination law. An initial overly broad JSON text search produced a
large truncated geometry line; the follow-up parsed JSON and emitted only bounded
surface/material summaries.

Located all five saves under the Bedroom hashed save directory. SHA256 and binary
headers were read only, with Python zstandard/msgpack following world/build.rs's
BRI-BUILD header. Initial JSON reading failed because .world.json is a binary
container; the first msgpack read needed strict_map_key=False for numeric keys.
The corrected reads establish map, brick counts and saved minigames, not route
playability or a replay of the reported observation ticks. Exact hashes, positions
and separate route/stack acceptance are in the plan. Attached screenshots were
reviewed as symptoms, without inferring collision clearance from pixels.

Commands included `git status --short`, `git worktree list --porcelain`,
`git show origin/main:<path>`, scoped `git grep`, `rg --files` over installed saves,
`git ls-remote origin refs/heads/main`, and the read-only archive/header scripts.
No builds, tests, GPU captures, audio rendering or interactive sessions ran.
No fixes are claimed. Only this new plan and progress entry were written.

Next: Maxwell reviews the proposed scope/defaults/acceptance and execution contract;
after approval use current warm source, preserve unrelated work and begin finite
reproductions. Full alpha remains open.
