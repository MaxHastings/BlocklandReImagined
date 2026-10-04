# 2026-10-03 Sol MiniGame creator journey

Implemented the MiniGame lane of `docs/audits/creator-and-npc-hardening-plan.md`
on `codex/v0.2.2-hardening`, using GPT-6.1 Sol High. Root owns shared Core/View
changes and integration. No interactive gameplay, commits, manifests, lockfiles
or original content changes were made in this lane.

## Creator flow and decisions

- The stock MiniGame screen now exposes **Setup** and **Teams** directly.
  Teams opens the existing Add-On editor at its Teams view. Root added a
  transient Core destination flag; it is consumed on opening, with no protocol
  or saved-data field.
- The editor has Setup, Teams and Players views. Add Team stays in the fixed
  header. A compact team selector shows only the selected team's details;
  adding a team selects it. Team settings and gameplay settings use authored
  Add-On/category metadata, with a generic fallback for empty categories.
  Dependency controllers remain visible before the selected settings category.
  There is no Slayer-name classifier or catch-all Advanced section.
- Team name, color, equipment and deeper authored categories remain editable.
  Players has its own short assignment list. Assignments use authoritative
  team IDs; newly created teams must be saved before players can be assigned.
  An inline message explains that prerequisite.
- The existing footer actions remain, with Apply relabeled Save and Apply &
  Reset relabeled Save & Reset. Favorites, reset/end, tell-player preference,
  permission checks and asynchronous request handling retain their semantics.
  The small layout has a scrollable detail viewport and a fixed action footer.
- Raw text is retained by stable setting key and draft team identity when
  switching views/categories/teams, resizing or receiving listings. Partial
  numbers and empty names remain visible when returning; Save validates all
  retained text before updating the typed draft. Removing a team shifts its
  successors' retained fields together. Hidden mode values are preserved.
  Focus/cursor are restored when the same field remains visible and editable.
- An adjacent pre-existing stock-editor defect was exposed by the journey:
  an incoming listing replaced unfinished rules/loadout while visiting Teams.
  The stock screen now separates permission refresh from initial form load,
  preserving dirty rules until the active game identity changes. Losing
  permission disables actions without replacing the unfinished text.

## Evidence

`cargo test -p bri-ui --test minigame_screens --locked -- --include-ignored --nocapture`
passed **23 tests**, 0 failures, including the ignored offscreen render test.
The final run reported 0.88 seconds after a 3.38-second warm compile.

The tests include a blank two-team journey (names, Gun/Bow equipment, score
limit and coherent Save), direct player assignment, incomplete text and
backtracking, hidden mode values, incoming listings, permission loss, focused
text, removal identity, stock partial rules/loadout, no-Add-On teams and
Add Team reachability despite 100 unrelated settings and deep detail scroll.
Existing server, favorites, invite, reset/end and read-only regressions pass.

The native content pack rendered 12 offscreen captures to ignored
`artifacts/v022-minigame-ui/`: Setup/Teams/Players at 400x300 1x, 1024x768 1x,
1024x768 requested 2x and 1920x1080 2x. The small view retains fixed page and
Save/Cancel actions, with detail scrolling where necessary. Inspected native
captures have readable labels and no footer overlap. These captures use a
representative unfamiliar Add-On fixture, not a fully populated live Slayer
session.

An initial expanded run passed 21 tests and failed two fixture assertions:
new teams correctly serialize the existing default Team Lives reset as None,
and a static hint uses Control text rather than a state override. The tests
were corrected to inspect the complete canonical draft and effective text;
no production behavior or expected workload was weakened.

Scoped clippy initially found one owned nested-if lint and an independent
shared View identical-branch lint. The owned field-capture conditional was
collapsed; root was notified to repair its View branch without changing
popup semantics. Final clippy result is recorded below after that shared fix.

`git diff --check` on the three owned source/test paths passed. Formatting is
limited to those files (`rustfmt --edition 2024`), not workspace-wide.

## Limits and next verification

Automated journey dispatch proves state preservation and actionable controls;
agent/offscreen inspection is heuristic evidence, not measured newcomer
comprehension or completion time. Maxwell still owns interactive acceptance.
The stock rules form and the Add-On form remain separate existing save flows;
visiting the latter preserves the former's unfinished fields. New team creation
and subsequent player assignment remain two authoritative operations. Full
combined gate, populated live Add-On review and platform packaging belong to
root integration.

Final `cargo clippy -p bri-ui --test minigame_screens --locked -- -D warnings`
passed after the owned conditional and root's shared View repair (6.16-second
warm check). Cargo slot released to the Wrench lane. MiniGame owned source and
regressions are frozen for independent integration review.

Integration review reopened two bounded presentation details: the title is now
**MiniGame Settings: …** on every page and favorite write is **Store**, keeping
all IDs/actions. Actual generated Slayer inventory inspection then found that
its broad Team category places sorting/limits/locks before its loadout. Root
preserved Item/PlayerType semantic kinds in the transient UI API and client
bridge; the MiniGame screen uses the existing popup controls for them and
stably promotes body/loadout choices within the selected team category. Local
prerequisite controls retain priority, and hidden values/draft identity remain
unchanged. A new unfamiliar-provider regression proves their ordering and Save
values. The offscreen test now additionally reads the actual generated Slayer
metadata (65 game, 54 team and four server definitions) for Setup and Teams at
400x300/1024x768. These final expanded checks/captures are pending below; earlier
23-test/native-fixture evidence remains a checkpoint, not a final freeze claim.

The first latest-source UI retry was blocked before running this lane's tests
by the independent colorset edit in screens/menus.rs (missing named helper at
line 395). Stability was notified; no MiniGame expectations were weakened.
Final 24-test metadata/capture checks remain pending that shared source repair.

Final metadata retry passed **24 tests** including ignored offscreen rendering
(1.33 seconds, 0.82-second warm compile). One new semantic-order fixture had
omitted its ordinary Add Team action; adding the real creation step repaired
the setup while retaining loadout ordering and coherent Save assertions.
Generated Slayer metadata contains 65 game and 54 team settings, and retains
its mode-dependent team visibility. Four additional native captures now show
actual Setup and Team Deathmatch Teams at 400x300/1024x768, for 16 total captures.
Inspected all four: common player type and five equipment slots precede Lives
and Auto Sort at 1024, while the small view keeps scrolling details and fixed
navigation/Add Team/Save/Cancel controls. MiniGame Settings and Store wording
are visible. Exact ignored paths:
`artifacts/v022-minigame-ui/Slayer-{Setup,Teams}-{400x300,1024x768}-1x.png`.
UI scoped clippy initially waited for the user's colorset-chooser source checkpoint.
this lane's MiniGame source is stable for independent integration review.

The independent latest shared-UI batch also passed MiniGame **24/24** and
field-flow **8/8** with ignored captures included:
`cargo test --locked -p bri-ui --test field_flow --test minigame_screens -- --include-ignored`
(`/tmp/bri-v022-sol-colorset-content-integration.log`).
`cargo clippy --locked -p bri-ui --all-targets -- -D warnings` passed in 8.91s
(`/tmp/bri-v022-sol-colorset-ui-clippy.log`). MiniGame sources are frozen; root
continues independent Colorsets composition review.
