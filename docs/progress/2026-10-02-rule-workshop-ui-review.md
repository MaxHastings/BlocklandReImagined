# 2026-10-02 Rule Workshop GUI usability pass

Continues `rewrite/rule-workshop`, an experimental design spike. Maxwell asked
for all related GUIs to be understandable through their layout and controls,
with little explanatory prose. No runtime architecture or compatibility contract
was added. Main remains separate. The shared primary checkout changed to main
during this review, so the existing branch continues in the isolated worktree
`../BlocklandReImagined-worktrees/rule-workshop-ui`; no changes were discarded.

## What changed

- Pause has Rule Workshop, opening an nine-example picker with short gameplay
  descriptions. It calls the existing rulelab command, prevents double-submit,
  and shows pending/errors. Chat is an optional shortcut. The picker scrolls in
  small windows rather than squeezing every example on screen.
- Events keeps familiar input/delay/target/output rows. Explicit column and
  parameter labels replace blank fields and provider/debug annotations. Input
  selection defaults to Self where available. Optional IF controls, copy/remove
  and row separators reveal complexity when needed; long rows scroll. Compact
  windows wrap target/output onto a second line. Maxwell pointed out that IF
  below the action visually suggests the wrong execution order. With guards,
  the layout now reads WHEN, IF/AND, then DO and its parameters; unguarded-only
  lists keep the compact classic row. Send/Cancel remain outside the
  scrolling body. No permanent explanation footer was added.
- IF checks filter by subject and target class. Booleans use Yes/No; teams use
  their names; object kinds use native vehicle/ball choices; colors have palette
  selection and a swatch. Variable name/value have labels. Region size and
  velocity have labeled axes instead of one unlabelled vector string. New IF
  starts at Self Exists = Yes; the eight-condition cap is visible as disabled
  Add IF. Existing unavailable rows are still preserved.
- MiniGame settings exposes Teams & Add-Ons without requiring a package to
  declare team settings. Native team editing/player assignment uses existing
  requests and per-game host permissions. Package conditions that hide teams for
  particular modes remain honored. Team controls have Name/Color captions and
  adapt to short/narrow windows, including separated footer rows and preserved
  drafts on resize. Player team pickers no longer overlap Remove buttons.
- Explain saved opens a small read-only results window. It filters fresh,
  brick-tagged responses from the existing bounded trace, with Close, Back to
  game and Refresh. Condition values use plain Yes/No/number text, unavailable
  context and pass/skipped; actions show ran/waiting/rejected. No provenance,
  debugger or new network schema was introduced. It reads saved rules, not an
  unsent draft. Error results stay visible. Old/unrelated chat is excluded.
- The Windows package now includes the creator test card and rewrites both
  local guide links to their shipped filenames.
- The playtest guide/design notes describe these controls and the authoring
  defaults. The GUI has concise captions, not a tutorial or general IDE.

## Practical recipe review requested during the GUI pass

Maxwell rejected Alive = Yes as an onActivate teaching example and suggested a
team-controlled door. The native preview now shows Player Team = Blue before a
DO that makes the named door collidable; the boolean preview gates a counter on
MiniGame Round ended = No. Alive remains available for delayed rewards and other
legitimate cases; the menu does not hard-code every input's presumed gameplay.

A ninth editable recipe, Team door, has explicit Open/Close controls and a gate;
the first team may operate both, other teams cannot. It reuses native collision/
rendering and MiniGame team operations. The puzzle now has three distinct
ordered switches, cooperative shared progress and a gate that recloses on Reset,
rather than three clicks ending a round. The state recipe now visibly charges a
launcher, grants a per-player third-visit bounce and pulses a color every five
seconds. Soccer starts with opposing-team checks; own goals reset without points.
The remaining switch/race/KOTH/kill/Add-On examples retain different purposes.
These are authored recipes, not new built-in game modes or interpreter paths.

New focused tests check both teams operating the door, out-of-order/repeated/
cooperative puzzle activations and Reset, shared versus per-player charge,
repeating timer colors without scoring, and credited opposing goal scoring.
The guide records their roles and mutations. Regions still need creator-built
courses/fields/walls; these are practical editable starting points, not maps.

## Verification scope

Used native generated `content/ui-pack-004`, automated UI host-action fixtures,
source layout checks and bounded offscreen GPU renders. No visible game window
was opened; no interactive playtest or mouse/gameplay input was automated.

The broad screen sweep covers entry/setup, pause, MiniGames, Wrench/Events,
selectors, save/load, Add-On settings and the other existing screens across
short/wide, 720p and 1440p scaled layouts. Dedicated populated Workshop renders
cover all Wrench variants, ordinary/guarded/state/boolean/vector rows, Examples,
Teams/player assignment, MiniGame settings, Pause and Explain. Additional native
renders use 400x300 and 853x480 logical sizes. Screenshots remain uncommitted
under `artifacts/ui-native-wrench`, `artifacts/workshop-ui` and
`artifacts/workshop-ui-sweep` in the isolated worktree.

Local validation at the handoff source:

- `cargo test --locked -p bri-events -p bri-ui -p bri-sim -p bri-world --lib --tests`:
  **871 passed, zero failed, 137 ignored**. The 21 focused native rule lab tests
  include the revised practical examples and delayed team/own-goal cases.
- `cargo clippy --locked -p bri-events -p bri-ui -p bri-sim -p bri-client -p bri-world -p bri-net --all-targets -- -D warnings` passed.
- `cargo test --locked -p bri-ui authored_wrench_offscreen -- --ignored` and
  `cargo test --locked -p bri-ui workshop_offscreen -- --ignored` passed with
  the real native pack and no missing textures. Guarded preview geometry asserts
  that input precedes IF, and IF precedes output. Visually inspected the resulting
  normal/guarded/state/boolean/small-window Events, examples, teams, pause,
  MiniGame and Explain screens.
- `BRI_SWEEP_OUT=<worktree>/artifacts/workshop-ui-sweep cargo test --locked -p bri-ui --test screen_sweep -- --ignored --nocapture` passed. Its diagnostics are
  observations, not a claim that existing unrelated screen defects were fixed.
- `git diff --check` passed. Main's unrelated local setup note is untouched.

A fresh artifact-only Windows build follows on this same branch. Human
understandability is still a playtest question; render checks cannot establish
that no creator will ever get confused.

During implementation tests caught an incorrect small-window anchor, loss of
team access when there are no declared settings, boolean/vector field routing,
and guard-cap tests relying on offscreen clicks. Those were corrected. Clippy
also caught a test-module placement issue, corrected before handoff. A broad
baseline sweep reports existing issues in unrelated avatar/options/selector/
loading screens; those are not a signoff for the entire application's UX.

Next evidence: Max authors a blank-brick door without the recipes, adds IF and
state gradually, sets up teams, modifies another creation and uses Explain
without reading internal implementation concepts. The same creator test card
remains the design research instrument; this pass does not claim a completed
rewrite or full alpha signoff.

## Independent first-impressions review

At Maxwell's explicit request, a GPT-6.1 Sol subagent performed a read-only
review of the actual native PNGs and screen/recipe code as a first-time creator.
This request overrides the older Luna-only subagent preference for this review.
The reviewer found the native presentation familiar, the WHEN/IF/DO order clear,
and small-window scrolling reasonable. It identified three concrete P2 issues:

- A listing refresh could erase typed team names/settings before Apply. Typed
  fields now update the draft and remain dirty across asynchronous refreshes,
  including temporarily invalid partial text; player assignment preserves edits.
- Changing Input automatically selected Self without resetting incompatible
  Target guards. Both input and explicit target changes now share normalization.
- Explain used team IDs despite named selections. The same runtime checks now
  format expected/current values with the observed player's real team names;
  missing teams fall back to IDs, and condition evaluation is unchanged.

Regression checks cover server refresh while renaming/clearing a team name,
Input changing a Player/Score guard into a brick target, and a real rejected Red
activation showing `Player Team = Blue (current: Red) - skipped`. The supplied
Explain render is updated to match the real response. The reviewer did not run
an interactive game or classify documented experimental limits as defects.
The final Windows artifact is rebuilt with these fixes rather than handing off
the superseded 03 source build.

Follow-up local validation: the full affected-crate test command now reports
**873 passed, zero failed, 137 ignored**. All-targets warnings-denied clippy
passes again. The first new guard regression fixture omitted its alternate
input; the test catalog was corrected and the full suite rerun, rather than
changing runtime behavior to accommodate the fixture.

A second read-only pass confirmed the three fixes and identified a regression
fixture that needed an explicit listing revision increment; that fixture was
corrected and rerun. The real-pack Explain render also passes after the change.
