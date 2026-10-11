# 2026-10-11 Honest join progress (v0.2.9 P0-03)

Release blocker 2 for v0.2.9. In a v0.2.8 internet playtest a friend sat
about 20 seconds on "Connecting to 107.x..." and thought the join was
timing out; the console showed the host's Add-Ons downloading. Maxwell
asked for the download on the map loading screen's blue bar, as v20 did.

## Root cause

`App::show_progress` (`crates/client/src/app/session.rs`) returned early
while `Progress::subject()` was empty, and the only place a join set the
subject was the host's `Welcome`, which comes after everything else. So the
whole of `Client::connect_fetching_resuming` (probe, package download,
install, loading the downloaded Add-Ons, the second handshake) ran behind
the Connecting dialog's fixed text, although the net crate already reported
`DOWNLOADING PACKAGES` with byte counts. The same was true of three more
steps:

- the join's local content hash (`environment()`), reported as nothing;
- installing the download (copies and re-hashes every file), which kept the
  full download bar and, being a `waits_on_peer` stage, could trip the
  60-second "server stopped responding" stall on a big install;
- loading the downloaded Add-Ons (`mods::load_fetched`, also still under
  the download stage, and the Add-On reload when they bring content), shown
  as a "Loading Add-Ons…" dialog, then "Connecting to…" again for the
  cached rejoin.

## What changed

- `bri-progress`: new stages `VerifyingPackages` ("VERIFYING ADD-ONS") and
  `LoadingAddOns` ("LOADING ADD-ONS"), both local work, so they never trip
  the peer stall. `DOWNLOADING PACKAGES` now reads `DOWNLOADING ADD-ONS`,
  the players' word. `Stage::ALL` lists every stage, and `Progress::trail()`
  keeps the last reading of each finished stage so tests (and later the
  session log) can see stages too quick to poll.
- `bri-net` (two small reporting changes, no wire change): the join sets the
  progress subject from the host's listing as soon as the probe answers, so
  the map is known before the download; installing reports
  `VerifyingPackages` one step per package.
- Client: until the host answers, the Connecting dialog says what the join
  waits on ("Checking your game content…", then "Connecting to …"). From
  then on, and for any stage after connecting even if a host names no map,
  the loading screen shows the stage and its bar, and never goes back to
  the dialog. The map preview is found by id or by listed name. Loading the
  downloaded Add-Ons, and the rejoin after, stay on the loading screen
  naming the host's map. The session log names each stage of a join
  (`Joining <address>: ...` before the map is known) and lists each of the
  host's Add-Ons as downloaded (with size) or already downloaded.

## Tests

- `bri-progress`: every stage listed once with a distinct label; download
  text in megabytes; trail keeps finished stages.
- `bri-net` `package_sync`: new
  `a_join_names_the_map_before_downloading_and_reports_each_step` (map
  known when the Add-Ons load; stages Connecting, Downloading, Verifying,
  Connecting, WaitingForServer, ReceivingWorld with full byte and package
  counts). The clean-client fetch test now reads its byte count from the
  trail and checks the verify stage.
- `bri-client`: `every_stage_of_a_join_is_on_screen` drives each join stage
  (with no map, a map id, and a listed map name) and fails if any stage is
  hidden behind the Connecting dialog once the host has answered, if the
  screen ever goes back to the dialog, or if a new `Stage` is not placed in
  the join's order. `loading_downloaded_add_ons_stays_on_the_loading_screen`
  and an added check in the failed-reload test cover the reload path.
  With the client fix removed, the screen test fails with
  `DownloadingPackages hid behind Connecting: Connecting to 127.0.0.1:9…`.

## Not changed

The unused `PackageDownload` dialog (`crates/ui/src/screens/addons.rs`) is
left as is. The bar text names the stage and bytes, not each Add-On (one
line of upper-case text, like v20's `LoadingProgressTxt`); each Add-On's
name and size go to the console and session log.

## Playtest

Maxwell joins (or has a friend join) a server running Add-Ons the joiner
does not have, over the internet, once with a clean download cache and once
again (warm cache).
