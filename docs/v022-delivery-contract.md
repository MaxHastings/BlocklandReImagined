# v0.2.2 delivery contract — 2026-10-03

Maxwell authorized an ambitious, thorough release and all-day work, with
integration, cleanup, main consolidation and Windows/macOS/Linux publication.
This contract makes that outcome reviewable. It is not a permanent architecture
or a promise that bots can infer arbitrary scripts. All interactive gameplay
belongs to Maxwell; agents use code, headless tests and bounded offscreen checks.

## What the release delivers

1. **A coherent objective-driven bot pipeline.** Authoritative rules/package
   policy describe desired state; grounded body/item/tool/object/seat affordances
   become actions in the existing bounded planner. Ordinary controls execute;
   actual physical/event/MiniGame results determine progress and repair. No
   content-name/game-name solution handlers or planner mutation backdoors.
2. **Useful breadth through one pipeline.** Supported switch/puzzle and ordered
   checkpoint compositions; exact spawned-object entry using physical contact,
   declared physical hold/release or the object's own controllable ground seat;
   intended enemy elimination with dated evidence and bounded search; typed
   package-owned pickup/carriage/return semantics. These are capabilities, not
   built-in polished versions of named sports or modes.
3. **Repair and creator diagnostics.** Live object/tool/source/rule/game/round/
   team/permission changes, human interference and competing claims invalidate
   assumptions. Existing Explain/bot diagnostics show desired goal, selected
   action/provider, approach/admission/wait phase and precise failure. Unknown
   mechanics, unreachable states and exhausted budgets fail honestly.
4. **Creator menus and workflows.** Compact Wrench rows with visually clear
   guards and consistent buttons; Region and Respawn on one row; grouped/searchable
   vocabulary and preserved unfinished/unavailable edits. Teams reachable
   directly in MiniGames, related settings grouped, useful examples instead of
   demonstration-only conditions or confusing repeated choices.
5. **Host/player polish.** Start Game colorset selection with Default, credited
   Trueno and bounded custom-folder palettes; host-filtered guest Wrench/event
   music through reconnect/map change; guest saving through the existing server
   save path; high-quality JPEG screenshots with PNG option; delayed live
   projectile Delete/Bounce/Redirect/Explode through the existing scheduler;
   longer cosmetic blast debris with unchanged resource limits.
6. **NPC fidelity, stability and performance.** Reviewed Shark body/swim/mouth
   behavior and documented remaining source-fidelity differences; Zombie and
   Blockhead integration regression coverage; ranged-flight/close-blast recovery;
   bounded planning/discovery/claims and sustained active battle measurements.
   Review remaining creature gaps for safe completion, without hiding partial
   ports or claiming the earlier Windows crash causally fixed.
7. **A clean handoff.** Editable recipes and deliberately strange mutation
   tests; important semantics and unsupported cases; harmonized entry docs;
   justified hotspot refactors; exact-source package receipts/credits/symbols;
   all worthwhile work on main and obsolete GitHub branches retired; published
   matching Windows x86-64, Apple-silicon macOS and Linux x86-64 archives.
8. **Duplicator rendering robustness and modern Dynamic lighting.** Fix the
   reported v0.2.1 failure when duplication crosses the 100,000-triangle render
   budget, with bounded rendering and meaningful regression coverage. Preserve
   Classic and Unified appearances; Dynamic shades current geometry with live
   lights and shadows rather than old baked shadow/lightmap masks. Original
   baked data may inform import-time light recovery where source metadata is
   missing. Record recovery limitations, offscreen comparisons and resource
   costs. Both changes are required in v0.2.2, not deferred work.

## Work ownership and definition of done

All remaining workers use GPT-6.1 Sol with High reasoning. Root coordinates
shared changes and serialized builds. The architecture audit comes first:
[npc-pipeline-current.md](audits/npc-pipeline-current.md). Unlinked drafts are
not evidence of implemented behavior.

| Lane | Owner and boundary | Done means |
| --- | --- | --- |
| Objective/action abstraction, physical affordances, costs and performance | Sol High performance; existing objectives/planning plus narrow physical provider and tests | Existing Activate/Region/Touch flows use the separated discovery/action seam before adding transport; contact/hold/control-seat alternatives compose with rule prerequisites; exact identity, real physics/event/winner outcomes and metamorphic negatives pass; shared budgets and sustained active work are measured |
| Enemy evidence/search and package semantics | Sol High NPC foundation; narrow search/combat/package providers, explicit package-runtime seams and owner policy | Intended elimination uses normal navigation/combat and dated/unchecked-space evidence; typed item return uses real pickup/carriage/zone callbacks; no engine mode-name inference; read-only discovery cannot commit state/ops; real completion and unfamiliar package variants pass |
| Independent creator acceptance and adversarial review | Sol High stability; new headless acceptance fixtures, read-only production/GUI review | All fourteen journeys have outcome/negative/variant evidence, not merely action-list assertions; active edits/interference/unknown/budget failures are covered; diagnostic and winner observers are checked independently; a held-out composition works without changing production code |
| Shared integration, canonical observers, UI/content/docs/release | Root; shared brain/lifecycle/authority, manifests/protocol, packaging/main | One selection/claim/preemption/validation lifecycle fits all linked actions; normal participants reach package hooks; canonical effect observations prove winners; existing UI/music/JPEG/projectile/guest-save regressions pass; docs/assets/CI/gate/cleanup/all three archives agree with final source |
| Pharzedia feedback: render-budget crash and Dynamic lighting | Separate GPT-6.1 Sol High thread; isolated client/render worktree, root integration | Real budget-crossing regression no longer terminates ordinary duplication; bounded rendering does not silently lose world content; Dynamic removes runtime baked-mask dependence with live geometry/light/shadow evidence; Classic/Unified regressions and measured resource limits pass; both changes merge into this release |

No worker adds a second brain, event interpreter/executor, physics system,
mode-specific handler or standalone objective scripting language. Cross-lane
API changes are coordinated before editing shared files. Meaningful decisions,
failures and evidence get their own dated progress notes.

## Required acceptance before calling the increment complete

The complete fourteen-case intent is frozen in
[objective-driven-integration.md](audits/objective-driven-integration.md):
ground pushing; cheaper declared hold; elevated/small goal; exact named-spawner
identity with decoys; control-seat delivery; ordered checkpoints; multi-action
puzzle; elimination with lost sight/search; typed carryable return; competition;
object replacement; tool disappearance; active goal/rule/team changes; honest
unsupported/unreachable/budget failures. Applicable positives have paired
name/ID/layout/decor transformations. At least one invented object/tool/vehicle
exposes equivalent mechanics without any production identifier branch.

The final held-out test combines existing mechanics in a task not supplied by
Maxwell. Freeze that fixture after the generic mechanisms are implemented; if
it fails, diagnose the abstraction. It cannot become a new production handler
or an acceptance test weakened to match the code.

Assertions include actual control/movement, relevant physical hold/projectile/
seat state, real admitted input, canonical progress/score and the actual
player/team round outcome. Setup may author worlds/rules normally; after setup
tests cannot teleport actors, reposition bodies, inject event inputs, award
score or manipulate rule state to manufacture successful behavior.

Audit production names/IDs, projected assumptions and observation paths,
mutation boundaries and duplicate executors again after integration. Retain
finite discovery/action/node/depth/candidate/fact/term/byte budgets and fair
planning turns. Benchmark pathological counts and active 12/16-bot city battles;
quiet or deadlocked bots do not count as a performance improvement. Report
hardware/source/content, actual activity and simulation timings separately from
render/network/snapshot costs. Headless Mac results do not establish Windows FPS.

## Publication gate

- Focused tests, affected crate regressions and strict clippy pass; no generated
  original content, downloaded tools or scratch implementation lands in Git.
- Required repository gate, including workspace ignored tests and client content
  startup check, passes on the committed integrated source. PR Windows CI passes
  before consolidation to main; main pushes use the installed gate.
- Updated private content bundle preserves provenance/credits, and each platform
  archive identifies the same release/source and required content. Build symbols
  are retained. Package guides have usable links and concrete human test steps.
- Obsolete branches/worktrees are reviewed, recoverably archived where needed,
  and retired after their useful work is on main. Preserve the main build output
  and unrelated local setup note.
- Prepare and inspect a draft with all three verified platform assets, then
  publish **v0.2.2**. Do not publish an earlier UI-only candidate while objective
  integration is incomplete. No claim of subjective human acceptance is made.

## Explicit boundary

Ambitious means a substantial composable slice with adversarial evidence.
Unknown Add-On effects, arbitrary program understanding, universal physics
reasoning, autonomous cooperative stacks, novel tether routes and unsupported
aircraft/boat handling are not quietly claimed solved. Discoveries may change
the implementation; required acceptance cannot silently shrink. Unreproduced
Windows crash/render/audio issues and incomplete original creature fidelity
remain plainly recorded unless evidence actually closes them.
