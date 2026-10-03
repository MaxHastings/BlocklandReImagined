# Known issues

Crashes, missing content, controls that don't work, lost saves or broken
core flows are blockers: please report them (see `TESTER-GUIDE.md` for what
to send). The items below are known. Everything v20 had that is still
missing is listed in `FEATURES.md`.

The [v0.2.2 creator and bot checks](rule-workshop/V0.2.2-PLAYTEST.md) cover the
current changes. Focused headless/offscreen checks do not replace your playtest.

- **Unsigned builds:** Windows SmartScreen warns on the first start. The Apple
  silicon Mac app uses an ad-hoc signature; see `PLAYTEST-MAC.md` for opening it.
  Linux builds target x86-64 systems with glibc 2.35 or newer.
- **Wrench events:** projectile outputs on delayed event rows aren't applied
  yet. Immediate `Delete`, `Bounce` and `Redirect` work. Rows using them are
  kept and shown read-only.
- **Vehicles:** handling is rebuilt, not copied from v20's engine. Tell us
  where driving feels off.
- **Bots** find paths over bricks and map shapes, but moving obstacles and
  narrow routes can still stall them. Combat remains scoped to the bot's
  mini-game. Autonomous piloting
  covers ground vehicles; aircraft and boats remain future work. Chassis routes
  are conservative. Supported native hand weapons use bounded ballistic
  checks; mounted, portal and script-driven mechanics still use their existing
  narrower behavior. Objective planning covers supported activation/region/
  bot-touch rules, not arbitrary Add-On scripts, ball delivery, hookshot routes
  or coordinated stacking. Complex rule arrangements can exceed the planner's
  finite depth/work limits even when a human can solve them. Resting pauses an
  approach's travel deadline; scheduled event delays keep passing in real time.
- **Firefight crash:** a Windows v0.2.0 main-thread NaN panic during a mixed
  Zombie/Blockhead battle remains unreproduced. The headless reproduction passes,
  but does not cover Windows rendering/audio. v0.2.1 improves crash reports and
  retains matching build symbols; it does not claim a causal fix. Please retain
  the complete crash/session files if it happens again.
- **Shark/Zombie ports remain partial.** Shark's original body, swim,
  mouth capture/five-second hold and hidden hole now have focused checks using
  actual imported content. Its original escape loop, restart delay, forced
  vehicle ejection, full white-Shark aggression/no-strafe behavior and bite
  death icon remain incomplete. Zombie infection markings/name prefixes also
  remain incomplete. Visual feel still needs your playtest.
- **Colorsets:** only valid UTF-8 text palettes with 1–256 RGBA colors load.
  Removed or invalid selections block a new host instead of silently choosing
  Default. Use **Start Game → Colorsets... → Folder...** for custom `.txt` files
  or subfolders containing `colorSet.txt`. A full 256-color palette can leave
  no room for a generated world's additional material colors, causing an
  explicit startup rejection.
- **Gamepads** work while playing; menus and building need a keyboard and
  mouse.
- **Unported old Add-On scripts don't run.** Imported v20 Add-Ons bring their
  bricks, weapons and vehicles. Supported ports provide specific native behavior;
  this is not a general TorqueScript interpreter.
- **Block faces** from Add-Ons that generate worlds aren't drawn yet.
- **The Kitchen's main floor** sits slightly off the brick grid.
- **The Tutorial** runs in single player only.
- **Lighting, shadows, water and sky** aren't final.
- **Knocked-out brick visuals** tumble differently on each player's screen.
  They never affect play. Explosion debris lasts up to 13 seconds (ten opaque,
  three fading); Physics Quality and load limits can retire older pieces early.

- **Rule Workshop** is experimental. IF checks run when a delayed action is
  due; regions use entity centers and sweep straight through portal/teleport
  jumps. Authored rules save, while live counters do not. Team totals follow
  current members' scores. The shipped Rule Workshop guides describe these
  choices and the remaining limits. Alpha save formats can change.
