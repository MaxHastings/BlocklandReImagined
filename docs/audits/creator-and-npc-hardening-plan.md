# Creator usability and NPC hardening plan — 2026-10-03

Implementation is active on `codex/v0.2.2-hardening`. v0.2.1 is already
published; v0.2.2 will be a separate release with fresh artifacts. Maxwell requested a cohesive next pass covering Wrench Events,
MiniGame setup, NPC behavior/performance, and incomplete Shark behavior.
All delegated work uses GPT-6.1 Sol High. Maxwell owns interactive playtests.

## Outcome and boundaries

Common building and game-creation tasks should be obvious and quick, and bots
should behave reliably within their supported capabilities under sustained load.
Keep recognizable Blockland controls and artwork, one authoritative event
scheduler, ordinary bot controls, and Add-On-owned gameplay policy. The Rule
Workshop remains an experimental spike; no beta format or universal-AI promise.
Prefer clearer organization and better defaults over explanatory paragraphs.
Do not expand general AI architecture, hide useful expressive capabilities, or
change existing event semantics merely to simplify their presentation.

## Verified friction and remaining gaps

- `EventsModel::input_choices` alphabetically flattens every supported input.
  Slayer registers 18 team-specific variants across three input families;
  those are distinct semantics, not accidental duplicate registration.
- One guarded event sets the wrench's global expanded layout, stacking every
  row. Complexity in one row therefore makes unrelated basic rows taller.
- MiniGame Add-On settings appear before Teams. Every team's details appear
  before Add Team, in the same scrolling viewport. This directly explains
  the reported long trip to a common action. The generated Slayer inventory
  contains 65 game and 54 team setting definitions; conditional visibility
  reduces the displayed count but does not repair the ordering.
- Existing style captures establish appearance and bounds, not task usability.
- Shark model reload works, but the port remains partial: collision dimensions,
  animation, mouth grab, variants/colors and hole behavior need verification.
- City query optimizations helped measured simulation work, but current release
  objective-enabled policy needs longer active-battle measurements. Windows
  hangs and the original mixed-battle NaN crash remain unresolved.

## Work lanes and acceptance

| Lane | Objective and scope | Definition of done |
|---|---|---|
| Wrench Events | Familiar compact basic rows; local disclosure of IF/state; grouped/searchable input/output choices; context-aware properties and named team picks; practical examples. Own `crates/ui/src/models/events.rs`, `screens/wrench.rs`, and associated tests. | A basic door row stays compact even beside guarded rows. Team-only door, exact-spawner goal, delayed puzzle and checkpoints are editable through ordinary controls. Add-On team families no longer flood the first list. Selected/imported/unavailable rows retain identity and intent. Copy/remove/Send/Cancel, focus, typing, scroll and async updates are verified. |
| MiniGame setup | Direct Setup/Teams access; visible Add Team; compact team list with details for the selected team; separate gameplay categories and deeper settings. Own `screens/minigames.rs`, `screens/minigame_addons.rs`, and associated tests. | Creating two named/color teams, setting loadouts, assigning players, adjusting a win/round setting and saving require no traversal of unrelated settings. Edits survive category changes and incoming listings. Locked/read-only settings and dependent mode settings are truthful. No dropped hidden values or silent resets. |
| NPC behavior | Consolidate priority changes, objective resumption, blocked-path recovery, jet/melee approach, weapon switching and useful seat selection using the existing hybrid. Own bot behavior/navigation/objective paths assigned by root. | Sustained physical-control scenarios cover interruption/resumption, unreachable targets with alternatives, close combat, inventory/ammo changes and vehicle occupancy. No fabricated progress, thrashing, permanent failed-action loops or permission bypass. Explain distinguishes blocked, waiting and unsupported. New cases vary geometry, IDs and timing rather than relying on scripted recipes. |
| NPC performance and stability | Profile active 12/16-bot city fights with the actual release policy; distinguish host simulation, planner, client effects/render/audio costs; reduce evidenced hotspots without enlarging search caps. Own benchmark/diagnostic/tactics paths assigned by root. | Paired optimized sustained runs retain comparable real combat activity and report p50/p95/p99/max, active counts and memory. Include mixed weapons, objectives, difficult searches and map reload. Measure retries of unchanged failed plans. Native Windows evidence is collected where feasible; no FPS or crash-fix claim from Mac headless timing. Deterministic correctness tests stay separate from wall-clock benchmarks. |
| Shark completion | Verify original Add-On behavior, then finish bounded missing mechanics: authored collision, swimming/animation, bite/grab lifecycle, colors/variants, hole/spawn/reload behavior. Package owns Shark policy; root integrates any shared actor mechanisms. | Real imported content has correct body and collision, water/land transitions, damage attribution and lifecycle behavior. Grabbed players are released on relevant death/removal/reset/disconnect; no stuck mounts or cross-game damage. Variants and spawner behavior match verified source or have explicit documented deviations. Asset, simulation and offscreen checks pass; changelog distinguishes completed from missing behavior. |
| Independent creator review | Fresh read-only review of combined UX and behavior evidence by GPT-6.1 Sol High, after implementation. | Review starts from creation tasks, not feature lists: basic/team door, duplicate balls/courses, puzzle/delay, two-team setup, loadouts and sustained bots. Every control is justified by a concrete task. Findings are fixed or recorded with impact; human first impressions are not claimed by agent tests. |

The NPC/performance lanes also own the measured four-versus-eight-latch
planning weakness. First attribute grounding/search/retry costs, then improve
generic goal-directed work and failure reuse within a declared finite envelope.
Acceptance includes eight straightforward supported independent switches
composing through real controls without starving combat, plus the existing
four-switch, delay, collateral-effect and invalidation negatives. Do not simply
increase every bot's per-tick limits or introduce recipe-specific handlers.
If evidence forces a scope change, record the actual creator limitation.

## Long-term UI approach

### Review complete creator journeys

Maxwell clarified that the review must consider an unfamiliar human creator,
not an automated agent that already knows commands, labels and screen order.
Review the following goals from a blank setup rather than following recipes:

- Make a button open a door, then restrict it to one team.
- Create a two-team game, assign equipment, and set a score/round ending.
- Make one of two identical balls score in a goal, then copy the contraption.
- Build checkpoints or a small state/delay puzzle, then diagnose one mistake.

At every transition ask: what does the creator think they are changing; what
visible label suggests the next step; can they identify the relevant choice
without knowing its internal name; does the feedback confirm their intent; and
can they safely recover? Inspect populated layouts and menus, not just code.
Record ambiguous wording, competing choices, unrelated scrolling, excessive
visible controls, lost context, hidden prerequisites and destructive mistakes.
Include incomplete setup, wrong selections, backtracking, incoming updates and
returning to an existing creation. Don't prescribe tab or picker count before
the journeys demonstrate why it helps.

Rank friction by blocked common task, likely wrong interpretation, repeated
extra work, then cosmetic polish. Each proposed control must serve a concrete
creator intention. Removal is appropriate for redundant presentation; less-used
power gets coherent local disclosure rather than being discarded. Search must
support gameplay words a newcomer would actually try.

Agent walkthroughs/offscreen inspection are heuristic evidence only. Automated
tests prove behavior and protect fixes; they do not measure human comprehension,
enjoyment or time-to-learn. Record human first-working-result time and confusion
only from Maxwell's actual playtest; never infer those from dispatch counts or
invent a first-time user study. The desired basic-door target is the existing
20–30-second ambition after discovering the controls, not a measured claim.

Use one small shared set of spacing, control-height, section, popup and footer
rules across the touched screens. Root owns shared UI controls and any metadata
changes; lanes must not independently build competing picker systems. Preserve
native fonts/button artwork. The first view contains common tasks; local
disclosure exposes depth as needed. Search complements organization rather than
substituting for it.

Add-On vocabulary and settings should carry enough presentation metadata for
grouping/relevance without hardcoding Slayer names in the UI. Unknown Add-Ons
get a usable generic fallback. Context filtering must retain selected values,
support authoring before a MiniGame exists, and distinguish unavailable context
from permanently invalid choices. Delayed events may make apparently pointless
conditions meaningful; do not blanket-remove them.

Do not automatically replace `onActivate(TeamN)` with a generic input and IF:
legacy input filtering and due-time IF evaluation can differ after team changes.
Any proposed normalization needs explicit semantic evidence and an intentional
decision; presentation grouping can preserve the current runtime identity.

## Sequence and integration ownership

1. Agree a small task matrix and shared presentation rules from the read-only
   findings. Root owns shared controls, manifests/protocol, and integration.
2. First implementation wave: Wrench, MiniGame, NPC consolidation. Maximum three
   worker slots plus root; narrow path ownership and no uncoordinated agents.
3. Profile the combined candidate early. Performance fixes and Shark work use
   freed slots; shared actor/physics changes are integrated by root and must not
   race NPC edits. Re-profile after those changes.
4. Rotate a reviewer independent of each implementation. Check task flows,
   populated/empty/locked states, native/fallback layouts, supported UI scales,
   persistence, permissions and lifecycle failures. Use offscreen renders only.
5. Run the required gate and Windows CI. Hand Maxwell a packaged candidate with
   a short mutation checklist and honest remaining limits. Subjective ease and
   behavior feel require Maxwell's playtest. A subsequent publication uses a
   new version and matching source/content across all three platforms.

### Final consolidation requested by Maxwell

After the lanes integrate, review maintenance hot spots and make bounded,
behavior-preserving cleanups where duplication or tangled ownership has a
demonstrable cost. Do not introduce frameworks or split files merely to reduce
line counts. Review every GitHub branch against main, recover worthwhile
unmerged changes, and discard stale or ambiguous work. Archive the inspected
tips locally before removing refs; never merge an old tree over newer fixes.
Main must contain the final reviewed implementation before publication.

Polish the repository's current entry points and player/creator guides, correct
stale platform and setup requirements, and check local links. Keep historical
progress and research as dated evidence rather than rewriting past results.
Record branch dispositions, maintenance changes, remaining limitations and
release verification in a new progress entry. Publish v0.2.2 only after the
gate, Windows CI and all three platform package/startup checks pass.

## Stop rule

Stop adding architecture when it no longer improves the task matrix. Do not
add generalized cooperative stacking, arbitrary-script inference, a new visual
programming system or a full UI framework in this pass. Evidence-based scope
changes are recorded with their creator impact. Windows crash/hang reports stay
open until there is causal evidence, not merely a passing benchmark.

## Planning review evidence

Three existing GPT-6.1 Sol High workers performed read-only audits of UX,
NPC/Shark behavior, and performance/stability. No builds, gameplay sessions or
source edits were made by these reviews. Root inspected the flattened event
choices, Slayer registrations, globally expanded row layout, MiniGame settings
order and Shark port directly. The new plan and progress record are the only
changes in this planning turn; the pre-existing local setup note is preserved.
