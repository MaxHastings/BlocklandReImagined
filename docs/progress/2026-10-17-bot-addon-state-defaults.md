# 2026-10-17 Bot Add-On state defaults

Maxwell reported repeated Player Throwing `allow_equip -> holding` errors in
v0.3.3 (`826396023`) after spawning/using bots. The saved report at
`%LOCALAPPDATA%/BlocklandReImagined/logs/add-on-health.json` identifies the
argument as `()`. This correction is on `codex/player-throwing-state`, based on
origin/main `82639602`, in the prescribed worktrees directory.

Player Throwing already declares `holding` and `holder` defaults of -1. Human
joins initialize these in `packages_joined`; bots skip that method to suppress
connection hooks. Later mini-game membership initializes defaults, leaving
free-build bots without their declared state. The equip policy then reads an
unset held-player ID and logs the reported error while allowing the command.

The shared join path now initializes bot defaults through the existing bounded,
idempotent state initializer before default mini-game entry. Bots still skip
human connection hooks. No package-specific engine policy, rule/settings changes,
schema/protocol changes or reimport are required for the fixed engine.

The new regression imports the CC0 Player Throwing stand-in through the real
importer, creates a free-build spawn-brick bot, and exercises the authoritative
equip command. The existing generic hook test now distinguishes human on_join
state from the bot's untouched declared default, while retaining its no-bot-hooks
assertions. Existing hosted coverage checks grabbing, escape timing, charged
throws and equip vetoes; carry coverage includes death cleanup and carrying bots.

Validation:

- Initial fixture attempt failed because its plate was off the stud grid;
  corrected only the synthetic fixture position.
- The corrected pre-fix regression failed with the exact reported diagnostic,
  `expected an id, got () (line 58, position 13)`, exit 101.
  Evidence: `artifacts/throwing-before.log`.
- After correction: `cargo test -p bri-addon-import --test ports player_throwing`
  passes 3/0, and `cargo test -p bri-sim --test carry_rules` passes 7/0.
  Evidence: `artifacts/throwing-after.log`, `artifacts/carry-rules.log`.
- `python tools/check.py quick --crates bri-sim,bri-addon-import` passes all four
  steps in 219 s: formatting, importer 58/0 and simulation 406/0 (4 existing
  ignored). Evidence: `artifacts/quick.log`.
- `python tools/check.py focused --crates bri-sim,bri-addon-import --depth 1`
  passes all 10 steps in 2063 s, including both all-target Clippy checks with
  warnings denied, importer/chaos/net/sim/stress-lab tests and client unit tests.
  Aggregate result rows: 2435 passed, 0 failed, 251 existing ignored. Client:
  601 passed, 0 failed, 76 existing ignored. Evidence: `artifacts/focused.log`.

Cargo used jobs=2, CARGO_PROFILE_DEV_DEBUG=0 and CARGO_TARGET_DIR unset; tier runs
used RUST_TEST_THREADS=8. The initial dependency build was restarted to bound
Cargo jobs. Separate package feature variants account for much of the first-build
cost. No visible game or interactive input; original installations are untouched.

The other three health entries remain separate unresolved content issues:
Fizzer's `fizzerPop` explosion and Butterfly Knife's `ButterflyknifeFireSound`
sound and `$DamageType::butterflyknifeRadius` damage-type references.

Next integration: land the committed fix through the normal integrated gate and
include it in a verified build. This source correction does not update Maxwell's
installed executable or claim interactive acceptance, publication or completion
of the full alpha contract. No full integration gate was run on this branch.

Cleanup: automatic approval review rejected direct recursive target deletion as
"blocked by policy". Used the repository's guarded `tools/clean_targets.py` from
the main checkout instead, with every other worktree explicitly kept. Reviewed
its dry run, verified the worktree inventory was unchanged, and applied that
same plan with --recent 0 after all builds finished. It removed only this lane's
34.1 GiB target; main/other targets and the logs outside target were retained.
Evidence: `artifacts/cleanup-plan.json`, `artifacts/cleanup.log`.
