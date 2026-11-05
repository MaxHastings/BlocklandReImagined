# Atomic clump publication waits for visible models — 2026-11-04

A phase audit of preview.4's saved offscreen rocket captures found that
JetPuff's 193-member tower admitted clumps at frame 95, two frames before the
complete section was GPU-ready at frame 97. The larger Pirate section finished
preparation after model readiness and did not expose this ordering. The existing
post-landing path could not hit this new early-flight handoff.

Prepared proxies now remain staged until model readiness. Rendering and
attachment frames retain the shared pose; fallback and solver loops continue
to exclude staged clumps. Once ready, the existing atomic publication initializes
all clumps at the current shared transform and material velocity field. This
preserves bounded preparation and does not hide incomplete work by advancing
invisible physics. Delivery/upload can still delay onset; no zero-latency or
universal feel claim is made.

The new regression deliberately delays readiness for one second at zero and
nonzero body limits. It checks unchanged visible positions, no dynamic bodies,
no proxy actor admission, retained material and gravity resuming once ready.
The previous test coverage did not challenge the render-ready ordering.

Maxwell may be playtesting the delivered preview.4. No local build, GPU suite,
full gate or benchmark is launched for this correction until his status answer.
Validation is sent to remote CI first; local scoped and packaged checks remain
pending. The unchanged preview.4 folder is not the final release artifact.
The active goal, destruction acceptance, full release gate and publication hold
remain open. NPC behavior and Speedkart content are unchanged.

Private phase evidence and current artifact identities are under
`E:/Downloads/BlocklandReImagined-Release-Evidence/v044-release-prep-2026-11-04`:
`preview4-destruction-phase-timeline.json`, `preview4-readiness-followup.md` and
`preview4-handoff.json`. Build-only remote artifacts for the earlier source
remain explicitly distinct from this correction; none may be published as if
it included this guard.
