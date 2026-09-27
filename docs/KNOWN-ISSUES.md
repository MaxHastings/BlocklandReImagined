# Alpha playtest — known issues

Crashes, missing content, unusable controls, disappearing saves or broken core
flows are blockers: please report them. The items below are known gaps.

- **Wrench events:** a subset of outputs runs (brick color/FX/render/collision,
  relays and similar). Many Player/Client/MiniGame/Vehicle/Bot outputs still
  show as unsupported rows and do nothing.
- **Special bricks:** checkpoint, teledoor, treasure chest, pumpkin and water
  bricks still refuse to plant until their behaviors exist.
- **Terrain:** maps use a finite loaded terrain region. Very long travel on
  Slopes and similar maps can leave the collision area.
- **Vehicles:** physics is a native adaptation, not Torque-exact; driving feel
  needs your judgment. The tank turret barrel may be oriented incorrectly.
  Vehicle burning/splash emitters and wreck models are not drawn yet.
- **Weapon icons:** kill messages use the base death icons, not each weapon's
  own icon.
- **Bots:** simple steering without path finding; they can get stuck on
  complex builds. They only fight inside the brick owner's minigame.
- **Emotes:** play their sound (alarm) and the sit pose; the floating emote
  images are not drawn yet.
- **Special map behavior:** Tutorial triggers and some map-specific objects are
  not implemented.
- **Admin:** host/admin roles, bans and basic moderation work; the complete
  original Admin/SuperAdmin tool set does not.
- **Visuals:** lighting, shadows, some materials, water and sky effects are not
  final.
- **Platform:** Windows only.
