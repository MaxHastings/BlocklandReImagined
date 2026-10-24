# 2026-10-23 Viewer region ambient must not relight other rooms

Maxwell reports abrupt, immediately reversible darkening when crossing an
Interdiction doorway. The client overwrites the shared camera ambient with the
viewer's region ambient. Actors already take region ambient and suns from their
own render boxes. This camera override also changes unrelated visible geometry
and the sky; it is not a valid spatial lighting relationship.

Remove only the shared ambient override. Keep actor lighting, region suns and
viewer-owned fog. A normal App, loopback host and offscreen capture holds the
view/world fixed while moving a region boundary two centimetres across the eye.
Before the correction, 19,200 pixels change; afterward zero change in Classic,
Unified and Dynamic. The fixture asserts stable baseline frames, retained local
actor light, a visible fog negative control and exact recovery on leaving fog.
Quick: 667 passed / 78 ignored. Focused: strict all-target clippy and the same
unit suite pass. A first workspace-wide cargo-fmt invocation exceeded Windows'
command length; the repository's per-crate formatter passes.

This proves the general camera-lighting defect. The reported exact doorway has
not yet been located, and actual mission captures remain dark in Classic. It
does not certify original-game appearance or solve the separate missing saved
lighting on movable doors. Direct original-data inspection finds saved lighting
instances for 154 Interdiction movers which the existing instance emitter skips.
Source data, screenshots, negative-control log and captures are retained outside
worktrees in the private interdiction-jail-report-20261023 archive.

Maxwell next requests console clipboard support and a coordinate command/build
before further lighting work. This correction is retained separately pending
integration and exact mission revalidation; no collision fix is claimed.
