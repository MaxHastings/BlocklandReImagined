# Foliage coverage and evidence

| Reference | Native support | Evidence |
|---|---|---|
| Bedroom node18 fxGrassReplicator |40,000 grassComp PNG, original RGBA and vertex tint, seeded radial placement, native fixed/random plane orientation, source width12..16/height3..4, terrain-only eligibility | pack source member/line/hash; actual Bedroom collision placement and GPU probe; closed grass class assumptions marked |
| Bedroom node20 fxFoliageReplicator |1,000 beargrass PNG, height1..10 with width=height, random horizontal flip, camera-width/world-up quad, source sway/light/fade/mask | OpenMBG engine-family source and native real-map/offscreen tests |
| Other13 native maps | No foliage replicator nodes found | importer scans every native scene and only converts foliage kind |
| TSStatic trees | Excluded from this subsystem | Already represented by shared map scene renderer; used only as probe collision blockers |

Authored fields preserved in evidence: seed/count/retries, original source path, inner/outer radii, square/rotation options, dimensions/fixed size/aspect/flip, terrain/interior/static/water masks, slope/offset, sway flags/magnitudes/timing, lighting flags/luminance/timing, grass top/bottom tint, alpha cutoff/ground alpha, view/fade limits, hide/cull flags and resolution. Editor placement/debug fields remain provenance only. Material filter Any with mode0 is the verified original case. Unit source scale is required explicitly.

Pinned engine-family CreateFoliage lines439ff and616–866 establish RNG/placement/size/animation initialization; rendering1365–1620 establishes fade, light, sway, alpha and vertex positioning. MRandom LCG is16807 modulo2147483647. Exact implementation/evidence SHA hashes and URLs are in ignored artifacts/native-foliage/provenance.json. The runtime imports only the typed pack; neither scene legacy-properties parsing nor source installations enter its dependency graph.

Nine tests pass (one LCG unit, seven CPU/native geometry, one offscreen GPU). Real Bedroom placement succeeds for41,000 plants after59,200 ray queries. Performance/culling output is machine-measured in placement-probe.json/gpu-probe.json, not a fixed claimed frame-rate. Repeat conversion002 is byte-identical to001. No original resource modifications, input automation, visible game or audio occurred.

Integration requirements: root workspace membership, map selection/lifetime, bounded loading scheduling, authoritative static-map collision categories/water queries, original-scene opaque depth, map fog/camera/time binding, map unload/cancel and eventual Maxwell visual calibration. README records grass class uncertainty, fixed-function billboard interpretation, absolute hidden-phase time, fog/mips/color-space and24h time-bound limitations. Alpha contract remains incomplete.
