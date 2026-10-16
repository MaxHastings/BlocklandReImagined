# 2026-10-16 v0.3.2 retained native body inputs, D1

Root granted D1 after Stage C `af8068a2f541c9b437e359e1229e4e474d0e95d7`.
The isolated branch merged immutable root candidate
`3775cc0149d825c8c545462e32c165561e223018` as
`07ae6344639a3cec902d621de7b929f24c9af0d9`. Only docs/architecture/seams.md
conflicted; retained both the v0.3.2 input row and root's interpolation/Stage1
rows. Production merged cleanly. No dirty files were copied. This input merge
is into v0.3.2 only, not an integration of this lane into v0.3.1/main.

## Input evidence, separate from D1

Before D1 test validation, seven controls on the immutable merged input passed:
sim authoritative_animation_completion15/zero/zero; weapons animation_identity
4/zero/zero; net animation_metadata6/zero/zero; content
authored_rotation_interpolation8/zero/zero; native Avatar units44/zero/24 existing
ignored; body_layer_plan2/zero/zero; world_items15/zero/13 existing ignored.
Commands are cargo test --locked for those named targets; Avatar uses
`cargo test -p bri-client --lib avatar::tests:: --locked`.
Logs `target/d1-input-0.log` through `target/d1-input-6.log`.

Root observed release/LTO file-access contention and instructed future worker
builds to use CARGO_BUILD_JOBS=2, leave CARGO_TARGET_DIR unset, avoid redundant
runs and continue implementation while remaining input controls finished.
No running build was interrupted. The last WorldItems control used its already
compiled input libraries while D1 implementation began; every input control
finished green before D1 tests ran. Worker quick/focused also use process-local
RUST_TEST_THREADS=8. The primary release checkout remains untouched.

## Implementation boundary

Exactly seven production paths: sim/session/animation.rs, new session/body_pose.rs,
session.rs module/state/export/constructor wiring, net/content_identity.rs,
host_setup.rs, dedicated.rs, and client/app/session.rs constructor propagation.
Four test paths: new sim/body_pose_assets.rs and net/body_pose_metadata.rs,
plus host_setup.rs/trench_dig_flow.rs required constructor fields. No manifests,
new dependencies, step logic, fire origins, clocks, hooks, packets or counters.

The existing asset validator now returns clip metadata and parsed immutable
native rigs together. Session::set_body_pose_assets stages bindings, full actual
definition receipts and fitting inputs before the existing catalog swap. Existing
set_animation_assets uses the same transaction and retains only still-available
bindings/reference; it never guesses a replacement reference. Failed registration
does not consume catalog revisions, invalidate prepared tickets or alter active
playback identities or existing resolved entries.

BodyPoseBinding carries model/resource/fitted provider facts. At root's review,
BodyPoseBindings has one explicit optional reference_resource, separate from both
native encoding and default actor definition. Unknown references fail; duplicate
reference JSON fields fail decoding. Absence leaves reference seat-hip adjustment
absent while independent native hand fits remain available. This preserves
permissive missing-fit semantics rather than substituting another rig.

The private resource cache retains Arc<Rig> by verified resource/digest. Derived
receipts retain full actual Archetype snapshots; queries compare the current
definition and recompute changed policy read-only. Two definitions sharing model
and preserve flag retain distinct roles/sockets. The eye diagnostic resolves the
authored socket; the existing fitting eye landmark remains the native literal
Eye node, matching Avatar. These are registration receipts, not actor poses.

The common bounded loader retains valid bind-only body shapes through the same
catalog byte/resource budget. Shape-derived rigs retain Shape hash/hierarchy and
alias validation; the full-avatar Rig branch still requires its existing Rig
validation. Provider candidates use Avatar's model/optional-file sort/dedup order
and package-relative file resolution. Optional selected-source failures are named
unsupported; no alternate provider is silently substituted. Mandatory avatar hash
failure and existing image ambiguity/budget checks remain strict.

Unbound/stand-in models and separately vehicle-owned body rigs are explicitly
unsupported by these native input diagnostics; no stock substitution is made.
Existing load_horse/provider integration, live pose composition, authored prone,
dynamic eye/fire and source clock/completion/replication remain separate work.
The static gameplay grip path is unchanged; D1 cannot close its mounted red test.

## Tests and corrections

Native Session fixture uses actual installed package definitions. It verifies
same-model role/socket distinction, authored-hand fitting, host socket tuning
without reinstalling assets, immutable old receipts, and failed replacement with
prepared/active native playback. Rejections include hash mismatch, Rig hierarchy,
bind-only Shape hierarchy, duplicate binding, unknown reference and catalog budget.
The valid absent-reference case keeps independent hand fitting and active ID.

Actual common/dedicated setup returns equal stock/raw/fitted bind-only receipts
from source-free packages. It verifies actual selected bytes, native fit eligibility,
existing active clock rollback, optional bad selected source containment, mandatory
stock hash failure and optional bind-only resources sharing the catalog budget.

Fixture failures were retained, not waived. Initial native test tried reinstalling
packages, which Session correctly forbids; switched to the supported host tuning
API. Tuning an absent/null socket payload also rejected the fixture path; authored
an explicit nonzero socket offset to exercise that existing operation. Production
tuning was not changed. Initial net fixture declared an asset ID as body content;
ordinary package admission correctly excluded it. Restored a valid ordinary-host
fixture and qualified the malformed candidate-order case as a direct selector
check, not evidence that the malformed package can play.

Corrected targeted results: native2/zero/zero and metadata3/zero/zero, logs
`target/d1-native-corrected2.log` and `target/d1-metadata-corrected.log`.
The additional bind-only hierarchy rejection is included in subsequent focused
validation. Earlier failed receipts remain in target/d1-native-first.log,
d1-native-corrected.log and d1-metadata-first.log. A draft-only Python quoting
error was corrected before any production mutation. No original assets were used.

Quick passed six steps in258s: client590/zero/76 existing ignored,
net81/zero/3 existing ignored, sim403/zero/4 existing ignored (1,074 passed,
zero failed,83 ignored); all three format checks pass. Log target/d1-quick.log.
Focused uses --crates bri-sim,bri-net,bri-client --depth0 plus explicit
trench_dig_flow, body_layer_plan, world_items and animation_identity controls;
passed all13 steps in1,900s with CARGO_BUILD_JOBS=2 and
RUST_TEST_THREADS=8. All-target Clippy -D warnings passed for all three changed
crates. Aggregate1,898 passed/zero failed/235 existing ignored across127 test
summaries: client units590/zero/76, all net tests217/zero/20 across40 groups, all
sim tests1,069/zero/126 across82 groups, trench_dig_flow1/zero/zero,
body_layer_plan2/zero/zero, world_items15/zero/13 and animation_identity4/zero/zero.
This includes native clock15, metadata6, D1 native2 (including bind-only hierarchy
rollback) and D1 metadata3. Log target/d1-focused.log. Existing crossover soak
passed in139.43s under the unchanged workload; this is not a one-hour alpha soak
or original fidelity acceptance. No check was waived. Diff whitespace passes.

```powershell
$env:CARGO_BUILD_JOBS='2'
$env:RUST_TEST_THREADS='8'
python tools/check.py quick --crates bri-sim,bri-net,bri-client
python tools/check.py focused --crates bri-sim,bri-net,bri-client --depth 0 --tests bri-client:trench_dig_flow --tests bri-client:body_layer_plan --tests bri-client:world_items --tests bri-weapons:animation_identity
``` Root owns integrated validation,
consumer routing and v0.3.1 publication. No main/release/full-gate or original
fidelity claim follows from D1. Original installations and both Halo previews
remain unchanged; Halo/AA active, GMod parked.
