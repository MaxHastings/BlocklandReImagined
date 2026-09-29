# Torque and Blockland v20 player quirks

Audit date: 2026-09-27. The question is which Torque Game Engine (TGE 1.x)
player behaviours shaped how v20 feels, how each one works, whether this engine
has it, and whether it can be recreated faithfully.

## Evidence

- **TGE 1.x source.** `.research/openmbu-reference/mbg-player.cc` is the Marble
  Blast Gold `player.cc`, which is stock TGE 1.x. It covers `updateMove`,
  `updatePos`, `findContact`, `canJump`, `setActionThread`,
  `updateActionThread` and `pickActionAnimation`.
- **Blockland's own changes.** These come from a read-only `objdump` disassembly
  of `E:\...\Blockland v20\blocklandv20.exe`. Addresses below are image virtual
  addresses. Nothing was run, patched or copied into the repository.
  - `Player` vtable: 0x7335f4
  - `setActionThread`: 0x5a2e40 (slot 0x1dc)
  - `pickActionAnimation`: 0x5a2fe0 (slot 0x1e4)
  - `updateActionThread` tail: 0x5a6900
  - `canJump`: 0x5a2aa0
  - `updateMove`: crouch near 0x5ae2ea, jump near 0x5af765, jet near 0x5afb07
  - Action table: 0x775c20
- **Datablock values.** `PlayerStandardArmor` in
  `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs:8738`.
- **Sequence data.** The converted `m.dts` sequences in
  `content/avatar-rig-001/rig.json`.

Field meanings come from matching the disassembly against the TGE source. Where
a meaning is inferred, the entry says so.

### Key facts from the binary

- **Action table.** `root, run, back, side, crouchRun, crouchBack, crouchSide,
  fall, jump, standjump, land`. The table directions are run (0,1,0), back
  (0,-1,0) and side (-1,0,0). The crouch entries have zero direction.
- **No ground transforms.** None of the `m.dts` movement sequences carry a
  ground transform. So `getGroundInfo` falls back to the table: run, back and
  side get speed 1 with `velocityScale=false`, and the crouch clips get speed
  0. **Every locomotion clip plays at time scale 1**, and nothing is matched to
  movement speed.
- **Re-picked every frame.** `updateActionThread` still decrements
  `delayTicks` but no longer tests it. The move action is therefore re-picked
  every client frame, not every 4 ticks as in TGE.
- **Early return on the same action.** `setActionThread` returns early when
  the action index is unchanged. It does not compare the forward flag. A new
  action transitions over 0.25 s (0.15 s for jump actions) from position 0, or
  from position 1 when reversed.

## Quirks

Status key:
- **Has**: already matches v20.
- **Added**: implemented by this audit.
- **Partial**: we have something close, but not exact.
- **Missing**: not in our engine.
- **Routed**: owned by another thread and reported to it.

### 1. The humping bug: crouch pose thread re-entry snap. Added.

**Mechanism.** Blockland keeps a dedicated shape thread on the `crouch`
sequence (`Player+0x890`). Its rules:

- Starting to crouch, or holding crouch while the thread's time scale is not
  +1, calls `setSequence(crouch, 0)` and sets the time scale to +1 (0x5ae906).
  The body starts again from the standing frame and sinks over the 0.2 s
  clip.
- Releasing crouch only sets the time scale to -1 (0x5ae80c and 0x5ae894). The
  pose rises from wherever it currently is.
- When a reversed thread reaches position 0, `advanceTime` parks it on
  sequence 0, the empty root (0x5a6b10).
- Ghosts apply the same rules from the network crouch flag (0x5b2435).

So pressing crouch again while the pose is still rising snaps the body back to
standing, then sinks it again. Tapping crouch pumps the hips. The clip pitches
the torso forward about 60 degrees and drops the Hip from 0.69 to 0.24, so the
pumping reads as a hip thrust.

The Eye node is on the same clip (2.16 to 0.63), so the first-person view
dips and snaps along with the body.

Max described crouching, moving backwards, then tapping. Moving backwards
while crouched flips the action between `back` and `crouchBack` on every crouch
toggle, and each flip restarts that clip from frame 0 (see quirk 2). The two
restarts stack. Nothing in the binary makes the wobble continue with no input,
so I read "stuck" as describing the repeated tapping.

**Before this audit.** The avatar sampled `crouch` at its last frame whenever
the player was crouched. The camera eased the eye height exponentially. Both
snapped straight down and could not produce the snap back up.

**Now.** `crates/motor/src/crouch.rs` models the thread. The avatar samples
the crouch layer at the thread's position. The local first-person eye follows
the thread through the authored Eye keys. A player who is already crouched
when first seen starts fully crouched, as a new v20 thread does.

**Verification.**
- Unit tests are in `crates/motor/src/crouch.rs`.
- The capture test is `crates/client/tests/crouch_capture.rs`
  (`cargo test -p bri-client --test crouch_capture -- --ignored`). It poses
  the original rig and renders an offscreen sheet to
  `artifacts/torque-quirks/crouch-tap.png`. The sampled hip heights go to
  `crouch-tap-hip.tsv`.
- Sample from the hip log: sinking reaches 0.2445. Releasing for 0.1 s rises
  to 0.5434. Re-pressing jumps to 0.6660, one frame from standing, then sinks
  again to 0.2445.

### 2. Animation state flicker. Partial, routed to the diagonal walk thread.

**Mechanism.** Blockland's `pickActionAnimation` (0x5a2fe0):

- Root is chosen if the object-space velocity's full 3D length is below 0.4,
  including vertical speed.
- Otherwise it compares run, back and side using `vel . dir`. The starting
  maximum is 0.1, comparisons are strict, and the first match wins ties.
- Only side can play reversed. Exactly 45 degrees resolves to run or back,
  never side.
- The choice is made every frame. Any change of action restarts the new clip
  with a 0.25 s blend. Keeping the same action never restarts it, even when
  its reverse flag is stale.
- Near a boundary, noise in velocity flips the action frame to frame, and each
  flip restarts a clip. That is the v20 flicker.
- When the player is crouched, the chosen run, back or side maps to
  crouchRun, crouchBack or crouchSide.

**Ours.** `locomotion()` in `avatar.rs` uses a per-axis 0.1 root threshold. It
restarts on any mode change, with no blend. It is owned by the diagonal walk
animation thread, and I sent them the details.

**Faithful?** Yes. It needs the 0.4 3D gate, the first-wins comparison order,
the reverse-only-for-side rule and a 0.25 s transition blend.

### 3. Gates on the animation picker. Partial, routed.

- **Jetting forces root.** Flag `Player+0xa00` is set from move trigger 4 when
  `canJet` is set and energy is at least `minJetEnergy`. We have this.
- **Contact timer.** Contact means `mContactTimer < 2` ticks (TGE uses 30).
  Standing on a moving object also counts. Without contact, the root pose is
  used unless falling.
- **Falling** selects `fall`.
- **Water.** Coverage above 0.6, or above 0.01 without contact, forces root.
  Partial coverage while sinking faster than 0.1 also forces root.
- **First person.** For a controlling client in first person, side is replaced
  by run. Only that client ever sees it.
- **Held script animations.** A held script animation suppresses root and fall
  until movement cancels it. In `updateMove`, any movement input clears script
  actions and `land`. This is how emotes end when you move.

### 4. Jump rules. Added (except the 70 to 80 degree case; see below).

**Mechanism.** From TGE `updateMove` and `canJump`, and Blockland's `canJump`
at 0x5a2aa0:

- **Held jump rehops.** Jumping is level-triggered: it happens whenever
  trigger 2 is held and `canJump` passes. `jumpDelay = 3` ticks is set on each
  jump, and it counts down only on ticks with jumpable contact. Holding space
  hops again on the fourth contact tick after landing: the landing tick plus
  three.
- **Late jumps.** `mJumpSurfaceLastContact` must be below
  `JumpSkipContactsMax` (8). A jump is still allowed for 7 ticks (224 ms)
  after walking off a ledge.
- **Steep surfaces.** A surface is jumpable when its normal is within
  `jumpSurfaceAngle = 80` degrees of up, which is steeper than the 70 degree
  run surface. You can jump off slopes you cannot walk up.
- **Impulse.** The jump adds `jumpForce / mass = 12` along the surface
  normal's up component to the current velocity. When the move direction
  points away from the surface, it also adds `12 * dot` along the move. So
  jumping while running uphill carries the climb, and jumping downhill or off
  a steep face is weaker.
- **Fade at speed.** The impulse fades between upward speeds of 20 and 30
  (`minJumpSpeed` and `maxJumpSpeed`). There is no jump above 30.
- **Rising guard.** Blockland's `canJump` also refuses while rising faster
  than 3, unless the total speed is above 4. The second vector is read through
  a virtual getter; I infer that it is `getVelocity`.

**Before this audit.** Jumps were edge-triggered, allowed only while
grounded, and set vertical speed to 12.

**Now.** `Player::step` finds jumpable contact with a 0.035 downward cast (TGE
`sTractionDistance` plus our skin), then applies all of the rules above.
Converted to 120 Hz: jumpDelay is 12 ticks and the late-jump window is 27
ticks.

`PlayerState` carries `jump: JumpState` (delay, ticks since contact, last
normal), so prediction replays exactly. It replaces `jump_held`. This needed
net `VERSION` 16, which Max approved on 2026-09-27.

**Verification.** `crates/sim/tests/player.rs` covers:
- A held rehop on the landing tick plus 12.
- A late jump 20 ticks after walking off a ledge.
- A 25 degree ramp jump that adds `7 sin a + 12 cos a` upward.
- The lintel test, which now releases jump after the first hop.

The 70 to 80 degree case is implemented but untested. Our controller slides
off anything steeper than 70 degrees, so contact there is brief.

### 5. Crouch jets. Added.

**Mechanism.** Near 0x5afbcf: while jetting and crouched, the thrust is the
body's horizontal forward axis times `2000 / mass` per second (for mass of 90
or more). There is no lift, no move direction and no falling boost, so
crouch-jetting is a flat forward dash that still falls. Standing jets
normalise the move vector plus 0.7 up, or thrust straight up with no move,
which matches what we already had.

**Now.** `Player::step` thrusts along the facing when crouched. This is covered
by `crouched_jets_push_flat_along_the_facing_without_lift`.

### 6. Crouch speeds. Has.

**Mechanism.** Crouched speed is the larger of `maxSideCrouchSpeed * |x|` and
`maxForward` or `maxBackwardCrouchSpeed * |y|`, times the player's scale
(0x5aed92). Blockland drops TGE's underwater speed branch here. We use the
same crouch speeds.

### 7. Standing up under a ceiling. Has (head collision thread).

**Mechanism.** Releasing crouch builds the standing box and tests it (0x5ae315
onward). The box is extended further when moving up or holding jump. It
stands only if the box is clear. Ours checks standing clearance. The ceiling
work belongs to the head collision thread.

### 8. Collision response jitter. Partial.

**Mechanism.** TGE `updatePos`:

- It backs off 0.01 / |v| from each hit.
- It removes velocity along the hit normal plus `sNormalElasticity = 0.01`, a
  tiny bounce.
- On the second hit it re-aims velocity along the crease between the two
  normals.
- After 5 failed retries it cancels the move and zeroes velocity. This is the
  "stuck in a corner" stop.

On a run surface, `updateMove` pushes 0.002 along the contact normal each tick
so the player can rest. The box also steps up to `maxStepHeight`.

**Ours.** Rapier's kinematic controller, with explicit velocity removal. It
behaves similarly without the elasticity bounce or the retry-failure stop.

**Faithful?** Only by replacing the controller with a TGE-style swept box. The
visible differences are small. I recommend not doing it before the playtest.

### 9. Air control and falling. Has.

Air control, the horizontal and vertical resistance caps, and the fall
animation below -10 were matched by the jets and air-control work
(`player.rs`, `air_control_direction`).

### 10. Ghost prediction of remote players. Different by design.

**Mechanism.** TGE client ghosts replay the last received move for up to 30
ticks (`sMaxPredictionTicks`), then warp over up to 3 ticks toward server
corrections. Remote players visibly overrun and rubber-band after a brief tap.

**Ours.** Remote players are interpolated behind server poses, with bounded
extrapolation. Recreating the overrun would take the ghost path for remote
players. It is a netcode trait, not a feel choice, so I recommend not
recreating it.

## Follow-up

- **Diagonal walk animation thread:** items 2 and 3. Sent through the
  coordinator.
