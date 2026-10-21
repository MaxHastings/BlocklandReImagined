> **Historical note. Written 2026-09-28 (the file's date in the cloud project folder, `worktree-inventory.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# BlocklandReImagined worktree inventory (2026-09-27, ~23:30 local)

All 76 folders are git worktrees of the main checkout. No stray non-worktree
`BlocklandReImagined-*` folders. Two unrelated loose folders in Games:
`foundations-parked` (5 small .rs/.py scratch files) and `scratch_prev` (PNGs).
Also loose in Games: ~70 `flake-*.log`, `*-gate.log`, `*-msg.txt` files from threads.

Sizes are before this cleanup; most small trees had their target\ cleared earlier.
"On main" = HEAD is an ancestor of origin/main. "Landed as" = same change is on
main under a rebased/squashed commit. Last = time of last HEAD move.

## A. Safe to remove now (merged, clean, idle) — 46 worktrees

| Worktree | Branch | Status | Size GB |
|---|---|---|---|
| a7 | (detached) | on main | 0 |
| brickfx | brickfx | on main, pushed | 0 |
| brickvis | brick-visuals | on main | 0 |
| clearance | mirror-sync | on main | 0 |
| console | console | on main | 0 |
| control | control | on main, pushed | 0 |
| cursor | cursor-capture | on main | 0 |
| diag | split-fix | on main | 0 |
| flyjeep | flying-jeep | on main | 0 |
| foliage | kitchen-foliage | on main | 0 |
| fov | fov | on main | 0 |
| heldbrick | held-brick | on main | 0 |
| items | items | on main (origin/items differs) | 0 |
| jets | jets | on main | 0 |
| mp | multiplayer | on main | 0 |
| perf | perf | on main, pushed | 0 |
| persist | world-persistence | on main | 0 |
| plantrules | (detached) | on main | 22.4 (target cleared) |
| plight | player-light | on main | 0 |
| printer | printer | on main | 0 |
| quirks | torque-quirks | on main | 0 |
| render | render-infra | on main | 0.2 |
| saveload | saveload-hud | on main | 0 |
| spraycan | spray-can-held | on main | 0 |
| spraycolor | spray-color | on main | 0 |
| sweep | bug-sweep | on main | 0 |
| terrain | terrain-streaming | on main | 0 |
| tools | tools | on main, pushed | 0 |
| turret | turret-aim | on main | 0 |
| tutorial | tutorial | on main, pushed | 0 |
| ui | ui | on main | 0 |
| undo | undo | on main | 0 |
| vaudit | vehicles-audit | on main | 0 |
| vbase | (detached) | on main | 0 |
| vehicles | vehicles-axis-fix | on main | 0 |
| vseat | vehicle-seats | on main, pushed | 0 |
| wheel | wheel | on main | 0 |
| worldgen | worldgen | on main | 0 |
| addonimport | multipack-c | landed as 9ef7144/5d024c1/8fd122a | 152.0 (target cleared) |
| breakables | breakables | landed as 33f13a2 | 32.6 (target cleared) |
| loading | loading-progress | landed as 9996bf7 | 26.7 (target cleared) |
| toolrules | tool-rules | landed as 264d7c9 (PR #2) | 21.4 (target cleared) |
| fx | fx | WIP imported as bae2359 | 0 |
| mpwip | modplatform-package-wip | parked; PR #6 closed; pushed to origin | 0 |
| BRI-a10 | (detached) | on main; only untracked feature-tests.log | 2.2 (target cleared) |
| BRI-pr3 | pr3 | on main; only untracked gate-push.log | 1.5 (target cleared) |

Branches of the "landed as" rows are not ancestors of main, so `git branch -d`
refuses them; they stay as local refs unless Max says to drop them.

## B. Keep until the current landing and a13/a14 builds finish

| Worktree | Branch | Why kept | Size GB |
|---|---|---|---|
| .bri-gate/worktree | (detached) | gate runner; test binary running now | 290.5 |
| land | land | verifier's batch (15 commits not on main) | 13.0 |
| pr9 | no-mm-addons | local-only ed60f3d, not pushed | 17.0 |
| doorclosers | claude/project-thread-u0pw8d | door-closers rejoin rebase | 78.4 |
| doorclosers2 | door-closers-avatar | door-closers (on main, but in use) | 22.2 |
| gate | gate-speed | Gate thread; 1 unpushed commit | 40.2 |
| slides | slides | active today 23:02; 2 commits not on main | 9.1 |
| stresslab | stress-lab | holds a11 build Max is playing (dist\...-a11-da5668e-stress-lab) | 148.0 |
| leftovers | horse-render-test2 | on main, but moved 23:09 (likely Cleanup) | 45.7 |
| foundations | engine-foundations | on main, but moved 23:02 | 252.7 |
| glass | land14 | active 22:43; 2 commits not on main | 30.8 |
| pr13 | pr13 | PR #13 landing; 6 commits | 6.7 |
| pr5 | pr5 | PR #5 landing; 6 commits | 22.9 |
| pr10 | pr10 | PR #10 open | 8.4 |
| pr11 | pr11 | PR #11 landing | 1.4 |
| pr12 | pr12 | PR #12 open | 4.8 |
| bootstrap | land8 | active 22:47; 3 import commits not on main | 23.7 |
| BRI-addon | multipack | on main (9ef7144), moved 22:44; untracked gate-push.log | 2.6 |
| BRI-pr1 | pr1 | PR #1 stress campaign; 23 commits not on main | 1.8 |
| BRI-pr2 | pr2-land | on main, moved 22:17; 5 untracked logs | 40.5 |

## C. Has work not on main (idle; needs a decision before removal)

| Worktree | Branch | Work not on main | Size GB |
|---|---|---|---|
| chat | chat-emotes | d8da15c v20 chat colours, admin/join lines, emotes (not pushed) | 0 |
| creature | creature-spike | 777266e creature seams doc + uncommitted Cargo.toml, crates/package/ | 0 |
| docsdrift | docs-drift | 4 commits: docs drift guard, doc-sync rules (not pushed) | 0 |
| parity | v20-parity | 3 commits: v20 eye, 3rd-person camera, keyboard turn speed (not pushed) | 22.5 (target cleared) |
| water | water-player | 2 commits: v20 water splashes, underwater tint (not pushed) | 34.6 (target cleared) |
| verify | modding-guide | 3 commits = PR #7 author guide (open) | 2.7 (target cleared) |
| tuthammer | tutorial-hammer | WIP tutorial probe commit + 2 uncommitted files | 2.0 (target cleared) |
| adminorb | (detached, on main) | merge in progress with conflicts, 30 staged/conflicted files | 0 |
| modplatform | mod-platform (on main) | uncommitted Cargo changes, untracked crates/package/, docs/modding/ | 0.3 (target cleared) |
| visual | visual-compare (on main) | uncommitted building.rs/controls.rs, untracked visual_capture.rs, tools/visual_compare/ | 2.4 (target cleared) |

## Method
`git worktree list --porcelain`; per tree: `merge-base --is-ancestor HEAD origin/main`,
`rev-list --count origin/main..HEAD`, `git cherry`, subject match against main's log,
`status --porcelain`, remote tracking; sizes via robocopy /L. Git on this machine is
2.12 (no `git worktree remove`), so removal = verify clean, delete folder, `git worktree prune`.
