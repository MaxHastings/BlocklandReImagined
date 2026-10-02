# Known issues

Crashes, missing content, controls that don't work, lost saves or broken
core flows are blockers: please report them (see `TESTER-GUIDE.md` for what
to send). The items below are known. Everything v20 had that is still
missing is listed in `FEATURES.md`.

- **Unsigned builds:** Windows SmartScreen warns on the first start. The Apple
  silicon Mac app uses an ad-hoc signature; see `PLAYTEST-MAC.md` for opening it.
  Linux builds target x86-64 systems with glibc 2.35 or newer.
- **Wrench events:** projectile outputs on delayed event rows aren't applied
  yet. Immediate `Delete`, `Bounce` and `Redirect` work. Rows using them are
  kept and shown read-only.
- **Vehicles:** handling is rebuilt, not copied from v20's engine. Tell us
  where driving feels off.
- **Bots** find paths over bricks and map shapes but not through moving
  things (vehicles, other players, doors being opened); they stop, then try
  another way. Combat remains scoped to the bot's mini-game. See the release
  notes for the supported vehicle and physics interactions.
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

- **Rule Workshop** is experimental. IF checks run when a delayed action is
  due; regions use entity centers and sweep straight through portal/teleport
  jumps. Authored rules save, while live counters do not. Team totals follow
  current members' scores. The shipped Rule Workshop guides describe these
  choices and the remaining limits. Alpha save formats can change.
