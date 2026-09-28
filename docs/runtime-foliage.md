# Native map foliage in the client

Bedroom's two authored replicators now participate in normal host/join map
loading and rendering: 40,000 grass and 1,000 beargrass plants, original textures
and alpha. The configured native pack is foliage-pack-001. Other maps receive
only their own authored definitions; no grass is invented for dry/empty maps.

NativeMap labels collision builders as Terrain, Interior or Static. Simulation
still replaces these with its authority map tag; the client query mirror retains
classification. Foliage queries the nearest static surface, including prohibited
roofs/objects that block terrain underneath. Native water is also considered,
including repeating footprints and authored water fallback rules. Player bricks
do not regenerate these static authored map decorations.

Placement runs in the existing bounded background map-loading workers. Each
builder advances at most 1024 rays per chunk, retains seeded state and reports
requested/placed/rejected/query counts. A 16-million total query limit fails the
load explicitly; it does not silently truncate plants. This does not yet provide
fine-grained cancellation inside an already running map-loading closure.

Prepared immutable fields reach the client with the matching map load result.
GPU resources upload once, then culling updates visible indices. Foliage draws
after map/build geometry with existing depth and before particles/weather/UI.
GPU recreation retains CPU placement; disconnect/map replacement clear fields,
GPU resources and local animation time. Plants are cosmetic and not networked
individually; all placement parameters/textures join runtime content identity.

Long sessions use f64 elapsed time. Every ten minutes, original per-plant sway and
light phases are rebased using each plant's own authored rate, avoiding a 24-hour
failure/wrap and large shader time values. This rare buffer upload is included in
render diagnostics; normal frames do not resend meshes/images. CPU placement is
retained for GPU recreation, in addition to renderer data.

Evidence: actual client static collision places all 41,000 plants without rejects;
sampled positions are on permitted terrain. Synthetic roof classification tests
prevent tracing through prohibited interiors. All ten foliage tests pass when
private asset/GPU cases are explicitly included, including depth occlusion,
original alpha, bounded culling and seven-day phase/render checks. The release
App/QUIC/UI integration passes with placement counts, GPU recreation without
replacement, and disconnect clearing. See native-client-flow/report.json and
the additional original-map compositor in artifacts/native-client-foliage.

Remaining fidelity: exact closed-engine grass plane construction; simple fog
versus original layered fog; foliage shadows/lighting, mip filtering and ground
blending; global translucent ordering; visibility from normal player traversal;
streamed terrain integration; Maxwell's visual acceptance. Converted sources and
engine-family assumptions remain in the foliage crate README and pack evidence.
