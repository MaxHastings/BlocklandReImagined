# 2026-10-04 Portal polish for the showcase video (branch `fix/portal-polish`)

Max asked for portal jank to be found and removed before he films the
showcase video. Scope stayed at fixes and polish of existing behaviour (no
new subsystem). Each fix is at the shared mechanism, not per object.

## Fixed

1. **Bodies between poses drawn at the wrong doorways (players, bots,
   vehicles, Steel Ball).** Remote bodies and interpolated vehicles are drawn
   between two host poses. When a portal crossing fell between the poses,
   `blend_through` (`motion.rs`) and `vehicles::sample` carried the earlier
   pose past the partner from the start. Until the drawn middle reached the
   opening, the body sat behind the partner's pane, which a doorway's back
   face shares. `Straddle::find` then cut it at that face, so for a frame or
   two its halves showed at the wrong doorways (front half at B's back, the
   rest at A's back). Now both blend in the space the body went in from and
   carry once the blended middle is through. Vehicles judge the crossing by
   their middle (centre of mass, learned in `prepare`) instead of their root
   position, as the host and `Straddle` do.
2. **Debris pushed by an unscaled, spinning box.** Every vehicle pushed
   client debris (and Add-On bodies) as a box the size of its *unscaled*
   bounds, spinning with it. A giant Steel Ball (scale 3) left loose bricks
   0.86 units inside itself, and a rolling ball swept bricks like a paddle
   wheel. Pushers now use the vehicle's own collision shape at its scale:
   `bri_vehicles::body_shape`, pulled out of `prepare_spawn` so the host and
   the client build it from the same code. `ClientVehicles::pusher` caches it
   by definition and scale.
3. **Sound ignored portals.** Positional audio used straight-line distance
   only, so a crash seen through a portal sounded as far away as it really
   was (usually silent) and came from the wrong side. `bri_audio::Window`
   (the listener carried to a portal's far side, plus the far face it hears
   out of) lets the engine place each voice by the shortest way, whether
   direct or through a window. That is the same rule sight and reach use
   (`Passages::shortest`). The client sends the portals in front of the
   camera (`portal_view::hearing`, up to 16 within 150 units) only when they
   change. Attached and looping voices keep their handles, so nothing
   restarts or doubles at a crossing.
4. **Brick debris fell out of the doorway's back.** Debris knocked into a
   portal fell through the hole in the frame and tumbled out behind the
   doorway. Each debris step now uses `local_physics::carry_through_openings`,
   the host's vehicle rule: a body whose origin went in through an opening
   comes out of the partner, with its velocity and spin turned to match.

## Before / after

| Category | Measure (test) | Before | After |
| --- | --- | --- | --- |
| Remote player crossing | samples drawn cut at the wrong doorway, 41 frames over one crossing (`motion::tests::a_remote_body_between_poses_through_a_doorway_draws_only_on_its_way`) | 19 of 41 | 0 |
| Steel Ball / vehicle crossing | same, ball spinning, middle off its origin (`vehicles::tests::a_ball_between_poses_through_a_doorway_draws_only_on_its_way`) | 20 of 41 | 0 |
| Giant ball vs debris | nearest brick centre to the centre of a 3.75-radius ball (`brick_debris::tests::a_giant_steel_ball_rolls_debris_aside_as_a_ball_its_size`) | 2.89 (0.86 inside) | 4.22 (outside); fastest brick 21.8 u/s for a 12 u/s ball |
| Debris through a portal | came out of the partner (`brick_debris::tests::debris_knocked_into_a_portal_flies_out_of_its_partner`) | never | yes, velocity turned with the carry |
| Audio through a portal | LoopSound 8 units past a window 100 away (`bri-audio` `a_sound_seen_through_a_window_is_heard_from_where_it_shows`; client `portal_view::tests::a_sound_seen_through_a_doorway_is_heard_where_it_shows`) | silent | within 2% of the same sound 8 units away directly, panned to its side; distance equals `Passages::shortest` |

Each "before" was measured by temporarily reverting only the production
change and rerunning the new test. The audio and debris guards cannot run on
the old code at all, since the mechanism did not exist.

## Video beats checked (headless)

- *Walk through, turn round, walk back:*
  `crossing_tests::walking_back_through_every_face_of_the_pair_never_changes_the_picture`
  walks into the other three faces of a pair (A from the north, B from the
  east and the west), first person and chase camera. No frame jumps. It
  passed on the old code too; it guards the return trip.
- *Steel Ball launched through a portal into bricks:* `bri-sim` showcase
  `a_steel_ball_hurled_into_a_portal_smashes_the_wall_beyond_its_partner`.
  The ball comes out at x = 40 still above smash speed and breaks at least
  3 bricks of the wall beyond. It already passed. The gravity-gun grab and
  carry through a portal stay covered by the existing showcase tests.
- *Jeep / horse / tank through:* already covered by `bri-sim` vehicles and
  portals tests and the client mounted-prediction tests (2026-10-03 note).
  Nothing new found.
- *Admin (F8) camera:* `controls::a_free_camera_flies_through_a_portal`
  covers the carry. Doorway pictures are drawn on both faces, 1 mm out from
  the pane, and cover the whole side under the frame, so I found no seam
  path from behind or at grazing angles in review.

## Mirrors (light pass)

Reviewed `bri_render::reflection` and the client's per-view draw lists.
Debris, vehicles, avatars, held items and particles (`snapshot_in_other_view`)
all draw in mirror and portal views. Facing mirrors and portals fall back to
the previous frame's picture past the pass budget. I found nothing I could
show was wrong, so no mirror code changed. The render `mirrors` offscreen
tests (lavapipe) and `reflection` lib tests pass.

## Observed, not changed (stabilization scope)

- Rolled along the ground at speed, a Steel Ball hits the doorway's 0.2-unit
  sill (the brick's frame, matching its mesh) and loses about half its speed
  with a hop (24 → 12 u/s in the probe). Thrown through in the air, it
  keeps its speed. This is content geometry, not a mechanism bug.
- Add-On bodies (ragdolls) do not cross portals. Their joints form a rapier
  multibody, so carrying one needs its generalized velocities rotated too.
  This is next-release work.
- The stock catalog only has upright doorway portals (quarter turns about
  the vertical), so floor and ceiling portals cannot be built from stock
  bricks. The mechanism supports them (`falling_through_a_floor_portal...`).
- Chat captions still use straight distance for their earshot check.

## Commands

All with `CARGO_TARGET_DIR=/home/claude/bri-target`, debug off, per-crate:

- `cargo test -p bri-client --lib` (all lib tests, GPU tests on lavapipe)
- `cargo test -p bri-audio`
- `cargo test -p bri-sim --test showcase --test portals --test vehicles`
- `cargo test -p bri-vehicles`
- `cargo test -p bri-render --test mirrors` and `--lib reflection`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
