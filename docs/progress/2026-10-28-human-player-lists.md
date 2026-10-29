# 2026-10-28 Human player lists, bot team management retained

Maxwell authorized implementation after the player-list investigation. Based on
origin/main f891e5aa8, branch `codex/human-player-lists-isolated`, worktree
`../BlocklandReImagined-worktrees/human-player-lists`.

The client carries the existing host administration bot flag into `PlayerRow`
using the same unique-name association already used for ranks. Ordinary F2 rows,
the window count, console `players` output/count and its fallback player-name
completion exclude bots. The administration UI adapter excludes bot targets;
existing model updates clear selections whose target disappears. Humans without
persistent identity and loading humans remain listed. The raw network roster,
names, simulation, mini-game members and both team-management sections remain
complete, allowing bots to move teams. No new wire/save/content schema or engine
seam, permissions or gameplay behavior changed. No Slayer-name special case.

Regression checks cover mixed bot/human/loading rows and count, stale selection
and disabled voice mute, admin selection removal, console listing, and actual
team assignment actions for both existing members and bots outside the game.

Setup initially found a clean merged worktree (`afghan-tank-navigation`), but
another thread claimed it during setup. The first quick check stopped at that
thread's unformatted vehicle test; no formatting was applied to its file. Our
queued UI test was stopped before it acquired the build lock. Moved only our
nine-file patch into the isolated worktree, reversed that exact patch in the
other tree, and restored its `codex/tank-independent-aim` branch. Its tests/build
were left running and its working files were clean afterward. Initial scripted
edits also exposed a local Python cp1252 default; reversed the encoding error
and verified the diff contains only intended changes before validation.

Validation:

- `python tools/check.py quick --crates bri-ui,bri-client`: pass, both format
  checks plus 747 client and 257 UI unit tests; zero failures, 89 existing
  ignored cases. Fresh isolated compilation dominated the 727-second run.
- `cargo test --locked -p bri-ui --test console --test minigame_screens
  --test field_flow --test admin_screens -- --skip render_offscreen`: pass,
  69 tests, zero failures, five existing ignored content cases, three offscreen
  cases explicitly filtered out. Both new team assignment and console tests
  pass. No offscreen matrix or expanded GPU suite was requested.
- `cargo fmt --package bri-ui --check` after the final team-test edit: pass.
- `python tools/check_docs.py`: pass, 75 maintained documents and six generated
  release-guide layouts.
- `cargo clippy --locked -p bri-ui -p bri-client --all-targets -- -D warnings`:
  pass, no warnings (2m23s).

Logs are ignored `artifacts/human-player-lists-{quick-isolated,integration,
clippy}.log`. No visible game, OS input, interactive acceptance, installed-build
update, merge or publication is claimed. The v0.4.1 coordinator owns integration
and release publication; this lane supplies its scoped PR.
