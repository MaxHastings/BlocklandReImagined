# Native effects integration

The client loads effects-runtime-pack-001 through the configurable native
`effects_runtime` package path. No runtime reads the original installation.
Its complete manifest/library/texture bytes participate in runtime content
identity v4; texture and library checksums are verified before peer admission.
The dedicated server uses the same identity without a renderer dependency.

WorldEffects reconciles light/emitter attachments from authoritative PublicWorld
snapshots. Existing clocks/handles survive unrelated world deltas; paint, direction
and transforms update through original brick attachment rules. Hidden bricks may
intentionally keep effects. Removing/changing a source stops emission and drains
existing particles; leaving a session clears everything immediately. Native
save/reload and late-join reconstruction use the replicated brick properties.
Unresolved imported references are retained as diagnostic warnings.

The shared host device draws original-texture particles/flares after geometry,
before UI, loading existing depth. Animated point lights illuminate terrain,
lightmapped architecture and vertex-lit shapes/bricks without geometry reupload.
Flares use map/visible-brick line-of-sight queries independent of collision and
tool-ray flags, excluding their own source brick. Large sparse spatial queries
switch to occupied-bucket traversal instead of enumerating empty volumes.

Cosmetic budgets default to 4096 sources, 65536 particles and 256 lights. Nearby
attachments take priority; deferred source counts and runtime particle/emission
diagnostics remain observable. These limits never consume or drop server gameplay
events. This is a bounded initial implementation, not a large-world performance
acceptance result; per-frame sorting and per-pixel point-light loops need profiling.

Evidence: the actual App/QUIC/UI/offscreen test sets Red Light and Player Jet
through SendWrench, observes sources and particles, renders them, preserves them
through save/reload and removes them through authoritative undo/disconnect.
Separate tests cover unchanged-source continuity, late-join reconstruction,
nearest-source budgeting, draining/removal, flare obstruction and changing/clearing
point lights without scene upload. No visible window or input automation.

Remaining fidelity work: exact light falloff/color-space behavior and shadows;
transparency-aware occlusion/sorting (particles currently follow the geometry
pass); material-aware glass occlusion; point lighting on water; map wind;
cross-client phase/seed alignment; fake-kill state; player/weapon/vehicle/transient
effect dispatch; and stress measurements. These remain alpha requirements where
applicable. The isolated runtime handles more effects than normal gameplay yet
dispatches; build compatibility is not complete integration.
