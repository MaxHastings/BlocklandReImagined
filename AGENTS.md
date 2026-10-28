# Blockland ReImagined

## Setup
From a fresh clone, one command checks the toolchain (printing the exact
install command for anything missing), recovers the v20 scripts, generates
every content pack, builds the client and runs `bri-client --check`:

```sh
python tools/bootstrap.py --v20 "/path/to/Blockland v20"
```

The v20 folder holds `base/`, `Add-Ons/` and `saves/` and is only read. Rerun
the same command (the path is remembered) after pulling: it rebuilds only packs
that are missing or whose importer inputs changed. If `--check` names missing
packs, this is the fix. A `content/` folder copied from elsewhere is fine: its
packs are kept and only the missing ones are built. Details and flags are in
`docs/content-regeneration.md`.

## Builds and disk
Every worktree's `target/` grows to 10-200 GB, and dozens of parallel worktrees
filled Maxwell's C: drive (1.5 TB of build output on 2026-09-27). His PC
compiles every cargo build through sccache (`~/.cargo/config.toml` sets
`rustc-wrapper = "sccache"`, capped at 40 GiB in
`%APPDATA%\Mozilla\sccache\config\config`), so a new worktree reuses the
crates.io dependencies other worktrees already compiled. Workspace crates
still compile per worktree. On other machines `bootstrap.py` uses sccache when
it is on PATH (`cargo install sccache --locked`). Check with
`sccache --show-stats`. Leave `CARGO_TARGET_DIR` unset: it is hashed into
every cache key, and a target dir shared between worktrees would make cargo
queue parallel builds and overwrite each worktree's `target/release/bri-client`.

Create worktrees only under `..\BlocklandReImagined-worktrees\<name>`, never
as new folders beside the main checkout, and reuse an idle one (clean, its
branch merged) before adding another; a fresh worktree's first build writes
about 14 GB. Remove a finished worktree once its branch is on main
(`git worktree prune` after deleting the folder; this machine's git has no
`git worktree remove`).

When a thread finishes, delete its worktree's build output (never the main
checkout's, which packaging uses). `python tools/clean_targets.py` is a dry
run listing each finished worktree's `target/` and its size; add `--apply` to
delete, `--keep <folder>` to spare active worktrees. It skips folders a cargo
build has locked or that changed in the last 30 minutes, and only ever deletes
`target/` folders.

## Product contract
Read `docs/STATUS.md` for current priorities and decisions, then
`docs/alpha-contract.md` and `docs/progress.md` (plus newer entries in
`docs/progress/`) before substantial work.
There is no feature freeze (Maxwell's decision, recorded in STATUS.md); the
priority is the release blockers, verification and Windows packaging in its
"Release definition of done". `docs/playtest-contract.md` is kept as history.
The original Blockland v20 experience is the fidelity reference. Original art,
music and sounds retain their identity; modernization primarily improves the
underlying implementation. Preserve the source installation unchanged.

Maxwell designated `E:\Downloads\B4v21Launcher\versions\Blockland v20` as the
vanilla content reference on 2026-09-26. Keep it read-only. See
`docs/vanilla-reference.md` for the verified comparison and map coverage.
The earlier C: installation remains secondary evidence and stress-test input;
its extra packages do not define alpha scope. Its generated mission-lighting
caches are explicitly secondary derived inputs, absent from the new reference.

## Platform principles
Mod-ready foundations now, mod platform later. Read
`docs/architecture/platform-principles.md` before changing content identity,
save formats, the wire protocol, permissions or packaging. The engine owns
mechanisms; packages own policy. No backward compatibility or migrations during
alpha; schemas freeze at the first beta. Open door-closers and their priorities
are in `docs/audits/platform-door-closers.md`. Before building an engine seam,
check `docs/architecture/seams.md` for an owner and add a row if there is none.

## User's testing boundary
Maxwell performs ALL interactive playtests. Do not move the user's mouse,
send gameplay keystrokes, click menus, or automate an interactive play session.
Use code, builds, headless tests, logs and offscreen rendering behind the scenes.
A bounded screenshot/render check is allowed when needed. Do not launch a
visible game window merely for routine validation. Hand off a packaged build
with concrete test instructions when the agreed alpha is ready.

## Recreating source-game experiences
Treat each game as an interconnected experience. Establish source-backed behavior
and dependencies for the current playable slice: assets, animation, state changes,
controls, rendering/effects/audio, physics, ballistics and gameplay. Trace defects
through the complete relevant chain and assign the cause to Add-On policy,
conversion, or a generic engine capability before fixing symptoms. Reuse existing
mechanisms and seam owners; avoid guessed offsets, unrelated actions and duplicate
systems. Keep original evidence separate from related-build evidence or inference.
Never silently omit or approximate behavior, or claim fidelity without evidence.

Maintain one concise, current dependency/acceptance handoff per experience, linking
reusable evidence, exact inputs, unresolved questions and next supported correction.
Automate repetitive extraction and checks; reuse prior findings across sessions and
agents. Investigate only as far as needed to implement the next supported correction;
carry unresolved dependencies explicitly and continue independent useful work.
Validate ordinary integrated gameplay and important state combinations/transitions.
Tests must challenge assumptions and retained mismatches; passing isolated tests or
plausible captures do not establish source fidelity. Distinguish technical checks,
visual inspection, original-reference comparison and Maxwell's playtest acceptance.
Favor cohesive playable increments, minimal durable notes and meaningful progress
over repeated inventories, low-value tests, fragmented probes or routine approvals.

## Engineering
Earlier technical choices are working assumptions, not permanent commitments.
Maxwell explicitly authorizes evidence-based pivots without repeated permission.
Explain meaningful changes and tradeoffs; preserve the product contract and
testing boundary. Do not use this latitude to silently reduce acceptance scope.
Keep Torque readers/conversion tooling outside the runtime game dependency graph.
Keep generated content and manual overrides separate. Never commit original
game content, decompiled originals, downloaded tools or the research clones.
Use explicit versioned content/save schemas, stable authored IDs, and independent
physics/render identities. World state owns the game; adapters derive views.
Do not implement structural fracture/collapse or a general TorqueScript VM.

Bots know affordances, not content. Games expose objectives, not bot scripts.
Discover success through authoritative rules/provider semantics, compose only
grounded capabilities, execute ordinary controls, and replan from observed
outcomes. Content names never stand in for mechanics; unknown semantics remain
explicitly unsupported. See `docs/audits/objective-driven-integration.md`.

Record meaningful decisions, evidence, commands, failures and next work as a
new file in `docs/progress/` (one file per entry; see its README).
`docs/progress.md` holds the history before 2026-10-01. Check off acceptance items only with evidence. Keep the
active goal incomplete until the full alpha contract and handoff are satisfied.

## Fast iteration and playtest handoff
Use the existing warm worktree and `--profile playtest` for private previews.
Build and hand off the playable folder after changed-area checks and packaged
startup verification. Do not hold a private playtest for merge-only checks,
whole-workspace suites, ZIP creation, or release LTO. Finish the requested build
before starting expensive validation. Do not run local GPU suites, full gates,
soaks, or benchmarks while Maxwell is playtesting; they distort performance.

On Maxwell's PC, target at most five seconds for the first map/server load in
a fresh process and three seconds for repeated loads, measured from selection
to playable control. Log phase timings and reuse verified unchanged content;
retain correct invalidation and content identity checks. These are acceptance
targets, not a claim that current loading already meets them.

## Practical testing and merge policy
Maxwell replaced the mandatory whole-workspace merge gate on 2026-10-28.
Default to the smallest checks that challenge the changed behavior:
- Format and warning-free clippy for changed crates, their unit tests, and the
  relevant integration tests/direct consumers. Use `tools/check.py quick` or
  `focused`, with explicit targets where needed. Keep warm build outputs.
- Check affected content/packaging with `bri-client --check` and validate any
  changed schemas, permissions, wire/save formats or resource boundaries.
- Require passing platform CI and history/protocol checks before merging.
  Record the actual commands and scope; do not call scoped checks a full pass.

Reserve exhaustive map/render matrices, asset corpora, long fuzz/soak/endurance
suites and performance campaigns for nightly/manual/release validation or work
that directly needs them. Use the extended/full gate deliberately for releases
and high-risk shared physics, wire/save or authority changes; explain its need.
Do not start it for every commit or ordinary cosmetic/content tuning PR.

Aggressively retire duplicate/obsolete cases and empty test harnesses. Add a
test for an uncovered regression or a real contract, not to mirror implementation
or inflate counts. Preserve meaningful coverage while consolidating overlapping
suites. Never weaken assertions, hide a new failure, or add arbitrary skips just
to get green. Keep slow validation available separately from the default path.
See `docs/testing-tiers.md` for commands and suite ownership.

## Before you push
Keep the pre-push hook enabled; never use `--no-verify`. Main updates must include
current origin/main, preserve recent work and keep protocol VERSION monotonic
(each wire change adds a file under `crates/net/protocol-changes/`). Retain the
history/stale-tree checks and guarded push support in `tools/gate.py`.
Ordinary merges use the scoped checks above and passing platform CI. The full
workspace/ignored-content/performance gate is an explicit extended check, not a
routine requirement for every main update. Report pre-existing failures and
fix or assign them; a test that passes only on retry is reported as flaky.
Keep original installations and other threads' dirty work untouched.

## Current collaboration boundary
Maxwell requires GPT-6.1 Sol with High reasoning for all subagent work
(latest instruction 2026-10-03). This supersedes the earlier Luna preference.
Earlier GPT-6 Astra subagents were interrupted; do not resume them.
Root owns shared integration, root manifests/lockfile and existing engine
crates. New agents must remain inside their assigned paths; do not spawn further
agents without root coordination. Opus delivered a partial terrain handoff:
data/conversion/collision components, without renderer or runtime integration.
Those components are preserved; the first building playtest ships the verified
finite map-bundle path. Future terrain integration needs separate verification.
Root remains the active integrator;
the interrupted Astra subagents are separate from root. All agents
must obey the testing boundary and leave original installations unchanged.
