# Alpha playtest — known issues

Crashes, missing content, unusable controls, disappearing saves or broken core
flows are blockers: please report them. The items below are known gaps.

- **Wrench events:** all vanilla inputs and outputs are listed. Not yet
  applied: projectile outputs on delayed rows (immediate `Delete`, `Bounce` and
  `Redirect` work). Rows using them are kept and shown read-only.
- **Terrain:** terrain streams without bounds; distance LOD and detail/bump
  texturing are still missing.
- **Vehicles:** physics is a native adaptation, not Torque-exact; driving feel
  needs your judgment.
- **Bots:** simple steering without path finding; they can get stuck on
  complex builds. They only fight inside the brick owner's minigame.
- **Special map behavior:** some map-specific objects are not implemented.
  The Tutorial's triggers and lessons work; it runs single player only.
- **Admin:** the Admin menu (kick, ban, unban, clear bricks, change map,
  server settings), the wand, the F7/F8 camera and v20's admin chat commands
  work. Every v20 feature still missing, from menus to chat commands, is
  listed in `FEATURES.md`.
- **Visuals:** lighting, shadows, some materials, water and sky effects are not
  final.
- **Platform:** Windows only.
