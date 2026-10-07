# Mini-game Setup and Teams work before Create

Max: "I don't like I have to create the mini game before I can press things
like Setup." In the Create Mini-Game window, Setup and Teams (our buttons for
the running Add-Ons' settings, such as Slayer's) were greyed out until the
game existed, because they opened the Add-On Settings window for a game id.
v20 had no such buttons: Slayer brought its own window.

Now, before Create:
- Setup and Teams open the Add-On Settings window as a draft for the new
  game. Reset, End, Save & Reset, Tell players and Players are hidden: there
  is no game to reset or end and nobody to assign.
- Save keeps the draft (`Core::minigame_addon_draft`) and returns to the
  Create window. Nothing goes to the host yet.
- Create makes the game. Once the host lists it as the player's to edit,
  `Core::send_minigame_draft` sends the settings that differ from their
  defaults and the draft's teams, with a reset (as Save & Reset), once.
- Closing the Create window without creating drops the draft.

Reset and End stay off before Create, as in v20.

Evidence:
- New tests in crates/ui/tests/minigame_screens.rs:
  `setup_and_teams_work_before_create_and_create_sends_them` and
  `a_cancelled_create_forgets_its_setup_draft`. All 35 mini-game screen tests
  pass.
- `cargo clippy -p bri-ui --tests -D warnings` clean.
- `cargo test -p bri-ui`: everything passes except the six `_synthetic`
  offscreen render tests, which need a GPU adapter the cloud container lacks
  ("no wgpu adapter"). They are untouched by this change.
- Not checked visually: the real window needs the v20 UI pack and a GPU. A
  screenshot on the PC is still owed before calling it done.

## Review fix (host-only settings)

Review of dd0a4b3 failed one item: before Create nothing was greyed, so a
guest could set a host-only Slayer setting in Setup and the host would refuse
the whole draft after Create. The client now also publishes
`addon_locked_new`, the settings the local player could not change in a game
of their own (the same rule as `addon_locked`, with them as creator). Setup
greys those before Create, and the draft never sends one, even from a loaded
favourite. A Setup draft is also dropped on leaving the server.

New test `a_guests_setup_draft_greys_and_never_sends_host_only_settings`; 36
mini-game screen tests pass; `clippy -p bri-ui -p bri-client --tests -D
warnings` clean; `bri-client` mini-game unit tests pass.
