# 2026-10-01 The Gravity Gun through portals

Branch `claude/gravity-gun-rework-ainb6j`. Max, v0.1.11: "using the gravity
gun through the portal don't work i can't grab or carry something through".

## Engine seams (generic)

- `Passages::sight(from, direction, length)` (`bri_content::passage`): a
  straight sight through the openings it goes in by, as legs, each with
  the carry that took the sight there. Shots and views already went
  through; sights now do too.
- `Session::sight`: the brick and the nearest movable object a player
  sees, through portals. Command `aim()` (every Add-On) and `reach` use it,
  so `aim()` positions are where things are beyond a portal and its
  distances run along the sight. `aim().object_at` is where the aim met the
  object (the gun grips there instead of computing it from the eye).
- Holds carry a `through` carry from the holder's side to the object's,
  picked from the holder's sight when the hold starts and turned by every
  opening the holder or the object goes through (`Session::crossed`, fed
  by the player and entity motors' `passed` and the vehicles'
  `carry_through_openings`). A thing held beyond a portal stays there; one
  carried through stays held, at its distance and angle.

## Evidence

- `crates/sim/tests/showcase.rs`
  `the_gun_grabs_through_a_portal_and_reels_it_back_through` and
  `a_held_thing_carried_through_a_portal_stays_held`: both fail on main
  d2ae52670 (nothing caught; the crate jumped 39.5 units in a tick when
  carried through) and pass now.
- `cargo clippy --workspace --tests -- -D warnings` clean; bri-sim
  showcase, portals, script_api, hardening_packages and lib, bri-content
  lib and bri-package-runtime pass.

## Next

- The beam effect (`gravity-gun-fx`, client WASM) still draws one curve
  from the muzzle to the held thing; through a portal it should bend
  through the opening. That needs the openings exposed to client code.
