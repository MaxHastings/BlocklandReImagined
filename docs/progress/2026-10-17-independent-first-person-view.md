# 2026-10-17 Independent local held-image view

Root granted a bounded implementation on codex/first-person-view from exact
c1e31fdbf2990ef68dd53eaf84893fc45b689ce2. Old branches, observation pin and
paused surround patches remain preserved. Source evidence and AA's ordinary
red are owned by the private consumer; public code contains no source-game
projection/scale constants or original assets. Root owns integration.

Optional Image.first_person_view owns horizontal FOV, near/far, local visual
scale and optional reference-FOV eye translation. WorldItems already retains
native weapons, avoiding a second presentation-policy schema/map. Current
controls are sampled before ordinary mounts/effects, then the SAME controls
are cloned for render; current custom shots suppress opted layers. Scale is
in the retained mount before posed nodes, after unscaled eye translation.

Existing per-view uniforms/effect buffers, reserved after mirrors/probe faces,
draw current image layers with color load and explicit fresh depth. Existing
world sky-exposure inputs also prepare modern local views. World-depth consumers
finish before the clear; screen vignette remains above local images, HUD follows.
MSAA color stays retained and resolves once after the final local pass. Main
shadow inputs are retained. This is not original AA lighting acceptance.

Immutable local source affinity copies into particles, including detached ones.
Eligible exact current image/policy selects a local view; holstered/behind-view
particles drain hidden, never move into the world/new weapon projection. Existing
source/particle capacities and IMAGE_SLOTS bound all retained/current state.
Global farthest-first trimming shares the sprite budget; main/local skins share
draw/triangle limits. Point lights remain world once, without mesh-scale inference.

Targeted controls: metadata3/0; detached effect lifetime/partition1/0; native
transform1/0; global particle budget1/0; enabled skin prepare/sharedbudget1/0;
actual ordinary App/offscreen plus two-image adapter2/0. Latest App45.83s:
modern lighting, 4:3/MSAA and widescreen/render-scale, model pixel pairs3095/5090
with identical world matrices and zero weapon particles, additional actual model
submission, independent world FOV, admitted projected transient, current custom
camera transition, TP/holster/disconnect. Same-pin original AA consumer remains
required; technical checks do not establish source fidelity or playtest acceptance.

Tooling/test failures retained under ignored artifacts/first-person-view-*:
initial metadata test compared f32 serialization to unrounded JSON literals;
corrected typed roundtrip. New FX test initially used wrong constructor/reset API;
new App test initially omitted Box for SaveSettings and compared display name to
package ID. Corrected tests without runtime concessions. Root rejected an early
pixel comparison across different FOV/lifecycle states; replaced before validation
with same-camera/no-weapon-FX pair plus submitted-model count. Extreme numeric
policies now fail admission rather than rendering. Full Image literal maintenance
only adds None in client actor_effects, its test, and weapons-import; no import
policy/content regeneration. Logs and captures stay outside commits.

Final source freeze and normal tier results follow below.

First quick PASS8steps/78s632/0/76. First focused stopped at Clippy
collapsible_if before runtime tests; simplified the new vignette condition.
A direct all-target client lint also flagged manual_contains in the new test;
corrected without behavioral changes. Both first logs preserved. The initial
queued CSV selector spelling never reached execution; final focused uses
separate --tests arguments. Refreshed all16 source hashes before final tiers.

Second focused run stopped exit101/951s at net compile: a full test
Image literal omitted the new optional field. Its normal importer/chaos/
client/FX steps passed, but this is not a focused pass. Preserved first-net-
literal.log. Scanned all explicit first_person_origin initializers and
shorthand fields: added None only to net/tests/bandwidth.rs, net/tests/
loopback.rs, net/src/content_identity.rs inline test, sim/tests/showcase.rs.
The sixteen prior hashes remain unchanged; these four additions produce
a twenty-Rust-file freeze. Both bri-net and bri-sim join the final changed-
crate selection so their formatting/all-target lint/unit tests are required.

Final validation: workspace all-target compile exit0/149s. Final quick
exit0,12steps/353s,1119passed/0failed/83existingignored. Final focused
exit0,27steps/1760s; all six changed-crate all-target Clippys and normal
dependent suites passed, plus explicit client:first_person_view (2),
client:weapon_frame_probe, weapons:first_person_view (3), fx-runtime:local_view (1).
All twenty frozen source hashes reverified; git diff --check passed.

Maxwell then explicitly ordered wind-down of Halo/AA and all other work,
with only v0.3.2 publication to fully complete. Preserve this tested branch
as a pushed handoff; do not begin original AA consumer/integration or further
feature work. Existing original-source acceptance remains unresolved. Root
publication coordinator was notified of the superseding instruction.
