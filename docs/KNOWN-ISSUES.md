# Alpha playtest — known issues

Crashes, missing content, unusable controls, disappearing saves or broken core
flows are blockers: please report them. The items below are known gaps.

- **Wrench events:** all vanilla inputs and outputs are listed. Not yet
  applied: projectile outputs on delayed rows (immediate `Delete`, `Bounce` and
  `Redirect` work). Rows using them are kept and shown read-only.
- **Terrain:** terrain streams without bounds; distance LOD and detail/bump
  texturing are still missing.
- **Vehicles:** physics is a native adaptation, not Torque-exact; driving feel
  needs your judgment. The tank turret barrel may be oriented incorrectly.
  Vehicle burning/splash emitters and wreck models are not drawn yet.
- **Weapon icons:** kill messages use the base death icons, not each weapon's
  own icon.
- **Bots:** simple steering without path finding; they can get stuck on
  complex builds. They only fight inside the brick owner's minigame.
- **Emotes:** play their sound (alarm) and the sit pose; the floating emote
  images are not drawn yet.
- **Special map behavior:** some map-specific objects are not implemented.
  The Tutorial's triggers and lessons work; it runs single player only.
- **Admin:** host/admin roles, bans and basic moderation work; the complete
  original Admin/SuperAdmin tool set does not.
- **Visuals:** lighting, shadows, some materials, water and sky effects are not
  final.
- **Platform:** Windows only.
