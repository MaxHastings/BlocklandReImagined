# Blockland ReImagined

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
