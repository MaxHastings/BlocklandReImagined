# Player and session integration

`bri-sim::player` is a fixed 120 Hz motor used by the server session. Inputs
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

Eye heights 2.4/0.85, gravity 20, jet thrust 35, horizontal
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
to authoritative poses. Animation transitions, movement-rate matching, tools/emotes,
effects, vehicle mounting and camera polish remain work.
The third-person camera sphere-sweeps backward from the eye. The native client
now queries map geometry and replicated authored brick collision shapes, including
shapes extending beyond the placement footprint. Visibility and raycast toggles
do not override collision. Original tilt/vertical offsets, smoothing and dynamic
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
walk onto low ramps by stepping onto their vertices as in v20.

**120 Hz against Torque's 32 ms tick.** Per-tick epsilons are rescaled so
behaviour per second matches: the 0.002/0.0021 rest values and the 0.01
back-off after each hit scale by 120 Hz / 31.25 Hz (unscaled, a rider
grinding along a lane loses 3.84 times the distance per second and stalls).
`EqualEpsilon` is kept as the same absolute distance, so it is smaller for a
face to lead a move and larger as a fraction for two hits to tie. All hits
within the tie count (Torque's running comparison made near-ties depend on
polygon order), and polygons reaching less than 1e-5 into a face's swept
volume are misses, so float noise cannot turn a brick end face whose edge
runs under the box corner along a slope into a head-on stop. Elasticity is a
speed and is unscaled.

**Evidence.** `crates/sim/tests/slides.rs` runs on the v20 Slate save
"Mr.Block's Slides" (ignored by default; needs converted content):
`no_72_degree_ramp_face_holds_a_player` drops a player on 524 ramp faces and
none holds it (before: every one did);
`slide_lanes_carry_a_player_down_hands_free` pushes a rider gently downhill
into each of 893 lane segments: 889 reach the end of their leg, the median
rider falls 46 units at a top speed of 25, and one ride goes from the top
of the 545-unit tower to the ground. The four remaining stop at one spot
where riders drop down a shaft at over 20 u/s and land a hair inside the next
lane's first ramp.

```powershell
cargo test --release -p bri-sim --test slides -- --ignored --nocapture
```

Not yet ported: canJump's refusal right after a ceiling hit (v20 0x8A2) and
v20's hard-landing recover state. Both need new `PlayerState` fields and so a
protocol bump.

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
with inspection/revision checks. Item, sound and vehicle properties remain pending.
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
within the same host process. Restart persistence and LAN discovery remain work.
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

Still required: full windowed client integration, original animations/customization,
tool/UI flows, original sounds/effects, terrain streaming and missing environment
objects, mounted vehicle interaction, network integration and adversarial/load checks,
packaging, and Maxwell's eventual interactive fidelity assessment. In particular,
the complete movement and multiplayer acceptance boxes remain unchecked.
