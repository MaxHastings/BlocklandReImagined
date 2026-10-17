# 2026-10-17 AA-driven v0.3.3 priorities

Maxwell explicitly requested coordination with the AA2 working chat and a start
on its high-priority engine needs. This resumes bounded AA gameplay work after
v0.3.2 publication; it does not restart every parked consumer or mechanism plan.
Root reused its clean, merged release worktree on a new branch
`codex/v033-aa-gameplay-integration` from published e4d9bb05. Old private/source
work, release artifacts and saved engine31f/722/4b branches remain untouched.

AA owner handoff: private PR91 `codex/aa25-soldier-showcase`, pushed45b393c at
the handoff, with active AA-only pitch work kept in that owner's checkout.
The current Pipeline TDM has US/OPFOR bodies, M16/AK, original assets, material
feedback, optional placed ambient audio, terrain/building and office props.
Real client material-binding failure121 was corrected at45b393c, with both
actual renderer loaders/GPU upload exercised. Existing headless and offscreen
checks do not establish real transport or Maxwell's interactive acceptance.

Current source expected behavior and engine gaps:

1. Vertical aim: the AA owner reports both bodies passing native look-layer
   pitch checks and captures without moving the feet. This uses existing engine
   mechanisms, not a new procedural-pose API. Selected two-spine source targets
   are qualified; posture branches, lean and temporal behavior remain limited.
2. Aimed viewmodel pose is the first selected engine slice. On e4, `Zoom` is
   local FOV/scope/sensitivity behavior. `ImagePresentation` has only static eye
   placement. It cannot express a package-authored sight pose driven by ordinary
   aim input, with consistent posed muzzle/effects. Root asked AA for minimal
   source/profile endpoints and a retained ordinary press/hold/release plus
   swap/death/respawn failing consumer before implementation. No such ADS red
   exists yet; source audits alone are not that test. The engine owner is checking
   the smallest optional extension to the existing seam and its exact paths.
3. Sprint follows that consumer: current `MoveInput` has no sprint intent;
   native Shift binding must not be globally replaced. Package-owned source
   StartSprint/StopSprint lower/raise behavior needs accepted movement/input to
   weapon-state integration and fire eligibility/interrupt controls. Existing
   image states may supply the clips. Exact cancellation/timing still requires
   qualification; no guessed timer or general callback framework is approved.
4. Pipeline lighting is a separate bounded design task. The219 source light
   inventory is not equivalent to219 simultaneously rendered shadowed lights.
   The24 budget spans scene admission, descriptor validation, shaders, visibility
   and shadows. Raising one constant is not an accepted fix. Source-light
   storage/bake and live selection/budget need separate evidence.
5. Indoor prop collision is private recovery/import using existing finite native
   collider mechanisms until a concrete native limitation is demonstrated.
   Visual props are not reliable cover today. Original third-person textured
   flash orientation is a lower-priority separate native-effect gap.

Root messaged both existing AA and engine owners; no new chats/subagents were
created. AA finishes the small pitch delivery and supplies the first actual ADS
consumer. Engine owner returns a minimal proposal read-only before a path grant;
root owns integration/registry. No source values enter the public engine. The
old full-ammo strict red and bridge31f/proposal4b remain preserved and parked;
they are not silently made prerequisites for this TDM interaction.

Evidence read: AA TDM/PIPELINE handoffs, aim-pitch-source-contract.json,
view-transitions-source-audit.json; public Zoom/Controls/ImagePresentation,
MoveInput, scene_loader, lighting_parameters and MAX_LIGHTS/shader call sites.
No gameplay/production change or new acceptance pass is claimed by this note.

## Actual first-consumer checkpoint

The engine's source-free actual-App diagnostic reached its intended missing-pose
assertion after RMB FOV90->40, admitted shot12->11, release/focus/swap/death
controls. This proves a generic optional placement feature is absent; it does
not prove the AA sight path needs that feature. Driver/log/evidence are retained
outside target in the engine owner's artifacts. Root granted no production API.

AA then saved selected source endpoint facts at49aeb418, explicitly retaining
unqualified original selector/3D/interpolation composition. Root challenged the
new API assumption and requested an actual existing-Zoom.overlay trial. Private
b1ecaf7ee6cfd6b3122e75c25671f0553b7affbd passes that consumer: ordinary App host,
real seven AA packages, QUIC, UI/GPU rendering, original512-square M16 artwork
and full pixel hash plus317 opaque GPU texels, FOV70, admitted shot and original
sound request, release/focus/AK swap/death/respawn. The same M16 with its zoom
profile removed supplies the no-overlay/FOV90 baseline. Two tests and scoped
Clippy pass. Root read the actual test/receipts and inspected the aimed capture.

This is an original-artwork preview adaptation: native hold-RMB/no-jets/input,
FOV easing, screen-height fit/black bars and hidden rig. It is not exact original
runtime selector, 3D sights, positional interpolation or source gameplay parity.
The earlier generic pose red is preserved but does not justify implementing an
unneeded API for this preview. The AA owner may finish a reproducible private
delivery only after checking the exact game is not running, retaining affected
file backups and proving actual startup health plus unrelated-file identity.
No active user playtest may be interrupted or hot-edited.

Sprint is the next selected engine need. Root requested one armed/standing-forward
source/preview trace from AA and a minimal existing-route/input/cancel proposal
from the engine owner. Package policy owns speed, eligibility and clips. General
messaging, new motor physics, full-ammo completion and lighting expansion remain
outside that bounded step. No sprint production paths or acceptance are granted
by this checkpoint. Vertical aim was separately tested/installed at f9d514c.

## Coupled consumer justifies the first production change

Private06973e4c saves the existing held-bind sprint trial and same-candidate sight
regressions. Ordinary App/QUIC bool down/up reaches server package policy; source
clips play in native Lower/Hold/Raise and authoritative speed_scale1.5 gives
4.4856 versus2.9904 horizontal velocity. It is not installed. Root review required
normal input ordering, aim/sprint interaction and ambient/grounding checks beyond
the first already-host-moving case. Those remain private policy work; a support
ray is explicitly a preview proxy rather than a canonical grounded observation.

AA's actual App scope-policy run20261017-071002 reaches a strict failure after
valid Ready/Fire/WaitRelease sight, admitted shot and release/re-aim controls.
All five accepted Draw/SprintLower/SprintHold/SprintRaise/Reload states retain
aiming, overlay and FOV70 while hiding the rifle. The error is
AA_SCOPE_STATE_POLICY_MISSING:5barredstates retain sights; actual exit101,
scoped Clippy0. The fixture SHA is
ba516b07310d083fa1d61ae8a526c93ee2f5f3f546079c19982fb2700b37d2ad.
Existing set_fov90 does not override the active scope; a no-Zoom image variant
changes image identity and restarts Draw, rather than preserving the same state.
Root read the actual log, JSON and ordinary-input fixture. This is the required
affected consumer evidence; the earlier invented pose diagnostic is not its basis.

Production grant on the existing engine branch from e4 is exactly
weapons/src/lib.rs, client/src/controls.rs and client/src/app/view.rs. Add optional
Zoom.allowed_states: omitted preserves legacy, explicit empty denies all, bounded
case-insensitive unique names resolve within the owning image. Use the already
accepted local mounted state; unknown/missing state denies a restricted profile.
Retain the profile for on_jet/jets input ownership while local aiming is barred,
including suppressing the generic keyboard-Zoom fallback. No new host permission,
counter, clock, mount identity, Sprint enum, wire message, renderer or motor field.

Preview policy permits Ready/Fire/WaitRelease, bars the other five states and
re-aims when Ready returns while RMB remains held. Existing no-profile behavior
is preserved; this is not exact AA runtime selector/interpolation parity. Public
source-free metadata/control/App regressions, quick and full normal focused tiers
must pass before an immutable engine pin. AA then consumes that pin in the SAME
state-policy test with opt-in metadata. New metadata must not be installed into
the old e4 executable's folder. No v0.3.3 publication grant is inferred.
