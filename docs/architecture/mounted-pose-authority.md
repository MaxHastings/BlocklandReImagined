# Authoritative mounted body basis investigation

Original red diagnostic base: `19db868d38a4d334cb0c2a491fff9a60d0c34f0c`.
Design/fixture checkpoint: `20a84b1da07cd1fd085b5d085b70b42259c8fbc5` on
`codex/mounted-layer-design`. Stage A implementation branch `codex/body-layer-plan`
starts from clean mechanical root-shift checkpoint
`d614f46d367d9dc92e15bbc44d6359550df10760`, excluding the deliberately red tests. The Session red
fixture is retained; the scale/root helper prerequisites do not close it.

The actionable next-stage specification is **Concrete layer/fitting extraction**
below. Earlier broad tables are historical proposals, not production grants.
Owner: Coordinate shared engine seams; root assigns production paths. This note
preserves the diagnostic/design and records the separately granted Stage A
planner extraction. The deliberately failing ignored diagnostics remain on their
separate investigation branches and are not part of the Stage A integration
chain; they cannot be represented as passing an include-ignored gate.

## Recorded native comparison

`crates/client/tests/authoritative_mounted_pose.rs` at investigation checkpoint
42275af9 (extended by20a84b1) uses invented content through
the real loaders, AvatarMesh, WorldItems, Session, physics and weapon state machine.
The server receives static body grip registration and image barrels through the
same public setup calls used by HostSetup. It never receives client nodes, mounts,
muzzles or a client-selected firing origin.

The test physically walks/jumps onto the native car, verifies actual occupancy,
switches to passenger seat 1 through Session, and applies body scale through the
existing `setPlayerScale` brick event. It records the authoritative player,
vehicle pose/scale, mounted identity, image binding, shot origin and tick. Client
sampling uses that exact latest vehicle tick with no render interpolation or
extrapolation. The comparison is third-person held-mount presentation, not a
first-person eye-offset comparison. Session samples the shot basis before the weapon step emits its current-tick
shiftAway cue. The diagnostic records that cue and compares the pre-fire phase:
older authoritative action channels apply, current-tick post-fire cues do not
retroactively move the shot. This is not post-fire visible-animation parity.

| Case at the recorded base | Result |
|---|---|
| Unmounted stock, body scales 1/1.5 and yaw 0/.45 | Four comparisons, maximum error 2.385e-7 native units |
| Owned/missing selected seat clip × level/tilted seat × body scales 1/1.5 × passenger turns 0/.45 | Sixteen real mounted comparisons, errors .685120–2.557943 |
| Stock mounted, no new seat policies | Error .402705 at tick 339; retained as a separate red diagnostic |
| Real Session wall between posed eye and existing held-origin path | Mounted and unmounted controls use eye fallback and start before the actual wall |

The authored open cases record vehicle scale 1. The public host tooling
`Session::spawn_vehicle_at` fixes spawn scale to 1; no scaled vehicle is fabricated
by changing only the client record. Nonunit body scale is genuine authoritative
state. Broader vehicle-scale/animated-actor/action cases remain required follow-up
coverage, not claimed by this neutral diagnostic.

The receipt is generated at `target/authoritative-mounted-pose-evidence.json`.
Near-wall origins are assessed against obstruction policy separately from open
held-mount parity: a valid eye fallback need not equal the visible muzzle. Wall
colliders are part of Simulation from setup, and ordinary Session fire performs
the query; no empty WeaponsWorld query substitutes for that evidence.

The first wall attempt reconstructed a standing eye for a seated body. Correcting
the test to `PlayerState::posed_eye` with authoritative scaled tuning exercised
the existing wall fallback successfully. This is not an obstruction runtime fix.

## Current production path

1. `client/app/session.rs` and `net/dedicated.rs` obtain the stock rig's static
   `shape_grips`, package `body_grips`, and item `image_barrels`.
2. `net/content_identity.rs::package_body_grips` loads bounded native shapes via
   `body_rig`, resolves socket/role metadata, then discards the full rig after
   sampling idle + ready + level look. Package maps currently merge by model and
   preserve-hand flag, which also loses distinctions between role mappings on
   archetypes sharing that model.
3. `net/host_setup.rs` registers those maps using Session setters before joining.
   `sim/session/grips.rs::grip` selects the model/preserve/one-or-both hand entry.
   `muzzle` rotates that static frame by player yaw/pitch and shifts it by the
   posed eye delta. It does not sample selected rider_pose, ready contracts,
   fallback hip, seat tilt, seat aim, current action channels or seat hand holds.
4. Session step order applies vehicle input/physics/post-step, synchronizes seated
   eyes, then calls `step_weapons`. WeaponsWorld receives server-owned Frame data
   and performs existing near-eye/muzzle obstruction and aim correction. This is
   the authority boundary to preserve.
5. Client `app/frame.rs` derives the actual seat frame before passenger turn,
   mounted images, fallback world hip and current animation inputs. `avatar.rs`
   resolves the active owned seat clip and RoleClips fallback, composes existing
   priorities/actions/look/seat aim, samples native channels, anchors fallback
   hips, then performs existing fitting/support/seat IK. `mount_socket` supplies
   WorldItems from this same sampled mesh. Image mount correction and muzzle
   nodes remain in ItemAssets/WorldItems.
6. `Session::play_thread` and weapon Animation events currently emit cues; the
   host does not retain an equivalent authoritative composed body-thread state.
   A shared sampler alone cannot invent those missing playback inputs.

## Smallest shared helper proposal

Extract the mounted branch of the existing composer into a pure helper in
`bri_content`, using the existing Rig, RoleClips, Layer and native sampling/hold
functions. Do not create another animation graph, VM or parallel body framework.
The client and host must call the same selection, sampling, anchor and socket
rules rather than copying the current client code into Session.

Proposed conceptual API (names require root review):

```text
MountedPoseInput {
  rig + resolved body socket/role/fitting policy,
  server tick / explicit playback clocks and existing transition state,
  selected seat clip + exact resolved-ready eligibility,
  seat frame before passenger turn + passenger turn + rider scale,
  optional world fallback hip, look / seat-aim phase,
  authoritative body/action/gesture channels and applicability,
  support and world seat hand targets
}
sample_mounted_pose(input) -> MountedBodyBasis {
  model transform, sampled nodes, eye/socket frames,
  existing hand fitting corrections / action deltas
}
```

The helper returns body/attachment bases, not a mandatory firing origin. Reuse
the vehicles schema's existing exact-image matcher and `covers_resolved` before
calling it; pass an eligibility predicate calling `SeatReadyMatches::covers_resolved` with
the helper-selected Role and RoleClips rather than introducing a content
dependency on vehicles. Preserve native priority sorting, transition semantics,
ready fallback, authored root and missing-clip anchor algebra. Reuse existing
hold fitting/reach operations and extract the thin orchestration needed for host
parity; do not claim parity while leaving client-only IK or hand correction out.

Host registration must retain immutable native rig data and each actual body
definition's roles/sockets/fitting identity. A `(model, preserve)` key alone is
insufficient when different archetypes author different roles. Retain bounded
loading and a deliberate cache identity/lifetime; do not add textures or Torque
readers to the game runtime. Thread inputs must come from existing authoritative
thread/weapon events, with spawn/equip/body/death/dismount invalidation and a
defined pre-fire sampling phase. Never use local render completion to mutate
gameplay. Client presentation may sample between ticks using those same rules;
the diagnostic compares exact fixed ticks.

## Proposed production reservations, not yet granted

| Paths | Purpose |
|---|---|
| New `crates/content/src/body_pose.rs`; `content/src/lib.rs` | Pure mounted layer selection/sampling, root/hip basis and socket composition using existing primitives |
| `crates/client/src/avatar.rs` | Delegate the mounted branch and existing fitting orchestration; keep mesh/GPU ownership here |
| `crates/vehicles/src/world.rs` | Expose the existing authoritative effective seat transform/scale as a read-only basis; no alternate physics pose |
| `crates/sim/src/session/grips.rs`; new `session/body_pose.rs`; required module/state fields in `session.rs` | Register immutable body pose assets; sample actual mounted server state and apply only the existing declared held-origin path |
| `crates/sim/src/session.rs::play_thread`; `session/weapons.rs` animation event handler | Retain authoritative playback state at its existing producers, with explicit timing/invalidation; do not depend on dropped cosmetic cues |
| `crates/net/src/content_identity.rs`, `host_setup.rs`, `dedicated.rs`; `crates/client/src/app/session.rs` | Reuse existing loaders to register full bounded pose inputs for dedicated and local hosts; constructor propagation only as discovered |
| Existing diagnostic plus focused shared-helper/Session tests | Turn open red comparisons green without changing their measured origins; retain genuine wall fallback and unmounted controls |

Root should reserve these in small sequential checkpoints after reviewing the
input identity/clock ownership, rather than treating this table as permission.
Actual constructor paths and additional public APIs must be coordinated first.
Animated actor mount frames need their existing authoritative sampling owner;
using a static node in place of an unavailable live frame is unsupported.

## Gameplay-origin policy is separate

Existing declared held-mount semantics are the subject of the numeric parity
diagnostic. They do not establish a universal original-game firing rule. AA's
committed private crosswalk `29c107889684e00f49ef02998a52b90ab54ffb39` distinguishes
EyePosition plus aim-relative normal/prone offsets from WeaponBone cosmetic
emitters. Its native constructor default, obstruction and Session adapter remain
open; its WeaponsWorld Frame test is not a Session test. Halo's committed firing
and aim crosswalk is also required before choosing any new origin policy API.
FirstPersonOrigin remains presentation metadata. Do not infer gameplay policy
from it, force every shot to the eye, or force every game to the rendered muzzle.

Root reviews the concrete proposal and red/control evidence before any production
implementation. No main merge, full gate, original fidelity or release readiness
is claimed by this investigation.

## Separate confirmed static seat-scale defect

Halo private checkpoint 156dce182ef66f801613414583444c9f663e8d4c,
WARTHOG-PASSENGER-FALLBACK-HANDOFF.md and passenger_fallback_inspect.rs,
consumes exact 19db868d. Its 84 native-current-frame comparisons pass, but 24
independent source-frame comparisons fail at nonunit vehicle scale (maximum
1.7608795 native units). The private point is not copied into public fixtures.

The generic equation is seatWorld = vehiclePosition + vehicleRotation *
(vehicleScale * seatLocalPosition). ClientVehicles::seat_transform currently
omits vehicleScale on this local translation; seat_fallback_hip scales only the
additional point. The authoritative vehicles/world.rs seat_pose uses local_pose
with spawn.scale; effective_seat_pose also scales turret-pivot and broken-seat
translations. This is a distinct client basis defect, not a justification to
change authored hip metadata or gameplay firing-origin semantics.

Root granted a separate narrow client seat_transform correction after this atomic
investigation, comparing the actual authoritative effective seat pose at vehicle
scales .5/1/2, unequal rider scales and rotation/tilt. Preserve animated mount-node
scale, attachment/turret behavior and default scale 1. Halo must rerun its exact
consumer at the correction's SHA; the current real Session fire diagnostic uses
vehicle scale 1 and cannot close enlarged-vehicle coverage.

## Completed mechanical extraction (d614f46d)

The earlier matrix-input prototype below is historical. The granted d614f46d
implementation uses raw feet/rotation/scale to preserve invalid-input eligibility
exactly. Stage A below builds on that completed prerequisite.

After the scale fix, propose only new content/src/body_pose.rs + its lib.rs
export, client/avatar.rs delegation, focused pure/native regression and docs as
the first implementation checkpoint. No host registration or new clocks yet.

Extract the existing avatar.rs seat_shift block (selected active seat clip's
owned root versus missing-clip hip retarget) into a concrete pure function:

```text
seat_root_shift(shape: &Shape, sampled: &Pose, owns_clip: bool,
                fallback_world_hip: Option<Vec3>,
                rider_model_frame: Mat4) -> Option<Vec3>
```

The caller retains the current active mounted sit/root/dead guard. Owned clips
return zero shift; missing clips find the native sampled hip and transform an
explicit world point through the inverse valid rider frame, retaining the
existing absent-point legacy bound. AvatarMesh::finish_pose consumes that shift
as today. Existing selected clip/ready contracts and fitting remain unchanged
in this first move. It is a mechanical shared basis prerequisite, not a claim
that the red host diagnostic is fixed. Exact function signature/validation must
be reviewed against existing legacy behavior before granting edits.

Then separately reserve canonical mounted role/layer selection and existing hand
fitting orchestration, followed by immutable host rig registration keyed by body
identity and host playback state. The state owner is Session, populated at its
existing play_thread and weapon Animation producers, invalidated by actual spawn,
body/equipment/death/mount transitions. Sampling belongs after sync_seated and
before step_weapons constructs its frames. Current-tick firing animation emitted
after that construction must not retroactively alter the shot. Client transitions
remain presentation state; existing clock semantics must be extracted explicitly
rather than replaced with a new fixed blend. Each stage needs its own exact path
reservation, controls and still-red/then-green diagnostic evidence.

## Concrete layer/fitting extraction

Root accepted and separately granted Stage A after the design/fixture checkpoint.
Only content/body_pose.rs planning/types and Avatar's layer-block delegation are
implemented in that atom; its matching regressions first passed against the legacy
block. Stages B�D remain proposals requiring their own exact reservations.

### Stage A: plan existing layers, retain existing mutable state

Exact next production files: `content/src/body_pose.rs` for `plan_body_layers`
and plain input/output types, and `client/src/avatar.rs` for delegation of the
existing layer-planning block. Existing native sampler and root-shift helper are
used, not replaced. No manifest, host, fitting or clock file is needed for A.

Concrete Rust-level borrow shape:

```rust,ignore
struct ActionRef<'input> { sequence: &'input str, started_at: f64 }
struct LayerPlanInput<'rig, 'input> {
    base: Option<Layer<'rig>>,                 // already selected/clocked
    ready_role: Option<Role>,
    omit_ready: bool,                         // actual owned-seat + matcher
    crouch: Option<Layer<'rig>>,              // caller updates CrouchThread
    actions: &'input [Option<ActionRef<'input>>],
    time: f64,
    head_up: bool,
    seat_aim: Option<f32>,
    look_phase: f32,                          // current look_position caller
    head_yaw: f32,
}
struct BodyLayerPlan<'rig> {
    layers: Vec<Layer<'rig>>,
    unacted: Vec<Layer<'rig>>,
    transition_at: usize,
    unacted_transition_at: usize,
    acted: bool,
    skipped: Vec<String>,
}
fn plan_body_layers<'rig>(
    rig: &'rig Rig,
    roles: &RoleClips,
    input: LayerPlanInput<'rig, '_>,
) -> Result<BodyLayerPlan<'rig>>;
```

Every returned Layer borrows only an Animation owned by that exact Rig. The plan
owns its vectors and skipped lowercase names; it does not borrow temporary action
names or RoleClips strings. The caller keeps the Rig alive through sampling and
cannot retain a plan past asset replacement. The base/crouch layers must originate
from that Rig. No global interning, render references, PlayerState or vehicle
crate dependency enters content.

The caller adapts its current two body slots, arm action and gesture, in that
order, into the borrowed optional-action slice. **Four is the existing adapter
shape, not a new planner cap.** Keep the sampler's existing 32-layer validation;
do not silently drop extra input actions or add a new action-count limit.

Resolution uses the actual RoleClips produced for this BodyKind and rig: known
Blockhead names resolve through Role::from_blockhead_clip and RoleClips::clip,
otherwise through Rig::sequence. Ready samples the resolved clip at its end.
RoleClips missing-role provenance remains the source-ready matcher input; a
Both request falling back to Right needs explicit Right coverage. `omit_ready`
is derived at the existing client caller from active owned seat clip plus
SeatReadyMatches::covers_resolved, after matching the entire mounted image set.
The content planner cannot infer it from clip names, preserve_hand_pose or a
Both-hand flag.

Exact order to preserve, not replace with a generic priority sort:

1. Base, optional ordinary ready, optional already-clocked crouch.
2. Build absolute/additive action lists in input order. Skip lowercase root;
   record missing names. Absolute actions loop modulo positive duration or clamp
   to their final key; additive actions retain existing unbounded played-time
   input to the native sampler. Nonlooping absolute actions keep their final key.
3. Clone the pre-action stack for unacted. Stable-sort each current base stack
   by `(additive, priority)`; extend acted with absolute actions and sort it by
   that same key. This preserves stable order for equal priorities.
4. Append head-up, then SeatAim at clamped phase if finite/resolved, otherwise
   Look at caller-computed look_phase; append HeadSide at its existing normalized
   head_yaw phase. Append these to acted and unacted without a new priority sort.
5. Append additive actions only to acted, then stable-partition both complete
   stacks by additive flag only. An additive action's low priority must not move
   it ahead of look just because it has a smaller number. Absolute/custom look
   moves before all additive deltas, retaining order within each category.
6. For an absolute base, each transition boundary is immediately after that exact
   Animation pointer in that stack. For additive/no base it is the first additive
   layer or stack length. Compute **both** indices: absolute actions can change
   the acted index without changing the unacted index. Do not blend a final world
   pose or substitute one boundary for the other.

NodeTrack's nonempty translation/rotation/scale/scale-rotation arrays are the
existing absolute component masks: absent channels preserve earlier values.
Additive defaults are identity/zero and accumulate locally; they are not a new
full-body pose. Object tracks retain current visibility/mesh/material semantics.
No source reload mask is reconstructed from a body's name. The sampler already
implements these semantics and hierarchy evaluation.

The current `named` resolver remains needed outside the moved block for missing
reference tool actions and the hand-action support guard. Keep those uses at
Avatar in A, with identical role/name resolution; moving their orchestration is
Stage C, not a reason to expand A into tool or fitting changes.

### Mutable-state inventory and exact caller boundaries

| Existing owner/function | Reads and mutations | A treatment |
|---|---|---|
| AvatarMesh::roles | Actual BodyKind animations/stand_in and actual for_mesh Rig; caches Arc<RoleClips> | Remains client-owned, passed by reference; never resolve only by model |
| pose_with_animation before layer block | validates input; clears skipped/tool-only deltas; updates last_time; chooses scripted/locomotion/seat clip | Remains client-owned, active-seat/dead guard unchanged |
| locomotion state | mode, forward, phase, stood_at, elapsed clamped to .25; movement tick/body scale and time_scale | Remains unchanged; no clock/tween/rate API introduced |
| next-mode transition | takes old Channels into transition, resets phase/direction only when mode string changes | Remains unchanged, including current same-mode/new-clip behavior |
| crouch block | updates CrouchThread from current crouched flag, elapsed and resolved duration | Runs before plan; produces optional Layer rather than moving state |
| planned actions/overlays | named clips, existing action timestamps, current head/seat/look values | Pure plan output; skipped diagnostics returned without actor mutation |
| sample/transition block | .25 normal or .15 jump expiry, frozen Channels/weight, last channels replacement | Remains Avatar; feeds plan's two indices to existing sampler |
| unacted sampling | only when acted and wants_unacted, otherwise clears unacted_nodes | Same conditional and lifetime; no second sample for every remote body |
| post-sample | shared seat_root_shift, Fits setup, support guard, finish_pose | Unchanged in A |

Current production callers of pose_with_animation: App frame's player loop
(app/frame.rs), horse/actor mount meshes (app/mounts.rs), Avatar preview rendering
(avatar.rs::Preview::render), and AvatarMesh::pose default-input wrapper. Preview
supplies thread-1 run at its existing .85 clock and must retain it. Direct
vehicle_camera calls shown by search are in that file's test module, not a new
production owner. Tests/tools call the same public methods. The planned block has
one direct implementation caller in pose_with_animation; finish_pose and fit_hands
also each have one direct caller in that chain. Roles additionally serves fits
and body reporting; Fits additionally feeds mount/socket/first-person helpers.

Client clocks are not server clocks: app/frame increments animation_time by
min(game_elapsed,.25), app/fx queues cues with client animation start times, and
app/avatars applies image-scoped/waiting cue policy. Action.started_at is in that
same monotonic domain. No `tick/120` replacement belongs in A. AA's separate
initial tween, channel envelopes and phase-preserving rate updates remain missing
mechanisms with their existing owners; do not introduce approximations here.

### Stage B: pure fitting orchestration, separate reservation

Proposed production paths only `content/src/body_pose.rs` and `client/src/avatar.rs`,
with pure/native tests and docs. Move the existing private Fits payload to a
plain shared BodyFits: first_person[2], hands[2] of existing hold::Fit, and legacy
seat offset. Keep Arc cache ownership in Avatar. A pure build_body_fits takes
Rig, actual resolved roles, fitted/stand_in/preserve flags, hand indices, eye
landmark and reference seat hip. It performs exactly the current idle + own-hand
ready-at-end + level-look sample, stable ordering, existing hold::fit, and legacy
seat-hip delta. Do not replace this with sim::body_grips, whose input/resolution
and returned information are currently different.

Then a pure apply_hand_fitting receives sampled mutable node matrices, native
parents/hand indices/eye landmark, BodyFits, right socket offset and the current
Holds inputs. Preserve early exits and this order:

1. Generic raises for held hands with an existing fitted raise and no seat hold.
2. Left support from the right hand only when current support is enabled, the
   right fit exists and the left seat hold is absent. Use the same corrected or
   raised right frame and image-derived support point, not a guessed grip.
3. World seat holds right then left after model/root transform is determined,
   transformed by its finite inverse. This applies even when preserve_hand_pose
   disables generic fitting. It is currently in finish_pose outside fit_hands;
   do not accidentally put it behind the fitted-hands early return.

Support enablement currently depends on HeldToolPose::Both and **thread-2** named
hand-action liveness (looping or elapsed < duration). A nonlooping absolute action
may still hold its final channels while this support guard re-enables. Preserve
that behavior rather than inventing a source completion/unlock rule. Missing
limbs/indices remain explicit skipped operations. Keep geometry/GPU/pending-pose
work in finish_pose/build_mesh; do not move renderer structs into content.

### Stage C: socket/tool deltas, separate reservation

Proposed same two production files plus focused native WorldItems tests. The
current foreign-tool fallback samples only the reference rig's arm-ready/action
socket delta if a named non-role action is missing from the body's rig. It never
borrows reference body deformation. Preserve these per-hand tool_only_actions,
hand_turn correction, fitted held_hand rotation, socket offset and multiplication
order from mount_node/mount_socket. mount_action's acted/unacted data and current
correction order must remain exact. first_person_hand flags/offsets stay
presentation data; the authority must not derive gameplay policy from them.
This stage needs the reference Rig and native mount indices explicitly, rather
than reaching back into AvatarAssets or a renderer from content.

### Identity, lifecycle, and absent authoritative producers

Body identity for shared caches is immutable Rig generation/node layout plus
actual role/socket/fitting policy. Model string and preserve flag are insufficient.
Current set_body_kind compares the full BodyKind, invalidates roles and fits,
and resets only crouch for composition/stand_in changes. It does not reset phase
just because a role alias changes. App replaces a changed model's mesh; same-model
outfit rebuild calls continue_animation, copying mode/direction/phase/last time,
Channels/transition/stood_at/crouch/body generation but not graphics or old fit
caches. set_body on a new spawn clears those animation histories; App separately
clears external action/thread/equipment identities. Keep these distinct contracts.

Client action eligibility already exists at app/avatars: threads 0/1/3 with no
image hand are free channels, root stops them; thread 2 tracks whole sorted
mounted-image identity, cancels on equipment changes, waits for an image hand
up to the existing .5s cue age, and drops counted expired cues. These are current
client adapter semantics to preserve, not an authoritative producer state source.
Actual image thread0 restarts the item sequence in app/frame rather than being
silently treated as a body thread0. Host replacement must observe original
producers and equipment identities, not replay that lossy client queue as truth.

Stage D requires a separate root design/grant for net content_identity/host_setup/
dedicated, local host app/session registration, sim session/grips and new body
pose state, session play_thread and weapon animation producers, and vehicle basis
access as needed. Required inputs are full bounded immutable body rigs, actual
body roles/sockets/fitted identity, per-actor spawn and mounted/equipment identity,
server-owned active thread names/start/order, current motor/seat/aim/holds and
reference item offsets. Already available state is not permission to invent the
missing clocks, event lifecycle, source grid mapping or gameplay-origin policy.

The host currently emits action cues without retaining equivalent composed pose
state. It samples weapon frames after sync_seated and before this tick's firing
Animation events. Its sampling phase must be explicit; current-tick post-fire
animation must not retroactively alter the shot. Preserve the actual Session red
fixture as the eventual integration target. A/B/C are shared prerequisites and
will not be labelled authority closure while host asset/state inputs are absent.

### Design fixture checkpoint

The retained actual Session diagnostic still boards physically, switches seats,
fires ordinary commands and uses real wall queries. Four added native
characterizations specify the mechanical contract without replacing that red test:
rotation-only absolute actions preserve translations, additive deltas retain exact
order and unrelated channels; same-model role mapping changes invalidate cached
RoleClips; a body loaded through the real fitted-body manifest preserves authored
hands separately from generic support/raise and world seat holds; and outfit
rebuild preserves transition snapshots while a new spawn clears them. Their
results and retained red/control runs are in the new dated progress entry.


Stage A refinement accepted by root: look_position(pitch, limits) stays at its
existing Avatar implementation and unit callers. Pass its normalized look_phase
into the planner, which selects SeatAim versus Look and multiplies the selected
clip duration. Matching regressions must include noncentral/clamped look phase
and differing selected durations. No second look mapping API or source law.
