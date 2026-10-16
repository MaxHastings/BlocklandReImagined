# 2026-10-16 D2b applicability and capacity proposal

Root independently accepted D2a `84dee79add6fba5ba311dab17f7fbf0a0567a624`,
then granted only a documentation proposal from that immutable checkpoint.
Added `docs/architecture/v032-final-pose-retention.md`. No production files,
manifests, original assets, package hooks or motor/client extraction changed.
The isolated v0.3.2 branch remains outside main/v0.3.1.

Audited existing Session admission/records/root/catalog/cancellation boundaries,
the client's free-thread and body-action identity rules, runtime hand/worn/emote
storage and generation reads, actual equip/swap/wear/custody paths, and immediate
versus delayed animation producers and D1's private staged accessor. Important
findings: ordinary body-action
identity includes worn slots2/3 but excludes the separate emote store; an occupied
mount without a generation must not become an empty snapshot slot; the current
image is detached during weapon advancement; PendingCue retains one binding but
not a whole-set snapshot. Later native opt-in must capture at emission/queueing,
not recapture changed equipment at Session admission.

Body context also reuses the full D1 receipt, so in-place role/socket/fitting
tuning cannot leave a model-only held pose valid. Staged catalog validation
would need a specifically reserved visibility-only change to BodyPoseInputs::info
in body_pose.rs; no second validator, counter or weaker catalog.

Proposed first independent reservation is trusted-host opt-in Pawn2 only, with
all existing producers/defaults unchanged. Actual native producer opt-in requires
a separate reviewed reservation and full detached/delayed-context controls.
Its candidate paths are audited, but exact producer opt-in is not proposed for
code approval from this document. The proposed host APIs and query cache limits
are explicit; existing clock mutation revision is not a new world-state version.
The design specifies one combined128 active/held target-key occupancy limit,
the unchanged independent512 result bound with reservations only for active
clocks, atomic preflight/revalidation, transitions and explicit clear semantics,
unsupported channel/provider policies, exact paths, real Session red/green tests
and independently routed Halo/AA requirements. No new clock or identity counter.

Validation is documentation-only: `git diff --check` and
`git diff --cached --check` passed. `git diff --name-only HEAD -- crates`
was empty; only four documentation files are staged. D2a's prior runtime test
receipt remains unchanged. No broad
test rerun or visible game session. Root must review this exact proposal before
any further code grant.
