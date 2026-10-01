# Player and session integration

`bri-sim::player` is the motor the server session steps at a fixed 120 Hz;
inside those steps it runs v20's own 32 ms ticks (below). Inputs
contain bounded movement axes, look angles and jump/crouch/jet intentions. They
contain no position, velocity, owner, administrator flag or client time step.

## Movement fidelity

The recovered `PlayerStandardArmor` declaration in
`.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs:8738` supplies:

| Parameter | Starting value | Evidence/interpretation |
| --- | --- | --- |
| Forward/backward/side speed | 7 / 4 / 6 | Script fields at 8764–8766 |
| Crouch speeds | 3 / 2 / 2 | Script fields at 8768–8770 |
| Ground acceleration | 48 | runForce / mass, implementation interpretation |
| Air control | 0.1 | Script field; scales the new motor's acceleration |
| Jump speed | 12 | jumpForce / mass, implementation interpretation |
| Unlimited standard jets | no fuel drain | canJet=1 and jetEnergyDrain=0 |
| Climb angle | 70 degrees | runSurfaceAngle |
| Third-person maximum distance | 8 | cameraMaxDist |

These values do not establish movement equivalence. Dimensions are confirmed
from `blocklandv20.exe` (read-only disassembly, 2026-09-27): PlayerData stores
`boundingBox` at +0x374 and `crouchBoundingBox` at +0x380 unscaled, and
`Player::step`/the collision box multiply them by the object scale and 0.25.
The world box is therefore 1.25 x 1.25 x 2.65 standing and 1.25 x 1.25 x 1.0
crouched, feet at the box bottom, matching `PlayerTuning`. `maxStepHeight`
(+0x2AC) defaults to 1.0 and is not quartered.

The eye is m.dts's `Eye` node, which `Player::getRenderEyeTransform`
(0x5aafa0) reads from the animated shape: 2.156 above the feet standing,
0.627 at the end of the `crouch` sequence, and 0.141 ahead of the box centre
along the body's facing. Gravity 20, jet thrust 35, horizontal
jet thrust 48, jet rise cap 25 and forward cap 33 are explicit adaptation
assumptions. The original
script's resistance limits inform the caps but do not prove the new equations.
`PlayerTuning` keeps these assumptions together. Do not copy Blockland2's scaled
render/collision constants: that prototype deliberately changed world scale and
clearance, and is not our numerical source of truth.

The motor normalizes diagonal input, accelerates toward directional speeds,
preserves idle airborne momentum, jumps on the input edge, checks clearance before
standing, slides along obstructions and steps onto low geometry. Standard jets
are unlimited; crouch blends toward aimed forward thrust. Original avatar rendering,
customization and initial run/back/side/crouch/jump/fall/look layers are connected
to authoritative poses. Emotes and vehicle mounting now ship. Animation transitions,
movement-rate matching, tools, effects and camera polish remain work.
The third-person camera follows `Player::getCameraTransform` (0x5ab7d0): it
pivots at the middle of the standing box plus `cameraVerticalOffset` (feet +
2.075), pitches the view down by `cameraTilt` (0.261) and sits `cameraMaxDist`
back along that tilted view, all scaled with the player. Toggling slides the
camera over 0.2 s (`$cameraSpeed` 5). It sphere-sweeps backward from the pivot. The native client
now queries map geometry and replicated authored brick collision shapes, including
shapes extending beyond the placement footprint. Visibility and raycast toggles
do not override collision. Dynamic
actor/vehicle camera obstruction have not yet been integrated.

## Collision: v20 `updatePos`, not a physics solver

v20 players never touch a physics solver. Since 2026-09-27 the motor
(`crates/motor/src/torque.rs`) ports v20's own player collision, read from a
read-only disassembly of `blocklandv20.exe` and checked against the TGE
`player.cc`/`extrudedPolyList.cc` sources. It replaced Rapier's
`KinematicCharacterController`, which treated the 74.5 degree face of a
"72 degree" ramp as ground, so slide builds stood players still.

**How the motor queries the world.** Once per tick `torque::Soup::gather`
asks Rapier's query pipeline for every non-sensor collider (except the
player's own body) in a box around the player sized for this tick's
possible travel, a step and the contact slab. It turns their shapes into
world polygons: cuboid faces, convex-hull faces (all brick collision),
trimesh and heightfield triangles, compound parts; rounded shapes fall back
to their bounding box. Polygons are stored relative to the player's feet so
sub-millimetre sweeps keep full float precision far from the origin, and
sorted by collider tag (brick id) so client and server agree on ties. Rapier
still owns the bodies; the motor only reads collider shapes and poses. The
kinematic player body is still targeted each tick so other bodies see it.
Every polygon remembers its collider, and `MotionEvents::hits` lists each
blocking hit's collider and its speed into the surface (v20's `onImpact`
`bd`), which breakable glass uses.

**updateMove (0x5AE2A0).** Gravity always applies. `findContact` (0x5AA570)
takes the flattest polygon in a slab 0.013 (`sTractionDistance`) under the
feet: a run surface when within `runSurfaceAngle` (70), a jump surface
within `jumpSurfaceAngle` (80). On a run surface the part of gravity into
it is cancelled (plus a 0.002 lift, zeroed below Blockland's 0.0021 so level
ground rests exactly), the move is turned parallel to the surface (not while
jetting), and the run force (`runForce / mass` per tick) steers toward it.
Anything steeper is not a run surface, so gravity slides the player down it;
air control (not while jetting or swimming) applies instead.

**updatePos (0x5B0714).** The axis-aligned box is swept through the polygons
(`ExtrudedPolyList`): each box face leading the move is extruded, each
polygon facing the move is clipped to it, and the first contact stops the
move. The collision normal is always the hit polygon's own plane, never a
separating axis, so the hidden top of one ramp under the next never reads as
ground. Of simultaneous hits, the one most parallel to the box face wins. The
box backs off, may step (below), then loses the velocity into the plane plus
`sNormalElasticity` 0.01. On the second hit the velocity is re-aimed along
the crease between the two planes at full speed: this is what carries a
Blockhead wedged in a slide lane (two 72 degree ramps facing each other,
narrower than the box) down the lane, turning each drop into lane speed.
Five hits in one tick give up and stop dead, as in v20.

**Step (0x5A9FD0).** Only from a run surface, when the hit is low enough
(below `maxStepHeight * scale`), off a wall (|normal.y| < 0.05) or a walkable
slope, and never off terrain: the highest static vertex at the destination
under the remaining `maxStepHeight` with none of the others within the
player's own height above it. Only the player's height must be clear, so
players step onto plates and bricks under ceilings they fit beneath, and
walk onto low ramps by stepping onto their vertices as in v20. As in TGE the
step probes the rest of the move from the backed-off box; probing from the
contact instead (a 120 Hz workaround) lifted a player walking onto a slope
0.015 above it, off the 0.013 contact slab, so it hopped up every ramp. A
step must also rise above the contact point, as in TGE (v20 also
accepts a zero step): otherwise the floor a fall reaches counts as a step,
the move loops to its retry limit and the landing's impact is lost.
`grounded` means a run surface under the feet and not still closing on it
faster than 1 u/s, so a fall that stops within the contact slab lands, and
impacts, on the next sweep.

**Torque's 32 ms tick inside 120 Hz steps.** Since 2026-09-28 the motor
runs v20's tick, not a rescaled 120 Hz one. Slides need it: the crease rule
re-aims a wedged rider's whole speed along the lane once per tick, so the
speed a lane gives is per tick, not per second. Iterating v20's per-tick
equations, a rider wedged in a level lane settles at 6.517 u/s; at 120 Hz it
settled at 3.248, and a ride from the top of "Mr.Block's Slides" stopped 39
units down instead of 353. Time is counted in 1/3000 s (a step is 25, a tick
96), so every 3.84 steps on average one whole 32 ms `updateMove`/`updatePos`
runs, with every v20 constant unscaled. The tick uses that step's input,
with jump counted if it was held at any step since the last tick (a Move's
trigger); look turns every step. `PlayerState::feet`, `velocity` and the
flags are the last tick's, as on v20's server; `PlayerState::tick` carries
the previous tick's feet and the phase, and `shown_feet()` places the body
between the two, as v20's client renders it, so the client draws smoothly.
The phase travels in every pose (protocol 43), so prediction replays the
same ticks. Consequences that are v20's: players rest 0.01 above floors
(the back-off), run at 6.978 u/s (drag after the run force), swim at 3.41,
launch briefly off the top of a 45 degree ramp, and a jump or crouch lands
on the next tick, up to 32 ms after the key. There is no fall-speed cap:
v20 has none (the old -80 clamp was ours), so falls reach 199 u/s under
drag. Players met head-on part along their least-overlap axis when already
touching; the shape cast's normal is arbitrary there. All hits
within the tie count (Torque's running comparison made near-ties depend on
polygon order), and polygons reaching less than 1e-5 into a face's swept
volume are misses, so float noise cannot turn a brick end face whose edge
runs under the box corner along a slope into a head-on stop. Elasticity is a
speed and is unscaled.

**Evidence.** `crates/sim/tests/player.rs`
`a_wedged_rider_gains_lane_speed_on_v20_ticks` (runs without content) wedges
a rider in a level lane of two 74.5 degree faces: 120 Hz steps settle at
6.518 u/s against 6.517 from iterating v20's equations, and 120 Hz ticks at
3.248. `crates/sim/tests/slides.rs` runs on the v20 Slate save
"Mr.Block's Slides" (ignored by default; needs converted content):
`the_tower_ride_runs_on_v20_ticks` checks that 120 Hz steps visit exactly
the positions of 32 ms ticks and that the ride from the top of the tower
falls 366 units in 10 s (top speed 96.7);
`no_72_degree_ramp_face_holds_a_player` drops a player on 3570 ramp faces
and none holds it; `slide_lanes_carry_a_player_down_hands_free`
(`BRI_SLIDES_FULL=1`) pushes a rider gently downhill into each of 893 lane
segments: 891 reach the end of their leg (889 at 120 Hz), the median rider
falls 42.5 units at a top speed of 22.5, and the longest ride falls 511.5.
Two stop where riders drop down a shaft and land a hair inside the next
lane's first ramp.

```powershell
cargo test --release -p bri-sim --test slides -- --ignored --nocapture
```

**Jump bookkeeping (2026-09-30, from a read-only disassembly of
blocklandv20.exe).** `jumpDelay` (3 ticks) runs down every tick, in the air
too (updateMove 0x5AFAC3); a jumpable contact reopens the jump only once it
has run out, and updatePos reopens it the moment a blocking hit's normal is
flatter than 0.8 (0x5B175B). So a held jump hops again on the first tick
after landing, and a bunny hop loses one or two ticks of run-force braking
per landing (about 3 u/s), not four: speed from a ramp launch carries across
hops. canJump (0x5A2AA0) also refuses after a hit whose list held a
ceiling polygon (normal.y <= -0.99, 0x5B16B9) until the next blocking hit
without one (`JumpState::ceiling`). Only a ceiling polygon the box's top
meets head-on counts: a move along a wall of stacked bricks grazes the upper
brick's underside edge-on at each seam, and counting that left the jump
refused on level ground, where no further blocking hit comes, until the
player jetted (2026-09-30). Its rising guard compares
horizontal speed (z zeroed at 0x5A2AC1) with 4. Above maxJumpSpeed the jump
and that tick's bookkeeping are skipped (0x5AF7AC). A jump in the air pushes
along the move as air control rewrote it (0x5AF4B5).

Not yet ported: v20's hard-landing recover state.

## Player archetypes

What a player is, is data (`bri_motor::archetype`). An `Archetype` holds the
motor constants (`PlayerTuning`, including the collision `body`, `box` or
`ball`, and the `steering` model), maximum health, the energy bar, whether
others may ride it and whether it may ride, and its look (a model id and a
third-person camera distance). The session's `Archetypes` table starts with
v20's eight datablocks in `PlayerType` order, so `PlayerType::archetype()`
is also the index; enabled packages append theirs (the `archetype` content
kind: a `base` archetype plus the constants it changes, merged by name and
checked by the motor). `PlayerState.archetype` is an index into that table.

- **One table, both sides.** The checkpoint carries the table, the replica
  validates it (v20's entries first, every entry valid, unique ids), and the
  client's `Predictor` moves the local player with the same constants as
  the server. A package's body predicts exactly like the Blockhead (E30).
- **Assignment.** A mini-game's player type may name any named archetype,
  v20's or a package's. A package script calls `set_archetype(player, id)`
  (capability `player`); that choice outlives death until the script clears
  it with `set_archetype(player, "")`, and the mini-game decides otherwise.
- **Controllers.** Steering models are engine mechanisms that packages pick
  by name, because clients predict them and receive no code: `strafe` (v20:
  face the look direction, strafe sideways) and `turn` (a vehicle: left and
  right turn the body at `turn_rate`, no sideways movement).
- **Driving another body.** A package hands a player one of its entities
  with `control(player, entity)` (capability `player`) and takes it back
  with `release(player)`. The player's `ControlObject` becomes
  `Entity(id)`: their moves drive the entity's body, which moves by its
  kind's `archetype`, while the avatar stands where it was. Only the
  package that owns the entity may hand it over, one player drives it at a
  time, and death, leaving or the entity's removal hand the player back.
  The client orbits the entity with its camera and records its inputs
  without predicting the avatar, as when seated; the entity itself moves at
  the server's entity rate, not predicted. Predicting a package-authored
  controller would need client code: the sandboxed tier-2 case.
- **Looks.** v20's Blockhead and horse draw as before; an archetype whose
  `model` is a package box model draws as that model (not animated), and
  third person keeps the archetype's `camera_distance`.

## Session authority

`bri-sim::session` owns the players and simulation. The QUIC transport resolves the
connection to its server-assigned owner ID; that ID is not accepted in `Command`.
Spawn positions and administrator privileges are server-side join arguments.
IDs start after the highest owner already present in the loaded world.

Commands cover movement, planting, editing, removal, activation and chat. Building
uses the player's authoritative feet and the placement validator; edits/removal
require an authoritative unobstructed eye ray, then ownership checks. The audit
corrected generic distances: wrench/printer edits use 10 units; hammer uses 5,
or5.5 looking nearly straight down; brick activation uses 5. Player scale and
exact muzzle offsets remain work, and spray painting requires a projectile rather
than this generic edit command. Ordinary wrench properties now use atomic batches
with inspection/revision checks, including a brick's item, music and vehicle.
Reliable actions may carry validated aim captured at dispatch; this selects the
ray direction without changing authoritative position, motion or body orientation.

Movement messages update intent without advancing time. The session advances once
per server tick and stops stale movement after half a second. Monotonic command
sequences reject replay. Per-second budgets currently allow240 movement messages,
60 actions and 4 chat lines; chat is bounded to 256 bytes per line and 100 retained
lines. These are working server policy values. Join count is bounded at 64.

Snapshots contain the authoritative world, player states/names and bounded chat.
An in-process late join receives exact serialized state. Socket checkpoints,
dirty-brick deltas and separately sequenced movement/poses now exist in `bri-net`.
See `networking.md` for bounds, tests and remaining integration. Disconnect destroys
the player body. Fresh joins get fresh owners; a valid secret can resume ownership
within the same host process. After a restart, a world's owner table gives a
returning player their owner number back from their durable principal.
Ownership enforcement on LAN deliberately modernizes stock v20's fully trusted LAN.

## Evidence and remaining acceptance

Five player tests verify directional movement/jump/landing, crouch clearance,
wall sliding/camera obstruction, jets, rejected input atomicity, step traversal,
idle height and touch entry. Five session tests cover two builders and ownership,
serialized late join, replay/stale connection rejection, rate and input bounds,
authoritative tool rays, chat bounds, converging players, and physical touch
driving a delayed event once.

```powershell
cargo run -p bri-sim --release --bin player_probe -- content/map-bundle-005 artifacts/native-player/integration.json
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The native-map probe passes at all original Bedroom/Kitchen/Slopes spawns: settle,
jump and land, crouch movement, jet rise and a finite swept camera. Bedroom and
Kitchen jump rises are about3.55 native units with the current motor. The report
records positions and clearances rather than treating successful loading as proof
of contact. These are scripted headless physics tests, with no visible game or
mouse/keyboard operation. That milestone passed52 tests; current whole-workspace
evidence is recorded in `progress.md`.

Movement and multiplayer feel are judged in Maxwell's interactive playtests.
