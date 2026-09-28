# First-impressions audit

Audited on 2026-09-28 against main `1599ef2`. This is a read-only audit: no
gameplay code changed. It asks what a new player would notice as missing, rough
or fragile in their first sessions, after the Add-On work and before the game
goes to people outside the project.

Scope follows Max's 2026-09-28 direction: general quality of life, player
expectations, first impressions, well-roundedness, robustness and stability.
Out of scope: legal and asset replacement (a separate process handles it), any
master server or server list (direct IP connect only), modder docs and samples
(tracked separately), and the in-game Add-Ons screen (PR #4).

Comparison points are Blockland v20 (the fidelity reference) and what players of
Minecraft, Garry's Mod and Roblox Studio-style builders take for granted.

## Labels

- **WORKING**: meets or beats what a new player expects.
- **ROUGH**: works, but a player will notice the rough edge or be confused.
- **MISSING**: a player will look for it and not find it.
- **FRAGILE**: works on the happy path, fails badly (lost work, cannot start,
  cannot join) off it.

Impact is the effect on a new player: **high** (blocks or loses work in the
first sessions), **med** (noticed, annoying, erodes trust), **low** (polish).
Every claim cites the file:line or test that backs it. Line numbers are from
`1599ef2`.

## Top 20, ranked

| # | Item | Label | Impact |
|---|---|---|---|
| 1 | Double-clicking the game opens nothing, and startup errors are invisible | FRAGILE | high |
| 2 | A crash or fatal error never tells the player what happened or where the logs are | MISSING | high |
| 3 | Unsaved builds are lost: no autosave, no prompt on leave or close, host world discarded | MISSING | high |
| 4 | A damaged or older `settings.json` stops the game from starting, with a misleading hint | FRAGILE | high |
| 5 | Joining by IP depends on a second port and fails with vague or raw messages | FRAGILE | high |
| 6 | Rejoining the same session loses edit rights on your own bricks; no reconnect | FRAGILE | high |
| 7 | One unreadable save file empties both the Save and Load lists | FRAGILE | high |
| 8 | Building gives no warning before a bad plant, and the undo render path is a known failing test | ROUGH / FRAGILE | high |
| 9 | Disconnect, kick, ban and mismatch reasons reach the player as raw internal text | ROUGH | med |
| 10 | Loading feedback: no splash at startup, host loading bar stuck at 0, no loading screen on join | ROUGH | med |
| 11 | The Tutorial works but is untested end to end, and the docs steer players away from it | FRAGILE | med |
| 12 | No frame cap, graphics presets or render distance | MISSING | med |
| 13 | UI scale is automatic only; no text size, colorblind or subtitle options | ROUGH / MISSING | med |
| 14 | Anti-grief basics: join passwords and brick limits exist in the UI but are not applied | MISSING | med |
| 15 | No name prompt; everyone starts as "Blockhead" and duplicate names are allowed | ROUGH | med |
| 16 | Dedicated server saves only on Ctrl+C and never resumes on its own | FRAGILE | med |
| 17 | No installer, version string, update path or shipped debug symbols | MISSING | med |
| 18 | Brick selector has no search; no duplicator or multi-select | MISSING | med |
| 19 | Input options: no crouch toggle, no gamepad, no side mouse buttons, one key per action | MISSING | med |
| 20 | Audio: no music slider, and no live preview for volume, FOV or sensitivity | ROUGH | low |

Details for each follow, then a list of what already works well, then doc drift.

## 1. Double-clicking the game opens nothing; startup errors are invisible

FRAGILE, high.

- `bri-client.exe` with no arguments prints usage text and exits, so
  double-clicking it flashes a console and no game appears
  (`crates/client/src/main.rs:39-45`). The only working entry point is
  `Launch.cmd` (`tools/package_playtest.ps1:159`).
- There is no `windows_subsystem = "windows"` attribute anywhere under
  `crates/`, and the launcher runs PowerShell with `-NoNewWindow -Wait`
  (`tools/Launch-Playtest.ps1:18`), so a console window stays behind the game
  for the whole session.
- Startup failures go to `logs/client-*.stderr.log`
  (`tools/Launch-Playtest.ps1:18-21`). `Launch-Playtest.cmd` has no `pause`
  (`tools/Launch-Playtest.cmd:3-4`), so the one "Client exited with code N"
  line vanishes with the window.
- Every startup error is wrapped in `REGENERATE_HINT`
  (`crates/client/src/main.rs:74-75`), which tells the player to run
  `python tools/bootstrap.py --v20 ...` (`crates/client/src/content.rs:97`).
  That is a developer instruction and contradicts `docs/PLAYTEST.md`, which
  says no v20 install is needed.
- Any one missing content pack refuses startup, including optional ones such
  as `tutorial-pack-001` (`crates/client/src/content.rs:91`, `:331-341`,
  `:365`). The message lists every missing pack, which is good for
  developers.

Expected: v20, Minecraft and Roblox all open the game from the exe or a
shortcut, with no console, and show a dialog when startup fails.

## 2. Crashes and fatal errors are silent

MISSING, high. The capture itself is good (see "Already working well").

- There is no MessageBox or error dialog anywhere in `crates/crash/src` or
  `crates/client/src/main.rs`. `platform.rs` `fail()` just exits the event loop
  (`crates/client/src/platform.rs:419-422`).
- GPU device loss three times in 60 s ends the game
  (`crates/client/src/platform.rs:680-704`, `:718-719`), also silently.
- The launcher's exit line names `client-*.stderr.log`, not the
  `crash-*.txt` report the crash crate writes (`tools/Launch-Playtest.ps1:20`
  versus `crates/crash/src/lib.rs:47-104`). Only `docs/PLAYTEST.md:71-73`
  explains where to look.
- The launcher's own two logs per run are never rotated
  (`tools/Launch-Playtest.ps1:8-10`); the crash crate keeps 20 session logs and
  10 reports (`crates/crash/src/lib.rs:25-26`).
- Minidumps cannot be read for shipped builds: `package_playtest.ps1` copies
  only `bri-client.exe` (line 178) and neither ships nor archives the matching
  `.pdb`, which `docs/audits/engine-foundations.md` says is needed.
- The playtester-reported crash while jetting, crouching or jumping is still
  unconfirmed as fixed (`docs/progress.md:1592-1595`,
  `docs/ALPHA-HANDOFF.md:102-110`); the shipped ZIP predates crash capture
  (`dd741e5`).

Expected: "Blockland ReImagined stopped unexpectedly. A report was saved to X"
with an Open Folder button, as Minecraft and Roblox show.

## 3. Unsaved work is lost

MISSING, high.

- No autosave in client or server: a search for autosave or auto_save across
  `crates/` finds nothing.
- "Leave this game?" confirms but never mentions unsaved bricks or offers to
  save (`crates/ui/src/screens/menus.rs:613-614`, `:682-683`).
- Closing the window (X or Alt+F4) exits immediately:
  `WindowEvent::CloseRequested => event_loop.exit()`
  (`crates/client/src/platform.rs:1014`).
- When a client-hosted game (single player, LAN or Internet) stops, the host's
  final world is dropped: `let _=host.stop().await;` with the comment "A host
  persistence adapter consumes its final world later"
  (`crates/client/src/network.rs:149-153`). No such adapter exists in the
  client; only `bri-server` writes a final world
  (`crates/net/src/bin/bri-server.rs:160`).

v20 also lost unsaved builds, but the Autosaver add-on was near-universal.
Minecraft saves on quit and periodically; Roblox Studio autosaves and asks
"Save changes?" on close.

## 4. Settings file damage stops the game

FRAGILE, high.

- A corrupt, unparseable or wrong-schema `settings.json` returns an error
  (`crates/client/src/settings.rs:55-61`) that `App::load` propagates
  (`crates/client/src/app.rs:808`), so the game refuses to start. The error
  then gets the content-regeneration hint from item 1. Keeping the file is
  deliberate (test
  `preferences_roundtrip_replace_and_corruption_is_not_silently_reset`), but
  there is no fallback to defaults.
- The format breaks on any change: the stored wrapper uses
  `deny_unknown_fields` and every `Settings` field except `avatar_colors` lacks
  `#[serde(default)]` (`crates/client/src/settings.rs:36-41`,
  `crates/ui/src/api.rs:929-946`). The first build that adds a setting, or a
  player going back one version, gets a game that will not start.

Expected: rename the bad file to `settings.json.bad`, start with defaults, and
say so once.

## 5. Joining by IP

FRAGILE, high. The address box itself is good: it accepts `IP:port`, a bare IP
(port 28000) or bracketed IPv6, and blocks duplicate submits
(`crates/client/src/app.rs:4865-4876`; tests
`join_address_accepts_public_ips_with_or_without_port`,
`runtime_input::direct_join_accepts_text_and_blocks_duplicate_request`).

- A first join to a host first asks for its certificate over UDP 28050
  (`crates/client/src/app.rs:1676-1693`,
  `crates/net/src/discovery.rs:16`). If that port is blocked or not forwarded,
  the join fails even with 28000 open, with "No Blockland ReImagined host
  answered at that address", which does not separate wrong IP, host down,
  firewall and closed 28050.
- The QUIC connect after that uses `.await??` on a 10 s timeout
  (`crates/net/src/client.rs:163-167`), so timeouts and refusals likely reach
  the "Connection Failed" box as raw Tokio or Quinn strings.
- Host names are rejected; only literal IPs parse
  (`crates/client/src/app.rs:4867-4875`). Players share `play.example.com`
  addresses in Minecraft and GMod.
- The last typed IP is not remembered; `$pref::Join::Address` is only written
  for passworded list entries (`crates/ui/src/screens/menus.rs:448`, `:487`).
  v20 remembered it.
- A trusted host certificate is saved on first join
  (`crates/client/src/app.rs:1636-1653`, `:1740-1754`). If the host reinstalls
  and the certificate changes, joining fails with a TLS error and the only fix
  is deleting `trusted-hosts.json` by hand.
- Firewall guidance lives only in `docs/PLAYTEST.md:47-48`; the game detects
  nothing. `docs/progress.md:1681-1683` notes joining is not yet tested over a
  real remote link.

## 6. Rejoin and reconnect

FRAGILE, high.

- A player who disconnects and rejoins the same host session gets a new owner
  number, because departed entries never expire and the returning check skips
  them: `.filter(|n| !self.peers.contains_key(n) &&
  !self.departed.contains_key(n))` (`crates/sim/src/session.rs:600-603`;
  inserted on disconnect at `:715-723`, removed only on resume at `:829`).
  They lose edit rights on bricks they just built. After a host restart the
  owner does come back through identity (`:593-605`).
- Resume tokens exist in the protocol, but the client always passes `None`
  (`crates/client/src/app.rs:1734-1741`). Resume is only exercised in the test
  `real_quic_clients_build_late_join_and_resume_owned_bricks`. There is no
  automatic reconnect and no Reconnect button after a drop.

v20 kept brick ownership across rejoin by BL_ID. Roblox reconnects
automatically.

## 7. One bad save hides every save

FRAGILE, high.

- The save list reads and parses every file, and a single failure aborts the
  whole list: `Self::read(&record).with_context(...)?` inside the loop
  (`crates/client/src/saves.rs:187-188`). The Save and Load dialogs then show
  only the error.
- With the list hidden, the "File Exists, Overwrite?" prompt cannot appear
  (`crates/ui/src/screens/saveload.rs:326-330` checks listed files only), so a
  save to an existing name is refused (`crates/client/src/saves.rs:271`).
- After a successful save the list is rebuilt, and a rebuild failure reports
  the successful save as failed (`crates/client/src/saves.rs:359-368`).
- Listing parses every save in full, up to 63 MiB each; more than 1000 files or
  512 MiB total fails the whole list (`crates/client/src/saves.rs:167-170`,
  `:207`). It runs off the frame thread but slows as collections grow.
- Overwritten saves are kept as hard links in `.history`
  (`crates/client/src/saves.rs:278-295`), which is good, but history is never
  pruned and has no restore UI.

Expected: skip the bad file, list it as damaged, and keep everything else
usable, as Minecraft does with a broken world.

## 8. Building feedback and undo

ROUGH / FRAGILE, high. Building is the core loop, so this ranks above its label.

- The ghost brick is never checked before planting ("no planting/removal is
  predicted here", `crates/client/src/building.rs:843`). Overlap, floating,
  buried, too far, no permission and limit failures appear only after the
  server rejects the plant, as v20's plant-error icon and sound
  (`crates/sim/src/simulation.rs:15-40`, `crates/client/src/app.rs:2102-2116`).
  Minecraft, Roblox and GMod players expect a red ghost before they click.
- The known failing test `app_flow.rs::native_host_cancel_rehost_chat_compositor_disconnect_and_settings`
  times out waiting for "authoritative plant undo removes render/query brick"
  since `d8b8bb4` (`tools/gate-known-failures.toml:13-16`, owner "Hammer rules
  and Destructo Wand"). If real, an undone brick can stay visible. Undo itself
  (Ctrl+Z, 512 steps covering plants, paint and prints) works in the
  simulation (`crates/sim/src/session/undo.rs:1-18`).
- Placement follows v20: click moves the ghost and Enter or the numpad plants
  (`crates/client/src/building.rs:986-999`, `:1049`). That is faithful, but
  players from Minecraft and Roblox will try click-to-place first. The
  Tutorial is the place to teach it (item 11).
- No redo. v20 had none either; Roblox Studio users expect Ctrl+Y.

## 9. Messages are raw internal text

ROUGH, med.

- Server loss returns to the main menu with "Connection Failed" and the raw
  transport reason, for example "Connection closed: timed out" or "closed by
  peer ... Server shutdown" (`crates/client/src/app.rs:2383-2398`,
  `crates/ui/src/ui.rs:1229`, `:1265-1266`, `crates/net/src/server.rs:806`).
- Kicks and bans arrive as "Administration disconnect"; the ban reason, expiry
  and the kick/ban distinction are dropped (`crates/sim/src/session/admin.rs:345`,
  `crates/net/src/server.rs:753`). v20 showed "You have been banned ...
  reason ... minutes remaining".
- Version and content mismatches say "Incompatible protocol version" and
  "Required content does not match" with no versions, no Add-On names and no
  suggested fix (`crates/net/src/server.rs:426-429`, `:713`). Minecraft says
  "Outdated server! I'm still on 1.x". (Which Add-On differs is the mod
  manager's job; the message should still name it.)
- Several player-visible messages speak to developers: setting a server
  password gives "The server join password is not connected yet. It cannot be
  silently ignored." (`crates/client/src/app.rs:1356-1359`); unimplemented menu
  commands open "Interface under construction ... before the alpha handoff"
  (`crates/ui/src/screens/menus.rs:641-644`); a map without a render bundle
  says "Server map has no supported native render bundle yet"
  (`crates/client/src/app.rs:1758-1762`).
- The admin "damaged certificate" error contains a long run of stray spaces
  (`crates/net/src/server.rs:49`).
- Save load errors surface raw serde text such as "EOF while parsing"
  (`crates/client/src/saves.rs:122-126`).

## 10. Loading feedback

ROUGH, med.

- The window appears only after all content loads (`App::load` runs before
  `platform::run`, `crates/client/src/main.rs:72-80`), so the player watches a
  console with no splash.
- The hosting loading screen shows map name, preview and description, but its
  phase is always "LOADING OBJECTS" and progress always `0.0`; both places that
  set `ConnectionState::Loading` hard-code it
  (`crates/client/src/app.rs:1415-1425`, `:2218-2228`). `bri-progress` stages
  exist (`crates/progress/src/lib.rs:81-90`) but never reach the UI.
- Joining never enters the loading screen; the "Connecting to X..." box stays
  up through handshake, world download and map preparation with no progress or
  preview (`crates/client/src/app.rs:1668-1673`, `:2637-2660`). Cancel works
  (test `runtime_input::pending_direct_join_cancel_emits_transport_cancellation`).
- Loading a save grows the build in batches with a "Loading bricks" message
  (`crates/sim/src/session/build_load.rs:1-16`, `:85-88`), but validation runs
  on the authority loop: Golden Gate measured 247 ms, a stall for every player
  (`docs/progress.md:798-800`).

## 11. Tutorial

FRAGILE, med. The Tutorial exists and is substantial: the main menu button
hosts a single-player Tutorial map with zones, tips, restricted abilities, a
brick hand, target practice, wand, spray and driving lessons
(`crates/ui/src/screens/menus.rs:582-584`, `crates/client/src/app.rs:4161-4180`,
`crates/sim/src/session/tutorial.rs:216-1199`). Tips show the player's own
bindings (`crates/client/src/app.rs:2943`). Sim tests cover pieces
(`tutorial_keeps_the_wand_and_cans_for_their_rooms`,
`tutorial_layout_swaps_keep_their_item_spawns_between_publishes`).

- No client test drives Tutorial button to spawn to first lesson; nothing in
  `crates/*/tests` uses `StartTutorial` or `map_tutorial`.
- `docs/PLAYTEST.md:12` recommends Slate and Bedroom as first maps and never
  mentions the Tutorial. `docs/KNOWN-ISSUES.md` still says "Tutorial triggers
  ... are not implemented".
- Nothing offers the Tutorial on first launch. Roblox and Minecraft make
  onboarding the first thing a new player sees.

## 12. Performance options

MISSING, med.

- No frame cap. With VSync off the loop runs `ControlFlow::Poll` as fast as it
  can (`crates/client/src/platform.rs:1197`, `:1233-1237`); no fps limit exists
  in `crates/client/src/settings.rs`. VSync is on by default (`settings.rs:33`).
  A saved no-VSync setting the GPU cannot do falls back with only a console
  warning (`platform.rs:168-176`).
- No graphics presets and no render distance; draw distance comes only from
  each map's `visibledistance` (`crates/convert/src/environment.rs:225`).
- Bricks are baked chunk meshes with no distance LOD, about 2.4 KB of geometry
  per brick (Golden Gate: 964k triangles, 108 MB for 44k bricks,
  `docs/progress.md:1786`). The world cap is 1,000,000 bricks
  (`crates/world/src/model.rs:6`) and a save is capped at 63 MB of JSON, about
  150k bricks (`crates/world/src/build.rs:9`, `:28`). Large v20-style city
  builds are the likely first performance complaint.
- The 120 Hz server tick uses a Tokio interval with no high-resolution timer on
  Windows (`crates/net/src/server.rs:684-685`; no `timeBeginPeriod`), still
  open as `docs/audits/engine-infrastructure.md` #6.
- No wgpu pipeline cache (`crates/render/src/scene.rs:1328`, `cache: None`);
  pipelines build on the frame thread at startup, on device loss and on MSAA or
  shadow changes (`crates/client/src/app.rs:4277-4333`).
- 16 real audio voices (`crates/audio/src/engine.rs:40`); busy minigames will
  cut sounds.
- No per-build frame-budget check; `perf_probe` exists
  (`crates/client/src/bin/perf_probe.rs`) but nothing gates on it.

## 13. Readability and accessibility

ROUGH / MISSING, med.

- UI scale is the largest whole multiple that keeps 640x480: 2x at 1080p, 3x at
  1440p (very large), 1x at 1600x900 (small). There is no user setting; the
  override is always `None` (`crates/ui/src/ui.rs:79-102`,
  `crates/client/src/app.rs:821`).
- Chat size is the only text-size control; menus, HUD and name tags use v20's
  bitmap fonts at a fixed size (`crates/ui/src/screens/play.rs:39-44`,
  `:113-131`).
- No colorblind, subtitle, caption or high-contrast option: no matches in
  `crates/`. Brick paint and the minigame team colors lean on color alone.
- Crouch, walk, jet, zoom and free look are hold-only
  (`crates/ui/src/ui.rs:554-567`); see item 19.
- Setting Chat Line Time to 0 hides chat and also stops T and Y from opening
  the chat box (`crates/ui/src/ui.rs:680-681`,
  `crates/ui/src/models/chat.rs:69`). Faithful to v20, but a trap.

## 14. Anti-grief basics

MISSING, med. Kick and ban work and bans survive restarts (test
`persistent_bans_bind_keys_survive_restart_and_unban`).

- Join passwords are refused on host and join
  (`crates/client/src/app.rs:1356-1359`, `:1628-1632`). Anyone with the IP can
  join an Internet game.
- Admin server settings (brick limit 256,000, bricks per second 10, chat length
  120, chat filter) are defined and shown, but applying them fails with
  "Administration setting has no installed host adapter"
  (`crates/sim/src/session/admin.rs:498-499`). The only cap is the global
  1,000,000 bricks. v20 enforced brick limits and a plant rate against spam.
- No per-player brick counter on the HUD; counts appear only in the save list
  and the `stats` console command.
- Undo is per player; there is no time-based rollback and no undo of an admin
  clear or a load (`crates/ui/src/screens/admin.rs:666-675`, `:975`).
- Chat is rate limited (4 per ~1 s, 256 bytes,
  `crates/sim/src/session.rs:1300-1308`), which works. The input box allows
  255 characters while the server counts bytes, so a long message with accented
  letters is rejected as "Invalid chat message"
  (`crates/ui/src/view.rs:1981-1988`).

## 15. Player name

ROUGH, med.

- No name prompt on first run; everyone is "Blockhead" until they find the
  Avatar screen (`crates/ui/src/api.rs:333`,
  `crates/client/src/app.rs:1336-1343`).
- The server only rejects empty, control-character or over-48-character names
  (`crates/sim/src/session.rs:590-591`), so a server can hold three
  "Blockhead"s and chat and the player list cannot tell them apart.

## 16. Dedicated server persistence

FRAGILE, med (for players who host with `bri-server`).

- The world is written only on clean shutdown, which listens for
  `tokio::signal::ctrl_c()` only (`crates/net/src/bin/bri-server.rs:152-160`,
  `:155`). Closing the console window, logging off or a power cut loses the
  session. No periodic save.
- Each shutdown writes a new `world-<ms>.json`; on restart the operator must
  pass the newest one by hand (`bri-server.rs:95`, `:51-53`), so restarting
  from an old world is easy.
- `host.json`, `server-cert.der` and `last-run.json` use plain `std::fs::write`
  rather than the atomic writer (`bri-server.rs:140-146`, `:161`).
- No command-line settings for password, max players or name; max players is
  hard-coded to 64 (`bri-server.rs:127-138`).

## 17. Packaging, version and updates

MISSING, med.

- Distribution is a folder the packager builds and someone zips by hand
  (`tools/package_playtest.ps1:174-198`, no `Compress-Archive`).
- No installer, Start menu shortcut, uninstaller or code signing; SmartScreen
  will warn on the unsigned exe.
- No update check or auto-update anywhere. Player data lives in the package's
  `user-state/`, so updating means copying that folder by hand
  (`docs/ALPHA-HANDOFF.md:87-89`). v20 itself auto-updated.
- The main menu reads "ReImagined — development" with no build number
  (`crates/ui/src/screens/menus.rs:55`); the real version is only in the
  `version` console command (`crates/client/src/console.rs:103-110`) and log
  headers. Players need it for bug reports.
- The window title still says "building playtest"
  (`crates/client/src/main.rs:81`).

## 18. Building tools players expect

MISSING, med.

- No search in the brick selector (`crates/ui/src/screens/selector.rs`,
  `crates/ui/src/models/selector.rs`). Categories, the 10-slot cart and
  favorites work. With imported Add-On packs the catalog grows fast.
- No multi-select, duplicator or copy and paste: no matches in
  `crates/sim/src/session/tools.rs`, `crates/client/src/building.rs` or
  `crates/ui/src/binds.rs`. v20 lacked it, but the Duplicator add-on was on
  almost every server, and GMod's Advanced Duplicator and Roblox's multi-select
  are standard.
- Third-person camera distance is fixed at 8 units with no scroll zoom
  (`crates/client/src/app.rs:3587`).

## 19. Input options

MISSING, med.

- No hold or toggle choice for crouch, walk, jet, zoom or free look
  (`crates/ui/src/ui.rs:554-567`, `crates/client/src/controls.rs:87-96`).
  Minecraft and Roblox offer toggle crouch.
- No gamepad support at all: no gilrs, XInput or gamepad code in `crates/`.
- Only left, right, middle and wheel can be bound; Mouse4 and Mouse5 cannot
  (`crates/ui/src/input.rs:343-347`).
- One key per action; accepting a conflict leaves the other command unbound
  with only its empty row to show it (`crates/ui/src/binds.rs:153-181`).
- Defaults are exact v20 (Left Shift crouch, Tab camera, F2 player list;
  `crates/ui/src/ui.rs:554-756`). Keep them, but newcomers from Minecraft and
  GMod will press Ctrl or C to crouch and Tab for the scoreboard. The first-run
  controls dialog is the natural place to say so.

## 20. Audio and live preview

ROUGH, low.

- Sliders are Master, Shell (menus) and Sim (game), plus a "Play Music" on/off.
  There is no music volume slider although the engine has a music bus
  (`crates/ui/src/screens/options.rs:88-100`,
  `crates/audio/src/runtime.rs:299`, `:317`).
- Volume, FOV and sensitivity apply only on Done
  (`crates/ui/src/screens/options.rs:765-772`; test
  `audio_preferences_use_seeded_defaults_and_apply_only_on_done`), so dragging
  a slider gives no preview.
- "Fullscreen" is always borderless at native size; there is no exclusive mode
  and no "keep these settings?" revert (`crates/client/src/platform.rs:269`,
  `:539`).
- Console cvars use different ranges from the Options sliders (sensitivity
  0-10 versus 0.02-2; chat lines 1-64 versus 4-100), and Options Done quietly
  clamps a console value (`crates/ui/src/screens/console.rs:235`, `:246`,
  `crates/ui/src/screens/options.rs:619-636`).

## Already working well

These meet or beat what a new player expects and need no action before release.

- **Crash capture.** Panics write `crash-*.txt` with backtrace and the last 200
  log lines; native crashes write a minidump
  (`crates/crash/src/lib.rs:47-104`, `crates/crash/src/windows.rs:73-106`;
  tests `a_panic_leaves_a_report_with_backtrace_and_the_session_log`,
  `a_native_crash_leaves_a_minidump_and_a_report`).
- **GPU fallback and recovery.** DX12, then Vulkan, then WARP, with reasons
  logged; device loss rebuilds the renderer; lost surfaces recover in place
  (`crates/client/src/platform.rs:96-167`, `:680-750`).
- **No risky panics on input.** Runtime unwraps in client, render, net and sim
  guard state the code has just checked; none runs on raw network input or
  file IO.
- **Save writes are crash-safe.** Temp file, fsync, rename
  (`crates/files/src/lib.rs:66-100`), with overwrite confirmation, Windows-safe
  names and read-only shipped templates (test
  `local_saves_preserve_templates_refuse_clobber_and_archive_overwrites`).
- **Settings writes are atomic** (`crates/client/src/settings.rs:64-73`);
  out-of-range values are clamped on read.
- **First-run controls dialog** picks numpad or laptop defaults, like v20
  (`crates/ui/src/screens/menus.rs:591-605`; test `defaults_follow_hardware_conditions`).
- **Rebinding** detects conflicts, refuses reserved keys, supports Remap All and
  Clear All, and rolls back if Options closes without Done (tests
  `remap_conflict_decline_confirm_clear_and_reserved_inputs`,
  `closing_without_done_discards_draft_audio_prefs_and_remaps`).
- **Camera.** First and third person, camera collision with map and bricks,
  free look, zoom, free camera (test
  `third_person_camera_sweeps_map_and_independent_brick_collision_flags`).
- **Held keys release on focus loss**, so movement does not stick after
  Alt+Tab (`crates/ui/src/ui.rs:481-496`).
- **Start Game** lists 14 maps with preview, description and a 1-32 player
  count, and loading can be cancelled (`crates/ui/src/screens/menus.rs:64-81`;
  test `runtime_input::focus_loss_and_loading_cancel_never_leave_held_controls`).
- **UPnP for Internet hosting** opens both ports and explains failure and
  double NAT in chat (`crates/net/src/server.rs:156-190`); single player binds
  to localhost, so there is no firewall prompt.
- **Connection health.** 2 s keep-alive, 15 s idle timeout, late join streams
  the world in chunks (`crates/net/src/server.rs:319-320`; test
  `real_quic_clients_build_late_join_and_resume_owned_bricks`).
- **Chat** with scrollback, fade, size and rate limiting; **console** with
  history and completion that keeps passwords out of history
  (`crates/ui/src/models/chat.rs:68-123`,
  `crates/ui/src/screens/console.rs:420-423`).
- **Brick streaming.** 32-unit chunks rebuilt off-thread and frustum culled;
  Golden Gate frame p50 went from 20 ms to 4-6 ms
  (`crates/client/src/world_chunks.rs:1-10`, `docs/progress.md:1785-1798`).
- **Audio device loss** (headset unplugged) is handled.
- **Escape menu** matches v20 (Options, Player List, Mini-Games, Admin, Save,
  Load, Disconnect, Quit) with modal confirmations (test
  `runtime_input::confirmation_is_modal_and_escape_declines_without_underlying_action`).

## Doc drift found along the way

- `docs/KNOWN-ISSUES.md` says Tutorial triggers are not implemented; they are
  (item 11). `crates/client/src/content.rs:505` has similar wording.
- `docs/PLAYTEST.md:25` says "Tab shows scores"; its own table at `:62` and the
  binds use Tab for the camera and F2 for the player list.
- `docs/playtest-ui-check.md:45` says LAN discovery is unavailable; Query LAN
  works (`crates/client/src/app.rs:4242-4258`). Its line references
  (`app.rs:2130-2137`, `:847`) have moved.
- `docs/audits/engine-infrastructure.md` "Remaining" items 1-4 were fixed by
  chunked brick meshes.

## Suggested order

Small, contained fixes that remove the worst first impressions:

1. Error and crash dialog with the log path, `windows_subsystem`, open the game
   when the exe is double-clicked, and a player-facing startup message instead
   of `REGENERATE_HINT` (items 1, 2).
2. Settings fall back to defaults and keep the bad file; `#[serde(default)]` on
   every field (item 4).
3. Save list skips and marks damaged files (item 7).
4. Expire or reuse departed owner entries on rejoin (item 6).
5. Plain-language messages for disconnect, kick, ban, mismatch and join
   failure, and remove developer wording from player text (items 5, 9).
6. Version string in the main menu and window title (item 17).
7. Offer the Tutorial on first launch and fix the doc drift (item 11).

Larger pieces of work, in rough priority:

1. Save on leave or close, persist the host's final world, and periodic
   autosave (item 3).
2. Loading progress on host and join, and a splash while content loads
   (item 10).
3. Ghost validation before planting, and the undo render test (item 8).
4. Join passwords and applying the admin brick limits (item 14).
5. Frame cap, graphics presets and render distance (item 12).
6. UI scale setting, toggle crouch and the other accessibility options
   (items 13, 19).
7. Reconnect using the existing resume tokens (item 6).
8. Installer, update path and shipped symbols (item 17).
9. Brick search and a duplicator (item 18).
