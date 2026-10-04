# Independent v0.2.3 correction review and passage-frame handoff

2026-10-03. GPT-6.1 Sol High review lane. Shared production, tests and original
content remained read-only; implementation and proof changes were delivered as
`/tmp` patches for the root integrator. No Cargo/GPU lease, gameplay automation,
visible window, additional agents or worktree was used by this lane.

## Independent correction review

The live copy-authority patch has no remaining static blocker in the inspected
jobs. Plant/Cut/Look/Wrench/SuperCut/Fill refresh the same Build/Paint admission
used by their original constructors. Undo refreshes current actor/connection
without inventing alive, inventory, equipped-tool or build gates. Plant retains
PlantAs's explicit admin policy and admin-only float permission. A denied atomic
Place enters existing provenance-owned cleanup without publishing new bricks.

The final GroupSupport correction stores compact identity/rotation/bounds,
charges lazy spatial candidates and raw connector cells, caps synchronous work,
and invalidates Check/Place after external support topology or actor-authority
changes. Own publications update the expected epoch before package callbacks.
An initially missed native contact loop now charges raw AABB candidates before
filter/contact work. No further blocker was found in this correction. These
bounds do not claim every old occupancy/collision algorithm is incremental or
that a million-copy run was measured. Root's actual separate-editor Cut proof
first demonstrated unsupported placement before the correction. The final
`/tmp/bri-v023-support-integrated.log` records 20 Advanced Duplicator and 14
building tests passing; this lane read the final receipt.

A concrete v0.2.2 UI lifecycle sibling was found in the creator patch: authoring
controls and favorite Load remained active while Save waited, but success marked
the current mutable draft as applied instead of the captured sent draft. Loading
favorite B after sending A could report B Applied and later erase it on listing
refresh. The busy-control patch uses one mutation predicate, blocks pending
mutation events and Delete, and clears inactive text focus/popup captures while
retaining navigation and Close. No static blocker remains in the reviewed
production/refinement/busy patches. Root reported 31 MiniGame and 8 field-flow
checks passing; this lane did not run those checks.

## Mounted rejected-crossing finding reproduced and corrected

The first actual Wrench fixture failed admission because driver-only settling
steps left the editor's movement lease expired. An ordinary idle editor move
fixed the fixture. The corrected pre-fix run
`/tmp/bri-v023-mounted-rollback-before-2.log` then reproduced the production bug
for both synthetic and native vehicles: the predictor restored the host's
source pose, while presentation inverse-carried it through an obsolete pending
trip. The rejected native root was approximately (0, .694, -24.299), but the
drawn root became (-.299, .694, 6).

Position/plane heuristics cannot resolve arbitrary overlapping frames after a
link edit. Root approved an alpha wire change. This lane authored a reusable
PassageFrame (revision plus cumulative rigid transform) recorded by the existing
Session Crossings seam and paired with own/vehicle authoritative pose/ack.
Prediction restores that frame and recomposes surviving replay trips. Motion
compares pending and shown frames before inverse-carry or smoothing: rejected
speculation is removed; an accepted pending trip survives a later unlink;
changed routes at equal revisions still have distinct transforms. Walking uses
the same correction mechanism. There are no content or portal-name exceptions.

The ordinary Session/boarding/driving/actual Wrench regressions include far and
valid grid-aligned overlapping portal frames. The first overlap offset was
invalid authored grid and failed setup, so it was corrected to a half-stud
shift; acceptance assertions were retained. Both cases include a real earlier
host pose/ack delivered after correction. Final
`/tmp/bri-v023-mounted-marker-final.log` records seven checks passing, including
synthetic/native rejection, accepted-after-unlink controls and overlapping
frames. This lane read that receipt; root performed execution.

## Marker obligations and final pending verification

Replica validates the marker for unreliable own/vehicle poses and both
checkpoint paths before storage. Numeric bounds leave arithmetic/replay
headroom: finite MAX translations otherwise overflow frame subtraction, and
MAX revision otherwise panics on the next predicted trip. Public predictor
bootstrap/restore setters also validate. A codec-to-Replica test covers NaN,
Inf, finite MAX, zero/NaN quaternions and MAX revision with atomic refusal.
The first root filter selected zero library tests; that is not proof. The
proper replication-test run remains root-owned.

Persistent frames do not expire with recent crossing history. Bot removal uses
Disconnect and forgets the Player marker; vehicle removal uses forget_vehicle;
Change Map swaps a fresh Session, adopts selected state without Crossings, and
installs a fresh client mirror/Motion. Respawn retains the connection frame and
uses spawn identity to reset old-body presentation. Lifecycle/relink mechanism
proofs were delivered in `/tmp/bri-v023-frame-lifecycle-proof.patch`.

A final adapter ordering issue was confirmed: observe_local changed spawn and
pending presentation before predictor stale-tick/ack rejection. Replica already
protects ordinary old-tick datagrams, but a newer-tick superseded ack could reach
this boundary. `/tmp/bri-v023-frame-pose-admission.patch` centralizes the existing
read-only admission and runs it before presentation mutation, including mounted
poses; the first own Pose initializes the predictor's tick/ack baseline. It
preserves existing invalid-current-owner/future-ack errors. The accompanying
mounted/walking stale-spawn and stale-ack test is pending root execution at this
handoff.

This lane independently reviewed other lanes' authority/support/UI corrections,
but authored the new passage-frame implementation. Its own source inspection
is not independent approval of that new seam; root was asked to have another
GPT-6.1 Sol High reviewer examine it before publication. Platform gates,
packaging/publishing and Maxwell's interactive acceptance remain root-owned.


## Controlled-body target changes and paired frame basis

A fresh independent GPT-6.1 Sol High reviewer found two further concrete
blockers in this lane's initial passage-frame implementation. Leaving or
changing a driven target unconditionally promoted its pending speculative trip,
then switched the shown frame to walking history, losing the rejected-trip undo.
An unbounded future driver acknowledgement could also become sticky and reject
all subsequent genuine corrections. These findings were release blockers, not
accepted limitations.

The integrated correction records actual vehicle carries for the live seated
Player bodies as well as the vehicle. Own Pose pairs that body history with the
mounted vehicle ID and its frame at the same tick; independent own/vehicle
streams therefore do not guess a boarding basis. Prediction maps travel since
that vehicle basis into the rider's body frame. An admitted target departure or
incoming mount settles the old presentation against the authoritative body
history; new walking inputs do not replay into a vehicle. A reliable seat change
arriving before own Pose parks prediction and retains unresolved fate instead
of publishing it. App chooses mount/prediction before advancing its next frame.
New-body and older tick/ack admission use the same preflight boundary.

The independent reviewer also required preserving the original basis tick
while paired body/vehicle markers remain unchanged: own idle poses run at about
10 Hz, while settled vehicles use a one-second keepalive. Updating the basis
on every heartbeat could indefinitely stop a stationary vehicle. An existing
same target remains eligible when an own marker update precedes its next vehicle
sample; the old vehicle sample is ignored without retiring the target.

Live seats, rather than the Session's delayed mount index, determine carried
riders at the actual crossing. Ordinary jet input removes occupancy before
vehicle travel, but Session's index catches up when post-step intents drain;
using that index could invent a trip for a rider who just ejected.

Root execution evidence read by this lane:

- `/tmp/bri-v023-mounted-rider-first.log`: nine checks pass, including native
  and synthetic ordinary Session jet-eject accepted/rejected trips in both
  own-pose/seat-change orders, before the final idle/incoming/live-seat followups.
- `/tmp/bri-v023-motion-final.log`: 22 Motion checks pass, including native
  mounted rollback/overlap/eject, replay correction and walking/floor picture
  continuity, before the final idle/incoming/live-seat additions.
- `/tmp/bri-v023-frame-validation-actual.log`: the actual net replication
  integration target runs the codec/checkpoint malformed-frame test and passes.
  The earlier `--lib` invocation matched zero tests and is not evidence.

The new optional paired vehicle anchor has source validation in Replica and
Motion before state mutation; the earlier malformed-frame runtime proof did
not yet mutate that newly added optional anchor. Boarding with independent
historical vehicle/rider frames has a rebasing unit proof and an incoming-mount
presentation unit; it is not claimed as a second complete real boarding
session. The last exact ordinary-control live-seat same-tick ejection proof was
delivered as `/tmp/bri-v023-rider-frame/same-tick-tests.patch`; it was unrun by
this lane at handoff. Root owns its execution and all final workspace/platform
gates. The fresh independent reviewer owns the review of this lane's authored
canonical-frame change; this lane does not self-certify its own implementation.

Source work stopped at root's requested integration boundary. No further
optional work, source exploration or compute was started by this lane.
