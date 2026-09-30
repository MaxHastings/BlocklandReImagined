# Known issues

Crashes, missing content, controls that don't work, lost saves or broken
core flows are blockers: please report them (see `TESTER-GUIDE.md` for what
to send). The items below are known. Everything v20 had that is still
missing is listed in `FEATURES.md`.

- **Windows only**, and unsigned: SmartScreen warns on the first start.
- **Wrench events:** projectile outputs on delayed event rows aren't applied
  yet. Immediate `Delete`, `Bounce` and `Redirect` work. Rows using them are
  kept and shown read-only.
- **Vehicles:** handling is rebuilt, not copied from v20's engine. Tell us
  where driving feels off.
- **Bots** find paths over bricks and map shapes but not through moving
  things (vehicles, other players, doors being opened); they stop, then try
  another way. They don't swim, drive or jet across gaps, and they only fight
  inside the brick owner's mini-game.
- **Gamepads** work while playing; menus and building need a keyboard and
  mouse.
- **Old Add-On scripts don't run.** Imported v20 Add-Ons bring their bricks,
  weapons and vehicles, not their custom behaviour.
- **Block faces** from Add-Ons that generate worlds aren't drawn yet.
- **The Kitchen's main floor** sits slightly off the brick grid.
- **The Tutorial** runs in single player only.
- **Lighting, shadows, water and sky** aren't final.
- **Knocked-out bricks** tumble differently on each player's screen. That's
  on purpose: they're only for show and never affect play.
