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

## End-to-end check in place of a playtest

Maxwell decided on 2026-10-11 not to playtest before v0.2.9, so the join
was checked end to end headlessly instead. `add_on_join.rs`
`a_guest_without_a_vehicle_add_on_downloads_it_and_can_spawn_it` runs two
real `App`s: a host on a loopback port running the test plane Add-On, and
a guest with it off. The guest joins over QUIC, downloads and installs the
plane, loads it (the Add-On reload path), joins again and spawns it. The
test now records every screen the guest shows and fails if the dialog shows
anything but "Checking your game content…" or "Connecting to …", if the
join goes back to the dialog after the loading screen, or if downloading,
verifying and loading the Add-Ons are missing from the session log for the
host's map. Three runs in a row (synthetic content) each showed:

```
dialog: Connecting to 127.0.0.1:49459…
dialog: Checking your game content…
dialog: Connecting to 127.0.0.1:49459…
loading Fixture Room 4: CONNECTING
loading Fixture Room 4: DOWNLOADING ADD-ONS  0.0 MB OF 0.0 MB
loading Fixture Room 4: LOADING ADD-ONS
loading Fixture Room 4: LOADING
loading Fixture Room 4: CHECKING CONTENT
loading Fixture Room 4: CONNECTING
loading v20/add-ons/map_slate/slate.mis: RECEIVING WORLD
loading v20/add-ons/map_slate/slate.mis: LOADING MAP
in game
```

The check found two more problems, both fixed:

- Over loopback the install finished between two frames, so it never
  showed and the session log never named it. The log now also names any
  stage that began and ended between frames (`Progress::ended_since`),
  marked "(between frames)".
- Many Add-Ons are a few kilobytes and read "0.0 MB OF 0.0 MB". A download
  under one megabyte now reads in kilobytes ("12 KB OF 49 KB").

The map shows as the host's listed name ("Fixture Room 4") until the
Welcome names it by id; the loading screen looks both up to the same name
and picture. No game window was opened. The bar text was not rendered
with the real v20 pack: the longest new text ("DOWNLOADING ADD-ONS
12.4 MB OF 30.0 MB", 39 characters) is shorter than the existing
"RECEIVING WORLD  187,123 OF 187,123 BRICKS".

## Gate failure on the PC: no VERIFYING ADD-ONS with real content

The integration gate (2edf850) failed the content variant of
`a_guest_without_a_vehicle_add_on_downloads_it_and_can_spawn_it`: "no
Loading Slate: VERIFYING ADD-ONS in the session log". The synthetic variant
passed. The download cache lives in the content folder (`.downloads`), so
on the PC an earlier run had left the test plane installed. That join has
nothing to install, so the verify stage never begins (`fetch_missing`
starts it only for packages it must install). That is the right behaviour,
and the test's premise was wrong.

- The test notes which packages are installed in the cache before the
  join. It requires VERIFYING ADD-ONS only when the join installed one.
  DOWNLOADING ADD-ONS and LOADING ADD-ONS are always required, and so is a
  log line listing the plane as downloaded or already downloaded.
- That last check found that a join which reloads for the server's Add-Ons
  never listed them: `log_fetched` ran only after a successful join, and
  the rejoin after the reload fetches nothing. The list is now written when
  the Add-Ons are handed over to load, before the reload. Its sizes read
  in KB under 1 MB, like the bar.

Checked in the cloud with a throwaway second join from the same cached
folder, as on the PC: the cache showed no new package, the test took the
cached path and passed, and the log read "Add-On test_plane 1.0.0 was
already downloaded". `add_on_join` 6/6 and `package_sync` 16/16 pass.
Clippy `-D warnings` and fmt are clean for `bri-client`. `bri-client
--lib`: 493 passed, and the same 6 failed with "no wgpu adapter".
