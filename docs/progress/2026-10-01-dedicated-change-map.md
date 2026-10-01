# 2026-10-01 Change Map on a dedicated server, one map loader for both hosts

Max asked for Change Map on dedicated servers and for the dedicated and
client-hosted paths to stop diverging.

- New `bri_net::map_content`: `MapContent::load` is the one way a hosted
  session loads a map (simulation, terrain, water, spawn points, Tutorial,
  breakables, pending objects). The client's `ContentPaths::load_map` and
  `bri_net::dedicated` both call it; the copy that lived in each is gone
  (the dedicated copy skipped the Tutorial and the pending-object summary).
  `bri_client::content::LoadedMap` is now that shared type.
- `LOADABLE_MAPS` moved from `bri-client` to `bri_sim::map` so the server
  offers the same maps (the client re-exports it).
- The dedicated server lists the loadable maps but the Tutorial for Change
  Map (`MapContent::maps`) and loads a new empty world painted with the
  server's starting colors (`MapContent::loader`). `server.json` settings and
  passwords carry over (`HostSetup`).
- Guard: `dedicated::tests::a_dedicated_server_changes_maps` (fails on main:
  no maps listed, "This host cannot change maps").
