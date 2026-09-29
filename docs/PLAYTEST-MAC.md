# Blockland ReImagined on a Mac

This is the same game as the Windows release, built for Apple Silicon Macs
(M1 and newer). `PLAYTEST.md` covers what to try; this page covers only what
is different on a Mac.

## Start

1. Unzip the download and drag `BlocklandReImagined.app` to Applications (or
   anywhere you like).
2. Open it. The app is signed by its builder, not by an Apple developer
   account, so the first time macOS says it can't verify the app. Click
   **Done**, open System Settings → Privacy & Security, scroll to
   "BlocklandReImagined was blocked" and click **Open Anyway**. On older macOS
   versions, right-click the app, choose **Open** and confirm instead. macOS
   remembers the choice.
3. The first launch of each build copies the game content (about 600 MB) to
   `~/Library/Application Support/BlocklandReImagined/content/<build>` and plays
   from there, because an app must not change itself. It takes a few seconds.
4. When you host or join, macOS may ask to let the game find devices on your
   local network. Allow it, or LAN games won't show up.

## Where things are

Settings, identity, saves, screenshots and logs live in
`~/Library/Application Support/BlocklandReImagined/` (logs under `logs/`).
In Finder, press Shift-Command-G and paste that path. Keep the latest log
when reporting a problem, and don't share `client.identity` publicly.

Each build keeps its own copy of the content there. Delete the folders of
builds you no longer play to free the space; the game copies its own again if
needed.

## Controls

Most Mac keyboards have no numpad, so choose the laptop scheme on first run.
Jetting holds the right mouse button, so a mouse is easier than a trackpad.
