# Next release execution checklist

This is the working checklist for the
[detailed scope proposal](2026-10-30-connected-sessions-bots-and-lighting.md).
Maxwell approved autonomous implementation and publication as v0.4.3 on October 30.
The detailed plan owns behavior
and acceptance; this file owns sequencing and execution state. Root is the integrator.

## Readiness checked

- [x] Current origin/main inspected at `9390c39cf63874ac22b8470541666b1f208ef891`.
- [x] Required Rust 1.93.1 Windows MSVC toolchain, cargo, Python, PowerShell 7 and git respond; sccache responds and is configured as rustc-wrapper.
- [x] CARGO_TARGET_DIR is unset and the default pre-push hook is present.
- [x] Disk availability checked: approximately 1,149 GiB free on C: and 10,203 GiB on E: at inspection.
- [x] Original source ZIPs, reference installs, five saves, five recent recordings and relevant logs are available.
- [x] Twenty-nine private evidence files copied and SHA256-verified, totaling 136.4 MiB, under `E:/Downloads/BlocklandReImagined-Release-Evidence/planning-2026-10-30`; `INPUTS.json` records their identities. These original assets and user recordings must never enter Git or public release payloads.
- [x] Global/dead-player voice, loading audience, subtle material style and the precise ModTer hammer report are captured in the plan.
- [x] Receive approval of the detailed plan and its publication conditions.
- [x] Prepare the canonical implementation checkout `../BlocklandReImagined-worktrees/v043-integration` on `codex/v043-integration` from current main and carry the planning files into it. No suitable idle merged private warm checkout was available.

No currently registered checkout is both an available private warm workspace and
appropriate to reuse: the primary is historical/dirty; the shared `.bri-gate`
checkout/cache is reserved; `voip-review` contains unmerged work; and the clean
`user-docs-audit` checkout is a separate registered docs workspace outside the
prescribed worktree root. Leave all of those untouched. If still necessary at
implementation start, create one canonical worktree and keep its outputs warm.
No compilation, link, SDK check or full gate is claimed by the tool version checks.

## Execution order and dependencies

| Step | Work | Dependencies and exit evidence |
| --- | --- | --- |
| 1 | Establish diagnostic baselines for accumulating movement delay and the five bot worlds. Finish the bounded later-core-fix register. | Preserve the current recorded refusal failure and trace its onset. Capture actual body, package and route state before changing shared motion. If Maxwell's local symptom still needs a live capture, enable a lightweight capture in the first preview and continue independent work. |
| 2 | Fix supported movement admission/prediction causes; reproduce the ModTer hammer flicker and death blocker, and investigate issue 400. | Before/after evidence must challenge the actual failure. Record remaining local-feel uncertainty separately from the recorded guest refusal. These investigations establish the movement/death contracts stacking must preserve. |
| 3 | Deliver small independent corrections: spawn rotation, issue 412 music gain, issue 414 dependency/catalog checking, and the requested audio gains/sword swing binding. | Use the supplied original inputs and existing recipes/operations; no manually repaired Bot_Town as source. Can proceed alongside unresolved live movement capture. Ship a private preview after changed-area checks and package startup. |
| 4 | Loading voice/chat, Global live/dead conversation, and creator departure with continued encounter management. | Shared connection/membership/resource contracts, not UI-only fixes. Actual accepted-loading and gameplay QUIC clients; source ownership remains independent of participation. |
| 5 | Navigation, evidence-driven search, stuck recovery and the useful bot event crosswalk. | Five saved worlds retain distinct route/acquisition/clearance controls. Fix within existing navigation first. The explicit tracking event does not become hidden omniscience for normal bots. |
| 6 | Physical zombie stacking. | Depends on supported living-body contacts, death/support revocation and ordinary movement; cube attack, moving/dead supports and observer agreement. Recheck indoor routes independently. |
| 7 | Destroyed-brick light/emitter following and authoritative spawner lifecycle. | Reuse debris identity and frames; check fake kill, fragment multiplicity, fade/respawn, invisible-brick distinction and pickups. Independent drops retain their lifetime. |
| 8 | Lit overlays, BadDream animated lighting, issue 415 behavior/control parity and subtle material detail across applicable world families. | Reuse native renderer/settings/sandbox. Original v21 reference and custom shader evidence remain separate. Small reviewed gallery before broad material rollout; measure relevant budgets. |
| 9 | Combined acceptance and final release. | All agreed items accounted for, Maxwell's combined playtest accepted, required exact-source CI/history/protocol/extended release checks passed, actual three-platform archives independently verified. Publish immutable tag/assets and verify public digests. |

These are dependency boundaries, not a requirement to stop all progress while one
consumer is waiting for evidence. Root may reorder independent corrections using
the evidence. No additional agents are assumed; delegation requires an explicit
applicable instruction and assigned paths/model requirements.

## Rules for each increment

1. Pin the source and content inputs; identify the existing seam owner. Keep one
   concise current failure/acceptance record for the affected experience.
2. Reproduce the defect or state precisely what remains unknown. Test supported
   competing causes before adding a new mechanism. Do not increase arbitrary
   timers, remove authority checks or add periodic resets to conceal a failure.
3. Implement through the existing authoritative/native path. Version changed
   schemas and protocol representations; preserve original assets and generated
   versus manual-override separation.
4. Run formatting, warning-free changed-crate Clippy, meaningful unit/direct-consumer
   and relevant integration checks. Affected content gets actual startup validation.
   Record failed first runs and flaky retries; do not relabel scoped checks as full.
5. Build the private folder with `--profile playtest`, verify packaged startup and
   hand Maxwell concise scenario instructions. No ZIP/LTO/full-gate prerequisite
   for that preview. Keep its source/content version fixed during acceptance.
6. Record technical checks, source comparison and Maxwell's acceptance separately.
   Mark an item complete only against its defined behavior. Merge with required CI
   and guarded history/protocol checks; keep the pre-push hook enabled.

Do not run local GPU suites, full gates, soaks or benchmarks during Maxwell's
playtests. Preserve warm outputs. Expensive long-run validation is deliberate
for the movement reproduction and final release, rather than repeated after
ordinary tuning changes.

## Scope decisions and release authority

The execution baseline includes issues 400, 412, 413, 414 and 415, the five bot
cases, all session/minigame/rendering requests, both audio gain requests, the
long-run movement report and destroyed-brick attachments/spawners. Newly arriving
reports get explicit triage; they do not silently expand or reduce the baseline.
This is a release workload agreement, not a project-wide feature freeze.

Routine engineering decisions, evidence-backed pivots and private preview builds
proceed autonomously once approved. Return to Maxwell for a material acceptance
scope change, an unresolved product choice or subjective preview acceptance.
Do not ask repeatedly for implementation details already covered by the plan.

Plan approval authorizes publication only after the detailed plan's combined
acceptance and release conditions pass. A hard-to-reproduce bug is not closed by
a synthetic pass; unexplained failures hold the affected acceptance or require an
explicit scope decision. No fixed release date or final version number is promised
before the evidence supports them; Maxwell has now selected v0.4.3 as the release version.
