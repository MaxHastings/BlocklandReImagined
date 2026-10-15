# 2026-10-15 v0.3.1: vehicle camera, gunner pose, turret sway, gun bob, footsteps

Branch `claude/v031-engine-camera-anim-vugxtd`, for the Halo CE vertical
slice (rifle, Chief, Warthog, Blood Gulch, HUD). Each piece is a general
engine field; nothing Halo-specific is in the engine.

- **Surface kinds** (`bri_content::surface`, `bri_sim::map::MapSurfaces`,
  `Building::surface_at`): a map bundle's `surfaces` names what each
  material is made of; the glTF importer reads `bri_surface`. Shared with
  the visuals thread's impacts. Seam row in `seams.md`.
- **Orbit vehicle camera** (`camera.orbit` in `vehicles.json`): the
  driver's third-person camera orbits with the mouse within
  `min_pitch`..`max_pitch`, looks at a pivot `height` over the box centre,
  trails by `lag` seconds; the existing boom ray keeps it out of the ground.
- **Gunner pose** (body role `seat_aim`): held at the gun's pitch across
  `look_pitch`, in place of `look`. The body already turns with the turret.
- **Turret sway** (`turret_sway`): a client-side damped spring tips the
  attachment model against the hull's acceleration.
- **Gun bob** (`camera.sway.walk`): the weapon's own bob, independent of
  the view's.
- **Footsteps by surface** (archetype `sounds.surfaces`): the footfalls
  #159 added (`sounds.footsteps`) now pick per surface kind underfoot.
  This branch's own `look.footsteps` was dropped in the merge with the
  integration branch in favour of #159's.

Protocol: `archetype-footsteps-weapon-bob.md`.

Evidence: unit tests `the_weapon_bobs_on_its_own_while_the_view_stays_still`,
`an_orbiting_camera_*`, `a_turret_tips_back_*`, `a_gunners_aim_*`,
`a_step_sounds_each_stride_*`, and the glTF map import test reading
`bri_surface` back by hit triangle. Clippy `-D warnings` clean on every
changed crate. Not verified: on-screen feel; Max playtests it with the
Halo Add-Ons.
