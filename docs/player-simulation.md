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

v20 `Player::step` (0x5A9FD0) gathers static polygons in the box at the move's
destination extended upward by `maxStepHeight`. It picks the highest vertex
below `maxStepHeight` that has no other vertex within the player's own height
above it. Only the player's height must fit above the step, not
`maxStepHeight` of extra headroom. The motor mirrors this in `v20_step`.
Rapier's controller autostep required a full 1.0 above the head, so players
stopped at plates and bricks beneath ceilings that v20 players walk under.
Eye heights 2.4/0.85, gravity 20, ground snap 0.2, jet thrust 35, horizontal
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

During testing, applying tiny downward gravity steps while already grounded could
gradually sink the controller into its floor. Grounded motion now holds zero
vertical speed and lets the controller determine continued support. Airborne
gravity starts when support is lost. The final collision shape remains a box;
brief capsule/rounded-box experiments did not resolve the underlying update issue
and were discarded. A 2,400-tick regression verifies stable idle height and one
touch-entry event. Contact callbacks and near-surface contact checks together
allow touch events while stationary without firing them every tick.

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
- **Not yet.** The client draws v20's Blockhead and horse; package box
  models on player bodies are not built.

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
