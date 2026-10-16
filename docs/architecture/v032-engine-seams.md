# v0.3.2 engine seams: ownership and first extraction

Owner: v0.3.2 engine-seam chat, branch `codex/v032-engine-seams`.
Immutable starting point: `3117e8c36c460f20bfc571700108890a9d4b7ad4`.
Initial read-only production audit committed as `d644c0e9`. Root subsequently
granted Stage B only: `crates/content/src/body_pose.rs`,
`crates/client/src/avatar.rs`, tests inside their existing unit modules and own
docs/seam rows. Stage B is implemented with passing native characterization, quick and scoped
focused checks;
Stage C's subsequent grant is recorded below; host reservations remain proposals.

## Boundary and dependency order

Use only `BlocklandReImagined-worktrees/v032-engine-seams` for mutations/builds.
The primary release checkout and `shared-engine-seams` are read-only here.
v0.3.1 is frozen; root owns its integration, checks and authorized publication.
Halo and AA private fidelity are active for v0.3.2; GMod remains parked. This
assignment supersedes older parking/publication language in historical notes.
Do not change manifests/lockfile, add dependencies, publish or create agents.
Original installations and both delivered Halo previews remain unchanged.

1. Extract existing fitting (Stage B below), independently of pending Stage1.
2. Extract socket/tool calculations (Stage C, separately granted; local native
   characterizations, quick and scoped focused pass).
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


## Stage C granted extraction

Root granted the same two production paths plus existing client/tests/world_items.rs
for native controls, unit tests and own docs/seams. Stage C adds BodySocketInput,
body_mount_node, body_offset_socket, body_mount_action, first_person_hand_frame
and sample_reference_tool_action in content/body_pose.rs. Avatar retains actual
mount_index/for_mesh lookup, offset-only/finite-pitch eligibility, held eligibility,
foreign named-action fallback selection, timestamps and tool-only override precedence.
Reference sampling takes an explicit immutable Rig, its two hand indices, selected
native Animation and played time. It returns only socket deltas, never foreign
body deformation. Out-of-range reference indices stay absent operations.

Node composition remains model * corrected-or-raised-hand * socket-offset *
tool-action; offset-only composition remains seated-frame * offset * aim-pitch *
tool-action. The owned action delta remains inverse(unacted * correction) * posed
* correction, with the same finite/nonidentity threshold. Foreign tool overrides
still work with no unacted sample. First-person hand fallback remains independent
presentation metadata, using the model X-axis scale and mirrored left offset.
No new finite/invertibility rejection is imposed on ordinary socket composition.

Before moving production, three native Avatar tests passed on1127ffdb production,
plus a real native Avatar -> WorldItems control in both views. Their strict
expectations include noncommuting offset/action and correction-conjugation negative
controls, live locomotion transition, authored hand preservation, missing-action
reference sampling without body deformation, seated offset-only sockets and
first-person action application exactly once. The same three Avatar cases pass
after extraction. Quick and scoped focused pass with client590/content101 and32 passing selected
integration controls; the full receipt is in the Stage C progress note. Existing control assertions are unchanged.


## Next host-pose atom proposed: D1 retained native inputs

**Proposal only; no paths below are granted or edited.** Root controls dependency
merges. Before D1, root must arrange this branch to contain immutable Stage1
`94bae5632d1a19d16d9c663e146bcc5034522d55` and the required metadata correction
`853d02f3c3b076cf28e9c59dac7ec6d7758555e9`, plus its reviewed sampler dependency.
Those commits were inspected by git show; they have not been cherry-picked here.
No statement that they passed the integrated candidate or are main/released follows.

Request the following exact next production reservation, initially for asset
registration and read-only native diagnostics, with no gameplay-origin switch:

| Path | D1 change |
|---|---|
| `crates/sim/src/session/animation.rs` | Retain parsed native rigs from the existing validated asset transaction; share its resource/digest/catalog identity and byte budgets rather than introduce another admission/generation owner |
| `crates/sim/src/session/body_pose.rs` (new) | Resolve each actual archetype's roles/sockets/fitted policy against retained immutable rigs; build shared BodyFits; expose bounded read-only registration diagnostics |
| `crates/sim/src/session.rs` | Module/state/export and constructor wiring only; no step/weapons change |
| `crates/net/src/content_identity.rs` | Return body-resource/fitted bindings from the existing bounded native providers, with client-equivalent body/asset precedence; retain valid bind-only bodies too |
| `crates/net/src/host_setup.rs` | Carry bindings in common SessionContent and install the complete native input set after package/archetype setup |
| `crates/net/src/dedicated.rs` | Construct that common binding input beside existing animation assets |
| `crates/client/src/app/session.rs` | Same constructor propagation from the existing background native loader for local hosts |

Test reservation requested: new `crates/sim/tests/body_pose_assets.rs`, new
`crates/net/tests/body_pose_metadata.rs`, and constructor propagation in existing
`crates/net/tests/host_setup.rs` and `crates/client/tests/trench_dig_flow.rs`.
The latter two are the only test SessionContent literals found by git grep at
853d02f3; discovered additional paths still require root coordination.

Proposed native-only API, final names/layout subject to review:

```text
BodyPoseBinding { model, resource, fitted }
Session::set_body_pose_assets(assets: AnimationAssets, bindings) -> Result<()>
Session::body_pose_asset_info(archetype: ArchetypeId) -> BodyPoseAssetInfo
```

Bindings describe provider facts, not duplicate role/socket policy. Resolve the
actual Session archetype by its existing authored `Archetype.id`; retain its full
relevant native policy (roles, sockets, preservation, stand-in/fitted state),
resource digest and node layout. Cache identity cannot be model/preserve alone.
Shared immutable Rig allocation may be keyed by existing resource/digest; fitted
policy/RoleClips/hand indices are per actual definition. On definition changes,
compare full native policy and rebuild that entry. Asset catalog replacement
invalidates resource-dependent entries through Stage1's existing catalog owner.
No new actor/item/asset counters, script-supplied rig, texture loading or render
objects enter sim. Registration remains transactional: malformed input cannot
replace a working native set or cancel playbacks as a side effect of failure.
The combined setter must reuse/refactor the existing catalog transaction, staging
bindings against its incoming validated native set before the existing atomic
swap. It must not install a second catalog or perform two fallible mutations in
sequence. Existing set_animation_assets callers retain their established clock
semantics through that same validation owner.

The current Stage1 catalog retains verified raw native bytes but indexes only
clip metadata. Its read_shape loader explicitly drops animation-empty shapes.
D1 must deliberately retain validated bind-only body resources through the same
bounded asset owner; simply decoding current clock entries cannot close that
case. Optional absent/ambiguous/over-budget bodies remain named unsupported
inputs and preserve ordinary hosting, as Stage1 already does for optional metadata.
Original readers remain offline. No second catalog silently weakens the hash or
provider-conflict checks fixed by853d02f3.

D1 acceptance: actual local/common/dedicated setup produces equivalent native
resource digests and resolved body inputs; two archetypes sharing a model with
different role/socket policy remain distinct; bind-only, authored-hand and stock
bodies are represented; modified bytes, invalid indices/hierarchy, ambiguous
providers and budgets are bounded; failed replacement retains prior catalog,
playback identities and resolved entries. Compare registered fitting to the shared
native functions/actual Avatar control, using invented content only. Existing
Stage1 animation_metadata, animation_identity and authoritative Session clock
controls must remain green. Run scoped quick/focused and report exact SHA.
This checkpoint exposes assets only; it cannot turn the mounted Session red test
green, change a shot origin or satisfy original pose/camera fidelity.

### Required subsequent pose-state design, not implicit D1 scope

Stage1 `animation_view()` returns active records with phase anchors. Its Natural
path removes a playback. Avatar can continue holding a final key after that
endpoint, so active views alone cannot drive equivalent held body channels.
Do not recreate phase from local cue timestamps, use a hard-coded tick divider,
drain the terminal outbox as a second state source, or create a fourth clock.
Propose evaluated phases and any explicit final-key pose retention at the existing
clock owner, sharing actual producer/binding/life/body identities. Source completion
outcomes remain separate from pose hold policy. Pawn15 needs authored channel
semantics, not automatic mapping onto current native slots.

Client locomotion phase, CrouchThread, frozen channel transitions, selected seat
clip/ready and support applicability are still mutable client adapter state.
Their exact shared-state extraction and authoritative input mapping must be
specified before a host composer replaces static grips; the pure B/C helpers
cannot manufacture them. Effective seat frame/scale, pre-passenger-turn fallback
point and actor mount sampling also require exact existing vehicle-owner APIs.
Then reserve sim/session/grips.rs + weapons pre-fire adapter and the exact vehicle
basis accessor, with a native Session test exercising real boarding/switching,
scaled/tilted frames, action lifecycle, actual wall fallback and the explicit
sync_seated -> pre-fire sample -> step_weapons order. AA authored-prone and dynamic
eye/fire policy remain separately demonstrated requirements, not stock fitting.

AA PR54 `def522999e9e9413a9a2c947f5504a8eeb08995b` supplies a static old-pin
package-view audit (activation-v032-handoff.json), not an executed Stage1
replacement counterexample. At853d02f3, runtime already has AnimationOwner,
AnimationItem(slot,generation,item), AnimationImage and AnimationBinding, with
animation_item/image queries. Future authorized package views/completion must
reuse these actual custody/mount/life identities where appropriate and prove
same-id replacement, transfer, duplicate weapons, redraw and respawn behavior.
Do not substitute package counters, equate item/slot with an incarnation, or
activate a private kit for all players. Root routes any separate package API work.
