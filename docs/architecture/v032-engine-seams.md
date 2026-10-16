# v0.3.2 engine seams: ownership and first extraction

Owner: v0.3.2 engine-seam chat, branch `codex/v032-engine-seams`.
Immutable starting point: `3117e8c36c460f20bfc571700108890a9d4b7ad4`.
Initial read-only production audit committed as `d644c0e9`. Root subsequently
granted Stage B only: `crates/content/src/body_pose.rs`,
`crates/client/src/avatar.rs`, tests inside their existing unit modules and own
docs/seam rows. Stage B is implemented with passing native characterization, quick and scoped
focused checks;
all subsequent reservations below remain proposals.

## Boundary and dependency order

Use only `BlocklandReImagined-worktrees/v032-engine-seams` for mutations/builds.
The primary release checkout and `shared-engine-seams` are read-only here.
v0.3.1 is frozen; root owns its integration, checks and authorized publication.
Halo and AA private fidelity are active for v0.3.2; GMod remains parked. This
assignment supersedes older parking/publication language in historical notes.
Do not change manifests/lockfile, add dependencies, publish or create agents.
Original installations and both delivered Halo previews remain unchanged.

1. Extract existing fitting (Stage B below), independently of pending Stage1.
2. Extract socket/tool calculations (Stage C, separate reservation and atom).
3. Register full immutable body inputs and compose host pose using real clocks
   and pre-fire state. Coordinate exact sim/net/client setup paths with root.
4. Deliver truthful package completion and active client playback in separate
   Stage2 atoms after immutable Stage1 is available. Source endpoint/tween and
   outcome policy require explicit private evidence, not clip-length guesses.
5. Route demonstrated authored-prone and posed eye/fire requirements, plus
   vehicle contact/material/frame requirements, through separate concrete
   designs. No general animation graph or game-name engine branch is proposed.

Stage1 in the shared checkout owns catalog/admission/generation/provenance,
native views and producer lifetime work. It is not a completion callback,
durable replication, channel15 client adapter or original AA timing solution.
Its dirty files are not a dependency to copy. Root's sampler `b37b280f` and
integration candidate `f554` are not implied ancestors or passing candidates of
this lane. A fixture correction awaiting root validation is not an engine fix.

## First production atom proposed: existing fitting only

Exact Stage B production paths granted:

- `crates/content/src/body_pose.rs`: plain fitting inputs/results and pure
  orchestration using existing `hold::{fit,reach,sit_hip}` and native sampling.
- `crates/client/src/avatar.rs`: replace private Fits payload and delegate
  `fits`, generic arm/support fitting and the world-seat-hold loop.

Tests stay in those files' existing unit modules, using invented native rigs
and the existing source-free `own_body` loader fixture. No lib export is needed:
`body_pose` is already public. No host, weapons, package-runtime, vehicle,
wire, clock, renderer or manifest path is required for this atom.

Approved Stage B API (implemented with the following borrow shape):

```rust,ignore
pub struct BodyFits {
    pub first_person: [bool; 2],
    pub hands: [Option<hold::Fit>; 2],
    pub seat: Option<Vec3>,
}
pub struct BodyFitInput<'a> {
    pub rig: &'a Rig,
    pub roles: &'a RoleClips,
    pub fitted: bool,
    pub stand_in: bool,
    pub preserve_hand_pose: bool,
    pub hands: [Option<usize>; 2],
    pub eye: Option<usize>,
    pub reference_seat_hip: Option<Vec3>,
}
pub fn build_body_fits(input: BodyFitInput<'_>) -> BodyFits;

pub struct BodyHolds {
    pub raised: [bool; 2],
    pub pitch: f32,
    pub support: Option<Vec3>,
    pub seat: [Option<Vec3>; 2], // world space, right then left
}
pub struct BodyFittingInput<'a> {
    pub parents: &'a [Option<usize>],
    pub hands: [Option<usize>; 2],
    pub eye: Option<usize>,
    pub fits: Option<&'a BodyFits>,
    pub right_socket_offset: Vec3,
    pub model_transform: Mat4, // already includes root/seat/posture composition
    pub holds: BodyHolds,
}
pub fn apply_body_fitting(nodes: &mut [Mat4], input: BodyFittingInput<'_>);
```

Preserve existing permissive failures: a failed per-hand reference sample or
missing limb leaves that fit absent. Keep the legacy seat delta finite/length
guard, stable idle/own-hand-ready-at-end/level-look layer order and actual
resolved role aliases. `preserve_hand_pose` disables generic raises/support,
but retains first-person fallback eligibility and explicit world seat reaches.

Within apply, generic fitting's early returns must not suppress the independent
seat loop. Keep generic raises, right-hand corrected/raised support basis, left
support reach, then world seat reaches **right before left**. Preserve finite
pitch fallback, parent-count check, finite inverse of the completed model frame,
finite target checks and the other-hand/eye landmark fallback. The current root,
posture and seat model-frame construction remains at Avatar's existing site.

Avatar retains Arc cache ownership, full BodyKind equality/invalidation, actual
for_mesh rig and mount resolution, animation histories, channels/transitions,
pending meshes and GPU resources. No `(model,preserve)` cache key is introduced.
Thread2 support eligibility stays at its current caller: Both-hand hold plus no
live named thread2 hand action. At its nonlooping endpoint, support can return
while the action still holds final channels. This is not a completion outcome.

## Acceptance before a fitting commit

Add source-free characterizations before moving production and run them on the
unchanged implementation. Then run the same expectations after delegation:

- Fitted/stock/stand-in and preserve-hand variants; actual role aliases sharing
  one model; changed sockets/body definition invalidate cached fits.
- Generic raise and support preserve the corrected right-hand socket offset;
  authored hand matrices survive generic fitting, while explicit seat holds
  still reach. Missing limbs/eye and malformed parent counts remain bounded.
- Both world seat holds under tilted/turned, translated, nonunit model frames;
  distinguish inverse completed root/seat frame from inverse raw player frame.
  Keep the original right-before-left loop and exercise finite-input exits.
  On validated tree rigs, the other-hand landmark isolates disjoint arms; the
  two reaches need not numerically distinguish reversed iteration. No claim of
  order-sensitive source evidence follows from those fixtures.
- Native Avatar thread2 before/end/after and looping cases: final-key hold and
  support liveness remain distinct. Existing acted/unacted transition behavior,
  same-model role invalidation, outfit continuation and spawn resets survive.
- Existing socket matrices and action deltas are unchanged controls. No change
  to `mount_node`, `mount_socket`, `mount_action` or their multiplication order
  belongs in B. Record exact targeted results, scoped quick after edits and
  focused before commit; root runs the integrated candidate gate later.

This atom cannot close the deliberately red mounted Session authority diagnostic
or AA dynamic eye/prone counterexamples. Retain those evidence requirements.

## Following atoms and demonstrated requirements

Stage C requests the same two production paths later. It needs explicit native
reference rig/mount inputs for foreign-tool fallback, plus pure socket helpers.
Preserve node/correction/offset/tool-delta multiplication order, raised hand
orientation and acted/unacted inverse order. Keep first-person placement as
presentation. Do not borrow reference skeleton deformation into a foreign body.

Host integration needs actual body identity, immutable rig/node layout and roles,
sockets/fitting policy, authoritative seat frame, life/body/equipment/mount
generations and retained playback state. Sample after sync_seated, before weapon
frame construction; current-tick fire cues cannot retroactively move the shot.
Request exact paths before touching sim/session, weapon runtime, net setup,
client app/session or package-runtime. Source firing policy remains independent
of displayed muzzle and first-person origin metadata.

Private evidence was read with pinned `git show`, without changing its checkout:

- `fbdd03ead1c943229a34de68b0a2e5463ef7c094`: Halo passenger composition and
  fallback handoffs. Ready contracts, sparse replacement channels and scaled
  seat correction have bounded evidence; two-axis passenger aim and host
  mounted shot parity remain open.
- AA `49104a6`: source actor placement has 18 independent native positive cases;
  ordinary prone adds a second rotation (up to 1.698112 native units), and
  scalar host eye cannot carry CameraBone (up to .491854). These require explicit
  authored posture and authoritative pose/eye mechanisms, not offset fitting.
- AA reload notes at pinned `fbdd03ea`: original M16 visual sample span140/30
  versus dedicated completion141/30, independent pawn15 .05 tween, natural end
  versus polling/stopped/replaced outcomes. The full-clip Stage1 duration is not
  an original completion contract. Request Stage2 against its immutable SHA.
- Halo `2006726`: 64 tire/chassis material rows and25 effects require actual
  contact role/material/normal/motion and source-backed timing/full frames.
  PR50 hit resource `77356b7` is unbound with12 strict nonplanar mismatches;
  PR34 remains incomplete. Neither is an engine acceptance shortcut.

Root routes consumer assignments and immutable engine SHAs to Halo/AA chats.
No host authority, original-game fidelity, release closure or full gate pass is
claimed by Stage B. Its consumer-ready scope is only the tested pure fitting API.
