# 2026-10-12 Host and Join stop re-reading all content

Maxwell: after starting the game, clicking Host on Bedroom or Kitchen sits on
"Loading Map" for a long time.

## Evidence

Max's v0.2.10 session logs (`E:\Downloads\BlocklandReImagined-v0.2.10-windows\logs`):

| Session | Host | LOADING MAP took |
|---|---|---|
| 20261012-140537 | Kitchen, first after launch | 23,948 ms |
| 20261012-134137 | Bedroom, first after launch | 27,035 ms |
| 20261012-134137 | Bedroom, second in the same run | 1,467 ms |
| 20261012-034544 | Bedroom, first (a run 20 s earlier warmed the disk cache) | 7,149 ms |
| 20261012-04xxxx | Bedroom/Skylands, repeat | 1,361-1,476 ms |

Every other stage after LOADING MAP took under 0.6 s. So the cost is cold
disk reads on the first Host of a run, not the map itself.

## Cause

`ContentPaths::environment()` (the host's package identity, and the join's
"Checking your game content") ran `hash_dir` over every enabled package on
every Host and Join: about 563 MB in 9,758 files in the v0.2.10 install,
read and SHA-256'd from scratch. Nothing else on the host path reads the
whole content folder.

## Change

`bri-package` keeps each file's SHA-256 with its length and modification
time; a file whose length and time are unchanged is not read again. The
client keeps these in `package-hashes.json` in its state folder, so it holds
across runs. Package identity is unchanged (same bytes hashed the same way;
a unit test checks the cached result equals a full read). Files modified in
the last 5 s are hashed but not kept, so a file rewritten inside its
timestamp resolution is never trusted stale. Download verification still
calls the uncached `hash_dir`. The session log now says
`Content hashes: N packages checked in X ms, Y MB read` on each Host and Join.

## Not measured yet

The cloud box has no content, so the new first-Host time is not measured.
The first Host after this build still reads everything once to fill the
cache; from the second run on, it should only list files. The log line above
shows what remains; if LOADING MAP is still slow with ~0 MB read, the next
suspects are the map bundle and world reads on a cold disk.
