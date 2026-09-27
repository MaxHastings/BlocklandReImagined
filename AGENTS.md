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
filled Maxwell's C: drive (1.5 TB of build output on 2026-09-27). Compile
through sccache so worktrees share compiled dependencies: `bootstrap.py` uses it
whenever `sccache` is on PATH, capped by `SCCACHE_CACHE_SIZE` (default 40G).
Do not share one `CARGO_TARGET_DIR` between worktrees: cargo locks it for a
whole build, so parallel threads would queue, and each worktree's
`target/release/bri-client` would overwrite the others'.

When a thread finishes, delete its worktree's build output (never the main
checkout's, which packaging uses). `python tools/clean_targets.py` is a dry
run listing each finished worktree's `target/` and its size; add `--apply` to
delete, `--keep <folder>` to spare active worktrees. It skips folders a cargo
build has locked or that changed in the last 30 minutes, and only ever deletes
`target/` folders.

## Product contract
Read `docs/alpha-contract.md` and `docs/progress.md` before substantial work.
Maxwell's latest priority is the first core-building playtest; read
`docs/playtest-contract.md`. Freeze feature expansion and focus on its release
blockers, coherent current changes, verification and Windows packaging.
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
are in `docs/audits/platform-door-closers.md`.

## User's testing boundary
Maxwell performs ALL interactive playtests. Do not move the user's mouse,
send gameplay keystrokes, click menus, or automate an interactive play session.
Use code, builds, headless tests, logs and offscreen rendering behind the scenes.
A bounded screenshot/render check is allowed when needed. Do not launch a
visible game window merely for routine validation. Hand off a packaged build
with concrete test instructions when the agreed alpha is ready.

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

Record meaningful decisions, evidence, commands, failures and next work in
`docs/progress.md`. Check off acceptance items only with evidence. Keep the
active goal incomplete until the full alpha contract and handoff are satisfied.

## Current collaboration boundary
Maxwell requires GPT-6 Luna for all subagent work going forward (latest instruction
2026-09-26). Earlier GPT-6 Astra subagents were interrupted; do not resume them.
Root owns shared integration, root manifests/lockfile and existing engine
crates. New agents must remain inside their assigned paths; do not spawn further
agents without root coordination. Opus delivered a partial terrain handoff:
data/conversion/collision components, without renderer or runtime integration.
Those components are preserved; the first building playtest ships the verified
finite map-bundle-014 path. Future terrain integration needs separate verification.
Root remains the active integrator;
the interrupted Astra subagents are separate from root. All agents
must obey the testing boundary and leave original installations unchanged.
