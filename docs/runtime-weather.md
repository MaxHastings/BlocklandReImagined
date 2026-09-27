# Native client rain and snow

The weather runtime/importer are main-workspace members. The client loads
weather-pack-001 and sets authored placements on map preparation, for hosted and
joined games. Storm uses 5000 rain drops; Slopes uses 500 snowflakes; maps without
placements stay dry. The original precipitationOn checkbox is enabled, defaults
on and applies on committed settings. Disconnect/map replacement clear all drops,
splashes and cached queries. GPU stop/recreation uses the host device lifecycle.

The real first/third-person camera and player velocity drive weather. Queries
choose the nearest solid or native water surface. Solid queries include native
terrain/interiors and colliding replicated bricks independently of tool raycast
flags. Brick geometry/collision changes invalidate cached roofs using the query
generation. Native water repetition preserves authored tile gaps. Still-water
volume surfaces govern impacts; visual waves do not move authoritative water.
Query failures invalidate weather instead of treating unavailable geometry as
clear space. Dynamic actors and streamed terrain will need to join this query
boundary when their client geometry is integrated.

Original atlas textures draw through the existing host color/depth pass after
geometry and before UI. Weather does not own physics or a second GPU device.
Its definitions, authored values and checksum-validated PNG bytes participate in
runtime content identity v6 on local host, remote join and the dedicated server.

Evidence: release app_flow tests host Storm, Slopes and Bedroom through actual
App/Worker/QUIC paths, render offscreen, toggle settings and disconnect. With the
scene frozen between on/off renders, weather changed 12604 Storm pixels and 595
Slopes pixels; Bedroom changed zero. No invalid hits, capacity clipping or pending
queries remained at capture. Storm recorded 4345 water impacts/splashes in that
bounded run. Report and inspected PNGs: artifacts/native-client-weather/.
Separate tests cover a replicated roof with tool raycast disabled, collision
changes, and nearest roof versus repeating real native water. Counts/timings are
probe evidence, not interactive visual acceptance or a server-scale benchmark.

The first real client run exposed an identity-reader filename mismatch
(manifest.json versus the delivered weather.json); corrected before the passing
rerun. Existing source qualifications for wind/tick conversion and snow atlas
interpretation remain in research/weather/evidence.md. Weather/particle global
transparency sorting, fog matching, dynamic blockers, long traversal/streaming
and Maxwell's fidelity assessment remain open; this does not close the overall
map-fidelity gate.
