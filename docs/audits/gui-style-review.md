# GUI style review — 2026-10-02

This review checks the native screen layouts for visual consistency and
whether each screen explains its next action through short labels, control
names, and compact hints. It supplements the recovered 114 GUI profiles and
68 layouts, the screen/flow and interaction research in
[`docs/research/ui-ux/`](../research/ui-ux/), and the conversion evidence in
[`ui-conversion.md`](../ui-conversion.md). Those sources establish the v20
visual vocabulary and behavior; they are not a reason to treat a screen as
unreviewed when it already has an authored counterpart.

## Review boundary

The native probe rendered 90 frames across its first-impression flow at
1024×768 and 1920×1080 (1× and 2× UI scale). The reviewed and retained set is
`artifacts/ui-style-audit-final/`. I visually inspected representative frames
from the main menu, map selection, Options, Escape Menu, player list, print
selector, and wrench at those sizes, and reviewed the prior event/wrench
frames. This sample does not mean every one of the 90 images received a
separate visual inspection.

The authored offscreen fixture in `artifacts/workshop-ui/` now also covers
Rule Workshop Examples, Explain saved events, Pause, Mini-Game settings,
Mini-Game teams, Admin, Environment Simple and Advanced, and Add-Ons with a
selected package. Those additions were captured at 640×480. Admin uses an
unavailable-administration fixture, so its empty player list and disabled
actions do not represent an active administrator session. Workshop Examples
also has 400×300 and 853×480 captures. This is an offscreen appearance and
layout review; it does not establish action dispatch, readability at every
display size, input feel, or fidelity acceptance by a v20 player. Maxwell
remains responsible for interactive playtests.

## Shared conventions

- Use the original Blockland bitmap button profiles and their supplied
  `button1`/`button2` artwork for clickable actions. Preserve native button
  silhouettes and use the height appropriate to the artwork; avoid introducing
  plain text rectangles beside rounded stock controls.
- Keep dialog title bars, grey panels, black section headers, tab treatment,
  stock bitmap fonts, and compact spacing consistent with the recovered UI.
  Prefer adding a short label or one-line hint when a control needs context.
- Make screens explain themselves through action labels, field labels, and
  brief status text. Do not add instructional paragraphs where a concise hint
  or clearer label will do.
- Adapt layouts around controls that are actually available. Empty rows,
  unsupported settings, clipped text, and overlapping hit targets are review
  findings even when the source layout itself is authentic.
- At each render size, judge the controls at their logical UI dimensions as
  well as their scaled appearance. A bitmap stretched into a mismatched
  rectangle can look out of place even when it remains clickable.

## Findings and changes

| Area | Finding | Disposition |
|---|---|---|
| Options → Graphics | The Apply action used a flat `GuiButtonCtrl`, while Done and the neighboring v20 actions use Blockland bitmap artwork. It was the clearest native-style outlier in the supplied options frames. | Rebuilt Apply with `BlockButtonProfile` and `button1`, preserving its command and horizontal placement at a native 30 px height. |
| Escape Menu | The added Rule Workshop action was 22 px high and touched Options and Player List. | Reflowed it to 38 px with 6 px gaps above and below. Shifted Player List and later actions by 26 px, and grew the centered window from 421 px to 447 px so it fits the 480 px logical canvas with the original 12 px bottom inset. Added a focused overlap and fit assertion. |
| Wrench / Rule Workshop | Existing frames establish stock bitmap controls, dense native sections, and brief field labels as the baseline. The in-progress native wrench edits retain their `BlockButtonProfile` treatment for the event actions and the 185×30 detection action. The comparison field reserves space before its remove button. | Kept the existing wrench changes intact. Its offscreen fixture already accepts `BRI_CONTENT`; the Workshop fixture now accepts the same content-root override. |
| Admin, Environment, Explain, Add-Ons, and Workshop Examples | The first 90-frame probe did not cover these screens. Authored offscreen captures show the stock dialog frames, labels, tabs, bitmap actions, and compact hints in the captured states. Explain contains four short trace lines in this fixture; Add-Ons shows a selected package; Environment shows both tabs. | Added actual-pack fixture renders. No style change was justified by these captured states. The later populated-authority review below covers selected-player SuperAdmin state; additional changed states need their own captures. |

The fixes were rendered and inspected after rebuilding the UI probe. The
Escape Menu and added screen captures are in `artifacts/workshop-ui/`; the
options and wrench captures are in `artifacts/ui-native-dialogs/` and
`artifacts/ui-native-wrench/`. The final Escape Menu frame shows Rule
Workshop at 38 px with 6 px gaps. Options Apply uses the rounded native
artwork at 30 px. The wrench retains its detection action at 185×30 and its
comparison popup clears the X control at both inspected scales. Workshop
captures at 640×480 and 853×480 show the full two-column list; at 400×300 the
single-column list scrolls inside its viewport.

The actual-pack runtime probe rendered 90 frames at 1024×768 and 1920×1080
(1× and 2× at 1920×1080); its console, host/cancel flows, and external-UV
check passed. The authored Options, Workshop, and wrench captures passed with
no missing textures. Added Admin, Environment, Explain, Add-Ons, Pause, and
Mini-Game captures also passed with no missing textures. The Escape Menu
layout assertion verifies the two 6 px gaps and the 447 px centered window
fits inside the 480 px logical canvas. These results close the code/offscreen
findings only. Maxwell's human review of interaction, readability, and
fidelity remains open.

## Suggested verification

From the worktree root, set `BRI_CONTENT` to the local generated content
folder (read-only source installation inputs remain outside this command):

```sh
BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content \
  cargo test -p bri-ui --lib screens::options::tests -- --nocapture
BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content \
  cargo test -p bri-ui --lib screens::workshop::tests::workshop_offscreen -- --ignored --exact
BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content \
  cargo test -p bri-ui --lib screens::menus::tests::escape_workshop_row_has_native_gaps_and_window_fits_canvas -- --exact
BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content \
  cargo test -p bri-ui --lib screens::wrench::tests::authored_wrench_offscreen -- --ignored --exact
BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content \
  cargo test -p bri-ui --lib screens::options::tests::authored_options_save_players_offscreen -- --ignored --exact
```

The ignored offscreen tests do not open a visible game window. The options
suite contains behavioral tests but its frame capture path should be rerun by
the existing native probe after the UI build. Generated snapshots should stay
under `artifacts/`; screenshots and automated renders remain evidence, not a
substitute for Maxwell's interactive acceptance.

## Populated creator and authority review — v0.2.1

The Sol High follow-up adds `artifacts/creator-authority-ui/`: Admin with a
selected remote Builder, authoritative SuperAdmin capabilities, available host
options, and active Kick/Ban/Environment/rank controls, plus Workshop Examples.
Both were rendered at 1024×768 and 1920×1080 with requested UI scales 1×/2×.
The fixtures use the actual preference calculation: requested 2× at 1024×768
becomes 1.6×, keeping the required 640×480 logical canvas; 1920×1080 retains
2× (960×540 logical). An initial forced 512×384 logical capture bypassed this
runtime calculation and clipped Admin; that was a fixture error, corrected
before the final captures. A missing AdminOptions value also correctly disabled
Host Options in the first fixture; supplying the advertised options corrected
the active-state fixture without changing runtime permissions.

The real-pack Wrench fixture now captures Normal, Sound, Vehicle Spawn,
classic Events, guarded Events, region/vector, variable/state and boolean
controls at the same resolutions/scales, retaining its earlier 640×480 and
small-window renders. Representative final frames were visually inspected.
Classic unguarded rows keep Input/Target/Output together; guarded rows place
WHEN and its Delay above IF/AND, then DO. This matches event scheduling:
`Runtime::plan` computes the due time and `Runtime::execute` queries conditions
at execution. Send/Cancel remain outside the scrolling rows. Stock native
buttons, named team/color/boolean pickers and labeled axes require no restyle.
The populated Admin renders fit the supported canvas and preserve visible
authority-gated actions. These are appearance/control-state checks; they do
not establish human interaction acceptance.

All nine Workshop recipes were read: switch, team door, cooperative ordered
puzzle, checkpoint race, uncontested hill, credited kill scoring, soccer,
charged/visit/timer state, and a package-defined route switch. Conditions serve
actual team, identity, progress or score gates; the preview uses Team = Blue
rather than an Alive-on-activation example. Soccer authors two identical Steel
Ball spawners and filters scoring by `SpawnedBy`, with distinct generated batch
names. Ordinary-control selection excludes bot holes, links, reflections,
special bricks and vehicle-spawn bricks. Generated catalogs identify real Zombie
and Shark hole bricks with their nonempty bot IDs, so the exclusion has an
actual content basis as well as the existing synthetic regression.

One physical recipe defect was sent to root: classic `Door panel` does not
enter the tall-panel selection/lift branch, which only matches `Gate panel`.
Its intended panel therefore starts as the ordinary one-plate control brick.
Team door/puzzle use the tall branch. The actual base catalog includes 4×4F
([4,4], one plate), 1×4×5 Window ([4,1], 15 plates) and 1×4×4 Print
([4,1], 12 plates), plus plain 1×4×5 ([1,4], 15 plates); the selected tall
candidate is not a bot. A source/content identity inspection is not a full
physical-door playtest; root owns the recipe repair and regression.

The first-impressions draft-refresh correction is already present:
`asynchronous_listings_preserve_valid_and_partial_typed_team_names` increments
MiniGames revision before applying the second listing, preserving both a valid
name and temporarily empty text. The read-only permission fixture also
increments revision. The regression was rerun; no redundant patch was made.

Validation after the final fixture changes:

- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-ui --lib offscreen -- --include-ignored --nocapture`: 6 passed, real native pack captures, no missing textures.
- `cargo test -p bri-ui --lib --test minigame_screens --test admin_screens`: 209 passed, 8 ignored; covers creator drafts, authority dispatch and native screen controls.
- `cargo clippy -p bri-ui --all-targets -- -D warnings`: passed.
- `git diff --check` on the touched UI paths: passed.

No visible game window or operating-system input was used. Screenshots remain
local ignored artifacts. Maxwell's creator usability/fidelity playtest remains
required; this review does not claim every screen state was rendered.
