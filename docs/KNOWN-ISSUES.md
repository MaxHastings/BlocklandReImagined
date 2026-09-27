# First building playtest — known limitations

This deliberately early playtest follows Maxwell's decision to focus on core
building. The full vanilla alpha remains unfinished.

- **Combat, vehicles and minigames:** incomplete and not acceptance targets for
  this build. Some converted assets or menus exist without complete gameplay.
  Weapon debris and new minigame UI work are not evidence of working combat.
- **Wrench/events:** the implemented brick-event subset works; many vanilla
  Player/Client/MiniGame/vehicle/bot event outputs and special brick behaviors
  still lack adapters. Unsupported imported records are retained/reported, not
  executed. Vehicle/music/special item workflows may reject explicitly.
- **World fidelity:** original architecture, resources and environment adapters
  are present, but lighting/shadows/materials, water effects, animation and
  movement timing are not final. Current terrain uses a finite loaded region;
  do not use long-distance travel as evidence of finished streaming. Opus's
  handoff supplies data/conversion/collision components, but no renderer or runtime
  integration. This release uses map-bundle-014 and the existing finite path;
  the unintegrated map-bundle-015 is not included.
- **Large saves/performance:** large imports, mesh rebuilds and checkpoints can
  stall. Begin with a small build. The current renderer has explicit geometry
  limits; streaming, local prediction and remote interpolation are unfinished.
- **Multiplayer setup:** direct IP requires importing the host's public
  certificate with the provided helper. LAN discovery and join passwords are
  unfinished. Leave the join password blank. Certificates change on host restart.
- **Ownership/admin:** host authority and basic moderation are implemented;
  complete trust management and all Admin/SuperAdmin actions are not. Native
  saved keys are local identities, not Blockland accounts or legacy BL_IDs.
  Old ownership/credentials must not be assumed to survive every host restart.
- **Building fidelity:** exact legacy ghost anchoring/re-centering and some tool
  timings still need comparison. Undo currently focuses on planting, not every
  paint/print/effect operation. Macros and other unfinished controls may reject.
- **Audio:** automated mixing/bindings were checked with silent output. Actual
  balance, timing and device behavior require Maxwell's listening test.
- **Platform:** this handoff is Windows. It is not a verified macOS/Linux build.

Disabled or explicitly rejected unfinished features are expected. Crashes,
missing content, unusable basic controls, disappearing saves, or failures on the
core building path should be reported as blockers, not dismissed as normal WIP.
