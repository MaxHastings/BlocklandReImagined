> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-portal-shadows.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: far portal sun shadows (PR #30), head 2b5d786, base main

This was an independent, read-only review. Nothing was built or run. I read the whole diff and the shadow code it touches (`shadow.rs`, `scene.rs` `render_view_shadows`, `kept_shadows.rs`, `reflection.rs` `render_views`, the client's caster lists).

## Verdict: correct, but hold the merge for one measurement

The fix is right and the test is real. The open question is cost, and it's only unknown in one case: **bricks casting sun shadows** (Brick Shadows on, or Dynamic lighting). In that case every live mirror or window now redraws every brick in reach into every cascade, every frame. The player's own cascades avoid exactly that with the kept brick layers. A cheaper correct way exists for that case (below). Under Max's rule, that makes it a blocker **if the measurement shows a cost a player would notice**. With bricks not casting (the default), the extra cost is small and it can merge.

## 1. Was the old check the cause? Yes.

- The old `reach` (`shadow.rs`, removed) tested only the plane's **eye**. It passed when that one point was inside any cascade's light box, which runs 400 units (`CASTER_REACH`) towards the sun, and short of the last split's depth.
- What a window shows lies **past** its eye. A partner standing inside the player's shadow distance passed the check, so the window read the player's cascades, which end (and fade) right where the window starts looking.
- Mirrors never had their own. A mirror shows what's behind the player, which the player's forward-fitted cascades only partly hold.

## 2. Is "every plane fits its own" the simplest correct fix? Yes. A corrected check would not save much.

- **A correct check is possible.** Project the corners of what the plane shows (its window crop, out to the shadow distance) into the player's cascades. If every corner is inside, the player's cascades already cover it.
- **But it would rarely skip anything.**
  - A mirror shows behind the player, so it almost never passes.
  - A window's view runs a full shadow distance from the partner, so it passes only when the partner is at the player's own spot.
  - So it would add code and a second rule for little saving. Always fitting is the right base.

**The cheaper correct fix is a different one: reuse the player's kept brick layers in plane cascades.**
- Every kept layer is drawn on the player's cascade grid, twice the cascade's width, around the player's eye (`kept_shadows.rs:1-19`).
- A plane's cascades are fitted to the player's frustum moved rigidly (`reflection.rs:1101-1107`). So their split radii, and therefore their texel sizes, come out the same as the player's (`shadow.rs` `cascades_after`, quantised radius).
- So when a plane's cascade square lies inside the player's kept layer (`Placement::holds`), it can copy its bricks from that layer and draw only the moving casters, just as the player's cascades do. Otherwise it draws directly, as now.
- This mostly helps the big outer cascades, which hold the most bricks. They're also the ones a near mirror or portal stays inside.
- What it needs:
  - a blit slot per view;
  - reading the placement without changing it;
  - doing the kept redraw for brick edits before the plane passes. Today the player's pass runs after the planes (`reflection.rs:1324-1329`), so a plane would otherwise show a just-placed brick's shadow one frame late.

## 3. Is the new test real? Yes.

- `a_window_onto_a_place_at_the_edge_of_the_players_shadows_shows_them` puts the partner one shadow distance (`ShadowSettings::LOW.distance`, 100) ahead. The camera's far plane is also 100, so what the window shows is past the player's shadows.
- Under the old rule the partner's eye passes `reach`, so the window used the player's cascades. The write-up's failing count (639 of 799 shaded pixels, under the 90 % the test needs) fits that. I didn't run it.
- It shares the comparison helper with the existing 1000-unit test: shaded pixels through the window against standing there, and a bound on how many pixels differ. Nothing was loosened.
- It ran on lavapipe only. It hasn't been seen on a GPU.

## 4. Is the extra cost bounded?

- **In count, yes.**
  - It applies only to live planes: at most `ReflectionSettings::MAX_PLANES` = 3.
  - It applies only while sun shadows are on and there is a sun. Otherwise `fit_view` returns false.
  - Lamps and map light cubes stay the player's (`scene.rs` `render_view_shadows`, `player` gates).
- **In size, no.** It grows with the build. Per plane, each cascade draws:
  - with bricks not casting (default): moving casters (players, vehicles, items), bricks only as blockers inside those casters' footprints, and the map layer. That's the same work as the player's cascades, and small.
  - with bricks casting (Brick Shadows on, or Dynamic lighting): **every brick chunk in the cascade's reach, directly**. The kept layers were added because this "in a large build was most of the frame" (`kept_shadows.rs:1-4`). Their own measurement is 0.24 ms per million brick triangles per cascade on a 4070 SUPER (`kept_shadows.rs:33-35`). So the new cost is about that, times the cascades, times the live planes.
  - Far portals already paid this since v0.2.5. Near portals and every mirror are new.

**Before merge:** the PC thread measures frame time with the existing GPU timing (`begin_timing`), offscreen if possible. The case to measure:
- Brick Shadows on;
- Max's biggest build;
- a mirror or near portal in view;
- once at Best shadows.

**What the result means:**
- **Small:** merge as is, and put the kept-layer reuse on v0.2.7.
- **Noticeable:** add the kept-layer reuse in this PR first.

## Not blocking (v0.2.7 notes)

- **Nested planes rely on a guess.** The removed parent rule made a plane seen in a plane with its own shadows also have its own. The new comment says fitting "fails for every plane alike". It mostly does, but `cascades_after` can fail for one plane alone, on a degenerate matrix. That inner plane would then draw after its parent (`render_views` order) and show last frame's picture for a frame. It's harmless and rare. Either keep the one-line parent rule or check the claim.
- **Plane cascades fit the whole moved frustum, not the window.** A small window far off pays casters for the full frustum and gets coarser texels than it could. Fitting to the window's crop, out to the shadow distance, would cut cost and sharpen shadows. The comment at `reflection.rs:1101-1104` explains why it's not done (no far plane to split), but a far plane at the shadow distance would do.
- **Leftovers:** none. `shadows_reach` and `forward` have no other callers.
