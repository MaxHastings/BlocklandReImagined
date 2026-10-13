# 2026-10-13 v0.3.0 engine pile: grips, first person and seats for imported bodies

Branch `claude/v031-bodies-grips-fp-0wnhlw`, PR against `v0.3.0-experimental`.
Max's playtest of Mario, Master Chief, the Halo rifle and the Warthog:
"weapon positioning in the hand still not right especially across different
games", "first person camera not right, feels off". He approved the generic
engine fixes ("yes the engine pile") and the v0.3.0 plan's DoD line: a weapon
sits in every body's hands at the right grip, imported bodies get a working
first-person view, and imported models sit naturally in seats. No per-body or
per-Add-On special case.

## Cause

- An imported body held an image in its hand socket's own axes. Each game's
  hand bones point differently, so Chief held the Halo rifle vertical and the
  carbine pointing at the ground; Mario held them along his forearm.
- An arm with no `aim_right` clip (Chief, Mario) hung at the side while
  holding, so even a level gun pointed at the floor.
- In first person a held image drew at the body's hand socket, which the
  camera never sees: Chief had no gun on screen and Mario's carbine pointed
  straight up from the bottom edge.
- Riders sat at their own `sit` clip's hip, whatever height the seat was
  authored for.

## What changed

- `bri_content::hold` (new, shared by client and host): two-bone `reach`
  (moved from the client), `fit` (raise a hanging arm in front of the chest
  to the Blockhead's aim spot scaled to arm length, turn the hand to the
  Blockhead's aiming frame), `hip` and `sit_hip`.
- Measured on the real Blockhead: `Mount0` in the aim pose is level
  (`AIM_HAND = identity`) and sits at (0.527, -0.585, -0.654) from the eye
  (`FIRST_PERSON_HAND`). The content test
  `the_blockheads_aiming_hand_is_the_frame_fitted_hands_take` pins both.
- Client (`avatar.rs`): a fitted body (imported glTF `body`) caches its
  `Fits`; a held hand is IK'd each frame to its aim spot about the shoulder,
  pitched with the look, and the image drawn in the aiming frame; the left
  hand of an `aim_both` image reaches its `supportPoint` (else halfway to the
  muzzle). v20 shapes and the Blockhead are untouched.
- First person (`frame.rs`): the viewer's own fitted body draws held images
  at eye × `first_person_hand`, the Blockhead's spot, gun only.
- Host (`grips.rs`): `body_grips` fits the same way and records each hand's
  shoulder as the muzzle pitch pivot, so shots leave the gun the clients draw.
- Seats: a rider is shifted so its `sit` hip meets the Blockhead's `sit` hip.
- `supportPoint` imported from glTF items; `bri_weapons::muzzle::support`.
- Docs: `modding/content-kinds.md` (Grips on imported bodies),
  `modding/gltf-models.md`, `modding/weapons.md`.

## Evidence

Offscreen renders with Max's private Halo and Mario Add-Ons (not committed),
before on the experimental head and after on this branch, in
`/mnt/project-files/v0.3.0/engine-pile/`:

- `before-rifle.png` / `after-rifle.png`, `before-carbine.png` /
  `after-carbine.png`: Blockhead, Mario and Chief, third and first person.
  After, Chief and Mario hold both guns forward and level and their first-
  person view matches the Blockhead's. The Blockhead looks unchanged.
- `before-seats.png` / `after-seats.png`: the Warthog's seats. Little
  change: the Halo Add-On's seats are tuned to Chief's pelvis (lowered
  1.62 units), so Chief still sits right and the Blockhead and Mario sit
  high in them. Seats authored for the Blockhead now seat every body at the
  Blockhead's hip. Making the Warthog seat every body is the Add-On's seat
  authoring, not an engine change.

Tests: `cargo test -p bri-content hold`, the avatar unit tests with and
without `BRI_CONTENT`, `python tools/check.py focused --base
origin/v0.3.0-experimental`.

## Not done

- Imported bodies' own first-person arms: the view is gun only (the DoD
  allows either). Arms need a per-body first-person rig.
- Mario/Halo animation quality, movement fidelity and per-Add-On polish stay
  parked by Max's decision.
