# 2026-10-01 Portal names and far portal views

Max reported two portal bugs (relayed 20:15Z): player name tags showed when the
body was hidden behind a portal, and far portals turned flat light blue. His
friend Sauce saw nested portals drawn flat light blue.

## Changes
- Name tags and item labels follow the body. `portal_view::seen_at` lists
  every place the eye sees a target: directly, when no opening is in the way,
  and through each portal whose view shows it. `app.rs` `name_tags` places the
  tag at the first of these that is not blocked by bricks. A body behind an
  opening has no tag. A body seen through a portal gets its tag where the
  portal shows it.
- Windows (portals) go live at any distance. The `ReflectionSettings.distance`
  cutoff now applies only to mirrors. Cost stays bounded: passes are still
  capped by the Mirrors setting and ranked by screen fill, and each pass's
  frustum is fitted to the window's screen rect. The headline scenes have no
  portals.
- A surface that would show the idle colour (a nested window in its own view,
  or a group over the pass budget that was live last frame) now shows the
  previous frame's picture (`Shows::Last`), as Valve's Portal does. Each target
  keeps a `previous` texture, copied from the picture at the start of
  `Reflections::render`.

## Limit
A group that was never live (for example a second portal pair on Low, which
has one pass) still shows its idle colour.

## Tests
- `portal_view::tests::a_name_shows_where_the_portal_shows_its_body` (fails on
  the old placement, which ignored portals).
- `reflection` lib tests: a window at `distance: 1.0` on Low is still live; a
  facing pair's over-budget plane shows `Last(0)`.
- `crates/render/tests/mirrors.rs` replans every frame after the first, which
  exercises the previous-picture copy. Mirrors (6) and metal (3) GPU tests pass.
