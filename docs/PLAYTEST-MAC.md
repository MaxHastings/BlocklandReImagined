# Blockland ReImagined on a Mac

This is the same game as the Windows release, built for Apple Silicon Macs
(M1 and newer). Intel Macs can't run it: macOS says the app isn't supported
on this Mac. `PLAYTEST.md` covers what to try; this page covers only what
is different on a Mac.

## Start

1. Unzip the download and drag `BlocklandReImagined.app` to Applications (or
   anywhere you like).
2. Open it. The app is signed by its builder, not by an Apple developer
   account, so the first time macOS says it can't verify the app. Click
   **Done**, open System Settings → Privacy & Security, scroll to
   "BlocklandReImagined was blocked" and click **Open Anyway**. On older macOS
   versions, right-click the app, choose **Open** and confirm instead. macOS
   remembers the choice. If the game then can't start, it shows an alert
   saying why, with a button that opens the logs folder.
3. The first launch of each build copies the bundled game content to
   `~/Library/Application Support/BlocklandReImagined/content/<build>` and plays
   from there, because an app must not change itself. It takes a few seconds.
4. When you host or join, macOS may ask to let the game find devices on your
   local network. Allow it, or LAN games won't show up.

## Where things are

Settings, identity, saves, screenshots and logs live in
`~/Library/Application Support/BlocklandReImagined/` (logs under `logs/`).
In Finder, press Shift-Command-G and paste that path. Keep the latest log
when reporting a problem, and don't share `client.identity` publicly.

Each build keeps its own copy of the content there. Before updating, close
the game and back up the state folder and your custom content. After opening
the new app once, copy only your own Add-On ZIPs/folders from the old build's
`content/<build>/Add-Ons` and `content/<build>/addons` into the corresponding
new build folders. Keep the new build's bundled Add-Ons and `packages.json`.
Then check and enable your additions in **Start Game → Add-Ons**.

To free space, remove only an old build's directory under `content/`, after
backing up any custom additions. The app can recopy its bundled content, but
it cannot recover custom Add-Ons you delete. Keep the parent state folder:
that is also where your settings, identity and saves live. Alpha save/Add-On
formats can change; see [updating and backups](TESTER-GUIDE.md#updating-and-backups).

## Controls

Most Mac keyboards have no numpad, so choose the laptop scheme on first run.
Jetting holds the right mouse button, so a mouse is easier than a trackpad.
