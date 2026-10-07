# 2026-10-07 Facing mirrors: no undrawn patches while moving

Max (v0.2.6 preview bfdf854): between two facing mirror bricks the tunnel
repeats, but grey patches flicker in it while he moves. In a square of four
mirrors, mirrors seen inside another show flat silver: that is the
2026-10-06 cutoff (`2026-10-06-mirror-room-echoes.md`), shipped in v0.2.5,
not this bug.

## Cause

Live planes draw deepest first, so when a deeper plane's view shows a
nearer plane's mirror past the passes (`Shows::Echo`), that nearer plane's
target still holds its picture from the frame before. The echo sampled it
with this frame's viewport and view. Where the view had moved, the sample
fell on texels that frame never drew: the target's clear colour (the sky's
in the game, so grey), recursing down the tunnel for a frame at a time.

## Change

- `reflection::Plan::slots`: in a plane's view, an echo of a plane not yet
  drawn this frame shows that group's kept picture with the sampling it was
  drawn for (`Shows::Last`, the copy and uniforms `prepare` already keeps).
  The player's view, drawn last, still echoes this frame's pictures. The
  first frame (nothing kept yet) is as before.
- No new pass, texture or setting; no protocol or content change.

## Evidence

Offscreen (lavapipe), a closed striped box between two facing mirrors,
cleared magenta so only undrawn texels show it, the camera walking and
turning:

- Before: up to 444 undrawn pixels of 16,384 per frame at Medium and 260 at
  High. After: at most 44 at Medium and 5 at High, at the vanishing point.
- `-p bri-render --test mirrors walking_between_facing_mirrors_never_shows_an_undrawn_picture`:
  fails on main (168 undrawn pixels in frame 3), passes with the fix.
- `reflection::tests` (slots after a kept picture) and the whole `mirrors`
  test file pass; `cargo clippy --workspace --tests -- -D warnings` clean.

Not checked: the real game on Max's PC.

## Also answered (no change)

- Shadows through portals: players, bots, vehicles, items and debris
  already cast in portal views (same casters as the player's view).
- Fence shadows differ through a portal: lamp shadows are picked near the
  player and in the player's view (`ShadowMaps` lamp picking), and portal
  views reuse those picks. Picking lamps per plane is proposed for v0.2.7.
- Bedroom window light on Low vs Best: Low's two 1024 cascades over 100
  units give texels about 0.23 units past ~15 units (Best 0.05 to 0.1), so
  a live sun patch (Dynamic lighting) is visibly softer.
