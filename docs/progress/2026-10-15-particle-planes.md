# Authored particle planes

Particles can explicitly select screen, axis or plane orientation. Omission keeps
the existing emitter convention. A plane retains the source right axis and
emission normal, including attachment movement and both portal integration paths.
The GPU projects the authored right into the normal's plane, with a stable
camera-independent basis if velocity turns through that axis. Spin acts within
the plane. Existing depth, blend, fog and texture registration paths remain shared.

The private source converter distinguishes the three recovered orientation modes
instead of collapsing perpendicular planes into axis-facing billboards. The
current rifle/Warthog effects use screen/axis modes. Pistol and energy-ring plane
integration still require their actual assets and rendered review; the recovered
co-op teleport ring is not evidence for the map's green teleporter.

Evidence: cargo check for client/converter passes. Changed-crate quick checks
completed 10 steps in 279s. The focused run's content, converter, client and effects
checks pass, including the offscreen GPU plane/roll/spin/edge-on/degenerate-basis
checks, attachment carrying, portal carrying and existing blend/depth checks.
The full changed-crate focused run passes: 15 steps in 503s, including network
integration, crossover and sized join. Command: `python tools/check.py focused
--crates bri-content,bri-fx-runtime,bri-convert,bri-client,bri-net --depth 0`.

Initial test fixture errors (integer sample indices and a missing generic flare
texture) were corrected without weakening runtime validation. Generic synthetic
pixels verify renderer semantics; they do not certify source-game appearance.
Particle lighting, source blend laws and delayed events remain separate gaps.
No installed playtest update, interactive validation, release gate or publication.
