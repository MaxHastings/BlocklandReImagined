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
