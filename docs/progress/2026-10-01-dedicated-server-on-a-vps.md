# 2026-10-01 Dedicated server on a VPS

Max asked whether a friend can run a v20-style dedicated server on a VPS.
`bri-server` already hosted headlessly, but players could not use it: the
release zips did not ship it, it needed a world file to start, and with no
host player and no passwords nobody could ever become admin.

- `bri-server <content> <map | world.json | resume> <state> <address>`: a
  map name (`slate`, or a full map id) starts an empty world with the
  default palette (`bri_net::dedicated::blank_world`).
- `<state>/server.json` (`dedicated::ServerConfig`), written with defaults on
  first start: admin and Super Admin passwords plus Start Game's Advanced
  Config (`bri_admin::ServerSettings`). `Dedicated::configure` applies it to
  the session and the host setup before anyone joins. The server's player
  cap is now `settings.max_players` (was a fixed 64), and it answers
  Connect to IP / LAN probes with its name and map.
- The Windows and Linux packagers ship `bri-server` and the new
  `docs/DEDICATED-SERVER.md`; `release.yml` and `linux-release-asset.yml`
  build it. The Mac app bundle does not (a Mac VPS is not a target).
- Tests: `dedicated::tests::a_map_name_starts_an_empty_world_with_the_default_palette`,
  `dedicated::tests::server_json_is_written_with_defaults_then_read_back`
  (`cargo test -p bri-net --lib dedicated::`).

Next: Change Map on a dedicated server (`load_map` is None), autosave.
