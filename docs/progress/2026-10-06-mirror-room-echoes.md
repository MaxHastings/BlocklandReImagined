# 2026-10-06 Mirror rooms: echoes only where they are true

Max's friend built a room walled with four mirrors and saw heavy artifacts
from the middle. Max picked the no-cost fix (option 1 of three: honest
cutoff; more passes on an opt-in setting; ray tracing).

## Cause

Only the Mirrors setting's passes (Low 1, Medium 2, High 3) draw live. A
mirror seen deeper shows an echo: the picture its wall's nearest live plane
drew, reprojected through the view that plane was seen in (also the frame
before's picture, `Shows::Last`). For two parallel facing mirrors that
picture is close to what the deeper view sees, and the tunnel looks right.
In a square room the side walls bounce the view sideways, so a wall seen in
another shows the picture drawn for the player's own view of it: the wrong
part of the room, clamped into streaks, and black wedges where the picture
was never drawn. Which planes went live could also swap frame to frame
when two filled about the same screen.

## Change

- `mirror.wgsl`: an echo of a mirror fades to its fallback (silver) by how
  far the viewing eye sits off the line along the mirror's normal through
  the eye the picture was drawn for, per unit of distance from the mirror
  (full echo below 0.05, none past 0.25). Parallel facing mirrors shift the
  eye only along the normal, so their tunnel is unchanged; a square room's
  side-wall bounce shifts it by about 2. Windows (portals) echo as before.
  The slot uniform carries that eye and the plane (`source`, `plane`).
- `reflection::plan_after`: a plane live in the frame before
  (`Plan::paths`, the group keys down to it) counts for 1.5x its screen
  (`KEEP_LIVE`) when planes compete for passes. `plan` is unchanged.

No extra passes, no new setting, no protocol or content change.

## Evidence

Offscreen (lavapipe), a 6x6 room of four mirrors, three views at High and
Medium, 3 frames each: before, black wedges (11,271 near-black pixels in the
Medium corner view, 1,778 in the High straight view) and streak fans; after,
none, with the parallel tunnel kept (29 dark pixels left at its vanishing
point, still unfilled after 3 frames, in both runs).

- `-p bri-render --test mirrors in_a_mirror_room_a_side_wall_past_the_passes_never_shows_another_view`:
  718 black pixels on main, 0 with the fix.
- `reflection::tests::a_live_plane_keeps_its_pass_until_another_clearly_fills_more_screen`.
- `cargo test -p bri-render` all pass (existing facing-mirror tunnel tests
  included); `cargo clippy -p bri-render --all-targets -D warnings` clean.

Not checked: the real game on Max's PC; the gate runs at release merge.
